//! Links the ESP-HAL linker script and turns the mascot animations into LVGL
//! images: each frame from the art crate is written as a PNG, converted with
//! LVGL's `LVGLImage.py` (needs Python with `pypng` and `lz4`) into an
//! `lv_image_dsc_t` in ARGB8888, and compiled in. `frames.rs`, included by the
//! firmware, declares them and lists each animation's frames.

use std::{
    env,
    fmt::Write as _,
    fs::{self, File},
    io::BufWriter,
    path::{Path, PathBuf},
    process::Command,
};

use herdr_panel_art::{Behavior, CANVAS_H, CANVAS_W, Ink, cycle, frame};
use oxivgl_build::ImageConfig;

/// The behaviors the firmware animates, with the names used in symbols.
const ANIMATIONS: [(Behavior, &str); 5] = [
    (Behavior::Work, "work"),
    (Behavior::Sleep, "sleep"),
    (Behavior::Blocked, "blocked"),
    (Behavior::Done, "done"),
    (Behavior::Unknown, "unknown"),
];

/// Screen pixels per mascot pixel, as one wide and two tall.
const PIXEL_H: usize = 2;

fn rgb(ink: Ink) -> [u8; 3] {
    match ink {
        Ink::Body => [0xd9, 0x77, 0x57],
        Ink::Zzz => [0x90, 0xca, 0xf9],
        Ink::Alert => [0xff, 0x52, 0x52],
        Ink::Spark => [0xff, 0xeb, 0x3b],
        Ink::Question => [0xb0, 0xbe, 0xc5],
    }
}

fn write_png(path: &Path, grid: &herdr_panel_art::Grid) {
    let (w, h) = (CANVAS_W, CANVAS_H * PIXEL_H);
    let mut data = Vec::with_capacity(w * h * 4);
    for row in grid {
        for _ in 0..PIXEL_H {
            for pixel in row {
                match pixel {
                    Some(ink) => data.extend_from_slice(&[rgb(*ink)[0], rgb(*ink)[1], rgb(*ink)[2], 0xff]),
                    None => data.extend_from_slice(&[0, 0, 0, 0]),
                }
            }
        }
    }
    let mut encoder = png::Encoder::new(BufWriter::new(File::create(path).unwrap()), w as u32, h as u32);
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);
    encoder.write_header().unwrap().write_image_data(&data).unwrap();
}

/// Converts `png` to a C image named `symbol` and compiles it.
fn convert(cfg: &ImageConfig, out: &Path, symbol: &str, png: &Path) {
    let status = Command::new("python3")
        .arg(&cfg.converter)
        .args(["--ofmt", "C", "--cf", "ARGB8888", "--align", "1", "--name", symbol, "-o"])
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
            write_png(&png, &frame(behavior, 0, i * c.tick_step));
            convert(&cfg, &out, &symbol, &png);
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
    let source = format!(
        "{declarations}\n\
         /// The animation for each behavior. Call once: it leaks the frame lists, which\n\
         /// LVGL keeps pointers to.\n\
         pub fn loops() -> [(Behavior, Loop); {n}] {{\n    [\n{loops}    ]\n}}\n",
        n = ANIMATIONS.len()
    );
    fs::write(out.join("frames.rs"), source).unwrap();
}
