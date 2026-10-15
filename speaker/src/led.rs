//! The on-board WS2812B, driven from the RMT peripheral.

use embassy_sync::{blocking_mutex::raw::CriticalSectionRawMutex, signal::Signal};
use esp_hal::{
    Blocking,
    gpio::Level,
    rmt::{Channel, PulseCode, Tx},
};

/// Latest colour to show, as (red, green, blue). Signalling replaces any
/// colour that has not been sent yet.
pub static COLOR: Signal<CriticalSectionRawMutex, (u8, u8, u8)> = Signal::new();

/// RMT ticks per microsecond; the channel is clocked at 80 MHz.
const TICKS_PER_US: f32 = 80.0;

fn ticks(us: f32) -> u16 {
    (us * TICKS_PER_US) as u16
}

/// Encodes one pixel as WS2812 pulses, green first, plus the end marker.
fn encode(r: u8, g: u8, b: u8) -> [PulseCode; 25] {
    let zero = PulseCode::new(Level::High, ticks(0.4), Level::Low, ticks(0.85));
    let one = PulseCode::new(Level::High, ticks(0.8), Level::Low, ticks(0.45));
    let mut out = [PulseCode::end_marker(); 25];
    let word = u32::from(g) << 16 | u32::from(r) << 8 | u32::from(b);
    for (i, p) in out.iter_mut().take(24).enumerate() {
        *p = if word >> (23 - i) & 1 == 1 { one } else { zero };
    }
    out
}

/// Sends every colour signalled on [`COLOR`] to the LED.
pub async fn run(mut channel: Channel<'static, Blocking, Tx>) -> ! {
    loop {
        let (r, g, b) = COLOR.wait().await;
        let data = encode(r, g, b);
        channel = match channel.transmit(&data) {
            Ok(tx) => match tx.wait() {
                Ok(ch) => ch,
                Err((e, ch)) => {
                    log::warn!("led transmit: {:?}", e);
                    ch
                }
            },
            Err((e, ch)) => {
                log::warn!("led transmit start: {:?}", e);
                ch
            }
        };
    }
}
