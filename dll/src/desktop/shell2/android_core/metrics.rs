//! The arithmetic between the display's density, the layout's logical pixels and the frame's device pixels.
//!
//! Android reports a density in dpi (`DisplayMetrics.densityDpi`: 160 is mdpi, 420 a Pixel 6). The engine
//! keeps `dpi = density * 96 / 160` in the window state and renders the frame at the factor `dpi / 96`
//! (`present_geometry::frame_dpi_factor`): a frame is `ceil(logical * factor)` device pixels long. The
//! shell used THREE different conversions for it:
//!
//! * the window's logical size was `physical / (density / 160)`, a plain `f32` division: the engine's own
//!   documentation of `logical_len_for_px` says why that is wrong ("the product can land a few ulps ABOVE
//!   `px`, and the ceiling then makes the frame one pixel larger than the window"): of 440 (density, width)
//!   pairs, 63 gave a frame one pixel too large (768 px at 420 dpi was a 769 px frame). A frame that is not
//!   the size of its window is resampled by the compositor over the whole surface: every frame soft, and no
//!   overlay;
//! * input positions were divided by `density / 160`, the layout used `dpi / 96` with `dpi` ROUNDED: for a
//!   density that is no multiple of 5 (213, 392, 411, 450) the two differ, and a touch lands a pixel or two
//!   off at the far edge.
//!
//! One factor now: [`frame_factor`], from the window's `dpi`, for the size, the input and the insets.

/// The densest display the shell takes at its word: 1280 dpi is 8x mdpi, twice the densest phone class
/// (xxxhdpi, 640). A larger report is a broken driver or emulator, and would make a window of a few
/// logical pixels; it is capped (the shell logs that it did) instead of computed with.
pub const MAX_DENSITY: u32 = 1280;

/// The engine's `dpi` for a density Android reported (`Configuration::density()`, an unsigned dpi):
/// `density * 96 / 160`, rounded, at least 1. A density of 0 (a device that reports none) is mdpi; one above
/// [`MAX_DENSITY`] is capped, so the arithmetic cannot overflow whatever the system says.
#[must_use]
pub fn dpi_of_density(density: u32) -> u32 {
    let density = if density == 0 { 160 } else { density.min(MAX_DENSITY) };
    // 96 / 160 = 3 / 5, in integers: no float, no ulp
    let dpi = (density * 3 + 2) / 5;
    dpi.max(1)
}

/// The factor of a window's frame: `dpi / 96` (1.0 for a `dpi` of 0, a window that was never told).
#[must_use]
pub fn frame_factor(dpi: u32) -> f32 {
    if dpi == 0 {
        1.0
    } else {
        dpi as f32 / 96.0
    }
}

/// The frame's length in device pixels for `logical` logical pixels at `factor`: the engine's formula
/// (`CpuBackend::render_frame`).
#[must_use]
pub fn frame_len_px(logical: f32, factor: f32) -> u32 {
    (logical * factor).ceil() as u32
}

/// The logical length whose frame is EXACTLY `px` device pixels at `factor`: the largest `f32` at or below
/// `px / factor` that renders to `px` (one ulp lower where the division overshoots). The same algorithm as
/// the engine's `present_geometry::logical_len_for_px`, which a test in the dll crate compares it with.
#[must_use]
pub fn logical_len_for_px(px: u32, factor: f32) -> f32 {
    if !(factor > 0.0) {
        return px as f32;
    }
    let mut len = px as f32 / factor;
    for _ in 0..4 {
        if !(len > 0.0) || frame_len_px(len, factor) <= px {
            break;
        }
        len = f32::from_bits(len.to_bits() - 1);
    }
    len
}

/// A window of `px_w` x `px_h` device pixels at `dpi`, as the logical size to store: every dimension
/// renders to exactly the pixels the window has.
#[must_use]
pub fn logical_size(px_w: i32, px_h: i32, dpi: u32) -> (f32, f32) {
    let factor = frame_factor(dpi);
    (
        logical_len_for_px(px_w.max(0) as u32, factor),
        logical_len_for_px(px_h.max(0) as u32, factor),
    )
}

/// A device pixel position or length as logical pixels (a touch, an inset, a gesture's centre).
#[must_use]
pub fn to_logical(px: f32, dpi: u32) -> f32 {
    px / frame_factor(dpi)
}

#[cfg(test)]
#[path = "metrics_test.rs"]
mod tests;

// Where the engine is built (the dll crate's own test run), the copies above are checked against it.
#[cfg(all(test, feature = "azul-layout"))]
#[path = "metrics_engine_test.rs"]
mod engine_tests;
