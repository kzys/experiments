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
- **mpv over JSON IPC** (PLAN.md step 4). Done in mpv.rs: one idle mpv
  for the app's lifetime, selecting a track loads it and the rest of the
  album as mpv's playlist, and the UI shows the position/pause state mpv
  reports. Left over:
  - L1/R1 skip reloads the playlist from the adjacent track rather than
    using `playlist-next`/`playlist-prev`, so skipping back re-fetches
    the stream and there's no prefetch across a skip.
  - No volume control.
- **Local cache** (PLAN.md step 3). The stream is transcoded AAC now, but
  nothing is cached, so the mockup's "caching to ~/.cache/music/<id>.aac"
  log line doesn't exist yet.
- **Unverified on the device**:
  - `Fields=ChildCount` on the album listing actually returning counts for
    MusicAlbum items — if not, the "N tracks" column is blank.
  - X decoding as `BTN_NORTH` (assumed from the Nintendo-style A/B
    mapping lowkey confirmed).
