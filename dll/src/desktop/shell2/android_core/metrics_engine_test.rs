//! The shell's frame arithmetic is the engine's: `android_core::metrics` copies three small functions so
//! that it can be tested with nothing built; here, where the engine is built, they are compared with it.

use azul_layout::window::present_geometry;

use super::*;

/// The inverse, the factor and the frame length are the engine's for every density the shell may
/// meet and every length up to 4200 px: if the engine changes its rounding, this fails.
#[test]
fn the_shells_copies_are_the_engines_functions() {
    for density in [120, 160, 213, 240, 280, 320, 392, 400, 411, 420, 440, 450, 480, 510, 560, 640] {
        let dpi = dpi_of_density(density);
        let factor = frame_factor(dpi);
        assert_eq!(factor, present_geometry::frame_dpi_factor(dpi), "density {density}");
        for px in 1..4200u32 {
            let ours = logical_len_for_px(px, factor);
            assert_eq!(ours, present_geometry::logical_len_for_px(px, factor), "density {density}, {px} px");
            assert_eq!(
                frame_len_px(ours, factor),
                present_geometry::frame_size_px(ours, 1.0, factor).0,
                "density {density}, {px} px"
            );
        }
    }
}
