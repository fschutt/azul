//! Text shadows drawn once and kept as pictures, for the GPU backend.
//!
//! `WebRender` draws a blurred text shadow as a picture of its own: an
//! offscreen surface, the glyphs drawn into it and two blur passes, on every
//! frame. A map draws a halo behind each of its labels and, on each frame of a
//! pan, every label again where it moved: a hundred and more blurred surfaces
//! per frame. Here the CPU renderer's text-shadow code draws a halo once, in
//! white, and it is kept by what its pixels depend on - the glyphs relative to
//! each other, their fonts and clips, the blur and the scale - and not by
//! where the text stands or by the shadow's colour. The compositor uploads it
//! as an image and paints it tinted with the shadow's colour, one textured
//! quad: a label that moves keeps its picture, and the GPU's halo is the CPU
//! renderer's halo.

use std::{
    cell::RefCell,
    collections::hash_map::DefaultHasher,
    hash::{Hash, Hasher},
};

use azul_core::{
    geom::{LogicalPosition, LogicalRect},
    resources::{ImageRef, RawImage, RawImageData, RawImageFormat, RendererResources},
    ui_solver::GlyphInstance,
};
use azul_css::props::basic::{pixel::DEFAULT_FONT_SIZE, ColorU, FontRef};

use super::raster::{blurred_glyphs, shadow_blur_radius, text_run_clip, BufferedRun, PixelBox};
use crate::{
    glyph_cache::GlyphCache,
    image_memo::ImageMemo,
    solver3::display_list::DisplayListItem,
    text3::cache::{FontHash, FontManager},
};

/// At most this many halos are kept ...
const MAX_HALOS: usize = 1024;
/// ... holding at most this many bytes of pixels (RGBA: four per pixel).
const MAX_HALO_BYTES: usize = 32 * 1024 * 1024;
/// The most pixels one halo may cover; a larger shadow (a page of shadowed
/// text) is left to the renderer's own blur.
const MAX_HALO_PIXELS: u64 = 1 << 21;

/// A text shadow drawn as a picture, where its `PushTextShadow` stands.
#[derive(Debug, Clone, PartialEq)]
pub struct TextHalo {
    /// The index of the shadow's `PushTextShadow` in the display list.
    pub push: usize,
    /// The halo in premultiplied white; tinted with `color` it is the shadow.
    pub image: ImageRef,
    /// The shadow's colour.
    pub color: ColorU,
    /// The picture's top-left corner in device px of the window (before any
    /// scrolling), the shadow's offset included.
    pub origin: (f32, f32),
    /// Its width in device px; the picture is drawn one texel per pixel.
    pub width: u32,
    /// Its height in device px.
    pub height: u32,
}

/// The text shadows of `items` that are drawn as pictures at `dpi`: blurred
/// shadows over nothing but text (decorations or anything else under a
/// shadow leave it to the renderer's own). With `fonts` a halo that is not
/// kept yet is drawn and kept; without, only kept ones are found. The GPU
/// backend draws them while it collects a frame's images, before it builds
/// the frame's display list, so the second call finds every one it can use.
#[must_use]
pub fn text_halos(
    items: &[DisplayListItem],
    dpi: f32,
    fonts: Option<(&RendererResources, &FontManager<FontRef>)>,
) -> Vec<TextHalo> {
    // NaN too: no positive scale, no halo.
    if dpi.partial_cmp(&0.0) != Some(core::cmp::Ordering::Greater) {
        return Vec::new();
    }
    scopes(items)
        .into_iter()
        .filter_map(|scope| {
            let (key, anchor) = scope.key(dpi);
            let kept = HALOS.with(|halos| halos.borrow_mut().get(&key));
            let kept = match (kept, fonts) {
                (Some(kept), _) => kept,
                (None, Some((renderer_resources, font_manager))) => {
                    let drawn = scope.draw(anchor, dpi, renderer_resources, font_manager);
                    let bytes = drawn.as_ref().map_or(0, |d| d.width as usize * d.height as usize * 4);
                    HALOS.with(|halos| halos.borrow_mut().insert(key, drawn.clone(), bytes));
                    drawn
                }
                (None, None) => None,
            }?;
            Some(TextHalo {
                push: scope.push,
                image: kept.image,
                color: scope.color,
                origin: (
                    (anchor.0 + kept.offset.0) as f32 + scope.offset.0 * dpi,
                    (anchor.1 + kept.offset.1) as f32 + scope.offset.1 * dpi,
                ),
                width: kept.width,
                height: kept.height,
            })
        })
        .collect()
}

/// A halo as it is kept: the picture and where it lies from its scope's
/// anchor, in device px.
#[derive(Debug, Clone)]
struct Kept {
    /// The halo in premultiplied white.
    image: ImageRef,
    /// The picture's top-left corner from the scope's anchor.
    offset: (i32, i32),
    /// The picture's size in device px.
    width: u32,
    height: u32,
}

thread_local! {
    /// The halos drawn so far, by [`Scope::key`]. `None` remembers a shadow
    /// that cannot be a picture (too large), so it is not tried every frame.
    static HALOS: RefCell<ImageMemo<u64, Option<Kept>>> =
        RefCell::new(ImageMemo::new(MAX_HALOS, MAX_HALO_BYTES));
    /// The glyph rasteriser's cache for the halos' glyphs.
    static GLYPHS: RefCell<GlyphCache> = RefCell::new(GlyphCache::new());
}

/// One glyph run under a text shadow.
struct Run<'a> {
    /// Its glyphs, at their pen positions in CSS px of the window.
    glyphs: &'a [GlyphInstance],
    /// The font they are drawn in, and its size.
    font_hash: FontHash,
    font_size_px: f32,
    /// The run's clip rect, CSS px of the window.
    clip_rect: LogicalRect,
    /// Where its glyphs can paint: their ink, padded, inside the clip rect.
    ink: LogicalRect,
}

/// A blurred text shadow over nothing but text.
struct Scope<'a> {
    /// The index of its `PushTextShadow`.
    push: usize,
    /// The shadow's colour; the halo is drawn without it.
    color: ColorU,
    /// The shadow's offset, CSS px.
    offset: (f32, f32),
    /// Its blur radius, CSS px.
    blur: f32,
    /// The text under the shadow, in list order; never empty.
    runs: Vec<Run<'a>>,
}

/// A shadow length in CSS px, resolved as the CPU renderer resolves it.
fn shadow_px(value: &azul_css::props::basic::pixel::PixelValue) -> f32 {
    value.to_pixels_internal(0.0, DEFAULT_FONT_SIZE, DEFAULT_FONT_SIZE)
}

/// The shadows of `items` a halo can stand for: blurred, over text alone. An
/// unblurred shadow is not one: `WebRender` draws it as a plain copy of the
/// text, without a surface.
fn scopes(items: &[DisplayListItem]) -> Vec<Scope<'_>> {
    let mut out = Vec::new();
    for (push, item) in items.iter().enumerate() {
        let DisplayListItem::PushTextShadow { shadow } = item else {
            continue;
        };
        let blur = shadow_px(&shadow.blur_radius.inner).max(0.0);
        if blur <= 0.0 || shadow.color.a == 0 {
            continue;
        }
        let mut runs = Vec::new();
        let mut closed = false;
        for inner in &items[push + 1..] {
            match inner {
                DisplayListItem::Text {
                    glyphs,
                    font_hash,
                    font_size_px,
                    clip_rect,
                    ..
                } => {
                    if let (false, Some(ink)) = (glyphs.is_empty(), inner.visual_bounds()) {
                        runs.push(Run {
                            glyphs,
                            font_hash: *font_hash,
                            font_size_px: *font_size_px,
                            clip_rect: *clip_rect.inner(),
                            ink,
                        });
                    }
                }
                // They paint nothing on either backend: the text's layout
                // (kept for PDF and accessibility) and its hit area.
                DisplayListItem::TextLayout { .. } | DisplayListItem::HitTestArea { .. } => {}
                DisplayListItem::PopTextShadow => {
                    closed = true;
                    break;
                }
                _ => break,
            }
        }
        if closed && !runs.is_empty() {
            out.push(Scope {
                push,
                color: shadow.color,
                offset: (
                    shadow_px(&shadow.offset_x.inner),
                    shadow_px(&shadow.offset_y.inner),
                ),
                blur,
                runs,
            });
        }
    }
    out
}

impl Scope<'_> {
    /// The first glyph's pen position: what everything is measured from.
    fn first(&self) -> LogicalPosition {
        self.runs[0].glyphs[0].point
    }

    /// What the halo's pixels depend on, hashed, and the anchor it is drawn
    /// from: the first glyph's pen, on the nearest device pixel. Positions
    /// count relative to the first glyph, so text that moves keeps its key.
    #[allow(clippy::cast_possible_truncation)] // device px and 1/64 px steps
    fn key(&self, dpi: f32) -> (u64, (i32, i32)) {
        let first = self.first();
        let anchor = ((first.x * dpi).round() as i32, (first.y * dpi).round() as i32);
        let step = |v: f32| (v * 64.0).round() as i64;
        let mut h = DefaultHasher::new();
        self.blur.to_bits().hash(&mut h);
        dpi.to_bits().hash(&mut h);
        for run in &self.runs {
            run.font_hash.font_hash.hash(&mut h);
            run.font_size_px.to_bits().hash(&mut h);
            for rect in [run.clip_rect, run.ink] {
                step(rect.origin.x - first.x).hash(&mut h);
                step(rect.origin.y - first.y).hash(&mut h);
                step(rect.size.width).hash(&mut h);
                step(rect.size.height).hash(&mut h);
            }
            run.glyphs.len().hash(&mut h);
            for glyph in run.glyphs {
                glyph.index.hash(&mut h);
                step(glyph.point.x - first.x).hash(&mut h);
                step(glyph.point.y - first.y).hash(&mut h);
            }
        }
        (h.finish(), anchor)
    }

    /// The halo drawn with the first glyph on `anchor`: the runs moved by
    /// less than a device pixel, so the picture fits the text wherever it
    /// stands. `None` when nothing of it paints or it is too large.
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss, clippy::cast_precision_loss)]
    fn draw(
        &self,
        anchor: (i32, i32),
        dpi: f32,
        renderer_resources: &RendererResources,
        font_manager: &FontManager<FontRef>,
    ) -> Option<Kept> {
        let first = self.first();
        let shift = (anchor.0 as f32 / dpi - first.x, anchor.1 as f32 / dpi - first.y);
        let moved = |r: LogicalRect| {
            LogicalRect::new(
                LogicalPosition::new(r.origin.x + shift.0, r.origin.y + shift.1),
                r.size,
            )
        };
        let glyphs: Vec<Vec<GlyphInstance>> = self
            .runs
            .iter()
            .map(|run| {
                run.glyphs
                    .iter()
                    .map(|g| {
                        let mut g = *g;
                        g.point.x += shift.0;
                        g.point.y += shift.1;
                        g
                    })
                    .collect()
            })
            .collect();
        let mut runs = Vec::with_capacity(self.runs.len());
        let mut reach: Option<(f32, f32, f32, f32)> = None;
        for (run, glyphs) in self.runs.iter().zip(&glyphs) {
            let clip_rect = moved(run.clip_rect);
            let Some(paints) = text_run_clip(&moved(run.ink), None, dpi)
                .and_then(|ink| text_run_clip(&clip_rect, Some(ink), dpi))
            else {
                continue;
            };
            let (x0, y0, x1, y1) = (paints.x, paints.y, paints.x + paints.width, paints.y + paints.height);
            reach = Some(match reach {
                Some((a, b, c, d)) => (a.min(x0), b.min(y0), c.max(x1), d.max(y1)),
                None => (x0, y0, x1, y1),
            });
            runs.push(BufferedRun {
                glyphs,
                font_hash: run.font_hash,
                font_size_px: run.font_size_px,
                clip_rect,
                clip: paints,
            });
        }
        let (x0, y0, x1, y1) = reach?;
        let radius = shadow_blur_radius(self.blur, dpi);
        let r = radius as i32;
        let buffer = PixelBox {
            x: x0.floor() as i32 - r,
            y: y0.floor() as i32 - r,
            width: (x1.ceil() as i32 - x0.floor() as i32 + 2 * r).max(0) as u32,
            height: (y1.ceil() as i32 - y0.floor() as i32 + 2 * r).max(0) as u32,
        };
        if buffer.width == 0
            || buffer.height == 0
            || u64::from(buffer.width) * u64::from(buffer.height) > MAX_HALO_PIXELS
        {
            return None;
        }
        let pixels = GLYPHS.with(|glyph_cache| {
            blurred_glyphs(
                &runs,
                ColorU::WHITE,
                buffer,
                radius,
                renderer_resources,
                font_manager,
                dpi,
                &mut glyph_cache.borrow_mut(),
                (0.0, 0.0),
            )
        })?;
        let image = ImageRef::new_rawimage(RawImage {
            pixels: RawImageData::U8(pixels.data().to_vec().into()),
            width: buffer.width as usize,
            height: buffer.height as usize,
            premultiplied_alpha: true,
            data_format: RawImageFormat::RGBA8,
            tag: Vec::new().into(),
        })?;
        Some(Kept {
            image,
            offset: (buffer.x - anchor.0, buffer.y - anchor.1),
            width: buffer.width,
            height: buffer.height,
        })
    }
}
