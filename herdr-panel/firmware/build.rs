//! Links the ESP-HAL linker script and turns the art into LVGL images: the
//! tower's backdrop and clouds and storeys' spaces at each time of day, street,
//! lobby, digits, roof and elevator, which
//! the scene crate puts together from the PNGs in `assets/`, and each mascot
//! frame (cut where its desk hides it). Each is written as
//! a PNG at screen size, converted with LVGL's `LVGLImage.py` (needs Python
//! with `pypng` and `lz4`) into an `lv_image_dsc_t`, and compiled in.
//! The text font, and its outline, become LVGL fonts in C, compiled in too.
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

use herdr_panel_protocol::{Status, TimeOfDay};
use herdr_panel_scene::{
    ARROW_SIZE, ARROW_X, CAR_AT, CLOUDS, DECOR_AT, DECORS, DESK_SPACING, DESKS_PER_SPACE,
    DESKS_PER_STOREY, DESKS_X, FONT_DESCENT, FONT_H, FONT_W, GROUND_Y, Image, LABEL_COLOUR,
    LABEL_ENDS, LABEL_OUTLINE, LABEL_Y, PLUMBOB_ABOVE, ROOF_H, SCALE, SIGN_BOARD, SIGN_CENTRE,
    SIGN_HALO, SIGN_HALO_OPA, SIGN_INK, SIGN_NEON, SPACE_X, SPACES_PER_STOREY, STOREY_H, TICKER,
    TICKER_LED, WALLPAPERS, animation, arrow, awake_animation, backdrop, behind_desk, car, clouds,
    decor, digits, font, lobby, number_at, outline, plumbob, roof, sign_board, slot, space, street,
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

/// Writes an LVGL font named `symbol` for ASCII ' ' to '~' and compiles it.
/// Each of `glyphs` is a character's rows from the top, the leftmost pixel
/// in the top bit, `box_w` (at most 8) wide; its bottom `descent` rows are
/// below the base line. Characters are [`FONT_W`] apart whatever their size,
/// so a font of outlines lines up with the text font.
fn lvgl_font(
    cfg: &ImageConfig,
    out: &Path,
    symbol: &str,
    glyphs: &[&[u8]],
    box_w: usize,
    descent: usize,
) {
    let box_h = glyphs[0].len();
    let mut bitmap = String::new();
    let mut dscs = String::from("    {0},\n");
    let mut index = 0;
    for g in glyphs {
        assert_eq!(g.len(), box_h);
        for row in *g {
            write!(bitmap, "0x{row:02x}, ").unwrap();
        }
        writeln!(
            dscs,
            "    {{.bitmap_index = {index}, .adv_w = {adv}, .box_w = {box_w}, .box_h = {box_h}, .ofs_x = 0, .ofs_y = -{descent}}},",
            adv = FONT_W * 16,
        )
        .unwrap();
        index += box_h;
    }
    let c = format!(
        "#include \"lvgl.h\"\n\n\
         static const uint8_t bitmap[] = {{{bitmap}}};\n\n\
         static const lv_font_fmt_txt_glyph_dsc_t glyphs[] = {{\n{dscs}}};\n\n\
         static const lv_font_fmt_txt_cmap_t cmaps[] = {{\n\
         \x20   {{.range_start = 32, .range_length = {n}, .glyph_id_start = 1, .unicode_list = NULL,\n\
         \x20    .glyph_id_ofs_list = NULL, .list_length = 0, .type = LV_FONT_FMT_TXT_CMAP_FORMAT0_TINY}},\n}};\n\n\
         /* A byte per row: stride 1. */\n\
         static const lv_font_fmt_txt_dsc_t dsc = {{\n\
         \x20   .glyph_bitmap = bitmap, .glyph_dsc = glyphs, .cmaps = cmaps, .kern_dsc = NULL,\n\
         \x20   .kern_scale = 0, .cmap_num = 1, .bpp = 1, .kern_classes = 0, .bitmap_format = 0, .stride = 1,\n}};\n\n\
         const lv_font_t {symbol} = {{\n\
         \x20   .get_glyph_dsc = lv_font_get_glyph_dsc_fmt_txt, .get_glyph_bitmap = lv_font_get_bitmap_fmt_txt,\n\
         \x20   .line_height = {box_h}, .base_line = {descent}, .subpx = LV_FONT_SUBPX_NONE,\n\
         \x20   .underline_position = -1, .underline_thickness = 1, .dsc = &dsc,\n}};\n",
        n = glyphs.len(),
    );
    let file = out.join(format!("{symbol}.c"));
    fs::write(&file, c).unwrap();
    cc::Build::new()
        .file(file)
        .define("LV_LVGL_H_INCLUDE_SIMPLE", None)
        .include(&cfg.lvgl_include_dir)
        .include(&cfg.lv_conf_dir)
        .opt_level(2)
        .compile(&format!("lvgl_font_{symbol}"));
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
    let mut backdrops = String::new();
    let mut cloud_arms = String::new();
    for t in TimeOfDay::ALL {
        let name = t.name();
        image(&format!("backdrop_{name}"), &backdrop(t), false);
        writeln!(backdrops, "        TimeOfDay::{t:?} => backdrop_{name}(),").unwrap();
        for (k, img) in clouds(t).iter().enumerate() {
            image(&format!("cloud_{name}_{k}"), img, true);
            writeln!(
                cloud_arms,
                "        (TimeOfDay::{t:?}, {k}) => Some(cloud_{name}_{k}()),"
            )
            .unwrap();
        }
    }
    image("street", &street(), false);
    image("lobby", &lobby(), false);
    image("car", &car(), true);
    image("arrow_up", &arrow(true), true);
    image("arrow_down", &arrow(false), true);
    let mut spaces = String::new();
    for paper in 0..WALLPAPERS {
        for lit in [false, true] {
            for t in TimeOfDay::ALL {
                for side in 0..SPACES_PER_STOREY {
                    let symbol = format!(
                        "space_{paper}_{}_{}_{side}",
                        if lit { "lit" } else { "dark" },
                        t.name()
                    );
                    image(&symbol, &space(paper, lit, t, side), false);
                    writeln!(
                        spaces,
                        "        ({paper}, {lit}, TimeOfDay::{t:?}, {side}) => {symbol}(),"
                    )
                    .unwrap();
                }
            }
        }
    }
    // Decor dims as its space does: by moonlight at night, in shade by day.
    let mut decors = String::new();
    for kind in 0..DECORS {
        image(
            &format!("decor_{kind}_lit"),
            &decor(kind, true, TimeOfDay::Day),
            true,
        );
        image(
            &format!("decor_{kind}_moon"),
            &decor(kind, false, TimeOfDay::Night),
            true,
        );
        image(
            &format!("decor_{kind}_shade"),
            &decor(kind, false, TimeOfDay::Day),
            true,
        );
        writeln!(
            decors,
            "        ({kind}, true, _) => decor_{kind}_lit(),\n\
             \x20       ({kind}, false, TimeOfDay::Night) => decor_{kind}_moon(),\n\
             \x20       ({kind}, false, _) => decor_{kind}_shade(),"
        )
        .unwrap();
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

    // Text is drawn by LVGL at the font's size, not scaled.
    assert_eq!(SCALE, 1);
    let glyphs = font();
    let outlines: Vec<_> = glyphs.iter().map(outline).collect();
    lvgl_font(
        &cfg,
        &out,
        "label_font",
        &glyphs.iter().map(|g| &g[..]).collect::<Vec<_>>(),
        FONT_W,
        FONT_DESCENT,
    );
    lvgl_font(
        &cfg,
        &out,
        "label_outline_font",
        &outlines.iter().map(|g| &g[..]).collect::<Vec<_>>(),
        FONT_W + 2,
        FONT_DESCENT + 1,
    );
    let hex = |c: [u8; 4]| format!("0x{:02x}{:02x}{:02x}", c[0], c[1], c[2]);

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
         /// The sky and the city at `time`, filling the screen; it stays still.\n\
         pub fn backdrop_image(time: TimeOfDay) -> &'static oxivgl::widgets::lv_image_dsc_t {{\n\
         \x20   match time {{\n{backdrops}    }}\n}}\n\n\
         /// Cloud `k` of [`CLOUDS`] at `time`; none at night.\n\
         pub fn cloud_image(time: TimeOfDay, k: usize) -> Option<&'static oxivgl::widgets::lv_image_dsc_t> {{\n\
         \x20   match (time, k) {{\n{cloud_arms}        _ => None,\n    }}\n}}\n\n\
         {street}{lobby}{car}{arrow_up}{arrow_down}\
         /// Space `side` (0 left, 1 right) of a storey in wallpaper `paper` (modulo\n\
         /// [`WALLPAPERS`]) at `time`, its lights on or off. It goes at [`SPACE_X`].\n\
         pub fn space_image(paper: usize, lit: bool, time: TimeOfDay, side: usize) -> &'static oxivgl::widgets::lv_image_dsc_t {{\n\
         \x20   match (paper % WALLPAPERS, lit, time, side) {{\n{spaces}        _ => unreachable!(),\n    }}\n}}\n\n\
         /// Decor `kind` (modulo [`DECORS`]) for a storey's right end, lit or dim\n\
         /// as its space is at `time`.\n\
         pub fn decor_image(kind: usize, lit: bool, time: TimeOfDay) -> &'static oxivgl::widgets::lv_image_dsc_t {{\n\
         \x20   match (kind % DECORS, lit, time) {{\n{decors}        _ => unreachable!(),\n    }}\n}}\n\n\
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
         /// A storey's spaces, each of `DESKS_PER_SPACE` desks, and where each\n\
         /// starts, relative to the storey.\n\
         pub const DESKS_PER_SPACE: usize = {DESKS_PER_SPACE};\n\
         pub const SPACES_PER_STOREY: usize = {SPACES_PER_STOREY};\n\
         pub const SPACE_X: [i32; {SPACES_PER_STOREY}] = [{space_x}];\n\
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
         unsafe extern \"C\" {{\n\
         \x20   static label_font: u8;\n\
         \x20   static label_outline_font: u8;\n\
         }}\n\
         /// The text font, for workspace labels, and the outline drawn under\n\
         /// its characters: a label in each, the outline's a pixel up and left\n\
         /// and 2 pixels wider and taller.\n\
         pub static LABEL_FONT: oxivgl::fonts::Font =\n\
         \x20   unsafe {{ oxivgl::fonts::Font::from_extern(core::ptr::addr_of!(label_font) as *const ()) }};\n\
         pub static LABEL_OUTLINE_FONT: oxivgl::fonts::Font =\n\
         \x20   unsafe {{ oxivgl::fonts::Font::from_extern(core::ptr::addr_of!(label_outline_font) as *const ()) }};\n\
         pub const FONT_W: i32 = {FONT_W};\n\
         pub const FONT_H: i32 = {FONT_H};\n\
         /// Where each space's workspace label's last character cell ends at the\n\
         /// right, and where labels' cells' tops are, relative to the storey; and\n\
         /// their colours.\n\
         pub const LABEL_ENDS: [i32; {SPACES_PER_STOREY}] = [{label_ends}];\n\
         pub const LABEL_Y: i32 = {label_y};\n\
         pub const LABEL_COLOUR: u32 = {label_colour};\n\
         /// The neon sign on the roof, relative to the roof's top left: the middle\n\
         /// of its board, the board's top and height, and how much wider than its\n\
         /// text it is; the text is 3 pixels in and 1 down, a leg 6 pixels in from\n\
         /// each side. Then how far the halo reaches past its sides, above and\n\
         /// below, how opaque it is, and the colours.\n\
         pub const SIGN_CENTRE: i32 = {sign_centre};\n\
         pub const SIGN_TOP: i32 = {sign_top};\n\
         pub const SIGN_H: i32 = {sign_h};\n\
         pub const SIGN_MARGIN: i32 = {sign_margin};\n\
         pub const SIGN_HALO: (i32, i32, i32) = {sign_halo:?};\n\
         pub const SIGN_HALO_OPA: u8 = {SIGN_HALO_OPA};\n\
         pub const SIGN_NEON: u32 = {sign_neon};\n\
         pub const SIGN_BOARD: u32 = {sign_board_c};\n\
         pub const SIGN_INK: u32 = {sign_ink};\n\
         /// The lobby's LED ticker: its board's left edge, top, width and height,\n\
         /// relative to the lobby; its text runs a pixel inside, in `TICKER_LED`.\n\
         pub const TICKER: (i32, i32, i32, i32) = {ticker:?};\n\
         pub const TICKER_LED: u32 = {ticker_led};\n\
         pub const LABEL_OUTLINE: u32 = {label_outline};\n\
         /// Each cloud's left edge and top with no drift, and its width.\n\
         pub const CLOUDS: [(i32, i32, i32); {n_clouds}] = [{clouds_at}];\n\
         /// Where the agents at each desk go, relative to the storey.\n\
         pub const SLOTS: [Slot; {DESKS_PER_STOREY}] = [\n{slots}];\n",
        n = ANIMATIONS.len(),
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
        label_ends = LABEL_ENDS.map(|x| (x * scale).to_string()).join(", "),
        label_y = LABEL_Y * scale,
        space_x = SPACE_X.map(|x| (x * scale).to_string()).join(", "),
        n_clouds = CLOUDS.len(),
        clouds_at = CLOUDS
            .iter()
            .map(|&(x, y, w)| format!("({}, {}, {})", x * scale, y * scale, w as i32 * scale))
            .collect::<Vec<_>>()
            .join(", "),
        label_colour = hex(LABEL_COLOUR),
        sign_centre = SIGN_CENTRE,
        sign_top = sign_board(0).1,
        sign_h = sign_board(0).3,
        sign_margin = sign_board(0).2,
        sign_halo = SIGN_HALO,
        sign_neon = hex(SIGN_NEON),
        sign_board_c = hex(SIGN_BOARD),
        sign_ink = hex(SIGN_INK),
        ticker = TICKER,
        ticker_led = hex(TICKER_LED),
        label_outline = hex(LABEL_OUTLINE),
    );
    fs::write(out.join("frames.rs"), source).unwrap();
}
