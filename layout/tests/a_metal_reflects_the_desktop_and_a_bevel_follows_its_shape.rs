//! Two things a material reads besides its own surface: the desktop behind
//! the window, which a metal reflects (blurred by its roughness, the
//! system's highlight colour where there is no wallpaper), and the shape it
//! is cut to, whose bevel follows any outline - a `clip-path` - through the
//! exact Euclidean distance to its edge.

use std::sync::Arc;

use azul_css::{
    props::{basic::PixelValue, layout::shape::ClipPath},
    shape::{CssShape, ShapePosition, ShapeRadius},
};
use azul_layout::texture::{
    environment::{Environment, Wallpaper},
    materials::{
        render_material_pixels, Bevel, BevelFollow, MaterialParams, MaterialSource, ShapeField,
        SurfaceScene,
    },
    Material,
};
use azul_core::resources::{RawImage, RawImageData};

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

/// The mean red and blue of an aluminium strip in `environment`.
fn silver_under(environment: Environment) -> (f32, f32) {
    let mut params = MaterialParams::of(Material::Aluminium);
    params.set_tint_srgb([0xC8, 0xCC, 0xD2]);
    let scene = SurfaceScene {
        environment,
        desktop_origin: [100.0, 100.0],
        screen: [0.0, 0.0, 1000.0, 800.0],
        ..SurfaceScene::default()
    };
    let image = render_material_pixels(Material::Aluminium, 200, 40, 2.0, &params, &scene)
        .expect("a render");
    let (mut r, mut b) = (0.0, 0.0);
    for y in 0..image.height {
        for x in 0..image.width {
            let p = pixel(&image, x, y);
            r += f32::from(p[0]);
            b += f32::from(p[2]);
        }
    }
    let n = (image.width * image.height) as f32;
    (r / n, b / n)
}

#[test]
fn a_metal_reflects_the_wallpaper_behind_it_and_else_the_highlight_colour() {
    // No wallpaper: the system's text highlight colour (a pale blue).
    let (r, b) = silver_under(Environment::highlight([0xB3, 0xD7, 0xFF]));
    assert!(b > r + 6.0, "silver reflects the blue highlight: red {r:.0} blue {b:.0}");

    // A red wallpaper behind the window: the silver turns warm.
    let red = vec![[230_u8, 40, 40, 255]; 64 * 40].concat();
    let wallpaper = Wallpaper::from_rgba8(64, 40, &red, [0.0, 0.0, 1000.0, 800.0])
        .expect("a wallpaper");
    let (r, b) = silver_under(Environment::wallpapers(vec![Arc::new(wallpaper)]));
    assert!(r > b + 6.0, "silver reflects the red desktop: red {r:.0} blue {b:.0}");
}

#[test]
fn the_distance_field_of_a_shape_is_its_euclidean_distance() {
    // A disc of radius 30 around (50, 50) in a 100 x 100 box at 2x, its
    // distances kept 40 px each way.
    let (cx, cy, radius) = (50.0_f32, 50.0_f32, 30.0_f32);
    let field = ShapeField::from_mask_with_range(200, 200, 2.0, 40.0, |x, y| {
        let (px, py) = ((x as f32 + 0.5) / 2.0, (y as f32 + 0.5) / 2.0);
        (px - cx).hypot(py - cy) < radius
    });
    let mut worst = 0.0_f32;
    for &(px, py) in &[
        (50.25, 50.25),
        (50.25, 22.25),
        (78.75, 50.25),
        (65.25, 65.25),
        (30.75, 70.25),
        (5.25, 5.25),
        (95.25, 50.25),
        (50.25, 81.75),
    ] {
        let exact = (px - cx).hypot(py - cy) - radius;
        let got = field.distance_at(px, py);
        worst = worst.max((got - exact).abs());
        assert_eq!(got < 0.0, exact < 0.0, "inside is negative at ({px}, {py})");
    }
    assert!(worst <= 0.75, "within a device pixel of the true distance (worst {worst:.2} px)");
}

#[test]
fn a_bevel_follows_the_clip_path_it_is_cut_to() {
    let circle = ClipPath::Shape(CssShape::circle(
        ShapePosition::px(50.0, 50.0),
        ShapeRadius::Length(PixelValue::px(40.0)),
    ));
    let field = ShapeField::from_clip_path(&circle, 100.0, 100.0, 2.0).expect("a circle's field");
    let mut params = MaterialParams::of(Material::Leather);
    params.source = MaterialSource::Smooth;
    params.sheen = 0.0;
    params.bevel = Bevel {
        width: 6.0,
        depth: 2.0,
        softness: 1.0,
        follow: BevelFollow::Shape,
        ..Bevel::NONE
    };
    let scene = SurfaceScene {
        shape: Some(Arc::new(field)),
        ..SurfaceScene::default()
    };
    let image = render_material_pixels(Material::Leather, 200, 200, 2.0, &params, &scene)
        .expect("a render");
    assert_eq!(pixel(&image, 6, 6)[3], 0, "the box's corner lies outside the circle");
    assert_eq!(pixel(&image, 100, 100)[3], 255, "its middle inside");
    let face = luma(pixel(&image, 100, 100));
    // 3 px inside the circle's top and bottom: the rim's slope, lit from the
    // top right, is bright at the top and dark at the bottom.
    let top = luma(pixel(&image, 100, 26));
    let bottom = luma(pixel(&image, 100, 173));
    assert!(top > face + 4.0, "the top of the rim catches the light: {top:.0} vs {face:.0}");
    assert!(bottom < face - 4.0, "the bottom of the rim is in shade: {bottom:.0} vs {face:.0}");
    // The bevel runs along the circle, not along the box: the box's own
    // top edge at the left (outside the circle) draws nothing.
    let left_of_circle = pixel(&image, 30, 2);
    assert_eq!(left_of_circle[3], 0, "no bevel along the box where the circle is not");
}
