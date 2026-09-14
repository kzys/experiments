# TODO

## Redesign follow-ups

The existing screens (Albums, Tracks, Can't reach server) plus the new
Server address screen now follow the "Music Player Scope" design canvas.
What's left of it needs real data-model or process work rather than
drawing, and is stubbed or approximated in the meantime:

- **Quick Connect login screen** (PLAN.md step 1). Not started: login is
  still username/password from `-u`/`-p`, env, or a stdin prompt. Needs
  the `Initiate`/`Connect`/`AuthenticateWithQuickConnect` flow in api.rs,
  a "Connect to Jellyfin" screen showing the 6-digit code and polling
  for confirmation, and the resulting token persisted to
  `$XDG_STATE_HOME/music/token` (0600) so the code is only ever entered
  once.
- **mpv over JSON IPC** (PLAN.md step 4). Still one `mpv` process per
  track. Stopgaps in ui.rs until one long-lived mpv is driven over
  `--input-ipc-server`:
  - X play/pause sends SIGSTOP/SIGCONT to the mpv process.
  - The "1:42 / 4:21" position on the playing row is wall-clock time since
    spawn minus pauses, not mpv's real position.
  - Playback doesn't advance to the next track when one ends; the ▶
    marker just disappears on the next redraw after mpv exits.
  - L1/R1 skip works by killing mpv and spawning it on the adjacent track
    of the album the playing track came from.
- **Local cache** (PLAN.md step 3). The stream is transcoded AAC now, but
  nothing is cached, so the mockup's "caching to ~/.cache/music/<id>.aac"
  log line doesn't exist yet.
- **Unverified on the device**:
  - `Fields=ChildCount` on the album listing actually returning counts for
    MusicAlbum items — if not, the "N tracks" column is blank.
  - X decoding as `BTN_NORTH` (assumed from the Nintendo-style A/B
    mapping lowkey confirmed).
