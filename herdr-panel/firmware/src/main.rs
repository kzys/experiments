//! herdr panel firmware: shows herdr's workspaces and tabs on the Freenove
//! FNK0104N and sends tab focus requests over USB serial.
#![no_std]
#![no_main]
#![feature(impl_trait_in_assoc_type, type_alias_impl_trait)]

extern crate alloc;

mod link;
mod sched;
mod st77922;
mod st77922_init;
mod touch;
mod ui;

use embassy_executor::Spawner;
use esp_backtrace as _;
use esp_hal::{
    dma::{DmaRxBuf, DmaTxBuf, aligned::DmaAlignedMut},
    dma_buffers,
    gpio::{Level, Output, OutputConfig},
    i2c::master::{Config as I2cConfig, I2c},
    interrupt::Priority,
    spi::{
        Mode,
        master::{Config as SpiConfig, Spi},
    },
    time::Rate,
    timer::timg::TimerGroup,
    usb::usb_serial_jtag::{UsbSerialJtag, UsbSerialJtagRx},
};
use esp_rtos::embassy::InterruptExecutor;
use oxivgl::{
    display::{Buffers, COLOR_BUF_LINES, LvglBuffers},
    flush_pipeline::{DisplayOutput, SemaphoreFlushSync, UiError, flush_frame_buffer, set_flush_sync},
    view::{RenderConfig, Ui},
};
use st77922::{HEIGHT as SCREEN_H, St77922, WIDTH as SCREEN_W};
use static_cell::{StaticCell, make_static};

esp_bootloader_esp_idf::esp_app_desc!();

const LVGL_BUF_BYTES: usize = SCREEN_W as usize * COLOR_BUF_LINES * 2;

struct DisplayDriver {
    display: St77922,
}

// SAFETY: only the flush executor touches the panel after hand-off.
unsafe impl Send for DisplayDriver {}

impl DisplayOutput for DisplayDriver {
    async fn show_raw_data(&mut self, x: u16, y: u16, w: u16, h: u16, data: &[u8]) -> Result<(), UiError> {
        self.display.write_pixels(x, y, w, h, data).await.map_err(|_| UiError::Display)
    }
}

#[embassy_executor::task]
#[esp_hal::ram]
async fn flush_task(driver: DisplayDriver) -> ! {
    flush_frame_buffer(driver).await
}

#[embassy_executor::task]
async fn touch_task(i2c: I2c<'static, esp_hal::Async>, rst: Output<'static>) -> ! {
    touch::run(i2c, rst).await
}

#[embassy_executor::task]
async fn link_task(rx: UsbSerialJtagRx<'static, esp_hal::Blocking>) -> ! {
    link::run(rx).await
}

struct UiHooks(&'static Ui);

impl esp_rtos::embassy::Callbacks for UiHooks {
    fn before_poll(&mut self) {}

    fn on_idle(&mut self) {
        self.0.refresh();
    }
}

#[embassy_executor::task]
async fn render_task(ui: &'static Ui) -> ! {
    ui.run_events(ui::Panel::new(), RenderConfig::default()).await
}

/// Widens invalidated areas to multiples of 4 pixels, which this panel needs
/// for its write window.
extern "C" fn round_area(e: *mut oxivgl_sys::lv_event_t) {
    // SAFETY: LVGL passes the area being invalidated as the event parameter.
    unsafe {
        let area = oxivgl_sys::lv_event_get_param(e) as *mut oxivgl_sys::lv_area_t;
        if let Some(a) = area.as_mut() {
            a.x1 &= !3;
            a.y1 &= !3;
            a.x2 |= 3;
            a.y2 |= 3;
        }
    }
}

extern "C" fn render_thread(_: *mut core::ffi::c_void) {
    static mut BUFS: LvglBuffers<LVGL_BUF_BYTES> = LvglBuffers::new();
    // SAFETY: touched only here, before this thread owns LVGL.
    let bufs = unsafe { &mut *core::ptr::addr_of_mut!(BUFS) };
    static UI: StaticCell<Ui> = StaticCell::new();
    let ui: &'static Ui = UI.init(Ui::init(SCREEN_W.into(), SCREEN_H.into(), Buffers::partial(bufs)));
    // SAFETY: Ui::init created the default display on this (the LVGL) thread.
    unsafe {
        oxivgl_sys::lv_display_add_event_cb(
            oxivgl_sys::lv_display_get_default(),
            Some(round_area),
            oxivgl_sys::lv_event_code_t_LV_EVENT_INVALIDATE_AREA,
            core::ptr::null_mut(),
        );
    }
    let exec = make_static!(esp_rtos::embassy::Executor::new());
    exec.run_with_callbacks(
        |s| {
            s.spawn(render_task(ui).unwrap());
        },
        UiHooks(ui),
    )
}

#[esp_rtos::main]
async fn main(spawner: Spawner) {
    let p = esp_hal::init(esp_hal::Config::default().with_cpu_clock(esp_hal::clock::CpuClock::max()));
    esp_println::logger::init_logger_from_env();
    esp_alloc::heap_allocator!(size: 64 * 1024);

    let timg0 = TimerGroup::new(p.TIMG0);
    esp_rtos::start(timg0.timer0, p.FROM_CPU_INTR0);
    // The second core runs the LVGL render thread (see `sched::RENDER_CORE`).
    let app_core_stack = make_static!(esp_hal::system::Stack::<4096>::new());
    esp_rtos::start_second_core(p.CPU_CTRL, p.FROM_CPU_INTR1, app_core_stack, || {});

    // Display: quad SPI.
    let (rx, rx_desc, tx, tx_desc) = dma_buffers!(64, LVGL_BUF_BYTES);
    const ALIGNED: &str = "dma_buffers! is DMA-aligned";
    let dma_rx = DmaRxBuf::new(DmaAlignedMut::new(rx_desc).expect(ALIGNED), DmaAlignedMut::new(rx).expect(ALIGNED))
        .expect("dma rx");
    let dma_tx = DmaTxBuf::new(DmaAlignedMut::new(tx_desc).expect(ALIGNED), DmaAlignedMut::new(tx).expect(ALIGNED))
        .expect("dma tx");
    let spi = Spi::new(p.SPI2, SpiConfig::default().with_frequency(Rate::from_mhz(80)).with_mode(Mode::_0))
        .expect("spi cfg")
        .with_sck(p.GPIO12)
        .with_sio0(p.GPIO11)
        .with_sio1(p.GPIO13)
        .with_sio2(p.GPIO14)
        .with_sio3(p.GPIO9)
        .with_dma(p.DMA_CH0)
        .with_buffers(dma_rx, dma_tx)
        .into_async();
    let cs = Output::new(p.GPIO10, Level::High, OutputConfig::default());
    let mut backlight = Output::new(p.GPIO41, Level::Low, OutputConfig::default());
    let mut display = St77922::new(spi, cs);
    display.init().await.expect("panel init");
    backlight.set_high();

    // Touch controller.
    let i2c = I2c::new(p.I2C0, I2cConfig::default().with_frequency(Rate::from_khz(400)))
        .expect("i2c cfg")
        .with_sda(p.GPIO38)
        .with_scl(p.GPIO39)
        .into_async();
    let touch_rst = Output::new(p.GPIO48, Level::High, OutputConfig::default());
    spawner.spawn(touch_task(i2c, touch_rst).unwrap());

    // USB serial link to the bridge (receive side).
    let (usb_rx, _usb_tx) = UsbSerialJtag::new(p.USB_DEVICE).split();
    spawner.spawn(link_task(usb_rx).unwrap());

    sched::raise_app_executor();
    set_flush_sync(SemaphoreFlushSync::leak_isr());
    let int_exec = make_static!(InterruptExecutor::new(p.FROM_CPU_INTR2));
    let hi = int_exec.start(Priority::min());
    hi.spawn(flush_task(DisplayDriver { display }).unwrap());

    // SAFETY: render_thread never returns.
    unsafe {
        sched::spawn("ui-render", render_thread, sched::PRIO_RENDER, sched::RENDER_STACK, sched::RENDER_CORE)
    };
    core::future::pending::<()>().await
}
