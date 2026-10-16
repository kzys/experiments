# herdr-panel

A small touch display (Freenove FNK0104N, ESP32-S3, 3.5" 320x480) that shows
herdr as a pixel-art tower in the style of SimTower and Tiny Tower: storeys
of eight desks, one or more per workspace, and each agent as
the Claude Code mascot at a desk, animated by what the agent is doing. The
elevator rides to the focused agent; arrow buttons over the shaft scroll a
tall tower.
Tapping a mascot focuses that agent in herdr. The board talks to a bridge on
the laptop over USB serial.

<img src="demo.png" width="640" alt="The tower at night: ten storeys of desks with mascots working, dozing, waving and hopping, an elevator on the left, and a lobby and street below">

A mock-up of the screen, drawn by `art-tools/demo.py` (its `demo-2x.png`).

```
herdr  <--Unix socket-->  bridge (laptop)  <--USB serial-->  firmware (board)
```

All Rust, apart from the Python scripts that draw the art.

- `protocol/`: the messages, `no_std`, shared by both sides, and how agents
  are split into storeys. One JSON value per line; externally tagged enums so
  `serde-json-core` on the board can parse them. Tests check the board's parser
  against the bridge's output.
- `assets/`: the tower's art as PNG files (see Art below).
- `art-tools/`: Python scripts that draw the art: `demo.py` draws a mock-up
  of the whole screen, `export.py` writes the assets from the same drawings.
- `scene/`: loads the art and puts the tower together on the laptop: desks
  onto storeys, lamps lit or, at night, the room tinted by moonlight (when
  every agent there is idle and none has the focus), and the mascot's frames,
  cut where their desk hides them.
  Also says where everything goes and how fast each animation plays. `cargo run -p
  herdr-panel-scene --example preview -- out.png 2,1,8 [scroll] [time] [hours]`
  draws the screen with workspaces of two, one and eight agents, at a time of
  day (`dawn`, `day`, `sunset` or `night`) with the clouds drifted for `hours`.
- `bridge/`: reads `agent.list` and `workspace.list` from herdr's socket
  (`$HERDR_SOCKET_PATH`), pushes the agents to the board when they change, and
  runs `agent.focus` when the board asks. It subscribes to herdr's change events
  and also polls every 2 s in case one is missed. It also sends its local time
  whenever the minute changes.
- `firmware/`: esp-hal and oxivgl. The sky is one still, full-screen image
  for the time of day by the bridge's clock (night until it sends one): dawn
  from 5:00, day from 7:00, sunset from 17:00, night from 19:00. By day four
  clouds drift across it a pixel an hour, placed by the hours since the Unix
  epoch, so they need no state. The street, lobby and storeys sit in front of it in a
  container that the arrow buttons scroll a storey at a time (a jump: redrawing
  the whole screen is too slow to slide). Each workspace (the pane id's part
  before `:`) takes as many storeys as its agents need; each storey gets a
  piece of decor at its right end and the workspace's label at its top right,
  in the 5x8 font, outlined (LVGL fonts that `build.rs` makes from it). Each
  agent is a mascot looping its frames; a green plumbob bobs over the focused
  one, the elevator car slides to its storey, and the view jumps there unless
  an arrow was pressed in the last 10 s. LVGL gets 512 KB of PSRAM on top of its 48 KB
  internal pool, for the widgets of a tall tower.
  `build.rs` renders the images from `scene/` to PNGs at screen size, converts
  them with LVGL's `LVGLImage.py` and compiles them in.
  Separate from the workspace because it needs the Xtensa toolchain
  (`cargo +esp`).

## Art

Everything is drawn pixel for pixel on the 320x480 screen; each agent has a
33x36 space. The art is drawn by `art-tools/demo.py` (a mock-up of the
whole screen, to try a look) and written to `assets/` by
`art-tools/export.py`:

    python art-tools/demo.py /tmp/demo [TIME] # demo.png, demo-2x.png, demo-3x.png, demo-touch*.png
    python art-tools/export.py assets

The PNGs in `assets/` are what the build reads, so they can also be touched
up in a pixel editor; a re-export overwrites such edits. Colours are cut to
15 bits on loading, so a colour may shift by up to 7 per channel.
Translucent pixels blend (the lamp glow, the moonbeams, the neon sign's
halo).

| file | what |
| --- | --- |
| `backdrop-dawn.png`, `-day`, `-sunset`, `-night` | the sky at each time of day, with its stars and sun or moon, 320x480; stays still. Dithered, so its gradient does not band |
| `clouds.png` | the clouds: a row per time of day that has them (dawn, day, sunset), 13 high, each cloud a pixel right of the last, as wide as `CLOUDS` in `scene/src/lib.rs` says |
| `street.png` | pavement and road below the tower, 320x17 |
| `lobby.png` | storey 0, 320x36 |
| `storey-0-dawn.png` .. `storey-3-night.png` | a storey's room, one wallpaper per workspace in turn, its windows showing each time of day's sky, 320x36, with no desks and the lamps off |
| `roof-on.png`, `roof-off.png` | the roof, its antenna light on and off, 320x22 |
| `sprites.png` | the small sprites, decor and mascot, on one sheet; `sprites.yaml` gives each one's `[x, y, width, height]` |
| `font-5x8.bdf` | the text font: X11's public-domain misc-fixed 5x8, ASCII only; read by the art scripts and `scene` |

The sprites in `sprites.png`:

| name | what |
| --- | --- |
| `desk` | a desk in oblique projection with a monitor, 19x14; drawn at each desk. The furniture in the lobby and the decor is drawn the same way |
| `lamp-on` | a lit bulb and its light, drawn over each lamp fitting |
| `moonbeam` | moonlight through a window, for storeys with the lights off |
| `decor-0` .. `decor-3` | bookshelf, water cooler, plant, coffee machine: one per storey in turn, at its right end |
| `car` | the elevator car |
| `arrow-up`, `arrow-down` | the scroll buttons, 14x14 |
| `digits` | 0 to 9 for storey numbers, 5x8 each, in a row |
| `plumbob` | the marker over the focused agent |
| `mascot-working`, `mascot-idle`, `mascot-awake`, `mascot-blocked`, `mascot-done`, `mascot-unknown` | the mascot's animations: frames of 30x26 in a row, as many as the strip is wide |

A misspelt or missing name in `sprites.yaml`, a sprite outside the sheet, or
two that overlap fail the scene tests. Sizes and positions (where desks,
lamps, decor and agents go) are constants in `scene/src/lib.rs`, as are the
mascot's frame timings; its tests fail if a PNG no longer fits them. A
mascot frame has the feet at (13, 21); the desk hides the mascot's lower
right.

The mascot is the terminal's block-character art
`▐▛███▜▌ / ▝▜█████▛▘ / ▘▘ ▝▝`, each pixel one wide and two tall like a
character cell, flat, with a dark outline that also makes the eyes.

## Animations

| herdr status | mascot |
| --- | --- |
| working | running on the spot, arms swinging |
| idle | dozing, eyes half shut, "z"s rising |
| idle, with the focus | awake, blinking now and then |
| blocked | waving, with a blinking "!" |
| done | hopping, arms up, sparkles |
| unknown | a "?" above it |

Markers ("z", "!", "?", sparkles) have a dark border. Frames show for 150 ms
when working or done, 250 ms when blocked, 500 ms when idle or awake, and 1 s
(a single frame) when unknown.

## Messages

Bridge to board, a state as one line each: `{"Begin":{"count":2}}`, then
`{"Agent":{"id","label","workspace","status","focused"}}` per agent, then `"End"`. `id` is
the herdr pane id; `label` is the agent's name, or its workspace's label, and
`workspace` its workspace's label. The board decodes one agent at a time and shows the state
only when all announced agents arrived: decoding a whole state at once needs
several copies of it on the board's stack, which overflowed it. Also bridge to
board: `{"Clock":{"minute":450,"hour":493838}}` on its own, whenever the
minute changes: minutes since local midnight, and hours since the Unix epoch.
Board to bridge: `"Hello"` (sent at start and every 3 s until a state arrives) and
`{"Focus":{"agent":"w1:p2"}}`. `status` is `idle`, `working`, `blocked`, `done`
or `unknown`. Limits: 250 agents, labels cut to 24 bytes. The board shows
only workspace labels, with characters outside ASCII as `?`.

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
    . ./env.sh    # sources espup's export-esp.sh; set ESP_EXPORT if it isn't in $HOME
    cargo +esp build --release
    espflash flash --chip esp32s3 --flash-size 16mb target/xtensa-esp32s3-none-elf/release/herdr-panel-firmware

Tests: `make` (which runs `cargo test`) in this directory; CI runs the same.

## Notes

- Flashing with espflash's own bootloader runs the flash in DIO mode at 40 MHz;
  rendering is much faster behind the ESP-IDF bootloader (QIO, 80 MHz). That
  might make sliding scrolls usable; not tried.
- Tapping a mascot moves your real herdr focus.
