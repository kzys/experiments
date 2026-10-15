//! Touch controller of the ST77922 panel (I2C, 16-bit register addresses).

use embassy_time::Timer;
use esp_hal::{Async, gpio::Output, i2c::master::I2c};
use oxivgl::indev::PointerState;

const ADDR: u8 = 0x55;
const REG_STATUS: u16 = 0x0001;
const REG_MAX_TOUCHES: u16 = 0x0009;
const REG_TOUCH_INFO: u16 = 0x0010;
const REG_TOUCH_POINT0: u16 = 0x0014;
/// `REG_TOUCH_INFO` bit set when fresh point data is available.
const INFO_DATA_READY: u8 = 0x08;
/// Bit set in the first byte of a point while the finger is down.
/// Size of the controller's point table.
const MAX_POINTS: usize = 10;
const POINT_DOWN: u8 = 0x80;

async fn read(
    i2c: &mut I2c<'static, Async>,
    reg: u16,
    buf: &mut [u8],
) -> Result<(), esp_hal::i2c::master::Error> {
    i2c.write_read_async(ADDR, &reg.to_be_bytes(), buf).await
}

/// Touch input for LVGL; register it with `PointerIndev::new`.
pub static POINTER: PointerState = PointerState::new();

/// Decodes the first touch point from a raw 7-byte point record.
fn decode(p: &[u8; 7]) -> Option<(u16, u16)> {
    (p[0] & POINT_DOWN != 0).then(|| {
        let x = u16::from(p[0] & 0x3F) << 8 | u16::from(p[1]);
        let y = u16::from(p[2] & 0x3F) << 8 | u16::from(p[3]);
        (x, y)
    })
}

/// Resets the controller, then polls it and publishes the first touch point.
pub async fn run(mut i2c: I2c<'static, Async>, mut rst: Output<'static>) -> ! {
    rst.set_low();
    Timer::after_millis(100).await;
    rst.set_high();
    Timer::after_millis(100).await;

    let mut status = [0u8];
    for _ in 0..50 {
        if read(&mut i2c, REG_STATUS, &mut status).await.is_ok() && status[0] & 0x0F == 0 {
            break;
        }
        Timer::after_millis(10).await;
    }
    // The controller only clears its data-ready flag once every point record
    // has been read, so read them all.
    let mut max = [0u8];
    read(&mut i2c, REG_MAX_TOUCHES, &mut max).await.ok();
    let points = usize::from(max[0]).clamp(1, MAX_POINTS);
    log::info!("touch ready, status {:#x}, {} points", status[0], points);

    loop {
        let mut info = [0u8];
        let mut raw = [0u8; 7 * MAX_POINTS];
        let down = match read(&mut i2c, REG_TOUCH_INFO, &mut info).await {
            Ok(()) if info[0] & INFO_DATA_READY != 0 => {
                read(&mut i2c, REG_TOUCH_POINT0, &mut raw[..7 * points])
                    .await
                    .ok()
                    .and_then(|()| decode(raw[..7].try_into().unwrap()))
            }
            Ok(()) => None,
            Err(e) => {
                log::warn!("touch read: {:?}", e);
                None
            }
        };
        match down {
            Some((x, y)) => POINTER.touch(x, y),
            None => POINTER.release(),
        }
        Timer::after_millis(15).await;
    }
}
