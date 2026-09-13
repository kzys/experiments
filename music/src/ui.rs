// The Wayland front end: a fullscreen xdg_shell surface showing the current
// browse::Frame, redrawn on pad navigation. Reuses lowkey's registry/shm
// buffer plumbing, but a plain fullscreen `xdg_toplevel` instead of a
// layer-shell overlay — this app is the whole screen, not something drawn
// on top of another client (see PLAN.md's "UI" section).

use std::ffi::CString;
use std::fs::File;
use std::os::fd::{AsFd, AsRawFd, FromRawFd, OwnedFd};

use input_linux::EvdevHandle;
use wayland_client::protocol::{wl_buffer, wl_compositor, wl_registry, wl_shm, wl_shm_pool, wl_surface};
use wayland_client::{delegate_noop, Connection, Dispatch, EventQueue, QueueHandle};
use wayland_protocols::xdg::shell::client::{xdg_surface, xdg_toplevel, xdg_wm_base};

use crate::api::Item;
use crate::browse::Browser;
use crate::font::{Canvas, Rasterizer};
use crate::pad::{self, NavEvent};
use crate::render;
use crate::util::die;

const LEGEND: &str = "A: select   B: back   Select: quit";

struct App {
    compositor: Option<wl_compositor::WlCompositor>,
    shm: Option<wl_shm::WlShm>,
    wm_base: Option<xdg_wm_base::XdgWmBase>,

    surface: Option<wl_surface::WlSurface>,
    toplevel: Option<xdg_toplevel::XdgToplevel>,
    buffer: Option<wl_buffer::WlBuffer>,

    pixels: *mut u32,
    pixels_len: usize,

    browser: Browser,
    font: Rasterizer,
    pad: EvdevHandle<File>,

    dirty: bool,
    configured: bool,
    running: bool,
}

fn draw(app: &mut App) {
    let frame = app.browser.current();
    let pixels = unsafe { std::slice::from_raw_parts_mut(app.pixels, app.pixels_len) };
    let mut canvas = Canvas::new(pixels, render::WIDTH, render::HEIGHT);
    render::draw(&mut canvas, &app.font, &frame.title, &frame.items, frame.selected, LEGEND);
}

fn commit(app: &mut App) {
    draw(app);
    let buffer = app.buffer.as_ref().unwrap();
    let surface = app.surface.as_ref().unwrap();
    surface.attach(Some(buffer), 0, 0);
    surface.damage_buffer(0, 0, render::WIDTH, render::HEIGHT);
    surface.commit();
    app.dirty = false;
}

fn make_buffer(app: &mut App, qh: &QueueHandle<App>) {
    let size = (render::WIDTH as usize) * (render::HEIGHT as usize) * 4;

    let name = CString::new("music").unwrap();
    let raw_fd = unsafe { libc::memfd_create(name.as_ptr(), libc::MFD_CLOEXEC) };
    if raw_fd < 0 {
        die(&format!("memfd_create: {}", std::io::Error::last_os_error()));
    }
    if unsafe { libc::ftruncate(raw_fd, size as libc::off_t) } < 0 {
        die(&format!("ftruncate: {}", std::io::Error::last_os_error()));
    }

    let ptr = unsafe {
        libc::mmap(std::ptr::null_mut(), size, libc::PROT_READ | libc::PROT_WRITE, libc::MAP_SHARED, raw_fd, 0)
    };
    if ptr == libc::MAP_FAILED {
        die(&format!("mmap: {}", std::io::Error::last_os_error()));
    }

    app.pixels = ptr as *mut u32;
    app.pixels_len = size / 4;

    let fd = unsafe { OwnedFd::from_raw_fd(raw_fd) };
    let shm = app.shm.as_ref().unwrap();
    let pool = shm.create_pool(fd.as_fd(), size as i32, qh, ());
    let buffer = pool.create_buffer(
        0,
        render::WIDTH,
        render::HEIGHT,
        render::WIDTH * 4,
        wl_shm::Format::Argb8888,
        qh,
        (),
    );
    pool.destroy();
    // fd is dropped (closed) here; the mapping stays valid.

    app.buffer = Some(buffer);
}

// Runs mpv on the item's stream URL and blocks until it exits. Logged, not
// fatal, on failure — a bad track shouldn't take down the browser (unlike
// v0's stdin loop, there's now a UI worth staying alive for). Non-blocking
// playback control is a later step (see PLAN.md).
fn play(browser: &Browser, item: &Item) {
    let url = browser.stream_url(&item.id);
    match std::process::Command::new("mpv").arg("--no-video").arg(&url).status() {
        Ok(status) if !status.success() => eprintln!("mpv exited with {status}"),
        Err(e) => eprintln!("failed to run mpv: {e}"),
        Ok(_) => {}
    }
}

fn handle_nav(app: &mut App, ev: NavEvent) {
    match ev {
        NavEvent::Up => {
            app.browser.move_selection(-1);
            app.dirty = true;
        }
        NavEvent::Down => {
            app.browser.move_selection(1);
            app.dirty = true;
        }
        NavEvent::Back => {
            if app.browser.back() {
                app.dirty = true;
            }
        }
        NavEvent::Select => {
            if let Some(item) = app.browser.enter() {
                play(&app.browser, &item);
            }
            app.dirty = true;
        }
        NavEvent::Quit => app.running = false,
    }
}

impl Dispatch<wl_registry::WlRegistry, ()> for App {
    fn event(
        app: &mut Self,
        registry: &wl_registry::WlRegistry,
        event: wl_registry::Event,
        _: &(),
        _: &Connection,
        qh: &QueueHandle<Self>,
    ) {
        let wl_registry::Event::Global { name, interface, .. } = event else {
            return;
        };
        match interface.as_str() {
            "wl_compositor" => {
                app.compositor = Some(registry.bind::<wl_compositor::WlCompositor, _, _>(name, 4, qh, ()));
            }
            "wl_shm" => {
                app.shm = Some(registry.bind::<wl_shm::WlShm, _, _>(name, 1, qh, ()));
            }
            "xdg_wm_base" => {
                app.wm_base = Some(registry.bind::<xdg_wm_base::XdgWmBase, _, _>(name, 1, qh, ()));
            }
            _ => {}
        }
    }
}

impl Dispatch<xdg_wm_base::XdgWmBase, ()> for App {
    fn event(
        _: &mut Self,
        wm_base: &xdg_wm_base::XdgWmBase,
        event: xdg_wm_base::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        if let xdg_wm_base::Event::Ping { serial } = event {
            wm_base.pong(serial);
        }
    }
}

impl Dispatch<xdg_surface::XdgSurface, ()> for App {
    fn event(
        app: &mut Self,
        xdg_surface: &xdg_surface::XdgSurface,
        event: xdg_surface::Event,
        _: &(),
        _: &Connection,
        qh: &QueueHandle<Self>,
    ) {
        let xdg_surface::Event::Configure { serial } = event else {
            return;
        };
        xdg_surface.ack_configure(serial);

        if app.buffer.is_none() {
            make_buffer(app, qh);
        }
        app.configured = true;
        commit(app);
    }
}

impl Dispatch<xdg_toplevel::XdgToplevel, ()> for App {
    fn event(
        app: &mut Self,
        _: &xdg_toplevel::XdgToplevel,
        event: xdg_toplevel::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        // Width/height are ignored: the panel is a fixed 640x480 (confirmed
        // against the real device), so there's no other size to adapt to.
        if let xdg_toplevel::Event::Close = event {
            app.running = false;
        }
    }
}

delegate_noop!(App: ignore wl_compositor::WlCompositor);
delegate_noop!(App: ignore wl_shm::WlShm);
delegate_noop!(App: ignore wl_shm_pool::WlShmPool);
delegate_noop!(App: ignore wl_buffer::WlBuffer);
delegate_noop!(App: ignore wl_surface::WlSurface);

pub fn run(browser: Browser) {
    let pad = pad::open_pad(None).unwrap_or_else(|| die("no gamepad found"));
    // Exclusive access: EmulationStation is still running underneath (this
    // app is fullscreen on top of it, not replacing it), and would
    // otherwise also react to the same button presses. See lowkey's
    // main.rs, which grabs for the same reason.
    if let Err(e) = pad.grab(true) {
        eprintln!("EVIOCGRAB: {e}");
    }

    let font = Rasterizer::load(crate::font::DEFAULT_PATH);

    let conn = Connection::connect_to_env().unwrap_or_else(|e| die(&format!("cannot connect to a Wayland display: {e}")));
    let mut event_queue: EventQueue<App> = conn.new_event_queue();
    let qh = event_queue.handle();

    let display = conn.display();
    display.get_registry(&qh, ());

    let mut app = App {
        compositor: None,
        shm: None,
        wm_base: None,
        surface: None,
        toplevel: None,
        buffer: None,
        pixels: std::ptr::null_mut(),
        pixels_len: 0,
        browser,
        font,
        pad,
        dirty: false,
        configured: false,
        running: true,
    };

    event_queue.roundtrip(&mut app).unwrap_or_else(|e| die(&format!("initial roundtrip: {e}")));

    let compositor = app.compositor.clone().unwrap_or_else(|| die("compositor lacks wl_compositor"));
    let wm_base = app.wm_base.clone().unwrap_or_else(|| die("compositor lacks xdg_wm_base"));
    if app.shm.is_none() {
        die("compositor lacks wl_shm");
    }

    let surface = compositor.create_surface(&qh, ());
    let xdg_surface = wm_base.get_xdg_surface(&surface, &qh, ());
    let toplevel = xdg_surface.get_toplevel(&qh, ());
    toplevel.set_title("music".to_string());
    toplevel.set_app_id("music".to_string());
    toplevel.set_fullscreen(None);
    app.surface = Some(surface.clone());
    app.toplevel = Some(toplevel);
    surface.commit();

    event_queue.roundtrip(&mut app).unwrap_or_else(|e| die(&format!("configure roundtrip: {e}")));

    while app.running {
        event_queue.flush().ok();

        if let Some(guard) = event_queue.prepare_read() {
            let wl_fd = guard.connection_fd().as_raw_fd();
            let pad_fd = app.pad.as_inner().as_raw_fd();
            let mut fds = [
                libc::pollfd { fd: wl_fd, events: libc::POLLIN, revents: 0 },
                libc::pollfd { fd: pad_fd, events: libc::POLLIN, revents: 0 },
            ];

            let ret = unsafe { libc::poll(fds.as_mut_ptr(), fds.len() as libc::nfds_t, -1) };
            if ret < 0 {
                drop(guard);
                if std::io::Error::last_os_error().kind() == std::io::ErrorKind::Interrupted {
                    continue;
                }
                break;
            }

            if fds[0].revents & libc::POLLIN != 0 {
                let _ = guard.read();
            } else {
                drop(guard);
            }

            if fds[1].revents & libc::POLLIN != 0 {
                for ev in pad::read_pad(&app.pad) {
                    handle_nav(&mut app, ev);
                }
            }
        }

        if event_queue.dispatch_pending(&mut app).is_err() {
            break;
        }

        if app.dirty && app.configured {
            commit(&mut app);
        }
    }
}
