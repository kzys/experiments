//! One card per herdr workspace, with a button for each of its tabs.
//! Tapping a tab button asks the bridge to focus that tab.

use core::fmt::Write;

use alloc::vec::Vec;
use herdr_panel_protocol::heapless::String;
use herdr_panel_protocol::{Id, State, Status, ToHost};
use oxivgl::{
    enums::{EventCode, ObjFlag},
    event::Event,
    fonts::MONTSERRAT_20,
    indev::PointerIndev,
    layout::FlexFlow,
    style::{Selector, Style},
    view::{NavAction, View, register_event_on},
    widgets::{Align, Button, Label, Obj, RADIUS_MAX, WidgetError},
};

use crate::{link, touch::POINTER};

const CARD_W: i32 = 304;
const HEADER_H: i32 = 36;
const TAB_W: i32 = 68;
const TAB_H: i32 = 40;
const TAB_GAP: i32 = 6;
const TABS_PER_ROW: usize = 4;

fn status_color(status: Status) -> u32 {
    match status {
        Status::Working => 0xffb300,
        Status::Idle => 0x43a047,
        Status::Blocked => 0xe53935,
        Status::Done => 0x1e88e5,
        Status::Unknown => 0x607d8b,
    }
}

/// Black text on the light amber, white on the rest.
fn text_color(status: Status) -> u32 {
    if status == Status::Working { 0x000000 } else { 0xffffff }
}

/// The widgets built for one state. Widgets own their LVGL objects, and a
/// parent deletes its children, so fields are in drop order: children first.
#[derive(Default)]
struct Rendered {
    labels: Vec<Label<'static>>,
    buttons: Vec<(Button<'static>, Id)>,
    dots: Vec<Obj<'static>>,
    cards: Vec<Obj<'static>>,
}

pub struct Panel {
    indev: Option<PointerIndev>,
    title: Option<Label<'static>>,
    status: Option<Label<'static>>,
    list: Option<Obj<'static>>,
    rendered: Rendered,
    focus: Option<Style>,
    shown: u32,
}

impl Panel {
    pub fn new() -> Self {
        Self { indev: None, title: None, status: None, list: None, rendered: Rendered::default(), focus: None, shown: 0 }
    }

    fn render(&mut self, state: &State) -> Result<(), WidgetError> {
        self.rendered = Rendered::default();
        let (Some(list), Some(focus)) = (&self.list, &self.focus) else { return Ok(()) };
        let mut r = Rendered::default();

        for ws in &state.workspaces {
            let rows = ws.tabs.len().div_ceil(TABS_PER_ROW).max(1) as i32;
            let card = Obj::new(list)?;
            card.size(CARD_W, HEADER_H + rows * (TAB_H + TAB_GAP) + 4).bg_color(0x1c2733).bg_opa(255);
            card.remove_flag(ObjFlag::SCROLLABLE);
            if ws.focused {
                card.add_style(focus, Selector::DEFAULT);
            }

            let dot = Obj::new(&card)?;
            dot.size(16, 16).pos(8, 8).radius(RADIUS_MAX, Selector::DEFAULT);
            dot.bg_color(status_color(ws.status)).bg_opa(255).border_width(0);
            dot.remove_flag(ObjFlag::SCROLLABLE);
            r.dots.push(dot);

            let label = Label::new(&card)?;
            label.text(ws.label.as_str()).text_color(0xffffff).pos(32, 8);
            r.labels.push(label);

            for (i, tab) in ws.tabs.iter().enumerate() {
                let x = 8 + (i % TABS_PER_ROW) as i32 * (TAB_W + TAB_GAP);
                let y = HEADER_H + (i / TABS_PER_ROW) as i32 * (TAB_H + TAB_GAP);
                let btn = Button::new(&card)?;
                btn.size(TAB_W, TAB_H).pos(x, y).bg_color(status_color(tab.status));
                btn.bubble_events();
                if tab.focused {
                    btn.add_style(focus, Selector::DEFAULT);
                }
                let text = Label::new(&btn)?;
                text.text(tab.label.as_str()).text_color(text_color(tab.status)).center();
                r.labels.push(text);
                r.buttons.push((btn, tab.id.clone()));
            }
            r.cards.push(card);
        }
        self.rendered = r;
        Ok(())
    }

    fn set_status(&self, text: &str) {
        if let Some(l) = &self.status {
            l.text(text);
        }
    }
}

impl View for Panel {
    fn create(&mut self, container: &Obj<'static>) -> Result<(), WidgetError> {
        self.indev = Some(PointerIndev::new(&POINTER)?);
        container.bg_color(0x0b1117).bg_opa(255);
        self.focus = Some(Style::new(|s| {
            s.border_width(3).border_color_hex(0xffffff);
        }));

        let title = Label::new(container)?;
        title.text("herdr").text_font(MONTSERRAT_20).text_color(0xffffff).align(Align::TopLeft, 12, 10);
        self.title = Some(title);

        let status = Label::new(container)?;
        status.text("waiting for the bridge...").text_color(0x90a4ae).align(Align::TopRight, -12, 14);
        self.status = Some(status);

        let list = Obj::new(container)?;
        list.size(320, 430).pos(0, 50).bg_opa(0).border_width(0);
        list.style_pad_all(8, Selector::DEFAULT);
        list.set_flex_flow(FlexFlow::Column);
        list.style_pad_row(8, Selector::DEFAULT);
        self.list = Some(list);
        Ok(())
    }

    fn update(&mut self) -> Result<NavAction, WidgetError> {
        let Some((version, state)) = link::newer_than(self.shown) else { return Ok(NavAction::None) };
        self.shown = version;
        self.render(&state)?;
        let handles: Vec<_> = self.rendered.buttons.iter().map(|(b, _)| b.handle()).collect();
        for h in handles {
            register_event_on(self, h);
        }
        let mut text = String::<32>::new();
        let _ = write!(text, "{} workspaces", state.workspaces.len());
        self.set_status(&text);
        Ok(NavAction::None)
    }

    fn on_event(&mut self, event: &Event) -> NavAction {
        for (btn, id) in &self.rendered.buttons {
            if event.matches(btn, EventCode::CLICKED) {
                link::send(&ToHost::Focus { tab: id.clone() });
            }
        }
        NavAction::None
    }
}
