//! A grid of mascots, one per herdr agent, each looping the animation for its
//! agent's state: running when it works, asleep when idle, waving with "!"
//! when blocked, hopping when done, "?" when unknown. Tapping one asks the
//! bridge to focus that agent.

use core::fmt::Write;

use alloc::vec::Vec;
use herdr_panel_art::Behavior;
use herdr_panel_protocol::{Id, State, Status, ToHost, heapless::String};
use oxivgl::{
    anim::ANIM_REPEAT_INFINITE,
    enums::EventCode,
    event::Event,
    fonts::MONTSERRAT_20,
    indev::PointerIndev,
    style::{Selector, Style},
    view::{NavAction, View, register_event_on},
    widgets::{Align, AnimImg, AsLvHandle, Button, Label, Obj, Screen, WidgetError},
};

use crate::{
    frames::{Loop, loops},
    link,
    touch::POINTER,
};

const SCREEN_W: i32 = 320;
const COLUMNS: usize = 7;
const CELL_W: i32 = 44;
const CELL_H: i32 = 36;
/// The area below the header, where the grid is centred.
const AREA_TOP: i32 = 50;
const AREA_H: i32 = 430;

fn behavior(status: Status) -> Behavior {
    match status {
        Status::Working => Behavior::Work,
        Status::Idle => Behavior::Sleep,
        Status::Blocked => Behavior::Blocked,
        Status::Done => Behavior::Done,
        Status::Unknown => Behavior::Unknown,
    }
}

/// The widgets built for one state. Widgets own their LVGL objects, and a
/// parent deletes its children, so fields are in drop order: children first.
#[derive(Default)]
struct Rendered {
    mascots: Vec<AnimImg<'static>>,
    cells: Vec<(Button<'static>, Id)>,
}

pub struct Panel {
    indev: Option<PointerIndev>,
    title: Option<Label<'static>>,
    summary: Option<Label<'static>>,
    rendered: Rendered,
    focus: Option<Style>,
    loops: Vec<(Behavior, Loop)>,
    shown: u32,
}

impl Panel {
    pub fn new() -> Self {
        Self {
            indev: None,
            title: None,
            summary: None,
            rendered: Rendered::default(),
            focus: None,
            loops: Vec::new(),
            shown: 0,
        }
    }

    fn render(&mut self, container: &impl AsLvHandle, state: &State) -> Result<(), WidgetError> {
        self.rendered = Rendered::default();
        let Some(focus) = &self.focus else { return Ok(()) };
        let mut r = Rendered::default();

        let n = state.agents.len();
        let rows = n.div_ceil(COLUMNS) as i32;
        let top = AREA_TOP + (AREA_H - rows * CELL_H) / 2;
        for (i, agent) in state.agents.iter().enumerate() {
            // Rows are centred on their own, so a short last row sits in the middle.
            let row = i / COLUMNS;
            let in_row = COLUMNS.min(n - row * COLUMNS) as i32;
            let x = (SCREEN_W - in_row * CELL_W) / 2 + (i % COLUMNS) as i32 * CELL_W;
            let y = top + row as i32 * CELL_H;

            let cell = Button::new(container)?;
            cell.size(CELL_W - 4, CELL_H - 4).pos(x, y).bg_opa(0).border_width(0);
            cell.bubble_events();
            if agent.focused {
                cell.add_style(focus, Selector::DEFAULT);
            }

            let mascot = AnimImg::new(&cell)?;
            mascot.align(Align::Center, 0, 0);
            if let Some((_, l)) = self.loops.iter().find(|(b, _)| *b == behavior(agent.status)) {
                mascot.set_src(l.frames).set_duration(l.duration_ms).set_repeat_count(ANIM_REPEAT_INFINITE).start();
            }
            r.mascots.push(mascot);
            r.cells.push((cell, agent.id.clone()));
        }
        self.rendered = r;
        Ok(())
    }

    fn set_summary(&self, state: &State) {
        let working = state.agents.iter().filter(|a| a.status == Status::Working).count();
        let mut text = String::<40>::new();
        if state.agents.is_empty() {
            let _ = write!(text, "no agents");
        } else {
            let _ = write!(text, "{} agents, {} working", state.agents.len(), working);
        }
        if let Some(l) = &self.summary {
            l.text(&text);
        }
    }
}

impl View for Panel {
    fn create(&mut self, container: &Obj<'static>) -> Result<(), WidgetError> {
        self.indev = Some(PointerIndev::new(&POINTER)?);
        self.loops = loops().into_iter().collect();
        container.bg_color(0x0b1117).bg_opa(255);
        self.focus = Some(Style::new(|s| {
            s.border_width(2).border_color_hex(0xffffff).radius(10);
        }));

        let title = Label::new(container)?;
        title.text("herdr").text_font(MONTSERRAT_20).text_color(0xffffff).align(Align::TopLeft, 12, 10);
        self.title = Some(title);

        let summary = Label::new(container)?;
        summary.text("waiting for the bridge...").text_color(0x90a4ae).align(Align::TopRight, -12, 14);
        self.summary = Some(summary);
        Ok(())
    }

    fn update(&mut self) -> Result<NavAction, WidgetError> {
        let Some((version, state)) = link::newer_than(self.shown) else { return Ok(NavAction::None) };
        self.shown = version;
        let Some(screen) = Screen::active() else { return Ok(NavAction::None) };
        self.render(&screen, &state)?;
        let handles: Vec<_> = self.rendered.cells.iter().map(|(b, _)| b.handle()).collect();
        for h in handles {
            register_event_on(self, h);
        }
        self.set_summary(&state);
        Ok(NavAction::None)
    }

    fn on_event(&mut self, event: &Event) -> NavAction {
        for (cell, id) in &self.rendered.cells {
            if event.matches(cell, EventCode::CLICKED) {
                link::send(&ToHost::Focus { agent: id.clone() });
            }
        }
        NavAction::None
    }
}
