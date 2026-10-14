//! Keeps the panel in sync with herdr and runs the tab focus requests it sends.
//!
//! Usage: herdr-panel-bridge [--port /dev/ttyACM0] [--socket PATH]

mod herdr;
mod panel;

use std::{path::PathBuf, sync::mpsc, thread, time::Duration};

use herdr::Herdr;
use herdr_panel_protocol::{State, ToHost};
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
    for wake in rx {
        match wake {
            Wake::BoardConnected | Wake::Board(ToHost::Hello) => last = None,
            Wake::Board(ToHost::Focus { agent }) => match herdr.focus_agent(&agent) {
                Ok(()) => eprintln!("focused {agent}"),
                Err(e) => eprintln!("{e}"),
            },
            Wake::Refresh => {}
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
