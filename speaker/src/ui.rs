//! The four tabs: LED buttons, microphone meter, speaker tones and wake
//! word.

use core::{fmt::Write, sync::atomic::Ordering};

use alloc::vec::Vec;
use embassy_time::Instant;
use heapless::String;
use oxivgl::{
    enums::{EventCode, ObjFlag, ObjState},
    event::Event,
    fonts::MONTSERRAT_20,
    indev::PointerIndev,
    layout::{FlexAlign, FlexFlow},
    style::{Selector, color_make},
    view::{NavAction, View, register_event_on},
    widgets::{
        Align, Bar, Button, Chart, ChartAxis, ChartSeries, ChartType, Label, Obj, Slider, Tabview, WidgetError,
    },
};

use crate::{
    audio::{self, Note},
    led,
    touch::POINTER,
    wakeword::{self, NO_MODEL},
};

/// Colour buttons: label, colour, text colour. "Off" is the black one.
const COLORS: [(&str, u32, u32); 7] = [
    ("Red", 0xff0000, 0xffffff),
    ("Green", 0x00ff00, 0x000000),
    ("Blue", 0x0000ff, 0xffffff),
    ("Yellow", 0xffff00, 0x000000),
    ("Purple", 0xa020f0, 0xffffff),
    ("White", 0xffffff, 0x000000),
    ("Off", 0x000000, 0xffffff),
];

const SPEAKER_BUTTONS: [(&str, u32, u32); 4] = [
    ("Beep", 0x2196f3, 0xffffff),
    ("Scale", 0x4caf50, 0xffffff),
    ("Siren", 0xff9800, 0x000000),
    ("Stop", 0x606060, 0xffffff),
];

/// Widgets own their LVGL objects (dropping one deletes it), so everything
/// built in `create` is kept here.
#[derive(Default)]
pub struct App {
    indev: Option<PointerIndev>,
    tabview: Option<Tabview<'static>>,
    objs: Vec<Obj<'static>>,
    labels: Vec<Label<'static>>,
    color_btns: Vec<Button<'static>>,
    speaker_btns: Vec<Button<'static>>,
    rainbow_btn: Option<Button<'static>>,
    led_slider: Option<Slider<'static>>,
    volume_slider: Option<Slider<'static>>,
    status_label: Option<Label<'static>>,
    mic_label: Option<Label<'static>>,
    mic_bar: Option<Bar<'static>>,
    mic_chart: Option<Chart<'static>>,
    mic_series: Option<ChartSeries>,
    wake_btns: Vec<Button<'static>>,
    wake_status: Option<Label<'static>>,
    wake_count: Option<Label<'static>>,
    /// What the wake status label shows, to update it only on change.
    wake_shown: (u8, u8, bool),
    wake_heard: u32,
    /// The LED shows green for a wake word until then.
    wake_flash_until: Option<Instant>,
    led_rgb: u32,
    led_level: u8,
    hue: u16,
    last_rainbow: Option<Instant>,
    last_mic: Option<Instant>,
}

/// Converts a hue (0..360) at full saturation and value to 0xRRGGBB.
fn hue_to_rgb(hue: u16) -> u32 {
    let h = u32::from(hue % 360);
    let sector = h / 60;
    let f = (h % 60) * 255 / 60;
    let (r, g, b) = match sector {
        0 => (255, f, 0),
        1 => (255 - f, 255, 0),
        2 => (0, 255, f),
        3 => (0, 255 - f, 255),
        4 => (f, 0, 255),
        _ => (255, 0, 255 - f),
    };
    r << 16 | g << 8 | b
}

impl App {
    pub fn new() -> Self {
        Self { led_level: 128, wake_shown: (NO_MODEL, NO_MODEL, false), ..Default::default() }
    }

    /// Sends the current colour, scaled by brightness, to the LED, unless a
    /// wake word flash holds it.
    fn apply_led(&self) {
        if self.wake_flash_until.is_some() {
            return;
        }
        let level = u32::from(self.led_level);
        let scale = |c: u32| (c & 0xff) * level / 255;
        led::COLOR.signal((
            scale(self.led_rgb >> 16) as u8,
            scale(self.led_rgb >> 8) as u8,
            scale(self.led_rgb) as u8,
        ));
        if let Some(l) = &self.status_label {
            let mut s = String::<48>::new();
            let _ = write!(s, "LED #{:06X}  brightness {}%", self.led_rgb, self.led_level as u32 * 100 / 255);
            l.text(&s);
        }
    }

    fn rainbow_on(&self) -> bool {
        self.rainbow_btn.as_ref().is_some_and(|b| b.has_state(ObjState::CHECKED))
    }

    /// A transparent wrapping row of buttons inside `tab`.
    fn button_grid(&mut self, tab: &Obj<'static>, height: i32) -> Result<Obj<'static>, WidgetError> {
        let grid = Obj::new(tab)?;
        grid.size(308, height).pos(0, 0);
        grid.bg_opa(0).border_width(0).style_pad_all(0, Selector::DEFAULT);
        grid.set_flex_flow(FlexFlow::RowWrap);
        grid.set_flex_align(FlexAlign::SpaceEvenly, FlexAlign::Start, FlexAlign::Start);
        grid.remove_flag(ObjFlag::SCROLLABLE);
        Ok(grid)
    }

    fn button(&mut self, parent: &Obj<'static>, text: &str, bg: u32, fg: u32) -> Result<Button<'static>, WidgetError> {
        let btn = Button::new(parent)?;
        btn.size(92, 64).bg_color(bg);
        btn.bubble_events();
        let label = Label::new(&btn)?;
        label.text(text).text_color(fg).center();
        self.labels.push(label);
        Ok(btn)
    }

    fn build_led_tab(&mut self, tab: &Obj<'static>) -> Result<(), WidgetError> {
        let grid = self.button_grid(tab, 250)?;
        for (name, rgb, fg) in COLORS {
            let btn = self.button(&grid, name, if rgb == 0 { 0x404040 } else { rgb }, fg)?;
            self.color_btns.push(btn);
        }
        let rainbow = self.button(&grid, "Rainbow", 0xe91e63, 0xffffff)?;
        rainbow.add_flag(ObjFlag::CHECKABLE);
        self.rainbow_btn = Some(rainbow);
        self.objs.push(grid);

        let slider = Slider::new(tab)?;
        slider.width(260).align(Align::TopMid, 0, 275);
        slider.set_range(0, 255);
        slider.set_value(i32::from(self.led_level));
        slider.bubble_events();
        self.led_slider = Some(slider);

        let status = Label::new(tab)?;
        status.text_color(0xffffff).align(Align::TopMid, 0, 310);
        self.status_label = Some(status);
        self.apply_led();
        Ok(())
    }

    fn build_mic_tab(&mut self, tab: &Obj<'static>) -> Result<(), WidgetError> {
        let label = Label::new(tab)?;
        label.text_font(MONTSERRAT_20).text_color(0xffffff).align(Align::TopMid, 0, 10);
        self.mic_label = Some(label);

        let bar = Bar::new(tab)?;
        bar.size(270, 40).align(Align::TopMid, 0, 50);
        bar.set_range_raw(0, 100);
        self.mic_bar = Some(bar);

        let chart = Chart::new(tab)?;
        chart.size(280, 200).align(Align::TopMid, 0, 115);
        chart.set_type(ChartType::Line);
        chart.set_point_count(100);
        chart.set_axis_range(ChartAxis::PrimaryY, 0, 100);
        let series = chart.add_series(color_make(0xff, 0xcc, 0x00), ChartAxis::PrimaryY);
        for _ in 0..100 {
            chart.set_next_value(&series, 0);
        }
        self.mic_series = Some(series);
        self.mic_chart = Some(chart);
        Ok(())
    }

    fn build_speaker_tab(&mut self, tab: &Obj<'static>) -> Result<(), WidgetError> {
        let grid = self.button_grid(tab, 170)?;
        for (name, bg, fg) in SPEAKER_BUTTONS {
            let btn = self.button(&grid, name, bg, fg)?;
            self.speaker_btns.push(btn);
        }
        self.objs.push(grid);

        let label = Label::new(tab)?;
        label.text("Volume").text_color(0xffffff).align(Align::TopMid, 0, 200);
        self.labels.push(label);

        let slider = Slider::new(tab)?;
        slider.width(260).align(Align::TopMid, 0, 235);
        slider.set_range(0, 100);
        slider.set_value(i32::from(audio::VOLUME.load(Ordering::Relaxed)));
        slider.bubble_events();
        self.volume_slider = Some(slider);
        Ok(())
    }

    fn build_wake_tab(&mut self, tab: &Obj<'static>) -> Result<(), WidgetError> {
        let status = Label::new(tab)?;
        status.text_font(MONTSERRAT_20).text_color(0xffffff).align(Align::TopMid, 0, 10);
        self.wake_status = Some(status);

        let count = Label::new(tab)?;
        count.text("Detections: 0").text_color(0xffffff).align(Align::TopMid, 0, 50);
        self.wake_count = Some(count);

        let title = Label::new(tab)?;
        title.text("Wake word").text_color(0xffffff).align(Align::TopMid, 0, 95);
        self.labels.push(title);

        let grid = self.button_grid(tab, 250)?;
        grid.pos(0, 120);
        let wanted = wakeword::WANTED.load(Ordering::Relaxed);
        for (i, m) in wakeword::MODELS.try_get().map(Vec::as_slice).unwrap_or_default().iter().enumerate() {
            let btn = self.button(&grid, &m.word, 0x455a64, 0xffffff)?;
            btn.add_flag(ObjFlag::CHECKABLE);
            btn.style_bg_color(color_make(0x2e, 0x7d, 0x32), ObjState::CHECKED);
            if i == usize::from(wanted) {
                btn.add_state(ObjState::CHECKED);
            }
            self.wake_btns.push(btn);
        }
        self.objs.push(grid);
        Ok(())
    }

    /// Shows what the detector is doing and flashes the LED green for a
    /// second after a wake word.
    fn update_wake(&mut self) {
        let heard = wakeword::COUNT.load(Ordering::Relaxed);
        if heard != self.wake_heard {
            self.wake_heard = heard;
            led::COLOR.signal((0, 255, 0));
            self.wake_flash_until = Some(Instant::now() + embassy_time::Duration::from_secs(1));
            if let (Some(status), Some(count)) = (&self.wake_status, &self.wake_count) {
                status.text("Heard it!");
                let mut s = String::<32>::new();
                let _ = write!(s, "Detections: {heard}");
                count.text(&s);
            }
            return;
        }
        if self.wake_flash_until.is_some_and(|t| Instant::now() >= t) {
            self.wake_flash_until = None;
            self.wake_shown = (NO_MODEL, NO_MODEL, false);
            self.apply_led();
        }
        if self.wake_flash_until.is_some() {
            return;
        }
        let state = (
            wakeword::WANTED.load(Ordering::Relaxed),
            wakeword::LOADED.load(Ordering::Relaxed),
            wakeword::RUNNING.load(Ordering::Relaxed),
        );
        if state == self.wake_shown {
            return;
        }
        self.wake_shown = state;
        let Some(status) = &self.wake_status else { return };
        let (wanted, loaded, running) = state;
        let models = wakeword::MODELS.try_get().map(Vec::as_slice).unwrap_or_default();
        let mut s = String::<48>::new();
        match models.get(usize::from(wanted)) {
            _ if wanted != loaded => s.push_str("Loading...").unwrap(),
            Some(m) if running => {
                let _ = write!(s, "Listening for \"{}\"", m.word);
            }
            _ => s.push_str("Could not load that word").unwrap(),
        }
        status.text(&s);
    }

    fn play_speaker(&self, index: usize) {
        match index {
            0 => audio::play(&[Note { hz: 880, ms: 150 }]),
            1 => {
                const HZ: [u16; 8] = [262, 294, 330, 349, 392, 440, 494, 523];
                let mut notes = [Note { hz: 0, ms: 0 }; 8];
                for (n, hz) in notes.iter_mut().zip(HZ) {
                    *n = Note { hz, ms: 220 };
                }
                audio::play(&notes);
            }
            2 => {
                let mut notes = [Note { hz: 0, ms: 0 }; 24];
                for (i, n) in notes.iter_mut().enumerate() {
                    let step = if i < 12 { i } else { 24 - i } as u16;
                    *n = Note { hz: 500 + step * 60, ms: 60 };
                }
                audio::play(&notes);
            }
            _ => audio::stop(),
        }
    }
}

impl View for App {
    fn create(&mut self, container: &Obj<'static>) -> Result<(), WidgetError> {
        self.indev = Some(PointerIndev::new(&POINTER)?);
        container.bg_color(0x003a57).bg_opa(255);

        let tv = Tabview::new(container)?;
        tv.set_tab_bar_size(48);
        tv.bg_opa(0);
        // Swiping would fight with the sliders and the chart.
        tv.get_content().remove_flag(ObjFlag::SCROLLABLE);
        let led_tab = tv.add_tab("LED");
        let mic_tab = tv.add_tab("Mic");
        let speaker_tab = tv.add_tab("Speaker");
        let wake_tab = tv.add_tab("Wake");
        self.build_led_tab(&led_tab)?;
        self.build_mic_tab(&mic_tab)?;
        self.build_speaker_tab(&speaker_tab)?;
        self.build_wake_tab(&wake_tab)?;
        self.tabview = Some(tv);
        Ok(())
    }

    fn register_events_on(&mut self, _container: &Obj<'static>) {
        let handles: Vec<_> = self
            .color_btns
            .iter()
            .chain(self.speaker_btns.iter())
            .chain(self.rainbow_btn.iter())
            .chain(self.wake_btns.iter())
            .map(|b| b.handle())
            .chain(self.led_slider.iter().map(|s| s.handle()))
            .chain(self.volume_slider.iter().map(|s| s.handle()))
            .collect();
        for h in handles {
            register_event_on(self, h);
        }
    }

    fn on_event(&mut self, event: &Event) -> NavAction {
        for (i, btn) in self.color_btns.iter().enumerate() {
            if event.matches(btn, EventCode::CLICKED) {
                if let Some(r) = &self.rainbow_btn {
                    r.remove_state(ObjState::CHECKED);
                }
                self.led_rgb = COLORS[i].1;
                self.apply_led();
            }
        }
        for (i, btn) in self.speaker_btns.iter().enumerate() {
            if event.matches(btn, EventCode::CLICKED) {
                self.play_speaker(i);
            }
        }
        if let Some(i) = self.wake_btns.iter().position(|b| event.matches(b, EventCode::CLICKED)) {
            // One model at a time: the buttons act as radio buttons.
            for (j, b) in self.wake_btns.iter().enumerate() {
                if j == i {
                    b.add_state(ObjState::CHECKED);
                } else {
                    b.remove_state(ObjState::CHECKED);
                }
            }
            wakeword::WANTED.store(i as u8, Ordering::Relaxed);
        }
        if let Some(r) = &self.rainbow_btn {
            if event.matches(r, EventCode::VALUE_CHANGED) && !self.rainbow_on() {
                self.apply_led();
            }
        }
        if let Some(s) = &self.led_slider {
            if event.matches(s, EventCode::VALUE_CHANGED) {
                self.led_level = s.get_value().clamp(0, 255) as u8;
                self.apply_led();
            }
        }
        if let Some(s) = &self.volume_slider {
            if event.matches(s, EventCode::VALUE_CHANGED) {
                audio::VOLUME.store(s.get_value().clamp(0, 100) as u8, Ordering::Relaxed);
            }
        }
        NavAction::None
    }

    fn update(&mut self) -> Result<NavAction, WidgetError> {
        let mic_due = self.last_mic.is_none_or(|t| t.elapsed().as_millis() >= 50);
        if mic_due {
            self.last_mic = Some(Instant::now());
            let level = audio::MIC_LEVEL.load(Ordering::Relaxed);
            if let (Some(bar), Some(label), Some(chart), Some(series)) =
                (&self.mic_bar, &self.mic_label, &self.mic_chart, &self.mic_series)
            {
                bar.set_value_raw(i32::from(level), false);
                let mut s = String::<16>::new();
                let _ = write!(s, "{} dB", i32::from(level) * 80 / 100 - 80);
                label.text(&s);
                chart.set_next_value(series, i32::from(level));
            }
        }

        self.update_wake();

        if self.rainbow_on() && self.last_rainbow.is_none_or(|t| t.elapsed().as_millis() >= 40) {
            self.last_rainbow = Some(Instant::now());
            self.hue = (self.hue + 3) % 360;
            self.led_rgb = hue_to_rgb(self.hue);
            self.apply_led();
        }
        Ok(NavAction::None)
    }
}
