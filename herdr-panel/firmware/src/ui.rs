//! A tower seen from the side, storeys of desks stacked on a lobby: each
//! workspace takes a storey, or several in a row if its agents do not fit
//! one. Each agent is a mascot behind its desk, looping the animation for its
//! state: running when it works, dozing with "z"s when idle, waving with "!"
//! when blocked, hopping when done, "?" when unknown. A storey's lights are
//! off when all its agents doze. The elevator rides to the focused agent's
//! storey and a plumbob floats over that agent. Tapping an agent asks the
//! bridge to focus it.
//!
//! The sky stays still; when the tower is taller than the screen, arrow
//! buttons over the elevator shaft scroll it a storey at a time (a jump, not a
//! slide: the panel is too slow to redraw a whole screen smoothly). The view
//! follows the focused agent, unless an arrow was pressed in the last
//! [`MANUAL_SCROLL_HOLD_SECS`] seconds.

use alloc::vec::Vec;
use embassy_time::Instant;
use herdr_panel_protocol::{Id, State, Status, ToHost, storeys};
use oxivgl::{
    anim::{ANIM_REPEAT_INFINITE, Anim, anim_path_ease_in_out, anim_set_y},
    enums::{EventCode, ObjFlag, ScrollbarMode},
    event::Event,
    indev::PointerIndev,
    view::{NavAction, View, register_event_on},
    widgets::{AnimImg, AsLvHandle, Image, Obj, WidgetError},
};

use crate::{
    frames::{
        ARROW_X, CAR_AT, DESK_SPACING, DESKS_PER_STOREY, DESKS_X, GROUND_Y, Loop, NUMBER_AT, PLUMBOB_ABOVE, ROOF_H,
        SLOTS, STOREY_H, arrow_down_image, arrow_up_image, backdrop_image, car_image, digit_image, lobby_image, loops,
        plumbob, roof, storey_image, street_image,
    },
    link,
    touch::POINTER,
};

const SCREEN_H: i32 = 480;
/// Width of the plumbob, in screen pixels.
const PLUMBOB_W: i32 = 10;
/// Sky kept above the roof when scrolled to the top, in screen pixels.
const TOP_MARGIN: i32 = 48;
/// How long the elevator takes per storey, in milliseconds.
const CAR_MS_PER_LEVEL: u32 = 250;
/// After a scroll by hand, how long the view stops following the focus.
const MANUAL_SCROLL_HOLD_SECS: u64 = 10;
/// Where the scroll buttons are, and how far their touch area reaches past
/// their edges, in screen pixels.
const ARROW_UP_Y: i32 = 8;
const ARROW_DOWN_Y: i32 = SCREEN_H - 36;
const ARROW_REACH: i32 = 10;

/// The top of storey `level` (0 is the lobby) with the view at the bottom.
fn storey_top(level: usize) -> i32 {
    GROUND_Y - STOREY_H * (level as i32 + 1)
}

/// The storey `level` and desk under point `y`, `x` with the view at the
/// bottom, if it is on a desk of a storey above the lobby.
fn desk_under(x: i32, y: i32) -> Option<(usize, usize)> {
    if y >= storey_top(0) || x < DESKS_X {
        return None;
    }
    let level = ((GROUND_Y - 1 - y) / STOREY_H) as usize;
    let desk = ((x - DESKS_X) / DESK_SPACING) as usize;
    (desk < DESKS_PER_STOREY).then_some((level, desk))
}

/// The widgets built for one state; each is a child of the scroller.
#[derive(Default)]
struct Rendered {
    images: Vec<Image<'static>>,
    sprites: Vec<AnimImg<'static>>,
    /// The agents on each storey above the lobby, by desk.
    storeys: Vec<Vec<Id>>,
}

pub struct Panel {
    indev: Option<PointerIndev>,
    backdrop: Option<Image<'static>>,
    scroller: Option<Obj<'static>>,
    up: Option<Image<'static>>,
    down: Option<Image<'static>>,
    car: Option<Image<'static>>,
    /// The level the car is at or heading for.
    car_level: usize,
    rendered: Rendered,
    loops: Vec<(Status, Loop)>,
    plumbob: Option<Loop>,
    roof: Option<Loop>,
    /// How far the content is shifted down so that it starts at 0: what
    /// "the view at the bottom" adds to every y.
    offset: i32,
    /// The level the view should show, once it may follow again.
    follow: Option<usize>,
    manual_scroll: Option<Instant>,
    shown: u32,
    /// The latest state from the bridge.
    state: State,
}

impl Panel {
    pub fn new() -> Self {
        Self {
            indev: None,
            backdrop: None,
            scroller: None,
            up: None,
            down: None,
            car: None,
            car_level: 0,
            rendered: Rendered::default(),
            loops: Vec::new(),
            plumbob: None,
            roof: None,
            offset: 0,
            follow: None,
            manual_scroll: None,
            shown: 0,
            state: State { agents: herdr_panel_protocol::heapless::Vec::new() },
        }
    }

    fn image(
        &self,
        parent: &impl AsLvHandle,
        src: &'static oxivgl::widgets::lv_image_dsc_t,
        x: i32,
        y: i32,
    ) -> Result<Image<'static>, WidgetError> {
        let img = Image::new(parent)?;
        img.set_src(src);
        img.pos(x, y);
        Ok(img)
    }

    fn looping(&self, parent: &impl AsLvHandle, l: &Loop, x: i32, y: i32) -> Result<AnimImg<'static>, WidgetError> {
        let img = AnimImg::new(parent)?;
        img.pos(x, y);
        img.set_src(l.frames).set_duration(l.duration_ms).set_repeat_count(ANIM_REPEAT_INFINITE).start();
        Ok(img)
    }

    /// Rebuilds the tower for `self.state`.
    fn render(&mut self) -> Result<(), WidgetError> {
        let Some(scroller) = &self.scroller else { return Ok(()) };
        self.rendered = Rendered::default();
        let mut r = Rendered::default();
        let storeys: Vec<_> = storeys(&self.state.agents, DESKS_PER_STOREY).collect();

        // Shift everything so the top of the sky above the roof is at 0, and
        // keep the view where it was relative to the street.
        let roof_top = storey_top(storeys.len()) - ROOF_H + 2;
        let offset = (TOP_MARGIN - roof_top).max(0);
        let from_bottom = self.offset - scroller.get_scroll_y();
        let shift = offset - self.offset;
        self.offset = offset;
        let at = |y: i32| y + offset;

        r.images.push(self.image(scroller, street_image(), 0, at(GROUND_Y))?);
        r.images.push(self.image(scroller, lobby_image(), 0, at(storey_top(0)))?);
        let mut focused = 0;
        for (k, storey) in storeys.iter().enumerate() {
            let level = k + 1;
            let top = at(storey_top(level));
            let lit = storey.agents.iter().any(|a| a.status != Status::Idle);
            r.images.push(self.image(scroller, storey_image(storey.workspace, lit), 0, top)?);
            r.storeys.push(storey.agents.iter().map(|a| a.id.clone()).collect());
            if storey.agents.iter().any(|a| a.focused) {
                focused = level;
            }
        }
        for level in 0..=storeys.len() {
            self.number(&mut r, level, at(storey_top(level)))?;
        }
        if let Some(l) = &self.roof {
            r.sprites.push(self.looping(scroller, l, 0, at(roof_top))?);
        }
        for (k, storey) in storeys.iter().enumerate() {
            let top = at(storey_top(k + 1));
            for (agent, slot) in storey.agents.iter().zip(&SLOTS) {
                if let Some((_, l)) = self.loops.iter().find(|(s, _)| *s == agent.status) {
                    r.sprites.push(self.looping(scroller, l, slot.mascot.0, top + slot.mascot.1)?);
                }
                if agent.focused
                    && let Some(l) = &self.plumbob
                {
                    let (fx, fy) = slot.feet;
                    r.sprites.push(self.looping(scroller, l, fx - PLUMBOB_W / 2, top + fy - PLUMBOB_ABOVE)?);
                }
            }
        }
        self.rendered = r;

        // SAFETY: the scroller is a live LVGL object; this runs on the LVGL thread.
        unsafe { oxivgl_sys::lv_obj_update_layout(scroller.handle()) };
        scroller.scroll_to_y((offset - from_bottom).max(0), false);
        if let Some(car) = &self.car {
            car.move_foreground();
            if shift != 0 {
                // SAFETY: the car is a live LVGL object; stop its ride before moving it.
                unsafe { oxivgl_sys::lv_anim_delete(car.handle() as *mut _, Some(anim_set_y)) };
                car.pos(CAR_AT.0, at(storey_top(self.car_level)) + CAR_AT.1);
            }
        }
        self.ride_to(focused);
        self.follow = Some(focused);
        self.show_arrows();
        Ok(())
    }

    /// Jumps the view to `scroll`, kept within the tower, and shows only the
    /// arrows that can still move it.
    fn scroll_to(&self, scroll: i32) {
        let Some(scroller) = &self.scroller else { return };
        scroller.scroll_to_y(scroll.clamp(0, self.offset), false);
        self.show_arrows();
    }

    fn show_arrows(&self) {
        let Some(scroller) = &self.scroller else { return };
        let scroll = scroller.get_scroll_y();
        for (arrow, visible) in [(&self.up, scroll > 0), (&self.down, scroll < self.offset)] {
            if let Some(a) = arrow {
                if visible {
                    a.remove_flag(ObjFlag::HIDDEN);
                } else {
                    a.add_flag(ObjFlag::HIDDEN);
                }
            }
        }
    }

    /// Scrolls one storey up (`-1`) or down (`1`).
    fn step(&mut self, dir: i32) {
        let Some(scroller) = &self.scroller else { return };
        self.scroll_to(scroller.get_scroll_y() + dir * STOREY_H);
        self.manual_scroll = Some(Instant::now());
    }

    /// Puts storey `level`'s number on its shaft, the storey's top being `top`.
    fn number(&self, r: &mut Rendered, level: usize, top: i32) -> Result<(), WidgetError> {
        let Some(scroller) = &self.scroller else { return Ok(()) };
        let mut digits = [0usize; 3];
        let mut len = 0;
        let mut n = level;
        loop {
            digits[len] = n % 10;
            len += 1;
            n /= 10;
            if n == 0 || len == digits.len() {
                break;
            }
        }
        let (x, y) = NUMBER_AT[len - 1];
        for i in 0..len {
            r.images.push(self.image(scroller, digit_image(digits[len - 1 - i]), x + 8 * i as i32, top + y)?);
        }
        Ok(())
    }

    /// Sends the car to storey `level`.
    fn ride_to(&mut self, level: usize) {
        let Some(car) = &self.car else { return };
        if level == self.car_level {
            return;
        }
        let from = car.get_y();
        let to = storey_top(level) + self.offset + CAR_AT.1;
        let mut a = Anim::new();
        a.set_var(car)
            .set_values(from, to)
            .set_duration(CAR_MS_PER_LEVEL * self.car_level.abs_diff(level) as u32)
            .set_path_cb(Some(anim_path_ease_in_out))
            .set_exec_cb(Some(anim_set_y));
        a.start();
        self.car_level = level;
    }

    /// Scrolls to the storey the view should follow, unless the user has
    /// scrolled lately.
    fn follow(&mut self) {
        let Some(level) = self.follow else { return };
        if self.manual_scroll.is_some_and(|t| t.elapsed().as_secs() < MANUAL_SCROLL_HOLD_SECS) {
            return;
        }
        // Whole storeys from the bottom, so the arrows step along the same grid.
        let centre = self.offset + storey_top(level) + STOREY_H / 2;
        let storeys_up = (self.offset - (centre - SCREEN_H / 2) + STOREY_H / 2).div_euclid(STOREY_H);
        self.scroll_to(self.offset - storeys_up * STOREY_H);
        self.follow = None;
    }

    /// The agent under the screen point being pressed, if any.
    fn tapped_agent(&self) -> Option<Id> {
        let scroller = self.scroller.as_ref()?;
        let mut p = oxivgl_sys::lv_point_t { x: 0, y: 0 };
        // SAFETY: called while LVGL handles a press, so there is an active input device.
        unsafe {
            let indev = oxivgl_sys::lv_indev_active();
            if indev.is_null() {
                return None;
            }
            oxivgl_sys::lv_indev_get_point(indev, &mut p);
        }
        let y = p.y + scroller.get_scroll_y() - self.offset;
        let (level, desk) = desk_under(p.x, y)?;
        self.rendered.storeys.get(level.checked_sub(1)?)?.get(desk).cloned()
    }
}

impl View for Panel {
    fn create(&mut self, container: &Obj<'static>) -> Result<(), WidgetError> {
        self.indev = Some(PointerIndev::new(&POINTER)?);
        self.loops = loops().into_iter().collect();
        self.plumbob = Some(plumbob());
        self.roof = Some(roof());
        container.bg_color(0x000000).bg_opa(255);
        self.backdrop = Some(self.image(container, backdrop_image(), 0, 0)?);

        let scroller = Obj::new(container)?;
        scroller.remove_style_all();
        scroller.size(320, SCREEN_H).pos(0, 0);
        scroller.remove_flag(ObjFlag::SCROLLABLE);
        scroller.set_scrollbar_mode(ScrollbarMode::Off);
        register_event_on(self, scroller.handle());
        let car = Image::new(&scroller)?;
        car.set_src(car_image());
        car.pos(CAR_AT.0, storey_top(0) + CAR_AT.1);
        self.car = Some(car);
        self.scroller = Some(scroller);

        for (src, y, slot) in
            [(arrow_up_image(), ARROW_UP_Y, &mut self.up), (arrow_down_image(), ARROW_DOWN_Y, &mut self.down)]
        {
            let arrow = Image::new(container)?;
            arrow.set_src(src);
            arrow.pos(ARROW_X, y);
            arrow.add_flag(ObjFlag::CLICKABLE);
            // SAFETY: the arrow is a live LVGL object.
            unsafe { oxivgl_sys::lv_obj_set_ext_click_area(arrow.handle(), ARROW_REACH) };
            *slot = Some(arrow);
        }
        for h in [self.up.as_ref().map(|a| a.handle()), self.down.as_ref().map(|a| a.handle())].into_iter().flatten() {
            register_event_on(self, h);
        }

        // Until the bridge answers: the lobby under an empty roof.
        self.render()
    }

    fn update(&mut self) -> Result<NavAction, WidgetError> {
        if let Some(version) = link::newer_into(self.shown, &mut self.state) {
            self.shown = version;
            self.render()?;
        }
        self.follow();
        Ok(NavAction::None)
    }

    fn on_event(&mut self, event: &Event) -> NavAction {
        let pressed = |w: &Option<Image<'static>>| {
            w.as_ref().is_some_and(|w| {
                event.matches(w, EventCode::PRESSED) || event.matches(w, EventCode::LONG_PRESSED_REPEAT)
            })
        };
        if pressed(&self.up) {
            self.step(-1);
        } else if pressed(&self.down) {
            self.step(1);
        } else if let Some(scroller) = &self.scroller
            && event.matches(scroller, EventCode::CLICKED)
            && let Some(id) = self.tapped_agent()
        {
            link::send(&ToHost::Focus { agent: id });
        }
        NavAction::None
    }
}
