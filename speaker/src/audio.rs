//! Microphone level and speaker tones over one full-duplex I2S port; the
//! microphone also feeds the wake word detector.
//!
//! The receive side takes its clocks from the transmit side, so transmit runs
//! all the time and sends silence when nothing is queued. Only the amplifier
//! is switched on and off.

use core::sync::atomic::{AtomicU8, Ordering};

use embassy_sync::{blocking_mutex::raw::CriticalSectionRawMutex, channel::Channel};
use embassy_time::{Instant, Timer};
use esp_hal::{
    dma_rx_stream_buffer, dma_tx_stream_buffer,
    gpio::Output,
    i2s::master::{I2sRx, I2sTx},
};

use crate::wakeword;

/// Sample rate of both directions.
pub const SAMPLE_RATE: u32 = 16_000;

/// One tone of a melody.
#[derive(Clone, Copy)]
pub struct Note {
    /// Frequency in hertz; 0 is a rest.
    pub hz: u16,
    /// Length in milliseconds.
    pub ms: u16,
}

static NOTES: Channel<CriticalSectionRawMutex, Note, 32> = Channel::new();

/// Speaker volume in percent.
pub static VOLUME: AtomicU8 = AtomicU8::new(50);

/// Microphone level, 0..=100 over -80..0 dBFS.
pub static MIC_LEVEL: AtomicU8 = AtomicU8::new(0);

/// Replaces whatever is playing with `notes`.
pub fn play(notes: &[Note]) {
    NOTES.clear();
    for n in notes {
        let _ = NOTES.try_send(*n);
    }
}

/// Stops playback.
pub fn stop() {
    NOTES.clear();
}

/// Sine generator that walks through notes sample by sample.
struct Tone {
    note: Option<Note>,
    done: u32,
    total: u32,
    phase: f32,
}

impl Tone {
    const fn new() -> Self {
        Self {
            note: None,
            done: 0,
            total: 0,
            phase: 0.0,
        }
    }

    /// Next sample, or `None` when nothing is queued.
    fn next(&mut self, amp: f32) -> Option<i16> {
        if self.note.is_none() || self.done >= self.total {
            let n = NOTES.try_receive().ok()?;
            self.total = SAMPLE_RATE * u32::from(n.ms) / 1000;
            self.note = Some(n);
            self.done = 0;
        }
        let n = self.note?;
        // 5 ms ramps at both ends avoid clicks.
        let fade = SAMPLE_RATE / 200;
        let left = self.total - self.done;
        let env = if self.done < fade {
            self.done as f32 / fade as f32
        } else if left < fade {
            left as f32 / fade as f32
        } else {
            1.0
        };
        let s = if n.hz == 0 {
            0.0
        } else {
            amp * env * libm::sinf(self.phase)
        };
        self.phase += core::f32::consts::TAU * f32::from(n.hz) / SAMPLE_RATE as f32;
        if self.phase > core::f32::consts::TAU {
            self.phase -= core::f32::consts::TAU;
        }
        self.done += 1;
        Some(s as i16)
    }
}

/// Sample amplitude for a volume in percent: 0 is silent, 1..=100 spans
/// -40..0 dB of half scale, so each step sounds about as big.
fn amplitude(volume: u8) -> f32 {
    if volume == 0 {
        return 0.0;
    }
    let db = -40.0 * (100.0 - f32::from(volume.min(100))) / 99.0;
    32767.0 * 0.5 * libm::powf(10.0, db / 20.0)
}

/// Maps a block of samples to 0..=100 (-80..0 dBFS).
fn level(samples: &[i16]) -> u8 {
    if samples.is_empty() {
        return 0;
    }
    let sum: f32 = samples.iter().map(|&s| f32::from(s) * f32::from(s)).sum();
    let rms = libm::sqrtf(sum / samples.len() as f32);
    let db = 20.0 * libm::log10f((rms + 1.0) / 32768.0);
    ((db + 80.0) * 100.0 / 80.0).clamp(0.0, 100.0) as u8
}

/// Streams silence or tones out, measures the microphone and passes its
/// samples on to the wake word detector, forever.
pub async fn run(
    i2s_tx: I2sTx<'static, esp_hal::Blocking>,
    i2s_rx: I2sRx<'static, esp_hal::Blocking>,
    mut amp_off: Output<'static>,
) -> ! {
    let tx_buf = dma_tx_stream_buffer!(4096, 1024);
    let rx_buf = dma_rx_stream_buffer!(4096, 1024);

    let mut tx = match i2s_tx.write(tx_buf) {
        Ok(t) => t,
        Err((e, ..)) => panic!("i2s tx start: {:?}", e),
    };
    // Prime the transmitter with silence before the receiver starts.
    let silence = [0u8; 1024];
    while tx.available_bytes() >= silence.len() {
        tx.push(&silence);
    }
    let mut rx = match i2s_rx.read(rx_buf) {
        Ok(r) => r,
        Err((e, ..)) => panic!("i2s rx start: {:?}", e),
    };

    let mut tone = Tone::new();
    let mut amp_on = false;
    let mut last_sound = Instant::now();
    let mut out = [0u8; 1024];
    let mut inp = [0u8; 1024];
    let mut samples = [0i16; 512];

    loop {
        // Transmit: fill the free space with tone or silence.
        let free = tx.available_bytes().min(out.len()) / 2 * 2;
        if free > 0 {
            let amp = amplitude(VOLUME.load(Ordering::Relaxed));
            let mut playing = false;
            for chunk in out[..free].chunks_exact_mut(2) {
                let s = tone.next(amp).inspect(|_| playing = true).unwrap_or(0);
                chunk.copy_from_slice(&s.to_le_bytes());
            }
            if playing {
                last_sound = Instant::now();
                if !amp_on {
                    amp_off.set_low();
                    amp_on = true;
                }
            }
            tx.push(&out[..free]);
        }
        if amp_on && last_sound.elapsed().as_millis() > 300 {
            amp_off.set_high();
            amp_on = false;
        }

        // Receive: measure what arrived since the last pass and hand it to
        // the detector; what does not fit (while it loads a model) is
        // dropped.
        let avail = rx.available_bytes().min(inp.len()) / 2 * 2;
        if avail > 0 {
            let bytes = rx.pop(&mut inp[..avail]);
            for (s, b) in samples.iter_mut().zip(inp.chunks_exact(2)).take(bytes / 2) {
                *s = i16::from_le_bytes([b[0], b[1]]);
            }
            MIC_LEVEL.store(level(&samples[..bytes / 2]), Ordering::Relaxed);
            let mut rest = &inp[..bytes];
            while let Ok(n) = wakeword::SAMPLES.try_write(rest) {
                rest = &rest[n..];
                if rest.is_empty() {
                    break;
                }
            }
        }
        Timer::after_millis(8).await;
    }
}
