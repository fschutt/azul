//! Density, logical pixels and frame pixels. No device.

use super::*;

/// The densities Android devices report (the standard buckets and what manufacturers and the "display
/// size" setting make of them) and the widths and heights of their screens.
static DENSITIES: [u32; 22] = [
    120, 140, 160, 213, 240, 280, 320, 360, 392, 400, 411, 420, 440, 450, 480, 510, 540, 560, 600, 640, 720, 0,
];
static LENGTHS: [i32; 24] = [
    320, 360, 411, 480, 540, 600, 720, 768, 800, 828, 854, 1080, 1170, 1200, 1280, 1440, 1600, 1920, 2160, 2340, 2400,
    2560, 3120, 3200,
];

/// A window of any size at any density has a frame of exactly its size: no pixel more, which the
/// compositor would resample over the surface (soft text, no overlay).
///
/// Mutation: the plain division `px / factor` (63 of 440 pairs came out a pixel too large).
#[test]
fn the_frame_of_a_window_is_exactly_the_size_of_the_window() {
    for density in DENSITIES {
        let dpi = dpi_of_density(density);
        let factor = frame_factor(dpi);
        for px in LENGTHS {
            let logical = logical_len_for_px(px as u32, factor);
            assert_eq!(
                frame_len_px(logical, factor),
                px as u32,
                "density {density} (dpi {dpi}): a window {px} px long has a frame {} px long",
                frame_len_px(logical, factor)
            );
        }
    }
}

/// The same pairs with the shell's old division, to show what it cost: the test above is not vacuous.
#[test]
fn the_plain_division_gave_frames_a_pixel_too_large() {
    let mut too_large = 0;
    for density in DENSITIES {
        let dpi = dpi_of_density(density);
        let factor = frame_factor(dpi);
        for px in LENGTHS {
            let plain = px as f32 / factor;
            if frame_len_px(plain, factor) != px as u32 {
                too_large += 1;
            }
        }
    }
    assert!(too_large > 0, "the division the shell used is not exact: that is why it needs the inverse");
}

/// The engine's `dpi` for a density: `density * 96 / 160` rounded, in integers.
#[test]
fn the_dpi_of_a_density_is_the_engines_96_per_160() {
    assert_eq!(dpi_of_density(160), 96);
    assert_eq!(dpi_of_density(320), 192);
    assert_eq!(dpi_of_density(420), 252);
    assert_eq!(dpi_of_density(480), 288);
    assert_eq!(dpi_of_density(213), 128, "127.8 rounds up");
    assert_eq!(dpi_of_density(411), 247, "246.6");
    assert_eq!(dpi_of_density(0), 96, "a device that reports no density is mdpi");
    assert_eq!(dpi_of_density(1), 1, "never zero");
    for density in 1..=MAX_DENSITY {
        let float = ((density as f32) * 96.0 / 160.0).round() as u32;
        assert_eq!(dpi_of_density(density), float.max(1), "density {density}");
    }
}

/// Whatever the system reports, the arithmetic neither overflows nor makes a window of a few logical pixels.
///
/// Mutation: no cap (u32::MAX * 3 overflows: a panic in a debug build, a nonsense dpi in a release one).
#[test]
fn an_absurd_density_is_capped_not_computed_with() {
    assert_eq!(dpi_of_density(MAX_DENSITY), 768);
    assert_eq!(dpi_of_density(MAX_DENSITY + 1), 768);
    assert_eq!(dpi_of_density(100_000), 768);
    assert_eq!(dpi_of_density(u32::MAX), 768);
    assert!(MAX_DENSITY >= 2 * 640, "the densest phone class must pass uncapped");
}

/// For the standard densities the one factor IS `density / 160`, so nothing a phone with a standard
/// density did changes.
#[test]
fn for_a_standard_density_the_factor_is_density_over_160() {
    for density in [120, 160, 240, 320, 400, 420, 440, 480, 560, 640] {
        assert_eq!(frame_factor(dpi_of_density(density)), density as f32 / 160.0, "density {density}");
    }
}

#[test]
fn a_touch_and_an_inset_use_the_factor_of_the_frame() {
    // 252 dpi: 2.625 device pixels per logical one
    assert_eq!(to_logical(2.625 * 100.0, 252), 100.0);
    assert_eq!(to_logical(10.0, 0), 10.0, "a window that was never told is 1:1");
    let (w, h) = logical_size(1080, 2400, 252);
    assert_eq!(frame_len_px(w, frame_factor(252)), 1080);
    assert_eq!(frame_len_px(h, frame_factor(252)), 2400);
    assert_eq!(logical_size(-3, 0, 252), (0.0, 0.0), "no negative size");
}
