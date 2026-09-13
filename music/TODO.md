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
    spawn minus pauses, not mpv's real position, and only refreshes when
    something else triggers a redraw (there's no periodic repaint).
  - Playback doesn't advance to the next track when one ends; the ▶
    marker just disappears on the next redraw after mpv exits.
  - L1/R1 skip works by killing mpv and spawning it on the adjacent track
    of the album the playing track came from.
- **Transcoded AAC stream + local cache** (PLAN.md steps 2–3). Stream URL
  still has `static=true`; nothing is cached, so the mockup's
  "caching to ~/.cache/music/<id>.aac" log line doesn't exist yet.
- **Unverified on the device** (no on-device run yet after the redesign):
  - `Fields=ChildCount` on the album listing actually returning counts for
    MusicAlbum items — if not, the "N tracks" column is blank.
  - X decoding as `BTN_NORTH` (assumed from the Nintendo-style A/B
    mapping lowkey confirmed).
  - The 800×600 mockup sizes rendered through `set_zoom_factor(0.8)` on
    the 640×480 panel, and Rubik's row height at 20pt fitting ~10 rows.

## B (Back) sometimes continues the last D-pad scroll

Symptom: after scrolling Up or Down in a list, pressing B sometimes visibly
scrolls the list again in that same direction, as if B were repeating the
last direction instead of going back.

Confirmed, via raw evdev logging on-device:
- A single B press decodes cleanly to exactly one `NavEvent::Back` — no
  stray `Up`/`Down` events appear alongside it, with or without the
  auto-repeat feature's `Repeater` state involved. The input layer
  (`pad.rs`) is not emitting a bogus nav event.
- `browser.back()` behaves correctly (`popped=true`/`false` logged as
  expected; no extra `move_selection` call anywhere in that path).

Tried and did *not* fix it: switched the list's `scroll_to_me` from
animated to `ScrollAnimation::none()` (see "Snap list selection into view
instead of animating"), on the theory that `egui_glow::EguiGlow::run()`
drops `ViewportOutput::repaint_delay` (confirmed by reading its source —
it destructures `ViewportOutput { commands, .. }` and discards the rest),
so an in-flight scroll animation could sit frozen until some unrelated
event happened to trigger the next redraw, at which point it'd take one
more step toward its target. That mechanism is real, but removing the
animation didn't fix the reported symptom, so it's either not the (whole)
cause, or something else is also going on. That change is still in
(instant snap is arguably better UX regardless), but the actual bug here
is still open.

Not yet checked:
- Whether this happens only right after a *held* (repeating) scroll, only
  after single taps, or both.
- Whether it's egui-side (e.g. `selectable_label`/list layout reacting to
  something on the B press) rather than input or scroll-animation related.
- Getting a clean raw-event + decoded-event capture for a case that
  actually reproduces the bug (the two captures taken so far, with the
  reporter's help, came back clean).
