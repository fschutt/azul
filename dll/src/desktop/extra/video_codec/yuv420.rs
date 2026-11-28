//! YUV 4:2:0 pictures in a codec's buffers, pure Rust over byte slices (`yuv420_test.rs`).
//!
//! A hardware or software codec reads and writes a picture in a layout of its own: the Y plane
//! with a row stride that is at least the width, the chroma planes after `slice_height` rows,
//! either as one plane of interleaved Cb,Cr pairs (NV12, "semi-planar") or as two planes (I420,
//! "planar"), and the visible picture a window inside it (cropping). This module says where every
//! sample is ([`Planes`], [`Picture`]), builds that from a codec's output format
//! ([`plan_output`]), and moves pictures between a codec's layout and the engine's frames:
//! tightly packed NV12 out of a decoder's buffer ([`Picture::to_nv12`]), NV12 or RGB into an
//! encoder's buffer ([`write_picture`]).

use super::h264;

/// `COLOR_FormatYUV420Planar`: I420, U plane then V plane.
pub(super) const COLOR_FORMAT_YUV420_PLANAR: i32 = 19;
/// `COLOR_FormatYUV420PackedPlanar`.
pub(super) const COLOR_FORMAT_YUV420_PACKED_PLANAR: i32 = 18;
/// `COLOR_FormatYUV420SemiPlanar`: NV12, one plane of interleaved Cb,Cr.
pub(super) const COLOR_FORMAT_YUV420_SEMI_PLANAR: i32 = 21;
/// `COLOR_FormatYUV420PackedSemiPlanar`.
pub(super) const COLOR_FORMAT_YUV420_PACKED_SEMI_PLANAR: i32 = 20;
/// `COLOR_FormatYUV420Flexible`: the layout is the codec's, described by its `image-data`.
pub(super) const COLOR_FORMAT_YUV420_FLEXIBLE: i32 = 0x7F42_0888;
/// `COLOR_QCOM_FormatYUV420SemiPlanar`: NV12.
pub(super) const COLOR_QCOM_YUV420_SEMI_PLANAR: i32 = 0x7FA3_0C00;
/// `COLOR_QCOM_FormatYUV420SemiPlanar32m`: NV12 with its rows and slices aligned to 32.
pub(super) const COLOR_QCOM_YUV420_SEMI_PLANAR_32M: i32 = 0x7FA3_0C04;

/// The largest picture side the backend takes, in pixels.
pub(super) const MAX_SIDE: usize = 16384;

/// The colour matrix of an RGB <-> YCbCr conversion.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Matrix {
    Bt601,
    Bt709,
}

/// One plane of a 4:2:0 picture inside a buffer: where it starts and the distance, in bytes,
/// between two rows and between two samples of a row.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct Plane {
    pub offset: usize,
    pub row_inc: usize,
    pub col_inc: usize,
}

/// The three planes of a 4:2:0 picture: the chroma planes have half the rows and half the samples
/// of a row of the Y plane.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct Planes {
    pub y: Plane,
    pub u: Plane,
    pub v: Plane,
}

impl Planes {
    /// NV12: a Y plane of `slice_height` rows of `stride` bytes, then one plane of Cb,Cr pairs
    /// with rows of `stride` bytes.
    pub(super) const fn semi_planar(stride: usize, slice_height: usize) -> Self {
        let chroma = stride.saturating_mul(slice_height);
        Planes {
            y: Plane {
                offset: 0,
                row_inc: stride,
                col_inc: 1,
            },
            u: Plane {
                offset: chroma,
                row_inc: stride,
                col_inc: 2,
            },
            v: Plane {
                offset: chroma.saturating_add(1),
                row_inc: stride,
                col_inc: 2,
            },
        }
    }

    /// I420: a Y plane of `slice_height` rows of `stride` bytes, then the U plane and the V plane
    /// with rows of half a stride.
    pub(super) const fn planar(stride: usize, slice_height: usize) -> Self {
        let chroma = stride.saturating_mul(slice_height);
        let chroma_stride = stride.div_ceil(2);
        let chroma_rows = slice_height.div_ceil(2);
        Planes {
            y: Plane {
                offset: 0,
                row_inc: stride,
                col_inc: 1,
            },
            u: Plane {
                offset: chroma,
                row_inc: chroma_stride,
                col_inc: 1,
            },
            v: Plane {
                offset: chroma.saturating_add(chroma_stride.saturating_mul(chroma_rows)),
                row_inc: chroma_stride,
                col_inc: 1,
            },
        }
    }

    /// Whether the chroma plane is one plane of interleaved Cb,Cr pairs, Cb first.
    const fn is_interleaved(&self) -> bool {
        self.u.col_inc == 2
            && self.v.col_inc == 2
            && self.v.offset == self.u.offset + 1
            && self.u.row_inc == self.v.row_inc
    }
}

/// A picture in a buffer: its planes and the window of them that is visible.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct Picture {
    pub planes: Planes,
    /// The visible width and height in pixels.
    pub width: usize,
    pub height: usize,
    /// Where the visible window begins in the Y plane.
    pub crop_left: usize,
    pub crop_top: usize,
}

/// One past the last byte of a `cols x rows` window of `plane` that begins at (`left`, `top`):
/// `None` when that overflows.
fn window_end(plane: &Plane, left: usize, top: usize, cols: usize, rows: usize) -> Option<usize> {
    if cols == 0 || rows == 0 {
        return Some(0);
    }
    plane
        .offset
        .checked_add(top.checked_add(rows - 1)?.checked_mul(plane.row_inc)?)?
        .checked_add(left.checked_add(cols - 1)?.checked_mul(plane.col_inc)?)?
        .checked_add(1)
}

impl Picture {
    /// A `width x height` picture with no cropping.
    pub(super) const fn new(planes: Planes, width: usize, height: usize) -> Self {
        Picture {
            planes,
            width,
            height,
            crop_left: 0,
            crop_top: 0,
        }
    }

    /// The columns and rows of one chroma plane.
    const fn chroma_size(&self) -> (usize, usize) {
        (self.width.div_ceil(2), self.height.div_ceil(2))
    }

    /// The number of bytes a buffer needs for the picture to be readable: one past the last
    /// sample of any plane. `None` when that overflows.
    pub(super) fn required_len(&self) -> Option<usize> {
        let (cw, ch) = self.chroma_size();
        let (cl, ct) = (self.crop_left / 2, self.crop_top / 2);
        let y = window_end(
            &self.planes.y,
            self.crop_left,
            self.crop_top,
            self.width,
            self.height,
        )?;
        let u = window_end(&self.planes.u, cl, ct, cw, ch)?;
        let v = window_end(&self.planes.v, cl, ct, cw, ch)?;
        Some(y.max(u).max(v))
    }

    /// The bytes of a tightly packed NV12 image of this picture's size.
    pub(super) fn nv12_len(&self) -> Option<usize> {
        let (cw, ch) = self.chroma_size();
        self.width
            .checked_mul(self.height)?
            .checked_add(cw.checked_mul(ch)?.checked_mul(2)?)
    }

    /// The visible picture of `src` as tightly packed NV12 (the Y plane, then the Cb,Cr pairs).
    /// `None` when `src` is too short for the picture or the picture is empty or over
    /// [`MAX_SIDE`].
    pub(super) fn to_nv12(&self, src: &[u8]) -> Option<Vec<u8>> {
        if self.width == 0 || self.height == 0 || self.width > MAX_SIDE || self.height > MAX_SIDE {
            return None;
        }
        if src.len() < self.required_len()? {
            return None;
        }
        let (w, h) = (self.width, self.height);
        let (cw, ch) = self.chroma_size();
        let mut out = vec![0u8; self.nv12_len()?];
        let (y_plane, rest) = out.split_at_mut(w * h);
        let y = &self.planes.y;
        for (row, dst) in y_plane.chunks_exact_mut(w).enumerate() {
            let start = y.offset + (self.crop_top + row) * y.row_inc + self.crop_left * y.col_inc;
            if y.col_inc == 1 {
                dst.copy_from_slice(&src[start..start + w]);
            } else {
                for (x, d) in dst.iter_mut().enumerate() {
                    *d = src[start + x * y.col_inc];
                }
            }
        }
        let (u, v) = (&self.planes.u, &self.planes.v);
        let (cl, ct) = (self.crop_left / 2, self.crop_top / 2);
        for (row, dst) in rest.chunks_exact_mut(cw * 2).enumerate().take(ch) {
            let u_start = u.offset + (ct + row) * u.row_inc + cl * u.col_inc;
            if self.planes.is_interleaved() {
                dst.copy_from_slice(&src[u_start..u_start + cw * 2]);
            } else {
                let v_start = v.offset + (ct + row) * v.row_inc + cl * v.col_inc;
                for (x, pair) in dst.chunks_exact_mut(2).enumerate() {
                    pair[0] = src[u_start + x * u.col_inc];
                    pair[1] = src[v_start + x * v.col_inc];
                }
            }
        }
        Some(out)
    }
}

// ---------------------------------------------------------------------------
// A decoder's output format
// ---------------------------------------------------------------------------

/// The integers of a decoder's output `AMediaFormat` that say how its buffers are laid out, and
/// its `image-data`, read as they are.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub(super) struct RawOutputFormat {
    pub color_format: Option<i32>,
    pub width: Option<i32>,
    pub height: Option<i32>,
    pub stride: Option<i32>,
    pub slice_height: Option<i32>,
    pub crop_left: Option<i32>,
    pub crop_top: Option<i32>,
    /// Inclusive, as MediaFormat has it.
    pub crop_right: Option<i32>,
    /// Inclusive.
    pub crop_bottom: Option<i32>,
    /// The `MediaImage` / `MediaImage2` blob of a flexible format: where its planes are.
    pub image_data: Option<Vec<u8>>,
    pub color_standard: Option<i32>,
    pub color_range: Option<i32>,
}

/// A positive `i32` as a `usize`.
fn positive(value: Option<i32>) -> Option<usize> {
    value.and_then(|v| usize::try_from(v).ok()).filter(|&v| v > 0)
}

/// A non-negative `i32` as a `usize`.
fn non_negative(value: Option<i32>) -> Option<usize> {
    value.and_then(|v| usize::try_from(v).ok())
}

/// The planes `MediaImage` / `MediaImage2` describe (`image-data` of a flexible output): the
/// offsets are those of the visible window already. `None` for a blob that is not an 8-bit
/// 4:2:0 YUV image with three planes.
fn parse_media_image(blob: &[u8]) -> Option<(Planes, usize, usize)> {
    let field = |at: usize| -> Option<u32> {
        let bytes = blob.get(at..at + 4)?;
        Some(u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]))
    };
    // MediaImage2 has the allocated bit depth after the bit depth, MediaImage has not.
    let header = match blob.len() {
        84 => 24,
        80 => 20,
        _ => return None,
    };
    if field(0)? != 1 || field(4)? != 3 || field(16)? != 8 {
        return None;
    }
    let (width, height) = (field(8)? as usize, field(12)? as usize);
    let mut planes = [Plane {
        offset: 0,
        row_inc: 0,
        col_inc: 0,
    }; 3];
    for (index, plane) in planes.iter_mut().enumerate() {
        let base = header + index * 20;
        let (horizontal, vertical) = (field(base + 12)?, field(base + 16)?);
        let expected = if index == 0 { 1 } else { 2 };
        if horizontal != expected || vertical != expected {
            return None;
        }
        let (col_inc, row_inc) = (field(base + 4)? as i32, field(base + 8)? as i32);
        if col_inc < 1 || row_inc < 1 {
            return None;
        }
        *plane = Plane {
            offset: field(base)? as usize,
            row_inc: row_inc as usize,
            col_inc: col_inc as usize,
        };
    }
    if width == 0 || height == 0 || width > MAX_SIDE || height > MAX_SIDE {
        return None;
    }
    Some((
        Planes {
            y: planes[0],
            u: planes[1],
            v: planes[2],
        },
        width,
        height,
    ))
}

/// Where the visible picture is in the buffers of a decoder that reports `raw` as its output
/// format. `Err` says in words why the layout cannot be told (an unknown colour format with no
/// `image-data`, a size that is not a picture).
///
/// `visible_hint` is the picture size the stream's SPS says (cropping applied). A format that
/// gives no crop window (Android before 9 cannot read the rect) names the coded size, which an
/// OMX decoder pads to the macroblock (1088 rows for a 1080p stream): the hint is the window
/// then, from the top left corner, when it fits in what the format names.
pub(super) fn plan_output(
    raw: &RawOutputFormat,
    visible_hint: Option<(usize, usize)>,
) -> Result<Picture, String> {
    let crop = (
        non_negative(raw.crop_left),
        non_negative(raw.crop_top),
        non_negative(raw.crop_right),
        non_negative(raw.crop_bottom),
    );
    let (crop_left, crop_top, width, height) = match crop {
        (Some(l), Some(t), Some(r), Some(b)) if r >= l && b >= t => (l, t, r - l + 1, b - t + 1),
        _ => match (positive(raw.width), positive(raw.height)) {
            (Some(w), Some(h)) => match visible_hint {
                Some((hw, hh)) if hw > 0 && hh > 0 && hw <= w && hh <= h => (0, 0, hw, hh),
                _ => (0, 0, w, h),
            },
            _ => return Err(String::from("the output format names no picture size")),
        },
    };
    if width > MAX_SIDE || height > MAX_SIDE {
        return Err(format!("a {width}x{height} picture is over the {MAX_SIDE} pixel cap"));
    }
    let color_format = raw.color_format.unwrap_or(COLOR_FORMAT_YUV420_FLEXIBLE);
    // The buffer's own size: the keys say it, else the visible window ends at its edge.
    let stride = positive(raw.stride)
        .or_else(|| positive(raw.width))
        .unwrap_or(crop_left + width);
    let slice_height = positive(raw.slice_height)
        .or_else(|| positive(raw.height))
        .unwrap_or(crop_top + height);
    if stride < crop_left + width || slice_height < crop_top + height {
        return Err(format!(
            "stride {stride} x slice height {slice_height} is smaller than the {width}x{height} \
             picture at ({crop_left}, {crop_top})"
        ));
    }
    let semi_planar = matches!(
        color_format,
        COLOR_FORMAT_YUV420_SEMI_PLANAR
            | COLOR_FORMAT_YUV420_PACKED_SEMI_PLANAR
            | COLOR_QCOM_YUV420_SEMI_PLANAR
            | COLOR_QCOM_YUV420_SEMI_PLANAR_32M
    );
    let planar = matches!(
        color_format,
        COLOR_FORMAT_YUV420_PLANAR | COLOR_FORMAT_YUV420_PACKED_PLANAR
    );
    if semi_planar || planar {
        let planes = if semi_planar {
            Planes::semi_planar(stride, slice_height)
        } else {
            Planes::planar(stride, slice_height)
        };
        return Ok(Picture {
            planes,
            width,
            height,
            crop_left,
            crop_top,
        });
    }
    // The flexible format, or one this backend does not know: only an `image-data` says.
    match raw.image_data.as_deref().and_then(parse_media_image) {
        Some((planes, width, height)) => Ok(Picture::new(planes, width, height)),
        None => Err(format!(
            "the output colour format {color_format:#x} is not one this backend reads (no usable \
             image-data)"
        )),
    }
}

// ---------------------------------------------------------------------------
// Colour
// ---------------------------------------------------------------------------

/// The matrix and range of a decoded picture: what the codec's output format says
/// (`color-standard`: 1 is BT.709, 2 and 4 are BT.601, 6 is BT.2020 and read as BT.709;
/// `color-range`: 1 is full, 2 is limited), else what the stream's VUI says, else the convention
/// of players: BT.709 from 720 rows up, BT.601 below, limited range. Returns `(matrix, full)`.
pub(super) fn picture_colour(
    standard: Option<i32>,
    range: Option<i32>,
    vui: Option<h264::Colour>,
    height: usize,
) -> (Matrix, bool) {
    let by_height = if height >= 720 {
        Matrix::Bt709
    } else {
        Matrix::Bt601
    };
    let matrix = match standard {
        Some(1 | 6) => Matrix::Bt709,
        Some(2 | 4) => Matrix::Bt601,
        _ => match vui.map(|c| c.matrix_coefficients) {
            Some(1) => Matrix::Bt709,
            Some(5 | 6) => Matrix::Bt601,
            _ => by_height,
        },
    };
    let full = match range {
        Some(1) => true,
        Some(2) => false,
        _ => vui.is_some_and(|c| c.full_range),
    };
    (matrix, full)
}

// ---------------------------------------------------------------------------
// RGB -> YCbCr (the encoder's input)
// ---------------------------------------------------------------------------

/// Limited-range luma of an RGB triple (0..=255 each).
fn rgb_to_y(r: i32, g: i32, b: i32, matrix: Matrix) -> u8 {
    let y = match matrix {
        Matrix::Bt601 => 16 + ((66 * r + 129 * g + 25 * b + 128) >> 8),
        Matrix::Bt709 => 16 + ((47 * r + 157 * g + 16 * b + 128) >> 8),
    };
    y.clamp(16, 235) as u8
}

/// Limited-range Cb and Cr of an RGB triple.
fn rgb_to_uv(r: i32, g: i32, b: i32, matrix: Matrix) -> (u8, u8) {
    let (u, v) = match matrix {
        Matrix::Bt601 => (
            128 + ((-38 * r - 74 * g + 112 * b + 128) >> 8),
            128 + ((112 * r - 94 * g - 18 * b + 128) >> 8),
        ),
        Matrix::Bt709 => (
            128 + ((-26 * r - 86 * g + 112 * b + 128) >> 8),
            128 + ((112 * r - 102 * g - 10 * b + 128) >> 8),
        ),
    };
    (u.clamp(16, 240) as u8, v.clamp(16, 240) as u8)
}

/// The pixels an encoder is given.
#[derive(Debug, Clone, Copy)]
pub(super) enum Source<'a> {
    /// Tightly packed NV12: copied as it is (its matrix and range are the caller's tag).
    Nv12(&'a [u8]),
    /// Tightly packed RGBA8.
    Rgba(&'a [u8]),
    /// Tightly packed BGRA8.
    Bgra(&'a [u8]),
}

/// Writes a `width x height` picture into `dst`, laid out as `planes`. RGB sources become
/// limited-range YCbCr with `matrix`, the chroma of each 2x2 block the average of its pixels.
/// `None` when `src` or `dst` is too short, or the size is empty or over [`MAX_SIDE`]. Bytes of
/// `dst` outside the picture (row padding) are left alone.
pub(super) fn write_picture(
    dst: &mut [u8],
    planes: &Planes,
    width: usize,
    height: usize,
    src: Source<'_>,
    matrix: Matrix,
) -> Option<()> {
    if width == 0 || height == 0 || width > MAX_SIDE || height > MAX_SIDE {
        return None;
    }
    let target = Picture::new(*planes, width, height);
    if dst.len() < target.required_len()? {
        return None;
    }
    let (cw, ch) = target.chroma_size();
    let (y, u, v) = (planes.y, planes.u, planes.v);
    match src {
        Source::Nv12(bytes) => {
            if bytes.len() < target.nv12_len()? {
                return None;
            }
            let (luma, chroma) = bytes.split_at(width * height);
            for (row, line) in luma.chunks_exact(width).enumerate() {
                let start = y.offset + row * y.row_inc;
                if y.col_inc == 1 {
                    dst[start..start + width].copy_from_slice(line);
                } else {
                    for (x, &value) in line.iter().enumerate() {
                        dst[start + x * y.col_inc] = value;
                    }
                }
            }
            for (row, pairs) in chroma.chunks_exact(cw * 2).enumerate().take(ch) {
                let u_start = u.offset + row * u.row_inc;
                if planes.is_interleaved() {
                    dst[u_start..u_start + cw * 2].copy_from_slice(pairs);
                } else {
                    let v_start = v.offset + row * v.row_inc;
                    for (x, pair) in pairs.chunks_exact(2).enumerate() {
                        dst[u_start + x * u.col_inc] = pair[0];
                        dst[v_start + x * v.col_inc] = pair[1];
                    }
                }
            }
        }
        Source::Rgba(bytes) | Source::Bgra(bytes) => {
            if bytes.len() < width.checked_mul(height)?.checked_mul(4)? {
                return None;
            }
            let bgra = matches!(src, Source::Bgra(_));
            let pixel = |x: usize, row: usize| -> (i32, i32, i32) {
                let at = (row * width + x) * 4;
                let (a, b, c) = (bytes[at], bytes[at + 1], bytes[at + 2]);
                if bgra {
                    (i32::from(c), i32::from(b), i32::from(a))
                } else {
                    (i32::from(a), i32::from(b), i32::from(c))
                }
            };
            for cy in 0..ch {
                for cx in 0..cw {
                    let (mut sum_r, mut sum_g, mut sum_b, mut count) = (0, 0, 0, 0);
                    for dy in 0..2 {
                        for dx in 0..2 {
                            let (x, row) = (2 * cx + dx, 2 * cy + dy);
                            // The edge of an odd-sized picture: the last pixel stands for the
                            // missing one in the chroma average, and has no luma of its own.
                            let (sx, sy) = (x.min(width - 1), row.min(height - 1));
                            let (r, g, b) = pixel(sx, sy);
                            if x < width && row < height {
                                dst[y.offset + row * y.row_inc + x * y.col_inc] =
                                    rgb_to_y(r, g, b, matrix);
                            }
                            sum_r += r;
                            sum_g += g;
                            sum_b += b;
                            count += 1;
                        }
                    }
                    let half = count / 2;
                    let (cb, cr) = rgb_to_uv(
                        (sum_r + half) / count,
                        (sum_g + half) / count,
                        (sum_b + half) / count,
                        matrix,
                    );
                    dst[u.offset + cy * u.row_inc + cx * u.col_inc] = cb;
                    dst[v.offset + cy * v.row_inc + cx * v.col_inc] = cr;
                }
            }
        }
    }
    Some(())
}

#[cfg(test)]
#[path = "yuv420_test.rs"]
mod yuv420_test;
