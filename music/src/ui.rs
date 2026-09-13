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
use crate::pad::{self, NavEvent};
use crate::util::die;

const LEGEND: &str = "A: select   B: back   Select: quit";
const ERROR_LEGEND: &str = "A: edit server   Select: quit";
const EDITING_LEGEND: &str = "close lowkey (Select) when done typing";

// Same install location music itself ends up at (see Makefile); spawned by
// absolute path since rocknix/Music.sh execs us with no guarantee that
// directory is on $PATH.
const LOWKEY_BIN: &str = "/storage/.local/bin/lowkey";

fn server_field_id() -> egui::Id {
    egui::Id::new("server_address")
}

// The event loop's user event: decoded gamepad nav events, plus the one
// other async signal the UI reacts to — lowkey (the delegated on-screen
// keyboard) exiting once the user is done editing.
enum AppEvent {
    Nav(NavEvent),
    LowkeyExited,
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
// also has no /usr/share/fonts to fall back on, same problem lowkey's
// Terminal.sh hits for its on-screen keyboard; reuse its fix and point at
// EmulationStation's own bundled Rubik, present on every ROCKNIX install.
const FONT_PATH: &str = "/usr/config/emulationstation/resources/Rubik-Regular.ttf";

// Track and album names often contain CJK text, which Rubik has no glyphs
// for. Fall back to EmulationStation's own bundled Droid Sans Fallback,
// present on every ROCKNIX install, for whatever Rubik can't draw.
const CJK_FONT_PATH: &str = "/usr/config/emulationstation/resources/DroidSansFallbackFull.ttf";

fn load_fonts() -> egui::FontDefinitions {
    let bytes =
        std::fs::read(FONT_PATH).unwrap_or_else(|e| die(&format!("cannot read font {FONT_PATH}: {e}")));
    let cjk_bytes = std::fs::read(CJK_FONT_PATH)
        .unwrap_or_else(|e| die(&format!("cannot read font {CJK_FONT_PATH}: {e}")));
    let mut fonts = egui::FontDefinitions::empty();
    fonts.font_data.insert("Rubik".to_owned(), Arc::new(egui::FontData::from_owned(bytes)));
    fonts.font_data.insert("CJK".to_owned(), Arc::new(egui::FontData::from_owned(cjk_bytes)));
    fonts.families.insert(egui::FontFamily::Proportional, vec!["Rubik".to_owned(), "CJK".to_owned()]);
    fonts.families.insert(egui::FontFamily::Monospace, vec!["Rubik".to_owned(), "CJK".to_owned()]);
    fonts
}

// Spawns mpv on the item's stream URL and returns immediately — mpv keeps
// playing in the background while the event loop (gamepad input included)
// stays responsive. Failure to spawn is logged, not fatal: a bad track
// shouldn't take down the browser. Full transport control (pause/skip/
// volume over mpv's JSON IPC) is still PLAN.md's playback-control step; this
// just stops playback from freezing the UI.
fn play(browser: &Browser, item: &Item) -> Option<std::process::Child> {
    let url = browser.stream_url(&item.id);
    log::info!("playing {} ({})", item.name, item.id);
    match std::process::Command::new("mpv").arg("--no-video").arg(&url).spawn() {
        Ok(child) => Some(child),
        Err(e) => {
            log::warn!("failed to run mpv: {e}");
            None
        }
    }
}

// Either browsing normally, or a request failed and there's nothing to
// browse — most likely the server being unreachable over a flaky WiFi
// link, which is routine enough here that it needs an on-screen message
// instead of the process just dying with nothing visible on the kiosk's
// screen.
enum State {
    Browsing(Browser),
    Error(String),
}

struct App {
    gl_window: Option<GlutinWindowContext>,
    gl: Option<Arc<glow::Context>>,
    egui_glow: Option<egui_glow::EguiGlow>,
    server: String,
    username: String,
    password: String,
    state: Option<State>,
    mpv: Option<std::process::Child>,
    pad: Arc<EvdevHandle<File>>,
    proxy: winit::event_loop::EventLoopProxy<AppEvent>,
    // Whether lowkey is currently up editing the server address. Only ever
    // set from the Error screen's "A: edit server" gesture; cleared when
    // lowkey's process exits (see AppEvent::LowkeyExited).
    editing: bool,
}

fn connect(server: &str, username: &str, password: &str) -> State {
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
    let mut child = match std::process::Command::new(LOWKEY_BIN).arg("-f").arg(FONT_PATH).spawn() {
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

// Stops whatever's currently playing, if anything, so starting a new track
// doesn't leave the old one's audio running underneath it.
fn stop_playback(mpv: &mut Option<std::process::Child>) {
    if let Some(mut child) = mpv.take() {
        child.kill().ok();
        child.wait().ok();
    }
}

// A handful of the most recent log lines (see util.rs), always visible: the
// only way to see what's going wrong on a kiosk device with no console.
fn draw_log_panel(ui: &mut egui::Ui, lines: &[String]) {
    if lines.is_empty() {
        return;
    }
    egui::Panel::bottom("log").show(ui, |ui| {
        for line in lines {
            ui.small(line);
        }
    });
}

impl App {
    fn redraw(&mut self) {
        let egui_glow = self.egui_glow.as_mut().unwrap();
        let window = self.gl_window.as_ref().unwrap().window();
        let log_lines = crate::util::recent_lines();

        match self.state.as_ref().unwrap() {
            State::Error(message) => {
                let message = message.clone();
                let editing = self.editing;
                let mut server = self.server.clone();
                egui_glow.run(window, |ctx| {
                    egui::CentralPanel::default().show(ctx, |ui| {
                        ui.heading("Can't reach server");
                        ui.label(&message);
                        ui.add_enabled(
                            editing,
                            egui::TextEdit::singleline(&mut server).id(server_field_id()),
                        );
                    });
                    egui::Panel::bottom("legend").show(ctx, |ui| {
                        ui.weak(if editing { EDITING_LEGEND } else { ERROR_LEGEND });
                    });
                    draw_log_panel(ctx, &log_lines);
                });
                self.server = server;
            }
            State::Browsing(browser) => {
                let frame = browser.current();
                let title = frame.title.clone();
                let names: Vec<String> = frame.items.iter().map(|i| i.name.clone()).collect();
                let selected = frame.selected;

                egui_glow.run(window, |ctx| {
                    egui::Panel::top("title").show(ctx, |ui| {
                        ui.heading(&title);
                    });
                    egui::Panel::bottom("legend").show(ctx, |ui| {
                        ui.weak(LEGEND);
                    });
                    draw_log_panel(ctx, &log_lines);
                    egui::CentralPanel::default().show(ctx, |ui| {
                        egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
                            if names.is_empty() {
                                ui.weak("(empty)");
                            }
                            for (idx, name) in names.iter().enumerate() {
                                let resp = ui.selectable_label(idx == selected, name);
                                if idx == selected {
                                    resp.scroll_to_me(Some(egui::Align::Center));
                                }
                            }
                        });
                    });
                });
            }
        }

        unsafe {
            use glow::HasContext as _;
            let gl = self.gl.as_ref().unwrap();
            gl.clear_color(0.06, 0.06, 0.08, 1.0);
            gl.clear(glow::COLOR_BUFFER_BIT);
        }
        egui_glow.paint(window);
        self.gl_window.as_ref().unwrap().swap_buffers().ok();
    }

    fn handle_nav(&mut self, ev: NavEvent, event_loop: &winit::event_loop::ActiveEventLoop) {
        if ev == NavEvent::Quit {
            event_loop.exit();
            return;
        }
        match self.state.as_mut() {
            Some(State::Error(_)) if ev == NavEvent::Select && !self.editing => {
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
                    Ok(Some(item)) => {
                        stop_playback(&mut self.mpv);
                        self.mpv = play(browser, &item);
                    }
                    Ok(None) => {}
                    Err(e) => {
                        log::error!("browse failed: {e}");
                        self.state = Some(State::Error(e));
                    }
                },
                NavEvent::Refresh => {}
                NavEvent::Quit => unreachable!(),
            },
            _ => {}
        }
    }
}

impl winit::application::ApplicationHandler<AppEvent> for App {
    fn resumed(&mut self, event_loop: &winit::event_loop::ActiveEventLoop) {
        let (gl_window, gl) = create_display(event_loop);
        let gl = Arc::new(gl);
        gl_window.window().set_visible(true);

        let egui_glow = egui_glow::EguiGlow::new(event_loop, Arc::clone(&gl), None, None, true);
        egui_glow.egui_ctx.set_fonts(load_fonts());
        self.egui_glow = Some(egui_glow);
        self.gl = Some(gl);
        self.gl_window = Some(gl_window);

        // Only now, with a surface up, is there somewhere to show a failure
        // — connect after, not before, so a flaky link shows an on-screen
        // message instead of the process dying with nothing to see.
        self.state = Some(connect(&self.server, &self.username, &self.password));

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
            AppEvent::LowkeyExited => {
                self.editing = false;
                if let Err(e) = self.pad.grab(true) {
                    log::warn!("EVIOCGRAB: {e}");
                }
                if let Some(egui_glow) = &self.egui_glow {
                    egui_glow.egui_ctx.memory_mut(|m| m.surrender_focus(server_field_id()));
                }
                self.state = Some(connect(&self.server, &self.username, &self.password));
                if let Some(gl_window) = &self.gl_window {
                    gl_window.window().request_redraw();
                }
                return;
            }
            AppEvent::Nav(ev) => ev,
        };

        self.handle_nav(ev, event_loop);
        if let Some(gl_window) = &self.gl_window {
            gl_window.window().request_redraw();
        }
    }

    fn exiting(&mut self, _event_loop: &winit::event_loop::ActiveEventLoop) {
        stop_playback(&mut self.mpv);
        if let Some(egui_glow) = self.egui_glow.as_mut() {
            egui_glow.destroy();
        }
    }
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

    let reader_pad = Arc::clone(&pad);
    let reader_proxy = proxy.clone();
    std::thread::spawn(move || {
        let mut repeat = pad::Repeater::new();
        loop {
            // Wakes early for a real event, or on its own once a held
            // direction's repeat comes due — read_pad blocks indefinitely
            // otherwise, which would starve auto-repeat between presses.
            let timeout_ms = repeat.poll_timeout_ms(std::time::Instant::now());
            if pad::wait_readable(&reader_pad, timeout_ms) {
                let Some(events) = pad::read_pad(&reader_pad, &mut repeat) else {
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
        pad,
        proxy,
        editing: false,
    };
    event_loop.run_app(&mut app).unwrap_or_else(|e| die(&format!("event loop error: {e}")));
}
