//! ES8311 audio codec, set up for 16 kHz mono with MCLK = 256 * fs.
//!
//! The register values follow Espressif's ES8311 driver for this one
//! configuration: slave mode, 16-bit I2S, analog microphone, DAC unmuted.

use embassy_time::Timer;
use esp_hal::{
    Async,
    i2c::master::{Error, I2c},
};

/// 7-bit I2C address of the codec.
const ADDR: u8 = 0x18;

/// Microphone gain register value for 30 dB.
const MIC_GAIN_30DB: u8 = 5;
/// DAC volume register value for 0 dB; the volume slider scales samples.
const DAC_VOLUME: u8 = 0xbf;

async fn write(i2c: &mut I2c<'static, Async>, reg: u8, value: u8) -> Result<(), Error> {
    i2c.write_async(ADDR, &[reg, value]).await
}

async fn read(i2c: &mut I2c<'static, Async>, reg: u8) -> Result<u8, Error> {
    let mut v = [0u8];
    i2c.write_read_async(ADDR, &[reg], &mut v).await?;
    Ok(v[0])
}

async fn update(
    i2c: &mut I2c<'static, Async>,
    reg: u8,
    f: impl FnOnce(u8) -> u8,
) -> Result<(), Error> {
    let v = read(i2c, reg).await?;
    write(i2c, reg, f(v)).await
}

/// Resets and configures the codec for microphone input and speaker output.
pub async fn init(i2c: &mut I2c<'static, Async>) -> Result<(), Error> {
    // Reset, then power on.
    write(i2c, 0x00, 0x1F).await?;
    Timer::after_millis(20).await;
    write(i2c, 0x00, 0x00).await?;
    write(i2c, 0x00, 0x80).await?;

    // Clocks: take MCLK from its pin, all clocks on, no inversion.
    write(i2c, 0x01, 0x3F).await?;
    update(i2c, 0x06, |v| v & !(1 << 5)).await?;
    // Dividers for MCLK 4.096 MHz / fs 16 kHz.
    update(i2c, 0x02, |v| v & 0x07).await?; // pre_div 1, pre_multi 1x
    write(i2c, 0x03, 0x10).await?; // single speed, ADC OSR
    write(i2c, 0x04, 0x10).await?; // DAC OSR
    write(i2c, 0x05, 0x00).await?; // ADC and DAC dividers 1
    update(i2c, 0x06, |v| (v & 0xE0) | 3).await?; // BCLK divider 4
    update(i2c, 0x07, |v| v & 0xC0).await?; // LRCK divider high
    write(i2c, 0x08, 0xFF).await?; // LRCK divider low

    // Serial port: slave, 16-bit I2S in and out.
    update(i2c, 0x00, |v| v & 0xBF).await?;
    write(i2c, 0x09, 3 << 2).await?;
    write(i2c, 0x0A, 3 << 2).await?;

    // Power up the analog parts, ADC and DAC paths.
    write(i2c, 0x0D, 0x01).await?;
    write(i2c, 0x0E, 0x02).await?;
    write(i2c, 0x12, 0x00).await?;
    write(i2c, 0x13, 0x10).await?;
    write(i2c, 0x1C, 0x6A).await?;
    write(i2c, 0x37, 0x08).await?;

    // Analog microphone, gain, DAC volume, unmute.
    write(i2c, 0x17, 0xC8).await?;
    write(i2c, 0x14, 0x1A).await?;
    write(i2c, 0x16, MIC_GAIN_30DB).await?;
    write(i2c, 0x32, DAC_VOLUME).await?;
    update(i2c, 0x31, |v| v & !((1 << 6) | (1 << 5))).await?;
    Ok(())
}
