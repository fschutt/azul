//! The built-in materials are lit per DEVICE pixel, the way the AzDrive
//! design's material shader lights them: the grain is a function of the
//! position, evaluated at every device pixel, so a 2x screen shows detail no
//! 1x render stretched to 2x has; the colour is only the tint; slopes are
//! dithered instead of banded; a cushion bevel rolls into the flat face
//! without drawing a line; and every surface agrees where the light is.

use azul_core::{
    light::Light,
    resources::{RawImage, RawImageData},
};
use azul_layout::texture::{
    materials::{
        render_material_pixels, Bevel, MaterialParams, MaterialSource, Pillow, SurfaceScene,
    },
    Material,
};

/// The straight RGBA of pixel (`x`, `y`) of a premultiplied frame.
fn pixel(image: &RawImage, x: usize, y: usize) -> [u8; 4] {
    let RawImageData::U8(bytes) = &image.pixels else {
        panic!("an 8-bit frame");
    };
    let i = (y * image.width + x) * 4;
    let b = bytes.as_ref();
    [b[i], b[i + 1], b[i + 2], b[i + 3]]
}

fn luma(p: [u8; 4]) -> f32 {
    0.2126 * f32::from(p[0]) + 0.7152 * f32::from(p[1]) + 0.0722 * f32::from(p[2])
}

fn lumas(image: &RawImage) -> Vec<f32> {
    let mut out = Vec::with_capacity(image.width * image.height);
    for y in 0..image.height {
        for x in 0..image.width {
            out.push(luma(pixel(image, x, y)));
        }
    }
    out
}

/// The mean spread (max - min of the luma) inside the image's aligned 2 x 2
/// blocks: how much detail lives below a logical pixel at 2x.
fn detail_below_a_logical_pixel(luma: &[f32], width: usize, height: usize) -> f32 {
    let mut sum = 0.0;
    let mut blocks = 0.0;
    for by in (0..height - 1).step_by(2) {
        for bx in (0..width - 1).step_by(2) {
            let block = [
                luma[by * width + bx],
                luma[by * width + bx + 1],
                luma[(by + 1) * width + bx],
                luma[(by + 1) * width + bx + 1],
            ];
            let hi = block.iter().copied().fold(f32::MIN, f32::max);
            let lo = block.iter().copied().fold(f32::MAX, f32::min);
            sum += hi - lo;
            blocks += 1.0;
        }
    }
    sum / blocks
}

/// `luma` (`width` x `height`) stretched to twice its size with bilinear
/// filtering: what a texture rendered at 1x looks like on a 2x screen.
fn stretched_twice(luma: &[f32], width: usize, height: usize) -> Vec<f32> {
    let (w2, h2) = (width * 2, height * 2);
    let at = |x: i64, y: i64| {
        let x = x.clamp(0, width as i64 - 1) as usize;
        let y = y.clamp(0, height as i64 - 1) as usize;
        luma[y * width + x]
    };
    let mut out = Vec::with_capacity(w2 * h2);
    for y in 0..h2 {
        for x in 0..w2 {
            let u = (x as f32 + 0.5) / 2.0 - 0.5;
            let v = (y as f32 + 0.5) / 2.0 - 0.5;
            let (fx, fy) = (u.floor(), v.floor());
            let (tx, ty) = (u - fx, v - fy);
            let (x0, y0) = (fx as i64, fy as i64);
            let top = at(x0, y0) * (1.0 - tx) + at(x0 + 1, y0) * tx;
            let bottom = at(x0, y0 + 1) * (1.0 - tx) + at(x0 + 1, y0 + 1) * tx;
            out.push(top * (1.0 - ty) + bottom * ty);
        }
    }
    out
}

/// A material with nothing on it but its colour and the shape's slope: a
/// smooth surface (no grain, no occlusion), no sheen.
fn flat(material: Material) -> MaterialParams {
    let mut p = MaterialParams::of(material);
    p.source = MaterialSource::Smooth;
    p.sheen = 0.0;
    p.bevel = Bevel::NONE;
    p.pillow = Pillow::NONE;
    p
}

#[test]
fn a_material_at_2x_shows_detail_finer_than_a_logical_pixel() {
    let params = MaterialParams::of(Material::Leather);
    let scene = SurfaceScene::default();
    let (w, h) = (120_usize, 60_usize);
    let at_1x = render_material_pixels(Material::Leather, w as u32, h as u32, 1.0, &params, &scene)
        .expect("a 1x render");
    let at_2x =
        render_material_pixels(Material::Leather, 2 * w as u32, 2 * h as u32, 2.0, &params, &scene)
            .expect("a 2x render");
    assert_eq!((at_2x.width, at_2x.height), (2 * w, 2 * h), "the frame is the device size");

    let sharp = detail_below_a_logical_pixel(&lumas(&at_2x), 2 * w, 2 * h);
    let stretched = detail_below_a_logical_pixel(&stretched_twice(&lumas(&at_1x), w, h), 2 * w, 2 * h);
    assert!(
        sharp >= 1.0 && sharp >= 1.5 * stretched,
        "the 2x render carries its own fine grain ({sharp:.2} levels inside a logical pixel), \
         not the 1x render stretched ({stretched:.2})"
    );
}

#[test]
fn a_material_s_colour_is_its_tint() {
    let scene = SurfaceScene::default();
    let mean = |tint: [u8; 3]| -> [f32; 3] {
        let mut params = MaterialParams::of(Material::Leather);
        params.set_tint_srgb(tint);
        let image = render_material_pixels(Material::Leather, 80, 40, 2.0, &params, &scene)
            .expect("a render");
        let mut sum = [0.0_f32; 3];
        for y in 0..image.height {
            for x in 0..image.width {
                let p = pixel(&image, x, y);
                for c in 0..3 {
                    sum[c] += f32::from(p[c]);
                }
            }
        }
        sum.map(|s| s / (image.width * image.height) as f32)
    };
    let denim_blue = mean([0x2D, 0x40, 0x61]);
    assert!(
        denim_blue[2] > denim_blue[0] + 20.0,
        "leather tinted denim blue is blue: {denim_blue:?}"
    );
    let nappa = mean([0xE4, 0xE2, 0xDD]);
    assert!(
        (nappa[0] - nappa[2]).abs() < 24.0 && nappa[1] > 150.0,
        "pale nappa is a pale, nearly neutral grey: {nappa:?}"
    );
}

#[test]
fn a_slow_slope_is_dithered_instead_of_banded() {
    // A shallow pillow on a smooth material: from the top edge to the crown
    // the light changes slowly, a few levels over hundreds of device pixels.
    let mut params = flat(Material::Leather);
    params.pillow = Pillow {
        width: 76.0,
        depth: 0.08,
        foot: 1.0,
    };
    let image = render_material_pixels(Material::Leather, 600, 800, 2.0, &params, &SurfaceScene::default())
        .expect("a render");
    let column: Vec<u8> = (8..400).map(|y| pixel(&image, 300, y)[1]).collect();
    let mut levels: Vec<u8> = column.clone();
    levels.sort_unstable();
    levels.dedup();
    let transitions = column.windows(2).filter(|w| w[0] != w[1]).count();
    assert!(levels.len() >= 3, "the slope spans several levels: {levels:?}");
    assert!(
        transitions >= 3 * (levels.len() - 1),
        "{transitions} changes across {} levels: each level boundary is crossed back and forth \
         (dithered), not once (a band's edge)",
        levels.len()
    );
}

/// The luma along the middle row of a flat 100 x 60 box at 2x with `bevel`,
/// from its left edge inward.
fn bevel_profile(bevel: Bevel) -> (Vec<f32>, f32) {
    let mut params = flat(Material::Leather);
    params.bevel = bevel;
    let image = render_material_pixels(Material::Leather, 200, 120, 2.0, &params, &SurfaceScene::default())
        .expect("a render");
    let row: Vec<f32> = (0..60).map(|x| luma(pixel(&image, x, 60))).collect();
    let face = luma(pixel(&image, 100, 60));
    (row, face)
}

#[test]
fn a_cushion_bevel_draws_no_line_at_its_edge_or_at_its_crown() {
    // 8 px wide (16 device px), deep enough to shade clearly.
    let cushion = Bevel {
        width: 8.0,
        depth: 2.0,
        softness: 1.0,
        ..Bevel::NONE
    };
    let (row, face) = bevel_profile(cushion);
    let excursion = row.iter().map(|l| (l - face).abs()).fold(0.0, f32::max);
    assert!(excursion > 8.0, "the bevel shades its band: {excursion:.1} levels off the face");
    assert!(
        (row[0] - face).abs() <= 0.3 * excursion,
        "at the outer edge the cushion is nearly level: {:.1} vs the face's {face:.1}",
        row[0]
    );
    assert!(
        (row[17] - face).abs() <= 3.0 && (row[24] - face).abs() <= 3.0,
        "past the crown it is the face: {:.1} / {:.1} vs {face:.1}",
        row[17],
        row[24]
    );
    let steepest = row[1..].windows(2).map(|w| (w[1] - w[0]).abs()).fold(0.0, f32::max);
    assert!(
        steepest <= 0.3 * excursion,
        "no step anywhere along it ({steepest:.1} levels between two pixels of {excursion:.1})"
    );

    // The same bevel as a chisel (softness 0) has an edge: a step at the
    // crown, where its slope ends - which is what the cushion avoids.
    let (row, face) = bevel_profile(Bevel {
        softness: 0.0,
        ..cushion
    });
    let excursion = row.iter().map(|l| (l - face).abs()).fold(0.0, f32::max);
    let steepest = row[1..].windows(2).map(|w| (w[1] - w[0]).abs()).fold(0.0, f32::max);
    assert!(
        steepest > 0.5 * excursion,
        "a chisel steps at its crown ({steepest:.1} of {excursion:.1})"
    );
}

/// The luma of the four edges' bevels of a raised, flat 80 x 40 box at 2x:
/// top, right, bottom, left (mid-edge, inside the bevel).
fn lit_edges(material: Material, scene: &SurfaceScene) -> [f32; 4] {
    let mut params = flat(material);
    params.bevel = Bevel {
        width: 6.0,
        depth: 2.0,
        softness: 1.0,
        ..Bevel::NONE
    };
    let image = render_material_pixels(material, 160, 80, 2.0, &params, scene).expect("a render");
    [
        luma(pixel(&image, 80, 6)),
        luma(pixel(&image, 153, 40)),
        luma(pixel(&image, 80, 73)),
        luma(pixel(&image, 6, 40)),
    ]
}

#[test]
fn every_surface_is_lit_from_the_one_light() {
    // The default light: the sun at the top right, low.
    let scene = SurfaceScene::default();
    assert_eq!(scene.light, Light::DEFAULT, "a surface is lit by the app's light by default");
    for material in [Material::Leather, Material::BlueJeans, Material::Vellum] {
        let [top, right, bottom, left] = lit_edges(material, &scene);
        assert!(
            top > bottom + 4.0 && right > left + 4.0,
            "{material:?}: the edges facing the top right are the lit ones \
             (top {top:.0} bottom {bottom:.0} right {right:.0} left {left:.0})"
        );
    }
    // The sun at the bottom left lights the other two edges.
    let low_left = SurfaceScene {
        light: Light::sun(225.0, 36.0),
        ..SurfaceScene::default()
    };
    let [top, right, bottom, left] = lit_edges(Material::Leather, &low_left);
    assert!(bottom > top + 4.0 && left > right + 4.0, "lit from the bottom left");

    // A point light to the left of the surface lights its left edge.
    let point = SurfaceScene {
        light: Light::point(-400.0, 40.0, 120.0),
        desktop_origin: [0.0, 0.0],
        ..SurfaceScene::default()
    };
    let [_, right, _, left] = lit_edges(Material::Leather, &point);
    assert!(left > right + 4.0, "a point light at the left: left {left:.0} right {right:.0}");
}

#[test]
fn the_grain_continues_from_one_surface_into_the_next() {
    // Two surfaces side by side are one hide: the right one rendered at its
    // place in the window is the right half of the two rendered as one.
    let mut params = MaterialParams::of(Material::Leather);
    params.bevel = Bevel::NONE;
    let whole = render_material_pixels(Material::Leather, 160, 40, 2.0, &params, &SurfaceScene::default())
        .expect("a render");
    let right_half = SurfaceScene {
        window_offset: [40.0, 0.0],
        desktop_origin: [40.0, 0.0],
        ..SurfaceScene::default()
    };
    let half = render_material_pixels(Material::Leather, 80, 40, 2.0, &params, &right_half)
        .expect("a render");
    let mut same = 0;
    let mut worst = 0;
    for y in 0..40 {
        for x in 0..80 {
            let a = pixel(&whole, x + 80, y);
            let b = pixel(&half, x, y);
            let d = (0..4).map(|c| a[c].abs_diff(b[c])).max().unwrap_or(0);
            worst = worst.max(d);
            if d == 0 {
                same += 1;
            }
        }
    }
    assert!(worst <= 1, "no seam between the two: the largest difference is {worst}");
    assert!(same * 100 >= 99 * 80 * 40, "{same} of {} pixels identical", 80 * 40);
}
