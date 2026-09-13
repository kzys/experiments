// music is a gamepad-driven Jellyfin audio client for the RG35xx Plus.
//
// Authenticates, then hands off to a fullscreen Wayland UI for browsing
// artists -> albums -> tracks and playing the selected one through mpv. See
// PLAN.md for what's next (Quick Connect login, transcoding/caching, etc).

mod api;
mod browse;
mod pad;
mod ui;
mod util;

struct Args {
    server: String,
    username: Option<String>,
    password: Option<String>,
}

fn usage() -> ! {
    eprintln!(
        "usage: music [-s server] [-u username] [-p password]\n  \
         -s must be given (or $JELLYFIN_SERVER); -u/-p fall back to\n  \
         $JELLYFIN_USER/$JELLYFIN_PASSWORD, then to a prompt (password hidden)."
    );
    std::process::exit(2);
}

fn parse_args() -> Args {
    let mut server = std::env::var("JELLYFIN_SERVER").ok();
    let mut username = std::env::var("JELLYFIN_USER").ok();
    let mut password = std::env::var("JELLYFIN_PASSWORD").ok();

    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "-s" => server = Some(args.next().unwrap_or_else(|| usage())),
            "-u" => username = Some(args.next().unwrap_or_else(|| usage())),
            "-p" => password = Some(args.next().unwrap_or_else(|| usage())),
            _ => usage(),
        }
    }

    let server = server.unwrap_or_else(|| usage()).trim_end_matches('/').to_string();
    Args { server, username, password }
}

fn main() {
    let args = parse_args();
    let username = args.username.unwrap_or_else(|| util::prompt_line("username: "));
    let password = args.password.unwrap_or_else(|| util::prompt_password("password: "));

    // The actual connection attempt happens inside ui::run, once a surface
    // exists to show a failure on — this device's server sits behind a
    // flaky Tailscale link, so that's a routine case, not just a startup one.
    ui::run(args.server, username, password);
}
