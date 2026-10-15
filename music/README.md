# music

A Jellyfin audio client for the RG35xx Plus.

## Status

Working:

- Authenticates against a Jellyfin server by username/password
  (`Users/AuthenticateByName`), then opens a fullscreen egui UI (via
  `egui_glow`/glutin, in a plain `xdg_shell` toplevel) for browsing albums
  → tracks with the gamepad: D-pad to move, A to select, B to back out a
  level, X to play/pause, L1/R1 to skip, Select to quit. Select+Start
  toggles hold mode for carrying the device in a bag: buttons are ignored,
  the backlight goes off (`bl_power`, as ROCKNIX's own fake-suspend does),
  and the power button is grabbed so a stray press can't trigger the OS's
  suspend — playback continues; the volume rocker still works since the
  OS handles it. No artist level — see PLAN.md. Text renders with
  Inconsolata (borrowed from PPSSPP's assets — monospace for tabular digits,
  which egui can't get from an OpenType feature), falling back to
  EmulationStation's bundled Droid Sans Fallback for CJK glyphs neither it
  nor egui's own (unbundled, to save binary size) fonts cover.
- Playback is one long-lived `mpv --no-video --idle` driven over its JSON
  IPC socket (mpv.rs). Selecting a track loads it and the rest of the
  album into mpv's playlist as stream URLs, which ask Jellyfin to
  transcode to 128kbps AAC on the fly so the original (FLAC, mostly) never
  crosses the WiFi; mpv then plays through to the end of the album by
  itself. Nothing is cached yet (see PLAN.md). Skip reloads the playlist
  from the adjacent track; pause, the position counter and the ▶ marker
  follow what mpv reports back over the socket.
- A handful of the most recent log lines are always visible on screen (log
  panel), and the full log is persisted to disk — the only way to see
  what's going wrong on a kiosk device with no console.
- Server/username/password come from `-s`/`-u`/`-p` or
  `$JELLYFIN_SERVER`/`$JELLYFIN_USER`/`$JELLYFIN_PASSWORD`, so the password
  needn't land in shell history. With no server set at all, the UI opens
  on a "Server address" screen and asks for one there (typed via lowkey).
- Cross-builds for aarch64 the same way as
  [lowkey](https://github.com/kzys/lowkey): rustls instead of
  OpenSSL, so there's nothing to have on the device's sysroot before linking.
- `rocknix/Music.sh` is the EmulationStation Ports launcher (shows up as
  "Music"); it sources `/storage/.config/music.env` for credentials rather
  than having them baked into a deployed, committed file.

## Build

```
make               # cross-builds target/aarch64-unknown-linux-gnu/release/music
make install       # scp the binary to the device, killing any running instance first
make install-config # scp rocknix/Music.sh into the device's Ports directory
```

`HOST` in the Makefile can be overridden on the command line. The target
needs `rustup target add aarch64-unknown-linux-gnu` and an
`aarch64-linux-gnu-gcc` on `PATH` for linking (configured in
`.cargo/config.toml`).

Playback needs `mpv` on the device.

## Roadmap

See PLAN.md.
