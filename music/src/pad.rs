// Minimal gamepad reading for list navigation: D-pad/hat move the
// selection, A confirms, B goes back, Select quits. Unlike lowkey's
// keyboard grid, a short list is comfortable to walk one press at a time,
// so there's no held-direction auto-repeat or shoulder-button chords here
// (see PLAN.md).
//
// Button mapping matches lowkey's confirmed layout on this device: A is
// BTN_EAST, B is BTN_SOUTH (this device's driver reports the Nintendo-style
// face buttons that way).

use std::fs::{File, OpenOptions};
use std::os::unix::fs::OpenOptionsExt;

use input_linux::{sys, EvdevHandle, Key};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NavEvent {
    Up,
    Down,
    Select,
    Back,
    Quit,
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
    match ev.code as i32 {
        c if c == sys::BTN_DPAD_UP => Some(NavEvent::Up),
        c if c == sys::BTN_DPAD_DOWN => Some(NavEvent::Down),
        c if c == sys::BTN_EAST => Some(NavEvent::Select),
        c if c == sys::BTN_SOUTH => Some(NavEvent::Back),
        c if c == sys::BTN_SELECT => Some(NavEvent::Quit),
        _ => None,
    }
}

// Finds an evdev node whose name contains `want`, or the first node
// advertising a gamepad button when `want` is None. Copied from lowkey's
// pad.rs (identical logic, no keyboard-specific parts to trim).
pub fn open_pad(want: Option<&str>) -> Option<EvdevHandle<File>> {
    for i in 0..32 {
        let path = format!("/dev/input/event{i}");
        let file = match OpenOptions::new().read(true).custom_flags(libc::O_NONBLOCK).open(&path) {
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
            eprintln!("pad: {name} ({path})");
            return Some(handle);
        }
    }
    None
}

pub fn read_pad(pad: &EvdevHandle<File>) -> Vec<NavEvent> {
    let mut evs: [sys::input_event; 32] = unsafe { std::mem::zeroed() };
    let n = match pad.read(&mut evs) {
        Ok(n) => n,
        Err(_) => return Vec::new(),
    };
    evs[..n].iter().filter_map(decode).collect()
}
