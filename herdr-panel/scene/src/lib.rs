//! A tower seen from the side at night, in the style of an 8-bit console game
//! (a small fixed palette, ordered dithering, dark outlines), after
//! tower-building games like SimTower: one storey per herdr workspace stacked
//! on a lobby, a desk per agent, and an elevator that rides to the focused
//! agent.
//!
//! Everything is drawn at half the screen's resolution and shown at twice the
//! size, so one art pixel is 2x2 screen pixels. The background holds the sky,
//! the street and the lobby; storeys, the roof, desks, the elevator car and
//! the mascots are separate sprites that the firmware stacks at the positions
//! given here, all in art pixels.

use herdr_panel_art::{CANVAS_H, CANVAS_W, Grid, HEIGHT, Ink, ORIGIN, WIDTH};

/// Size of the background in art pixels.
pub const W: usize = 160;
pub const H: usize = 240;
/// Screen pixels per art pixel.
pub const SCALE: usize = 2;
/// Height of the band at the top kept clear for the title, in art pixels.
pub const HEADER_H: usize = 18;

pub type Rgba = [u8; 4];

/// An RGBA picture; fully transparent where nothing is drawn.
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

    fn set(&mut self, x: i32, y: i32, c: Rgba) {
        if x >= 0 && y >= 0 && (x as usize) < self.w && (y as usize) < self.h {
            self.px[y as usize * self.w + x as usize] = c;
        }
    }

    /// Fills the rectangle at (`x`, `y`) of size `w` x `h`.
    fn fill(&mut self, x: i32, y: i32, w: i32, h: i32, c: Rgba) {
        for yy in y..y + h {
            for xx in x..x + w {
                self.set(xx, yy, c);
            }
        }
    }

    /// Fills the rectangle with an outline of colour `line`.
    fn boxed(&mut self, x: i32, y: i32, w: i32, h: i32, c: Rgba, line: Rgba) {
        self.fill(x, y, w, h, line);
        self.fill(x + 1, y + 1, w - 2, h - 2, c);
    }

    /// Draws `other` with its top left corner at (`x`, `y`), skipping its
    /// transparent pixels.
    pub fn draw(&mut self, other: &Image, x: i32, y: i32) {
        for oy in 0..other.h {
            for ox in 0..other.w {
                let c = other.get(ox, oy);
                if c[3] != 0 {
                    self.set(x + ox as i32, y + oy as i32, c);
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
    [(hex >> 16) as u8, (hex >> 8) as u8, hex as u8, 0xff]
}

// The palette: PICO-8's sixteen colours, close to what 8-bit consoles showed.
const BLACK: Rgba = rgb(0x000000);
const NAVY: Rgba = rgb(0x1d2b53);
const PLUM: Rgba = rgb(0x7e2553);
const GREEN: Rgba = rgb(0x008751);
const BROWN: Rgba = rgb(0xab5236);
const GREY_DARK: Rgba = rgb(0x5f574f);
const GREY: Rgba = rgb(0xc2c3c7);
const WHITE: Rgba = rgb(0xfff1e8);
const RED: Rgba = rgb(0xff004d);
const ORANGE: Rgba = rgb(0xffa300);
const YELLOW: Rgba = rgb(0xffec27);
const LIME: Rgba = rgb(0x00e436);
const BLUE: Rgba = rgb(0x29adff);
const LAVENDER: Rgba = rgb(0x83769c);
const PINK: Rgba = rgb(0xff77a8);
const PEACH: Rgba = rgb(0xffccaa);

/// The 4x4 ordered-dither threshold for pixel (`x`, `y`), in 0..16.
fn bayer(x: i32, y: i32) -> u32 {
    const M: [[u32; 4]; 4] = [[0, 8, 2, 10], [12, 4, 14, 6], [3, 11, 1, 9], [15, 7, 13, 5]];
    M[(y & 3) as usize][(x & 3) as usize]
}

/// `a` blended into `b` by `t` (0..1) with ordered dithering, the way 8-bit
/// games faked gradients.
fn dither(x: i32, y: i32, a: Rgba, b: Rgba, t: f32) -> Rgba {
    if (t * 16.0) as u32 > bayer(x, y) { b } else { a }
}

fn hash(a: i32, b: i32) -> u32 {
    let mut h = (a as u32).wrapping_mul(0x9E37_79B1) ^ (b as u32).wrapping_mul(0x85EB_CA77);
    h ^= h >> 15;
    h = h.wrapping_mul(0x2C1B_3C6D);
    h ^ (h >> 12)
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

fn text(img: &mut Image, x: i32, y: i32, s: &str, c: Rgba) {
    for (i, ch) in s.chars().enumerate() {
        for (dy, row) in glyph(ch).iter().enumerate() {
            for (dx, p) in row.chars().enumerate() {
                if p == '#' {
                    img.set(x + i as i32 * 4 + dx as i32, y + dy as i32, c);
                }
            }
        }
    }
}

// The tower: BUILDING_W wide from BUILDING_X, standing on the pavement at
// GROUND_Y. Inside, from the left: brick wall, elevator shaft, rooms, wall.
const GROUND_Y: i32 = 223;
const BUILDING_X: i32 = 20;
pub const BUILDING_W: usize = 120;
const WALL: i32 = 4;
const SHAFT_X: i32 = WALL;
const SHAFT_W: i32 = 14;
const ROOMS_X: i32 = SHAFT_X + SHAFT_W + 1;
/// Height of a storey, including the floor slab at its bottom.
pub const FLOOR_H: usize = 24;
const SLAB: i32 = 3;
/// Storeys above the lobby that fit below the title.
pub const MAX_FLOORS: usize = 7;
pub const DESKS_PER_FLOOR: usize = 4;
const DESK_SPACING: i32 = 24;
const ROOF_H: usize = 14;

/// Wallpaper colours, one per storey in turn.
const WALLPAPER: [Rgba; 4] = [PEACH, GREY, LAVENDER, BLUE];

/// The top of storey `level`, where 0 is the lobby.
fn storey_y(level: usize) -> i32 {
    GROUND_Y - FLOOR_H as i32 * (level as i32 + 1)
}

/// The sky, the skyline behind the tower, the street, and the tower's lobby.
pub fn background() -> Image {
    let mut img = Image::new(W, H);
    // Night sky in dithered bands: black, navy, then plum near the horizon.
    for y in 0..GROUND_Y {
        let t = y as f32 / GROUND_Y as f32;
        for x in 0..W as i32 {
            let c = if t < 0.5 { dither(x, y, BLACK, NAVY, t / 0.5) } else { dither(x, y, NAVY, PLUM, (t - 0.5) / 0.5 * 0.7) };
            img.set(x, y, c);
        }
    }
    for y in HEADER_H as i32..GROUND_Y - 60 {
        for x in 0..W as i32 {
            match hash(x, y) % 300 {
                0 => img.set(x, y, WHITE),
                1 => img.set(x, y, LAVENDER),
                _ => {}
            }
        }
    }
    // A twinkle: a star with four points.
    for (dx, dy) in [(0, 0), (1, 0), (-1, 0), (0, 1), (0, -1)] {
        img.set(16 + dx, 44 + dy, WHITE);
    }
    // The moon, with a bite out of it.
    for y in -7..=7 {
        for x in -7..=7 {
            let inside = x * x + y * y <= 49;
            let bite = (x + 4) * (x + 4) + (y - 2) * (y - 2) <= 30;
            if inside && !bite {
                img.set(151 + x, 34 + y, if x * x + y * y > 36 { YELLOW } else { WHITE });
            }
        }
    }
    // Skyline: outlined blocks of random height with a few lit windows.
    let mut x = -4;
    let mut i = 0;
    while x < W as i32 {
        let w = 12 + (hash(i, 1) % 12) as i32;
        let h = 36 + (hash(i, 2) % 60) as i32;
        img.boxed(x, GROUND_Y - h, w, h + 1, NAVY, BLACK);
        for wy in (GROUND_Y - h + 4..GROUND_Y - 4).step_by(6) {
            for wx in (x + 3..x + w - 3).step_by(4) {
                if hash(wx, wy) % 5 == 0 {
                    img.fill(wx, wy, 2, 3, if hash(wy, wx) % 3 == 0 { ORANGE } else { YELLOW });
                }
            }
        }
        x += w - 1;
        i += 1;
    }
    // Street: pavement in slabs, kerb, road with a dashed line.
    img.fill(0, GROUND_Y, W as i32, 1, BLACK);
    img.fill(0, GROUND_Y + 1, W as i32, 4, GREY);
    for x in (0..W as i32).step_by(8) {
        img.fill(x, GROUND_Y + 1, 1, 4, LAVENDER);
    }
    img.fill(0, GROUND_Y + 5, W as i32, 1, GREY_DARK);
    img.fill(0, GROUND_Y + 6, W as i32, H as i32 - GROUND_Y - 6, NAVY);
    for x in (2..W as i32).step_by(14) {
        img.fill(x, GROUND_Y + 11, 7, 2, YELLOW);
    }
    // A street lamp with a pool of light.
    img.fill(150, GROUND_Y - 30, 2, 30, BLACK);
    img.fill(145, GROUND_Y - 32, 8, 3, BLACK);
    img.fill(146, GROUND_Y - 31, 4, 1, YELLOW);
    for y in GROUND_Y + 1..GROUND_Y + 5 {
        for x in 136..160 {
            if bayer(x, y) < 6 {
                img.set(x, y, WHITE);
            }
        }
    }
    lobby(&mut img);
    img
}

/// Brick wall texture at (`x`, `y`), `w` x `h`.
fn bricks(img: &mut Image, x: i32, y: i32, w: i32, h: i32) {
    for yy in 0..h {
        for xx in 0..w {
            let row = (y + yy) / 3;
            let mortar = (y + yy) % 3 == 2 || (x + xx + row * 3) % 6 == 0;
            img.set(x + xx, y + yy, if mortar { PLUM } else { BROWN });
        }
    }
}

/// Walls, shaft and slab common to every storey, drawn at (`x`, `y`).
fn shell(img: &mut Image, x: i32, y: i32, interior: Rgba, level: usize) {
    let h = FLOOR_H as i32;
    let right = x + BUILDING_W as i32 - WALL;
    img.fill(x, y, BUILDING_W as i32, h, interior);
    bricks(img, x, y, WALL, h);
    bricks(img, right, y, WALL, h);
    img.fill(x, y, 1, h, BLACK);
    img.fill(x + BUILDING_W as i32 - 1, y, 1, h, BLACK);
    // The shaft: dark, with rails and the storey number.
    img.fill(x + SHAFT_X, y, SHAFT_W, h - SLAB, BLACK);
    img.fill(x + SHAFT_X + 2, y, 1, h - SLAB, GREY_DARK);
    img.fill(x + SHAFT_X + SHAFT_W - 3, y, 1, h - SLAB, GREY_DARK);
    img.fill(x + ROOMS_X - 1, y, 1, h - SLAB, BLACK);
    let label = level.to_string();
    text(img, x + SHAFT_X + SHAFT_W / 2 - 2 * label.len() as i32 + 1, y + 2, &label, ORANGE);
    // Floor slab, outlined.
    img.fill(x, y + h - SLAB, BUILDING_W as i32, SLAB, GREY_DARK);
    img.fill(x, y + h - SLAB, BUILDING_W as i32, 1, BLACK);
    img.fill(x + 1, y + h - SLAB + 1, BUILDING_W as i32 - 2, 1, GREY);
}

fn lobby(img: &mut Image) {
    let (x, y) = (BUILDING_X, storey_y(0));
    shell(img, x, y, GREY, 0);
    // Checkered floor tiles, a sign, a reception desk, a plant, glass doors.
    for xx in x + ROOMS_X..x + BUILDING_W as i32 - WALL {
        for yy in y + 18..y + 21 {
            img.set(xx, yy, if (xx / 2 + yy) % 2 == 0 { WHITE } else { GREY });
        }
    }
    text(img, x + ROOMS_X + 3, y + 3, "LOBBY", NAVY);
    img.boxed(x + 30, y + 13, 22, 8, BROWN, BLACK);
    img.fill(x + 31, y + 14, 20, 1, ORANGE);
    img.boxed(x + 60, y + 15, 6, 6, BROWN, BLACK);
    img.boxed(x + 57, y + 7, 12, 9, GREEN, BLACK);
    img.fill(x + 59, y + 9, 3, 2, LIME);
    let door_x = x + 80;
    img.boxed(door_x, y + 6, 28, 15, GREY_DARK, BLACK);
    for dx in [2, 15] {
        img.boxed(door_x + dx, y + 8, 11, 13, BLUE, BLACK);
        img.fill(door_x + dx + 2, y + 10, 1, 4, WHITE);
        img.fill(door_x + dx + 3, y + 10, 1, 2, WHITE);
    }
    // A striped awning over the door.
    for dx in 0..32 {
        let c = if (dx / 4) % 2 == 0 { RED } else { WHITE };
        img.fill(door_x - 2 + dx, y + 2, 1, 3, c);
    }
    img.fill(door_x - 2, y + 5, 32, 1, BLACK);
}

/// Storey `level` (1 is the first above the lobby), with its lights on if
/// `lit`. Draw it at [`floor_pos`].
pub fn floor(level: usize, lit: bool) -> Image {
    let mut img = Image::new(BUILDING_W, FLOOR_H);
    let paper = WALLPAPER[(level - 1) % WALLPAPER.len()];
    shell(&mut img, 0, 0, paper, level);
    let room = ROOMS_X..BUILDING_W as i32 - WALL;
    // Skirting board.
    img.fill(room.start, FLOOR_H as i32 - SLAB - 2, room.end - room.start, 2, BROWN);
    for i in 0..DESKS_PER_FLOOR {
        let cx = desk_centre(i);
        // A window behind each desk, and a lamp above it.
        img.boxed(cx - 6, 3, 13, 9, NAVY, BLACK);
        img.fill(cx, 4, 1, 7, BLACK);
        if hash(level as i32, i as i32) % 3 == 0 {
            img.set(cx - 3, 6, WHITE);
        }
        img.fill(cx - 2, 0, 5, 1, BLACK);
        img.fill(cx - 1, 1, 3, 1, if lit { YELLOW } else { GREY_DARK });
    }
    if !lit {
        // Lights out: the room behind a dithered veil of navy.
        for y in 0..FLOOR_H as i32 - SLAB {
            for x in room.clone() {
                if (x + y) % 2 == 0 {
                    img.set(x, y, NAVY);
                }
            }
        }
    }
    img
}

/// Where storey `level` (1 or more) goes, top left.
pub fn floor_pos(level: usize) -> (i32, i32) {
    (BUILDING_X, storey_y(level))
}

/// The roof over the top storey: a parapet, a water tank, a "HERDR" sign and
/// an antenna whose light is on in frame 0 and off in frame 1.
pub fn roof() -> [Image; 2] {
    let w = BUILDING_W as i32 + 4;
    let h = ROOF_H as i32;
    let frame = |light: bool| {
        let mut img = Image::new(w as usize, ROOF_H);
        img.boxed(0, h - 4, w, 4, GREY, BLACK);
        // Water tank on legs.
        img.boxed(10, h - 13, 12, 8, BROWN, BLACK);
        img.fill(11, h - 10, 10, 1, PLUM);
        img.fill(11, h - 5, 1, 1, BLACK);
        img.fill(20, h - 5, 1, 1, BLACK);
        // Neon sign on a frame.
        img.fill(38, h - 5, 1, 1, BLACK);
        img.fill(52, h - 5, 1, 1, BLACK);
        img.boxed(33, h - 13, 23, 8, BLACK, PLUM);
        text(&mut img, 35, h - 11, "HERDR", PINK);
        // Antenna.
        img.fill(w - 16, 2, 1, h - 6, GREY);
        img.fill(w - 18, 6, 5, 1, GREY);
        img.fill(w - 17, 0, 3, 2, if light { RED } else { PLUM });
        img
    };
    [frame(true), frame(false)]
}

/// Where the roof goes over `floors` storeys above the lobby, top left.
pub fn roof_pos(floors: usize) -> (i32, i32) {
    (BUILDING_X - 2, storey_y(floors) - ROOF_H as i32 + 1)
}

/// The elevator car, with its doors shut.
pub fn car() -> Image {
    let (w, h) = (SHAFT_W - 2, FLOOR_H as i32 - SLAB - 2);
    let mut img = Image::new(w as usize, h as usize);
    img.boxed(0, 0, w, h, GREY, BLACK);
    img.fill(1, 1, w - 2, 2, ORANGE);
    img.fill(2, 4, w - 4, h - 5, LAVENDER);
    img.fill(w / 2, 4, 1, h - 5, BLACK);
    img.fill(3, 5, 1, 4, WHITE);
    img
}

/// Where the car stops at storey `level`, top left.
pub fn car_pos(level: usize) -> (i32, i32) {
    (BUILDING_X + SHAFT_X + 1, storey_y(level) + 2)
}

fn desk_centre(i: usize) -> i32 {
    ROOMS_X + 13 + DESK_SPACING * i as i32
}

const DESK_W: i32 = 22;
const DESK_H: i32 = 4;
const LID_H: i32 = 6;
/// How far left of its desk's centre the agent stands, clear of the monitor.
const AGENT_LEFT: i32 = 3;
/// How far above the floor its feet are, as if on a tall stool.
const AGENT_UP: i32 = 2;

/// A desk seen from the front, with a monitor at its right end seen from
/// behind: drawn over the agent standing behind it, hiding its legs.
pub fn desk() -> Image {
    let mut img = Image::new(DESK_W as usize, (DESK_H + LID_H) as usize);
    img.boxed(DESK_W - 7, 0, 6, LID_H - 1, GREY, BLACK);
    img.fill(DESK_W - 5, LID_H - 1, 2, 1, BLACK);
    img.boxed(0, LID_H, DESK_W, DESK_H, BROWN, BLACK);
    img.fill(1, LID_H + 1, DESK_W - 2, 1, ORANGE);
    img
}

/// Where one agent works, in art pixels on the background.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Slot {
    /// The point between the agent's feet.
    pub feet: (i32, i32),
    /// Where the top left corner of [`mascot`] goes.
    pub mascot: (i32, i32),
    /// Where the top left corner of [`desk`] goes.
    pub desk: (i32, i32),
}

/// Where the mascot's feet are in a [`mascot`] image: the middle of its
/// bottom edge, below the leg row.
const MASCOT_FEET: (i32, i32) = ((ORIGIN.0 + WIDTH / 2) as i32, ((ORIGIN.1 + HEIGHT - 1) * 2) as i32);

/// Desk `i` on storey `level` (1 or more).
pub fn slot(level: usize, i: usize) -> Slot {
    let x = BUILDING_X + desk_centre(i);
    let y = storey_y(level) + FLOOR_H as i32 - SLAB;
    let feet = (x - AGENT_LEFT, y - AGENT_UP);
    Slot {
        feet,
        mascot: (feet.0 - MASCOT_FEET.0, feet.1 - MASCOT_FEET.1),
        desk: (x - DESK_W / 2, y - DESK_H - LID_H + 1),
    }
}

/// The mascot frame `grid` as it is in the terminal: each pixel one wide and
/// two tall, like a character cell. `color` picks each ink's colour. Like a
/// console sprite it gets a black outline, which also fills in the gaps that
/// are its eyes.
pub fn mascot(grid: &Grid, color: impl Fn(Ink) -> Rgba) -> Image {
    let mut img = Image::new(CANVAS_W, CANVAS_H);
    for (y, row) in grid.iter().enumerate() {
        for (x, p) in row.iter().enumerate() {
            if let Some(ink) = p {
                img.px[y * CANVAS_W + x] = color(*ink);
            }
        }
    }
    let img = img.scaled(1, 2);
    let mut out = img.clone();
    for y in 0..img.h as i32 {
        for x in 0..img.w as i32 {
            let solid = |dx: i32, dy: i32| {
                let (nx, ny) = (x + dx, y + dy);
                nx >= 0 && ny >= 0 && (nx as usize) < img.w && (ny as usize) < img.h && img.get(nx as usize, ny as usize)[3] != 0
            };
            if !solid(0, 0) && (solid(-1, 0) || solid(1, 0) || solid(0, -1) || solid(0, 1)) {
                out.set(x, y, BLACK);
            }
        }
    }
    out
}

/// The marker over the focused agent: a green diamond, lit from the left.
pub fn plumbob() -> Image {
    const ROWS: [&str; 7] = ["..#..", ".#L#.", ".LG#.", "#LGG#", ".LG#.", ".#G#.", "..#.."];
    let mut img = Image::new(5, ROWS.len());
    for (y, row) in ROWS.iter().enumerate() {
        for (x, c) in row.chars().enumerate() {
            let color = match c {
                '#' => BLACK,
                'L' => LIME,
                'G' => GREEN,
                _ => continue,
            };
            img.set(x as i32, y as i32, color);
        }
    }
    img
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn background_is_opaque_and_screen_sized() {
        let bg = background();
        assert_eq!((bg.w * SCALE, bg.h * SCALE), (320, 480));
        assert!(bg.px.iter().all(|p| p[3] == 0xff));
    }

    #[test]
    fn the_tallest_tower_and_its_roof_stay_below_the_title() {
        assert!(roof_pos(MAX_FLOORS).1 >= HEADER_H as i32, "{:?}", roof_pos(MAX_FLOORS));
    }

    #[test]
    fn storeys_stack_without_gaps() {
        for level in 1..=MAX_FLOORS {
            assert_eq!(floor_pos(level).1 + FLOOR_H as i32, storey_y(level - 1));
        }
    }

    #[test]
    fn desks_and_mascots_fit_inside_their_storey() {
        for level in 1..=MAX_FLOORS {
            let (fx, fy) = floor_pos(level);
            for i in 0..DESKS_PER_FLOOR {
                let s = slot(level, i);
                assert!(s.desk.0 >= fx + ROOMS_X && s.desk.0 + DESK_W <= fx + BUILDING_W as i32 - WALL, "{s:?}");
                assert_eq!(s.feet.1, fy + FLOOR_H as i32 - SLAB - AGENT_UP, "feet on the stool");
                // The mascot itself starts ORIGIN.1 rows down its frame.
                assert!(s.mascot.1 + ORIGIN.1 as i32 * 2 > fy, "{s:?} pokes into the storey above");
            }
        }
    }

    #[test]
    fn neighbouring_mascots_do_not_overlap() {
        let (a, b) = (slot(1, 0), slot(1, 1));
        assert!(b.mascot.0 - a.mascot.0 >= WIDTH as i32 + 2);
    }

    #[test]
    fn car_fits_the_shaft_and_storey() {
        let c = car();
        let (x, y) = car_pos(3);
        assert!(x >= BUILDING_X + SHAFT_X && x + c.w as i32 <= BUILDING_X + SHAFT_X + SHAFT_W);
        assert!(y >= storey_y(3) && y + c.h as i32 <= storey_y(3) + FLOOR_H as i32 - SLAB);
    }

    #[test]
    fn lights_change_the_storey() {
        assert_ne!(floor(1, true), floor(1, false));
        assert!(floor(2, false).px.iter().all(|p| p[3] == 0xff));
    }

    #[test]
    fn everything_uses_the_palette() {
        const PALETTE: [Rgba; 16] = [
            BLACK, NAVY, PLUM, GREEN, BROWN, GREY_DARK, GREY, WHITE, RED, ORANGE, YELLOW, LIME, BLUE, LAVENDER, PINK, PEACH,
        ];
        let [on, off] = roof();
        for img in [background(), floor(1, true), floor(3, false), on, off, car(), desk(), plumbob()] {
            assert!(img.px.iter().all(|p| p[3] == 0 || PALETTE.contains(p)));
        }
    }

    #[test]
    fn mascot_has_an_outline_and_black_eyes() {
        use herdr_panel_art::{Behavior, frame};
        let m = mascot(&frame(Behavior::Work, 0, 0), |_| BROWN);
        // Each eye is a gap in the face's second row: two tall, filled black.
        let eye_rows = (ORIGIN.1 as i32 + 1) * 2;
        let eyes = (0..m.w).filter(|&x| m.get(x, eye_rows as usize) == BLACK && m.get(x, eye_rows as usize - 2) == BROWN).count();
        assert_eq!(eyes, 2);
        let body_top = ORIGIN.1 * 2;
        assert_eq!(m.get(ORIGIN.0 + WIDTH / 2, body_top - 1), BLACK, "outline above the head");
    }

    #[test]
    fn roof_frames_differ_only_by_the_light() {
        let [on, off] = roof();
        let differ = on.px.iter().zip(&off.px).filter(|(a, b)| a != b).count();
        assert_eq!(differ, 6);
    }
}
