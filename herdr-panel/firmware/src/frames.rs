//! The mascot's animation frames, converted from the art crate at build time.

use alloc::boxed::Box;

use herdr_panel_art::Behavior;

/// One looping animation.
pub struct Loop {
    /// Pointers to `lv_image_dsc_t`, for `AnimImg::set_src`.
    pub frames: &'static [*const core::ffi::c_void],
    /// Time for all frames, in milliseconds.
    pub duration_ms: u32,
}

include!(concat!(env!("OUT_DIR"), "/frames.rs"));
