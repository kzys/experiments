//! A tower seen from the side, storeys of desks stacked on a lobby, each
//! storey two spaces of four desks: each workspace takes a space, or several
//! in a row if its agents do not fit one, filling storeys from the left.
//! Each agent is a mascot behind its desk, looping the animation for its
//! state: running when it works, dozing with "z"s when idle, waving with "!"
//! when blocked, hopping when done, "?" when unknown. A space's lights are
//! off, and it is dim, when all its agents doze. The elevator rides to the focused agent's
//! storey and a plumbob floats over that agent. Tapping an agent asks the
//! bridge to focus it.
//!
//! The sky shows the time of day by the bridge's clock, night until it
//! sends one, with clouds by day that drift a pixel an hour. It stays still;
//! when the tower is taller than the screen, arrow
//! buttons over the elevator shaft scroll it a storey at a time (a jump, not a
//! slide: the panel is too slow to redraw a whole screen smoothly). The view
//! follows the focused agent, unless an arrow was pressed in the last
//! [`MANUAL_SCROLL_HOLD_SECS`] seconds.

use alloc::{string::String, vec::Vec};
use embassy_time::Instant;
use herdr_panel_protocol::{
    Attention, Id, State, Status, TimeOfDay, ToHost, VERSION_LEN, cloud_x, heapless, spaces,
};
use oxivgl::{
    anim::{ANIM_REPEAT_INFINITE, Anim, anim_path_ease_in_out, anim_set_y},
    enums::{EventCode, ObjFlag, ScrollbarMode},
    event::Event,
    indev::PointerIndev,
    style::{Selector, Style},
    view::{NavAction, View, register_event_on},
    widgets::{AnimImg, AsLvHandle, Image, Label, LabelLongMode, Obj, WidgetError},
};
use static_cell::ConstStaticCell;

use crate::{
    frames::{
        ARROW_SIZE, ARROW_X, CAR_AT, CLOUDS, DECOR_AT, DECORS, DESK_SPACING, DESKS_PER_SPACE,
        DESKS_PER_STOREY, DESKS_X, DIGIT_W, FONT_H, FONT_W, GROUND_Y, LABEL_COLOUR, LABEL_ENDS,
        LABEL_FONT, LABEL_OUTLINE, LABEL_OUTLINE_FONT, LABEL_Y, Loop, NUMBER_AT, PLUMBOB_ABOVE,
        PLUMBOB_W, ROOF_H, SIGN_BOARD, SIGN_CENTRE, SIGN_H, SIGN_HALO, SIGN_HALO_OPA, SIGN_INK,
        SIGN_MARGIN, SIGN_NEON, SIGN_TOP, SLOTS, SPACE_X, SPACES_PER_STOREY, STOREY_H, TICKER,
        TICKER_LED, arrow_down_image, arrow_up_image, awake, backdrop_image, car_image,
        cloud_image, decor_image, digit_image, lobby_image, loops, plumbob, roof, space_image,
        street_image,
    },
    link,
    touch::POINTER,
};

const SCREEN_W: i32 = 320;
const SCREEN_H: i32 = 480;
/// Sky kept above the roof when scrolled to the top, in screen pixels.
const TOP_MARGIN: i32 = 48;
/// How long the elevator takes per storey, in milliseconds.
const CAR_MS_PER_LEVEL: u32 = 250;
/// After a scroll by hand, how long the view stops following the focus.
const MANUAL_SCROLL_HOLD_SECS: u64 = 10;
/// Where the scroll buttons are, and how far their touch area reaches past
/// their edges, in screen pixels.
const ARROW_UP_Y: i32 = 8;
const ARROW_DOWN_Y: i32 = SCREEN_H - ARROW_SIZE - 8;
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

/// A list as long as the tower, in PSRAM.
type UiVec<T> = Vec<T, &'static esp_alloc::EspHeap>;

/// An empty [`UiVec`].
fn ui_vec<T>() -> UiVec<T> {
    Vec::new_in(&crate::UI_HEAP)
}

/// The widgets built for one state; each is a child of the scroller.
struct Rendered {
    images: UiVec<Image<'static>>,
    sprites: UiVec<AnimImg<'static>>,
    labels: UiVec<Label<'static>>,
    /// The neon sign's halo, legs and board.
    boxes: UiVec<Obj<'static>>,
    /// The agents on each storey above the lobby, by desk, as indices into
    /// the state's agents; `None` at a free desk.
    storeys: UiVec<[Option<u16>; DESKS_PER_STOREY]>,
}

impl Rendered {
    fn new() -> Self {
        Self {
            images: ui_vec(),
            sprites: ui_vec(),
            labels: ui_vec(),
            boxes: ui_vec(),
            storeys: ui_vec(),
        }
    }
}

pub struct Panel {
    indev: Option<PointerIndev>,
    backdrop: Option<Image<'static>>,
    clouds: Vec<Image<'static>>,
    /// What the sky shows, and the hours since the Unix epoch that the
    /// clouds have drifted for.
    time: TimeOfDay,
    hour: u32,
    scroller: Option<Obj<'static>>,
    up: Option<Image<'static>>,
    down: Option<Image<'static>>,
    car: Option<Image<'static>>,
    /// The level the car is at or heading for.
    car_level: usize,
    rendered: Rendered,
    loops: Vec<(Status, Loop)>,
    plumbob: Option<Loop>,
    awake: Option<Loop>,
    roof: Option<Loop>,
    /// The styles of a workspace label's text and of its outline.
    label_style: Style,
    outline_style: Style,
    /// The styles of the neon sign: its halo, board, legs and text.
    halo_style: Style,
    board_style: Style,
    ink_style: Style,
    neon_style: Style,
    led_style: Style,
    /// The agents that need you, for the lobby's ticker, which names them
    /// and focuses the first when tapped. It lives across renders so that
    /// its scrolling carries on.
    attention: Attention,
    ticker: Option<Label<'static>>,
    /// herdr's version, for the sign, once the bridge has sent it.
    version: Option<heapless::String<VERSION_LEN>>,
    /// How far the content is shifted down so that it starts at 0: what
    /// "the view at the bottom" adds to every y.
    offset: i32,
    /// The level the view should show, once it may follow again.
    follow: Option<usize>,
    manual_scroll: Option<Instant>,
    shown: u32,
    /// The latest state from the bridge.
    state: &'static mut State,
}

/// The latest state from the bridge, for the [`Panel`]: a static, built in
/// place, because a state is too big to build on the render thread's stack
/// and move.
static STATE: ConstStaticCell<State> = ConstStaticCell::new(State {
    agents: herdr_panel_protocol::heapless::Vec::new(),
});

impl Panel {
    pub fn new() -> Self {
        Self {
            indev: None,
            backdrop: None,
            clouds: Vec::new(),
            time: TimeOfDay::Night,
            hour: 0,
            scroller: None,
            up: None,
            down: None,
            car: None,
            car_level: 0,
            rendered: Rendered::new(),
            loops: Vec::new(),
            plumbob: None,
            awake: None,
            roof: None,
            label_style: Style::new(|s| {
                s.text_font(LABEL_FONT).text_color_hex(LABEL_COLOUR);
            }),
            outline_style: Style::new(|s| {
                s.text_font(LABEL_OUTLINE_FONT)
                    .text_color_hex(LABEL_OUTLINE);
            }),
            halo_style: Style::new(|s| {
                s.bg_color_hex(SIGN_NEON).bg_opa(SIGN_HALO_OPA);
            }),
            board_style: Style::new(|s| {
                s.bg_color_hex(SIGN_BOARD)
                    .bg_opa(255)
                    .border_color_hex(SIGN_INK)
                    .border_width(1);
            }),
            ink_style: Style::new(|s| {
                s.bg_color_hex(SIGN_INK).bg_opa(255);
            }),
            neon_style: Style::new(|s| {
                s.text_font(LABEL_FONT).text_color_hex(SIGN_NEON);
            }),
            led_style: Style::new(|s| {
                s.text_font(LABEL_FONT).text_color_hex(TICKER_LED);
            }),
            attention: Attention::new(),
            ticker: None,
            version: None,
            offset: 0,
            follow: None,
            manual_scroll: None,
            shown: 0,
            state: STATE.take(),
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

    fn looping(
        &self,
        parent: &impl AsLvHandle,
        l: &Loop,
        x: i32,
        y: i32,
    ) -> Result<AnimImg<'static>, WidgetError> {
        let img = AnimImg::new(parent)?;
        img.pos(x, y);
        img.set_src(l.frames)
            .set_duration(l.duration_ms)
            .set_repeat_count(ANIM_REPEAT_INFINITE)
            .start();
        Ok(img)
    }

    /// Rebuilds the tower for `self.state`.
    fn render(&mut self) -> Result<(), WidgetError> {
        let Some(scroller) = &self.scroller else {
            return Ok(());
        };
        self.rendered = Rendered::new();
        let mut r = Rendered::new();
        let spaces = {
            let mut v = ui_vec();
            v.extend(spaces(&self.state.agents, DESKS_PER_SPACE));
            v
        };
        let storeys = spaces.len().div_ceil(SPACES_PER_STOREY);

        // Shift everything so the top of the sky above the roof is at 0, and
        // keep the view where it was relative to the street.
        let roof_top = storey_top(storeys) - ROOF_H + 2;
        let offset = (TOP_MARGIN - roof_top).max(0);
        let from_bottom = self.offset - scroller.get_scroll_y();
        let shift = offset - self.offset;
        self.offset = offset;
        let at = |y: i32| y + offset;

        r.images
            .push(self.image(scroller, street_image(), 0, at(GROUND_Y))?);
        r.images
            .push(self.image(scroller, lobby_image(), 0, at(storey_top(0)))?);
        let mut focused = 0;
        // Spaces take the agents in order, so this counts their index.
        let mut index = 0u16;
        for (k, pair) in spaces.chunks(SPACES_PER_STOREY).enumerate() {
            let level = k + 1;
            let top = at(storey_top(level));
            let mut desks = [None; DESKS_PER_STOREY];
            let mut right_lit = false;
            for side in 0..SPACES_PER_STOREY {
                // An empty right space has the left one's wallpaper, and is dark.
                let space = pair.get(side);
                let paper = space.map_or(pair[0].workspace, |s| s.workspace);
                let lit = space.is_some_and(|s| s.lit());
                right_lit = lit;
                r.images.push(self.image(
                    scroller,
                    space_image(paper, lit, self.time, side),
                    SPACE_X[side],
                    top,
                )?);
                let Some(space) = space else { continue };
                self.label(&mut r, space.label(), side, top)?;
                for (i, a) in space.agents.iter().enumerate() {
                    desks[side * DESKS_PER_SPACE + i] = Some(index);
                    index += 1;
                    if a.focused {
                        focused = level;
                    }
                }
            }
            r.images.push(self.image(
                scroller,
                decor_image(level % DECORS, right_lit, self.time),
                DECOR_AT.0,
                top + DECOR_AT.1,
            )?);
            r.storeys.push(desks);
        }
        for level in 0..=storeys {
            self.number(&mut r, level, at(storey_top(level)))?;
        }
        if let Some(l) = &self.roof {
            r.sprites.push(self.looping(scroller, l, 0, at(roof_top))?);
        }
        self.sign(&mut r, at(roof_top))?;
        self.show_ticker(at(storey_top(0)));
        for (k, space) in spaces.iter().enumerate() {
            let top = at(storey_top(k / SPACES_PER_STOREY + 1));
            let first = k % SPACES_PER_STOREY * DESKS_PER_SPACE;
            for (agent, slot) in space.agents.iter().zip(&SLOTS[first..]) {
                // An idle agent with the focus is awake rather than dozing.
                let look = if agent.focused && agent.status == Status::Idle {
                    self.awake.as_ref()
                } else {
                    self.loops
                        .iter()
                        .find(|(s, _)| *s == agent.status)
                        .map(|(_, l)| l)
                };
                if let Some(l) = look {
                    r.sprites.push(self.looping(
                        scroller,
                        l,
                        slot.mascot.0,
                        top + slot.mascot.1,
                    )?);
                }
                if agent.focused
                    && let Some(l) = &self.plumbob
                {
                    let (fx, fy) = slot.feet;
                    r.sprites.push(self.looping(
                        scroller,
                        l,
                        fx - PLUMBOB_W / 2,
                        top + fy - PLUMBOB_ABOVE,
                    )?);
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

    /// Shows the sky for `self.time` and its clouds where `self.hour` puts
    /// them.
    fn show_sky(&self) {
        if let Some(b) = &self.backdrop {
            b.set_src(backdrop_image(self.time));
        }
        for (k, cloud) in self.clouds.iter().enumerate() {
            match cloud_image(self.time, k) {
                Some(src) => {
                    let (x, y, w) = CLOUDS[k];
                    cloud.set_src(src);
                    cloud.pos(cloud_x(x, w, SCREEN_W, self.hour), y);
                    cloud.remove_flag(ObjFlag::HIDDEN);
                }
                None => {
                    cloud.add_flag(ObjFlag::HIDDEN);
                }
            }
        }
    }

    /// Jumps the view to `scroll`, kept within the tower, and shows only the
    /// arrows that can still move it.
    fn scroll_to(&self, scroll: i32) {
        let Some(scroller) = &self.scroller else {
            return;
        };
        scroller.scroll_to_y(scroll.clamp(0, self.offset), false);
        self.show_arrows();
    }

    fn show_arrows(&self) {
        let Some(scroller) = &self.scroller else {
            return;
        };
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
        let Some(scroller) = &self.scroller else {
            return;
        };
        self.scroll_to(scroller.get_scroll_y() + dir * STOREY_H);
        self.manual_scroll = Some(Instant::now());
    }

    /// Puts storey `level`'s number on its shaft, the storey's top being `top`.
    fn number(&self, r: &mut Rendered, level: usize, top: i32) -> Result<(), WidgetError> {
        let Some(scroller) = &self.scroller else {
            return Ok(());
        };
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
            r.images.push(self.image(
                scroller,
                digit_image(digits[len - 1 - i]),
                x + DIGIT_W * i as i32,
                top + y,
            )?);
        }
        Ok(())
    }

    /// Puts a workspace's label `text`, outlined, at the top right of space
    /// `side` of the storey whose top is `top`. Characters outside ASCII show
    /// as '?'.
    fn label(
        &self,
        r: &mut Rendered,
        text: &str,
        side: usize,
        top: i32,
    ) -> Result<(), WidgetError> {
        let Some(scroller) = &self.scroller else {
            return Ok(());
        };
        if text.is_empty() {
            return Ok(());
        }
        let text: String = text
            .chars()
            .map(|c| if (' '..='~').contains(&c) { c } else { '?' })
            .collect();
        let (x, y) = (LABEL_ENDS[side] - FONT_W * text.len() as i32, top + LABEL_Y);
        let outline = Label::new(scroller)?;
        outline.add_style(&self.outline_style, Selector::DEFAULT);
        outline.text(&text);
        // Wider than the characters' advances, so the last one's outline is not cut.
        outline.size(FONT_W * text.len() as i32 + 2, FONT_H + 2);
        outline.pos(x - 1, y - 1);
        r.labels.push(outline);
        let label = Label::new(scroller)?;
        label.add_style(&self.label_style, Selector::DEFAULT);
        label.text(&text);
        label.pos(x, y);
        r.labels.push(label);
        Ok(())
    }

    /// Puts the neon sign on the roof whose top is `top`: "HERDR" and herdr's
    /// version once known, on a board as wide as the text.
    fn sign(&self, r: &mut Rendered, top: i32) -> Result<(), WidgetError> {
        let Some(scroller) = &self.scroller else {
            return Ok(());
        };
        let mut text = String::from("HERDR");
        if let Some(v) = &self.version {
            text.push(' ');
            text.extend(
                v.chars()
                    .map(|c| if (' '..='~').contains(&c) { c } else { '?' }),
            );
        }
        let w = FONT_W * text.len() as i32 + SIGN_MARGIN;
        let (x, y) = (SIGN_CENTRE - w / 2, top + SIGN_TOP);
        let (side, up, down) = SIGN_HALO;
        let mut boxed =
            |style: &Style, x: i32, y: i32, w: i32, h: i32| -> Result<(), WidgetError> {
                let b = Obj::new(scroller)?;
                b.remove_style_all();
                b.add_style(style, Selector::DEFAULT);
                b.remove_flag(ObjFlag::CLICKABLE);
                b.size(w, h).pos(x, y);
                r.boxes.push(b);
                Ok(())
            };
        boxed(
            &self.halo_style,
            x - side,
            y - up,
            w + 2 * side,
            SIGN_H + up + down,
        )?;
        boxed(&self.ink_style, x + 6, y + SIGN_H, 1, 1)?;
        boxed(&self.ink_style, x + w - 7, y + SIGN_H, 1, 1)?;
        boxed(&self.board_style, x, y, w, SIGN_H)?;
        let label = Label::new(scroller)?;
        label.add_style(&self.neon_style, Selector::DEFAULT);
        label.text(&text);
        label.pos(x + 3, y + 1);
        r.labels.push(label);
        Ok(())
    }

    /// Shows the ticker's message on the lobby whose top is `top`, in front
    /// of it, scrolling round when it is longer than the window. Characters
    /// outside ASCII show as '?'.
    fn show_ticker(&self, top: i32) {
        let Some(label) = &self.ticker else { return };
        let mut message = String::new();
        // Writing to a String does not fail.
        let _ = self.attention.write(&self.state.agents, &mut message);
        let message: String = message
            .chars()
            .map(|c| if (' '..='~').contains(&c) { c } else { '?' })
            .collect();
        let (x, y, _, _) = TICKER;
        // The same text again leaves its scrolling alone.
        label.text(&message);
        label.pos(x + 2, top + y + 1);
        label.move_foreground();
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
        if self
            .manual_scroll
            .is_some_and(|t| t.elapsed().as_secs() < MANUAL_SCROLL_HOLD_SECS)
        {
            return;
        }
        // Whole storeys from the bottom, so the arrows step along the same grid.
        let centre = self.offset + storey_top(level) + STOREY_H / 2;
        let storeys_up =
            (self.offset - (centre - SCREEN_H / 2) + STOREY_H / 2).div_euclid(STOREY_H);
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
        let i = (*self
            .rendered
            .storeys
            .get(level.checked_sub(1)?)?
            .get(desk)?)?;
        self.state.agents.get(i as usize).map(|a| a.id.clone())
    }
}

impl View for Panel {
    fn create(&mut self, container: &Obj<'static>) -> Result<(), WidgetError> {
        self.indev = Some(PointerIndev::new(&POINTER)?);
        self.loops = loops().into_iter().collect();
        self.plumbob = Some(plumbob());
        self.awake = Some(awake());
        self.roof = Some(roof());
        container.bg_color(0x000000).bg_opa(255);
        self.backdrop = Some(self.image(container, backdrop_image(self.time), 0, 0)?);
        for _ in CLOUDS {
            let cloud = Image::new(container)?;
            cloud.add_flag(ObjFlag::HIDDEN);
            self.clouds.push(cloud);
        }

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
        if let Some(scroller) = &self.scroller {
            let (_, _, w, h) = TICKER;
            let label = Label::new(scroller)?;
            label.add_style(&self.led_style, Selector::DEFAULT);
            label.set_long_mode(LabelLongMode::ScrollCircular);
            label.size(w - 3, h - 2);
            label.add_flag(ObjFlag::CLICKABLE);
            let handle = label.handle();
            self.ticker = Some(label);
            register_event_on(self, handle);
        }

        for (src, y, slot) in [
            (arrow_up_image(), ARROW_UP_Y, &mut self.up),
            (arrow_down_image(), ARROW_DOWN_Y, &mut self.down),
        ] {
            let arrow = Image::new(container)?;
            arrow.set_src(src);
            arrow.pos(ARROW_X, y);
            arrow.add_flag(ObjFlag::CLICKABLE);
            // SAFETY: the arrow is a live LVGL object.
            unsafe { oxivgl_sys::lv_obj_set_ext_click_area(arrow.handle(), ARROW_REACH) };
            *slot = Some(arrow);
        }
        for h in [
            self.up.as_ref().map(|a| a.handle()),
            self.down.as_ref().map(|a| a.handle()),
        ]
        .into_iter()
        .flatten()
        {
            register_event_on(self, h);
        }

        // Until the bridge answers: the lobby under an empty roof.
        self.render()
    }

    fn update(&mut self) -> Result<NavAction, WidgetError> {
        let mut redraw = false;
        if let Some((minute, hour)) = link::clock() {
            let time = TimeOfDay::at(minute);
            if time != self.time || hour != self.hour {
                redraw = time != self.time;
                self.time = time;
                self.hour = hour;
                self.show_sky();
            }
        }
        let version = link::herdr_version();
        if version != self.version {
            self.version = version;
            redraw = true;
        }
        if let Some(version) = link::newer_into(self.shown, self.state) {
            self.shown = version;
            self.attention.update(&self.state.agents);
            redraw = true;
        }
        if redraw {
            self.render()?;
        }
        self.follow();
        Ok(NavAction::None)
    }

    fn on_event(&mut self, event: &Event) -> NavAction {
        let pressed = |w: &Option<Image<'static>>| {
            w.as_ref().is_some_and(|w| {
                event.matches(w, EventCode::PRESSED)
                    || event.matches(w, EventCode::LONG_PRESSED_REPEAT)
            })
        };
        if pressed(&self.up) {
            self.step(-1);
        } else if pressed(&self.down) {
            self.step(1);
        } else if let Some(t) = &self.ticker
            && event.matches(t, EventCode::CLICKED)
        {
            if let Some(id) = self.attention.ids().next() {
                link::send(&ToHost::Focus { agent: id.clone() });
            }
        } else if let Some(scroller) = &self.scroller
            && event.matches(scroller, EventCode::CLICKED)
            && let Some(id) = self.tapped_agent()
        {
            link::send(&ToHost::Focus { agent: id });
        }
        NavAction::None
    }
}
