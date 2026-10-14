//! A pixel-art tower seen from the side at night, after tower-building games
//! like SimTower and Tiny Tower: storeys of desks
//! stacked on a lobby, one or more per herdr workspace, the Claude Code mascot
//! at each desk, animated by what its agent is doing, and an elevator that
//! rides to the focused agent.
//!
//! The art is PNG files in `assets/`, shown pixel for pixel. Full-width pieces
//! have a file each; small sprites, the decor and the mascot's animations
//! share `sprites.png`, and `sprites.yaml` says where each one is. Colours are
//! cut to 15 bits on loading. This crate loads them and puts them together:
//! desks onto storeys, lamps lit or the room tinted by moonlight, and the
//! mascot's frames cut where their desk hides them. The backdrop (sky) stays
//! still; the street, lobby, storeys, decor, roof, elevator car and mascots
//! are separate images that the firmware stacks at the positions given here,
//! and scrolls when the tower is taller than the screen.
//!
//! The positions here must match the art: the tests check that they do.

use herdr_panel_protocol::Status;
use serde::Deserialize;

/// Size of the screen in art pixels.
pub const W: usize = 320;
pub const H: usize = 480;
/// Screen pixels per art pixel.
pub const SCALE: usize = 1;
const STREET_H: usize = 17;

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

    /// The part of the picture at (`x`, `y`), `w` x `h`.
    fn crop(&self, x: usize, y: usize, w: usize, h: usize) -> Image {
        let mut out = Image::new(w, h);
        for yy in 0..h {
            for xx in 0..w {
                out.px[yy * w + xx] = self.get(x + xx, y + yy);
            }
        }
        out
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

/// Decodes a PNG of any colour type into RGBA, with colours cut to 15 bits.
/// Panics on a broken file: the assets are part of the source.
fn load(name: &str, bytes: &[u8]) -> Image {
    let mut decoder = png::Decoder::new(bytes);
    decoder.set_transformations(png::Transformations::normalize_to_color8());
    let mut reader = decoder.read_info().unwrap_or_else(|e| panic!("{name}: {e}"));
    let mut buf = vec![0; reader.output_buffer_size()];
    let info = reader.next_frame(&mut buf).unwrap_or_else(|e| panic!("{name}: {e}"));
    let channels = info.color_type.samples();
    let mut img = Image::new(info.width as usize, info.height as usize);
    for (i, px) in buf[..info.buffer_size()].chunks(channels).enumerate() {
        let c = match px {
            [r, g, b, a] => [*r, *g, *b, *a],
            [r, g, b] => [*r, *g, *b, 0xff],
            [v, a] => [*v, *v, *v, *a],
            [v] => [*v, *v, *v, 0xff],
            _ => unreachable!("{name}: {channels} channels"),
        };
        img.px[i] = if c[3] == 0 { [0; 4] } else { snes(c) };
    }
    img
}

macro_rules! asset {
    ($name:literal) => {
        load($name, include_bytes!(concat!("../../assets/", $name)))
    };
}

/// Where each small sprite is in `sprites.png`: `[x, y, width, height]`.
#[derive(Deserialize)]
#[serde(rename_all = "kebab-case", deny_unknown_fields)]
struct Sprites {
    desk: [usize; 4],
    lamp_on: [usize; 4],
    moonbeam: [usize; 4],
    car: [usize; 4],
    arrow_up: [usize; 4],
    arrow_down: [usize; 4],
    plumbob: [usize; 4],
    digits: [usize; 4],
    decor_0: [usize; 4],
    decor_1: [usize; 4],
    decor_2: [usize; 4],
    decor_3: [usize; 4],
    mascot_working: [usize; 4],
    mascot_idle: [usize; 4],
    mascot_awake: [usize; 4],
    mascot_blocked: [usize; 4],
    mascot_done: [usize; 4],
    mascot_unknown: [usize; 4],
}

impl Sprites {
    fn load() -> Self {
        let yaml = include_str!("../../assets/sprites.yaml");
        serde_saphyr::from_str(yaml).unwrap_or_else(|e| panic!("sprites.yaml: {e}"))
    }

    #[cfg(test)]
    fn all(&self) -> [(&'static str, [usize; 4]); 18] {
        [
            ("desk", self.desk),
            ("lamp-on", self.lamp_on),
            ("moonbeam", self.moonbeam),
            ("car", self.car),
            ("arrow-up", self.arrow_up),
            ("arrow-down", self.arrow_down),
            ("plumbob", self.plumbob),
            ("digits", self.digits),
            ("decor-0", self.decor_0),
            ("decor-1", self.decor_1),
            ("decor-2", self.decor_2),
            ("decor-3", self.decor_3),
            ("mascot-working", self.mascot_working),
            ("mascot-idle", self.mascot_idle),
            ("mascot-awake", self.mascot_awake),
            ("mascot-blocked", self.mascot_blocked),
            ("mascot-done", self.mascot_done),
            ("mascot-unknown", self.mascot_unknown),
        ]
    }
}

/// The small sprite that `pick` locates, cut out of `sprites.png`.
fn sprite(pick: impl Fn(&Sprites) -> [usize; 4]) -> Image {
    let [x, y, w, h] = pick(&Sprites::load());
    asset!("sprites.png").crop(x, y, w, h)
}

// The tower fills the screen's width and stands on the pavement at GROUND_Y.
// Inside, from the left: brick wall, elevator shaft, rooms, brick wall.
// Storey `level` (0 is the lobby) has its top at `storey_top(level)`, which
// goes negative for storeys above the screen: the firmware scrolls up to them.
// Every position below is in pixels on the screen at the bottom of that
// scroll, or, where it says so, relative to a storey's top left corner.
pub const GROUND_Y: i32 = (H - STREET_H) as i32;
const WALL: i32 = 4;
const SHAFT_X: i32 = WALL;
const SHAFT_W: i32 = 14;
const ROOMS_X: i32 = SHAFT_X + SHAFT_W + 1;
const ROOMS_END: i32 = W as i32 - WALL;
/// Height of a storey, including the floor slab at its bottom.
pub const STOREY_H: usize = 36;
const SLAB: i32 = 3;
/// Height of a storey's room, above the slab.
const INNER: i32 = STOREY_H as i32 - SLAB;
pub const DESKS_PER_STOREY: usize = 8;
/// The desks start this far from a storey's left edge, one every
/// `DESK_SPACING` pixels; for telling which desk a tap is on.
pub const DESKS_X: i32 = ROOMS_X + 1;
pub const DESK_SPACING: i32 = 33;
/// Height of the roof, from the antenna's tip to the parapet, which overlaps
/// the top storey by a pixel.
pub const ROOF_H: usize = 22;
/// How many wallpapers storeys cycle through, one per workspace: the
/// `storey-N.png` files.
pub const WALLPAPERS: usize = 4;
/// How many decor sprites storeys cycle through, one per storey.
pub const DECORS: usize = 4;
/// Where a storey's decor goes, relative to the storey: the room's right end.
pub const DECOR_AT: (i32, i32) = (DESKS_X + DESK_SPACING * DESKS_PER_STOREY as i32, 0);
/// How far the top of the plumbob is above the agent's feet.
pub const PLUMBOB_ABOVE: i32 = 25;
/// Size of an [`arrow`] button, and its left edge: over the elevator shaft.
pub const ARROW_SIZE: usize = 14;
pub const ARROW_X: i32 = SHAFT_X;
/// Where the car stops in a storey, relative to the storey: on its floor.
pub const CAR_AT: (i32, i32) = (SHAFT_X + 1, INNER - CAR_H);
const CAR_H: i32 = 24;

/// What a dark storey's room is multiplied by: moonlight blue.
const MOONLIGHT: Rgba = [0x58, 0x68, 0xb0, 0xff];
/// The middle column of the `lamp-on` sprite, which goes over each lamp fitting.
const LAMP_CENTRE: i32 = 18;
/// Where the `moonbeam` sprite goes, relative to the centre of a window.
const MOONBEAM_AT: (i32, i32) = (-7, 22);

/// The top of storey `level`, where 0 is the lobby.
pub fn storey_top(level: usize) -> i32 {
    GROUND_Y - STOREY_H as i32 * (level as i32 + 1)
}

/// The top of the roof over a tower of `storeys` storeys above the lobby.
pub fn roof_top(storeys: usize) -> i32 {
    storey_top(storeys) - ROOF_H as i32 + 1
}

/// What stays still behind the tower: the night sky and the moon.
pub fn backdrop() -> Image {
    asset!("backdrop.png")
}

/// The street in front of the tower, from GROUND_Y to the bottom of the
/// screen.
pub fn street() -> Image {
    asset!("street.png")
}

/// The lobby, storey 0.
pub fn lobby() -> Image {
    asset!("lobby.png")
}

/// A storey's room before desks and lighting: walls, shaft, wallpaper,
/// windows and unlit lamp fittings.
fn room(paper: usize) -> Image {
    match paper % WALLPAPERS {
        0 => asset!("storey-0.png"),
        1 => asset!("storey-1.png"),
        2 => asset!("storey-2.png"),
        _ => asset!("storey-3.png"),
    }
}

fn desk_centre(i: usize) -> i32 {
    DESKS_X + DESK_SPACING * i as i32 + DESK_SPACING / 2
}

/// The lamp fittings, between the desks.
fn lamp_xs() -> impl Iterator<Item = i32> {
    (1..DESKS_PER_STOREY).map(|i| DESKS_X + DESK_SPACING * i as i32)
}

/// `img` as seen by moonlight: every colour multiplied by moonlight blue.
fn moonlit(img: &mut Image, x0: i32, x1: i32, rows: i32) {
    for y in 0..rows.min(img.h as i32) {
        for x in x0.max(0)..x1.min(img.w as i32) {
            let c = img.get(x as usize, y as usize);
            if c[3] != 0 {
                let tint = |i: usize| (c[i] as u32 * MOONLIGHT[i] as u32 / 255) as u8;
                img.set(x, y, [tint(0), tint(1), tint(2), c[3]]);
            }
        }
    }
}

/// A storey of offices in wallpaper `paper` (counted modulo
/// [`WALLPAPERS`]), with its desks, and its lamps on if `lit`; otherwise the
/// room is tinted by moonlight falling through the windows. Agents are drawn
/// over it with [`behind_desk`]; its [`decor`] and number go on separately.
pub fn storey(paper: usize, lit: bool) -> Image {
    let mut img = room(paper);
    let desk = desk();
    for i in 0..DESKS_PER_STOREY {
        let (dx, dy) = desk_at(i);
        img.draw(&desk, dx, dy);
    }
    if lit {
        let lamp = sprite(|s| s.lamp_on);
        for x in lamp_xs() {
            img.draw(&lamp, x - LAMP_CENTRE, 0);
        }
    } else {
        moonlit(&mut img, ROOMS_X, ROOMS_END, INNER + 1);
        let beam = sprite(|s| s.moonbeam);
        for i in 0..DESKS_PER_STOREY {
            img.draw(&beam, desk_centre(i) + MOONBEAM_AT.0, MOONBEAM_AT.1);
        }
    }
    img
}

/// Decor `kind` (counted modulo [`DECORS`]) for a storey's right end, lit or
/// by moonlight. It goes at [`DECOR_AT`].
pub fn decor(kind: usize, lit: bool) -> Image {
    let mut img = match kind % DECORS {
        0 => sprite(|s| s.decor_0),
        1 => sprite(|s| s.decor_1),
        2 => sprite(|s| s.decor_2),
        _ => sprite(|s| s.decor_3),
    };
    if !lit {
        let (w, h) = (img.w as i32, img.h as i32);
        moonlit(&mut img, 0, w, h);
    }
    img
}

/// The digits 0 to 9 for storey numbers, cut from the `digits` strip.
pub fn digits() -> [Image; 10] {
    let sheet = sprite(|s| s.digits);
    core::array::from_fn(|d| sheet.crop(4 * d, 0, 4, sheet.h))
}

/// Where the first of a storey number's `len` digits goes, relative to the
/// storey; each next digit goes 4 pixels to the right.
pub fn number_at(len: usize) -> (i32, i32) {
    (SHAFT_X + SHAFT_W / 2 - 2 * len as i32 + 1, 2)
}

/// The roof over the top storey, its antenna light on in frame 0 and off in
/// frame 1. Its top goes at [`roof_top`].
pub fn roof() -> [Image; 2] {
    [asset!("roof-on.png"), asset!("roof-off.png")]
}

/// The elevator car.
pub fn car() -> Image {
    sprite(|s| s.car)
}

/// The button that scrolls up a storey if `up`, else down.
pub fn arrow(up: bool) -> Image {
    sprite(|s| if up { s.arrow_up } else { s.arrow_down })
}

/// The marker over the focused agent. Its top goes [`PLUMBOB_ABOVE`] over
/// the agent's feet.
pub fn plumbob() -> Image {
    sprite(|s| s.plumbob)
}

/// Size of the desk sprite: a desk in oblique projection with a monitor.
#[cfg(test)]
const DESK_W: usize = 19;
const DESK_H: usize = 14;

fn desk() -> Image {
    sprite(|s| s.desk)
}

/// Where desk `i`'s top left corner is, relative to the storey: in front of
/// the right half of its agent.
fn desk_at(i: usize) -> (i32, i32) {
    (desk_centre(i) - 5, INNER - DESK_H as i32)
}

/// Where the agent at desk `i` goes, relative to its storey.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Slot {
    /// The point between the agent's feet.
    pub feet: (i32, i32),
    /// Where the top left corner of its mascot frame goes.
    pub mascot: (i32, i32),
}

/// Size of a mascot frame, and where the mascot's feet are in it.
pub const MASCOT_W: usize = 30;
pub const MASCOT_H: usize = 26;
const MASCOT_FEET: (i32, i32) = (13, 21);

/// The agent's place at desk `i` (0 to [`DESKS_PER_STOREY`] - 1): left of the
/// monitor, high enough to see over the desk.
pub fn slot(i: usize) -> Slot {
    let feet = (desk_centre(i) - 4, INNER - 4);
    Slot { feet, mascot: (feet.0 - MASCOT_FEET.0, feet.1 - MASCOT_FEET.1) }
}

/// The mascot's animation for an agent with `status`: its frames, cut from
/// its strip in `sprites.png`, and how long each one shows, in milliseconds.
pub fn animation(status: Status) -> (Vec<Image>, u32) {
    match status {
        Status::Working => (frames(sprite(|s| s.mascot_working)), 150),
        Status::Idle => (frames(sprite(|s| s.mascot_idle)), 500),
        Status::Blocked => (frames(sprite(|s| s.mascot_blocked)), 250),
        Status::Done => (frames(sprite(|s| s.mascot_done)), 150),
        Status::Unknown => (frames(sprite(|s| s.mascot_unknown)), 1000),
    }
}

/// The animation for an idle agent that has the focus: awake, blinking now
/// and then, instead of dozing. Like [`animation`].
pub fn awake_animation() -> (Vec<Image>, u32) {
    (frames(sprite(|s| s.mascot_awake)), 500)
}

/// The frames of a mascot strip.
fn frames(strip: Image) -> Vec<Image> {
    (0..strip.w / MASCOT_W).map(|i| strip.crop(i * MASCOT_W, 0, MASCOT_W, MASCOT_H)).collect()
}

/// Every animation, for the tests.
#[cfg(test)]
fn all_animations() -> Vec<(&'static str, Vec<Image>)> {
    let mut out: Vec<_> = [Status::Working, Status::Idle, Status::Blocked, Status::Done, Status::Unknown]
        .into_iter()
        .map(|s| (format!("{s:?}").leak() as &'static str, animation(s).0))
        .collect();
    out.push(("awake", awake_animation().0));
    out
}

/// A mascot frame with the pixels its desk hides cut away, so that drawn over
/// a [`storey`] at a [`Slot`] it stands behind the desk drawn into the storey.
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

#[cfg(test)]
mod tests {
    use super::*;

    fn size(img: &Image) -> (usize, usize) {
        (img.w, img.h)
    }

    /// The first and last column with anything drawn in `img`.
    fn drawn_columns(img: &Image) -> (i32, i32) {
        let xs = (0..img.w).filter(|&x| (0..img.h).any(|y| img.get(x, y)[3] != 0));
        (xs.clone().min().unwrap() as i32, xs.max().unwrap() as i32)
    }

    #[test]
    fn assets_have_the_sizes_the_layout_expects() {
        assert_eq!(size(&backdrop()), (W, H));
        assert_eq!(size(&street()), (W, H - GROUND_Y as usize));
        assert_eq!(size(&lobby()), (W, STOREY_H));
        for p in 0..WALLPAPERS {
            assert_eq!(size(&room(p)), (W, STOREY_H), "storey-{p}.png");
        }
        for r in roof() {
            assert_eq!(size(&r), (W, ROOF_H));
        }
        assert_eq!(size(&desk()), (DESK_W, DESK_H));
        for up in [true, false] {
            assert_eq!(size(&arrow(up)), (ARROW_SIZE, ARROW_SIZE));
        }
        assert_eq!(size(&sprite(|s| s.digits)), (40, 6));
        assert_eq!(sprite(|s| s.lamp_on).w as i32, 2 * LAMP_CENTRE + 1);
        assert_eq!(car().h as i32, CAR_H);
        for k in 0..DECORS {
            let d = decor(k, true);
            assert!(DECOR_AT.0 + d.w as i32 <= ROOMS_END + WALL && d.h as i32 <= INNER, "decor {k}");
        }
    }

    #[test]
    fn sprites_sit_inside_the_sheet_without_overlapping() {
        let sheet = asset!("sprites.png");
        let all = Sprites::load().all();
        for (name, [x, y, w, h]) in all {
            assert!(w > 0 && h > 0 && x + w <= sheet.w && y + h <= sheet.h, "{name} is outside sprites.png");
        }
        for (i, (a, [ax, ay, aw, ah])) in all.iter().enumerate() {
            for (b, [bx, by, bw, bh]) in &all[i + 1..] {
                let apart = ax + aw <= *bx || bx + bw <= *ax || ay + ah <= *by || by + bh <= *ay;
                assert!(apart, "{a} and {b} overlap");
            }
        }
    }

    #[test]
    fn a_misspelt_or_missing_sprite_is_an_error() {
        let good = include_str!("../../assets/sprites.yaml");
        assert!(serde_saphyr::from_str::<Sprites>(good).is_ok());
        assert!(serde_saphyr::from_str::<Sprites>(&good.replace("plumbob:", "plumbbob:")).is_err());
        let missing: String = good.lines().filter(|l| !l.starts_with("car:")).map(|l| format!("{l}\n")).collect();
        assert!(serde_saphyr::from_str::<Sprites>(&missing).is_err());
    }

    #[test]
    fn backgrounds_are_opaque() {
        for (name, img) in [("backdrop", backdrop()), ("street", street()), ("lobby", lobby())] {
            assert!(img.px.iter().all(|p| p[3] == 0xff), "{name}");
        }
        for p in 0..WALLPAPERS {
            for lit in [true, false] {
                assert!(storey(p, lit).px.iter().all(|p| p[3] == 0xff), "storey {p} lit {lit}");
            }
        }
    }

    #[test]
    fn colours_are_cut_to_15_bits() {
        let mut img = load("test", &{
            let mut out = Vec::new();
            let mut enc = png::Encoder::new(&mut out, 1, 1);
            enc.set_color(png::ColorType::Rgb);
            enc.write_header().unwrap().write_image_data(&[0xff, 0x7f, 0x01]).unwrap();
            out
        });
        assert_eq!(img.px[0], [0xf8, 0x78, 0x00, 0xff]);
        img.blend(0, 0, [0x13, 0x13, 0x13, 0xff], 0.5);
        assert!(img.px[0][..3].iter().all(|c| c & 7 == 0));
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
    fn desks_and_decor_fit_the_rooms_and_taps_find_desks() {
        for i in 0..DESKS_PER_STOREY {
            let (x, _) = desk_at(i);
            assert!(x >= ROOMS_X && x + DESK_W as i32 <= ROOMS_END, "desk {i} at {x}");
            assert_eq!(((desk_centre(i) - DESKS_X) / DESK_SPACING) as usize, i);
        }
        assert!(DECOR_AT.0 >= desk_at(DESKS_PER_STOREY - 1).0 + DESK_W as i32);
    }

    #[test]
    fn every_status_has_whole_frames() {
        let sprites = Sprites::load();
        for [_, _, w, h] in [
            sprites.mascot_working,
            sprites.mascot_idle,
            sprites.mascot_awake,
            sprites.mascot_blocked,
            sprites.mascot_done,
            sprites.mascot_unknown,
        ] {
            assert!(w >= MASCOT_W && w % MASCOT_W == 0 && h == MASCOT_H, "a mascot strip is {w}x{h}");
        }
        for (name, frames) in all_animations() {
            assert!(!frames.is_empty(), "{name}");
        }
    }

    #[test]
    fn mascots_stay_in_the_rooms_and_clear_of_each_other() {
        for (s, frames) in all_animations() {
            for m in frames {
                let (lo, hi) = drawn_columns(&m);
                for i in 0..DESKS_PER_STOREY {
                    let x = slot(i).mascot.0;
                    assert!(x + lo >= ROOMS_X && x + hi < ROOMS_END, "{s} at desk {i}: {}..{}", x + lo, x + hi);
                    if i > 0 {
                        assert!(x + lo > slot(i - 1).mascot.0 + hi, "{s} at desk {i} overlaps its neighbour");
                    }
                }
            }
        }
    }

    #[test]
    fn mascots_stay_inside_their_storey() {
        // Every drawn pixel, markers included, is inside the room.
        for (st, frames) in all_animations() {
            for m in frames {
                for y in 0..m.h {
                    if (0..m.w).any(|x| m.get(x, y)[3] != 0) {
                        let at = slot(0).mascot.1 + y as i32;
                        assert!((0..INNER).contains(&at), "{st} row {y} at {at}");
                    }
                }
            }
        }
        assert!(slot(0).feet.1 - PLUMBOB_ABOVE >= 0);
    }

    #[test]
    fn car_fits_the_shaft_and_storey() {
        let c = car();
        assert!(CAR_AT.0 >= SHAFT_X && CAR_AT.0 + c.w as i32 <= SHAFT_X + SHAFT_W);
        assert!(CAR_AT.1 >= 0 && CAR_AT.1 + c.h as i32 <= INNER);
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
    fn lights_change_the_storey_and_decor_and_wallpapers_differ() {
        assert_ne!(storey(0, true), storey(0, false));
        assert_ne!(storey(0, true), storey(1, true));
        assert_eq!(storey(0, true), storey(WALLPAPERS, true));
        assert_ne!(decor(0, true), decor(0, false));
        assert_eq!(decor(0, true), decor(DECORS, true));
    }

    #[test]
    fn agent_behind_desk_looks_like_agent_then_desk() {
        // The board draws the cut mascot over a storey with the desk drawn
        // in; that must match drawing the mascot and then the desk on top.
        for m in animation(Status::Working).0 {
            let s = slot(2);
            let (dx, dy) = desk_at(2);
            let mut room = room(0);
            let mut expected = room.clone();
            expected.draw(&m, s.mascot.0, s.mascot.1);
            expected.draw(&desk(), dx, dy);
            room.draw(&desk(), dx, dy);
            room.draw(&behind_desk(&m), s.mascot.0, s.mascot.1);
            assert_eq!(room, expected);
        }
    }

    #[test]
    fn translucent_pixels_blend_into_what_is_below() {
        let mut img = Image::new(1, 1);
        img.set(0, 0, [0, 0, 0, 0xff]);
        img.blend(0, 0, [0xf8, 0xf8, 0xf8, 0xff], 0.5);
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
                assert!(x >= w - 30 && y < 8, "({x}, {y})");
            }
        }
    }
}
