//! The USB serial link to the bridge.
//!
//! Receiving runs here; sending goes through `esp_println`, which owns the
//! transmit side of the same port (it also carries any log output, which the
//! bridge ignores because it does not start with `{`).

use core::cell::RefCell;

use embassy_sync::blocking_mutex::{Mutex, raw::CriticalSectionRawMutex};
use embassy_time::{Instant, Timer};
use esp_hal::{Blocking, usb::usb_serial_jtag::UsbSerialJtagRx};
use herdr_panel_protocol::heapless::Vec;
use herdr_panel_protocol::{Assembler, MAX_LINE, State, ToBoard, ToHost};

struct Shared {
    /// Bumped on every state received; 0 until the first one.
    version: u32,
    state: State,
}

static SHARED: Mutex<CriticalSectionRawMutex, RefCell<Shared>> = Mutex::new(RefCell::new(Shared {
    version: 0,
    state: State { agents: Vec::new() },
}));

/// Copies the latest state into `out` if it is newer than `seen`, and returns
/// its version. Copies agent by agent: a whole state is too big for the stack.
pub fn newer_into(seen: u32, out: &mut State) -> Option<u32> {
    SHARED.lock(|s| {
        let s = s.borrow();
        if s.version == seen {
            return None;
        }
        out.agents.clear();
        for agent in &s.state.agents {
            // Both have the same capacity.
            let _ = out.agents.push(agent.clone());
        }
        Some(s.version)
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

/// Handles one complete line from the bridge, publishing the state once the
/// assembler has all of it. The finished state is swapped in, not copied:
/// it is too big to move around on the stack.
fn handle_line(line: &[u8], assembler: &mut Assembler) {
    if line.first() != Some(&b'{') && line.first() != Some(&b'"') {
        return;
    }
    match serde_json_core::from_slice::<ToBoard>(line) {
        Ok((msg, _)) => {
            if let Some(state) = assembler.push(msg) {
                SHARED.lock(|s| {
                    let mut s = s.borrow_mut();
                    core::mem::swap(&mut s.state, state);
                    s.version = s.version.wrapping_add(1).max(1);
                });
            }
        }
        Err(_) => log::error!("link: bad line ({} bytes)", line.len()),
    }
}

/// Reads lines from the bridge and asks for the state until it arrives.
pub async fn run(mut rx: UsbSerialJtagRx<'static, Blocking>) -> ! {
    let mut line: Vec<u8, MAX_LINE> = Vec::new();
    let mut assembler = Assembler::new();
    let mut overflow = false;
    let mut last_hello: Option<Instant> = None;
    loop {
        while let Ok(byte) = rx.read_byte() {
            if byte == b'\n' {
                if !overflow {
                    handle_line(&line, &mut assembler);
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
