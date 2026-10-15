//! Writes a PNG of the tower as the board would show it: `cargo run -p
//! herdr-panel-scene --example preview -- out.png 2,1,8 [scroll]` shows
//! three workspaces with two, one and eight agents, in varying states, scrolled
//! up by `scroll` art pixels (default 0, the bottom). The first agent is
//! focused.

use std::{env, fs::File, io::BufWriter};

use herdr_panel_protocol::Status;
use herdr_panel_scene::{
    CAR_AT, DECOR_AT, DECORS, DESKS_PER_STOREY, DIGIT_W, GROUND_Y, PLUMBOB_ABOVE, SCALE, animation,
    backdrop, behind_desk, car, decor, digits, lobby, number_at, plumbob, roof, roof_top, slot,
    storey, storey_top, street,
};

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
    let statuses = [
        Status::Working,
        Status::Idle,
        Status::Blocked,
        Status::Done,
        Status::Idle,
        Status::Unknown,
    ];

    // Storeys: (wallpaper, behaviors of its agents), workspaces spilling over.
    let mut storeys: Vec<(usize, Vec<Status>)> = Vec::new();
    let mut n = 0;
    for (ws, &agents) in workspaces.iter().enumerate() {
        let all: Vec<Status> = (0..agents)
            .map(|i| statuses[(n + i) % statuses.len()])
            .collect();
        n += agents;
        for chunk in all.chunks(DESKS_PER_STOREY) {
            storeys.push((ws, chunk.to_vec()));
        }
    }

    let mut img = backdrop();
    let at = |y: i32| y + scroll;
    img.draw(&street(), 0, at(GROUND_Y));
    img.draw(&lobby(), 0, at(storey_top(0)));
    let digits = digits();
    for (k, (ws, agents)) in storeys.iter().enumerate() {
        let level = k + 1;
        let top = at(storey_top(level));
        let lit = agents.iter().any(|s| *s != Status::Idle);
        img.draw(&storey(*ws, lit), 0, top);
        img.draw(&decor(level % DECORS, lit), DECOR_AT.0, top + DECOR_AT.1);
        let label = level.to_string();
        let (nx, ny) = number_at(label.len());
        for (i, d) in label.bytes().enumerate() {
            img.draw(
                &digits[(d - b'0') as usize],
                nx + DIGIT_W * i as i32,
                top + ny,
            );
        }
        for (i, b) in agents.iter().enumerate() {
            let s = slot(i);
            let (frames, _) = animation(*b);
            img.draw(
                &behind_desk(&frames[1 % frames.len()]),
                s.mascot.0,
                top + s.mascot.1,
            );
            if k == 0 && i == 0 {
                img.draw(&plumbob(), s.feet.0 - 2, top + s.feet.1 - PLUMBOB_ABOVE);
                img.draw(&car(), CAR_AT.0, top + CAR_AT.1);
            }
        }
    }
    img.draw(&roof()[0], 0, at(roof_top(storeys.len())));

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
