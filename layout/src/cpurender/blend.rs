//! CSS blend modes (`mix-blend-mode`, Compositing and Blending Level 1) on
//! premultiplied RGBA8 pixels: how an isolated group, a layer or a whole
//! pixmap is put onto its backdrop.
//!
//! Every mode composites source-over, with the mode's mixing function `B`
//! standing in for the source colour where the two overlap (s5.8, s10):
//!
//! ```text
//! co' = cs' (1 - ab) + cb' (1 - as) + as ab B(cb, cs)
//! ao  = as + ab - as ab
//! ```
//!
//! (`'` = premultiplied.) `Normal` is plain source-over ([`blend_over`]);
//! `Multiply` - what a printed map's overprint is - stays in integers, as
//! `as ab B = cs' cb'` needs no division; the other modes un-premultiply.

// The formulas are written as the specification writes them, not fused.
#![allow(clippy::suboptimal_flops)]

use azul_css::props::style::effects::StyleMixBlendMode;

use super::{blend_over, AzulPixmap};

/// `a * b / 255`, rounded.
#[inline]
const fn mul255(a: u32, b: u32) -> u32 {
    let t = a * b + 128;
    (t + (t >> 8)) >> 8
}

/// Puts the premultiplied pixel `src`, at opacity `op` (0-255), onto the
/// premultiplied pixel `dst` with the blend `mode`.
///
/// `Normal` is exactly [`blend_over`] - what an opacity group or a layer
/// does - so a group that names no blend mode paints the levels it painted
/// before. A transparent `src` leaves `dst` as it is in every mode, and over
/// a transparent `dst` every mode is source-over (`B` only acts where both
/// are present).
#[inline]
pub fn blend_pixel(dst: &mut [u8], src: [u8; 4], op: u32, mode: StyleMixBlendMode) {
    match mode {
        StyleMixBlendMode::Normal => blend_over(dst, src, op),
        StyleMixBlendMode::Multiply => multiply_pixel(dst, src, op.min(255)),
        _ => blend_pixel_float(dst, src, op.min(255), mode),
    }
}

/// [`blend_pixel`] for `Multiply`, in integers:
/// `co' = cs' (1 - ab) + cb' (1 - as) + cs' cb'`.
#[inline]
#[allow(clippy::cast_possible_truncation)] // every result is <= 255
fn multiply_pixel(dst: &mut [u8], src: [u8; 4], op: u32) {
    let [r, g, b, a] = src.map(|c| mul255(u32::from(c), op));
    if a == 0 {
        return;
    }
    let ab = u32::from(dst[3]);
    let channel = |cs: u32, cb: u8| {
        let cb = u32::from(cb);
        (mul255(cs, 255 - ab) + mul255(cb, 255 - a) + mul255(cs, cb)).min(255) as u8
    };
    dst[0] = channel(r, dst[0]);
    dst[1] = channel(g, dst[1]);
    dst[2] = channel(b, dst[2]);
    dst[3] = (a + ab - mul255(a, ab)).min(255) as u8;
}

/// [`blend_pixel`] for the modes that need the colours un-premultiplied.
#[allow(
    clippy::cast_possible_truncation,
    clippy::cast_precision_loss,
    clippy::cast_sign_loss,
    clippy::many_single_char_names,
    clippy::needless_range_loop
)] // channels are clamped to 0..=255 before the cast; op <= 255
fn blend_pixel_float(dst: &mut [u8], src: [u8; 4], op: u32, mode: StyleMixBlendMode) {
    let k = op as f32 / (255.0 * 255.0);
    let src_p = [
        f32::from(src[0]) * k,
        f32::from(src[1]) * k,
        f32::from(src[2]) * k,
        f32::from(src[3]) * k,
    ];
    let a_s = src_p[3];
    if a_s <= 0.0 {
        return;
    }
    let dst_p = [
        f32::from(dst[0]) / 255.0,
        f32::from(dst[1]) / 255.0,
        f32::from(dst[2]) / 255.0,
        f32::from(dst[3]) / 255.0,
    ];
    let a_b = dst_p[3];
    let unpremul = |c: f32, a: f32| {
        if a > 0.0 {
            (c / a).clamp(0.0, 1.0)
        } else {
            0.0
        }
    };
    let cs = [
        unpremul(src_p[0], a_s),
        unpremul(src_p[1], a_s),
        unpremul(src_p[2], a_s),
    ];
    let cb = [
        unpremul(dst_p[0], a_b),
        unpremul(dst_p[1], a_b),
        unpremul(dst_p[2], a_b),
    ];
    let mixed = mix(cb, cs, mode);
    let both = a_s * a_b;
    for i in 0..3 {
        let co = src_p[i] * (1.0 - a_b) + dst_p[i] * (1.0 - a_s) + both * mixed[i];
        dst[i] = (co * 255.0).round().clamp(0.0, 255.0) as u8;
    }
    let ao = a_s + a_b - both;
    dst[3] = (ao * 255.0).round().clamp(0.0, 255.0) as u8;
}

/// The mixing function `B(cb, cs)` of `mode`, on straight colours 0..1.
fn mix(cb: [f32; 3], cs: [f32; 3], mode: StyleMixBlendMode) -> [f32; 3] {
    use StyleMixBlendMode as M;
    let each = |f: fn(f32, f32) -> f32| [f(cb[0], cs[0]), f(cb[1], cs[1]), f(cb[2], cs[2])];
    match mode {
        M::Normal => cs,
        M::Multiply => each(|b, s| b * s),
        M::Screen => each(screen),
        M::Overlay => each(|b, s| hard_light(s, b)),
        M::Darken => each(f32::min),
        M::Lighten => each(f32::max),
        M::ColorDodge => each(|b, s| {
            if b <= 0.0 {
                0.0
            } else if s >= 1.0 {
                1.0
            } else {
                (b / (1.0 - s)).min(1.0)
            }
        }),
        M::ColorBurn => each(|b, s| {
            if b >= 1.0 {
                1.0
            } else if s <= 0.0 {
                0.0
            } else {
                1.0 - ((1.0 - b) / s).min(1.0)
            }
        }),
        M::HardLight => each(hard_light),
        M::SoftLight => each(|b, s| {
            if s <= 0.5 {
                b - (1.0 - 2.0 * s) * b * (1.0 - b)
            } else {
                let d = if b <= 0.25 {
                    ((16.0 * b - 12.0) * b + 4.0) * b
                } else {
                    b.sqrt()
                };
                b + (2.0 * s - 1.0) * (d - b)
            }
        }),
        M::Difference => each(|b, s| (b - s).abs()),
        M::Exclusion => each(|b, s| b + s - 2.0 * b * s),
        M::Hue => set_lum(set_sat(cs, sat(cb)), lum(cb)),
        M::Saturation => set_lum(set_sat(cb, sat(cs)), lum(cb)),
        M::Color => set_lum(cs, lum(cb)),
        M::Luminosity => set_lum(cb, lum(cs)),
    }
}

fn screen(b: f32, s: f32) -> f32 {
    b + s - b * s
}

fn hard_light(b: f32, s: f32) -> f32 {
    if s <= 0.5 {
        b * 2.0 * s
    } else {
        screen(b, 2.0 * s - 1.0)
    }
}

// The non-separable modes (s10.2): luminosity and saturation of a colour.

fn lum(c: [f32; 3]) -> f32 {
    0.3 * c[0] + 0.59 * c[1] + 0.11 * c[2]
}

fn clip_color(c: [f32; 3]) -> [f32; 3] {
    let l = lum(c);
    let n = c[0].min(c[1]).min(c[2]);
    let x = c[0].max(c[1]).max(c[2]);
    let mut out = c;
    if n < 0.0 && l - n > 0.0 {
        out = out.map(|v| l + (v - l) * l / (l - n));
    }
    if x > 1.0 && x - l > 0.0 {
        out = out.map(|v| l + (v - l) * (1.0 - l) / (x - l));
    }
    out
}

fn set_lum(c: [f32; 3], l: f32) -> [f32; 3] {
    let d = l - lum(c);
    clip_color(c.map(|v| v + d))
}

fn sat(c: [f32; 3]) -> f32 {
    c[0].max(c[1]).max(c[2]) - c[0].min(c[1]).min(c[2])
}

fn set_sat(c: [f32; 3], s: f32) -> [f32; 3] {
    let (lo, hi) = (c[0].min(c[1]).min(c[2]), c[0].max(c[1]).max(c[2]));
    if hi > lo {
        c.map(|v| (v - lo) * s / (hi - lo))
    } else {
        [0.0; 3]
    }
}

/// Puts the premultiplied RGBA8 buffer `src` onto `dst`, pixel for pixel, at
/// `opacity` (0..1) with the blend `mode`: a layer drawn on its own (an SVG
/// rasterised over transparent pixels, a tile step) composited onto what is
/// below it. Both buffers are the same size; only their common length is
/// blended.
#[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)] // opacity clamped to 0..=255
pub fn blend_premultiplied(dst: &mut [u8], src: &[u8], opacity: f32, mode: StyleMixBlendMode) {
    let op = (opacity * 255.0).round().clamp(0.0, 255.0) as u32;
    for (d, s) in dst.chunks_exact_mut(4).zip(src.chunks_exact(4)) {
        blend_pixel(d, [s[0], s[1], s[2], s[3]], op, mode);
    }
}

/// [`super::blit_pixmap_clipped`] with a blend mode: `src` placed at
/// (`px_x`, `px_y`) in `dst`, at `opacity`, inside the optional DEST-space
/// clip `(x0, y0, x1, y1)` (device px, half-open).
#[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)] // opacity clamped to 0..=255
pub fn blit_pixmap_blend(
    src: &AzulPixmap,
    dst: &mut AzulPixmap,
    px_x: i32,
    px_y: i32,
    opacity: f32,
    mode: StyleMixBlendMode,
    clip: Option<(i32, i32, i32, i32)>,
) {
    let op = (opacity * 255.0).round().clamp(0.0, 255.0) as u32;
    super::pixmap::for_each_clipped_pixel(src, dst, px_x, px_y, clip, |below, layer| {
        blend_pixel(below, layer, op, mode);
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn blended(dst: [u8; 4], src: [u8; 4], mode: StyleMixBlendMode) -> [u8; 4] {
        let mut d = dst;
        blend_pixel(&mut d, src, 255, mode);
        d
    }

    fn close(a: [u8; 4], b: [u8; 4]) -> bool {
        a.iter().zip(b).all(|(x, y)| x.abs_diff(y) <= 1)
    }

    /// Multiply over an opaque backdrop is the channels' product; white
    /// leaves the backdrop, black makes black.
    #[test]
    fn multiply_is_the_product_over_an_opaque_backdrop() {
        let m = StyleMixBlendMode::Multiply;
        assert!(close(
            blended([200, 100, 50, 255], [128, 255, 0, 255], m),
            [100, 100, 0, 255]
        ));
        assert_eq!(
            blended([200, 100, 50, 255], [255, 255, 255, 255], m),
            [200, 100, 50, 255]
        );
        assert_eq!(
            blended([200, 100, 50, 255], [0, 0, 0, 255], m),
            [0, 0, 0, 255]
        );
        // Overprinting two inks: cyan x yellow = green.
        assert_eq!(
            blended([0, 255, 255, 255], [255, 255, 0, 255], m),
            [0, 255, 0, 255]
        );
    }

    /// Where either side is transparent, every mode is source-over.
    #[test]
    fn every_mode_is_source_over_where_one_side_is_transparent() {
        use StyleMixBlendMode as M;
        for mode in [
            M::Multiply,
            M::Screen,
            M::Overlay,
            M::Darken,
            M::Lighten,
            M::ColorDodge,
            M::ColorBurn,
            M::HardLight,
            M::SoftLight,
            M::Difference,
            M::Exclusion,
            M::Hue,
            M::Saturation,
            M::Color,
            M::Luminosity,
        ] {
            assert_eq!(
                blended([0, 0, 0, 0], [40, 80, 120, 160], mode),
                [40, 80, 120, 160],
                "{mode:?} over nothing"
            );
            assert_eq!(
                blended([40, 80, 120, 160], [0, 0, 0, 0], mode),
                [40, 80, 120, 160],
                "{mode:?} of nothing"
            );
        }
    }

    /// The float path agrees with the integer one where both apply, and a
    /// translucent source multiplies only by its share.
    #[test]
    fn translucent_multiply_matches_the_formula() {
        // 50 % black (premultiplied 0,0,0,128) over opaque white: grey 127.
        let got = blended(
            [255, 255, 255, 255],
            [0, 0, 0, 128],
            StyleMixBlendMode::Multiply,
        );
        assert!(close(got, [127, 127, 127, 255]), "{got:?}");
        let mut f = [200, 100, 50, 255];
        blend_pixel_float(&mut f, [64, 128, 0, 128], 255, StyleMixBlendMode::Multiply);
        let i = blended(
            [200, 100, 50, 255],
            [64, 128, 0, 128],
            StyleMixBlendMode::Multiply,
        );
        assert!(close(f, i), "float {f:?} vs integer {i:?}");
    }

    /// The separable modes on opaque pixels, against their definitions.
    #[test]
    fn separable_modes_on_opaque_pixels() {
        use StyleMixBlendMode as M;
        let b = [51, 153, 204, 255]; // 0.2, 0.6, 0.8
        let s = [204, 102, 51, 255]; // 0.8, 0.4, 0.2
        assert!(close(blended(b, s, M::Screen), [214, 194, 214, 255]));
        assert!(close(blended(b, s, M::Darken), [51, 102, 51, 255]));
        assert!(close(blended(b, s, M::Lighten), [204, 153, 204, 255]));
        assert!(close(blended(b, s, M::Difference), [153, 51, 153, 255]));
        // exclusion: b + s - 2bs = 0.68, 0.52, 0.68
        assert!(close(blended(b, s, M::Exclusion), [173, 133, 173, 255]));
        // overlay = hard-light with the two swapped: 2bs where b <= .5,
        // screen(s, 2b - 1) above: 0.32, 0.52, 0.68
        assert!(close(blended(b, s, M::Overlay), [82, 133, 173, 255]));
    }

    /// The non-separable modes keep the luminosity they promise.
    #[test]
    fn luminosity_and_color_keep_the_promised_luminosity() {
        use StyleMixBlendMode as M;
        let to_f = |p: [u8; 4]| {
            [
                f32::from(p[0]) / 255.0,
                f32::from(p[1]) / 255.0,
                f32::from(p[2]) / 255.0,
            ]
        };
        let b = [51, 153, 204, 255];
        let s = [204, 102, 51, 255];
        let l = blended(b, s, M::Luminosity);
        assert!(
            (lum(to_f(l)) - lum(to_f(s))).abs() < 0.01,
            "luminosity takes the source's"
        );
        let c = blended(b, s, M::Color);
        assert!(
            (lum(to_f(c)) - lum(to_f(b))).abs() < 0.01,
            "color keeps the backdrop's"
        );
    }

    #[test]
    fn blit_pixmap_blend_multiplies_where_the_source_lands() {
        let mut dst = AzulPixmap::new(4, 4).unwrap();
        dst.fill(0, 255, 255, 255);
        let mut src = AzulPixmap::new(2, 2).unwrap();
        src.fill(255, 255, 0, 255);
        blit_pixmap_blend(&src, &mut dst, 1, 1, 1.0, StyleMixBlendMode::Multiply, None);
        let at = |x: u32, y: u32| {
            let i = ((y * 4 + x) * 4) as usize;
            [
                dst.data()[i],
                dst.data()[i + 1],
                dst.data()[i + 2],
                dst.data()[i + 3],
            ]
        };
        assert_eq!(at(1, 1), [0, 255, 0, 255]);
        assert_eq!(at(2, 2), [0, 255, 0, 255]);
        assert_eq!(at(0, 0), [0, 255, 255, 255]);
        assert_eq!(at(3, 3), [0, 255, 255, 255]);
    }

    #[test]
    fn blend_premultiplied_at_half_opacity() {
        let mut dst = vec![255u8; 8];
        let src = [0u8, 0, 0, 255, 0, 0, 0, 0];
        blend_premultiplied(&mut dst, &src, 0.5, StyleMixBlendMode::Multiply);
        assert!(
            close(dst[..4].try_into().unwrap(), [128, 128, 128, 255]),
            "{dst:?}"
        );
        assert_eq!(&dst[4..], &[255, 255, 255, 255]);
    }
}
