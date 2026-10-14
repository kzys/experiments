# herdr-panel

A small touch display (Freenove FNK0104N, ESP32-S3, 3.5" 320x480) that shows
herdr as a tower in the style of SimTower and 16-bit console games: storeys
of six desks, one or more per workspace, and each agent as the Claude Code
mascot at a desk, animated by what the agent is doing. The elevator rides to
the focused agent; arrow buttons over the shaft scroll a tall tower.
Tapping a mascot focuses that agent in herdr. The board talks to a bridge on
the laptop over USB serial.

```
herdr  <--Unix socket-->  bridge (laptop)  <--USB serial-->  firmware (board)
```

All Rust.

- `protocol/`: the messages, `no_std`, shared by both sides, and how agents
  are split into storeys. One JSON value per line; externally tagged enums so
  `serde-json-core` on the board can parse them. Tests check the board's parser
  against the bridge's output.
- `art/`: the mascot and its animations as pixel frames, `no_std`, tested on the
  laptop. The mascot is the block-character art `▐▛███▜▌ / ▝▜█████▛▘ / ▘▘ ▝▝`
  turned into pixels (the two gaps in the top row are the eyes). One looping
  animation per state, plus a random "play" mood that is written but not used yet.
  Each pixel is drawn one wide and two tall, like the terminal's character cells.
- `assets/`: the tower's art as PNG files (see Art below).
- `scene/`: loads the art and puts the tower together on the laptop: desks
  onto storeys, lamps lit or the room tinted by moonlight when every agent
  there is idle, and the flat, outlined mascot frames, cut where their desk
  hides them. Also says where everything goes. `cargo run -p
  herdr-panel-scene --example preview -- out.png 2,1,8 [scroll]` draws the
  screen with workspaces of two, one and eight agents.
- `bridge/`: reads `agent.list` and `workspace.list` from herdr's socket
  (`$HERDR_SOCKET_PATH`), pushes the agents to the board when they change, and
  runs `agent.focus` when the board asks. It subscribes to herdr's change events
  and also polls every 2 s in case one is missed.
- `firmware/`: esp-hal and oxivgl. The sky and city are one still,
  full-screen image; the street, lobby and storeys sit in front of it in a
  container that the arrow buttons scroll a storey at a time (a jump: redrawing
  the whole screen is too slow to slide). Each workspace (the pane id's part
  before `:`) takes as many storeys as its agents need. Each agent is a mascot
  looping its frames; a green plumbob bobs over the focused one, the elevator
  car slides to its storey, and the view jumps there unless an arrow was
  pressed in the last 10 s. LVGL gets 512 KB of PSRAM on top of its 48 KB
  internal pool, for the widgets of a tall tower.
  `build.rs` renders the images from `scene/` and `art/` to PNGs, converts
  them with LVGL's `LVGLImage.py` and compiles them in.
  Separate from the workspace because it needs the Xtensa toolchain
  (`cargo +esp`).

## Art

The PNGs in `assets/` are the source: edit them in any pixel editor and
rebuild the firmware. They are at half the screen's resolution (the board
shows each pixel as 2x2), and colours are cut to the SNES's 15 bits on
loading, so a colour may shift by up to 7 per channel. Translucent pixels
blend (the lamp glow, the neon sign's halo).

| file | what |
| --- | --- |
| `backdrop.png` | sky, moon and city, 160x240; stays still |
| `street.png` | pavement and road below the tower, 160x17 |
| `lobby.png` | storey 0, 160x24 |
| `storey-0.png` .. `storey-3.png` | a storey's room, one wallpaper per workspace in turn, 160x24, with no desks and the lamps off |
| `roof-on.png`, `roof-off.png` | the roof, its antenna light on and off, 160x14 |
| `sprites.png` | the small sprites, on one sheet; `sprites.yaml` gives each one's `[x, y, width, height]` |

The sprites in `sprites.png`:

| name | what |
| --- | --- |
| `desk` | a desk with its monitor, 22x10; drawn at each of the six desk places |
| `lamp-on` | a lit bulb and its light, drawn over each lamp fitting |
| `moonbeam` | moonlight through a window, for storeys with the lights off |
| `car` | the elevator car |
| `arrow-up`, `arrow-down` | the scroll buttons, 14x14 |
| `digits` | 0 to 9 for storey numbers, 4x6 each, in a row |
| `plumbob` | the marker over the focused agent |

To move or resize a sprite, change it in `sprites.png` and its entry in
`sprites.yaml`. A misspelt or missing name, a sprite outside the sheet, or
two that overlap fail the scene tests.

Sizes and positions (where desks, lamps and agents go) are constants in
`scene/src/lib.rs`; its tests fail if a PNG's size no longer matches them.
The mascot is not a PNG: its poses come from the terminal block art in
`art/`.

## Animations

| herdr status | mascot |
| --- | --- |
| working | running on the spot, arms swinging |
| idle | dozing, eyes half shut, "z"s rising |
| blocked | waving, with a blinking "!" |
| done | hopping, arms up, sparkles |
| unknown | a "?" above it |

The "z", "!", "?" and sparkles stay flat, next to the mascot's head.

## Messages

Bridge to board, a state as one line each: `{"Begin":{"count":2}}`, then
`{"Agent":{"id","label","status","focused"}}` per agent, then `"End"`. `id` is
the herdr pane id. The board decodes one agent at a time and shows the state
only when all announced agents arrived: decoding a whole state at once needs
several copies of it on the board's stack, which overflowed it. Board to
bridge: `"Hello"` (sent at start and every 3 s until a state arrives) and
`{"Focus":{"agent":"w1:p2"}}`. `status` is `idle`, `working`, `blocked`, `done`
or `unknown`. Limits: 100 agents, labels cut to 24 bytes (the board does not
show labels at the moment).

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
  `~/ws/esp32-s3-hello` (`bench-rust.sh`). That might make sliding scrolls
  usable; not tried.
- Tapping a mascot moves your real herdr focus.
- The panel and touch drivers are copied from `~/ws/esp32-s3-hello/rust`.
