//! Writes a PNG of the tower as the board would show it: `cargo run -p
//! herdr-panel-scene --example preview -- out.png 2,1,8 [scroll] [time]
//! [hours]` shows three workspaces with two, one and eight agents, in spaces
//! of four desks, two to a storey, in varying
//! states, scrolled up by `scroll` art pixels (default 0, the bottom), at
//! `time` of day (dawn, day, sunset or night, the default) with the clouds
//! drifted for `hours` hours (default 0). The first agent is focused.
//! Workspaces take their labels from a made-up list in turn.

use std::{env, fs::File, io::BufWriter};

use herdr_panel_protocol::{Status, TimeOfDay};
use herdr_panel_scene::{
    CAR_AT, DECOR_AT, DECORS, DESKS_PER_SPACE, DIGIT_W, GROUND_Y, PLUMBOB_ABOVE, SCALE, SIGN_HALO,
    SPACE_X, SPACES_PER_STOREY, TICKER, animation, backdrop, behind_desk, car, cloud_at, clouds,
    decor, digits, label, label_at, lobby, number_at, plumbob, roof, roof_top, sign, sign_board,
    slot, space, storey_top, street, ticker_text,
};

/// Workspace labels.
const NAMES: [&str; 5] = [
    "herdr-panel",
    "dotfiles",
    "~",
    "statusline",
    "jumpy fog: QRS 0123",
];

fn main() {
    let args: Vec<String> = env::args().collect();
    let path = args.get(1).map_or("preview.png", String::as_str);
    let workspaces: Vec<usize> = args
        .get(2)
        .map_or("2,1,8", String::as_str)
        .split(',')
        .map(|s| s.parse().unwrap())
        .collect();
    let scroll: i32 = args.get(3).map_or(0, |s| s.parse().unwrap());
    let time = args.get(4).map_or(TimeOfDay::Night, |s| {
        *TimeOfDay::ALL
            .iter()
            .find(|t| t.name() == s)
            .unwrap_or_else(|| panic!("no time of day {s}"))
    });
    let hours: u32 = args.get(5).map_or(0, |s| s.parse().unwrap());
    let statuses = [
        Status::Working,
        Status::Idle,
        Status::Blocked,
        Status::Done,
        Status::Idle,
        Status::Unknown,
    ];

    // Spaces: (wallpaper, behaviors of its agents), workspaces spilling over,
    // two to a storey.
    let mut spaces: Vec<(usize, Vec<Status>)> = Vec::new();
    let mut n = 0;
    for (ws, &agents) in workspaces.iter().enumerate() {
        let all: Vec<Status> = (0..agents)
            .map(|i| statuses[(n + i) % statuses.len()])
            .collect();
        n += agents;
        for chunk in all.chunks(DESKS_PER_SPACE) {
            spaces.push((ws, chunk.to_vec()));
        }
    }
    let storeys = spaces.len().div_ceil(SPACES_PER_STOREY);

    let mut img = backdrop(time);
    for (k, cloud) in clouds(time).iter().enumerate() {
        let (cx, cy) = cloud_at(k, hours);
        img.draw(cloud, cx, cy);
    }
    let at = |y: i32| y + scroll;
    img.draw(&street(), 0, at(GROUND_Y));
    img.draw(&lobby(), 0, at(storey_top(0)));
    let (tx, ty, _, _) = TICKER;
    img.draw(
        &ticker_text("dotfiles: reviewer is waiting"),
        tx + 1,
        at(storey_top(0)) + ty + 1,
    );
    let digits = digits();
    for k in 0..storeys {
        let level = k + 1;
        let top = at(storey_top(level));
        let pair = &spaces[k * SPACES_PER_STOREY..spaces.len().min((k + 1) * SPACES_PER_STOREY)];
        let mut decor_lit = false;
        for side in 0..SPACES_PER_STOREY {
            // An empty right space has the left one's wallpaper.
            let (ws, agents) = pair.get(side).cloned().unwrap_or((pair[0].0, Vec::new()));
            let lit = agents.iter().any(|s| *s != Status::Idle);
            decor_lit = lit;
            img.draw(&space(ws, lit, time, side), SPACE_X[side], top);
            if !agents.is_empty() {
                let name = NAMES[ws % NAMES.len()];
                let (lx, ly) = label_at(name.len(), side);
                img.draw(&label(name), lx - 1, top + ly - 1);
            }
            for (i, b) in agents.iter().enumerate() {
                let s = slot(side * DESKS_PER_SPACE + i);
                let (frames, _) = animation(*b);
                img.draw(
                    &behind_desk(&frames[1 % frames.len()]),
                    s.mascot.0,
                    top + s.mascot.1,
                );
                if k == 0 && side == 0 && i == 0 {
                    img.draw(&plumbob(), s.feet.0 - 2, top + s.feet.1 - PLUMBOB_ABOVE);
                    img.draw(&car(), CAR_AT.0, top + CAR_AT.1);
                }
            }
        }
        img.draw(
            &decor(level % DECORS, decor_lit, time),
            DECOR_AT.0,
            top + DECOR_AT.1,
        );
        let number = level.to_string();
        let (nx, ny) = number_at(number.len());
        for (i, d) in number.bytes().enumerate() {
            img.draw(
                &digits[(d - b'0') as usize],
                nx + DIGIT_W * i as i32,
                top + ny,
            );
        }
    }
    img.draw(&roof()[0], 0, at(roof_top(storeys)));
    let text = "HERDR 0.9.3";
    let (sx, sy, _, _) = sign_board(text.len());
    img.draw(
        &sign(text),
        sx - SIGN_HALO.0,
        at(roof_top(storeys)) + sy - SIGN_HALO.1,
    );

    let img = img.scaled(SCALE, SCALE);
    let mut enc = png::Encoder::new(
        BufWriter::new(File::create(path).unwrap()),
        img.w as u32,
        img.h as u32,
    );
    enc.set_color(png::ColorType::Rgba);
    enc.set_depth(png::BitDepth::Eight);
    enc.write_header()
        .unwrap()
        .write_image_data(img.px.as_flattened())
        .unwrap();
}
