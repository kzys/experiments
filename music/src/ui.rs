// The front end: egui (immediate-mode UI) drawn via glow (OpenGL ES) into a
// fullscreen Wayland surface. Context creation goes through glutin rather
// than wgpu — wgpu's GLES backend unconditionally requests a robust GL
// context (a WebGL requirement it carries into native use), which this
// device's Panfrost driver rejects; glutin makes robustness opt-in, and
// falls back from core GL to GLES on its own when core isn't available.
//
// No toolkit here understands gamepads, so evdev reads (see pad.rs) still
// happen on a dedicated thread, forwarding decoded NavEvents to the winit
// event loop over an EventLoopProxy.

use std::fs::File;
use std::num::NonZeroU32;
use std::sync::Arc;

use egui_glow::glow;
use egui_winit::winit;
use input_linux::EvdevHandle;
use winit::raw_window_handle::HasWindowHandle as _;

use crate::api::{self, Item};
use crate::browse::Browser;
use crate::mpv::Mpv;
use crate::pad::{self, NavEvent};
use crate::util::die;


// Same install location music itself ends up at (see Makefile); spawned by
// absolute path since rocknix/Music.sh execs us with no guarantee that
// directory is on $PATH.
const LOWKEY_BIN: &str = "/storage/.local/bin/lowkey";

fn server_field_id() -> egui::Id {
    egui::Id::new("server_address")
}

// The event loop's user event: decoded gamepad nav events, plus the other
// async signals the UI reacts to — lowkey (the delegated on-screen keyboard)
// exiting once the user is done editing, mpv reporting playback state (see
// mpv.rs), egui asking for a repaint (see App::resumed), and SIGTERM/SIGINT
// (see watch_signals).
enum AppEvent {
    Nav(NavEvent),
    LowkeyExited,
    Mpv(crate::mpv::Event),
    Repaint(std::time::Duration),
    Terminate,
}

struct GlutinWindowContext {
    window: winit::window::Window,
    gl_context: glutin::context::PossiblyCurrentContext,
    gl_display: glutin::display::Display,
    gl_surface: glutin::surface::Surface<glutin::surface::WindowSurface>,
}

impl GlutinWindowContext {
    unsafe fn new(event_loop: &winit::event_loop::ActiveEventLoop) -> Self {
        use glutin::context::NotCurrentGlContext as _;
        use glutin::display::GetGlDisplay as _;
        use glutin::display::GlDisplay as _;
        use glutin::prelude::GlSurface as _;

        let window_attributes = winit::window::WindowAttributes::default()
            .with_title("music")
            .with_fullscreen(Some(winit::window::Fullscreen::Borderless(None)))
            .with_visible(false);

        let config_template = glutin::config::ConfigTemplateBuilder::new()
            .prefer_hardware_accelerated(None)
            .with_depth_size(0)
            .with_stencil_size(0)
            .with_transparency(false);

        let (mut window, gl_config) = glutin_winit::DisplayBuilder::new()
            .with_preference(glutin_winit::ApiPreference::FallbackEgl)
            .with_window_attributes(Some(window_attributes.clone()))
            .build(event_loop, config_template, |mut configs| {
                configs.next().unwrap_or_else(|| die("no matching GL config"))
            })
            .unwrap_or_else(|e| die(&format!("failed to create GL config: {e}")));
        let gl_display = gl_config.display();

        let raw_window_handle = window
            .as_ref()
            .map(|w| w.window_handle().expect("window handle").as_raw());

        // This device only has GLES, not core desktop GL, so the first
        // attempt always fails over to the fallback below.
        let context_attributes =
            glutin::context::ContextAttributesBuilder::new().build(raw_window_handle);
        let gles_attributes = glutin::context::ContextAttributesBuilder::new()
            .with_context_api(glutin::context::ContextApi::Gles(None))
            .build(raw_window_handle);
        let not_current = unsafe {
            gl_display.create_context(&gl_config, &context_attributes).unwrap_or_else(|_| {
                gl_display
                    .create_context(&gl_config, &gles_attributes)
                    .unwrap_or_else(|e| die(&format!("failed to create a GL context: {e}")))
            })
        };

        let window = window.take().unwrap_or_else(|| {
            glutin_winit::finalize_window(event_loop, window_attributes, &gl_config)
                .unwrap_or_else(|e| die(&format!("failed to create window: {e}")))
        });
        let (width, height): (u32, u32) = window.inner_size().into();
        let surface_attributes =
            glutin::surface::SurfaceAttributesBuilder::<glutin::surface::WindowSurface>::new().build(
                window.window_handle().expect("window handle").as_raw(),
                NonZeroU32::new(width).unwrap_or(NonZeroU32::MIN),
                NonZeroU32::new(height).unwrap_or(NonZeroU32::MIN),
            );
        let gl_surface = unsafe {
            gl_display
                .create_window_surface(&gl_config, &surface_attributes)
                .unwrap_or_else(|e| die(&format!("failed to create GL surface: {e}")))
        };
        let gl_context = not_current
            .make_current(&gl_surface)
            .unwrap_or_else(|e| die(&format!("failed to activate GL context: {e}")));
        gl_surface
            .set_swap_interval(&gl_context, glutin::surface::SwapInterval::Wait(NonZeroU32::MIN))
            .ok();

        Self { window, gl_context, gl_display, gl_surface }
    }

    fn window(&self) -> &winit::window::Window {
        &self.window
    }

    fn resize(&self, size: winit::dpi::PhysicalSize<u32>) {
        use glutin::surface::GlSurface as _;
        if let (Some(w), Some(h)) = (NonZeroU32::new(size.width), NonZeroU32::new(size.height)) {
            self.gl_surface.resize(&self.gl_context, w, h);
        }
    }

    fn swap_buffers(&self) -> glutin::error::Result<()> {
        use glutin::surface::GlSurface as _;
        self.gl_surface.swap_buffers(&self.gl_context)
    }

    fn get_proc_address(&self, addr: &std::ffi::CStr) -> *const std::ffi::c_void {
        use glutin::display::GlDisplay as _;
        self.gl_display.get_proc_address(addr)
    }
}

fn create_display(
    event_loop: &winit::event_loop::ActiveEventLoop,
) -> (GlutinWindowContext, glow::Context) {
    let ctx = unsafe { GlutinWindowContext::new(event_loop) };
    let gl = unsafe {
        glow::Context::from_loader_function(|s| {
            let s = std::ffi::CString::new(s).expect("gl proc name");
            ctx.get_proc_address(&s)
        })
    };
    (ctx, gl)
}

// egui's default_fonts feature is off (see Cargo.toml) to shave the ~1.4MB
// of bundled TTFs off the release binary, which leaves egui with no font at
// all — every label silently draws nothing rather than erroring. ROCKNIX
// also has no /usr/share/fonts to fall back on, so the UI borrows fonts
// other bundled apps ship on the read-only system partition, present on
// every ROCKNIX install.
//
// Inconsolata (from PPSSPP's assets) rather than EmulationStation's Rubik:
// monospace for the classic look, and — the practical reason — tabular
// digits. egui does no OpenType shaping (no `tnum`), and this Rubik has
// proportional figures only, so durations and positions wouldn't line up
// in anything but a fixed-pitch face. Costs nothing in line capacity:
// 0.5em per glyph, the same as Rubik's average.
const FONT_PATH: &str = "/usr/config/ppsspp/assets/Inconsolata-Regular.ttf";

// lowkey's on-screen keyboard is told to use this (see start_editing), as
// its own Terminal.sh does; it has the same no-/usr/share/fonts problem.
const LOWKEY_FONT_PATH: &str = "/usr/config/emulationstation/resources/Rubik-Regular.ttf";

// Track and album names often contain CJK text, which Inconsolata has no
// glyphs for. Fall back to EmulationStation's own bundled Droid Sans
// Fallback for whatever it can't draw.
const CJK_FONT_PATH: &str = "/usr/config/emulationstation/resources/DroidSansFallbackFull.ttf";

fn load_fonts() -> egui::FontDefinitions {
    let bytes =
        std::fs::read(FONT_PATH).unwrap_or_else(|e| die(&format!("cannot read font {FONT_PATH}: {e}")));
    let cjk_bytes = std::fs::read(CJK_FONT_PATH)
        .unwrap_or_else(|e| die(&format!("cannot read font {CJK_FONT_PATH}: {e}")));
    let mut fonts = egui::FontDefinitions::empty();
    fonts.font_data.insert("Mono".to_owned(), Arc::new(egui::FontData::from_owned(bytes)));
    fonts.font_data.insert("CJK".to_owned(), Arc::new(egui::FontData::from_owned(cjk_bytes)));
    fonts.families.insert(egui::FontFamily::Proportional, vec!["Mono".to_owned(), "CJK".to_owned()]);
    fonts.families.insert(egui::FontFamily::Monospace, vec!["Mono".to_owned(), "CJK".to_owned()]);
    fonts
}
// ---- Theme -----------------------------------------------------------------
//
// Sizes and colors follow the design mockups, which were drawn at 800×600.
// The device's panel is 640×480 — exactly 0.8× — so rather than scaling every
// number by hand, the mockup values are used verbatim as logical points and
// the whole UI is zoomed by ZOOM.

const ZOOM: f32 = 0.8;

const BG: egui::Color32 = egui::Color32::from_rgb(0x1b, 0x1b, 0x1b);
const LINE: egui::Color32 = egui::Color32::from_rgba_premultiplied(20, 20, 20, 20); // white @ 8%
const TEXT: egui::Color32 = egui::Color32::from_rgb(0xdc, 0xdc, 0xdc);
const WEAK: egui::Color32 = egui::Color32::from_rgb(0x8c, 0x8c, 0x8c);
const DIM: egui::Color32 = egui::Color32::from_rgb(0x5c, 0x5c, 0x5c);
const DIMMER: egui::Color32 = egui::Color32::from_rgb(0x4a, 0x4a, 0x4a);
const SEL_BG: egui::Color32 = egui::Color32::from_rgb(0x0f, 0x5a, 0x80);
const SEL_BORDER: egui::Color32 = egui::Color32::from_rgb(0x4f, 0xb0, 0xe0);
const SEL_WEAK: egui::Color32 = egui::Color32::from_rgb(0xcf, 0xe9, 0xf7);
const FIELD_BG: egui::Color32 = egui::Color32::from_rgb(0x23, 0x23, 0x23);
const LOG_WARN: egui::Color32 = egui::Color32::from_rgb(0xd0, 0xa0, 0x30);
const LOG_ERROR: egui::Color32 = egui::Color32::from_rgb(0xe0, 0x5a, 0x5a);
const LOG_INFO: egui::Color32 = egui::Color32::from_rgb(0x50, 0xc0, 0xc8);

// One size for everything, title included: the mockups had a 30px title,
// but a single-size monospace grid reads more like the classic players
// this is after. The log panel is the one exception, at FS_SM: it's
// diagnostics, not UI, and shouldn't compete with the list for the screen.
const FS_MD: f32 = 20.0;
const FS_SM: f32 = 15.0;
const SIDE: i8 = 32; // horizontal padding of every panel

const LEGEND_ALBUMS: &str = "A: select   B: back   Select: quit   Select+Start: hold";
const LEGEND_TRACKS: &str = "A: select   B: back   X: play/pause   L1/R1: skip   Select: quit";
const LEGEND_ERROR: &str = "A: edit server   Select: quit";
const LEGEND_ADDRESS: &str = "A: edit address   Select: quit";
const LEGEND_EDITING: &str = "close lowkey (Select) when done typing";

fn body_font() -> egui::FontId {
    egui::FontId::proportional(FS_MD)
}

fn apply_theme(ctx: &egui::Context) {
    use egui::{FontFamily::Proportional, FontId, TextStyle};

    ctx.set_zoom_factor(ZOOM);
    ctx.all_styles_mut(|style| {
        style.text_styles = [
            (TextStyle::Heading, FontId::new(FS_MD, Proportional)),
            (TextStyle::Body, FontId::new(FS_MD, Proportional)),
            (TextStyle::Button, FontId::new(FS_MD, Proportional)),
            (TextStyle::Small, FontId::new(FS_SM, Proportional)),
            (TextStyle::Monospace, FontId::new(FS_MD, Proportional)),
        ]
        .into();
        let v = &mut style.visuals;
        *v = egui::Visuals::dark();
        v.panel_fill = BG;
        v.window_fill = BG;
        v.extreme_bg_color = FIELD_BG;
        v.override_text_color = Some(TEXT);
        v.weak_text_color = Some(WEAK);
        v.selection.bg_fill = SEL_BG;
        v.selection.stroke = egui::Stroke::new(1.0, SEL_BORDER);
        // Panel separator lines and the text field's resting border.
        v.widgets.noninteractive.bg_stroke = egui::Stroke::new(1.0, LINE);
        v.widgets.inactive.bg_stroke = egui::Stroke::new(1.0, LINE);
        v.widgets.inactive.corner_radius = 2.0.into();
        // Disabled widgets keep their explicit colors instead of fading; the
        // text field's color is chosen per screen below.
        v.disabled_alpha = 1.0;
    });
}

fn panel_frame(top: i8, bottom: i8) -> egui::Frame {
    egui::Frame::new().fill(BG).inner_margin(egui::Margin { left: SIDE, right: SIDE, top, bottom })
}

fn draw_title(ui: &mut egui::Ui, title: &str) {
    egui::Panel::top("title").frame(panel_frame(14, 12)).show(ui, |ui| {
        ui.heading(title);
    });
}

fn draw_legend(ui: &mut egui::Ui, legend: &str) {
    egui::Panel::bottom("legend").frame(panel_frame(8, 12)).show(ui, |ui| {
        ui.weak(legend);
    });
}

// A handful of the most recent log lines (see util.rs), always visible: the
// only way to see what's going wrong on a kiosk device with no console.
// Added before the legend so it ends up below it, at the screen's edge.
fn draw_log_panel(ui: &mut egui::Ui, lines: &[(log::Level, String)]) {
    if lines.is_empty() {
        return;
    }
    egui::Panel::bottom("log").frame(panel_frame(6, 8)).show(ui, |ui| {
        ui.spacing_mut().item_spacing.y = 4.0;
        for (level, line) in lines {
            let color = match level {
                log::Level::Error => LOG_ERROR,
                log::Level::Warn => LOG_WARN,
                log::Level::Info => LOG_INFO,
                log::Level::Debug => TEXT,
                log::Level::Trace => DIM,
            };
            ui.add(egui::Label::new(egui::RichText::new(line).small().color(color)).wrap());
        }
    });
}

// The server address field shared by the address and error screens. Only
// editable while lowkey is up (see start_editing); otherwise shown inert,
// in the screen's chosen text color.
fn draw_server_field(ui: &mut egui::Ui, server: &mut String, editing: bool, color: egui::Color32) {
    let field = egui::TextEdit::singleline(server)
        .id(server_field_id())
        .font(body_font())
        .text_color(color)
        .background_color(FIELD_BG)
        .margin(egui::Margin::symmetric(10, 8))
        .desired_width(440.0)
        .hint_text(egui::RichText::new("not set").italics().color(DIMMER));
    ui.add_enabled(editing, field);
}

// What the list rows show at their left edge: nothing, or the marker for
// the track that's currently playing / paused.
#[derive(Clone, Copy)]
enum Marker {
    Playing,
    Paused,
}

impl Marker {
    // Text, not a drawn glyph: the same "> " a terminal player would print.
    // Every variant is the same width so the names line up across rows —
    // the font is monospace, so a blank one is just spaces.
    fn text(marker: Option<Marker>) -> &'static str {
        match marker {
            Some(Marker::Playing) => ">  ",
            Some(Marker::Paused) => "|| ",
            None => "   ",
        }
    }
}

// Every list row is this tall: one line of body text plus padding.
fn row_height(ui: &egui::Ui) -> f32 {
    let pad_y = 6.0;
    ui.fonts_mut(|f| f.row_height(&body_font())) + 2.0 * pad_y
}

// One list row: the `marker` cell (see Marker::text; empty on screens
// without one) and `left` text as one line, `right` (count or duration)
// flush to the right edge in the weak color, and the selection box around
// all of it when selected. The row spans the full list width.
fn draw_row(ui: &mut egui::Ui, selected: bool, marker: &str, left: &str, right: &str) {
    let pad_x = 12.0;
    let font = body_font();
    let (rect, _) = ui.allocate_exact_size(egui::vec2(ui.available_width(), row_height(ui)), egui::Sense::hover());
    let painter = ui.painter();

    if selected {
        painter.rect(rect, 3.0, SEL_BG, egui::Stroke::new(1.0, SEL_BORDER), egui::StrokeKind::Inside);
    }

    let right_color = if selected { SEL_WEAK } else { WEAK };
    let right_galley = painter.layout_no_wrap(right.to_owned(), font.clone(), right_color);
    let right_x = rect.right() - pad_x - right_galley.size().x;
    painter.galley(
        egui::pos2(right_x, rect.center().y - right_galley.size().y / 2.0),
        right_galley,
        right_color,
    );

    let left_x = rect.left() + pad_x;
    let mut job = egui::text::LayoutJob::default();
    job.wrap = egui::text::TextWrapping::truncate_at_width(right_x - 16.0 - left_x);
    let format = |color| egui::text::TextFormat { font_id: font.clone(), color, ..Default::default() };
    job.append(marker, 0.0, format(SEL_BORDER));
    job.append(left, 0.0, format(TEXT));
    let left_galley = painter.layout_job(job);
    painter.galley(egui::pos2(left_x, rect.center().y - left_galley.size().y / 2.0), left_galley, TEXT);
}

fn fmt_duration(secs: u64) -> String {
    format!("{}:{:02}", secs / 60, secs % 60)
}

// ---- Playback ----------------------------------------------------------------

// What's playing right now, as mpv reports it (see mpv.rs). Carries its
// own copy of the album's track list so L1/R1 can step through it
// regardless of where the browser has moved since — backing out to Albums
// doesn't lose the skip targets. mpv's playlist is the album from the
// chosen track to the end, so its entry n is tracks[first + n].
struct Playing {
    tracks: Vec<Item>,
    first: usize,
    index: usize,
    position: std::time::Duration,
    paused: bool,
}

impl Playing {
    fn item(&self) -> &Item {
        &self.tracks[self.index]
    }

    fn marker(&self) -> Marker {
        if self.paused { Marker::Paused } else { Marker::Playing }
    }
}

// ---- Hold mode ---------------------------------------------------------------
//
// For carrying the device in a bag while it keeps playing: buttons are
// ignored, the backlight is off, and the power button is grabbed so a
// stray press can't trigger ROCKNIX's suspend (which mutes audio, freezes
// the foreground process and shuts down after a delay). Real suspend is no
// use here — the CPU stops, and so does mpv.

// FB_BLANK_POWERDOWN / FB_BLANK_UNBLANK, written to each panel's bl_power —
// the same thing ROCKNIX's own rocknix-fake-suspend does by default. Leaves
// the user's brightness level alone.
const BL_POWER_OFF: &str = "4";
const BL_POWER_ON: &str = "0";

fn set_backlight(value: &str) {
    let Ok(dir) = std::fs::read_dir("/sys/class/backlight") else {
        log::warn!("no /sys/class/backlight; can't switch the backlight");
        return;
    };
    for entry in dir.flatten() {
        let path = entry.path().join("bl_power");
        if let Err(e) = std::fs::write(&path, value) {
            log::warn!("backlight {}: {e}", path.display());
        }
    }
}

// The power button is its own evdev node (the PMIC's key), separate from the
// gamepad, so it's found by name. Grabbed exclusively for as long as the
// returned handle lives; dropping it releases the grab.
const POWER_KEY_DEVICE: &str = "axp20x-pek";

fn grab_power_key() -> Option<EvdevHandle<File>> {
    let Some(key) = pad::open_pad(Some(POWER_KEY_DEVICE)) else {
        log::warn!("power button ({POWER_KEY_DEVICE}) not found; not blocking it");
        return None;
    };
    if let Err(e) = key.grab(true) {
        log::warn!("power button EVIOCGRAB: {e}");
        return None;
    }
    Some(key)
}

// ---- App ---------------------------------------------------------------------

// Browsing normally; a request failed and there's nothing to browse — most
// likely the server being unreachable over a flaky WiFi link, routine enough
// here to need an on-screen message instead of the process just dying with
// nothing visible; or no server address configured at all yet.
enum State {
    Browsing(Browser),
    Error(String),
    ServerAddress,
}

struct App {
    gl_window: Option<GlutinWindowContext>,
    gl: Option<Arc<glow::Context>>,
    egui_glow: Option<egui_glow::EguiGlow>,
    server: String,
    username: String,
    password: String,
    state: Option<State>,
    // Started on demand (see App::mpv), so a crashed mpv just costs the
    // next play a restart.
    mpv: Option<Mpv>,
    playing: Option<Playing>,
    pad: Arc<EvdevHandle<File>>,
    proxy: winit::event_loop::EventLoopProxy<AppEvent>,
    // Whether lowkey is currently up editing the server address. Only ever
    // set from the address/error screens' "A: edit" gesture; cleared when
    // lowkey's process exits (see AppEvent::LowkeyExited).
    editing: bool,
    // Hold mode (see set_hold). The power button's grab lives exactly as
    // long as the hold does.
    held: bool,
    power_key: Option<EvdevHandle<File>>,
}

fn connect(server: &str, username: &str, password: &str) -> State {
    if server.is_empty() {
        return State::ServerAddress;
    }
    match api::authenticate(server, username, password).and_then(Browser::new) {
        Ok(browser) => {
            log::info!("authenticated, browsing");
            State::Browsing(browser)
        }
        Err(e) => {
            log::error!("startup failed: {e}");
            State::Error(e)
        }
    }
}

// Releases our own EVIOCGRAB and spawns lowkey so it can grab the pad
// exclusively for itself — lowkey logs and carries on ungrabbed if the grab
// fails (e.g. because we still held it), which would leave it drawn on
// screen but deaf to button presses. A reaper thread waits for it to exit
// and wakes the event loop with AppEvent::LowkeyExited, since regrabbing
// the pad only makes sense once lowkey is actually done with the device.
fn start_editing(pad: &Arc<EvdevHandle<File>>, proxy: &winit::event_loop::EventLoopProxy<AppEvent>) -> bool {
    if let Err(e) = pad.grab(false) {
        log::warn!("EVIOCGRAB release: {e}");
    }
    // ROCKNIX has no /usr/share/fonts, so lowkey's own default font path
    // doesn't exist on-device (it dies immediately if asked to read it) —
    // same problem noted in load_fonts() above, same fix: point it at
    // EmulationStation's bundled Rubik, as lowkey's own Terminal.sh already
    // does for its terminal use case.
    let mut child = match std::process::Command::new(LOWKEY_BIN).arg("-f").arg(LOWKEY_FONT_PATH).spawn() {
        Ok(c) => c,
        Err(e) => {
            log::warn!("failed to run lowkey: {e}");
            if let Err(e) = pad.grab(true) {
                log::warn!("EVIOCGRAB: {e}");
            }
            return false;
        }
    };
    let proxy = proxy.clone();
    std::thread::spawn(move || {
        child.wait().ok();
        proxy.send_event(AppEvent::LowkeyExited).ok();
    });
    true
}

impl App {
    // The player, started if it isn't running. Failure is logged, not
    // fatal: no mpv shouldn't take down the browser.
    fn mpv(&mut self) -> Option<&mut Mpv> {
        if self.mpv.is_none() {
            let proxy = self.proxy.clone();
            match Mpv::spawn(move |ev| {
                proxy.send_event(AppEvent::Mpv(ev)).ok();
            }) {
                Ok(mpv) => self.mpv = Some(mpv),
                Err(e) => log::warn!("{e}"),
            }
        }
        self.mpv.as_mut()
    }

    // Plays tracks[index] and everything after it, in order.
    fn start_track(&mut self, tracks: Vec<Item>, index: usize) {
        let Some(State::Browsing(browser)) = self.state.as_ref() else { return };
        let urls: Vec<String> = tracks[index..].iter().map(|item| browser.stream_url(&item.id)).collect();
        let Some(mpv) = self.mpv() else { return };
        let item = &tracks[index];
        log::info!("playing {} ({}) at {}kbps", item.name, item.id, api::AUDIO_BITRATE / 1000);
        mpv.play(&urls);
        self.playing = Some(Playing {
            tracks,
            first: index,
            index,
            position: std::time::Duration::ZERO,
            paused: false,
        });
    }

    fn skip(&mut self, delta: i32) {
        let Some(p) = self.playing.as_ref() else { return };
        let next = p.index as i32 + delta;
        if next < 0 || next >= p.tracks.len() as i32 {
            return;
        }
        let tracks = p.tracks.clone();
        self.start_track(tracks, next as usize);
    }

    // The paused flag follows once mpv confirms (see Event::Paused).
    fn toggle_pause(&mut self) {
        if self.playing.is_some()
            && let Some(mpv) = self.mpv.as_mut()
        {
            mpv.toggle_pause();
        }
    }

    fn on_mpv(&mut self, ev: crate::mpv::Event) {
        use crate::mpv::Event;
        match ev {
            Event::Exited => {
                if self.mpv.take().is_some() {
                    log::warn!("mpv exited");
                }
                self.playing = None;
            }
            Event::Idle => {
                if let Some(p) = self.playing.take() {
                    log::info!("finished {}", p.item().name);
                }
            }
            Event::Track(pos) => {
                if let Some(p) = self.playing.as_mut()
                    && let Some(index) = p.first.checked_add(pos).filter(|&i| i < p.tracks.len())
                {
                    // Only worth a line when mpv moved on by itself; a
                    // selection or skip was logged when it was sent.
                    if index != p.index {
                        log::info!("playing {}", p.tracks[index].name);
                    }
                    p.index = index;
                    p.position = std::time::Duration::ZERO;
                }
            }
            Event::Position(secs) => {
                if let Some(p) = self.playing.as_mut() {
                    p.position = std::time::Duration::from_secs(secs);
                }
            }
            Event::Paused(paused) => {
                if let Some(p) = self.playing.as_mut()
                    && p.paused != paused
                {
                    p.paused = paused;
                    log::info!("{} {}", if paused { "paused" } else { "resumed" }, p.item().name);
                }
            }
        }
    }

    fn redraw(&mut self) {
        let egui_glow = self.egui_glow.as_mut().unwrap();
        let window = self.gl_window.as_ref().unwrap().window();
        let log_lines = crate::util::recent_lines();
        let editing = self.editing;

        match self.state.as_ref().unwrap() {
            State::Error(_) | State::ServerAddress => {
                let (title, message, legend, color) = match self.state.as_ref().unwrap() {
                    State::Error(message) => ("Can't reach server", message.clone(), LEGEND_ERROR, DIM),
                    _ => (
                        "Server address",
                        "Enter your Jellyfin server's address to get started.".to_owned(),
                        LEGEND_ADDRESS,
                        TEXT,
                    ),
                };
                let legend = if editing { LEGEND_EDITING } else { legend };
                let mut server = self.server.clone();
                egui_glow.run(window, |ui| {
                    draw_title(ui, title);
                    draw_log_panel(ui, &log_lines);
                    draw_legend(ui, legend);
                    egui::CentralPanel::default().frame(panel_frame(20, 0)).show(ui, |ui| {
                        ui.label(&message);
                        ui.add_space(18.0);
                        draw_server_field(ui, &mut server, editing, if editing { TEXT } else { color });
                    });
                });
                self.server = server;
            }
            State::Browsing(browser) => {
                let frame = browser.current();
                let title = frame.title.clone();
                let at_tracks = frame.at_tracks();
                // Track numbers right-aligned in a column just wide enough
                // for the album's largest, and an untagged track blanked to
                // the same width, keeping the names in a column of their
                // own — exact, since the font is monospace.
                let digits = frame
                    .items
                    .iter()
                    .filter_map(|item| item.index_number)
                    .max()
                    .map_or(1, |n| n.to_string().len());
                let rows: Vec<(&str, String, String)> = frame
                    .items
                    .iter()
                    .map(|item| {
                        let playing = self.playing.as_ref().filter(|p| p.item().id == item.id);
                        let name = match item.index_number {
                            Some(n) if at_tracks => format!("{n:>digits$}. {}", item.name),
                            None if at_tracks => format!("{:digits$}  {}", "", item.name),
                            _ => item.name.clone(),
                        };
                        let right = if at_tracks {
                            let dur = item.duration_secs().map(fmt_duration).unwrap_or_default();
                            match playing {
                                Some(p) => format!("{} / {dur}", fmt_duration(p.position.as_secs())),
                                None => dur,
                            }
                        } else {
                            item.child_count.map(|n| format!("{n} tracks")).unwrap_or_default()
                        };
                        // Only tracks can be playing, so only the track
                        // list has the marker column.
                        let marker = if at_tracks { Marker::text(playing.map(Playing::marker)) } else { "" };
                        (marker, name, right)
                    })
                    .collect();
                let selected = frame.selected;

                egui_glow.run(window, |ui| {
                    draw_title(ui, &title);
                    draw_log_panel(ui, &log_lines);
                    draw_legend(ui, if at_tracks { LEGEND_TRACKS } else { LEGEND_ALBUMS });
                    let list_frame = egui::Frame::new().fill(BG).inner_margin(egui::Margin {
                        left: SIDE,
                        right: SIDE,
                        top: 10,
                        bottom: 10,
                    });
                    egui::CentralPanel::default().frame(list_frame).show(ui, |ui| {
                        ui.spacing_mut().item_spacing.y = 2.0;
                        // The list isn't scrolled, it's selected: the offset
                        // is whatever centers the selected row, set up front
                        // so this pass lays the rows out in their final
                        // place. (scroll_to_me would only take effect on the
                        // next pass, showing one frame with the selection
                        // box moved but the list not yet caught up.) Clamped
                        // to the list here, since egui only does that after
                        // laying the content out.
                        let row_h = row_height(ui);
                        let spacing = ui.spacing().item_spacing.y;
                        let pitch = row_h + spacing;
                        let view_h = ui.available_height();
                        let content_h = rows.len() as f32 * pitch - spacing;
                        let offset = (selected as f32 * pitch - (view_h - row_h) / 2.0)
                            .clamp(0.0, (content_h - view_h).max(0.0));
                        egui::ScrollArea::vertical()
                            .vertical_scroll_offset(offset)
                            .auto_shrink([false, false])
                            .scroll_bar_visibility(egui::scroll_area::ScrollBarVisibility::AlwaysHidden)
                            .show(ui, |ui| {
                                if rows.is_empty() {
                                    ui.weak("(empty)");
                                }
                                for (idx, (marker, name, right)) in rows.iter().enumerate() {
                                    draw_row(ui, idx == selected, marker, name, right);
                                }
                            });
                    });
                });
            }
        }

        unsafe {
            use glow::HasContext as _;
            let gl = self.gl.as_ref().unwrap();
            gl.clear_color(0.106, 0.106, 0.106, 1.0); // BG
            gl.clear(glow::COLOR_BUFFER_BIT);
        }
        egui_glow.paint(window);
        self.gl_window.as_ref().unwrap().swap_buffers().ok();
    }

    fn set_hold(&mut self, held: bool) {
        if held == self.held {
            return;
        }
        self.held = held;
        if held {
            log::info!("hold on: buttons ignored, screen off (Select+Start to release)");
            self.power_key = grab_power_key();
            set_backlight(BL_POWER_OFF);
        } else {
            log::info!("hold off");
            set_backlight(BL_POWER_ON);
            self.power_key = None;
        }
    }

    fn handle_nav(&mut self, ev: NavEvent, event_loop: &winit::event_loop::ActiveEventLoop) {
        if ev == NavEvent::Hold {
            // Not while lowkey has the pad: it sees the same presses, and
            // a dark screen with a live keyboard would be confusing.
            if !self.editing {
                self.set_hold(!self.held);
            }
            return;
        }
        if self.held {
            return; // everything else is a stray press in a bag
        }
        match ev {
            NavEvent::Quit => {
                event_loop.exit();
                return;
            }
            NavEvent::PlayPause => return self.toggle_pause(),
            NavEvent::Prev => return self.skip(-1),
            NavEvent::Next => return self.skip(1),
            _ => {}
        }
        match self.state.as_mut() {
            Some(State::Error(_) | State::ServerAddress) if ev == NavEvent::Select && !self.editing => {
                if start_editing(&self.pad, &self.proxy) {
                    self.editing = true;
                    if let Some(egui_glow) = &self.egui_glow {
                        egui_glow.egui_ctx.memory_mut(|m| m.request_focus(server_field_id()));
                    }
                }
            }
            Some(State::Browsing(browser)) => match ev {
                NavEvent::Up => browser.move_selection(-1),
                NavEvent::Down => browser.move_selection(1),
                NavEvent::Back => {
                    browser.back();
                }
                NavEvent::Select => match browser.enter() {
                    Ok(Some(_)) => {
                        let frame = browser.current();
                        let (tracks, index) = (frame.items.clone(), frame.selected);
                        self.start_track(tracks, index);
                    }
                    Ok(None) => {}
                    Err(e) => {
                        log::error!("browse failed: {e}");
                        self.state = Some(State::Error(e));
                    }
                },
                _ => {}
            },
            _ => {}
        }
    }
}

impl winit::application::ApplicationHandler<AppEvent> for App {
    fn new_events(&mut self, event_loop: &winit::event_loop::ActiveEventLoop, cause: winit::event::StartCause) {
        if let winit::event::StartCause::ResumeTimeReached { .. } = cause {
            // The delayed repaint's timer (see AppEvent::Repaint). Back to
            // plain Wait, or the lapsed deadline would wake us every turn.
            event_loop.set_control_flow(winit::event_loop::ControlFlow::Wait);
            if !self.held && let Some(gl_window) = &self.gl_window {
                gl_window.window().request_redraw();
            }
        }
    }

    fn resumed(&mut self, event_loop: &winit::event_loop::ActiveEventLoop) {
        let (gl_window, gl) = create_display(event_loop);
        let gl = Arc::new(gl);
        gl_window.window().set_visible(true);

        let egui_glow = egui_glow::EguiGlow::new(event_loop, Arc::clone(&gl), None, None, true);
        // EguiGlow::run() drops the repaint delay egui reports per pass, and
        // without this callback egui has no other way to ask for a frame —
        // its own animations (scrolling, cursor blink, hover fades) and
        // request_repaint_after timers would stall until some input event
        // happened to trigger the next redraw.
        let proxy = self.proxy.clone();
        egui_glow.egui_ctx.set_request_repaint_callback(move |info| {
            proxy.send_event(AppEvent::Repaint(info.delay)).ok();
        });
        egui_glow.egui_ctx.set_fonts(load_fonts());
        apply_theme(&egui_glow.egui_ctx);
        self.egui_glow = Some(egui_glow);
        self.gl = Some(gl);
        self.gl_window = Some(gl_window);

        // Only now, with a surface up, is there somewhere to show a failure
        // — connect after, not before, so a flaky link shows an on-screen
        // message instead of the process dying with nothing to see.
        self.state = Some(connect(&self.server, &self.username, &self.password));
        // Up front rather than on the first Select, so that doesn't stall
        // waiting for mpv to start.
        self.mpv();

        self.gl_window.as_ref().unwrap().window().request_redraw();
    }

    fn window_event(
        &mut self,
        event_loop: &winit::event_loop::ActiveEventLoop,
        _window_id: winit::window::WindowId,
        event: winit::event::WindowEvent,
    ) {
        use winit::event::WindowEvent;

        if matches!(event, WindowEvent::CloseRequested | WindowEvent::Destroyed) {
            event_loop.exit();
            return;
        }
        if matches!(event, WindowEvent::RedrawRequested) {
            self.redraw();
            return;
        }
        if let WindowEvent::Resized(size) = &event {
            self.gl_window.as_ref().unwrap().resize(*size);
        }

        let response = self
            .egui_glow
            .as_mut()
            .unwrap()
            .on_window_event(self.gl_window.as_ref().unwrap().window(), &event);
        if response.repaint {
            self.gl_window.as_ref().unwrap().window().request_redraw();
        }
    }

    fn user_event(&mut self, event_loop: &winit::event_loop::ActiveEventLoop, ev: AppEvent) {
        let ev = match ev {
            // Same path as Select: exiting() stops mpv and undoes hold mode.
            AppEvent::Terminate => {
                log::info!("terminated, quitting");
                event_loop.exit();
                return;
            }
            AppEvent::LowkeyExited => {
                self.editing = false;
                if let Err(e) = self.pad.grab(true) {
                    log::warn!("EVIOCGRAB: {e}");
                }
                if let Some(egui_glow) = &self.egui_glow {
                    egui_glow.egui_ctx.memory_mut(|m| m.surrender_focus(server_field_id()));
                }
                self.server = self.server.trim().trim_end_matches('/').to_string();
                self.state = Some(connect(&self.server, &self.username, &self.password));
                if let Some(gl_window) = &self.gl_window {
                    gl_window.window().request_redraw();
                }
                return;
            }
            AppEvent::Mpv(ev) => {
                self.on_mpv(ev);
                // Once a second while playing, for the position counter;
                // not while held, same as for Nav below.
                if !self.held && let Some(gl_window) = &self.gl_window {
                    gl_window.window().request_redraw();
                }
                return;
            }
            // Nothing to show while held (see the Nav case below); any
            // pending timer is simply left to lapse.
            AppEvent::Repaint(_) if self.held => return,
            AppEvent::Repaint(delay) if delay.is_zero() => {
                if let Some(gl_window) = &self.gl_window {
                    gl_window.window().request_redraw();
                }
                return;
            }
            AppEvent::Repaint(delay) => {
                // Within a pass egui only reports a delay shorter than the
                // one before, and each pass re-requests whatever it still
                // needs, so the latest is the one to keep. Fires as
                // StartCause::ResumeTimeReached (see new_events).
                let at = std::time::Instant::now() + delay;
                event_loop.set_control_flow(winit::event_loop::ControlFlow::WaitUntil(at));
                return;
            }
            AppEvent::Nav(ev) => ev,
        };

        self.handle_nav(ev, event_loop);
        // No point painting a dark screen; also keeps the GPU idle while
        // held.
        if !self.held && let Some(gl_window) = &self.gl_window {
            gl_window.window().request_redraw();
        }
    }

    fn exiting(&mut self, _event_loop: &winit::event_loop::ActiveEventLoop) {
        // Never leave the screen dark behind us, whatever the exit path.
        self.set_hold(false);
        self.mpv = None;
        if let Some(egui_glow) = self.egui_glow.as_mut() {
            egui_glow.destroy();
        }
    }
}

// ---- Signals -----------------------------------------------------------------

// Write end of the self-pipe the signal handler pokes; -1 until set up.
static SIGNAL_PIPE: std::sync::atomic::AtomicI32 = std::sync::atomic::AtomicI32::new(-1);

extern "C" fn on_signal(_: libc::c_int) {
    // write(2) is async-signal-safe; nothing else here is allowed to be.
    let fd = SIGNAL_PIPE.load(std::sync::atomic::Ordering::Relaxed);
    unsafe { libc::write(fd, [0u8].as_ptr().cast(), 1) };
}

// Turns SIGTERM/SIGINT into AppEvent::Terminate so the event loop exits
// through the same exiting() as a Select press, stopping mpv and restoring
// the backlight — the default action would leave mpv playing on its own.
// The handler can only safely write to a pipe; a thread on the other end
// does the actual send. Handlers (unlike a blocked signal mask) aren't
// inherited across exec, so mpv and lowkey see plain defaults.
fn watch_signals(proxy: winit::event_loop::EventLoopProxy<AppEvent>) {
    use std::io::Read;
    use std::os::fd::FromRawFd;

    let mut fds = [0 as libc::c_int; 2];
    if unsafe { libc::pipe2(fds.as_mut_ptr(), libc::O_CLOEXEC) } != 0 {
        log::warn!("signal pipe: {}", std::io::Error::last_os_error());
        return;
    }
    SIGNAL_PIPE.store(fds[1], std::sync::atomic::Ordering::Relaxed);
    let mut read_end = unsafe { File::from_raw_fd(fds[0]) };
    for sig in [libc::SIGTERM, libc::SIGINT] {
        unsafe { libc::signal(sig, on_signal as extern "C" fn(libc::c_int) as libc::sighandler_t) };
    }
    std::thread::spawn(move || {
        let mut byte = [0u8];
        while read_end.read_exact(&mut byte).is_ok() {
            if proxy.send_event(AppEvent::Terminate).is_err() {
                return; // event loop shut down
            }
        }
    });
}

pub fn run(server: String, username: String, password: String) {
    let pad = pad::open_pad(None).unwrap_or_else(|| die("no gamepad found"));
    // Exclusive access: EmulationStation is still running underneath (this
    // app is fullscreen on top of it, not replacing it), and would
    // otherwise also react to the same button presses. Released temporarily
    // while lowkey is up editing the server address (see start_editing).
    if let Err(e) = pad.grab(true) {
        log::warn!("EVIOCGRAB: {e}");
    }
    let pad = Arc::new(pad);

    let event_loop = winit::event_loop::EventLoop::<AppEvent>::with_user_event()
        .build()
        .unwrap_or_else(|e| die(&format!("cannot create event loop: {e}")));
    let proxy = event_loop.create_proxy();
    watch_signals(proxy.clone());

    let reader_pad = Arc::clone(&pad);
    let reader_proxy = proxy.clone();
    std::thread::spawn(move || {
        let mut repeat = pad::Repeater::new();
        let mut chord = pad::Chord::new();
        loop {
            // Wakes early for a real event, or on its own once a held
            // direction's repeat comes due — read_pad blocks indefinitely
            // otherwise, which would starve auto-repeat between presses.
            let timeout_ms = repeat.poll_timeout_ms(std::time::Instant::now());
            if pad::wait_readable(&reader_pad, timeout_ms) {
                let Some(events) = pad::read_pad(&reader_pad, &mut repeat, &mut chord) else {
                    log::error!("gamepad reader: read error, giving up");
                    reader_proxy.send_event(AppEvent::Nav(NavEvent::Refresh)).ok();
                    return;
                };
                for ev in events {
                    if reader_proxy.send_event(AppEvent::Nav(ev)).is_err() {
                        return; // event loop shut down
                    }
                }
            }
            if let Some(ev) = repeat.due(std::time::Instant::now())
                && reader_proxy.send_event(AppEvent::Nav(ev)).is_err()
            {
                return; // event loop shut down
            }
        }
    });

    let mut app = App {
        gl_window: None,
        gl: None,
        egui_glow: None,
        server,
        username,
        password,
        state: None,
        mpv: None,
        playing: None,
        pad,
        proxy,
        editing: false,
        held: false,
        power_key: None,
    };
    event_loop.run_app(&mut app).unwrap_or_else(|e| die(&format!("event loop error: {e}")));
}
