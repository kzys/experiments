# herdr-panel

A small touch display (Freenove FNK0104N, ESP32-S3, 3.5" 320x480) that shows
herdr as a tower in the style of SimTower and 8-bit console games: one
storey per workspace, and each agent as the Claude Code mascot at a desk,
animated by what the agent is doing. The elevator rides to the focused agent.
Tapping a mascot focuses that agent in herdr. The board talks to a bridge on
the laptop over USB serial.

```
herdr  <--Unix socket-->  bridge (laptop)  <--USB serial-->  firmware (board)
```

All Rust.

- `protocol/`: the messages, `no_std`, shared by both sides. One JSON object
  per line; externally tagged enums so `serde-json-core` on the board can parse
  them. Tests check the board's parser against the bridge's output.
- `art/`: the mascot and its animations as pixel frames, `no_std`, tested on the
  laptop. The mascot is the block-character art `▐▛███▜▌ / ▝▜█████▛▘ / ▘▘ ▝▝`
  turned into pixels (the two gaps in the top row are the eyes). One looping
  animation per state, plus a random "play" mood that is written but not used yet.
- `scene/`: the tower, drawn on the laptop at half the screen's resolution
  and shown at twice the size, in a 16-colour palette (PICO-8's) with
  dithered shading and black outlines: night sky, skyline, street, lobby,
  storeys (lights off when every agent there is idle), roof, elevator, desks,
  and the mascot sprites. `cargo run -p herdr-panel-scene --example preview --
  out.png 2,1,3` draws the screen with three storeys of two, one and three
  agents.
- `bridge/`: reads `agent.list` and `workspace.list` from herdr's socket
  (`$HERDR_SOCKET_PATH`), pushes the agents to the board when they change, and
  runs `agent.focus` when the board asks. It subscribes to herdr's change events
  and also polls every 2 s in case one is missed.
- `firmware/`: esp-hal and oxivgl. The sky, street and lobby are one
  full-screen RGB565 image; storeys are stacked on it, one per workspace (the
  pane id's part before `:`), at most 7 with 4 desks each. Each agent is a
  mascot looping its frames with its desk drawn over it; a green plumbob bobs
  over the focused one and the elevator car slides to its storey.
  `build.rs` renders the images from `scene/` and `art/` to PNGs, converts
  them with LVGL's `LVGLImage.py` and compiles them in.
  Separate from the workspace because it needs the Xtensa toolchain
  (`cargo +esp`).

## Animations

| herdr status | mascot |
| --- | --- |
| working | running on the spot, arms swinging |
| idle | asleep, eyes closed, "z"s rising |
| blocked | waving, with a blinking "!" |
| done | hopping, arms up, sparkles |
| unknown | a "?" above it |

The "z", "!", "?" and sparkles stay flat, next to the mascot's head.

## Messages

Bridge to board: `{"State":{"agents":[{"id","label","status","focused"}]}}`,
where `id` is the herdr pane id. Board to bridge: `"Hello"` (sent at start and
every 3 s until a state arrives) and `{"Focus":{"agent":"w1:p2"}}`. `status` is
`idle`, `working`, `blocked`, `done` or `unknown`. Limits: 12 agents, labels cut
to 24 bytes (the board does not show labels at the moment).

The board's log output shares the port. Lines that do not start with `{` or `"`
are logs; the bridge prints them as `board: ...`.

## Run

Bridge (inside a herdr pane, or pass `--socket`):

    cargo run --release -p herdr-panel-bridge -- --port /dev/ttyACM0

Opening the port resets the board (USB-Serial-JTAG wires DTR/RTS to reset), so
it restarts and asks for the state.

Firmware (needs espup's Xtensa toolchain, and `pypng` and `lz4` on `PATH`'s
Python: the build converts the animation frames with LVGL's image tool):

    cd firmware
    ESP_EXPORT=~/export-esp.sh . ./env.sh
    cargo +esp build --release
    espflash flash --chip esp32s3 --flash-size 16mb target/xtensa-esp32s3-none-elf/release/herdr-panel-firmware

Tests: `cargo test` at the root.

## Notes

- Flashing with espflash's own bootloader runs the flash in DIO mode at 40 MHz;
  rendering is much faster behind the ESP-IDF bootloader (QIO, 80 MHz). See
  `~/ws/esp32-s3-hello` (`bench-rust.sh`). Not needed for this UI.
- Tapping a mascot moves your real herdr focus.
- The panel and touch drivers are copied from `~/ws/esp32-s3-hello/rust`.
