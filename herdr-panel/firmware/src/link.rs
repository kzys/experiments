//! The USB serial link to the bridge.
//!
//! Receiving runs here; sending goes through `esp_println`, which owns the
//! transmit side of the same port (it also carries any log output, which the
//! bridge ignores because it does not start with `{`).

use core::cell::RefCell;

use embassy_sync::blocking_mutex::{Mutex, raw::CriticalSectionRawMutex};
use embassy_time::{Instant, Timer};
use esp_hal::{Blocking, usb::usb_serial_jtag::UsbSerialJtagRx};
use herdr_panel_protocol::{MAX_LINE, State, ToBoard, ToHost};
use herdr_panel_protocol::heapless::Vec;

struct Shared {
    /// Bumped on every state received; 0 until the first one.
    version: u32,
    state: State,
}

static SHARED: Mutex<CriticalSectionRawMutex, RefCell<Shared>> =
    Mutex::new(RefCell::new(Shared { version: 0, state: State { workspaces: Vec::new() } }));

/// The latest state, if it is newer than `seen`.
pub fn newer_than(seen: u32) -> Option<(u32, State)> {
    SHARED.lock(|s| {
        let s = s.borrow();
        (s.version != seen).then(|| (s.version, s.state.clone()))
    })
}

/// Sends one message to the bridge.
pub fn send(msg: &ToHost) {
    let mut buf = [0u8; 64];
    match serde_json_core::to_slice(msg, &mut buf) {
        Ok(n) => match core::str::from_utf8(&buf[..n]) {
            Ok(json) => esp_println::println!("{json}"),
            Err(_) => log::error!("link: message is not UTF-8"),
        },
        Err(_) => log::error!("link: message too long"),
    }
}

/// Handles one complete line from the bridge.
fn handle_line(line: &[u8]) {
    if line.first() != Some(&b'{') {
        return;
    }
    match serde_json_core::from_slice::<ToBoard>(line) {
        Ok((ToBoard::State(state), _)) => SHARED.lock(|s| {
            let mut s = s.borrow_mut();
            s.state = state;
            s.version = s.version.wrapping_add(1).max(1);
        }),
        Err(_) => log::error!("link: bad line ({} bytes)", line.len()),
    }
}

/// Reads lines from the bridge and asks for the state until it arrives.
pub async fn run(mut rx: UsbSerialJtagRx<'static, Blocking>) -> ! {
    let mut line: Vec<u8, MAX_LINE> = Vec::new();
    let mut overflow = false;
    let mut last_hello: Option<Instant> = None;
    loop {
        while let Ok(byte) = rx.read_byte() {
            if byte == b'\n' {
                if !overflow {
                    handle_line(&line);
                }
                line.clear();
                overflow = false;
            } else if line.push(byte).is_err() {
                overflow = true;
            }
        }
        // Until the first state arrives the bridge may not be running yet.
        let waiting = SHARED.lock(|s| s.borrow().version == 0);
        if waiting && last_hello.is_none_or(|t| t.elapsed().as_secs() >= 3) {
            last_hello = Some(Instant::now());
            send(&ToHost::Hello);
        }
        Timer::after_millis(5).await;
    }
}
