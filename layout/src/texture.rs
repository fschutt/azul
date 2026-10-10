//! Built-in textures: grain compiled into the library, for an app to lay a
//! little texture over its surfaces without shipping an image of its own.
//!
//! Two ways in:
//!
//! - [`builtin_texture`] (`ImageRef::create_builtin_texture` in the API) - the
//!   texture as an image, for `Dom::create_image` or `add_image_to_cache`;
//! - CSS: `builtin(<name>)`, composed like `url(foo.png)` -
//!   `background: builtin(vellum-overlay), #f2f1ed;` lays the vellum grain
//!   over the colour (the first layer is the top one, and the image repeats
//!   by default). Every window has the textures registered under their
//!   reserved ids ([`BuiltinTexture::css_name`], `azul-builtin:<name>`) from
//!   the start; the CSS parser knows the names (`BUILTIN_IMAGES`).
//!
//! The vellum is stored as its INK - how much darker than the paper each
//! pixel is - in 16 levels, two pixels per byte (`assets/textures/README.md`):
//! 8 KiB, and no image decoder needed, so it is there in every build.

use alloc::vec::Vec;

use azul_core::resources::{ImageCache, ImageRef, RawImage, RawImageData, RawImageFormat};

/// A texture compiled into the library.
#[repr(C)]
#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum BuiltinTexture {
    /// Vellum (parchment) grain in black and white, opaque: white paper with
    /// grey creases (CC0).
    Vellum,
    /// The vellum's grain as black ink at a low alpha, to lay over any colour
    /// - the paper itself is transparent.
    VellumOverlay,
}

/// The vellum's side, in pixels (it tiles seamlessly).
pub const VELLUM_SIZE: usize = 128;

/// The vellum's ink, 4 bits a pixel, high nibble first, row by row.
static VELLUM_INK: &[u8; VELLUM_SIZE * VELLUM_SIZE / 2] =
    include_bytes!("../assets/textures/vellum_ink.bin");

/// The paper's brightness and the deepest crease's in [`BuiltinTexture::Vellum`]
/// (the source image's range).
const PAPER: u8 = 247;
const DEEPEST_CREASE: u8 = 195;

/// The overlay's alpha at the deepest crease: the grain reads on any colour
/// without turning it grey.
const OVERLAY_MAX_ALPHA: u8 = 56;

impl BuiltinTexture {
    /// Every built-in texture.
    pub const ALL: [Self; 2] = [Self::Vellum, Self::VellumOverlay];

    /// The name `builtin(<name>)` takes (one of the CSS parser's
    /// `BUILTIN_IMAGES`).
    #[must_use]
    pub const fn builtin_name(self) -> &'static str {
        match self {
            Self::Vellum => "vellum",
            Self::VellumOverlay => "vellum-overlay",
        }
    }

    /// The image id every window has the texture under: what `builtin(<name>)`
    /// parses to (`azul-builtin:<name>`).
    #[must_use]
    pub fn css_name(self) -> String {
        alloc::format!(
            "{}{}",
            azul_css::props::style::background::BUILTIN_IMAGE_PREFIX,
            self.builtin_name()
        )
    }
}

/// The ink of pixel `i` of the vellum, 0 (paper) to 255 (the deepest crease).
fn vellum_ink(i: usize) -> u8 {
    let byte = VELLUM_INK[i / 2];
    let level = if i.is_multiple_of(2) { byte >> 4 } else { byte & 0x0f };
    level * 17
}

/// `value` scaled by `ink / 255`.
fn scaled(value: u8, ink: u8) -> u8 {
    u8::try_from(u16::from(value) * u16::from(ink) / 255).unwrap_or(u8::MAX)
}

/// The texture as an RGBA8 image (premultiplied: the overlay's ink is black,
/// so its colour channels stay 0 whatever the alpha).
#[must_use]
pub fn builtin_raw_texture(texture: BuiltinTexture) -> RawImage {
    let n = VELLUM_SIZE * VELLUM_SIZE;
    let mut pixels: Vec<u8> = Vec::with_capacity(n * 4);
    for i in 0..n {
        let ink = vellum_ink(i);
        match texture {
            BuiltinTexture::Vellum => {
                let v = PAPER - scaled(PAPER - DEEPEST_CREASE, ink);
                pixels.extend_from_slice(&[v, v, v, 255]);
            }
            BuiltinTexture::VellumOverlay => {
                pixels.extend_from_slice(&[0, 0, 0, scaled(OVERLAY_MAX_ALPHA, ink)]);
            }
        }
    }
    RawImage {
        pixels: RawImageData::U8(pixels.into()),
        width: VELLUM_SIZE,
        height: VELLUM_SIZE,
        premultiplied_alpha: true,
        data_format: RawImageFormat::RGBA8,
        tag: Vec::new().into(),
    }
}

/// The texture as an image (`ImageRef::create_builtin_texture`).
#[must_use]
pub fn builtin_texture(texture: BuiltinTexture) -> ImageRef {
    ImageRef::new_rawimage(builtin_raw_texture(texture)).unwrap_or_else(|| {
        ImageRef::null_image(VELLUM_SIZE, VELLUM_SIZE, RawImageFormat::RGBA8, Vec::new())
    })
}

/// A window's image cache with every built-in texture under its CSS name.
#[must_use]
pub fn image_cache_with_builtin_textures() -> ImageCache {
    let mut cache = ImageCache::default();
    for texture in BuiltinTexture::ALL {
        cache.add_css_image_id(texture.css_name().as_str().into(), builtin_texture(texture));
    }
    cache
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_vellum_is_paper_with_grey_creases_and_the_overlay_ink_on_nothing() {
        let paper = builtin_raw_texture(BuiltinTexture::Vellum);
        let overlay = builtin_raw_texture(BuiltinTexture::VellumOverlay);
        let (RawImageData::U8(p), RawImageData::U8(o)) = (&paper.pixels, &overlay.pixels) else {
            panic!("RGBA8 bytes");
        };
        assert_eq!(p.as_ref().len(), VELLUM_SIZE * VELLUM_SIZE * 4);
        assert_eq!(o.as_ref().len(), VELLUM_SIZE * VELLUM_SIZE * 4);
        for (pp, op) in p.as_ref().chunks(4).zip(o.as_ref().chunks(4)) {
            assert!(pp[0] == pp[1] && pp[1] == pp[2], "black and white");
            assert_eq!(pp[3], 255, "the vellum is opaque");
            assert!((DEEPEST_CREASE..=PAPER).contains(&pp[0]));
            assert_eq!(&op[..3], &[0, 0, 0], "the overlay is black ink");
            assert!(op[3] <= OVERLAY_MAX_ALPHA);
        }
        // Grain, not a flat fill: creases of several depths.
        let mut depths: Vec<u8> = p.as_ref().chunks(4).map(|px| px[0]).collect();
        depths.sort_unstable();
        depths.dedup();
        assert!(depths.len() >= 8, "only {} shades", depths.len());
    }

    #[test]
    fn every_window_knows_the_textures_by_their_css_names() {
        let cache = image_cache_with_builtin_textures();
        for texture in BuiltinTexture::ALL {
            assert!(
                cache.get_css_image_id(&texture.css_name().as_str().into()).is_some(),
                "{}",
                texture.css_name()
            );
            assert!(
                azul_css::props::style::background::BUILTIN_IMAGES
                    .contains(&texture.builtin_name()),
                "the CSS parser knows `builtin({})`",
                texture.builtin_name()
            );
        }
    }
}
