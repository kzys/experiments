use std::io::{self, BufRead, Write};

pub fn die(msg: &str) -> ! {
    eprintln!("{msg}");
    std::process::exit(1);
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
