# experiments

Small projects that don't need a repository of their own yet. Each directory
stands alone, with its own README, build and tests. Some of them are vibe
coded. `make` in a directory runs what CI checks for it.

## herdr-panel

A 3.5" touch display (Freenove FNK0104N, ESP32-S3) that shows herdr's agents
as a pixel-art tower: one or more storeys per workspace, each agent the Claude
Code mascot at a desk, animated by what it is doing. Tapping a mascot focuses
that agent in herdr.

See [herdr-panel/README.md](herdr-panel/README.md) for the design, the
messages, and how to build and flash the firmware.

## speaker

The same board as a small speaker that listens for a wake word: Rust on
esp-hal and LVGL, with Espressif's prebuilt ESP-SR libraries linked in for
WakeNet. Tabs for the LED, the microphone, tones and the wake word.

See [speaker/README.md](speaker/README.md) for how ESP-SR runs without
ESP-IDF and how to build and flash it.

## music

A Jellyfin audio client for the Anbernic RG35xx Plus handheld running
ROCKNIX: an egui UI driven by the gamepad, with playback through mpv.

See [music/README.md](music/README.md) for what works and how to build and
install it.
