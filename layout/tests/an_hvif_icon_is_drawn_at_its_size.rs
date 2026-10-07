//! An HVIF icon (Haiku's vector icon format) drawn at a pixel size by the
//! CPU renderer: its gradients as gradients, its strokes as strokes, at any
//! size. The icon: Haiku's "New Mail File" (MIT, Haiku, Inc.) - a yellow
//! paper (a linear gradient #fffcac -> #e6e27c) outlined in black, with a
//! brown fold.

use azul_layout::cpurender::hvif::render_hvif_to_raw_image;

fn icon(name: &str) -> Vec<u8> {
    let path = format!(
        "{}/../examples/azul-mail/icons/haiku/{name}.hvif",
        env!("CARGO_MANIFEST_DIR")
    );
    std::fs::read(&path).unwrap_or_else(|e| panic!("read {path}: {e}"))
}

/// (r, g, b, a) at (x, y) of a premultiplied RGBA8 picture, un-premultiplied.
fn at(image: &azul_core::resources::RawImage, x: usize, y: usize) -> (u8, u8, u8, u8) {
    let azul_core::resources::RawImageData::U8(bytes) = &image.pixels else {
        panic!("8-bit");
    };
    let i = (y * image.width + x) * 4;
    let p = &bytes.as_ref()[i..i + 4];
    let un = |c: u8| if p[3] == 0 { 0 } else { (u32::from(c) * 255 / u32::from(p[3])) as u8 };
    (un(p[0]), un(p[1]), un(p[2]), p[3])
}

#[test]
fn the_paper_is_its_gradient_the_outline_black_the_corner_clear() {
    let image = render_hvif_to_raw_image(&icon("message"), 64).expect("drawn");
    assert_eq!((image.width, image.height), (64, 64));
    // The paper away from the fold: the gradient's yellow, shaded.
    let (r, g, b, a) = at(&image, 46, 38);
    assert!(a > 250 && r > 180 && g > 170 && b < 150 && r > b + 60, "yellow paper: {:?}", (r, g, b, a));
    assert_eq!(at(&image, 1, 1).3, 0, "nothing drawn in the corner");
    // The outline is a 4-unit black stroke along M2 38 L22 21, under the
    // paper: its outer half, just outside the edge's middle.
    let (r, g, b, a) = at(&image, 11, 28);
    assert!(a > 200 && r < 60 && g < 60 && b < 60, "black outline: {:?}", (r, g, b, a));
}

#[test]
fn an_icon_is_drawn_at_any_size() {
    for size in [16, 24, 32, 48, 128] {
        let image = render_hvif_to_raw_image(&icon("message"), size).expect("drawn");
        assert_eq!(image.width, size as usize);
        let azul_core::resources::RawImageData::U8(bytes) = &image.pixels else {
            panic!("8-bit");
        };
        let covered = bytes.as_ref().chunks(4).filter(|p| p[3] > 0).count();
        assert!(covered > (size * size / 5) as usize, "{size} px: {covered} pixels drawn");
    }
    assert!(render_hvif_to_raw_image(&icon("message"), 0).is_err());
    assert!(render_hvif_to_raw_image(b"not an icon", 32).is_err());
}

#[test]
fn every_icon_azmail_ships_draws() {
    let dir = format!("{}/../examples/azul-mail/icons/haiku", env!("CARGO_MANIFEST_DIR"));
    let mut count = 0;
    for entry in std::fs::read_dir(&dir).expect("the icons") {
        let path = entry.expect("an entry").path();
        if path.extension().is_some_and(|e| e == "hvif") {
            let bytes = std::fs::read(&path).expect("read");
            render_hvif_to_raw_image(&bytes, 32)
                .unwrap_or_else(|e| panic!("{}: {e}", path.display()));
            count += 1;
        }
    }
    assert!(count >= 30, "{count} icons");
}

