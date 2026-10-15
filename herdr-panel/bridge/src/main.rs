//! Keeps the panel in sync with herdr and runs the tab focus requests it sends.
//!
//! Usage: herdr-panel-bridge [--port /dev/ttyACM0] [--socket PATH]

mod herdr;
mod panel;

use std::{path::PathBuf, sync::mpsc, thread, time::Duration};

use chrono::{DateTime, TimeZone, Timelike};
use herdr::Herdr;
use herdr_panel_protocol::{State, ToBoard, ToHost};
use panel::Writer;

/// How often herdr is polled even without events, in case one is missed.
const POLL: Duration = Duration::from_secs(2);

enum Wake {
    /// herdr reported a change, or the poll interval passed.
    Refresh,
    /// The board connected; it needs the state even if nothing changed.
    BoardConnected,
    Board(ToHost),
}

fn args() -> (String, Option<PathBuf>) {
    let mut port = "/dev/ttyACM0".to_string();
    let mut socket = None;
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--port" => port = args.next().unwrap_or_else(|| usage()),
            "--socket" => socket = Some(args.next().unwrap_or_else(|| usage()).into()),
            _ => usage(),
        }
    }
    (port, socket)
}

fn usage() -> ! {
    eprintln!("usage: herdr-panel-bridge [--port /dev/ttyACM0] [--socket PATH]");
    std::process::exit(2)
}

/// The clock message for `now`: minutes since its local midnight, and hours
/// since the Unix epoch.
fn clock<Tz: TimeZone>(now: &DateTime<Tz>) -> ToBoard {
    ToBoard::Clock {
        minute: (now.hour() * 60 + now.minute()) as u16,
        hour: (now.timestamp().max(0) / 3600) as u32,
    }
}

fn main() {
    let (port, socket) = args();
    let herdr = match socket {
        Some(path) => Herdr::with_socket(path),
        None => Herdr::from_env().unwrap_or_else(|e| {
            eprintln!("{e}");
            std::process::exit(1)
        }),
    };
    let (tx, rx) = mpsc::channel();
    let writer = Writer::default();

    // herdr change events.
    let events = tx.clone();
    let events_herdr = Herdr::with_socket(herdr.socket().to_path_buf());
    thread::spawn(move || {
        loop {
            match events_herdr.subscribe() {
                Ok(lines) => {
                    for line in lines {
                        if line.is_err() || events.send(Wake::Refresh).is_err() {
                            break;
                        }
                    }
                }
                Err(e) => eprintln!("{e}"),
            }
            thread::sleep(Duration::from_secs(1));
        }
    });

    // Fallback poll.
    let ticks = tx.clone();
    thread::spawn(move || {
        while ticks.send(Wake::Refresh).is_ok() {
            thread::sleep(POLL);
        }
    });

    // The board.
    let board_writer = writer.clone();
    thread::spawn(move || {
        loop {
            let error = panel::session(&port, &board_writer, |event| {
                let _ = tx.send(match event {
                    panel::Event::Connected => Wake::BoardConnected,
                    panel::Event::Message(msg) => Wake::Board(msg),
                });
            });
            eprintln!("panel: {error}; retrying");
            thread::sleep(Duration::from_secs(1));
        }
    });

    let mut last: Option<State> = None;
    let mut last_clock: Option<ToBoard> = None;
    for wake in rx {
        match wake {
            Wake::BoardConnected | Wake::Board(ToHost::Hello) => {
                last = None;
                last_clock = None;
            }
            Wake::Board(ToHost::Focus { agent }) => match herdr.focus_agent(&agent) {
                Ok(()) => eprintln!("focused {agent}"),
                Err(e) => eprintln!("{e}"),
            },
            Wake::Refresh => {}
        }
        let now = clock(&chrono::Local::now());
        if last_clock.as_ref() != Some(&now) && writer.send(&now).is_ok() {
            last_clock = Some(now);
        }
        match herdr.state() {
            Ok(state) if last.as_ref() != Some(&state) => {
                match writer.send_state(&state) {
                    Ok(()) => {
                        eprintln!("sent {} agents", state.agents.len());
                        last = Some(state);
                    }
                    // Not connected yet; the next wake-up retries.
                    Err(_) => {}
                }
            }
            Ok(_) => {}
            Err(e) => eprintln!("{e}"),
        }
    }
}

#[cfg(test)]
mod tests {
    use chrono::FixedOffset;

    use super::*;

    #[test]
    fn the_clock_is_local_minutes_and_epoch_hours() {
        // 07:30 at UTC-7 is 14:30 UTC.
        let pdt = FixedOffset::west_opt(7 * 3600).unwrap();
        let now = pdt.with_ymd_and_hms(2026, 10, 15, 7, 30, 59).unwrap();
        let ToBoard::Clock { minute, hour } = clock(&now) else {
            panic!("not a clock");
        };
        assert_eq!(minute, 7 * 60 + 30);
        assert_eq!(hour % 24, 14);
        assert_eq!(hour as i64, now.timestamp() / 3600);
    }
}
