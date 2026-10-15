use std::collections::VecDeque;
use std::io::{self, BufRead, Write};
use std::path::PathBuf;
use std::sync::Mutex;

pub fn die(msg: &str) -> ! {
    eprintln!("{msg}");
    std::process::exit(1);
}

// How many recent lines the on-screen log panel shows (see ui.rs); older
// ones still make it to stderr and the log file, just not the screen.
const VISIBLE_LINES: usize = 10;

static RECENT: Mutex<VecDeque<(log::Level, String)>> = Mutex::new(VecDeque::new());

// A snapshot of the last few log lines, for ui.rs to paint on screen —
// there's no other way to see what's going wrong on a kiosk device with no
// console attached. The level rides along so warnings and errors can be
// colored differently from routine info lines.
pub fn recent_lines() -> Vec<(log::Level, String)> {
    RECENT.lock().unwrap().iter().cloned().collect()
}

// Wall-clock HH:MM:SS in the local timezone: what a person reading the
// on-screen panel can relate to, unlike raw epoch seconds.
fn timestamp() -> String {
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as libc::time_t)
        .unwrap_or(0);
    let mut tm: libc::tm = unsafe { std::mem::zeroed() };
    unsafe { libc::localtime_r(&secs, &mut tm) };
    format!("{:02}:{:02}:{:02}", tm.tm_hour, tm.tm_min, tm.tm_sec)
}

// Launched as an EmulationStation Ports script, this has no console for
// eprintln! to reach — stderr just gets swallowed. So `log`'s output (used
// throughout via its info!/warn!/error! macros) also gets appended to a
// file under XDG_STATE_HOME, the only way to see what happened after the
// fact (e.g. over ssh), and kept in a small in-memory ring buffer the UI
// paints directly on screen. No env_logger/etc: those assume a real
// terminal.
struct FileLogger;

impl log::Log for FileLogger {
    fn enabled(&self, _metadata: &log::Metadata) -> bool {
        true
    }

    fn log(&self, record: &log::Record) {
        let line = format!("[{}] {} {}", timestamp(), record.level(), record.args());
        eprintln!("{line}");

        let mut recent = RECENT.lock().unwrap();
        recent.push_back((record.level(), line.clone()));
        while recent.len() > VISIBLE_LINES {
            recent.pop_front();
        }
        drop(recent);

        let path = log_path();
        if let Some(dir) = path.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        if let Ok(mut f) = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&path)
        {
            let _ = writeln!(f, "{line}");
        }
    }

    fn flush(&self) {}
}

fn log_path() -> PathBuf {
    let dir = std::env::var("XDG_STATE_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|_| {
            PathBuf::from(std::env::var("HOME").unwrap_or_default()).join(".local/state")
        });
    dir.join("music/log")
}

static LOGGER: FileLogger = FileLogger;

pub fn init_logging() {
    log::set_logger(&LOGGER).ok();
    log::set_max_level(log::LevelFilter::Info);
}

pub fn prompt_line(prompt: &str) -> String {
    print!("{prompt}");
    io::stdout().flush().ok();
    let mut line = String::new();
    io::stdin().lock().read_line(&mut line).ok();
    line.trim_end_matches(['\n', '\r']).to_string()
}

// Like prompt_line, but disables terminal echo while the answer is typed, so
// the password doesn't end up on-screen (it can still land in scrollback
// once Enter is pressed and the line is read, but not before). Falls back to
// a plain, visible read if stdin isn't a terminal (tcgetattr fails) — a pipe
// has nothing to echo anyway.
pub fn prompt_password(prompt: &str) -> String {
    print!("{prompt}");
    io::stdout().flush().ok();

    let mut term: libc::termios = unsafe { std::mem::zeroed() };
    let have_tty = unsafe { libc::tcgetattr(libc::STDIN_FILENO, &mut term) } == 0;

    if have_tty {
        let mut hidden = term;
        hidden.c_lflag &= !libc::ECHO;
        unsafe { libc::tcsetattr(libc::STDIN_FILENO, libc::TCSANOW, &hidden) };
    }

    let mut line = String::new();
    io::stdin().lock().read_line(&mut line).ok();

    if have_tty {
        unsafe { libc::tcsetattr(libc::STDIN_FILENO, libc::TCSANOW, &term) };
        println!(); // the newline the hidden Enter keypress didn't produce
    }

    line.trim_end_matches(['\n', '\r']).to_string()
}
