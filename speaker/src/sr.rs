//! What ESP-SR's prebuilt libraries need from ESP-IDF, in Rust: C's
//! allocator and ESP-IDF's `heap_caps` on esp-alloc, log output, and
//! esp-dl's dot products. The rest is in `csrc/sr_glue.c`.

use core::{
    alloc::{GlobalAlloc, Layout},
    ffi::{CStr, c_char, c_int, c_void},
    ptr,
};

use enumset::EnumSet;
use esp_alloc::{HEAP, MemoryCapability};

const MALLOC_CAP_SPIRAM: u32 = 1 << 10;
const MALLOC_CAP_INTERNAL: u32 = 1 << 11;
const MALLOC_CAP_DMA: u32 = 1 << 3;

/// Blocks at least this big go to PSRAM unless the caller asks for
/// internal memory (ESP-IDF's default is 16 KB; the internal heap here is
/// small).
const PSRAM_MIN: usize = 4096;

/// Bytes before each block holding its total size and alignment.
const HEADER: usize = 8;

/// Allocates `size` bytes aligned to `align` (a power of two) with room for
/// the header that `free` reads.
unsafe fn alloc(size: usize, align: usize, caps: u32) -> *mut c_void {
    let align = align.max(4);
    let pad = HEADER.next_multiple_of(align);
    let Some(total) = size.checked_add(pad) else { return ptr::null_mut() };
    let Ok(layout) = Layout::from_size_align(total, align) else { return ptr::null_mut() };
    let internal = caps & (MALLOC_CAP_INTERNAL | MALLOC_CAP_DMA) != 0;
    let order: &[EnumSet<MemoryCapability>] = if caps & MALLOC_CAP_SPIRAM != 0 {
        &[EnumSet::only(MemoryCapability::External)]
    } else if internal {
        &[EnumSet::only(MemoryCapability::Internal)]
    } else if size >= PSRAM_MIN {
        &[EnumSet::only(MemoryCapability::External), EnumSet::empty()]
    } else {
        &[EnumSet::only(MemoryCapability::Internal), EnumSet::empty()]
    };
    for &c in order {
        // SAFETY: `total` is at least HEADER, so the layout is not empty.
        let base = unsafe { HEAP.alloc_caps(c, layout) };
        if !base.is_null() {
            // SAFETY: the block holds `pad` >= HEADER bytes before `p`.
            unsafe {
                let p = base.add(pad);
                (p as *mut usize).sub(2).write(total);
                (p as *mut usize).sub(1).write(align);
                return p.cast();
            }
        }
    }
    ptr::null_mut()
}

/// The bytes usable at `p`, from its header.
unsafe fn usable(p: *mut c_void) -> usize {
    // SAFETY: the caller passes a block from `alloc`.
    unsafe {
        let total = (p as *const usize).sub(2).read();
        let align = (p as *const usize).sub(1).read();
        total - HEADER.next_multiple_of(align)
    }
}

#[unsafe(no_mangle)]
unsafe extern "C" fn free(p: *mut c_void) {
    if p.is_null() {
        return;
    }
    // SAFETY: `p` came from `alloc`, whose header gives the layout.
    unsafe {
        let total = (p as *const usize).sub(2).read();
        let align = (p as *const usize).sub(1).read();
        let base = (p as *mut u8).sub(HEADER.next_multiple_of(align));
        HEAP.dealloc(base, Layout::from_size_align_unchecked(total, align));
    }
}

#[unsafe(no_mangle)]
unsafe extern "C" fn malloc(size: usize) -> *mut c_void {
    unsafe { alloc(size, 4, 0) }
}

#[unsafe(no_mangle)]
unsafe extern "C" fn calloc(n: usize, size: usize) -> *mut c_void {
    unsafe { heap_caps_calloc(n, size, 0) }
}

#[unsafe(no_mangle)]
unsafe extern "C" fn realloc(old: *mut c_void, size: usize) -> *mut c_void {
    if old.is_null() {
        return unsafe { malloc(size) };
    }
    let new = unsafe { malloc(size) };
    if !new.is_null() {
        // SAFETY: both blocks hold at least the bytes copied.
        unsafe {
            ptr::copy_nonoverlapping(old as *const u8, new as *mut u8, usable(old).min(size));
            free(old);
        }
    }
    new
}

// newlib's reentrant variants, which its printf and number parsing call.

#[unsafe(no_mangle)]
unsafe extern "C" fn _malloc_r(_reent: *mut c_void, size: usize) -> *mut c_void {
    unsafe { malloc(size) }
}

#[unsafe(no_mangle)]
unsafe extern "C" fn _free_r(_reent: *mut c_void, p: *mut c_void) {
    unsafe { free(p) }
}

#[unsafe(no_mangle)]
unsafe extern "C" fn _calloc_r(_reent: *mut c_void, n: usize, size: usize) -> *mut c_void {
    unsafe { calloc(n, size) }
}

#[unsafe(no_mangle)]
unsafe extern "C" fn _realloc_r(_reent: *mut c_void, p: *mut c_void, size: usize) -> *mut c_void {
    unsafe { realloc(p, size) }
}

#[unsafe(no_mangle)]
unsafe extern "C" fn _malloc_usable_size_r(_reent: *mut c_void, p: *mut c_void) -> usize {
    if p.is_null() { 0 } else { unsafe { usable(p) } }
}

#[unsafe(no_mangle)]
unsafe extern "C" fn heap_caps_malloc(size: usize, caps: u32) -> *mut c_void {
    unsafe { alloc(size, 4, caps) }
}

#[unsafe(no_mangle)]
unsafe extern "C" fn heap_caps_calloc(n: usize, size: usize, caps: u32) -> *mut c_void {
    let Some(bytes) = n.checked_mul(size) else { return ptr::null_mut() };
    let p = unsafe { alloc(bytes, 4, caps) };
    if !p.is_null() {
        // SAFETY: the block holds `bytes` bytes.
        unsafe { ptr::write_bytes(p as *mut u8, 0, bytes) };
    }
    p
}

#[unsafe(no_mangle)]
unsafe extern "C" fn heap_caps_aligned_alloc(align: usize, size: usize, caps: u32) -> *mut c_void {
    if !align.is_power_of_two() {
        return ptr::null_mut();
    }
    unsafe { alloc(size, align, caps) }
}

#[unsafe(no_mangle)]
unsafe extern "C" fn heap_caps_aligned_calloc(align: usize, n: usize, size: usize, caps: u32) -> *mut c_void {
    let Some(bytes) = n.checked_mul(size) else { return ptr::null_mut() };
    let p = unsafe { heap_caps_aligned_alloc(align, bytes, caps) };
    if !p.is_null() {
        // SAFETY: the block holds `bytes` bytes.
        unsafe { ptr::write_bytes(p as *mut u8, 0, bytes) };
    }
    p
}

#[unsafe(no_mangle)]
unsafe extern "C" fn heap_caps_free(p: *mut c_void) {
    unsafe { free(p) }
}

/// Free bytes in the memory `caps` asks for. esp-alloc does not track the
/// largest free block, so this overstates it.
#[unsafe(no_mangle)]
extern "C" fn heap_caps_get_largest_free_block(caps: u32) -> usize {
    if caps & MALLOC_CAP_SPIRAM != 0 {
        HEAP.free_caps(EnumSet::only(MemoryCapability::External))
    } else if caps & (MALLOC_CAP_INTERNAL | MALLOC_CAP_DMA) != 0 {
        HEAP.free_caps(EnumSet::only(MemoryCapability::Internal))
    } else {
        HEAP.free()
    }
}

/// Prints text from the C side: a log line when `tag` is set, else raw
/// `printf` output.
#[unsafe(no_mangle)]
unsafe extern "C" fn speaker_sr_print(level: c_int, tag: *const c_char, text: *const u8, len: usize) {
    // SAFETY: the C side passes `len` readable bytes.
    let bytes = unsafe { core::slice::from_raw_parts(text, len) };
    let s = core::str::from_utf8(bytes).unwrap_or("<not UTF-8>");
    if tag.is_null() {
        esp_println::print!("{s}");
        return;
    }
    // SAFETY: tags are C string literals.
    let tag = unsafe { CStr::from_ptr(tag) }.to_str().unwrap_or("?");
    let level = match level {
        1 => log::Level::Error,
        2 => log::Level::Warn,
        3 => log::Level::Info,
        4 => log::Level::Debug,
        _ => log::Level::Trace,
    };
    log::log!(level, "{tag}: {}", s.trim_end());
}

#[unsafe(no_mangle)]
extern "C" fn esp_log_timestamp() -> u32 {
    embassy_time::Instant::now().as_millis() as u32
}

/// ESP_LOG_INFO, the level the libraries are allowed to log at.
#[unsafe(no_mangle)]
extern "C" fn esp_log_level_get(_tag: *const c_char) -> c_int {
    3
}

#[unsafe(no_mangle)]
extern "C" fn esp_log_level_set(_tag: *const c_char, _level: c_int) {}

/// `x` scaled by 2^-shift, rounded half up and saturated.
fn requantize(x: i64, shift: c_int) -> i16 {
    let v = if shift > 0 {
        (x + (1 << (shift - 1))) >> shift
    } else {
        x.saturating_mul(1 << -shift)
    };
    v.clamp(i16::MIN.into(), i16::MAX.into()) as i16
}

/// The dot product of `a` and `b` (`len` items each) into `out`, scaled by
/// 2^-shift: esp-dl's `dl::base::dotprod` overloads. ESP-SR's speech
/// features call the float one; the integer ones are only linked.
unsafe fn dot<A: Copy + Into<i64>, B: Copy + Into<i64>>(a: *const A, b: *const B, out: *mut i16, len: c_int, shift: c_int) {
    let len = len.max(0) as usize;
    // SAFETY: the caller passes `len` items in both inputs.
    let (a, b) = unsafe { (core::slice::from_raw_parts(a, len), core::slice::from_raw_parts(b, len)) };
    let sum: i64 = a.iter().zip(b).map(|(&x, &y)| x.into() * y.into()).sum();
    // SAFETY: the caller passes a valid output.
    unsafe { *out = requantize(sum, shift) };
}

#[unsafe(export_name = "_ZN2dl4base7dotprodEPaS1_Psii")]
unsafe extern "C" fn dotprod_i8_i8(a: *const i8, b: *const i8, out: *mut i16, len: c_int, shift: c_int) {
    unsafe { dot(a, b, out, len, shift) }
}

#[unsafe(export_name = "_ZN2dl4base7dotprodEPaPsS2_ii")]
unsafe extern "C" fn dotprod_i8_i16(a: *const i8, b: *const i16, out: *mut i16, len: c_int, shift: c_int) {
    unsafe { dot(a, b, out, len, shift) }
}

#[unsafe(export_name = "_ZN2dl4base7dotprodEPsS1_S1_ii")]
unsafe extern "C" fn dotprod_i16_i16(a: *const i16, b: *const i16, out: *mut i16, len: c_int, shift: c_int) {
    unsafe { dot(a, b, out, len, shift) }
}

#[unsafe(export_name = "_ZN2dl4base7dotprodEPfS1_S1_ii")]
unsafe extern "C" fn dotprod_f32(a: *const f32, b: *const f32, out: *mut f32, len: c_int, _shift: c_int) {
    let len = len.max(0) as usize;
    // SAFETY: the caller passes `len` items in both inputs and a valid output.
    unsafe {
        let (a, b) = (core::slice::from_raw_parts(a, len), core::slice::from_raw_parts(b, len));
        *out = a.iter().zip(b).map(|(x, y)| x * y).sum();
    }
}

/// ESP-SR's `esp_wn_iface_t`: one WakeNet version's functions.
#[repr(C)]
pub struct WnIface {
    pub create: Option<unsafe extern "C" fn(name: *const c_void, mode: c_int) -> *mut c_void>,
    pub get_start_point: Option<unsafe extern "C" fn(*mut c_void) -> c_int>,
    pub get_samp_chunksize: Option<unsafe extern "C" fn(*mut c_void) -> c_int>,
    pub get_channel_num: Option<unsafe extern "C" fn(*mut c_void) -> c_int>,
    pub get_samp_rate: Option<unsafe extern "C" fn(*mut c_void) -> c_int>,
    pub get_word_num: Option<unsafe extern "C" fn(*mut c_void) -> c_int>,
    pub get_word_name: Option<unsafe extern "C" fn(*mut c_void, c_int) -> *const c_char>,
    pub set_det_threshold: Option<unsafe extern "C" fn(*mut c_void, f32, c_int) -> c_int>,
    pub reset_det_threshold: Option<unsafe extern "C" fn(*mut c_void) -> c_int>,
    pub get_det_threshold: Option<unsafe extern "C" fn(*mut c_void, c_int) -> f32>,
    pub get_triggered_channel: Option<unsafe extern "C" fn(*mut c_void) -> c_int>,
    pub get_vol_gain: Option<unsafe extern "C" fn(*mut c_void, f32) -> f32>,
    pub detect: Option<unsafe extern "C" fn(*mut c_void, *mut i16) -> c_int>,
    pub detect_mfcc: Option<unsafe extern "C" fn(*mut c_void, *mut i16, *mut c_void) -> c_int>,
    pub get_mfcc_data: Option<unsafe extern "C" fn(*mut c_void) -> *mut c_void>,
    pub clean: Option<unsafe extern "C" fn(*mut c_void)>,
    pub destroy: Option<unsafe extern "C" fn(*mut c_void)>,
}

/// ESP-SR's `srmodel_list_t`, as compiled into its libraries.
#[repr(C)]
pub struct SrModelList {
    pub model_name: *mut *mut c_char,
    pub model_info: *mut *mut c_char,
    partition: *mut c_void,
    mmap_handle: *mut c_void,
    pub num: c_int,
    model_data: *mut *mut c_void,
}

/// Detection mode for WakeNet9 (DET_MODE_95): the model runs from PSRAM.
pub const DET_MODE_95: c_int = 1;

unsafe extern "C" {
    pub fn speaker_srmodel_load(image: *const u8) -> *mut SrModelList;
    pub fn esp_wn_handle_from_name(name: *const c_char) -> *const WnIface;
}

unsafe extern "C" fn no_model(_: *const c_void, _: c_int) -> *mut c_void {
    ptr::null_mut()
}

/// WakeNet10 stands in here so that ESP-SR's model lookup links without
/// esp-dl; creating a WakeNet10 model fails.
#[unsafe(no_mangle)]
static esp_sr_wakenet10_quantized: WnIface = WnIface {
    create: Some(no_model),
    get_start_point: None,
    get_samp_chunksize: None,
    get_channel_num: None,
    get_samp_rate: None,
    get_word_num: None,
    get_word_name: None,
    set_det_threshold: None,
    reset_det_threshold: None,
    get_det_threshold: None,
    get_triggered_channel: None,
    get_vol_gain: None,
    detect: None,
    detect_mfcc: None,
    get_mfcc_data: None,
    clean: None,
    destroy: None,
};
