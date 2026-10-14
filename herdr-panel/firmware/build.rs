//! Links the ESP-HAL linker script and turns the art into LVGL images: the
//! tower's backdrop, street, lobby, storeys, digits, roof and elevator, which
//! the scene crate puts together from the PNGs in `assets/`, and each mascot
//! frame (cut where its desk hides it). Each is written as
//! a PNG at screen size, converted with LVGL's `LVGLImage.py` (needs Python
//! with `pypng` and `lz4`) into an `lv_image_dsc_t`, and compiled in.
//! `frames.rs`, included by the firmware, declares them, lists each
//! animation's frames and says where everything goes.

use std::{
    env,
    fmt::Write as _,
    fs::{self, File},
    io::BufWriter,
    path::{Path, PathBuf},
    process::Command,
};

use herdr_panel_protocol::Status;
use herdr_panel_scene::{
    ARROW_SIZE, ARROW_X, CAR_AT, DECOR_AT, DECORS, DESK_SPACING, DESKS_PER_STOREY, DESKS_X,
    GROUND_Y, Image, PLUMBOB_ABOVE, ROOF_H, SCALE, STOREY_H, WALLPAPERS, animation, arrow,
    awake_animation, backdrop, behind_desk, car, decor, digits, lobby, number_at, plumbob, roof,
    slot, storey, street,
};
use oxivgl_build::ImageConfig;

/// The statuses the mascot has an animation for, with the names used in
/// symbols.
const ANIMATIONS: [(Status, &str); 5] = [
    (Status::Working, "working"),
    (Status::Idle, "idle"),
    (Status::Blocked, "blocked"),
    (Status::Done, "done"),
    (Status::Unknown, "unknown"),
];

/// Writes `img` at screen size, with alpha only if `alpha`.
fn write_png(path: &Path, img: &Image, alpha: bool) {
    let img = img.scaled(SCALE, SCALE);
    let data: Vec<u8> = if alpha {
        img.px.as_flattened().to_vec()
    } else {
        img.px.iter().flat_map(|p| [p[0], p[1], p[2]]).collect()
    };
    let mut encoder = png::Encoder::new(
        BufWriter::new(File::create(path).unwrap()),
        img.w as u32,
        img.h as u32,
    );
    encoder.set_color(if alpha {
        png::ColorType::Rgba
    } else {
        png::ColorType::Rgb
    });
    encoder.set_depth(png::BitDepth::Eight);
    encoder
        .write_header()
        .unwrap()
        .write_image_data(&data)
        .unwrap();
}

/// Converts `png` to a C image named `symbol` in colour format `cf` and
/// compiles it.
fn convert(cfg: &ImageConfig, out: &Path, symbol: &str, png: &Path, cf: &str) {
    let status = Command::new("python3")
        .arg(&cfg.converter)
        .args([
            "--ofmt", "C", "--cf", cf, "--align", "1", "--name", symbol, "-o",
        ])
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
    println!("cargo:rerun-if-changed=../scene/src/lib.rs");
    println!("cargo:rerun-if-changed=../assets");
    println!("cargo:rerun-if-changed=build.rs");

    let cfg = ImageConfig::from_env();
    let out = PathBuf::from(env::var("OUT_DIR").unwrap());
    fs::create_dir_all(&out).unwrap();

    let mut declarations = String::new();
    let mut loops = String::new();
    for (status, name) in ANIMATIONS {
        let (frames, frame_ms) = animation(status);
        let mut pointers = String::new();
        for (i, frame) in frames.iter().enumerate() {
            let symbol = format!("mascot_{name}_{i}");
            let png = out.join(format!("{symbol}.png"));
            write_png(&png, &behind_desk(frame), true);
            convert(&cfg, &out, &symbol, &png, "ARGB8888");
            writeln!(declarations, "oxivgl::image_declare!({symbol});").unwrap();
            write!(
                pointers,
                "{symbol}() as *const _ as *const core::ffi::c_void, "
            )
            .unwrap();
        }
        writeln!(
            loops,
            "        (Status::{status:?}, Loop {{ frames: Box::leak(Box::new([{pointers}])), duration_ms: {} }}),",
            frames.len() as u32 * frame_ms
        )
        .unwrap();
    }
    let (frames, frame_ms) = awake_animation();
    let mut awake = String::new();
    for (i, frame) in frames.iter().enumerate() {
        let symbol = format!("mascot_awake_{i}");
        let png = out.join(format!("{symbol}.png"));
        write_png(&png, &behind_desk(frame), true);
        convert(&cfg, &out, &symbol, &png, "ARGB8888");
        writeln!(declarations, "oxivgl::image_declare!({symbol});").unwrap();
        write!(
            awake,
            "{symbol}() as *const _ as *const core::ffi::c_void, "
        )
        .unwrap();
    }
    let awake_ms = frames.len() as u32 * frame_ms;

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
        write!(
            pointers,
            "{symbol}() as *const _ as *const core::ffi::c_void, "
        )
        .unwrap();
    }
    // Opaque images are RGB565, like the panel; sprites keep their alpha.
    let mut image = |symbol: &str, img: &Image, alpha: bool| {
        let png = out.join(format!("{symbol}.png"));
        write_png(&png, img, alpha);
        convert(
            &cfg,
            &out,
            symbol,
            &png,
            if alpha { "ARGB8888" } else { "RGB565" },
        );
        writeln!(declarations, "oxivgl::image_declare!({symbol});").unwrap();
    };
    image("backdrop", &backdrop(), false);
    image("street", &street(), false);
    image("lobby", &lobby(), false);
    image("car", &car(), true);
    image("arrow_up", &arrow(true), true);
    image("arrow_down", &arrow(false), true);
    let mut storeys = String::new();
    for paper in 0..WALLPAPERS {
        for lit in [false, true] {
            let symbol = format!("storey_{paper}_{}", if lit { "lit" } else { "dark" });
            image(&symbol, &storey(paper, lit), false);
            writeln!(storeys, "        ({paper}, {lit}) => {symbol}(),").unwrap();
        }
    }
    let mut decors = String::new();
    for kind in 0..DECORS {
        for lit in [false, true] {
            let symbol = format!("decor_{kind}_{}", if lit { "lit" } else { "dark" });
            image(&symbol, &decor(kind, lit), true);
            writeln!(decors, "        ({kind}, {lit}) => {symbol}(),").unwrap();
        }
    }
    let mut digit_arms = String::new();
    for (d, img) in digits().iter().enumerate() {
        image(&format!("digit_{d}"), img, true);
        writeln!(digit_arms, "        {d} => digit_{d}(),").unwrap();
    }
    let mut roof_frames = String::new();
    for (i, img) in roof().iter().enumerate() {
        let symbol = format!("roof_{i}");
        image(&symbol, img, true);
        write!(
            roof_frames,
            "{symbol}() as *const _ as *const core::ffi::c_void, "
        )
        .unwrap();
    }

    let scale = SCALE as i32;
    let at = |(x, y): (i32, i32)| format!("({}, {})", x * scale, y * scale);
    let mut slots = String::new();
    for i in 0..DESKS_PER_STOREY {
        let s = slot(i);
        writeln!(
            slots,
            "    Slot {{ feet: {}, mascot: {} }},",
            at(s.feet),
            at(s.mascot)
        )
        .unwrap();
    }
    let number_at: Vec<String> = (1..=3).map(|len| at(number_at(len))).collect();
    let image_fn = |name: &str, doc: &str| {
        format!(
            "/// {doc}\npub fn {name}_image() -> &'static oxivgl::widgets::lv_image_dsc_t {{ {name}() }}\n\n"
        )
    };
    let source = format!(
        "{declarations}\n\
         /// The animation for each agent status. Call once: it leaks the frame lists, which\n\
         /// LVGL keeps pointers to.\n\
         pub fn loops() -> [(Status, Loop); {n}] {{\n    [\n{loops}    ]\n}}\n\n\
         /// The animation for an idle agent with the focus: awake, blinking. Call\n\
         /// once, like [`loops`].\n\
         pub fn awake() -> Loop {{\n    Loop {{ frames: Box::leak(Box::new([{awake}])), duration_ms: {awake_ms} }}\n}}\n\n\
         /// The marker over the focused agent, bobbing. Call once, like [`loops`].\n\
         pub fn plumbob() -> Loop {{\n    Loop {{ frames: Box::leak(Box::new([{pointers}])), duration_ms: 1200 }}\n}}\n\n\
         /// The roof, its antenna light blinking. Call once, like [`loops`].\n\
         pub fn roof() -> Loop {{\n    Loop {{ frames: Box::leak(Box::new([{roof_frames}])), duration_ms: 1600 }}\n}}\n\n\
         {backdrop}{street}{lobby}{car}{arrow_up}{arrow_down}\
         /// A storey in wallpaper `paper` (modulo [`WALLPAPERS`]), its lights on or off.\n\
         pub fn storey_image(paper: usize, lit: bool) -> &'static oxivgl::widgets::lv_image_dsc_t {{\n\
         \x20   match (paper % WALLPAPERS, lit) {{\n{storeys}        _ => unreachable!(),\n    }}\n}}\n\n\
         /// Decor `kind` (modulo [`DECORS`]) for a storey's right end, lit or not.\n\
         pub fn decor_image(kind: usize, lit: bool) -> &'static oxivgl::widgets::lv_image_dsc_t {{\n\
         \x20   match (kind % DECORS, lit) {{\n{decors}        _ => unreachable!(),\n    }}\n}}\n\n\
         /// Digit `d` (modulo 10) of a storey number.\n\
         pub fn digit_image(d: usize) -> &'static oxivgl::widgets::lv_image_dsc_t {{\n\
         \x20   match d % 10 {{\n{digit_arms}        _ => unreachable!(),\n    }}\n}}\n\n\
         /// Layout, in screen pixels, with the view scrolled to the bottom: y\n\
         /// grows downwards from the top of the screen, so storeys high up have\n\
         /// negative tops.\n\
         pub const GROUND_Y: i32 = {ground};\n\
         pub const STOREY_H: i32 = {storey_h};\n\
         pub const ROOF_H: i32 = {roof_h};\n\
         pub const WALLPAPERS: usize = {WALLPAPERS};\n\
         pub const DECORS: usize = {DECORS};\n\
         /// Where a storey's decor goes, relative to the storey.\n\
         pub const DECOR_AT: (i32, i32) = {decor_at};\n\
         pub const DESKS_PER_STOREY: usize = {DESKS_PER_STOREY};\n\
         /// Where desks start in a storey, and how far apart they are.\n\
         pub const DESKS_X: i32 = {desks_x};\n\
         pub const DESK_SPACING: i32 = {desk_spacing};\n\
         /// Where the car stops in a storey, relative to the storey.\n\
         pub const CAR_AT: (i32, i32) = {car_at};\n\
         /// The left edge and size of the scroll buttons, over the shaft.\n\
         pub const ARROW_X: i32 = {arrow_x};\n\
         pub const ARROW_SIZE: i32 = {arrow_size};\n\
         /// The plumbob's width.\n\
         pub const PLUMBOB_W: i32 = {plumbob_w};\n\
         /// How far the plumbob's top is above the agent's feet.\n\
         pub const PLUMBOB_ABOVE: i32 = {plumbob_above};\n\
         /// Where a storey number's first digit goes, for numbers of 1 to 3\n\
         /// digits; each digit is `DIGIT_W` pixels wide.\n\
         pub const NUMBER_AT: [(i32, i32); 3] = [{number_at}];\n\
         pub const DIGIT_W: i32 = {digit_w};\n\
         /// Where the agents at each desk go, relative to the storey.\n\
         pub const SLOTS: [Slot; {DESKS_PER_STOREY}] = [\n{slots}];\n",
        n = ANIMATIONS.len(),
        backdrop = image_fn(
            "backdrop",
            "The sky and the city, filling the screen; it stays still."
        ),
        street = image_fn(
            "street",
            "The street, from `GROUND_Y` to the bottom of the screen."
        ),
        lobby = image_fn("lobby", "The lobby, storey 0."),
        car = image_fn("car", "The elevator car."),
        arrow_up = image_fn("arrow_up", "The button that scrolls up a storey."),
        arrow_down = image_fn("arrow_down", "The button that scrolls down a storey."),
        arrow_x = ARROW_X * scale,
        arrow_size = ARROW_SIZE as i32 * scale,
        plumbob_w = plumbob().w as i32 * scale,
        digit_w = digits()[0].w as i32 * scale,
        ground = GROUND_Y * scale,
        storey_h = STOREY_H as i32 * scale,
        roof_h = ROOF_H as i32 * scale,
        desks_x = DESKS_X * scale,
        desk_spacing = DESK_SPACING * scale,
        car_at = at(CAR_AT),
        decor_at = at(DECOR_AT),
        plumbob_above = PLUMBOB_ABOVE * scale,
        number_at = number_at.join(", "),
    );
    fs::write(out.join("frames.rs"), source).unwrap();
}
