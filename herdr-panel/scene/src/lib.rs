//! A tower seen from the side at night, in the style of a 16-bit console game
//! (colours from the SNES's 15-bit range shaded in ramps, gradients in
//! horizontal bands, translucent glows, layered backgrounds, dark coloured
//! outlines), after tower-building games like SimTower: one storey per herdr
//! workspace stacked on a lobby, a desk per agent, and an elevator that rides
//! to the focused agent.
//!
//! Everything is drawn at half the screen's resolution and shown at twice the
//! size, so one art pixel is 2x2 screen pixels. The backdrop (sky and city)
//! stays still; the street, lobby, storeys, roof, elevator car and mascots
//! are separate images that the firmware stacks at the positions given here,
//! in art pixels, and scrolls when the tower is taller than the screen.

use herdr_panel_art::{CANVAS_H, CANVAS_W, Grid, HEIGHT, Ink, ORIGIN, WIDTH};

/// Size of the background in art pixels.
pub const W: usize = 160;
pub const H: usize = 240;
/// Screen pixels per art pixel.
pub const SCALE: usize = 2;

pub type Rgba = [u8; 4];

/// `c` with each channel cut to 5 bits, as the SNES stores colours.
fn snes(c: Rgba) -> Rgba {
    [c[0] & 0xf8, c[1] & 0xf8, c[2] & 0xf8, c[3]]
}

/// An RGBA picture; fully transparent where nothing is drawn. Every colour
/// put in it is cut to the SNES's 15 bits.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Image {
    pub w: usize,
    pub h: usize,
    pub px: Vec<Rgba>,
}

impl Image {
    pub fn new(w: usize, h: usize) -> Self {
        Self { w, h, px: vec![[0; 4]; w * h] }
    }

    pub fn get(&self, x: usize, y: usize) -> Rgba {
        self.px[y * self.w + x]
    }

    fn inside(&self, x: i32, y: i32) -> bool {
        x >= 0 && y >= 0 && (x as usize) < self.w && (y as usize) < self.h
    }

    fn set(&mut self, x: i32, y: i32, c: Rgba) {
        if self.inside(x, y) {
            self.px[y as usize * self.w + x as usize] = snes(c);
        }
    }

    /// Lays `c` over the pixel at opacity `a` (0..1), like the SNES's colour
    /// maths. Over a transparent pixel it leaves a translucent one.
    fn blend(&mut self, x: i32, y: i32, c: Rgba, a: f32) {
        if !self.inside(x, y) {
            return;
        }
        let old = self.get(x as usize, y as usize);
        let new = if old[3] == 0 {
            [c[0], c[1], c[2], (a * 255.0) as u8]
        } else {
            let oa = old[3] as f32 / 255.0;
            let out_a = a + oa * (1.0 - a);
            let ch = |i: usize| ((c[i] as f32 * a + old[i] as f32 * oa * (1.0 - a)) / out_a).round() as u8;
            [ch(0), ch(1), ch(2), (out_a * 255.0).round() as u8]
        };
        self.set(x, y, new);
    }

    /// Fills the rectangle at (`x`, `y`) of size `w` x `h`.
    fn fill(&mut self, x: i32, y: i32, w: i32, h: i32, c: Rgba) {
        for yy in y..y + h {
            for xx in x..x + w {
                self.set(xx, yy, c);
            }
        }
    }

    /// Fills the rectangle with a one-pixel outline of colour `line`.
    fn boxed(&mut self, x: i32, y: i32, w: i32, h: i32, c: Rgba, line: Rgba) {
        self.fill(x, y, w, h, line);
        self.fill(x + 1, y + 1, w - 2, h - 2, c);
    }

    /// A vertical gradient from `top` to `bottom` in bands `band` pixels tall.
    fn gradient(&mut self, x: i32, y: i32, w: i32, h: i32, top: Rgba, bottom: Rgba, band: i32) {
        for yy in 0..h {
            let t = (yy / band * band) as f32 / (h - 1).max(1) as f32;
            self.fill(x, y + yy, w, 1, mix(top, bottom, t));
        }
    }

    /// A round glow of colour `c` around (`cx`, `cy`): rings of falling
    /// opacity, out to radius `r`.
    fn glow(&mut self, cx: i32, cy: i32, r: i32, c: Rgba, a: f32) {
        for y in cy - r..=cy + r {
            for x in cx - r..=cx + r {
                let d = (((x - cx).pow(2) + (y - cy).pow(2)) as f32).sqrt() / r as f32;
                if d <= 1.0 {
                    // Three steps rather than a smooth fade.
                    let step = (3.0 - (d * 3.0).floor()) / 3.0;
                    self.blend(x, y, c, a * step);
                }
            }
        }
    }

    /// Draws `other` with its top left corner at (`x`, `y`), blending its
    /// translucent pixels.
    pub fn draw(&mut self, other: &Image, x: i32, y: i32) {
        for oy in 0..other.h {
            for ox in 0..other.w {
                let c = other.get(ox, oy);
                if c[3] != 0 {
                    self.blend(x + ox as i32, y + oy as i32, c, c[3] as f32 / 255.0);
                }
            }
        }
    }

    /// The picture with each pixel repeated `sx` times across and `sy` down.
    pub fn scaled(&self, sx: usize, sy: usize) -> Image {
        let mut out = Image::new(self.w * sx, self.h * sy);
        for y in 0..out.h {
            for x in 0..out.w {
                out.px[y * out.w + x] = self.get(x / sx, y / sy);
            }
        }
        out
    }
}

const fn rgb(hex: u32) -> Rgba {
    [(hex >> 16) as u8 & 0xf8, (hex >> 8) as u8 & 0xf8, hex as u8 & 0xf8, 0xff]
}

/// `a` mixed with `b`; `t` = 0 is all `a`.
fn mix(a: Rgba, b: Rgba, t: f32) -> Rgba {
    let ch = |i: usize| (a[i] as f32 + (b[i] as f32 - a[i] as f32) * t).round() as u8;
    snes([ch(0), ch(1), ch(2), 0xff])
}

/// Three tones of one material.
#[derive(Clone, Copy)]
struct Ramp {
    light: Rgba,
    base: Rgba,
    dark: Rgba,
}

const fn ramp(light: u32, base: u32, dark: u32) -> Ramp {
    Ramp { light: rgb(light), base: rgb(base), dark: rgb(dark) }
}

const INK: Rgba = rgb(0x181020);
const BRICK: Ramp = ramp(0xc06048, 0x983828, 0x602018);
const STONE: Ramp = ramp(0xe0d8c8, 0xb0a898, 0x787068);
const WOOD: Ramp = ramp(0xd89860, 0xa86838, 0x704020);
const METAL: Ramp = ramp(0xe8f0f8, 0xb0b8c8, 0x707888);
const SHAFT: Ramp = ramp(0x484058, 0x282030, 0x181020);
const LEAF: Ramp = ramp(0x78d058, 0x389838, 0x186028);
const NEON: Rgba = rgb(0xff78c0);
const LAMP: Rgba = rgb(0xfff0a0);
const WARM: Rgba = rgb(0xffd890);
const MOON: Rgba = rgb(0xf8f0d0);
const MOONLIGHT: Rgba = rgb(0x5868b0);

/// The night sky from the top down to the horizon's city glow.
const SKY: [Rgba; 6] = [rgb(0x100828), rgb(0x201848), rgb(0x382868), rgb(0x583880), rgb(0x904878), rgb(0xd07058)];

/// Wallpaper, one ramp per storey in turn.
const WALLPAPER: [Ramp; 4] = [
    ramp(0xf8e8c8, 0xf0d0a8, 0xc8a880),
    ramp(0xd0f0e0, 0xb0d8c8, 0x88b0a0),
    ramp(0xe0d0f0, 0xc8b8e0, 0xa090c0),
    ramp(0xc8e0f8, 0xa8c8e8, 0x80a0c8),
];

fn hash(a: i32, b: i32) -> u32 {
    let mut h = (a as u32).wrapping_mul(0x9E37_79B1) ^ (b as u32).wrapping_mul(0x85EB_CA77);
    h ^= h >> 15;
    h = h.wrapping_mul(0x2C1B_3C6D);
    h ^ (h >> 12)
}

/// The sky's colour at height `y`, in bands three pixels tall.
fn sky(y: i32) -> Rgba {
    let t = ((y / 3 * 3) as f32 / GROUND_Y as f32).clamp(0.0, 1.0).powf(1.3) * (SKY.len() - 1) as f32;
    let i = (t as usize).min(SKY.len() - 2);
    mix(SKY[i], SKY[i + 1], t - i as f32)
}

/// A 3x5 pixel font, for storey numbers and the signs.
fn glyph(c: char) -> [&'static str; 5] {
    match c {
        '0' => ["###", "#.#", "#.#", "#.#", "###"],
        '1' => [".#.", "##.", ".#.", ".#.", "###"],
        '2' => ["###", "..#", "###", "#..", "###"],
        '3' => ["###", "..#", ".##", "..#", "###"],
        '4' => ["#.#", "#.#", "###", "..#", "..#"],
        '5' => ["###", "#..", "###", "..#", "###"],
        '6' => ["###", "#..", "###", "#.#", "###"],
        '7' => ["###", "..#", "..#", ".#.", ".#."],
        '8' => ["###", "#.#", "###", "#.#", "###"],
        '9' => ["###", "#.#", "###", "..#", "###"],
        'H' => ["#.#", "#.#", "###", "#.#", "#.#"],
        'E' => ["###", "#..", "##.", "#..", "###"],
        'R' => ["##.", "#.#", "##.", "#.#", "#.#"],
        'D' => ["##.", "#.#", "#.#", "#.#", "##."],
        'L' => ["#..", "#..", "#..", "#..", "###"],
        'O' => ["###", "#.#", "#.#", "#.#", "###"],
        'B' => ["##.", "#.#", "##.", "#.#", "##."],
        'Y' => ["#.#", "#.#", ".#.", ".#.", ".#."],
        _ => ["...", "...", "...", "...", "..."],
    }
}

/// Writes `s` in the pixel font, with a drop shadow if `shadow` is given.
fn text(img: &mut Image, x: i32, y: i32, s: &str, c: Rgba, shadow: Option<Rgba>) {
    for (i, ch) in s.chars().enumerate() {
        for (dy, row) in glyph(ch).iter().enumerate() {
            for (dx, p) in row.chars().enumerate() {
                if p == '#' {
                    let (px, py) = (x + i as i32 * 4 + dx as i32, y + dy as i32);
                    if let Some(s) = shadow {
                        img.set(px + 1, py + 1, s);
                    }
                    img.set(px, py, c);
                }
            }
        }
    }
}

// The tower fills the screen's width and stands on the pavement at GROUND_Y.
// Inside, from the left: brick wall, elevator shaft, rooms, brick wall.
// Storey `level` (0 is the lobby) has its top at `storey_top(level)`, which
// goes negative for storeys above the screen: the firmware scrolls up to them.
// Every position below is in art pixels on the screen at the bottom of that
// scroll, or, where it says so, relative to a storey's top left corner.
pub const GROUND_Y: i32 = 223;
const WALL: i32 = 4;
const SHAFT_X: i32 = WALL;
const SHAFT_W: i32 = 14;
const ROOMS_X: i32 = SHAFT_X + SHAFT_W + 1;
const ROOMS_END: i32 = W as i32 - WALL;
/// Height of a storey, including the floor slab at its bottom.
pub const STOREY_H: usize = 24;
const SLAB: i32 = 3;
pub const DESKS_PER_STOREY: usize = 6;
/// The desks start this far from a storey's left edge, one every
/// `DESK_SPACING` pixels; for telling which desk a tap is on.
pub const DESKS_X: i32 = ROOMS_X + 1;
pub const DESK_SPACING: i32 = 22;
pub const ROOF_H: usize = 14;
/// How many wallpapers storeys cycle through, one per workspace.
pub const WALLPAPERS: usize = WALLPAPER.len();
/// How far the top of the plumbob is above the agent's feet.
pub const PLUMBOB_ABOVE: i32 = 19;

/// The top of storey `level`, where 0 is the lobby.
pub fn storey_top(level: usize) -> i32 {
    GROUND_Y - STOREY_H as i32 * (level as i32 + 1)
}

/// The top of the roof over a tower of `storeys` storeys above the lobby.
pub fn roof_top(storeys: usize) -> i32 {
    storey_top(storeys) - ROOF_H as i32 + 1
}

/// A row of city blocks along the horizon: heights from `seed`, between
/// `low` and `high` pixels tall.
fn skyline(img: &mut Image, seed: i32, low: i32, high: i32, body: Rgba, edge: Rgba, windows: &[Rgba], lit: u32) {
    let mut x = -6;
    let mut i = 0;
    while x < W as i32 {
        let w = 10 + (hash(i, seed) % 14) as i32;
        let h = low + (hash(i, seed + 1) % (high - low) as u32) as i32;
        let top = GROUND_Y - h;
        img.fill(x, top, w, h, body);
        img.fill(x, top, w, 1, edge);
        img.fill(x, top, 1, h, edge);
        // Some blocks get a spire or a stepped top.
        match hash(i, seed + 2) % 4 {
            0 => img.fill(x + w / 2, top - 6, 1, 6, edge),
            1 => {
                img.fill(x + 2, top - 3, w - 4, 3, body);
                img.fill(x + 2, top - 3, w - 4, 1, edge);
            }
            _ => {}
        }
        for wy in (top + 3..GROUND_Y - 3).step_by(4) {
            for wx in (x + 2..x + w - 2).step_by(3) {
                if hash(wx, wy + seed) % lit == 0 {
                    let c = windows[(hash(wy, wx) as usize) % windows.len()];
                    img.fill(wx, wy, 2, 2, c);
                }
            }
        }
        x += w;
        i += 1;
    }
}

/// What stays still behind the tower: the night sky, the moon, and the city
/// along the horizon, which shows above the roof of a short tower.
pub fn backdrop() -> Image {
    let mut img = Image::new(W, H);
    for y in 0..H as i32 {
        img.fill(0, y, W as i32, 1, sky(y));
    }
    // Stars: a few bright ones with crosses, many faint.
    for y in 0..GROUND_Y - 70 {
        for x in 0..W as i32 {
            match hash(x, y) % 260 {
                0 => img.set(x, y, rgb(0xf8f8f8)),
                1 | 2 => img.blend(x, y, rgb(0xc0c8f8), 0.6),
                _ => {}
            }
        }
    }
    for (sx, sy) in [(16, 46), (98, 30), (60, 70)] {
        img.set(sx, sy, rgb(0xffffff));
        for (dx, dy) in [(1, 0), (-1, 0), (0, 1), (0, -1)] {
            img.blend(sx + dx, sy + dy, rgb(0xc0c8f8), 0.7);
        }
    }
    // The moon: a halo, then the disc with a shaded edge and craters.
    img.glow(140, 30, 16, MOON, 0.18);
    for y in -7..=7 {
        for x in -7..=7 {
            let d = x * x + y * y;
            if d <= 49 {
                let c = if d > 36 && x + y > 0 {
                    rgb(0xd0c8a8)
                } else if x + y < -6 {
                    rgb(0xffffff)
                } else {
                    MOON
                };
                img.set(140 + x, 30 + y, c);
            }
        }
    }
    for (x, y) in [(138, 28), (143, 32), (139, 34)] {
        img.set(x, y, rgb(0xd8d0b0));
    }
    // Two layers of city: far and hazy, then near and dark with lit windows.
    skyline(&mut img, 11, 50, 110, rgb(0x483878), rgb(0x584888), &[rgb(0x9878a0)], 9);
    for y in GROUND_Y - 40..H as i32 {
        let t = ((y - (GROUND_Y - 40)) as f32 / 40.0).min(1.0);
        for x in 0..W as i32 {
            img.blend(x, y, SKY[5], 0.25 * t);
        }
    }
    skyline(&mut img, 3, 24, 70, rgb(0x201838), rgb(0x302850), &[rgb(0xf8c060), rgb(0xf8e0a0), rgb(0x6888f8)], 4);
    img
}

/// Where the lobby's doors are, from the left edge.
const DOOR_X: i32 = 118;

/// The street in front of the tower, from GROUND_Y to the bottom of the
/// screen: pavement lit by the lobby's doors, kerb, asphalt with a dashed line.
pub fn street() -> Image {
    let h = H as i32 - GROUND_Y;
    let mut img = Image::new(W, h as usize);
    img.fill(0, 0, W as i32, 1, STONE.light);
    img.gradient(0, 1, W as i32, 4, STONE.base, STONE.dark, 1);
    for x in (0..W as i32).step_by(8) {
        img.fill(x, 1, 1, 4, STONE.dark);
    }
    for y in 0..5 {
        for x in DOOR_X - 2..DOOR_X + 30 {
            img.blend(x, y, WARM, 0.25);
        }
    }
    img.fill(0, 5, W as i32, 1, rgb(0x383040));
    img.gradient(0, 6, W as i32, h - 6, rgb(0x403850), rgb(0x282030), 2);
    for x in (2..W as i32).step_by(14) {
        img.fill(x, 11, 7, 1, rgb(0xf8d048));
        img.fill(x, 12, 7, 1, rgb(0xb89030));
    }
    img
}

/// Brick wall texture at (`x`, `y`), `w` x `h`: each brick lit on top.
fn bricks(img: &mut Image, x: i32, y: i32, w: i32, h: i32) {
    for yy in y..y + h {
        for xx in x..x + w {
            let row = yy.div_euclid(3);
            let c = if yy.rem_euclid(3) == 2 || (xx + row * 3).rem_euclid(6) == 0 {
                BRICK.dark
            } else if yy.rem_euclid(3) == 0 {
                BRICK.light
            } else {
                BRICK.base
            };
            img.set(xx, yy, c);
        }
    }
}

/// A storey's walls, shaft and slab, with the room in `paper`.
fn shell(paper: Ramp) -> Image {
    let mut img = Image::new(W, STOREY_H);
    let h = STOREY_H as i32;
    let inner = h - SLAB;
    let rooms_w = ROOMS_END - ROOMS_X;
    // Room: wallpaper with a shadow under the ceiling and a darker dado.
    img.fill(ROOMS_X, 0, rooms_w, inner, paper.base);
    img.fill(ROOMS_X, 0, rooms_w, 2, paper.dark);
    img.fill(ROOMS_X, inner - 7, rooms_w, 7, paper.dark);
    img.fill(ROOMS_X, inner - 7, rooms_w, 1, paper.light);
    // Outer walls, with a dark line along the outside.
    bricks(&mut img, 0, 0, WALL, h);
    bricks(&mut img, ROOMS_END, 0, WALL, h);
    img.fill(0, 0, 1, h, INK);
    img.fill(W as i32 - 1, 0, 1, h, INK);
    // The shaft: dark with lit rails and a cable.
    img.gradient(SHAFT_X, 0, SHAFT_W, inner, SHAFT.base, SHAFT.dark, 2);
    for rail in [SHAFT_X + 1, SHAFT_X + SHAFT_W - 2] {
        img.fill(rail, 0, 1, inner, SHAFT.light);
    }
    img.fill(SHAFT_X + SHAFT_W / 2, 0, 1, inner, rgb(0x383040));
    img.fill(ROOMS_X - 1, 0, 1, inner, INK);
    // Floor: wooden boards on a concrete slab.
    img.fill(ROOMS_X - 1, inner, rooms_w + 1, 1, WOOD.light);
    img.fill(WALL, inner + 1, W as i32 - 2 * WALL, 1, STONE.base);
    img.fill(WALL, inner + 2, W as i32 - 2 * WALL, 1, STONE.dark);
    img.fill(SHAFT_X, inner, SHAFT_W, 1, STONE.light);
    img
}

/// The lobby: reception, a plant, and glass doors under a striped awning.
pub fn lobby() -> Image {
    let mut img = shell(ramp(0xf0e8d8, 0xd8d0c0, 0xb0a898));
    let inner = STOREY_H as i32 - SLAB;
    // Marble floor tiles in two tones.
    for x in ROOMS_X..ROOMS_END {
        let c = if (x / 4) % 2 == 0 { rgb(0xf8f0e8) } else { rgb(0xc8c0b8) };
        img.set(x, inner, c);
    }
    text(&mut img, ROOMS_X + 3, 4, "LOBBY", rgb(0xb08840), Some(rgb(0x806020)));
    img.boxed(34, 12, 26, 9, WOOD.base, INK);
    img.fill(35, 13, 24, 1, WOOD.light);
    img.fill(35, 19, 24, 1, WOOD.dark);
    for (px, py) in [(70, 6), (98, 6)] {
        img.boxed(px + 3, py + 9, 6, 6, BRICK.base, INK);
        img.fill(px + 4, py + 10, 4, 1, BRICK.light);
        img.boxed(px, py, 12, 10, LEAF.base, INK);
        img.fill(px + 2, py + 2, 4, 2, LEAF.light);
        img.fill(px + 6, py + 6, 4, 3, LEAF.dark);
    }
    img.boxed(DOOR_X, 6, 28, inner - 6, METAL.dark, INK);
    for dx in [2, 15] {
        img.boxed(DOOR_X + dx, 8, 11, inner - 8, rgb(0x78b8e8), INK);
        img.gradient(DOOR_X + dx + 1, 9, 9, inner - 10, rgb(0xa8d8f8), rgb(0x5890c8), 2);
        for i in 0..4 {
            img.set(DOOR_X + dx + 2 + i, 13 - i, rgb(0xe0f0f8));
        }
    }
    for dx in 0..32 {
        let stripe =
            if (dx / 4) % 2 == 0 { ramp(0xf87068, 0xd83838, 0x902020) } else { ramp(0xffffff, 0xf0e8e0, 0xc0b8b0) };
        img.set(DOOR_X - 2 + dx, 2, stripe.light);
        img.set(DOOR_X - 2 + dx, 3, stripe.base);
        img.set(DOOR_X - 2 + dx, 4, stripe.dark);
    }
    img.fill(DOOR_X - 2, 5, 32, 1, INK);
    lamps(&mut img, true, &[48, 86]);
    img
}

/// Ceiling lamps at `xs` in a storey; lit ones throw a cone of warm light
/// down to the floor.
fn lamps(img: &mut Image, lit: bool, xs: &[i32]) {
    let inner = STOREY_H as i32 - SLAB;
    for &cx in xs {
        img.fill(cx, 0, 1, 1, INK);
        img.fill(cx - 2, 1, 5, 1, INK);
        img.fill(cx - 1, 2, 3, 1, if lit { LAMP } else { rgb(0x605868) });
        if lit {
            for y in 3..inner {
                let half = 2 + y / 2;
                for x in cx - half..=cx + half {
                    img.blend(x, y, WARM, 0.16);
                }
            }
        }
    }
}

/// A storey of offices in wallpaper `paper` (counted modulo
/// [`WALLPAPERS`]), with its desks, and its lights on if `lit`. Agents are
/// drawn over it with [`behind_desk`]; the storey's number goes on with
/// [`digits`].
pub fn storey(paper: usize, lit: bool) -> Image {
    let paper = paper % WALLPAPERS;
    let ramp = WALLPAPER[paper];
    let mut img = shell(ramp);
    let inner = STOREY_H as i32 - SLAB;
    for i in 0..DESKS_PER_STOREY {
        let cx = desk_centre(i);
        // A window behind each desk, showing the sky, with a white frame.
        img.boxed(cx - 7, 3, 14, 10, rgb(0xf8f0e8), INK);
        for wy in 4..12 {
            img.fill(cx - 6, wy, 12, 1, sky(wy * 8 + paper as i32 * 4 + i as i32 * 6));
        }
        img.fill(cx, 4, 1, 8, rgb(0xd0c8c0));
        img.fill(cx - 6, 8, 12, 1, rgb(0xd0c8c0));
        img.fill(cx - 7, 12, 14, 1, ramp.dark);
        if hash(paper as i32, i as i32) % 3 == 0 {
            img.set(cx - 4, 5, rgb(0xf8f8f8));
        }
        img.set(cx + 3, 9, rgb(0x9888b8));
        let (dx, dy) = desk_at(i);
        img.draw(&desk(), dx, dy);
    }
    let lamp_xs: Vec<i32> = (0..DESKS_PER_STOREY - 1).map(|i| desk_centre(i) + DESK_SPACING / 2).collect();
    if lit {
        lamps(&mut img, true, &lamp_xs);
    } else {
        // Lights out: the room tinted by moonlight, the lamps dark.
        for y in 0..inner + 1 {
            for x in ROOMS_X..ROOMS_END {
                let c = img.get(x as usize, y as usize);
                let tint = |i: usize| (c[i] as u32 * MOONLIGHT[i] as u32 / 255) as u8;
                img.set(x, y, [tint(0), tint(1), tint(2), 0xff]);
            }
        }
        lamps(&mut img, false, &lamp_xs);
        // Moonlight falling through each window onto the floor.
        for i in 0..DESKS_PER_STOREY {
            let cx = desk_centre(i);
            for y in 13..inner {
                let shift = (y - 13) / 2;
                for x in cx - 6 + shift..cx + 6 + shift {
                    img.blend(x, y, rgb(0xa0b0f8), 0.12);
                }
            }
        }
    }
    img
}

/// The digits 0 to 9 for storey numbers, with a drop shadow.
pub fn digits() -> [Image; 10] {
    core::array::from_fn(|d| {
        let mut img = Image::new(4, 6);
        text(&mut img, 0, 0, &d.to_string(), rgb(0xf8b040), Some(rgb(0x582810)));
        img
    })
}

/// Where the first of a storey number's `len` digits goes, relative to the
/// storey; each next digit goes 4 pixels to the right.
pub fn number_at(len: usize) -> (i32, i32) {
    (SHAFT_X + SHAFT_W / 2 - 2 * len as i32 + 1, 2)
}

/// The roof over the top storey: a parapet, a water tank, a glowing "HERDR"
/// sign and an antenna whose light is on in frame 0 and off in frame 1. Its
/// top goes at [`roof_top`].
pub fn roof() -> [Image; 2] {
    let w = W as i32;
    let h = ROOF_H as i32;
    let sign = 68;
    let frame = |light: bool| {
        let mut img = Image::new(W, ROOF_H);
        // Neon glow first, so the sign and its frame sit on it.
        for y in 0..h - 3 {
            for x in sign - 3..sign + 26 {
                img.blend(x, y, NEON, 0.12);
            }
        }
        img.boxed(0, h - 4, w, 4, STONE.base, INK);
        img.fill(1, h - 3, w - 2, 1, STONE.light);
        // Water tank on legs: wooden staves with metal bands.
        img.boxed(9, h - 13, 13, 8, WOOD.base, INK);
        img.fill(10, h - 12, 3, 6, WOOD.light);
        img.fill(18, h - 12, 3, 6, WOOD.dark);
        img.fill(10, h - 10, 11, 1, METAL.dark);
        img.fill(10, h - 7, 11, 1, METAL.dark);
        img.fill(11, h - 5, 1, 1, INK);
        img.fill(19, h - 5, 1, 1, INK);
        // The sign on its frame.
        img.fill(sign + 5, h - 5, 1, 1, INK);
        img.fill(sign + 19, h - 5, 1, 1, INK);
        img.boxed(sign, h - 13, 23, 8, rgb(0x281828), INK);
        text(&mut img, sign + 2, h - 11, "HERDR", NEON, None);
        // Antenna, with its light glowing when on.
        img.fill(w - 16, 2, 1, h - 6, METAL.base);
        img.fill(w - 18, 6, 5, 1, METAL.base);
        if light {
            img.glow(w - 16, 1, 3, rgb(0xff4040), 0.35);
        }
        img.fill(w - 17, 0, 3, 2, if light { rgb(0xff4040) } else { rgb(0x702020) });
        img
    };
    [frame(true), frame(false)]
}

/// Size of an [`arrow`] button, and its left edge: over the elevator shaft.
pub const ARROW_SIZE: usize = 14;
pub const ARROW_X: i32 = SHAFT_X;

/// A metal button with a glowing arrow, pointing up if `up`, for scrolling
/// the tower a storey at a time. Its corners are cut round.
pub fn arrow(up: bool) -> Image {
    let n = ARROW_SIZE as i32;
    let mut img = Image::new(ARROW_SIZE, ARROW_SIZE);
    img.boxed(0, 0, n, n, METAL.base, INK);
    img.fill(1, 1, n - 2, 1, METAL.light);
    img.fill(1, 1, 1, n - 2, METAL.light);
    img.fill(1, n - 2, n - 2, 1, METAL.dark);
    img.fill(n - 2, 1, 1, n - 2, METAL.dark);
    for (x, y) in [(0, 0), (n - 1, 0), (0, n - 1), (n - 1, n - 1)] {
        img.px[(y * n + x) as usize] = [0; 4];
    }
    for r in 0..5 {
        let y = if up { 4 + r } else { n - 5 - r };
        img.fill(n / 2 - 1 - r, y, 2 + 2 * r, 1, if r == 4 { rgb(0xc07818) } else { rgb(0xf8b040) });
    }
    img
}

/// The elevator car, with its doors shut and its indicator lit.
pub fn car() -> Image {
    let (w, h) = (SHAFT_W - 2, STOREY_H as i32 - SLAB - 2);
    let mut img = Image::new(w as usize, h as usize);
    img.boxed(0, 0, w, h, METAL.base, INK);
    img.fill(1, 1, w - 2, 2, METAL.dark);
    img.fill(w / 2 - 1, 1, 2, 1, rgb(0xf8b040));
    img.gradient(2, 4, w - 4, h - 5, METAL.light, METAL.dark, 2);
    img.fill(w / 2, 4, 1, h - 5, INK);
    img.fill(3, 5, 1, 5, rgb(0xffffff));
    img.fill(w / 2 + 2, 5, 1, 3, rgb(0xffffff));
    img
}

/// Where the car stops in a storey, relative to the storey.
pub const CAR_AT: (i32, i32) = (SHAFT_X + 1, 2);

fn desk_centre(i: usize) -> i32 {
    DESKS_X + DESK_SPACING * i as i32 + DESK_SPACING / 2
}

const DESK_W: i32 = 22;
const DESK_H: i32 = 4;
const LID_H: i32 = 6;
/// How far left of its desk's centre the agent stands, clear of the monitor.
const AGENT_LEFT: i32 = 2;
/// How far above the floor its feet are, as if on a tall stool.
const AGENT_UP: i32 = 2;

/// A desk seen from the front, with a monitor at its right end seen from
/// behind, its screen's glow spilling round the edges.
fn desk() -> Image {
    let mut img = Image::new(DESK_W as usize, (DESK_H + LID_H) as usize);
    for y in 0..LID_H {
        for x in DESK_W - 9..DESK_W {
            img.blend(x, y, rgb(0x88c8ff), 0.3);
        }
    }
    img.boxed(DESK_W - 7, 0, 6, LID_H - 1, METAL.base, INK);
    img.fill(DESK_W - 6, 1, 4, 1, METAL.light);
    img.fill(DESK_W - 5, LID_H - 1, 2, 1, INK);
    img.boxed(0, LID_H, DESK_W, DESK_H, WOOD.base, INK);
    img.fill(1, LID_H + 1, DESK_W - 2, 1, WOOD.light);
    img.fill(1, LID_H + 2, DESK_W - 2, 1, WOOD.base);
    img.fill(4, LID_H + 2, 5, 1, WOOD.dark);
    img
}

/// Where desk `i`'s top left corner is, relative to the storey.
fn desk_at(i: usize) -> (i32, i32) {
    (desk_centre(i) - DESK_W / 2, STOREY_H as i32 - SLAB - DESK_H - LID_H + 1)
}

/// Where the agent at desk `i` goes, relative to its storey.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Slot {
    /// The point between the agent's feet.
    pub feet: (i32, i32),
    /// Where the top left corner of its [`mascot`] image goes.
    pub mascot: (i32, i32),
}

/// Where the mascot's feet are in a [`mascot`] image: the middle of its
/// bottom edge, below the leg row.
const MASCOT_FEET: (i32, i32) = ((ORIGIN.0 + WIDTH / 2) as i32, ((ORIGIN.1 + HEIGHT - 1) * 2) as i32);

/// The agent's place at desk `i` (0 to [`DESKS_PER_STOREY`] - 1).
pub fn slot(i: usize) -> Slot {
    let feet = (desk_centre(i) - AGENT_LEFT, STOREY_H as i32 - SLAB - AGENT_UP);
    Slot { feet, mascot: (feet.0 - MASCOT_FEET.0, feet.1 - MASCOT_FEET.1) }
}

/// The mascot frame `grid`, keeping the terminal's shape: each pixel one
/// wide and two tall, like a character cell, in flat colours picked by
/// `color`. A dark outline goes round it, which also fills in the gaps that
/// are its eyes; a drowsy eye ([`Ink::Lid`]) is open only in its lower half.
pub fn mascot(grid: &Grid, color: impl Fn(Ink) -> Rgba) -> Image {
    let mut img = Image::new(CANVAS_W, CANVAS_H * 2);
    for y in 0..CANVAS_H * 2 {
        for x in 0..CANVAS_W {
            match grid[y / 2][x] {
                Some(Ink::Lid) if y % 2 == 1 => {}
                Some(Ink::Lid) => img.set(x as i32, y as i32, color(Ink::Body)),
                Some(i) => img.set(x as i32, y as i32, color(i)),
                None => {}
            }
        }
    }
    let solid = img.clone();
    let filled = |x: i32, y: i32| solid.inside(x, y) && solid.get(x as usize, y as usize)[3] != 0;
    for y in 0..img.h as i32 {
        for x in 0..img.w as i32 {
            if !filled(x, y) && (filled(x - 1, y) || filled(x + 1, y) || filled(x, y - 1) || filled(x, y + 1)) {
                img.set(x, y, OUTLINE);
            }
        }
    }
    img
}

/// `mascot` with the pixels its desk hides cut away, so that drawn over a
/// [`storey`] at a [`Slot`] it stands behind the desk baked into the storey.
pub fn behind_desk(mascot: &Image) -> Image {
    let (s, d) = (slot(0), desk_at(0));
    let desk = desk();
    let mut out = mascot.clone();
    for y in 0..mascot.h as i32 {
        for x in 0..mascot.w as i32 {
            let (dx, dy) = (x + s.mascot.0 - d.0, y + s.mascot.1 - d.1);
            if desk.inside(dx, dy) && desk.get(dx as usize, dy as usize)[3] == 0xff {
                out.px[(y as usize) * out.w + x as usize] = [0; 4];
            }
        }
    }
    out
}

/// The mascot's outline and eyes.
const OUTLINE: Rgba = rgb(0x401818);

/// The marker over the focused agent: a green crystal, lit from the left,
/// with a faint glow. Its top goes [`PLUMBOB_ABOVE`] over the agent's feet.
pub fn plumbob() -> Image {
    const ROWS: [&str; 7] = ["..#..", ".#L#.", ".LG#.", "#LGD#", ".GD#.", ".#D#.", "..#.."];
    let mut img = Image::new(5, ROWS.len());
    for (y, row) in ROWS.iter().enumerate() {
        for (x, c) in row.chars().enumerate() {
            let color = match c {
                '#' => rgb(0x084018),
                'L' => rgb(0xb8ffa8),
                'G' => rgb(0x48d848),
                'D' => rgb(0x209030),
                _ => {
                    img.blend(x as i32, y as i32, rgb(0x80ff80), 0.2);
                    continue;
                }
            };
            img.set(x as i32, y as i32, color);
        }
    }
    img
}

#[cfg(test)]
mod tests {
    use super::*;
    use herdr_panel_art::{Behavior, frame};

    const BODY: Rgba = [0xd8, 0x78, 0x58, 0xff];

    #[test]
    fn backdrop_and_street_fill_the_screen_and_are_opaque() {
        let b = backdrop();
        assert_eq!((b.w * SCALE, b.h * SCALE), (320, 480));
        let s = street();
        assert_eq!(s.h as i32, H as i32 - GROUND_Y);
        for img in [b, s, lobby(), storey(0, true), storey(1, false)] {
            assert_eq!(img.w, W);
            assert!(img.px.iter().all(|p| p[3] == 0xff));
        }
    }

    #[test]
    fn storeys_stack_up_from_the_street() {
        assert_eq!(storey_top(0) + STOREY_H as i32, GROUND_Y);
        for level in 1..20 {
            assert_eq!(storey_top(level) + STOREY_H as i32, storey_top(level - 1));
        }
        assert_eq!(roof_top(3) + ROOF_H as i32 - 1, storey_top(3));
    }

    #[test]
    fn desks_fit_the_rooms_and_taps_find_them() {
        for i in 0..DESKS_PER_STOREY {
            let (x, _) = desk_at(i);
            assert!(x >= ROOMS_X && x + DESK_W <= ROOMS_END, "desk {i} at {x}");
            let centre = desk_centre(i);
            assert_eq!(((centre - DESKS_X) / DESK_SPACING) as usize, i);
        }
    }

    #[test]
    fn mascots_stay_in_the_rooms_and_clear_of_each_other() {
        // Sparkles may stray; the body and its outline must not.
        let m = mascot(&frame(Behavior::Work, 0, 1), |_| BODY);
        let opaque_x = |s: Slot| {
            let xs = (0..m.w).filter(|&x| (0..m.h).any(|y| m.get(x, y)[3] != 0));
            let (lo, hi) = (xs.clone().min().unwrap() as i32, xs.max().unwrap() as i32);
            (s.mascot.0 + lo, s.mascot.0 + hi)
        };
        for i in 0..DESKS_PER_STOREY {
            let (lo, hi) = opaque_x(slot(i));
            assert!(lo >= ROOMS_X && hi < ROOMS_END, "mascot {i}: {lo}..{hi}");
            if i > 0 {
                assert!(lo > opaque_x(slot(i - 1)).1, "mascot {i} overlaps its neighbour");
            }
        }
    }

    #[test]
    fn mascot_head_stays_inside_its_storey() {
        let m = mascot(&frame(Behavior::Done, 0, 1), |_| BODY);
        let top = (0..m.h).find(|&y| (0..m.w).any(|x| m.get(x, y) == BODY)).unwrap() as i32;
        assert!(slot(0).mascot.1 + top >= 2, "body starts at {}", slot(0).mascot.1 + top);
    }

    #[test]
    fn car_fits_the_shaft_and_storey() {
        let c = car();
        assert!(CAR_AT.0 >= SHAFT_X && CAR_AT.0 + c.w as i32 <= SHAFT_X + SHAFT_W);
        assert!(CAR_AT.1 >= 0 && CAR_AT.1 + c.h as i32 <= STOREY_H as i32 - SLAB);
    }

    #[test]
    fn numbers_fit_the_shaft() {
        for len in 1..=2 {
            let (x, _) = number_at(len);
            assert!(x >= SHAFT_X && x + 4 * len as i32 <= SHAFT_X + SHAFT_W, "{len} digits at {x}");
        }
        let d = digits();
        for i in 0..10 {
            for j in i + 1..10 {
                assert_ne!(d[i], d[j], "digits {i} and {j}");
            }
        }
    }

    #[test]
    fn lights_change_the_storey_and_wallpapers_differ() {
        assert_ne!(storey(0, true), storey(0, false));
        assert_ne!(storey(0, true), storey(1, true));
        assert_eq!(storey(0, true), storey(WALLPAPERS, true));
    }

    #[test]
    fn agent_behind_desk_looks_like_agent_then_desk() {
        // The board draws the cut mascot over a storey with the desk baked
        // in; that must match drawing the mascot and then the desk on top,
        // except under the desk's translucent glow.
        let m = mascot(&frame(Behavior::Work, 0, 1), |_| BODY);
        let s = slot(2);
        let (dx, dy) = desk_at(2);
        let mut room = shell(WALLPAPER[0]);
        let mut expected = room.clone();
        expected.draw(&m, s.mascot.0, s.mascot.1);
        expected.draw(&desk(), dx, dy);
        room.draw(&desk(), dx, dy);
        room.draw(&behind_desk(&m), s.mascot.0, s.mascot.1);
        let d = desk();
        for y in 0..STOREY_H as i32 {
            for x in 0..W as i32 {
                let (ix, iy) = (x - dx, y - dy);
                let glow = d.inside(ix, iy) && (1..0xff).contains(&d.get(ix as usize, iy as usize)[3]);
                if !glow {
                    assert_eq!(room.get(x as usize, y as usize), expected.get(x as usize, y as usize), "({x}, {y})");
                }
            }
        }
    }

    #[test]
    fn every_colour_fits_the_snes_15_bits() {
        let [on, off] = roof();
        let m = mascot(&frame(Behavior::Work, 0, 0), |_| [0xd9, 0x77, 0x57, 0xff]);
        let [d0, ..] = digits();
        for img in [backdrop(), street(), lobby(), storey(1, true), storey(3, false), on, off, car(), plumbob(), m, d0] {
            assert!(img.px.iter().all(|p| p[0] & 7 == 0 && p[1] & 7 == 0 && p[2] & 7 == 0));
        }
    }

    #[test]
    fn uses_many_colours_but_not_too_many() {
        // A 16-bit look: well beyond the NES's handful, within a SNES screen's 256.
        let mut colours: Vec<Rgba> = backdrop().px;
        colours.sort();
        colours.dedup();
        assert!((40..=256).contains(&colours.len()), "{} colours", colours.len());
    }

    #[test]
    fn mascot_is_flat_with_dark_outline_and_eyes() {
        let m = mascot(&frame(Behavior::Work, 0, 0), |_| BODY);
        assert!(m.px.iter().all(|p| p[3] == 0 || *p == BODY || *p == OUTLINE), "only body and outline");
        // Each eye is a gap two pixels tall in the face, filled with the outline.
        let eye = (ORIGIN.1 + 1) * 2;
        let eyes = (1..m.w - 1)
            .filter(|&x| {
                m.get(x, eye) == OUTLINE && m.get(x, eye + 1) == OUTLINE && m.get(x - 1, eye) == BODY && m.get(x + 1, eye) == BODY
            })
            .count();
        assert_eq!(eyes, 2);
        assert_eq!(m.get(ORIGIN.0 + WIDTH / 2, ORIGIN.1 * 2 - 1), OUTLINE, "outline above the head");
    }

    #[test]
    fn sleeping_mascot_has_drowsy_eyes() {
        let m = mascot(&frame(Behavior::Sleep, 0, 0), |_| BODY);
        // Half shut: the eye's upper pixel is lid, the lower one dark.
        let eye = (ORIGIN.1 + 1) * 2;
        let eyes = (1..m.w - 1)
            .filter(|&x| {
                m.get(x, eye) == BODY
                    && m.get(x, eye + 1) == OUTLINE
                    && m.get(x - 1, eye + 1) == BODY
                    && m.get(x + 1, eye + 1) == BODY
            })
            .count();
        assert_eq!(eyes, 2);
    }

    #[test]
    fn arrows_point_up_and_down() {
        let (up, down) = (arrow(true), arrow(false));
        let lit_row = |img: &Image, y: usize| (0..img.w).filter(|&x| img.get(x, y) == rgb(0xf8b040)).count();
        // The arrow is narrow at its tip: near the top for up, the bottom for down.
        assert!(lit_row(&up, 4) < lit_row(&up, 7));
        assert!(lit_row(&down, ARROW_SIZE - 5) < lit_row(&down, ARROW_SIZE - 8));
        for img in [up, down] {
            assert!(img.px.iter().all(|p| p[0] & 7 == 0 && p[1] & 7 == 0 && p[2] & 7 == 0));
        }
    }

    #[test]
    fn translucent_glows_blend_into_what_is_below() {
        let mut img = Image::new(1, 1);
        img.set(0, 0, rgb(0x000000));
        img.blend(0, 0, rgb(0xf8f8f8), 0.5);
        let p = img.get(0, 0);
        assert!(p[0] > 0x60 && p[0] < 0x90 && p[3] == 0xff, "{p:?}");
    }

    #[test]
    fn roof_frames_differ_only_around_the_light() {
        let [on, off] = roof();
        let w = on.w;
        for (i, (a, b)) in on.px.iter().zip(&off.px).enumerate() {
            if a != b {
                let (x, y) = (i % w, i / w);
                assert!(x >= w - 20 && y < 6, "({x}, {y})");
            }
        }
    }
}
