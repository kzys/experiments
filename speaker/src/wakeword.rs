//! Wake word detection with ESP-SR's WakeNet, on the models packed into
//! the firmware by `build.rs`. It runs on a thread of its own below the
//! audio task, fed through [`SAMPLES`]: loading a model takes longer than
//! the I2S buffers last, and an esp-hal stream that runs dry stops for good.

use alloc::{string::String, vec, vec::Vec};
use core::{
    ffi::{CStr, c_char, c_void},
    sync::atomic::{AtomicBool, AtomicU8, AtomicU32, Ordering},
};

use embassy_sync::{blocking_mutex::raw::CriticalSectionRawMutex, pipe::Pipe};
use static_cell::make_static;

use crate::{
    audio::{self, Note},
    sched, sr,
};

/// No wake word model.
pub const NO_MODEL: u8 = u8::MAX;

/// The model to listen with: an index into [`MODELS`].
pub static WANTED: AtomicU8 = AtomicU8::new(0);

/// The model last loaded or tried, and whether it is running.
pub static LOADED: AtomicU8 = AtomicU8::new(NO_MODEL);
pub static RUNNING: AtomicBool = AtomicBool::new(false);

/// Wake words heard so far.
pub static COUNT: AtomicU32 = AtomicU32::new(0);

/// Microphone samples (16-bit little-endian, 16 kHz) for the detector;
/// 256 ms of them.
pub static SAMPLES: Pipe<CriticalSectionRawMutex, 8192> = Pipe::new();

/// Stack for the detector thread; WakeNet's own use is small.
const STACK: usize = 16 * 1024;

#[repr(C, align(16))]
struct Aligned<T: ?Sized>(T);

/// The models, in ESP-SR's `srmodels.bin` layout.
static IMAGE: &Aligned<[u8]> = &Aligned(*include_bytes!(concat!(env!("OUT_DIR"), "/srmodels.bin")));

/// A wake word model in the image.
pub struct Model {
    /// Model name, like "wn9_hiesp".
    pub name: String,
    /// The word it listens for, like "Hi,ESP".
    pub word: String,
    name_c: *const c_char,
}

// SAFETY: `name_c` points into the model list, which is never changed or
// freed.
unsafe impl Send for Model {}
unsafe impl Sync for Model {}

/// The wake word models, set once at start-up.
pub static MODELS: embassy_sync::once_lock::OnceLock<Vec<Model>> = embassy_sync::once_lock::OnceLock::new();

/// Lists the wake word models in the image.
pub fn models() -> Vec<Model> {
    // SAFETY: the image is static, as the model list requires.
    let list = unsafe { &*sr::speaker_srmodel_load(IMAGE.0.as_ptr()) };
    let mut out = Vec::new();
    for i in 0..list.num as usize {
        // SAFETY: the list has `num` names and infos; infos may be NULL.
        let (name_c, info) = unsafe { (*list.model_name.add(i), *list.model_info.add(i)) };
        let name = unsafe { CStr::from_ptr(name_c) }.to_string_lossy().into_owned();
        if !name.starts_with("wn") {
            continue;
        }
        let info = if info.is_null() { String::new() } else { unsafe { CStr::from_ptr(info) }.to_string_lossy().into_owned() };
        out.push(Model { name, word: word_from_info(&info).into(), name_c });
    }
    out
}

/// The wake word in a model info string like
/// "wakeNet9_v1h24_Hi,ESP_3_0.63_0.635" (type_version_word_...).
fn word_from_info(info: &str) -> &str {
    info.split('_').nth(2).unwrap_or("?")
}

/// A running WakeNet model, fed 16 kHz mono samples.
pub struct Detector {
    iface: &'static sr::WnIface,
    model: *mut core::ffi::c_void,
    chunk: Vec<i16>,
    filled: usize,
}

// SAFETY: the model is only used through `&mut self`.
unsafe impl Send for Detector {}

impl Detector {
    /// Loads `model`, or returns `None` if WakeNet cannot.
    pub fn new(model: &Model) -> Option<Self> {
        // SAFETY: the name is a C string in the model list.
        let iface = unsafe { sr::esp_wn_handle_from_name(model.name_c).as_ref()? };
        let create = iface.create?;
        // SAFETY: WakeNet takes the model name as its argument.
        let handle = unsafe { create(model.name_c.cast(), sr::DET_MODE_95) };
        if handle.is_null() {
            return None;
        }
        // SAFETY: `handle` is a live model of this interface.
        let samples = iface.get_samp_chunksize.map_or(0, |f| unsafe { f(handle) });
        let mut detector = Self { iface, model: handle, chunk: Vec::new(), filled: 0 };
        // Returning early drops `detector`, which destroys the model.
        let samples = usize::try_from(samples).ok().filter(|&n| n > 0)?;
        detector.chunk = vec![0; samples];
        Some(detector)
    }

    /// Feeds samples; true if the wake word ended in them.
    pub fn feed(&mut self, mut samples: &[i16]) -> bool {
        let Some(detect) = self.iface.detect else { return false };
        let mut heard = false;
        while !samples.is_empty() {
            let n = (self.chunk.len() - self.filled).min(samples.len());
            self.chunk[self.filled..self.filled + n].copy_from_slice(&samples[..n]);
            self.filled += n;
            samples = &samples[n..];
            if self.filled == self.chunk.len() {
                self.filled = 0;
                // SAFETY: the chunk holds the model's chunk size of samples.
                // WakeNet returns the index (from 1) of the word it heard.
                heard |= unsafe { detect(self.model, self.chunk.as_mut_ptr()) } > 0;
            }
        }
        heard
    }
}

impl Drop for Detector {
    fn drop(&mut self) {
        if let Some(destroy) = self.iface.destroy {
            // SAFETY: the model is live and not used again.
            unsafe { destroy(self.model) };
        }
    }
}

/// Loads the wanted model and feeds it samples, forever.
#[embassy_executor::task]
async fn detect_task(models: &'static [Model]) -> ! {
    let mut detector: Option<Detector> = None;
    let mut bytes = [0u8; 1024];
    // Bytes in `bytes`; an odd one waits for the rest of its sample.
    let mut have = 0;
    let mut samples = [0i16; 512];
    loop {
        let wanted = WANTED.load(Ordering::Relaxed);
        if wanted != LOADED.load(Ordering::Relaxed) {
            // Free the old model first: both may not fit.
            drop(detector.take());
            detector = models.get(usize::from(wanted)).and_then(Detector::new);
            match (&detector, models.get(usize::from(wanted))) {
                (Some(_), Some(m)) => log::info!("wake word model {} (\"{}\")", m.name, m.word),
                _ => log::error!("cannot load wake word model {wanted}"),
            }
            RUNNING.store(detector.is_some(), Ordering::Relaxed);
            LOADED.store(wanted, Ordering::Relaxed);
            // Drop what queued up while loading.
            SAMPLES.clear();
            have = 0;
        }
        have += SAMPLES.read(&mut bytes[have..]).await;
        let n = have / 2 * 2;
        for (s, b) in samples.iter_mut().zip(bytes[..n].chunks_exact(2)) {
            *s = i16::from_le_bytes([b[0], b[1]]);
        }
        bytes.copy_within(n..have, 0);
        have -= n;
        let Some(d) = &mut detector else { continue };
        if d.feed(&samples[..n / 2]) {
            let count = COUNT.fetch_add(1, Ordering::Relaxed) + 1;
            log::info!("wake word (#{count})");
            audio::play(&[Note { hz: 660, ms: 90 }, Note { hz: 880, ms: 140 }]);
        }
    }
}

extern "C" fn detect_thread(_: *mut c_void) {
    let models = MODELS.try_get().map(Vec::as_slice).unwrap_or_default();
    let exec = make_static!(esp_rtos::embassy::Executor::new());
    exec.run(|s| s.spawn(detect_task(models).unwrap()))
}

/// Starts the detector thread on core 0 below the audio task, on the
/// models in [`MODELS`].
pub fn start() {
    // SAFETY: the thread never returns.
    unsafe { sched::spawn("wakeword", detect_thread, sched::PRIO_WAKE, STACK, 0) };
}
