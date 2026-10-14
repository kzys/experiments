//! ST77922 320x480 panel on a quad SPI bus.
//!
//! Register writes are 1-bit: command byte `0x02`, then the register in the
//! top byte of a 24-bit address, then data. Pixel writes use command `0x32`
//! with quad data lines.

use esp_hal::{
    Async,
    gpio::Output,
    spi::{
        Error,
        master::{Address, Command, DataMode, SpiDma},
    },
};

use crate::st77922_init::INIT;

const CMD_REG_WRITE: u16 = 0x02;
const CMD_PIXEL_WRITE: u16 = 0x32;
const REG_COLUMN: u32 = 0x2A;
const REG_ROW: u32 = 0x2B;
const REG_MEMORY_CONTINUE: u32 = 0x3C;

pub const WIDTH: u16 = 320;
pub const HEIGHT: u16 = 480;

pub struct St77922 {
    spi: SpiDma<'static, Async>,
    cs: Output<'static>,
}

impl St77922 {
    /// Wraps a quad-capable SPI bus; `cs` is driven by hand so one frame can
    /// span several transfers.
    pub fn new(spi: SpiDma<'static, Async>, cs: Output<'static>) -> Self {
        Self { spi, cs }
    }

    async fn write(&mut self, reg: u32, data_mode: DataMode, cmd: u16, data: &[u8]) -> Result<(), Error> {
        self.cs.set_low();
        let r = self
            .spi
            .half_duplex_write_async(
                data_mode,
                Command::_8Bit(cmd, DataMode::Single),
                Address::_24Bit(reg << 8, DataMode::Single),
                0,
                data,
            )
            .await;
        self.cs.set_high();
        r
    }

    /// Writes `data` to a panel register.
    pub async fn write_reg(&mut self, reg: u8, data: &[u8]) -> Result<(), Error> {
        self.write(reg.into(), DataMode::Single, CMD_REG_WRITE, data).await
    }

    /// Runs the vendor init sequence.
    pub async fn init(&mut self) -> Result<(), Error> {
        for &(reg, data, delay_ms) in INIT {
            self.write_reg(reg, data).await?;
            if delay_ms > 0 {
                embassy_time::Timer::after_millis(delay_ms.into()).await;
            }
        }
        Ok(())
    }

    /// Sends RGB565 pixels for the `w` x `h` window at (`x`, `y`).
    pub async fn write_pixels(&mut self, x: u16, y: u16, w: u16, h: u16, data: &[u8]) -> Result<(), Error> {
        let col = [(x >> 8) as u8, x as u8, ((x + w - 1) >> 8) as u8, (x + w - 1) as u8];
        let row = [(y >> 8) as u8, y as u8, ((y + h - 1) >> 8) as u8, (y + h - 1) as u8];
        self.write(REG_COLUMN, DataMode::Single, CMD_REG_WRITE, &col).await?;
        self.write(REG_ROW, DataMode::Single, CMD_REG_WRITE, &row).await?;
        self.write(REG_MEMORY_CONTINUE, DataMode::Quad, CMD_PIXEL_WRITE, data).await
    }
}
