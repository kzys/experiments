// Draws one browse::Frame: a title bar, a scrollable list with the
// selected row highlighted, and a footer legend. Sized for the RG35xx
// Plus's fixed 640x480 panel (confirmed via `swaymsg -t get_outputs` on the
// actual device) — no responsive layout needed for hardware that never
// changes resolution.

use crate::api::Item;
use crate::font::{Canvas, Rasterizer};

pub const WIDTH: i32 = 640;
pub const HEIGHT: i32 = 480;

const BG: u32 = 0xff101014;
const FG: u32 = 0xffe8e8e8;
const DIM: u32 = 0xff8a8a92;
const SEL_BG: u32 = 0xff2a5a9a;

const TITLE_H: i32 = 32;
const FOOTER_H: i32 = 22;
const ROW_H: i32 = 28;
const FONT_PX: f32 = 18.0;
const LEGEND_PX: f32 = 14.0;
const MARGIN: i32 = 12;

pub fn visible_rows() -> usize {
    ((HEIGHT - TITLE_H - FOOTER_H) / ROW_H) as usize
}

pub fn draw(canvas: &mut Canvas, font: &Rasterizer, title: &str, items: &[Item], selected: usize, legend: &str) {
    canvas.pixels.fill(BG);

    font.draw_str(canvas, MARGIN, TITLE_H - 8, FONT_PX, title, FG);

    let visible = visible_rows();
    // Keeps `selected` inside the window, biased to the window's bottom
    // edge once scrolled past the first screenful — a plain pager, not a
    // centering scroller; good enough for a list you walk one row at a time.
    let start = selected.saturating_sub(visible.saturating_sub(1));
    let end = (start + visible).min(items.len());

    for (row, item) in items[start..end].iter().enumerate() {
        let idx = start + row;
        let y0 = TITLE_H + (row as i32) * ROW_H;
        if idx == selected {
            fill_rect(canvas, 0, y0, WIDTH, ROW_H, SEL_BG);
        }
        font.draw_str(canvas, MARGIN, y0 + ROW_H - 8, FONT_PX, &item.name, FG);
    }

    if items.is_empty() {
        font.draw_str(canvas, MARGIN, TITLE_H + ROW_H - 8, FONT_PX, "(empty)", DIM);
    }

    font.draw_str(canvas, MARGIN, HEIGHT - 6, LEGEND_PX, legend, DIM);
}

fn fill_rect(canvas: &mut Canvas, x: i32, y: i32, w: i32, h: i32, color: u32) {
    for row in y.max(0)..(y + h).min(canvas.height) {
        for col in x.max(0)..(x + w).min(canvas.width) {
            canvas.pixels[(row * canvas.width + col) as usize] = color;
        }
    }
}
