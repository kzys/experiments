//! Links the ESP-HAL linker script and turns the art into LVGL images: the
//! tower's background, storeys, roof, desk and elevator from the scene crate,
//! and each mascot frame from the art crate, are written as PNGs, converted with LVGL's `LVGLImage.py`
//! (needs Python with `pypng` and `lz4`) into `lv_image_dsc_t`s, and compiled
//! in. `frames.rs`, included by the firmware, declares them, lists each
//! animation's frames and says where everything goes.

use std::{
    env,
    fmt::Write as _,
    fs::{self, File},
    io::BufWriter,
    path::{Path, PathBuf},
    process::Command,
};

use herdr_panel_art::{Behavior, Ink, cycle, frame};
use herdr_panel_scene::{
    DESKS_PER_FLOOR, Image, MAX_FLOORS, SCALE, background, car, car_pos, desk, floor, floor_pos, mascot, plumbob, roof,
    roof_pos, slot,
};
use oxivgl_build::ImageConfig;

/// The behaviors the firmware animates, with the names used in symbols.
const ANIMATIONS: [(Behavior, &str); 5] = [
    (Behavior::Work, "work"),
    (Behavior::Sleep, "sleep"),
    (Behavior::Blocked, "blocked"),
    (Behavior::Done, "done"),
    (Behavior::Unknown, "unknown"),
];

fn color(ink: Ink) -> [u8; 4] {
    match ink {
        Ink::Body => [0xd9, 0x77, 0x57, 0xff],
        Ink::Zzz => [0x29, 0xad, 0xff, 0xff],
        Ink::Alert => [0xff, 0x00, 0x4d, 0xff],
        Ink::Spark => [0xff, 0xec, 0x27, 0xff],
        Ink::Question => [0xc2, 0xc3, 0xc7, 0xff],
    }
}

/// Writes `img` at screen size, with alpha only if `alpha`.
fn write_png(path: &Path, img: &Image, alpha: bool) {
    let img = img.scaled(SCALE, SCALE);
    let data: Vec<u8> = if alpha {
        img.px.as_flattened().to_vec()
    } else {
        img.px.iter().flat_map(|p| [p[0], p[1], p[2]]).collect()
    };
    let mut encoder = png::Encoder::new(BufWriter::new(File::create(path).unwrap()), img.w as u32, img.h as u32);
    encoder.set_color(if alpha { png::ColorType::Rgba } else { png::ColorType::Rgb });
    encoder.set_depth(png::BitDepth::Eight);
    encoder.write_header().unwrap().write_image_data(&data).unwrap();
}

/// Converts `png` to a C image named `symbol` in colour format `cf` and
/// compiles it.
fn convert(cfg: &ImageConfig, out: &Path, symbol: &str, png: &Path, cf: &str) {
    let status = Command::new("python3")
        .arg(&cfg.converter)
        .args(["--ofmt", "C", "--cf", cf, "--align", "1", "--name", symbol, "-o"])
        .arg(out)
        .arg(png)
        .status()
        .expect("cannot run LVGLImage.py");
    assert!(status.success(), "LVGLImage.py failed for {symbol}");
    cc::Build::new()
        .file(out.join(format!("{symbol}.c")))
        .define("LV_LVGL_H_INCLUDE_SIMPLE", None)
        .include(&cfg.lvgl_include_dir)
        .include(&cfg.lv_conf_dir)
        .opt_level(2)
        .compile(&format!("lvgl_img_{symbol}"));
}

fn main() {
    println!("cargo:rustc-link-arg=-Tlinkall.x");
    println!("cargo:rerun-if-changed=../art/src/lib.rs");
    println!("cargo:rerun-if-changed=../scene/src/lib.rs");
    println!("cargo:rerun-if-changed=build.rs");

    let cfg = ImageConfig::from_env();
    let out = PathBuf::from(env::var("OUT_DIR").unwrap());
    fs::create_dir_all(&out).unwrap();

    let mut declarations = String::new();
    let mut loops = String::new();
    for (behavior, name) in ANIMATIONS {
        let c = cycle(behavior);
        let mut pointers = String::new();
        for i in 0..c.frames {
            let symbol = format!("mascot_{name}_{i}");
            let png = out.join(format!("{symbol}.png"));
            write_png(&png, &mascot(&frame(behavior, 0, i * c.tick_step), color), true);
            convert(&cfg, &out, &symbol, &png, "ARGB8888");
            writeln!(declarations, "oxivgl::image_declare!({symbol});").unwrap();
            write!(pointers, "{symbol}() as *const _ as *const core::ffi::c_void, ").unwrap();
        }
        writeln!(
            loops,
            "        (Behavior::{behavior:?}, Loop {{ frames: Box::leak(Box::new([{pointers}])), duration_ms: {} }}),",
            c.frames * c.frame_ms
        )
        .unwrap();
    }

    // The plumbob bobs: one art pixel up and down.
    let bob = plumbob();
    let mut pointers = String::new();
    for (i, dy) in [0, 1, 2, 1].into_iter().enumerate() {
        let mut img = Image::new(bob.w, bob.h + 2);
        img.draw(&bob, 0, dy);
        let symbol = format!("plumbob_{i}");
        let png = out.join(format!("{symbol}.png"));
        write_png(&png, &img, true);
        convert(&cfg, &out, &symbol, &png, "ARGB8888");
        writeln!(declarations, "oxivgl::image_declare!({symbol});").unwrap();
        write!(pointers, "{symbol}() as *const _ as *const core::ffi::c_void, ").unwrap();
    }
    // Opaque images are RGB565, like the panel; sprites keep their alpha.
    let mut image = |symbol: &str, img: &Image, alpha: bool| {
        let png = out.join(format!("{symbol}.png"));
        write_png(&png, img, alpha);
        convert(&cfg, &out, symbol, &png, if alpha { "ARGB8888" } else { "RGB565" });
        writeln!(declarations, "oxivgl::image_declare!({symbol});").unwrap();
    };
    image("tower", &background(), false);
    image("desk", &desk(), true);
    image("car", &car(), true);
    let mut floors = String::new();
    for level in 1..=MAX_FLOORS {
        for lit in [false, true] {
            let symbol = format!("floor_{level}_{}", if lit { "lit" } else { "dark" });
            image(&symbol, &floor(level, lit), false);
            writeln!(floors, "        ({level}, {lit}) => {symbol}(),").unwrap();
        }
    }
    let mut roof_frames = String::new();
    for (i, img) in roof().iter().enumerate() {
        let symbol = format!("roof_{i}");
        image(&symbol, img, true);
        write!(roof_frames, "{symbol}() as *const _ as *const core::ffi::c_void, ").unwrap();
    }

    let scale = SCALE as i32;
    let at = |(x, y): (i32, i32)| format!("({}, {})", x * scale, y * scale);
    let levels = 0..=MAX_FLOORS;
    let floor_pos: Vec<String> = levels.clone().map(|l| at(floor_pos(l))).collect();
    let roof_pos: Vec<String> = levels.clone().map(|l| at(roof_pos(l))).collect();
    let car_pos: Vec<String> = levels.map(|l| at(car_pos(l))).collect();
    let mut slots = String::new();
    for level in 1..=MAX_FLOORS {
        slots.push_str("    [\n");
        for i in 0..DESKS_PER_FLOOR {
            let s = slot(level, i);
            writeln!(slots, "        Slot {{ feet: {}, mascot: {}, desk: {} }},", at(s.feet), at(s.mascot), at(s.desk)).unwrap();
        }
        slots.push_str("    ],\n");
    }
    let source = format!(
        "{declarations}\n\
         /// The animation for each behavior. Call once: it leaks the frame lists, which\n\
         /// LVGL keeps pointers to.\n\
         pub fn loops() -> [(Behavior, Loop); {n}] {{\n    [\n{loops}    ]\n}}\n\n\
         /// The marker over the focused agent, bobbing. Call once, like [`loops`].\n\
         pub fn plumbob() -> Loop {{\n    Loop {{ frames: Box::leak(Box::new([{pointers}])), duration_ms: 1200 }}\n}}\n\n\
         /// The roof, its antenna light blinking. Call once, like [`loops`].\n\
         pub fn roof() -> Loop {{\n    Loop {{ frames: Box::leak(Box::new([{roof_frames}])), duration_ms: 1600 }}\n}}\n\n\
         /// The sky, the street and the lobby, filling the screen.\n\
         pub fn tower_image() -> &'static oxivgl::widgets::lv_image_dsc_t {{ tower() }}\n\n\
         /// A desk, drawn over the agent standing behind it.\n\
         pub fn desk_image() -> &'static oxivgl::widgets::lv_image_dsc_t {{ desk() }}\n\n\
         /// The elevator car.\n\
         pub fn car_image() -> &'static oxivgl::widgets::lv_image_dsc_t {{ car() }}\n\n\
         /// Storey `level` (1..=[`MAX_FLOORS`]), with its lights on or off.\n\
         pub fn floor_image(level: usize, lit: bool) -> &'static oxivgl::widgets::lv_image_dsc_t {{\n\
         \x20   match (level.clamp(1, MAX_FLOORS), lit) {{\n{floors}        _ => unreachable!(),\n    }}\n}}\n\n\
         /// Storeys that fit above the lobby, and desks on each.\n\
         pub const MAX_FLOORS: usize = {MAX_FLOORS};\n\
         pub const DESKS_PER_FLOOR: usize = {DESKS_PER_FLOOR};\n\n\
         /// Top left of storey `level`, by level (0 is the lobby).\n\
         pub const FLOOR_POS: [(i32, i32); {m}] = [{floor_pos}];\n\n\
         /// Top left of the roof over a tower with that many storeys above the lobby.\n\
         pub const ROOF_POS: [(i32, i32); {m}] = [{roof_pos}];\n\n\
         /// Top left of the elevator car when it stops at each level.\n\
         pub const CAR_POS: [(i32, i32); {m}] = [{car_pos}];\n\n\
         /// The desks on storeys 1..=[`MAX_FLOORS`], left to right.\n\
         pub const SLOTS: [[Slot; {DESKS_PER_FLOOR}]; {MAX_FLOORS}] = [\n{slots}];\n",
        n = ANIMATIONS.len(),
        m = MAX_FLOORS + 1,
        floor_pos = floor_pos.join(", "),
        roof_pos = roof_pos.join(", "),
        car_pos = car_pos.join(", "),
    );
    fs::write(out.join("frames.rs"), source).unwrap();
}
