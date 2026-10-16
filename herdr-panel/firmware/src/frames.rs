//! The images and layout, converted from the art and scene crates at build
//! time. Positions are in screen pixels.

use alloc::boxed::Box;

use herdr_panel_protocol::{Status, TimeOfDay};

/// One looping animation.
pub struct Loop {
    /// Pointers to `lv_image_dsc_t`, for `AnimImg::set_src`.
    pub frames: &'static [*const core::ffi::c_void],
    /// Time for all frames, in milliseconds.
    pub duration_ms: u32,
}

/// Where one agent works, relative to its storey.
pub struct Slot {
    /// The point between the agent's feet.
    pub feet: (i32, i32),
    /// Where the mascot image's top left corner goes.
    pub mascot: (i32, i32),
}

include!(concat!(env!("OUT_DIR"), "/frames.rs"));
