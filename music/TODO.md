# TODO

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
