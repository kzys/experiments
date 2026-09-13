# music

A Jellyfin audio client for the RG35xx Plus.

## Status

Working:

- Authenticates against a Jellyfin server by username/password
  (`Users/AuthenticateByName`), then opens a fullscreen egui UI (via
  `egui_glow`/glutin, in a plain `xdg_shell` toplevel) for browsing albums
  → tracks with the gamepad: D-pad to move, A to select, B to back out a
  level, Select to quit. No artist level — see PLAN.md. Playing a track
  shells out to `mpv
  --no-video` on its direct-play stream URL and blocks until it exits —
  there's no pause/skip/stop yet (see PLAN.md's playback-control step).
- Server/username/password come from `-s`/`-u`/`-p` or
  `$JELLYFIN_SERVER`/`$JELLYFIN_USER`/`$JELLYFIN_PASSWORD`, so the password
  needn't land in shell history.
- Cross-builds for aarch64 the same way as `../lowkey`: rustls instead of
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
