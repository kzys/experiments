//! The mascot and its animations, as pixel rectangles.
//!
//! The art comes from three rows of block characters. Each character is a 2x2
//! block of pixels, and the quadrant characters say which of the four are on.
//! The two gaps in the top row are the eyes. Frames are drawn on a small grid
//! (the mascot plus room above it for zzz, "!" and the like); the display draws
//! each grid pixel as a rectangle of its own choosing.
#![no_std]

use heapless::Vec;

/// The mascot's own size in pixels.
pub const WIDTH: usize = 18;
pub const HEIGHT: usize = 6;

/// Size of a frame in pixels: the mascot at [`ORIGIN`], with room around it.
pub const CANVAS_W: usize = 24;
pub const CANVAS_H: usize = 10;
/// Where the mascot's top left corner is in a frame at rest.
pub const ORIGIN: (usize, usize) = (3, 4);

const ART: [&str; 3] = [" ▐▛███▜▌", "▝▜█████▛▘", "  ▘▘ ▝▝"];

/// What a pixel is made of; the display picks the colours.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Ink {
    Body,
    Zzz,
    Alert,
    Spark,
    Question,
    /// A drowsy eye: the upper half of the pixel is body, the lower half open.
    Lid,
}

pub type Grid = [[Option<Ink>; CANVAS_W]; CANVAS_H];

/// What the mascot is doing.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Behavior {
    /// Busy: running on the spot, arms swinging.
    Work,
    /// Asleep, eyes drowsy, with zzz rising.
    Sleep,
    /// Idle and awake: looks around, blinks, hops and waves at random.
    Play,
    /// Waiting for an answer: "!" and a waving arm.
    Blocked,
    /// Finished: hopping, arms up, sparkles.
    Done,
    /// Nothing known: a question mark.
    Unknown,
}

/// A rectangle of pixels of one ink.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Rect {
    pub x: usize,
    pub y: usize,
    pub w: usize,
    pub h: usize,
}

/// Which quadrants of a character cell are on: upper left 1, upper right 2,
/// lower left 4, lower right 8.
fn quadrants(c: char) -> u8 {
    match c {
        '▘' => 1,
        '▝' => 2,
        '▌' => 1 | 4,
        '▐' => 2 | 8,
        '▛' => 1 | 2 | 4,
        '▜' => 1 | 2 | 8,
        '█' => 15,
        _ => 0,
    }
}

type Sprite = [[bool; WIDTH]; HEIGHT];

/// The mascot at rest, `true` where it is solid.
pub fn pixels() -> [[bool; WIDTH]; HEIGHT] {
    let mut grid = [[false; WIDTH]; HEIGHT];
    for (row, text) in ART.iter().enumerate() {
        for (col, c) in text.chars().enumerate() {
            let q = quadrants(c);
            for (bit, (dx, dy)) in [(0, 0), (1, 0), (0, 1), (1, 1)].into_iter().enumerate() {
                if q & (1 << bit) != 0 {
                    grid[row * 2 + dy][col * 2 + dx] = true;
                }
            }
        }
    }
    grid
}

// Parts of the mascot, in its own coordinates.
const ARM_ROW: usize = 2;
const LEFT_ARM: [usize; 2] = [1, 2];
const RIGHT_ARM: [usize; 2] = [15, 16];
const LEG_ROW: usize = 4;
const LEGS_STANDING: [usize; 4] = [4, 6, 11, 13];
const LEGS_STRIDE: [usize; 4] = [5, 7, 10, 12];
const EYE_ROW: usize = 1;
const EYES_FORWARD: [usize; 2] = [5, 12];
const EYES_LEFT: [usize; 2] = [4, 11];
const EYES_RIGHT: [usize; 2] = [6, 13];

#[derive(Clone, Copy, PartialEq, Eq)]
enum Eyes {
    Forward,
    Left,
    Right,
    Closed,
    /// Forward, but half shut.
    Drowsy,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Arm {
    Down,
    Up,
}

/// How to pose the mascot.
#[derive(Clone, Copy)]
struct Pose {
    eyes: Eyes,
    left: Arm,
    right: Arm,
    /// Legs mid-stride instead of standing.
    stride: bool,
    /// Rows the whole mascot is lifted.
    lift: usize,
}

impl Pose {
    const REST: Pose = Pose { eyes: Eyes::Forward, left: Arm::Down, right: Arm::Down, stride: false, lift: 0 };

    /// The mascot's pixels in this pose.
    fn sprite(self) -> Sprite {
        let mut s = pixels();
        for x in LEFT_ARM.into_iter().chain(RIGHT_ARM) {
            s[ARM_ROW][x] = false;
        }
        for x in LEGS_STANDING {
            s[LEG_ROW][x] = false;
        }
        for (arm, xs) in [(self.left, LEFT_ARM), (self.right, RIGHT_ARM)] {
            let row = if arm == Arm::Up { ARM_ROW - 1 } else { ARM_ROW };
            for x in xs {
                s[row][x] = true;
            }
        }
        for x in if self.stride { LEGS_STRIDE } else { LEGS_STANDING } {
            s[LEG_ROW][x] = true;
        }
        // The eyes are gaps in the face: fill every position, then open two.
        for x in EYES_FORWARD.into_iter().chain(EYES_LEFT).chain(EYES_RIGHT) {
            s[EYE_ROW][x] = true;
        }
        match self.eyes {
            Eyes::Forward => EYES_FORWARD,
            Eyes::Left => EYES_LEFT,
            Eyes::Right => EYES_RIGHT,
            Eyes::Closed | Eyes::Drowsy => [usize::MAX; 2],
        }
        .into_iter()
        .filter(|&x| x != usize::MAX)
        .for_each(|x| s[EYE_ROW][x] = false);
        s
    }
}

/// Puts the mascot, lifted by `lift` rows, on the grid. Drowsy eyes are
/// [`Ink::Lid`] pixels.
fn draw_mascot(grid: &mut Grid, pose: Pose) {
    let sprite = pose.sprite();
    for (y, row) in sprite.iter().enumerate() {
        for (x, &on) in row.iter().enumerate() {
            if on {
                grid[ORIGIN.1 + y - pose.lift][ORIGIN.0 + x] = Some(Ink::Body);
            }
        }
    }
    if pose.eyes == Eyes::Drowsy {
        for x in EYES_FORWARD {
            grid[ORIGIN.1 + EYE_ROW - pose.lift][ORIGIN.0 + x] = Some(Ink::Lid);
        }
    }
}

fn put(grid: &mut Grid, x: usize, y: usize, ink: Ink) {
    if x < CANVAS_W && y < CANVAS_H {
        grid[y][x] = Some(ink);
    }
}

/// Draws `rows` (`#` is on) with its top left corner at (`x`, `y`).
fn glyph(grid: &mut Grid, x: usize, y: usize, ink: Ink, rows: &[&str]) {
    for (dy, row) in rows.iter().enumerate() {
        for (dx, c) in row.chars().enumerate() {
            if c == '#' {
                put(grid, x + dx, y + dy, ink);
            }
        }
    }
}

const Z: [&str; 3] = ["###", ".#.", "###"];
const BANG: [&str; 4] = ["#", "#", ".", "#"];
const QUESTION: [&str; 5] = ["###", "..#", ".##", "...", ".#."];

fn hash(a: u32, b: u32) -> u32 {
    let mut x = a.wrapping_mul(0x9E37_79B1) ^ b.wrapping_mul(0x85EB_CA77);
    x ^= x >> 15;
    x = x.wrapping_mul(0x2C1B_3C6D);
    x ^ (x >> 12)
}

/// Ticks one idle mood lasts, about nine seconds at seven ticks a second.
const MOOD_TICKS: u32 = 64;
/// Ticks one play action lasts.
const ACTION_TICKS: u32 = 6;

/// Whether an idle agent is asleep or playing at `tick`. Moods last several
/// seconds and differ from agent to agent (`seed`); sleeping is more likely.
pub fn idle(seed: u32, tick: u32) -> Behavior {
    if hash(seed, tick / MOOD_TICKS) % 3 == 0 { Behavior::Play } else { Behavior::Sleep }
}

/// How a behavior loops: `frames` frames, each shown for `frame_ms`, where
/// frame `i` is [`frame`] at tick `i * tick_step`. After the last frame the
/// loop closes: tick `frames * tick_step` looks like tick 0.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Cycle {
    pub frames: u32,
    pub tick_step: u32,
    pub frame_ms: u32,
}

/// The looping animation for `behavior`. Playing at random is not looped: it
/// is a single frame until that is built.
pub const fn cycle(behavior: Behavior) -> Cycle {
    match behavior {
        Behavior::Work => Cycle { frames: 2, tick_step: 1, frame_ms: 150 },
        Behavior::Sleep => Cycle { frames: 4, tick_step: 4, frame_ms: 500 },
        Behavior::Blocked => Cycle { frames: 4, tick_step: 1, frame_ms: 250 },
        Behavior::Done => Cycle { frames: 6, tick_step: 1, frame_ms: 150 },
        Behavior::Play | Behavior::Unknown => Cycle { frames: 1, tick_step: 1, frame_ms: 1000 },
    }
}

/// The frame for `behavior` at `tick`. `seed` varies the random parts per agent.
pub fn frame(behavior: Behavior, seed: u32, tick: u32) -> Grid {
    let mut grid = [[None; CANVAS_W]; CANVAS_H];
    let odd = tick % 2 == 1;
    match behavior {
        Behavior::Work => {
            let pose = Pose { stride: odd, left: if odd { Arm::Up } else { Arm::Down }, right: if odd { Arm::Down } else { Arm::Up }, ..Pose::REST };
            draw_mascot(&mut grid, pose);
        }
        Behavior::Sleep => {
            draw_mascot(&mut grid, Pose { eyes: Eyes::Drowsy, ..Pose::REST });
            // zzz rises in four steps: one z, then two, then both stay, then none.
            let step = (tick / 4) % 4;
            if step >= 1 {
                glyph(&mut grid, 18, 1, Ink::Zzz, &Z);
            }
            if step >= 2 {
                glyph(&mut grid, 21, 0, Ink::Zzz, &Z);
            }
        }
        Behavior::Play => {
            let action = hash(seed, tick / ACTION_TICKS) % 5;
            let step = tick % ACTION_TICKS;
            let pose = match action {
                0 => Pose { eyes: if step < 3 { Eyes::Left } else { Eyes::Right }, ..Pose::REST },
                1 => Pose { eyes: if step == 2 || step == 3 { Eyes::Closed } else { Eyes::Forward }, ..Pose::REST },
                2 => Pose { lift: usize::from(step % 2 == 1), left: Arm::Up, right: Arm::Up, ..Pose::REST },
                3 => Pose { right: if step % 2 == 0 { Arm::Up } else { Arm::Down }, ..Pose::REST },
                _ => Pose { eyes: Eyes::Right, lift: usize::from(step == 1 || step == 4), ..Pose::REST },
            };
            draw_mascot(&mut grid, pose);
        }
        Behavior::Blocked => {
            draw_mascot(&mut grid, Pose { right: if odd { Arm::Up } else { Arm::Down }, ..Pose::REST });
            if (tick / 2) % 2 == 0 {
                glyph(&mut grid, 21, 0, Ink::Alert, &BANG);
            }
        }
        Behavior::Done => {
            draw_mascot(&mut grid, Pose { lift: usize::from(odd), left: Arm::Up, right: Arm::Up, ..Pose::REST });
            let corners = [(1, 1), (22, 2), (2, 8), (21, 7)];
            for (i, (x, y)) in corners.into_iter().enumerate() {
                if (tick + i as u32) % 3 != 0 {
                    put(&mut grid, x, y, Ink::Spark);
                }
            }
        }
        Behavior::Unknown => {
            draw_mascot(&mut grid, Pose::REST);
            glyph(&mut grid, 20, 0, Ink::Question, &QUESTION);
        }
    }
    grid
}

/// Few enough rectangles to draw `grid`: horizontal runs of one ink, with
/// identical runs on consecutive rows merged into taller rectangles.
pub fn rects(grid: &Grid) -> Vec<(Rect, Ink), 80> {
    let mut out: Vec<(Rect, Ink), 80> = Vec::new();
    for (y, row) in grid.iter().enumerate() {
        let mut x = 0;
        while x < CANVAS_W {
            let Some(ink) = row[x] else {
                x += 1;
                continue;
            };
            let start = x;
            while x < CANVAS_W && row[x] == Some(ink) {
                x += 1;
            }
            let run = Rect { x: start, y, w: x - start, h: 1 };
            match out.iter_mut().find(|(r, i)| *i == ink && r.x == run.x && r.w == run.w && r.y + r.h == y) {
                Some((above, _)) => above.h += 1,
                None => out.push((run, ink)).expect("a frame needs fewer than 80 rectangles"),
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    extern crate std;

    use std::{string::String, vec::Vec};

    use super::*;

    const ALL: [Behavior; 6] =
        [Behavior::Work, Behavior::Sleep, Behavior::Play, Behavior::Blocked, Behavior::Done, Behavior::Unknown];

    fn picture(grid: &Grid) -> Vec<String> {
        grid.iter()
            .map(|row| {
                row.iter()
                    .map(|p| match p {
                        None => '.',
                        Some(Ink::Body) => '#',
                        Some(Ink::Zzz) => 'z',
                        Some(Ink::Alert) => '!',
                        Some(Ink::Spark) => '*',
                        Some(Ink::Question) => '?',
                        Some(Ink::Lid) => '-',
                    })
                    .collect()
            })
            .collect()
    }

    #[test]
    fn mascot_matches_the_block_art() {
        let rows: Vec<String> =
            pixels().iter().map(|row| row.iter().map(|&on| if on { '#' } else { '.' }).collect()).collect();
        assert_eq!(
            rows,
            [
                "...############...",
                "...##.######.##...",
                ".################.",
                "...############...",
                "....#.#....#.#....",
                "..................",
            ]
        );
    }

    #[test]
    fn resting_mascot_sits_at_the_origin() {
        let mut grid = [[None; CANVAS_W]; CANVAS_H];
        draw_mascot(&mut grid, Pose::REST);
        let expected = pixels();
        for (y, row) in expected.iter().enumerate() {
            for (x, &on) in row.iter().enumerate() {
                assert_eq!(grid[ORIGIN.1 + y][ORIGIN.0 + x].is_some(), on, "({x}, {y})");
            }
        }
    }

    #[test]
    fn sleeping_shows_drowsy_eyes_and_zzz() {
        let frames: Vec<Vec<String>> = (0..16).map(|t| picture(&frame(Behavior::Sleep, 0, t))).collect();
        assert!(frames.iter().any(|f| f.iter().any(|r| r.contains('z'))), "no zzz");
        assert!(frames[0].iter().all(|r| !r.contains('z')), "zzz should start hidden");
        // Drowsy: both eyes half shut, in every frame.
        for f in &frames {
            assert_eq!(&f[ORIGIN.1 + 1][ORIGIN.0 + 3..ORIGIN.0 + 15], "##-######-##");
        }
    }

    #[test]
    fn working_alternates_between_two_poses() {
        let a = frame(Behavior::Work, 0, 0);
        let b = frame(Behavior::Work, 0, 1);
        assert_ne!(a, b);
        assert_eq!(a, frame(Behavior::Work, 0, 2));
    }

    #[test]
    fn blocked_blinks_an_exclamation_mark() {
        let shown = (0..8).filter(|&t| picture(&frame(Behavior::Blocked, 0, t)).iter().any(|r| r.contains('!'))).count();
        assert!(shown > 0 && shown < 8, "{shown} of 8 frames");
    }

    #[test]
    fn unknown_shows_a_question_mark() {
        assert!(picture(&frame(Behavior::Unknown, 0, 0)).iter().any(|r| r.contains('?')));
    }

    #[test]
    fn done_hops_and_sparkles() {
        let a = frame(Behavior::Done, 0, 0);
        let b = frame(Behavior::Done, 0, 1);
        assert_ne!(a, b);
        assert!(picture(&a).iter().any(|r| r.contains('*')));
    }

    #[test]
    fn idle_agents_sleep_and_play_at_different_times() {
        let moods: Vec<Behavior> = (0..32).map(|seed| idle(seed, 0)).collect();
        assert!(moods.contains(&Behavior::Sleep) && moods.contains(&Behavior::Play));
        // The same agent keeps its mood for a while.
        assert_eq!(idle(7, 0), idle(7, MOOD_TICKS - 1));
    }

    #[test]
    fn playing_does_something_different_over_time() {
        let frames: Vec<Grid> = (0..120).map(|t| frame(Behavior::Play, 3, t)).collect();
        let distinct = frames.iter().enumerate().filter(|(i, f)| frames[..*i].iter().all(|g| g != *f)).count();
        assert!(distinct >= 5, "{distinct} distinct frames");
    }

    #[test]
    fn rectangles_cover_every_pixel_exactly_once_and_stay_few() {
        for behavior in ALL {
            for tick in 0..200 {
                let grid = frame(behavior, 5, tick);
                let mut covered = [[0u8; CANVAS_W]; CANVAS_H];
                let rects = rects(&grid);
                for (r, _) in &rects {
                    for y in r.y..r.y + r.h {
                        for x in r.x..r.x + r.w {
                            covered[y][x] += 1;
                        }
                    }
                }
                for y in 0..CANVAS_H {
                    for x in 0..CANVAS_W {
                        assert_eq!(covered[y][x], u8::from(grid[y][x].is_some()), "{behavior:?} tick {tick} ({x}, {y})");
                    }
                }
                assert!(rects.len() <= 40, "{behavior:?} tick {tick}: {} rectangles", rects.len());
            }
        }
    }

    #[test]
    fn every_loop_closes() {
        for behavior in ALL {
            let c = cycle(behavior);
            assert_eq!(frame(behavior, 0, 0), frame(behavior, 0, c.frames * c.tick_step), "{behavior:?}");
        }
    }

    #[test]
    fn loops_with_several_frames_show_different_frames() {
        for behavior in [Behavior::Work, Behavior::Sleep, Behavior::Blocked, Behavior::Done] {
            let c = cycle(behavior);
            let frames: Vec<Grid> = (0..c.frames).map(|i| frame(behavior, 0, i * c.tick_step)).collect();
            let distinct = frames.iter().enumerate().filter(|(i, f)| frames[..*i].iter().all(|g| g != *f)).count();
            assert!(distinct >= 2, "{behavior:?}: {distinct} distinct frames");
        }
    }

    #[test]
    fn nothing_is_drawn_outside_the_frame_edges_by_lifting() {
        // A hop lifts the mascot one row; it must not wrap or panic.
        for tick in 0..20 {
            let _ = frame(Behavior::Done, 0, tick);
            let _ = frame(Behavior::Play, 1, tick);
        }
    }
}
