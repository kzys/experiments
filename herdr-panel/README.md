# herdr-panel

A small touch display (Freenove FNK0104N, ESP32-S3, 3.5" 320x480) that shows
herdr's workspaces and tabs with their agent status. Tapping a tab focuses it
in herdr. The board talks to a bridge on the laptop over USB serial.

```
herdr  <--Unix socket-->  bridge (laptop)  <--USB serial-->  firmware (board)
```

All Rust.

- `protocol/`: the messages, `no_std`, shared by both sides. One JSON object
  per line; externally tagged enums so `serde-json-core` on the board can parse
  them. Tests check the board's parser against the bridge's output.
- `bridge/`: reads `workspace.list` and `tab.list` from herdr's socket
  (`$HERDR_SOCKET_PATH`), pushes the layout to the board when it changes, and
  runs `tab.focus` when the board asks. It subscribes to herdr's change events
  and also polls every 2 s in case one is missed.
- `firmware/`: esp-hal and oxivgl. One card per workspace (status dot, name),
  one button per tab, coloured by status. Separate from the workspace because
  it needs the Xtensa toolchain (`cargo +esp`).

## Messages

Bridge to board: `{"State":{"workspaces":[{"id","label","status","focused","tabs":[...]}]}}`.
Board to bridge: `"Hello"` (sent at start and every 3 s until a state arrives)
and `{"Focus":{"tab":"w1:t2"}}`. `status` is `idle`, `working`, `blocked`, `done`
or `unknown`. Limits: 8 workspaces, 6 tabs each, labels cut to 24 bytes.

The board's log output shares the port. Lines that do not start with `{` or `"`
are logs; the bridge prints them as `board: ...`.

## Run

Bridge (inside a herdr pane, or pass `--socket`):

    cargo run --release -p herdr-panel-bridge -- --port /dev/ttyACM0

Opening the port resets the board (USB-Serial-JTAG wires DTR/RTS to reset), so
it restarts and asks for the state.

Firmware (needs espup's Xtensa toolchain, and `pypng` and `lz4` on `PATH`'s
Python for LVGL's image tool):

    cd firmware
    ESP_EXPORT=~/export-esp.sh . ./env.sh
    cargo +esp build --release
    espflash flash --chip esp32s3 --flash-size 16mb target/xtensa-esp32s3-none-elf/release/herdr-panel-firmware

Tests: `cargo test` at the root.

## Notes

- Flashing with espflash's own bootloader runs the flash in DIO mode at 40 MHz;
  rendering is much faster behind the ESP-IDF bootloader (QIO, 80 MHz). See
  `~/ws/esp32-s3-hello` (`bench-rust.sh`). Not needed for this UI.
- Tapping a tab moves your real herdr focus.
- The panel and touch drivers are copied from `~/ws/esp32-s3-hello/rust`.
