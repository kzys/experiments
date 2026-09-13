# PLAN

A gamepad-driven Jellyfin audio player for the RG35xx Plus.

## Where this is (v0)

A fullscreen gamepad UI (albums → tracks) authenticates, browses, and plays
the selection with `mpv` on its direct-play stream URL. No artist level:
Jellyfin's artist tagging on this library is too messy to browse by
("feat. X" splitting one act into several MusicArtist entries, "VA"
compilations, most artists tag-only with no folder of their own) — see
api.rs's list_albums for the full story. Albums don't have that problem,
being real folders regardless of the tagging above them. See README.md for
the exact status. Everything below is next.

## Audio format

Jellyfin transcodes on the fly — the stream URL just needs `static=true`
dropped and `audioCodec`/`maxAudioBitrate` added, and the server re-encodes
FLAC (or anything else) to the requested codec per request, no pre-conversion
needed on our side.

AAC is an ordinary file (`.m4a`), not a streaming-only format — the same
bytes a transcoded stream sends can be written to disk instead of piped to
`mpv`, and played back later with no network at all. So "transcode for
playback" and "cache for offline" are the same download, just with a
different destination. That argues for building the transcoded stream first
and getting a local cache almost for free, rather than a separate
FLAC-to-lossy conversion pipeline living outside Jellyfin.

Default: 192kbps AAC-LC for every transcode, untested on the device but
comfortably past "transparent" for a handheld's built-in speaker or a 3.5mm
jack. Treated as the shipped default, not a blocker — revisit only if it
audibly sounds bad on the actual hardware.

## Login

Typing a password with a D-pad and face buttons is miserable, so v0's stdin
prompt is a placeholder, not the intended UX. Jellyfin has an answer for
exactly this — **Quick Connect**, built for TVs/consoles with no keyboard —
and it's confirmed working end-to-end against the real server:

1. `POST /QuickConnect/Initiate` → `{Secret, Code, ...}`. Show `Code` (a
   6-digit number) on screen.
2. Poll `GET /QuickConnect/Connect?secret=...` until `Authenticated: true`.
   The user enters `Code` once on any already-logged-in client (phone,
   web) — nothing is typed on the handheld itself.
3. `POST /Users/AuthenticateWithQuickConnect {Secret}` → same
   `{AccessToken, User}` shape as `AuthenticateByName`. Verified: the
   resulting token authorizes normal API calls exactly like a
   password-derived one.

This becomes the default login path. `-u`/`-p` (or the env vars) stay as an
explicit override for scripting/testing — not removed, just no longer the
primary flow a person actually uses on the device.

Only the resulting `AccessToken` + `user_id` get persisted — never a
password, whichever login path produced it. See "Roadmap" below for where
and how.

## UI

Done, but not the way originally sketched here: rather than lowkey's raw
`wayland-client` approach (shm buffer, `fontdue`, hand-rolled layout), the
UI is egui (immediate-mode) drawn via `egui_glow` (OpenGL ES through
glutin) into a plain fullscreen `xdg_shell` toplevel obtained through
winit's Wayland backend. Simpler to build a scrolling list UI against, at
the cost of pulling in a toolkit rather than staying dependency-minimal
like lowkey. Evdev reads for the pad still happen directly on their own
thread (see pad.rs) — no toolkit here understands gamepads — forwarding
decoded nav events to the winit event loop over an `EventLoopProxy`.

egui's `default_fonts` feature is off (saves ~1.4MB in the release binary),
so text is drawn with a font already on the device — originally
EmulationStation's Rubik, now PPSSPP's Inconsolata (see ui.rs for why) —
falling back to EmulationStation's Droid Sans Fallback Full for CJK glyphs
it doesn't have. Also
not in the original sketch: a `log` crate logger (util.rs) that persists to
disk and keeps a handful of recent lines visible in an on-screen panel —
the only way to see what's going wrong on a kiosk device with no console.

### Text input (server address, etc.)

Done for the server address field on the "Can't reach server" screen (`A`
to edit, `Select` on lowkey when finished); confirmed working end-to-end on
the device. Delegates to lowkey (or any compatible software keyboard)
rather than building an on-screen keyboard into this app, shown and hidden
the way iOS/Android do:

- lowkey never takes Wayland keyboard focus itself
  (`KeyboardInteractivity::None`) and types through a `uinput` virtual
  keyboard, so whatever *does* have focus — our surface — just receives
  ordinary `wl_keyboard` events, indistinguishable from a real keyboard. No
  IPC or lowkey-specific protocol needed.
- Show/hide *is* the process lifecycle: spawn lowkey when a text field is
  activated, and treat its exit (however triggered — it has no way to hand
  a typed value back other than the keystrokes already sent) as "done
  editing", which re-authenticates with whatever ended up in the field.
- Turned out not to be "for free": *we* grab the gamepad exclusively at
  startup (to shut EmulationStation out), so lowkey's own `EVIOCGRAB` would
  otherwise find the device already claimed and — per its own "log and
  keep going ungrabbed" handling — sit on screen deaf to button presses.
  Fixed by releasing our grab before spawning lowkey and reacquiring it once
  lowkey's process exits.
- ROCKNIX has no `/usr/share/fonts`, so lowkey's own default font path
  doesn't exist on-device either — it died on spawn until fixed the same
  way `load_fonts()` above is: `-f` pointed at EmulationStation's bundled
  Rubik, the same font lowkey's own Terminal.sh already uses for this
  reason.
- lowkey requests the wlr-layer-shell *Overlay* layer specifically because,
  per its own source comment, "EmulationStation runs fullscreen, and sway
  draws fullscreen surfaces over everything below overlay" — confirmed on
  the device: it draws on top of our egui+glow fullscreen surface with no
  layout changes needed on our side.
- No custom evdev-keycode → ASCII table needed: now that the UI goes
  through winit rather than raw `wayland-client`, incoming `wl_keyboard`
  events already arrive as decoded text via winit's own `xkbcommon` use —
  one fewer thing this step had to build.

## Roadmap

1. **Gamepad UI** — done; see "UI" above for the approach taken. Text entry
   for the server address is also done (see "Text input" above); Quick
   Connect's `Code` still needs a field once Quick Connect itself (next
   item) is built.
2. **Login & persisted session** — Quick Connect by default (see above),
   `-u`/`-p` as an explicit fallback. Persist only the access token +
   user id, `0600`, at `$XDG_STATE_HOME/music/token` (else
   `$HOME/.local/state/music/token`) — this ecosystem has no OS keyring to
   delegate to (checked: no Secret Service, no usable `systemd-creds`
   without a TPM or persistent `/var`) and ROCKNIX's own ConnMan stores
   WiFi passwords the same way (plaintext under `/storage`), so this isn't
   a compromise, it's how this platform already does it. No need to track
   token lifetime up front either — retry once on a 401 from any
   authenticated call (Quick Connect again, or `AuthenticateByName` if
   `-u`/`-p` were given) and replace the stored token, which recovers
   regardless of *why* the old one died.
3. **Transcoded streaming + local cache, together** — drop `static=true`,
   request AAC at a modest bitrate, and as those bytes come back, tee them
   both to `mpv`'s stdin (for on-the-fly playback) and to a cache file
   (`item-id.aac`). One download serves both purposes from the start,
   instead of streaming now and bolting caching on later.
   - Cache hit: if the file already exists, trust it and play straight from
     disk — no re-download, no validation. Good enough until corruption or
     partial files actually show up as a problem.
   - Early exit: if `mpv` quits before the stream finishes (skip, short
     track), keep downloading anyway so the cache file still ends up
     complete.
   - Location: plain XDG — `$XDG_CACHE_HOME/music`, else `$HOME/.cache/music`.
     No dedicated override env var; XDG_CACHE_HOME is already one if
     needed.
   - Size: the whole cache directory capped at 1 GB (not per file). Once at
     the cap, stop caching new downloads — playback still works (streamed,
     just not saved), but nothing is ever auto-deleted. No LRU eviction:
     deleting something the user expected to still be offline is worse than
     an occasionally-full cache. Reaching the cap requires noticing and
     clearing it manually.
4. **Playback control** — partially done: selecting a track no longer
   blocks the event loop (`mpv` is spawned rather than waited on), but
   changing tracks still kills the old process and starts a new one. Actual
   pause/skip/volume without a restart still needs driving a single
   long-lived `mpv` over its JSON IPC socket (`--input-ipc-server`) instead
   of one process per track — not yet built.
5. **Offline sync** — an explicit command to pre-download a chosen album/
   playlist, so the device can go fully offline rather than only caching
   what's already been played once. Needs reconciling with step 3's 1 GB
   cap when it's built: an explicit, deliberate sync arguably shouldn't be
   silently refused just because opportunistic caching filled the cache
   first — undecided for now since sync doesn't exist yet.

## Open questions

None right now — see the resolutions folded into the roadmap above. New
ones will show up once step 1 is actually implemented.
