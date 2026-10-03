//! The CPU compositor: the picture of the sequence at a frame.
//!
//! The video tracks are drawn from V1 up, the top one wins; a clip is drawn
//! through its effects (crop, then fitted into the frame, scaled, moved,
//! with its opacity) as a layer of premultiplied pixels laid over what is
//! below. A clip's head transition mixes the clip before it on its track
//! (playing on into its handle) with the clip itself: a cross dissolve
//! mixes the two layers, a dip to black darkens the outgoing one to black
//! and lightens the incoming one from it.
//!
//! Pictures come from a [`FrameSource`]: [`Generated`] makes the app's own
//! patterns; the decode module reads files. The compositor is plain Rust,
//! the same for the program monitor (a small canvas), the export (the
//! sequence's size) and the tests.

use crate::model::{Clip, Effects, Frame, MediaItem, MediaSource, Pattern, Project, TransitionKind};

/// RGBA8 pixels, straight alpha, row after row.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Canvas {
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
}

impl Canvas {
    /// An opaque black picture.
    #[must_use]
    pub fn black(width: u32, height: u32) -> Self {
        let mut rgba = vec![0u8; (width as usize) * (height as usize) * 4];
        for px in rgba.chunks_exact_mut(4) {
            px[3] = 255;
        }
        Self { width, height, rgba }
    }

    /// A transparent picture (a layer before it is drawn).
    #[must_use]
    pub fn clear(width: u32, height: u32) -> Self {
        Self {
            width,
            height,
            rgba: vec![0u8; (width as usize) * (height as usize) * 4],
        }
    }

    /// One colour, opaque.
    #[must_use]
    pub fn filled(width: u32, height: u32, rgb: [u8; 3]) -> Self {
        let mut c = Self::black(width, height);
        for px in c.rgba.chunks_exact_mut(4) {
            px[0] = rgb[0];
            px[1] = rgb[1];
            px[2] = rgb[2];
        }
        c
    }

    fn index(&self, x: u32, y: u32) -> usize {
        ((y as usize) * (self.width as usize) + (x as usize)) * 4
    }
}

/// Where the pictures of media items come from.
pub trait FrameSource {
    /// Media frame `frame` of `media`, no larger than `max_width` x
    /// `max_height` (a source may hand out its own size); `None` when it
    /// cannot be read.
    fn picture(&mut self, media: &MediaItem, frame: Frame, max_width: u32, max_height: u32)
        -> Option<Canvas>;
}

/// The app's own patterns (and nothing for files).
#[derive(Debug, Clone, Copy, Default)]
pub struct Generated;

impl FrameSource for Generated {
    fn picture(
        &mut self,
        media: &MediaItem,
        frame: Frame,
        max_width: u32,
        max_height: u32,
    ) -> Option<Canvas> {
        match &media.source {
            MediaSource::Generated { pattern } => {
                let (w, h) = fit_within(media.width, media.height, max_width, max_height);
                Some(generate(pattern, frame, w, h))
            }
            _ => None,
        }
    }
}

/// `width` x `height` scaled down (never up) to fit `max_width` x
/// `max_height`, the aspect kept, at least 1 x 1 (a frame buffer is never
/// empty): azul's one fit rule (`RawImage::fit_within`, the rule
/// `RawImage::thumbnail` uses) - VideoCut's own f32 copy is gone
/// (DEDUP_OFFICE D15).
#[must_use]
pub fn fit_within(width: u32, height: u32, max_width: u32, max_height: u32) -> (u32, u32) {
    let size = azul::image::RawImage::fit_within(width, height, max_width, max_height);
    (size.width.max(1), size.height.max(1))
}

/// Frame `frame` of `pattern` at `width` x `height`.
#[must_use]
#[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss, clippy::cast_possible_wrap)]
pub fn generate(pattern: &Pattern, frame: Frame, width: u32, height: u32) -> Canvas {
    let (w, h) = (width.max(1), height.max(1));
    match pattern {
        Pattern::Matte { rgb } => Canvas::filled(w, h, *rgb),
        Pattern::Bars => {
            const BARS: [[u8; 3]; 7] = [
                [192, 192, 192],
                [192, 192, 0],
                [0, 192, 192],
                [0, 192, 0],
                [192, 0, 192],
                [192, 0, 0],
                [0, 0, 192],
            ];
            let mut c = Canvas::black(w, h);
            let box_size = (h / 6).max(1);
            let box_x = ((frame.max(0) as u64 * 4) % u64::from(w)) as u32;
            let box_y = h - h / 8 - box_size / 2;
            for y in 0..h {
                for x in 0..w {
                    let i = c.index(x, y);
                    let rgb = if y >= h * 3 / 4 {
                        let inside = x >= box_x
                            && x < box_x + box_size
                            && y + box_size / 2 >= box_y
                            && y < box_y + box_size / 2;
                        if inside {
                            [235, 235, 235]
                        } else {
                            [16, 16, 16]
                        }
                    } else {
                        BARS[((x as usize) * 7 / (w as usize)).min(6)]
                    };
                    c.rgba[i..i + 3].copy_from_slice(&rgb);
                }
            }
            c
        }
        Pattern::Sweep { rgb } => {
            let mut c = Canvas::filled(w, h, *rgb);
            let bar = (w / 12).max(2);
            let x0 = ((frame.max(0) as u64 * u64::from(w) / 50) % u64::from(w)) as u32;
            let light = rgb.map(|v| v.saturating_add(70));
            for y in 0..h {
                for x in x0..(x0 + bar).min(w) {
                    let i = c.index(x, y);
                    c.rgba[i..i + 3].copy_from_slice(&light);
                }
            }
            c
        }
    }
}

/// `c` scaled down to fit `max_width` x `max_height`, the aspect kept, by
/// azul's thumbnail scaler (`RawImage::thumbnail`); a picture that fits
/// already is kept as it is.
#[must_use]
pub fn fit_to(c: &Canvas, max_width: u32, max_height: u32) -> Canvas {
    use azul::image::{RawImage, RawImageData};

    let (w, h) = fit_within(c.width, c.height, max_width, max_height);
    if (w, h) == (c.width, c.height) || c.width == 0 || c.height == 0 {
        return c.clone();
    }
    let source = RawImage::create_rgba8(c.width, c.height, c.rgba.clone(), true);
    match source.thumbnail(max_width, max_height).into_option() {
        Some(scaled) => {
            let (sw, sh) = (scaled.width as u32, scaled.height as u32);
            match scaled.pixels {
                RawImageData::U8(bytes) => Canvas {
                    width: sw,
                    height: sh,
                    rgba: bytes.as_ref().to_vec(),
                },
                _ => scale_to(c, w, h),
            }
        }
        None => scale_to(c, w, h),
    }
}

/// `c` resized to `width` x `height` by azul's scaler
/// (`RawImage::resized`: area-averaging down, bilinear up - it was a
/// private nearest sampler, DEDUP_OFFICE D15).
#[must_use]
pub fn scale_to(c: &Canvas, width: u32, height: u32) -> Canvas {
    use azul::image::{RawImage, RawImageData};

    let (w, h) = (width.max(1), height.max(1));
    if c.width == 0 || c.height == 0 {
        return Canvas::black(w, h);
    }
    let source = RawImage::create_rgba8(c.width, c.height, c.rgba.clone(), true);
    match source.resized(w, h).into_option().map(|scaled| scaled.pixels) {
        Some(RawImageData::U8(bytes)) => Canvas {
            width: w,
            height: h,
            rgba: bytes.as_ref().to_vec(),
        },
        _ => Canvas::black(w, h),
    }
}

/// `src` drawn through `effects` with `opacity` on a transparent
/// `width` x `height` layer, PREMULTIPLIED. `pos_scale` turns the effects'
/// sequence pixels into the layer's pixels.
#[allow(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::cast_precision_loss
)]
fn render_layer(src: &Canvas, effects: &Effects, opacity: f32, width: u32, height: u32, pos_scale: f32) -> Canvas {
    let mut layer = Canvas::clear(width, height);
    if src.width == 0 || src.height == 0 || opacity <= 0.0 {
        return layer;
    }
    let (sw, sh) = (src.width as f32, src.height as f32);
    let fit = (width as f32 / sw).min(height as f32 / sh);
    let s = fit * effects.scale.max(0.0001);
    let cx = width as f32 / 2.0 + effects.x * pos_scale;
    let cy = height as f32 / 2.0 + effects.y * pos_scale;
    let x0 = effects.crop_left.clamp(0.0, 1.0) * sw;
    let x1 = sw * (1.0 - effects.crop_right.clamp(0.0, 1.0));
    let y0 = effects.crop_top.clamp(0.0, 1.0) * sh;
    let y1 = sh * (1.0 - effects.crop_bottom.clamp(0.0, 1.0));
    if x1 <= x0 || y1 <= y0 {
        return layer;
    }
    let left = (cx + (x0 - sw / 2.0) * s).floor().max(0.0) as u32;
    let right = ((cx + (x1 - sw / 2.0) * s).ceil().max(0.0) as u32).min(width);
    let top = (cy + (y0 - sh / 2.0) * s).floor().max(0.0) as u32;
    let bottom = ((cy + (y1 - sh / 2.0) * s).ceil().max(0.0) as u32).min(height);
    let opacity = opacity.clamp(0.0, 1.0);
    for py in top..bottom {
        let sy = (py as f32 + 0.5 - cy) / s + sh / 2.0;
        if sy < y0 || sy >= y1 {
            continue;
        }
        let syi = (sy as u32).min(src.height - 1);
        for px in left..right {
            let sx = (px as f32 + 0.5 - cx) / s + sw / 2.0;
            if sx < x0 || sx >= x1 {
                continue;
            }
            let sxi = (sx as u32).min(src.width - 1);
            let j = src.index(sxi, syi);
            let a = f32::from(src.rgba[j + 3]) / 255.0 * opacity;
            let i = layer.index(px, py);
            for k in 0..3 {
                layer.rgba[i + k] = (f32::from(src.rgba[j + k]) * a).round() as u8;
            }
            layer.rgba[i + 3] = (a * 255.0).round() as u8;
        }
    }
    layer
}

/// `layer` (premultiplied) laid over `dst` (opaque).
#[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
fn lay_over(dst: &mut Canvas, layer: &Canvas) {
    for (d, s) in dst.rgba.chunks_exact_mut(4).zip(layer.rgba.chunks_exact(4)) {
        let a = u32::from(s[3]);
        if a == 0 {
            continue;
        }
        for k in 0..3 {
            let v = u32::from(s[k]) + (u32::from(d[k]) * (255 - a) + 127) / 255;
            d[k] = v.min(255) as u8;
        }
        d[3] = 255;
    }
}

/// `a` and `b` (premultiplied) mixed: `a` * (1 - `m`) + `b` * `m`.
#[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
fn mix(a: &Canvas, b: &Canvas, m: f32) -> Canvas {
    let m = m.clamp(0.0, 1.0);
    let mut out = Canvas::clear(a.width, a.height);
    for ((o, x), y) in out
        .rgba
        .iter_mut()
        .zip(a.rgba.iter())
        .zip(b.rgba.iter())
    {
        *o = (f32::from(*x) * (1.0 - m) + f32::from(*y) * m).round() as u8;
    }
    out
}

/// `layer`'s colour times `k` (toward black), its coverage unchanged.
#[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
fn darken(layer: &mut Canvas, k: f32) {
    let k = k.clamp(0.0, 1.0);
    for px in layer.rgba.chunks_exact_mut(4) {
        for v in px.iter_mut().take(3) {
            *v = (f32::from(*v) * k).round() as u8;
        }
    }
}

/// `clip`'s picture at timeline frame `f` (its media frame, held at the
/// media's last frame past its end) as a layer.
fn clip_layer(
    project: &Project,
    clip: &Clip,
    f: Frame,
    width: u32,
    height: u32,
    pos_scale: f32,
    source: &mut dyn FrameSource,
) -> Option<Canvas> {
    let media = project.media(clip.media)?;
    let frame = clip.source_frame(f).clamp(0, (media.frames - 1).max(0));
    let picture = source.picture(media, frame, width, height)?;
    Some(render_layer(
        &picture,
        &clip.effects,
        clip.effects.opacity,
        width,
        height,
        pos_scale,
    ))
}

/// The sequence's picture at frame `f`, `width` x `height` (the sequence's
/// own size for an export, smaller for a monitor), opaque.
#[must_use]
#[allow(clippy::cast_precision_loss)]
pub fn compose(project: &Project, f: Frame, width: u32, height: u32, source: &mut dyn FrameSource) -> Canvas {
    let seq = &project.sequence;
    let mut canvas = Canvas::black(width, height);
    let pos_scale = width as f32 / seq.width.max(1) as f32;
    for t in seq.video_tracks() {
        let track = &seq.tracks[t];
        if track.hidden {
            continue;
        }
        let Some(ci) = track.clips.iter().position(|c| c.contains(f)) else {
            continue;
        };
        let clip = &track.clips[ci];
        if !clip.enabled {
            continue;
        }
        let incoming = clip_layer(project, clip, f, width, height, pos_scale, source);
        let transition = clip
            .transition
            .filter(|tr| tr.frames > 0 && f < clip.start + tr.frames);
        let Some(tr) = transition else {
            if let Some(layer) = incoming {
                lay_over(&mut canvas, &layer);
            }
            continue;
        };
        let m = (f - clip.start) as f32 / tr.frames as f32;
        let outgoing = (ci > 0)
            .then(|| &track.clips[ci - 1])
            .filter(|prev| prev.end() == clip.start && prev.enabled)
            .and_then(|prev| clip_layer(project, prev, f, width, height, pos_scale, source));
        let empty = || Canvas::clear(width, height);
        let layer = match tr.kind {
            TransitionKind::CrossDissolve => mix(
                &outgoing.unwrap_or_else(empty),
                &incoming.unwrap_or_else(empty),
                m,
            ),
            TransitionKind::DipToBlack => {
                if m < 0.5 {
                    let mut out = outgoing.unwrap_or_else(|| Canvas::black(width, height));
                    darken(&mut out, 1.0 - 2.0 * m);
                    out
                } else {
                    let mut inc = incoming.unwrap_or_else(empty);
                    darken(&mut inc, 2.0 * m - 1.0);
                    // The black under a dip covers what lies below.
                    for px in inc.rgba.chunks_exact_mut(4) {
                        px[3] = 255;
                    }
                    inc
                }
            }
        };
        lay_over(&mut canvas, &layer);
    }
    canvas
}

#[cfg(test)]
#[path = "render_tests.rs"]
mod tests;
