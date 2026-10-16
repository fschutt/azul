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
//!
//! MATERIALS - `builtin(leather)`, `builtin(bluejeans)`, `builtin(aluminium)`
//! - are surfaces, not pictures: a micro tile of each CC0 PBR set (albedo,
//! normal, roughness) and a small CPU shader that lights it for the rect it
//! fills, at that rect's size, with a bevel around its rounded edge
//! (`texture::materials`, cargo feature `materials`). The window's light
//! ([`MaterialLight`], `CallbackInfo::set_material_light`) is defined here in
//! every build, so the API is the same with or without the feature; without
//! it the names stay transparent, like any `builtin(..)` no window has.

use alloc::vec::Vec;

use azul_core::resources::{ImageCache, ImageRef, RawImage, RawImageData, RawImageFormat};
use azul_css::props::basic::ColorU;

// The 2.5D materials' tiles, decoder, shader and render cache.
#[cfg(feature = "materials")]
pub mod materials;
// What the metals reflect: the desktop's wallpapers or the highlight colour.
#[cfg(feature = "materials")]
pub mod environment;
// The cursor's sheen on the materials: its easing and the areas it follows.
pub mod sheen;

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

/// A window's image cache with every built-in texture under its CSS name -
/// and, with the `materials` feature, every [`Material`] as the callback
/// image that renders it for the rect it fills.
#[must_use]
pub fn image_cache_with_builtin_textures() -> ImageCache {
    let mut cache = ImageCache::default();
    for texture in BuiltinTexture::ALL {
        cache.add_css_image_id(texture.css_name().as_str().into(), builtin_texture(texture));
    }
    #[cfg(feature = "materials")]
    for material in Material::ALL {
        cache.add_css_image_id(material.css_name().as_str().into(), material_image(material));
    }
    cache
}

// -- Materials: the always-present half (the light, the names) ---------------

/// A material compiled into the library (cargo feature `materials`): a
/// surface lit per device pixel by the app's light for the rect it fills -
/// `background: builtin(leather);` (see `texture::materials`).
#[repr(C)]
#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Material {
    /// Fine-grained nappa leather, pale grey unless tinted.
    Leather,
    /// Denim, a 3/1 twill, indigo unless tinted.
    BlueJeans,
    /// Brushed aluminium, a metal (gold unless tinted): it reflects the
    /// desktop behind the window.
    Aluminium,
    /// Vellum, a page of parchment: blotches, fibres, creases.
    Vellum,
}

impl Material {
    /// Every material.
    pub const ALL: [Self; 4] = [Self::Leather, Self::BlueJeans, Self::Aluminium, Self::Vellum];

    /// The name `builtin(<name>)` takes.
    #[must_use]
    pub const fn builtin_name(self) -> &'static str {
        match self {
            Self::Leather => "leather",
            Self::BlueJeans => "bluejeans",
            Self::Aluminium => "aluminium",
            Self::Vellum => "vellum-leaf",
        }
    }

    /// The image id every window has the material under (`azul-builtin:<name>`).
    #[must_use]
    pub fn css_name(self) -> String {
        alloc::format!(
            "{}{}",
            azul_css::props::style::background::BUILTIN_IMAGE_PREFIX,
            self.builtin_name()
        )
    }

    /// The material an image id names ([`Self::css_name`]), if it names one.
    #[must_use]
    pub fn from_css_name(id: &str) -> Option<Self> {
        let name = id.strip_prefix(azul_css::props::style::background::BUILTIN_IMAGE_PREFIX)?;
        Self::ALL.into_iter().find(|m| m.builtin_name() == name)
    }
}

/// The accent as a strip of light: its hue at 60% lightness and 90%
/// saturation. What the `AzDrive` design's golden line throws into the content
/// below it (a gradient from this colour at a low alpha to transparent).
#[must_use]
#[allow(clippy::many_single_char_names)] // the HSL formulas' own letters
pub fn accent_glow(accent: ColorU) -> ColorU {
    let v = [accent.r, accent.g, accent.b].map(|c| f32::from(c) / 255.0);
    let mx = v[0].max(v[1]).max(v[2]);
    let mn = v[0].min(v[1]).min(v[2]);
    let d = mx - mn;
    let hue = if d <= 0.0 {
        0.0
    } else if mx == v[0] {
        ((v[1] - v[2]) / d) % 6.0
    } else if mx == v[1] {
        (v[2] - v[0]) / d + 2.0
    } else {
        (v[0] - v[1]) / d + 4.0
    };
    let hue = (hue * 60.0 + 360.0) % 360.0;
    let (l, sat) = (0.6_f32, 0.9_f32);
    let c = (1.0 - (2.0 * l - 1.0).abs()) * sat;
    let x = c * (1.0 - ((hue / 60.0) % 2.0 - 1.0).abs());
    let m = l - c / 2.0;
    let (r, g, b) = if hue < 60.0 {
        (c, x, 0.0)
    } else if hue < 120.0 {
        (x, c, 0.0)
    } else if hue < 180.0 {
        (0.0, c, x)
    } else if hue < 240.0 {
        (0.0, x, c)
    } else if hue < 300.0 {
        (x, 0.0, c)
    } else {
        (c, 0.0, x)
    };
    let byte = |q: f32| ((q + m) * 255.0).round().clamp(0.0, 255.0) as u8;
    ColorU {
        r: byte(r),
        g: byte(g),
        b: byte(b),
        a: 255,
    }
}

/// The material as an image for `Dom::create_image` or a CSS id of the app's
/// own: a render callback that lights the material for the size it is drawn
/// at. Without the `materials` feature a transparent pixel.
#[must_use]
pub fn material_image(material: Material) -> ImageRef {
    #[cfg(feature = "materials")]
    {
        materials::material_callback_image(material)
    }
    #[cfg(not(feature = "materials"))]
    {
        let _ = material;
        transparent_pixel()
    }
}

/// One transparent RGBA8 pixel: an image that draws nothing (a `NullImage`
/// would be painted as the grey "no pixels" placeholder).
#[must_use]
pub fn transparent_pixel() -> ImageRef {
    ImageRef::new_rawimage(RawImage {
        pixels: RawImageData::U8(alloc::vec![0_u8; 4].into()),
        width: 1,
        height: 1,
        premultiplied_alpha: true,
        data_format: RawImageFormat::RGBA8,
        tag: Vec::new().into(),
    })
    .unwrap_or_else(|| ImageRef::null_image(1, 1, RawImageFormat::RGBA8, b"transparent".to_vec()))
}

/// A window's OWN light for its materials, outranking the app's one light
/// (`azul_core::light::Light`) in that window
/// (`CallbackInfo::set_material_light`, `LayoutWindow::set_material_light`):
/// its direction as a sun, and the light's colour, strength, ambient share
/// and god rays, which every window takes from here. Directions are on the
/// SCREEN: x to the right, y up, the viewer in front.
#[repr(C)]
#[derive(Debug, Copy, Clone, PartialEq)]
pub struct MaterialLight {
    /// Where the light comes from, degrees counter-clockwise from the right:
    /// 45 = the upper right (the default, the app's sun), 135 = the upper
    /// left, 90 = above, 270 = below.
    pub azimuth_deg: f32,
    /// How high it stands over the screen, degrees: 90 = straight in front
    /// (no bevel is darker than another), small = grazing (strong
    /// highlights and shadows).
    pub elevation_deg: f32,
    /// Its colour (a warm white by default).
    pub color: ColorU,
    /// Its strength: at 1.0 a flat texel shows exactly its albedo.
    pub intensity: f32,
    /// The share of the light that comes from everywhere, 0..1 (the rest
    /// comes from the direction above).
    pub ambient: f32,
    /// A soft band of light across the window - god rays. Off by default.
    pub rays: MaterialRays,
}

impl MaterialLight {
    /// The default light: the app's default sun (the top right, 36 degrees
    /// high), a warm white (linear 1, 0.975, 0.94), 45% ambient.
    pub const DEFAULT: Self = Self {
        azimuth_deg: 45.0,
        elevation_deg: 36.0,
        color: ColorU {
            r: 255,
            g: 252,
            b: 248,
            a: 255,
        },
        intensity: 1.0,
        ambient: 0.45,
        rays: MaterialRays::OFF,
    };

    /// The default light.
    #[must_use]
    pub const fn create() -> Self {
        Self::DEFAULT
    }

    /// The default light from another direction.
    #[must_use]
    pub const fn from_direction(azimuth_deg: f32, elevation_deg: f32) -> Self {
        Self {
            azimuth_deg,
            elevation_deg,
            ..Self::DEFAULT
        }
    }

    /// The same light with these god rays.
    #[must_use]
    pub const fn with_rays(self, rays: MaterialRays) -> Self {
        Self { rays, ..self }
    }
}

impl Default for MaterialLight {
    fn default() -> Self {
        Self::DEFAULT
    }
}

/// God rays: a soft band of light lying across the whole window (window
/// coordinates, so every surface it crosses brightens where it passes),
/// running in `angle_deg`'s direction. Animate `phase` to sweep it.
#[repr(C)]
#[derive(Debug, Copy, Clone, PartialEq)]
pub struct MaterialRays {
    /// 0 = off; 1 = the band doubles the light at its centre.
    pub strength: f32,
    /// The direction the band runs in, degrees as `MaterialLight::azimuth_deg`.
    pub angle_deg: f32,
    /// The band's width (full width at half maximum), logical px.
    pub width: f32,
    /// 0 = a single band; otherwise bands repeat every `spacing` logical px.
    pub spacing: f32,
    /// Where the band lies across its direction, logical px from the
    /// window's origin.
    pub phase: f32,
}

impl MaterialRays {
    /// No rays.
    pub const OFF: Self = Self {
        strength: 0.0,
        angle_deg: 135.0,
        width: 160.0,
        spacing: 0.0,
        phase: 0.0,
    };

    /// One band of `strength`, `width` logical px wide, running in
    /// `angle_deg`'s direction, `phase` px across from the window's origin.
    #[must_use]
    pub const fn create(strength: f32, angle_deg: f32, width: f32, phase: f32) -> Self {
        Self {
            strength,
            angle_deg,
            width,
            spacing: 0.0,
            phase,
        }
    }
}

impl Default for MaterialRays {
    fn default() -> Self {
        Self::OFF
    }
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
    fn every_material_is_a_builtin_name_the_css_parser_knows() {
        for material in Material::ALL {
            assert!(
                azul_css::props::style::background::BUILTIN_IMAGES
                    .contains(&material.builtin_name()),
                "the CSS parser knows `builtin({})`",
                material.builtin_name()
            );
        }
        // With the feature every window has each material as the callback
        // image that renders it for the rect it fills; without it the names
        // name nothing (transparent).
        let cache = image_cache_with_builtin_textures();
        for material in Material::ALL {
            let image = cache.get_css_image_id(&material.css_name().as_str().into());
            #[cfg(feature = "materials")]
            assert!(image.is_some_and(ImageRef::is_callback), "{}", material.css_name());
            #[cfg(not(feature = "materials"))]
            assert!(image.is_none(), "{}", material.css_name());
        }
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
