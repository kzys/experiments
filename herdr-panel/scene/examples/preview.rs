//! Writes a PNG of the tower as the board would show it: `cargo run -p
//! herdr-panel-scene --example preview -- out.png 2,1,8 [scroll]` shows
//! three workspaces with two, one and eight agents, in varying states, scrolled
//! up by `scroll` art pixels (default 0, the bottom). The first agent is
//! focused.

use std::{env, fs::File, io::BufWriter};

use herdr_panel_art::{Behavior, Ink, frame};
use herdr_panel_scene::{
    CAR_AT, DESKS_PER_STOREY, GROUND_Y, PLUMBOB_ABOVE, SCALE, backdrop, behind_desk, car, digits, lobby,
    mascot, number_at, plumbob, roof, roof_top, slot, storey, storey_top, street,
};

fn ink(ink: Ink) -> [u8; 4] {
    match ink {
        Ink::Body | Ink::Lid => [0xd9, 0x77, 0x57, 0xff],
        Ink::Zzz => [0x29, 0xad, 0xff, 0xff],
        Ink::Alert => [0xff, 0x00, 0x4d, 0xff],
        Ink::Spark => [0xff, 0xec, 0x27, 0xff],
        Ink::Question => [0xc2, 0xc3, 0xc7, 0xff],
    }
}

fn main() {
    let args: Vec<String> = env::args().collect();
    let path = args.get(1).map_or("preview.png", String::as_str);
    let workspaces: Vec<usize> = args.get(2).map_or("2,1,8", String::as_str).split(',').map(|s| s.parse().unwrap()).collect();
    let scroll: i32 = args.get(3).map_or(0, |s| s.parse().unwrap());
    let behaviors = [Behavior::Work, Behavior::Sleep, Behavior::Blocked, Behavior::Done, Behavior::Sleep, Behavior::Unknown];

    // Storeys: (wallpaper, behaviors of its agents), workspaces spilling over.
    let mut storeys: Vec<(usize, Vec<Behavior>)> = Vec::new();
    let mut n = 0;
    for (ws, &agents) in workspaces.iter().enumerate() {
        let all: Vec<Behavior> = (0..agents).map(|i| behaviors[(n + i) % behaviors.len()]).collect();
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
        let lit = agents.iter().any(|b| *b != Behavior::Sleep);
        img.draw(&storey(*ws, lit), 0, top);
        let label = level.to_string();
        let (nx, ny) = number_at(label.len());
        for (i, d) in label.bytes().enumerate() {
            img.draw(&digits[(d - b'0') as usize], nx + 4 * i as i32, top + ny);
        }
        for (i, b) in agents.iter().enumerate() {
            let s = slot(i);
            img.draw(&behind_desk(&mascot(&frame(*b, 0, 1), ink)), s.mascot.0, top + s.mascot.1);
            if k == 0 && i == 0 {
                img.draw(&plumbob(), s.feet.0 - 2, top + s.feet.1 - PLUMBOB_ABOVE);
                img.draw(&car(), CAR_AT.0, top + CAR_AT.1);
            }
        }
    }
    img.draw(&roof()[0], 0, at(roof_top(storeys.len())));

    let img = img.scaled(SCALE, SCALE);
    let mut enc = png::Encoder::new(BufWriter::new(File::create(path).unwrap()), img.w as u32, img.h as u32);
    enc.set_color(png::ColorType::Rgba);
    enc.set_depth(png::BitDepth::Eight);
    enc.write_header().unwrap().write_image_data(img.px.as_flattened()).unwrap();
}
