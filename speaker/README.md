# speaker

The Freenove FNK0104N (ESP32-S3, 3.5" 320x480 ST77922 over QSPI) as a
small speaker that listens for a wake word. Rust on esp-hal with oxivgl for
LVGL, plus Espressif's prebuilt ESP-SR libraries for the wake word.

Tabs: LED buttons (WS2812), microphone level meter, speaker tones, wake
word. The wake word tab lists the models (Hi,ESP, Computer, Alexa) and
switches between them; a detection flashes the LED green and plays a chime.

A port of the C version in `esp32-s3-hello` (ESP-IDF), which has the same
tabs.

## Build

Needs the Xtensa Rust toolchain from `espup`, plus `pypng` and `lz4` on
`PATH`'s Python for LVGL's image tool:

    ./fetch-esp-sr.sh
    ESP_EXPORT=~/export-esp.sh . ./env.sh
    cargo +esp build --release
    espflash flash --chip esp32s3 --flash-size 16mb target/xtensa-esp32s3-none-elf/release/speaker

`fetch-esp-sr.sh` downloads ESP-SR and the two components it needs
(dl_fft, cJSON) from Espressif's component registry into `vendor/`.

The LVGL configuration is `conf/lv_conf.h`.

## ESP-SR without ESP-IDF

ESP-SR's WakeNet is a set of prebuilt static libraries built for ESP-IDF.
They are linked into the no_std firmware as they are, and what they expect
from ESP-IDF is filled in:

- `build.rs` compiles `csrc/sr_glue.c`, dl_fft and cJSON, links
  `libwakenet`, `libdl_lib`, `libc_speech_features` and `libhufzip` with
  newlib's libc and libm in one group, and packs the models listed in
  `MODELS` into an `srmodels.bin` image that the firmware includes.
- `csrc/sr_glue.c`: the model list (read from the image in flash, as ESP-SR
  does from a memory-mapped partition), `printf` and ESP-IDF logging, and
  stand-ins for FreeRTOS mutexes, newlib locks and file I/O.
- `src/sr.rs`: `malloc` and ESP-IDF's `heap_caps_*` on esp-alloc (large
  blocks and models go to PSRAM), log output, and esp-dl's dot products,
  which the speech features call.
- `src/wakeword.rs`: the safe wrapper and the detector thread.

Only WakeNet9 models work: WakeNet10 needs esp-dl's C++ runtime, which is
not linked; a stand-in makes creating one fail.

Things that took finding:

- esp-hal's linker script places bare `.literal` sections first, out of
  L32R reach of the libraries' assembly; `build.rs` renames them in copies
  of the archives so each stays next to its code.
- Loading a model takes longer than the I2S DMA buffers last, and an esp-hal
  stream that runs dry stops for good. Detection runs on its own thread
  below the audio task, fed through a pipe, so audio never waits for it.
- The receiver must use the transmitter's BCLK and WS
  (`with_signal_loopback`). Connecting both units to the clock pins lets the
  receiver drive them, out of phase with the transmitted data, and the
  speaker sounds loud and distorted at any volume.

## Board notes

| Part | Pins |
| --- | --- |
| LCD (QSPI) | CS 10, SCLK 12, D0-D3 11/13/14/9, backlight 41 |
| Touch (I2C 0x55, shared with codec) | SDA 38, SCL 39, RST 48, INT 47 |
| ES8311 codec (I2C 0x18) | I2S MCLK 17, BCLK 18, WS 21, DIN 16, DOUT 15; amp off = GPIO1 high |
| RGB LED (WS2812B) | GPIO 40 |

- The panel needs big-endian RGB565 and update areas on multiples of 4 pixels.
- The touch controller only clears its data-ready flag after all point records are read.
- The mic needs the I2S transmitter running: the receiver takes its clocks from it.
