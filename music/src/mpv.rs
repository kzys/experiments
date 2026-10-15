// One long-lived mpv, started idle and driven over its JSON IPC socket
// (--input-ipc-server). Tracks go into its playlist rather than each
// getting a process of their own, so it advances through an album by
// itself, and pause/skip don't restart anything. A reader thread forwards
// what mpv reports back — which playlist entry is playing, how far in,
// whether it's paused — to the event loop, so the UI shows mpv's real
// state instead of guessing from wall-clock time.

use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::UnixStream;
use std::path::PathBuf;
use std::process::{Child, Command};
use std::time::{Duration, Instant};

use serde_json::json;

// How long to wait for mpv to create its socket after being spawned.
const CONNECT_TIMEOUT: Duration = Duration::from_secs(5);

pub enum Event {
    // Which entry of the loaded playlist started playing.
    Track(usize),
    // Whole seconds into the current entry; sent once per second.
    Position(u64),
    Paused(bool),
    // The playlist ran out (or nothing was ever loaded).
    Idle,
    // The process is gone; the next play starts a new one.
    Exited,
}

pub struct Mpv {
    child: Child,
    socket: UnixStream,
    path: PathBuf,
}

impl Mpv {
    // Spawns mpv and connects to its socket, blocking until it's ready to
    // take commands. Every message mpv sends afterwards reaches on_event
    // from a background thread, ending with Event::Exited.
    pub fn spawn(on_event: impl Fn(Event) + Send + 'static) -> Result<Mpv, String> {
        let path = socket_path();
        // A stale socket from a previous run that didn't exit cleanly.
        std::fs::remove_file(&path).ok();
        let mut child = Command::new("mpv")
            .arg("--no-video")
            .arg("--no-terminal")
            .arg("--idle=yes")
            // Open the next entry before the current one ends, so a network
            // stream doesn't leave a gap between tracks.
            .arg("--prefetch-playlist=yes")
            .arg(format!("--input-ipc-server={}", path.display()))
            .spawn()
            .map_err(|e| format!("failed to run mpv: {e}"))?;

        // The socket appears asynchronously once mpv is up.
        let deadline = Instant::now() + CONNECT_TIMEOUT;
        let socket = loop {
            match UnixStream::connect(&path) {
                Ok(socket) => break socket,
                Err(e) => {
                    if let Ok(Some(status)) = child.try_wait() {
                        return Err(format!("mpv exited at startup: {status}"));
                    }
                    if Instant::now() >= deadline {
                        child.kill().ok();
                        child.wait().ok();
                        return Err(format!("mpv socket {}: {e}", path.display()));
                    }
                    std::thread::sleep(Duration::from_millis(50));
                }
            }
        };

        let reader = socket.try_clone().map_err(|e| format!("mpv socket: {e}"))?;
        std::thread::spawn(move || {
            let mut last_secs = None;
            for line in BufReader::new(reader).lines() {
                let Ok(line) = line else { break };
                let Ok(msg) = serde_json::from_str::<serde_json::Value>(&line) else { continue };
                if let Some(ev) = parse(&msg, &mut last_secs) {
                    on_event(ev);
                }
            }
            on_event(Event::Exited);
        });

        let mut mpv = Mpv { child, socket, path };
        for (id, name) in [(1, "playlist-pos"), (2, "time-pos"), (3, "pause"), (4, "idle-active")] {
            mpv.command(&[json!("observe_property"), json!(id), json!(name)]);
        }
        Ok(mpv)
    }

    // Replaces the playlist with these URLs and starts on the first.
    pub fn play(&mut self, urls: &[String]) {
        let mut urls = urls.iter();
        let Some(first) = urls.next() else { return };
        self.command(&[json!("loadfile"), json!(first), json!("replace")]);
        for url in urls {
            self.command(&[json!("loadfile"), json!(url), json!("append")]);
        }
        // Pause is a player setting, not a per-file one: a new selection
        // while paused should be heard.
        self.command(&[json!("set_property"), json!("pause"), json!(false)]);
    }

    pub fn toggle_pause(&mut self) {
        self.command(&[json!("cycle"), json!("pause")]);
    }

    // Failures are logged rather than returned: a lost socket means mpv is
    // gone, which the reader thread reports as Event::Exited anyway.
    fn command(&mut self, args: &[serde_json::Value]) {
        let mut line = serde_json::json!({ "command": args }).to_string();
        line.push('\n');
        if let Err(e) = self.socket.write_all(line.as_bytes()) {
            log::warn!("mpv command: {e}");
        }
    }
}

impl Drop for Mpv {
    fn drop(&mut self) {
        self.child.kill().ok();
        self.child.wait().ok();
        std::fs::remove_file(&self.path).ok();
    }
}

// XDG_RUNTIME_DIR is where the Wayland socket already lives, so it's set
// wherever the UI can come up at all; /tmp is just a fallback.
fn socket_path() -> PathBuf {
    let dir = std::env::var_os("XDG_RUNTIME_DIR").map(PathBuf::from).unwrap_or_else(|| PathBuf::from("/tmp"));
    dir.join(format!("music-mpv-{}.sock", std::process::id()))
}

// One line from mpv, as an Event if it's something the UI shows. time-pos
// changes many times a second; only the whole-second changes are passed
// on, which is all the position counter can display.
fn parse(msg: &serde_json::Value, last_secs: &mut Option<u64>) -> Option<Event> {
    match msg["event"].as_str() {
        Some("property-change") => match msg["name"].as_str() {
            Some("playlist-pos") => {
                let pos = usize::try_from(msg["data"].as_i64()?).ok()?;
                *last_secs = None;
                Some(Event::Track(pos))
            }
            Some("time-pos") => {
                let secs = msg["data"].as_f64()? as u64;
                if *last_secs == Some(secs) {
                    return None;
                }
                *last_secs = Some(secs);
                Some(Event::Position(secs))
            }
            Some("pause") => msg["data"].as_bool().map(Event::Paused),
            Some("idle-active") => (msg["data"].as_bool() == Some(true)).then_some(Event::Idle),
            _ => None,
        },
        Some("end-file") if msg["reason"] == "error" => {
            log::warn!("mpv: {}", msg["file_error"].as_str().unwrap_or("playback error"));
            None
        }
        Some(_) => None,
        None => {
            // A command's reply.
            if let Some(err) = msg["error"].as_str()
                && err != "success"
            {
                log::warn!("mpv: {err}");
            }
            None
        }
    }
}
