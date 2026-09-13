// Minimal gamepad reading for list navigation: D-pad/hat move the
// selection, A confirms, B goes back, Select quits; X toggles play/pause
// and L1/R1 skip between tracks; Select+Start together toggles hold mode
// (see Chord). Up/Down auto-repeat
// while held (see Repeater) — a large music library scrolls one screenful
// per held direction rather than one row per press; the other buttons stay
// one-shot, same as lowkey's own grid does for taps versus moves.
//
// Button mapping matches lowkey's confirmed layout on this device: A is
// BTN_EAST, B is BTN_SOUTH (this device's driver reports the Nintendo-style
// face buttons that way), so X is assumed to be BTN_NORTH by the same
// layout — not yet confirmed on the hardware. L1/R1 are the usual
// BTN_TL/BTN_TR.

use std::fs::{File, OpenOptions};
use std::os::fd::AsRawFd;
use std::time::{Duration, Instant};

use input_linux::{sys, EvdevHandle, Key};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NavEvent {
    Up,
    Down,
    Select,
    Back,
    Quit,
    PlayPause,
    Prev,
    Next,
    // Toggles hold mode: buttons ignored and the screen dark, for carrying
    // the device in a bag while it keeps playing. Select+Start together.
    Hold,
    // No navigation effect; just asks for a redraw. Used to get a just-logged
    // message (e.g. the reader thread dying) on screen instead of the app
    // going quiet with nothing to show for it.
    Refresh,
}

pub fn decode(ev: &sys::input_event) -> Option<NavEvent> {
    // This device's D-pad shows up as both a hat axis and discrete buttons;
    // lowkey's pad.rs found both present on the real hardware, so react to
    // either rather than guessing which one this hat-less/button-less unit
    // actually reports.
    if ev.type_ as i32 == sys::EV_ABS && ev.code as i32 == sys::ABS_HAT0Y {
        return match ev.value {
            v if v < 0 => Some(NavEvent::Up),
            v if v > 0 => Some(NavEvent::Down),
            _ => None, // back to center; no repeat to cancel
        };
    }

    if ev.type_ as i32 != sys::EV_KEY || ev.value != 1 {
        return None; // only react to presses
    }
    // Select isn't here: whether it means Quit or is half of the hold chord
    // only becomes known on release (see Chord).
    match ev.code as i32 {
        c if c == sys::BTN_DPAD_UP => Some(NavEvent::Up),
        c if c == sys::BTN_DPAD_DOWN => Some(NavEvent::Down),
        c if c == sys::BTN_EAST => Some(NavEvent::Select),
        c if c == sys::BTN_SOUTH => Some(NavEvent::Back),
        c if c == sys::BTN_NORTH => Some(NavEvent::PlayPause),
        c if c == sys::BTN_TL => Some(NavEvent::Prev),
        c if c == sys::BTN_TR => Some(NavEvent::Next),
        _ => None,
    }
}

// Finds an evdev node whose name contains `want`, or the first node
// advertising a gamepad button when `want` is None. Copied from lowkey's
// pad.rs (identical logic, no keyboard-specific parts to trim).
//
// Opened blocking: this runs on its own reader thread (see ui.rs), which
// just parks on read() rather than sharing a poll loop with anything else.
pub fn open_pad(want: Option<&str>) -> Option<EvdevHandle<File>> {
    for i in 0..32 {
        let path = format!("/dev/input/event{i}");
        let file = match OpenOptions::new().read(true).open(&path) {
            Ok(f) => f,
            Err(_) => continue,
        };
        let handle = EvdevHandle::new(file);

        let Ok(raw_name) = handle.device_name() else {
            continue;
        };
        let name_bytes = raw_name.split(|&b| b == 0).next().unwrap_or(&[]);
        let name = String::from_utf8_lossy(name_bytes);

        let found = match want {
            Some(w) => name.contains(w),
            None => handle.key_bits().map(|b| b.get(Key::ButtonSouth)).unwrap_or(false),
        };
        if found {
            log::info!("pad: {name} ({path})");
            return Some(handle);
        }
    }
    None
}

// None means the pad is actually gone (a read error). An empty Vec is the
// common case, not a sign of that: most raw reads decode to nothing at all
// (EV_SYN, key releases, a hat axis re-centering all hit decode's `None`
// arms) even though the device is perfectly fine.
pub fn read_pad(
    pad: &EvdevHandle<File>,
    repeat: &mut Repeater,
    chord: &mut Chord,
) -> Option<Vec<NavEvent>> {
    let mut evs: [sys::input_event; 32] = unsafe { std::mem::zeroed() };
    let n = pad.read(&mut evs).ok()?;
    let now = Instant::now();
    for ev in &evs[..n] {
        repeat.observe(ev, now);
    }
    Some(evs[..n].iter().filter_map(|ev| chord.observe(ev).or_else(|| decode(ev))).collect())
}

// Select alone quits; Select and Start down together toggle hold mode. The
// two can't be told apart on Select's press, so Quit fires on Select's
// release instead, and only if Start never joined it in between — a chord
// fires once, when the second button lands, whichever order they come in.
pub struct Chord {
    select_down: bool,
    start_down: bool,
    fired: bool,
}

impl Chord {
    pub fn new() -> Self {
        Chord { select_down: false, start_down: false, fired: false }
    }

    pub fn observe(&mut self, ev: &sys::input_event) -> Option<NavEvent> {
        if ev.type_ as i32 != sys::EV_KEY {
            return None;
        }
        let down = match ev.value {
            1 => true,
            0 => false,
            _ => return None, // driver-level autorepeat report
        };
        match ev.code as i32 {
            c if c == sys::BTN_SELECT => {
                let was_down = self.select_down;
                self.select_down = down;
                if !down && was_down && !self.fired {
                    return Some(NavEvent::Quit);
                }
                if !down {
                    self.fired = false;
                }
            }
            c if c == sys::BTN_START => self.start_down = down,
            _ => return None,
        }
        if down && self.select_down && self.start_down && !self.fired {
            self.fired = true;
            return Some(NavEvent::Hold);
        }
        None
    }
}

// Blocks up to `timeout_ms` (-1: forever, 0: don't block) for the pad to
// become readable, so the reader thread can wake on its own to fire a
// repeat rather than sitting in read() until the next real event arrives.
pub fn wait_readable(pad: &EvdevHandle<File>, timeout_ms: i32) -> bool {
    let mut fds = [libc::pollfd { fd: pad.as_raw_fd(), events: libc::POLLIN, revents: 0 }];
    let n = unsafe { libc::poll(fds.as_mut_ptr(), 1, timeout_ms) };
    n > 0
}

const REPEAT_DELAY: Duration = Duration::from_millis(400);
const REPEAT_INTERVAL: Duration = Duration::from_millis(120);

// Tracks which direction (if any) is currently held, so the reader thread
// can re-send it on a timer instead of waiting for more raw events. Only
// Up/Down repeat — matches lowkey's own grid, where taps (Select, Back,
// Quit) don't.
pub struct Repeater {
    held: Option<NavEvent>,
    next_at: Instant,
}

impl Repeater {
    pub fn new() -> Self {
        Repeater { held: None, next_at: Instant::now() }
    }

    // Same hat-or-buttons detection as decode(), but also needs releases
    // (decode() discards them — a release has no navigation effect of its
    // own, only this hold-tracking one) to know when to stop repeating.
    pub fn observe(&mut self, ev: &sys::input_event, now: Instant) {
        let change = if ev.type_ as i32 == sys::EV_ABS && ev.code as i32 == sys::ABS_HAT0Y {
            match ev.value {
                v if v < 0 => Some(Some(NavEvent::Up)),
                v if v > 0 => Some(Some(NavEvent::Down)),
                _ => Some(None), // back to center
            }
        } else if ev.type_ as i32 == sys::EV_KEY
            && (ev.code as i32 == sys::BTN_DPAD_UP || ev.code as i32 == sys::BTN_DPAD_DOWN)
        {
            let dir =
                if ev.code as i32 == sys::BTN_DPAD_UP { NavEvent::Up } else { NavEvent::Down };
            match ev.value {
                1 => Some(Some(dir)),
                0 => Some(None),
                _ => None, // driver-level autorepeat report; already held
            }
        } else {
            None
        };

        match change {
            Some(Some(dir)) => {
                self.held = Some(dir);
                self.next_at = now + REPEAT_DELAY;
            }
            Some(None) => self.held = None,
            None => {}
        }
    }

    // Timeout for wait_readable: how long until the next repeat is due, or
    // -1 (block indefinitely) when nothing is held.
    pub fn poll_timeout_ms(&self, now: Instant) -> i32 {
        match self.held {
            None => -1,
            Some(_) if self.next_at <= now => 0,
            Some(_) => (self.next_at - now).as_millis().min(i32::MAX as u128) as i32,
        }
    }

    // Fires a repeat if one is due.
    pub fn due(&mut self, now: Instant) -> Option<NavEvent> {
        let dir = self.held?;
        if now < self.next_at {
            return None;
        }
        self.next_at = now + REPEAT_INTERVAL;
        Some(dir)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn repeat_fires_after_delay_then_at_interval() {
        let mut r = Repeater::new();
        let t0 = Instant::now();
        let press = sys::input_event {
            time: unsafe { std::mem::zeroed() },
            type_: sys::EV_KEY as u16,
            code: sys::BTN_DPAD_DOWN as u16,
            value: 1,
        };
        r.observe(&press, t0);

        assert_eq!(r.poll_timeout_ms(t0), 400);
        assert_eq!(r.due(t0 + Duration::from_millis(399)), None);

        let t1 = t0 + Duration::from_millis(400);
        assert_eq!(r.due(t1), Some(NavEvent::Down));
        assert_eq!(r.due(t1 + Duration::from_millis(119)), None);
        assert_eq!(r.due(t1 + Duration::from_millis(120)), Some(NavEvent::Down));
    }

    #[test]
    fn release_cancels_pending_repeat() {
        let mut r = Repeater::new();
        let t0 = Instant::now();
        let press = sys::input_event {
            time: unsafe { std::mem::zeroed() },
            type_: sys::EV_KEY as u16,
            code: sys::BTN_DPAD_UP as u16,
            value: 1,
        };
        let release = sys::input_event { value: 0, ..press };
        r.observe(&press, t0);
        r.observe(&release, t0);

        assert_eq!(r.poll_timeout_ms(t0), -1);
        assert_eq!(r.due(t0 + Duration::from_secs(10)), None);
    }

    fn key(code: i32, value: i32) -> sys::input_event {
        sys::input_event {
            time: unsafe { std::mem::zeroed() },
            type_: sys::EV_KEY as u16,
            code: code as u16,
            value,
        }
    }

    #[test]
    fn select_alone_quits_on_release() {
        let mut c = Chord::new();
        assert_eq!(c.observe(&key(sys::BTN_SELECT, 1)), None);
        assert_eq!(c.observe(&key(sys::BTN_SELECT, 0)), Some(NavEvent::Quit));
    }

    #[test]
    fn select_then_start_holds_without_quitting() {
        let mut c = Chord::new();
        assert_eq!(c.observe(&key(sys::BTN_SELECT, 1)), None);
        assert_eq!(c.observe(&key(sys::BTN_START, 1)), Some(NavEvent::Hold));
        assert_eq!(c.observe(&key(sys::BTN_START, 0)), None);
        assert_eq!(c.observe(&key(sys::BTN_SELECT, 0)), None);
    }

    #[test]
    fn start_then_select_holds_too() {
        let mut c = Chord::new();
        assert_eq!(c.observe(&key(sys::BTN_START, 1)), None);
        assert_eq!(c.observe(&key(sys::BTN_SELECT, 1)), Some(NavEvent::Hold));
        assert_eq!(c.observe(&key(sys::BTN_SELECT, 0)), None);
        assert_eq!(c.observe(&key(sys::BTN_START, 0)), None);
    }

    #[test]
    fn start_alone_does_nothing() {
        let mut c = Chord::new();
        assert_eq!(c.observe(&key(sys::BTN_START, 1)), None);
        assert_eq!(c.observe(&key(sys::BTN_START, 0)), None);
    }
}
