//! Writes a PNG of the tower as the board would show it: `cargo run -p
//! herdr-panel-scene --example preview -- out.png 2,1,3` shows three storeys
//! with two, one and three agents, in varying states. The first agent is
//! focused.

use std::{env, fs::File, io::BufWriter};

use herdr_panel_art::{Behavior, Ink, frame};
use herdr_panel_scene::{SCALE, background, car, car_pos, desk, floor, floor_pos, mascot, plumbob, roof, roof_pos, slot};

fn ink(ink: Ink) -> [u8; 4] {
    match ink {
        Ink::Body => [0xd9, 0x77, 0x57, 0xff],
        Ink::Zzz => [0x29, 0xad, 0xff, 0xff],
        Ink::Alert => [0xff, 0x00, 0x4d, 0xff],
        Ink::Spark => [0xff, 0xec, 0x27, 0xff],
        Ink::Question => [0xc2, 0xc3, 0xc7, 0xff],
    }
}

fn main() {
    let args: Vec<String> = env::args().collect();
    let path = args.get(1).map_or("preview.png", String::as_str);
    let storeys: Vec<usize> = args.get(2).map_or("2,1,3", String::as_str).split(',').map(|s| s.parse().unwrap()).collect();
    let behaviors = [Behavior::Work, Behavior::Sleep, Behavior::Blocked, Behavior::Done, Behavior::Sleep, Behavior::Unknown];

    let mut img = background();
    let mut n = 0;
    for (k, &agents) in storeys.iter().enumerate() {
        let level = k + 1;
        let these: Vec<Behavior> = (0..agents).map(|i| behaviors[(n + i) % behaviors.len()]).collect();
        let lit = these.iter().any(|b| *b != Behavior::Sleep);
        let (x, y) = floor_pos(level);
        img.draw(&floor(level, lit), x, y);
        for (i, b) in these.iter().enumerate() {
            let s = slot(level, i);
            img.draw(&mascot(&frame(*b, 0, 1), ink), s.mascot.0, s.mascot.1);
            img.draw(&desk(), s.desk.0, s.desk.1);
            if n + i == 0 {
                img.draw(&plumbob(), s.feet.0 - 2, s.feet.1 - 20);
                let (cx, cy) = car_pos(level);
                img.draw(&car(), cx, cy);
            }
        }
        n += agents;
    }
    let (rx, ry) = roof_pos(storeys.len());
    img.draw(&roof()[0], rx, ry);

    let img = img.scaled(SCALE, SCALE);
    let mut enc = png::Encoder::new(BufWriter::new(File::create(path).unwrap()), img.w as u32, img.h as u32);
    enc.set_color(png::ColorType::Rgba);
    enc.set_depth(png::BitDepth::Eight);
    enc.write_header().unwrap().write_image_data(img.px.as_flattened()).unwrap();
}
