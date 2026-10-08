//! Text into pixels: a string set in a font and rasterised into an RGBA8
//! [`RawImage`] - what a photo editor's text tool, a watermark or a label
//! drawn into a picture needs (the DOM draws text into the WINDOW; this draws
//! it into an image the app owns).
//!
//! One path, no twin: the lines are shaped by the text engine
//! (`text3::default::shape_text_for_parsed_font`: kerning, ligatures, marks)
//! and their glyphs rasterised by the CPU renderer's own glyph path
//! (`raster::render_text`) in GRAYSCALE coverage - the target is
//! transparent, so there is no background for LCD subpixels to sit on.
//!
//! - [`text_image`] / [`text_image_with`]: a straight-alpha RGBA8 image as
//!   big as the lines' boxes (`RawImage::from_text` in the C API).
//! - [`draw_text`] / [`draw_text_with`]: the same pixels composited into an
//!   RGBA8 / BGRA8 image (straight or premultiplied) with the first line's
//!   box at (`x`, `y`) (`RawImage::draw_text`).
//!
//! The `_with` forms take the font cache (tests hand in a memory font); the
//! plain forms use a font cache of the system fonts built once per thread
//! on first use (a scan of the font folders - the first call takes that
//! long, so an app that draws text in a callback may warm it on a thread).

use azul_core::resources::{OptionRawImage, RawImage, RawImageData, RawImageFormat};
use azul_css::{props::basic::ColorU, AzString};

/// The largest em size and the largest side of a text image, in pixels.
pub const TEXT_RASTER_MAX_SIZE_PX: f32 = 4096.0;
pub const TEXT_RASTER_MAX_SIDE_PX: u32 = 16384;

/// How [`text_image`] / [`draw_text`] set a text: the font, its size, the
/// line spacing and the colour.
#[repr(C)]
#[derive(Debug, Clone, PartialEq)]
pub struct TextRasterStyle {
    /// A family name: `sans-serif`, `serif`, `monospace` or an installed
    /// family. A family that is not there falls back to one that covers the
    /// text.
    pub font_family: AzString,
    /// The em size in image pixels.
    pub size_px: f32,
    /// The distance between baselines as a multiple of the font's own line
    /// height (1.0 = the font's).
    pub line_height: f32,
    /// The ink colour; its alpha scales the coverage.
    pub color: ColorU,
    /// The bold face of the family (the regular one where it has none).
    pub bold: bool,
    /// The italic face of the family (the regular one where it has none).
    pub italic: bool,
}

impl TextRasterStyle {
    /// `font_family` at `size_px`, in `color`, the font's own line height.
    #[must_use]
    pub fn create(font_family: AzString, size_px: f32, color: ColorU) -> Self {
        Self {
            font_family,
            size_px,
            line_height: 1.0,
            color,
            bold: false,
            italic: false,
        }
    }

    #[must_use]
    pub fn with_bold(mut self, bold: bool) -> Self {
        self.bold = bold;
        self
    }

    #[must_use]
    pub fn with_italic(mut self, italic: bool) -> Self {
        self.italic = italic;
        self
    }

    /// Baselines `line_height` times the font's line height apart.
    #[must_use]
    pub fn with_line_height(mut self, line_height: f32) -> Self {
        self.line_height = line_height;
        self
    }

    /// The line-height multiple in use: a non-positive or non-finite one is 1.
    fn line_factor(&self) -> f32 {
        if self.line_height.is_finite() && self.line_height > 0.0 {
            self.line_height
        } else {
            1.0
        }
    }
}

/// A set text: PREMULTIPLIED RGBA8 rows (the renderer's own format).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TextPixels {
    pub width: u32,
    pub height: u32,
    pub rgba_premultiplied: Vec<u8>,
}

// ==== Setting and rasterising (needs fonts) ====

#[cfg(all(feature = "std", feature = "text_layout", feature = "font_loading"))]
mod set {
    use azul_core::{
        geom::{LogicalPosition, LogicalRect, LogicalSize},
        resources::RendererResources,
        ui_solver::GlyphInstance,
    };
    use azul_css::props::basic::FontRef;
    use rust_fontconfig::{FcFontCache, FcPattern, FcWeight, OwnedFontSource, PatternMatch};

    use super::{TextPixels, TextRasterStyle, TEXT_RASTER_MAX_SIDE_PX, TEXT_RASTER_MAX_SIZE_PX};
    use crate::{
        cpurender::AzulPixmap,
        font::parsed::ParsedFont,
        glyph_cache::GlyphCache,
        text3::{
            cache::{BidiDirection, FontHash, FontManager, StyleProperties},
            default::shape_text_for_parsed_font,
            script::{detect_script, Language, Script},
        },
    };

    /// The face for `style`: the family with the asked weight and slant,
    /// then the family as it is, then `sans-serif`; each through
    /// fontconfig's own fallback ladder (exact, family-relaxed, coverage).
    fn resolve_font(fc: &FcFontCache, style: &TextRasterStyle) -> Option<ParsedFont> {
        let family = style.font_family.as_str().trim();
        let family = if family.is_empty() {
            "sans-serif"
        } else {
            family
        };
        let styled = FcPattern {
            family: Some(family.to_string()),
            weight: if style.bold {
                FcWeight::Bold
            } else {
                FcWeight::Normal
            },
            bold: if style.bold {
                PatternMatch::True
            } else {
                PatternMatch::DontCare
            },
            italic: if style.italic {
                PatternMatch::True
            } else {
                PatternMatch::DontCare
            },
            ..Default::default()
        };
        let plain = FcPattern {
            family: Some(family.to_string()),
            ..Default::default()
        };
        let generic = FcPattern {
            family: Some("sans-serif".to_string()),
            ..Default::default()
        };
        let mut trace = Vec::new();
        let matched = [styled, plain, generic]
            .iter()
            .find_map(|p| fc.query_with_fallback(p, &mut trace))?;
        let bytes = fc.get_font_bytes(&matched.id)?;
        let index = fc.get_font_by_id(&matched.id).map_or(0, |src| match src {
            OwnedFontSource::Disk(path) => path.font_index,
            OwnedFontSource::Memory(font) => font.font_index,
        });
        Some(
            ParsedFont::from_bytes(bytes.as_slice(), index, &mut Vec::new())?
                .with_source_bytes(bytes.clone()),
        )
    }

    /// `text` set in `style` with the first line's box at (`dx`, `dy`) (the
    /// fractional offset of a draw), as premultiplied RGBA8 as big as the
    /// lines' boxes plus that offset. `None`: no text, no usable size, no
    /// font, or an image larger than [`TEXT_RASTER_MAX_SIDE_PX`].
    #[allow(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        clippy::cast_precision_loss
    )]
    #[must_use]
    pub fn rasterize_text_at(
        fc: &FcFontCache,
        text: &str,
        style: &TextRasterStyle,
        dx: f32,
        dy: f32,
    ) -> Option<TextPixels> {
        let size = style.size_px;
        if text.is_empty() || !size.is_finite() || size <= 0.0 || size > TEXT_RASTER_MAX_SIZE_PX {
            return None;
        }
        let parsed = resolve_font(fc, style)?;
        let upm = f32::from(parsed.font_metrics.units_per_em);
        if upm <= 0.0 {
            return None;
        }
        let scale = size / upm;
        let ascent = parsed.font_metrics.ascent * scale;
        let descent = parsed.font_metrics.descent.abs() * scale;
        let gap = parsed.font_metrics.line_gap.max(0.0) * scale;
        let line_box = ascent + descent + gap;
        if !line_box.is_finite() || line_box <= 0.0 {
            return None;
        }
        let step = line_box * style.line_factor();
        let shaping = StyleProperties {
            font_size_px: size,
            ..StyleProperties::default()
        };

        let mut glyphs = Vec::new();
        let mut width = 0.0f32;
        let mut lines = 0usize;
        for (i, line) in text.split('\n').enumerate() {
            lines = i + 1;
            let line = line.strip_suffix('\r').unwrap_or(line);
            let baseline = dy + i as f32 * step + ascent;
            let mut pen = dx;
            if !line.is_empty() {
                let script = detect_script(line).unwrap_or(Script::Latin);
                let shaped = shape_text_for_parsed_font(
                    &parsed,
                    line,
                    script,
                    Language::EnglishUS,
                    BidiDirection::Ltr,
                    &shaping,
                )
                .ok()?;
                for g in &shaped {
                    glyphs.push(GlyphInstance {
                        index: u32::from(g.glyph_id),
                        point: LogicalPosition {
                            x: pen + g.offset.x,
                            y: baseline - g.offset.y,
                        },
                        size: LogicalSize {
                            width: g.advance,
                            height: size,
                        },
                    });
                    pen += g.advance + g.kerning;
                }
            }
            width = width.max(pen - dx);
        }
        let height = line_box + lines.saturating_sub(1) as f32 * step;
        let w = (width + dx).ceil();
        let h = (height + dy).ceil();
        if !(w.is_finite() && h.is_finite())
            || w > TEXT_RASTER_MAX_SIDE_PX as f32
            || h > TEXT_RASTER_MAX_SIDE_PX as f32
        {
            return None;
        }
        let (w, h) = ((w as u32).max(1), (h as u32).max(1));
        let mut pixmap = AzulPixmap::new(w, h)?;
        pixmap.fill(0, 0, 0, 0);
        if !glyphs.is_empty() {
            // The face, registered in a one-font manager the glyph path
            // resolves it from (`render_text_run_to_pixmap`'s way).
            let font_ref: FontRef = crate::parsed_font_to_font_ref(parsed);
            let hash = crate::font_ref_to_parsed_font(&font_ref).hash;
            let fm: FontManager<FontRef> = FontManager::new(FcFontCache::default()).ok()?;
            fm.insert_font(rust_fontconfig::FontId::new(), font_ref);
            let clip_rect = LogicalRect {
                origin: LogicalPosition { x: 0.0, y: 0.0 },
                size: LogicalSize {
                    width: w as f32,
                    height: h as f32,
                },
            };
            let mut glyph_cache = GlyphCache::new();
            super::super::raster::render_text(
                &glyphs,
                FontHash { font_hash: hash },
                size,
                style.color,
                &mut pixmap,
                &clip_rect,
                None,
                &RendererResources::default(),
                &fm,
                1.0,
                &mut glyph_cache,
                (0.0, 0.0),
                // Transparent target: grayscale coverage, never LCD.
                true,
            );
        }
        Some(TextPixels {
            width: w,
            height: h,
            rgba_premultiplied: pixmap.data.to_vec(),
        })
    }

    thread_local! {
        /// The system fonts, scanned once per thread on first use.
        static SYSTEM_FONTS: core::cell::OnceCell<FcFontCache> = const { core::cell::OnceCell::new() };
    }

    /// `f` with this thread's cache of the system fonts.
    pub fn with_system_fonts<R>(f: impl FnOnce(&FcFontCache) -> R) -> R {
        SYSTEM_FONTS.with(|cell| f(cell.get_or_init(crate::font::loading::build_font_cache)))
    }
}

#[cfg(all(feature = "std", feature = "text_layout", feature = "font_loading"))]
pub use set::{rasterize_text_at, with_system_fonts};

// ==== Into images ====

/// Straight-alpha RGBA8 rows from premultiplied ones.
#[must_use]
pub fn unpremultiply_rgba(premultiplied: &[u8]) -> Vec<u8> {
    let mut out = premultiplied.to_vec();
    for px in out.chunks_exact_mut(4) {
        let a = u32::from(px[3]);
        if a == 0 {
            px.copy_from_slice(&[0, 0, 0, 0]);
        } else if a < 255 {
            for c in &mut px[..3] {
                *c = ((u32::from(*c) * 255 + a / 2) / a).min(255) as u8;
            }
        }
    }
    out
}

/// `text` set in `style` as a straight-alpha RGBA8 image as big as its
/// lines' boxes (the widest line, every line's box), with the fonts of
/// `fc`. `None`: no text, no usable size or no font.
#[cfg(all(feature = "std", feature = "text_layout", feature = "font_loading"))]
#[must_use]
pub fn text_image_with(
    fc: &rust_fontconfig::FcFontCache,
    text: &str,
    style: &TextRasterStyle,
) -> Option<RawImage> {
    let set = rasterize_text_at(fc, text, style, 0.0, 0.0)?;
    let rgba = unpremultiply_rgba(&set.rgba_premultiplied);
    Some(RawImage::create_rgba8(
        set.width,
        set.height,
        rgba.into(),
        false,
    ))
}

/// The pixels of `text` in `style` composited into `image` with the first
/// line's box at (`x`, `y`), with the fonts of `fc`: source-over, into an
/// RGBA8 or BGRA8 image (straight or premultiplied alpha). `false`: an image
/// of another format, or nothing to draw.
#[cfg(all(feature = "std", feature = "text_layout", feature = "font_loading"))]
#[allow(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::cast_possible_wrap
)]
pub fn draw_text_with(
    fc: &rust_fontconfig::FcFontCache,
    image: &mut RawImage,
    text: &str,
    style: &TextRasterStyle,
    x: f32,
    y: f32,
) -> bool {
    let bgr = match image.data_format {
        RawImageFormat::RGBA8 => false,
        RawImageFormat::BGRA8 => true,
        _ => return false,
    };
    if !(x.is_finite() && y.is_finite()) {
        return false;
    }
    let (ox, oy) = (x.floor(), y.floor());
    let Some(set) = rasterize_text_at(fc, text, style, x - ox, y - oy) else {
        return false;
    };
    let (iw, ih) = (image.width as i64, image.height as i64);
    let premultiplied = image.premultiplied_alpha;
    let RawImageData::U8(ref mut pixels) = image.pixels else {
        return false;
    };
    let dst = pixels.as_mut();
    let (ox, oy) = (ox as i64, oy as i64);
    for sy in 0..i64::from(set.height) {
        let ty = oy + sy;
        if ty < 0 || ty >= ih {
            continue;
        }
        for sx in 0..i64::from(set.width) {
            let tx = ox + sx;
            if tx < 0 || tx >= iw {
                continue;
            }
            let si = ((sy * i64::from(set.width) + sx) * 4) as usize;
            let di = ((ty * iw + tx) * 4) as usize;
            if di + 4 > dst.len() || si + 4 > set.rgba_premultiplied.len() {
                continue;
            }
            let s = &set.rgba_premultiplied[si..si + 4];
            if s[3] == 0 {
                continue;
            }
            let (ri, bi) = if bgr { (di + 2, di) } else { (di, di + 2) };
            let idx = [ri, di + 1, bi];
            let sa = f32::from(s[3]) / 255.0;
            let da = f32::from(dst[di + 3]) / 255.0;
            let inv = 1.0 - sa;
            let out_a = sa + da * inv;
            for k in 0..3 {
                let sc = f32::from(s[k]) / 255.0;
                let dc = f32::from(dst[idx[k]]) / 255.0;
                let c = if premultiplied {
                    sc + dc * inv
                } else if out_a > 0.0 {
                    (sc + dc * da * inv) / out_a
                } else {
                    0.0
                };
                dst[idx[k]] = (c * 255.0).round().clamp(0.0, 255.0) as u8;
            }
            dst[di + 3] = (out_a * 255.0).round().clamp(0.0, 255.0) as u8;
        }
    }
    true
}

/// [`text_image_with`] with the system fonts (`RawImage::from_text`).
#[must_use]
pub fn text_image(text: AzString, style: TextRasterStyle) -> OptionRawImage {
    #[cfg(all(feature = "std", feature = "text_layout", feature = "font_loading"))]
    {
        with_system_fonts(|fc| text_image_with(fc, text.as_str(), &style)).into()
    }
    #[cfg(not(all(feature = "std", feature = "text_layout", feature = "font_loading")))]
    {
        let _ = (text, style);
        OptionRawImage::None
    }
}

/// [`draw_text_with`] with the system fonts (`RawImage::draw_text`).
pub fn draw_text(
    image: &mut RawImage,
    text: AzString,
    style: TextRasterStyle,
    x: f32,
    y: f32,
) -> bool {
    #[cfg(all(feature = "std", feature = "text_layout", feature = "font_loading"))]
    {
        with_system_fonts(|fc| draw_text_with(fc, image, text.as_str(), &style, x, y))
    }
    #[cfg(not(all(feature = "std", feature = "text_layout", feature = "font_loading")))]
    {
        let _ = (image, text, style, x, y);
        false
    }
}
