use super::*;

/// The padding byte of a codec's rows: any read of it is a bug of the layout maths.
const PAD: u8 = 0xEE;

/// A tight NV12 image whose every sample is a function of where it is.
fn expected_nv12(w: usize, h: usize) -> Vec<u8> {
    let (cw, ch) = (w.div_ceil(2), h.div_ceil(2));
    let mut out = Vec::new();
    for y in 0..h {
        for x in 0..w {
            out.push((x + 3 * y) as u8 % 200 + 10);
        }
    }
    for cy in 0..ch {
        for cx in 0..cw {
            out.push((cx * 5 + cy) as u8 % 100 + 20); // Cb
            out.push((cx + cy * 7) as u8 % 100 + 120); // Cr
        }
    }
    out
}

/// `nv12` as the buffer of a codec: the window at (`left`, `top`) of a `stride x slice_height`
/// picture in `planes`, everything else padding.
fn buffer_of(nv12: &[u8], w: usize, h: usize, planes: &Planes, size: usize, left: usize, top: usize) -> Vec<u8> {
    let (cw, ch) = (w.div_ceil(2), h.div_ceil(2));
    let mut buffer = vec![PAD; size];
    for y in 0..h {
        for x in 0..w {
            buffer[planes.y.offset + (top + y) * planes.y.row_inc + (left + x) * planes.y.col_inc] =
                nv12[y * w + x];
        }
    }
    for cy in 0..ch {
        for cx in 0..cw {
            let at = |plane: &Plane| {
                plane.offset + (top / 2 + cy) * plane.row_inc + (left / 2 + cx) * plane.col_inc
            };
            buffer[at(&planes.u)] = nv12[w * h + (cy * cw + cx) * 2];
            buffer[at(&planes.v)] = nv12[w * h + (cy * cw + cx) * 2 + 1];
        }
    }
    buffer
}

#[test]
fn a_semi_planar_buffer_with_padding_comes_out_as_tight_nv12() {
    let (w, h) = (18, 10);
    let nv12 = expected_nv12(w, h);
    let planes = Planes::semi_planar(32, 16);
    let buffer = buffer_of(&nv12, w, h, &planes, 32 * 16 * 3 / 2, 0, 0);
    let picture = Picture::new(planes, w, h);
    assert_eq!(picture.to_nv12(&buffer), Some(nv12));
}

#[test]
fn a_planar_buffer_with_padding_comes_out_as_the_same_tight_nv12() {
    let (w, h) = (18, 10);
    let nv12 = expected_nv12(w, h);
    let planes = Planes::planar(32, 16);
    let buffer = buffer_of(&nv12, w, h, &planes, 32 * 16 * 3 / 2, 0, 0);
    let picture = Picture::new(planes, w, h);
    assert_eq!(picture.to_nv12(&buffer), Some(nv12));
}

#[test]
fn a_window_in_a_larger_picture_is_cropped_out() {
    let (w, h) = (14, 6);
    let nv12 = expected_nv12(w, h);
    for planes in [Planes::semi_planar(32, 16), Planes::planar(32, 16)] {
        let buffer = buffer_of(&nv12, w, h, &planes, 32 * 16 * 3 / 2, 6, 4);
        let picture = Picture {
            planes,
            width: w,
            height: h,
            crop_left: 6,
            crop_top: 4,
        };
        assert_eq!(picture.to_nv12(&buffer), Some(nv12.clone()));
    }
}

#[test]
fn an_odd_sized_picture_keeps_its_last_column_and_row_of_chroma() {
    let (w, h) = (7, 5);
    let nv12 = expected_nv12(w, h);
    assert_eq!(nv12.len(), 7 * 5 + 4 * 3 * 2);
    for planes in [Planes::semi_planar(8, 6), Planes::planar(8, 6)] {
        let buffer = buffer_of(&nv12, w, h, &planes, 8 * 6 * 3 / 2, 0, 0);
        assert_eq!(Picture::new(planes, w, h).to_nv12(&buffer), Some(nv12.clone()));
    }
}

#[test]
fn a_buffer_that_is_too_short_or_a_picture_that_is_empty_gives_nothing() {
    let planes = Planes::semi_planar(32, 16);
    let picture = Picture::new(planes, 18, 10);
    let need = picture.required_len().expect("a length");
    // The last chroma sample of the picture is the last byte it reads.
    assert_eq!(need, 32 * 16 + 4 * 32 + 18);
    assert!(picture.to_nv12(&vec![0; need - 1]).is_none());
    assert!(picture.to_nv12(&vec![0; need]).is_some());
    assert!(Picture::new(planes, 0, 10).to_nv12(&[0; 4096]).is_none());
    assert!(Picture::new(planes, MAX_SIDE + 1, 2).to_nv12(&[0; 4096]).is_none());
    // Maths that would overflow is an answer, not a panic.
    let huge = Picture {
        planes: Planes::semi_planar(usize::MAX / 2, usize::MAX / 2),
        width: 16,
        height: 16,
        crop_left: 0,
        crop_top: 0,
    };
    assert_eq!(huge.required_len(), None);
    assert!(huge.to_nv12(&[0; 64]).is_none());
}

#[test]
fn nv12_written_into_a_codecs_buffer_reads_back_unchanged_in_either_layout() {
    let (w, h) = (18, 10);
    let nv12 = expected_nv12(w, h);
    for planes in [Planes::semi_planar(32, 16), Planes::planar(32, 16)] {
        let mut buffer = vec![PAD; 32 * 16 * 3 / 2];
        write_picture(&mut buffer, &planes, w, h, Source::Nv12(&nv12), Matrix::Bt709)
            .expect("the buffer holds the picture");
        assert_eq!(Picture::new(planes, w, h).to_nv12(&buffer), Some(nv12.clone()));
        // The padding of the rows is untouched.
        assert_eq!(buffer[w], PAD);
        assert_eq!(buffer[31], PAD);
    }
    let mut short = vec![0u8; 100];
    assert!(write_picture(&mut short, &Planes::semi_planar(32, 16), w, h, Source::Nv12(&nv12), Matrix::Bt601).is_none());
    let mut buffer = vec![0u8; 32 * 16 * 3 / 2];
    assert!(write_picture(&mut buffer, &Planes::semi_planar(32, 16), w, h, Source::Nv12(&nv12[..50]), Matrix::Bt601).is_none());
}

/// A test-local inverse of the forward conversion (limited range).
fn yuv_to_rgb(y: u8, u: u8, v: u8, matrix: Matrix) -> [i32; 3] {
    let (c, d, e) = (i32::from(y) - 16, i32::from(u) - 128, i32::from(v) - 128);
    let (r, g, b) = match matrix {
        Matrix::Bt601 => (
            (298 * c + 409 * e + 128) >> 8,
            (298 * c - 100 * d - 208 * e + 128) >> 8,
            (298 * c + 516 * d + 128) >> 8,
        ),
        Matrix::Bt709 => (
            (298 * c + 459 * e + 128) >> 8,
            (298 * c - 55 * d - 136 * e + 128) >> 8,
            (298 * c + 541 * d + 128) >> 8,
        ),
    };
    [r.clamp(0, 255), g.clamp(0, 255), b.clamp(0, 255)]
}

fn flat_rgba(w: usize, h: usize, rgb: [u8; 3]) -> Vec<u8> {
    (0..w * h).flat_map(|_| [rgb[0], rgb[1], rgb[2], 255]).collect()
}

/// The (Y, Cb, Cr) of a flat RGBA picture written into a semi-planar buffer.
fn flat_yuv(rgb: [u8; 3], matrix: Matrix, bgra: bool) -> (u8, u8, u8) {
    let (w, h) = (8, 8);
    let mut pixels = flat_rgba(w, h, rgb);
    if bgra {
        for px in pixels.chunks_exact_mut(4) {
            px.swap(0, 2);
        }
    }
    let planes = Planes::semi_planar(w, h);
    let mut buffer = vec![0u8; w * h * 3 / 2];
    let source = if bgra { Source::Bgra(&pixels) } else { Source::Rgba(&pixels) };
    write_picture(&mut buffer, &planes, w, h, source, matrix).expect("an 8x8 picture");
    (buffer[0], buffer[w * h], buffer[w * h + 1])
}

#[test]
fn rgb_becomes_limited_range_ycbcr_of_the_matrix() {
    // The anchors of BT.601 video range.
    assert_eq!(flat_yuv([255, 255, 255], Matrix::Bt601, false), (235, 128, 128));
    assert_eq!(flat_yuv([0, 0, 0], Matrix::Bt601, false), (16, 128, 128));
    assert_eq!(flat_yuv([255, 0, 0], Matrix::Bt601, false), (82, 90, 240));
    assert_eq!(flat_yuv([0, 0, 255], Matrix::Bt601, false), (41, 240, 110));
    // BT.709: white and black are the same, the primaries are not.
    assert_eq!(flat_yuv([255, 255, 255], Matrix::Bt709, false), (235, 128, 128));
    assert_eq!(flat_yuv([0, 0, 0], Matrix::Bt709, false), (16, 128, 128));
    let (y, _, v) = flat_yuv([255, 0, 0], Matrix::Bt709, false);
    assert_eq!((y, v), (63, 240));
    // BGRA is the same colour with its bytes swapped.
    for rgb in [[255, 0, 0], [10, 200, 90], [3, 3, 250]] {
        assert_eq!(
            flat_yuv(rgb, Matrix::Bt709, true),
            flat_yuv(rgb, Matrix::Bt709, false)
        );
    }
}

#[test]
fn rgb_through_ycbcr_and_back_stays_within_two_levels_on_flat_colours() {
    for matrix in [Matrix::Bt601, Matrix::Bt709] {
        for rgb in [
            [0, 0, 0],
            [255, 255, 255],
            [128, 128, 128],
            [200, 30, 30],
            [30, 200, 30],
            [30, 30, 200],
            [240, 240, 20],
            [90, 160, 220],
        ] {
            let (y, u, v) = flat_yuv(rgb, matrix, false);
            let back = yuv_to_rgb(y, u, v, matrix);
            for channel in 0..3 {
                let error = (back[channel] - i32::from(rgb[channel])).abs();
                assert!(error <= 3, "{matrix:?} {rgb:?} -> {back:?}: error {error}");
            }
        }
    }
}

#[test]
fn chroma_is_the_average_of_the_four_pixels_it_covers() {
    // A 2x2 picture: two red and two blue pixels average to the chroma of purple.
    let pixels = [
        255, 0, 0, 255, 0, 0, 255, 255, //
        255, 0, 0, 255, 0, 0, 255, 255,
    ];
    let planes = Planes::semi_planar(2, 2);
    let mut buffer = vec![0u8; 6];
    write_picture(&mut buffer, &planes, 2, 2, Source::Rgba(&pixels), Matrix::Bt601).expect("2x2");
    let (_, cb, cr) = flat_yuv([128, 0, 128], Matrix::Bt601, false);
    assert!((i32::from(buffer[4]) - i32::from(cb)).abs() <= 1, "{buffer:?}");
    assert!((i32::from(buffer[5]) - i32::from(cr)).abs() <= 1, "{buffer:?}");
    // Luma is per pixel.
    assert_eq!(buffer[0], buffer[2]);
    assert_ne!(buffer[0], buffer[1]);
}

#[test]
fn an_odd_sized_rgb_picture_fills_every_luma_and_chroma_sample() {
    let (w, h) = (5, 3);
    let pixels = flat_rgba(w, h, [90, 160, 220]);
    let planes = Planes::planar(6, 4);
    let mut buffer = vec![PAD; 6 * 4 * 3 / 2];
    write_picture(&mut buffer, &planes, w, h, Source::Rgba(&pixels), Matrix::Bt709).expect("5x3");
    let nv12 = Picture::new(planes, w, h).to_nv12(&buffer).expect("readable");
    assert!(nv12.iter().all(|&b| b != PAD), "a sample was never written: {nv12:?}");
    let (y, u, v) = flat_yuv([90, 160, 220], Matrix::Bt709, false);
    assert!(nv12[..w * h].iter().all(|&b| b == y));
    assert!(nv12[w * h..].chunks(2).all(|p| p == [u, v]));
}

#[test]
fn a_source_or_buffer_that_does_not_fit_writes_nothing() {
    let planes = Planes::semi_planar(8, 8);
    let mut buffer = vec![0u8; 96];
    assert!(write_picture(&mut buffer, &planes, 8, 8, Source::Rgba(&[0; 8 * 8 * 4 - 1]), Matrix::Bt601).is_none());
    assert!(write_picture(&mut buffer, &planes, 0, 8, Source::Rgba(&[]), Matrix::Bt601).is_none());
    let mut small = vec![0u8; 95];
    assert!(write_picture(&mut small, &planes, 8, 8, Source::Rgba(&[0; 8 * 8 * 4]), Matrix::Bt601).is_none());
}

fn raw(color_format: i32) -> RawOutputFormat {
    RawOutputFormat {
        color_format: Some(color_format),
        width: Some(320),
        height: Some(240),
        stride: Some(320),
        slice_height: Some(240),
        crop_left: Some(0),
        crop_top: Some(0),
        crop_right: Some(319),
        crop_bottom: Some(239),
        ..RawOutputFormat::default()
    }
}

#[test]
fn the_output_format_of_a_semi_planar_decoder_is_planned_with_its_stride_and_crop() {
    let mut format = raw(COLOR_FORMAT_YUV420_SEMI_PLANAR);
    format.width = Some(1920);
    format.height = Some(1088);
    format.stride = Some(1920);
    format.slice_height = Some(1088);
    format.crop_right = Some(1919);
    format.crop_bottom = Some(1079);
    let picture = plan_output(&format, None).expect("planned");
    assert_eq!((picture.width, picture.height), (1920, 1080));
    assert_eq!(picture.planes, Planes::semi_planar(1920, 1088));
    assert_eq!((picture.crop_left, picture.crop_top), (0, 0));
    // The chroma plane starts after the 1088 rows of the Y plane, not after 1080.
    assert_eq!(picture.planes.u.offset, 1920 * 1088);
}

#[test]
fn the_vendor_semi_planar_formats_and_the_planar_format_are_known() {
    for code in [
        COLOR_FORMAT_YUV420_SEMI_PLANAR,
        COLOR_FORMAT_YUV420_PACKED_SEMI_PLANAR,
        COLOR_QCOM_YUV420_SEMI_PLANAR,
        COLOR_QCOM_YUV420_SEMI_PLANAR_32M,
    ] {
        assert_eq!(plan_output(&raw(code), None).expect("known").planes, Planes::semi_planar(320, 240));
    }
    for code in [COLOR_FORMAT_YUV420_PLANAR, COLOR_FORMAT_YUV420_PACKED_PLANAR] {
        assert_eq!(plan_output(&raw(code), None).expect("known").planes, Planes::planar(320, 240));
    }
}

#[test]
fn a_crop_window_and_missing_keys_are_planned_from_what_there_is() {
    let mut format = raw(COLOR_FORMAT_YUV420_PLANAR);
    format.crop_left = Some(8);
    format.crop_top = Some(4);
    format.crop_right = Some(327);
    format.crop_bottom = Some(243);
    format.stride = Some(352);
    format.slice_height = Some(256);
    let picture = plan_output(&format, None).expect("planned");
    assert_eq!((picture.width, picture.height, picture.crop_left, picture.crop_top), (320, 240, 8, 4));
    assert_eq!(picture.planes, Planes::planar(352, 256));

    // No crop keys: the width and height; no stride: the width.
    let bare = RawOutputFormat {
        color_format: Some(COLOR_FORMAT_YUV420_SEMI_PLANAR),
        width: Some(640),
        height: Some(480),
        ..RawOutputFormat::default()
    };
    let picture = plan_output(&bare, None).expect("planned");
    assert_eq!((picture.width, picture.height), (640, 480));
    assert_eq!(picture.planes, Planes::semi_planar(640, 480));
}

#[test]
fn an_output_format_that_cannot_be_read_is_an_error_with_words() {
    let none = plan_output(&RawOutputFormat::default(), None).expect_err("no size");
    assert!(none.contains("no picture size"), "{none}");
    let mut small = raw(COLOR_FORMAT_YUV420_SEMI_PLANAR);
    small.stride = Some(300);
    let err = plan_output(&small, None).expect_err("a stride under the width");
    assert!(err.contains("stride 300"), "{err}");
    let unknown = plan_output(&raw(0x7F00_0123), None).expect_err("an unknown format");
    assert!(unknown.contains("0x7f000123"), "{unknown}");
    let flexible = plan_output(&raw(COLOR_FORMAT_YUV420_FLEXIBLE), None).expect_err("no image-data");
    assert!(flexible.contains("image-data"), "{flexible}");
    let mut huge = raw(COLOR_FORMAT_YUV420_SEMI_PLANAR);
    huge.crop_right = Some(40_000);
    assert!(plan_output(&huge, None).expect_err("over the cap").contains("cap"));
}

/// A `MediaImage2` blob: type, planes, size, depth, allocated depth, then (offset, column
/// increment, row increment, horizontal and vertical subsampling) per plane.
fn media_image(old: bool, planes: [[u32; 5]; 3], depth: u32) -> Vec<u8> {
    let mut blob = Vec::new();
    let mut put = |value: u32| blob.extend_from_slice(&value.to_le_bytes());
    put(1);
    put(3);
    put(64);
    put(32);
    put(depth);
    if !old {
        put(depth);
    }
    for plane in planes {
        for value in plane {
            put(value);
        }
    }
    blob
}

#[test]
fn a_flexible_output_is_planned_from_its_image_data_and_takes_the_planes_as_given() {
    // NV21-like: Y, then V before U, interleaved, with a padded row stride.
    let planes = [[0, 1, 72, 1, 1], [72 * 32 + 1, 2, 72, 2, 2], [72 * 32, 2, 72, 2, 2]];
    for old in [false, true] {
        let mut format = raw(COLOR_FORMAT_YUV420_FLEXIBLE);
        format.image_data = Some(media_image(old, planes, 8));
        let picture = plan_output(&format, None).expect("planned from the image data");
        assert_eq!((picture.width, picture.height), (64, 32));
        assert_eq!(picture.planes.y, Plane { offset: 0, row_inc: 72, col_inc: 1 });
        assert_eq!(picture.planes.u, Plane { offset: 72 * 32 + 1, row_inc: 72, col_inc: 2 });
        assert_eq!(picture.planes.v.offset, 72 * 32);
        // Cb, Cr order comes out right whatever the order in the buffer.
        let mut buffer = vec![0u8; 72 * 32 * 3 / 2];
        buffer[72 * 32] = 200; // V of the first pair
        buffer[72 * 32 + 1] = 100; // U of the first pair
        let nv12 = picture.to_nv12(&buffer).expect("readable");
        assert_eq!(&nv12[64 * 32..64 * 32 + 2], &[100, 200]);
    }
    // 10-bit, a wrong plane count and a wrong subsampling are not read.
    let mut format = raw(COLOR_FORMAT_YUV420_FLEXIBLE);
    format.image_data = Some(media_image(false, planes, 10));
    assert!(plan_output(&format, None).is_err());
    let mut format = raw(COLOR_FORMAT_YUV420_FLEXIBLE);
    format.image_data = Some(media_image(false, [[0, 1, 72, 1, 1], [0, 2, 72, 1, 1], [0, 2, 72, 2, 2]], 8));
    assert!(plan_output(&format, None).is_err());
    let mut format = raw(COLOR_FORMAT_YUV420_FLEXIBLE);
    format.image_data = Some(vec![0; 83]);
    assert!(plan_output(&format, None).is_err());
}

#[test]
fn the_colour_of_a_picture_is_the_codecs_word_then_the_vuis_then_the_convention() {
    let vui = |full_range, matrix_coefficients| Some(h264::Colour { full_range, matrix_coefficients });
    // The output format wins.
    assert_eq!(picture_colour(Some(1), Some(1), vui(false, 5), 240), (Matrix::Bt709, true));
    assert_eq!(picture_colour(Some(4), Some(2), vui(true, 1), 1080), (Matrix::Bt601, false));
    assert_eq!(picture_colour(Some(2), None, None, 1080), (Matrix::Bt601, false));
    assert_eq!(picture_colour(Some(6), None, None, 240), (Matrix::Bt709, false));
    // Then the VUI.
    assert_eq!(picture_colour(None, None, vui(true, 1), 240), (Matrix::Bt709, true));
    assert_eq!(picture_colour(None, None, vui(false, 6), 1080), (Matrix::Bt601, false));
    // A VUI that says nothing of the matrix, and no VUI at all: the convention by height.
    assert_eq!(picture_colour(None, None, vui(false, 2), 719), (Matrix::Bt601, false));
    assert_eq!(picture_colour(None, None, vui(false, 2), 720), (Matrix::Bt709, false));
    assert_eq!(picture_colour(Some(0), Some(0), None, 480), (Matrix::Bt601, false));
}

#[test]
fn without_a_crop_window_the_streams_own_size_is_the_visible_picture() {
    // An OMX decoder names the padded coded size and no crop rect.
    let padded = RawOutputFormat {
        color_format: Some(COLOR_FORMAT_YUV420_SEMI_PLANAR),
        width: Some(1920),
        height: Some(1088),
        stride: Some(1920),
        slice_height: Some(1088),
        ..RawOutputFormat::default()
    };
    let picture = plan_output(&padded, Some((1920, 1080))).expect("planned");
    assert_eq!((picture.width, picture.height), (1920, 1080));
    assert_eq!(picture.planes, Planes::semi_planar(1920, 1088), "the buffer keeps its own padding");
    // No hint, or one that does not fit what the format names: the format's size.
    let picture = plan_output(&padded, None).expect("planned");
    assert_eq!((picture.width, picture.height), (1920, 1088));
    let picture = plan_output(&padded, Some((1920, 1200))).expect("planned");
    assert_eq!((picture.width, picture.height), (1920, 1088));
    let picture = plan_output(&padded, Some((0, 1080))).expect("planned");
    assert_eq!((picture.width, picture.height), (1920, 1088));
    // A crop window of the format wins over the hint.
    let mut cropped = padded.clone();
    cropped.crop_left = Some(0);
    cropped.crop_top = Some(0);
    cropped.crop_right = Some(1919);
    cropped.crop_bottom = Some(1071);
    let picture = plan_output(&cropped, Some((1920, 1080))).expect("planned");
    assert_eq!((picture.width, picture.height), (1920, 1072));
}

#[test]
fn output_formats_and_buffers_that_make_no_sense_never_panic() {
    let mut x = 0x9E37_79B9_7F4A_7C15u64;
    let mut next = move || {
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        x
    };
    let mut pick = |next: &mut dyn FnMut() -> u64| -> Option<i32> {
        match next() % 7 {
            0 => None,
            1 => Some(0),
            2 => Some(-1),
            3 => Some(i32::MAX),
            4 => Some(i32::MIN),
            _ => Some((next() % 4000) as i32),
        }
    };
    for _ in 0..4000 {
        let raw = RawOutputFormat {
            color_format: Some([19, 21, 0x7F42_0888, 0x7FA3_0C04, 7, -3][(next() % 6) as usize]),
            width: pick(&mut next),
            height: pick(&mut next),
            stride: pick(&mut next),
            slice_height: pick(&mut next),
            crop_left: pick(&mut next),
            crop_top: pick(&mut next),
            crop_right: pick(&mut next),
            crop_bottom: pick(&mut next),
            image_data: None,
            color_standard: pick(&mut next),
            color_range: pick(&mut next),
        };
        let hint = if next() % 2 == 0 { Some(((next() % 5000) as usize, (next() % 5000) as usize)) } else { None };
        if let Ok(picture) = plan_output(&raw, hint) {
            let src_len = (next() % 70_000) as usize;
            let src = vec![7u8; src_len];
            if let Some(nv12) = picture.to_nv12(&src) {
                assert_eq!(Some(nv12.len()), picture.nv12_len());
            }
        }
    }
    // Random bytes as an image-data blob.
    for _ in 0..2000 {
        let len = [80usize, 84, 83, 100, 0, 3][(next() % 6) as usize];
        let blob: Vec<u8> = (0..len).map(|_| (next() & 0xff) as u8).collect();
        let mut raw = raw(COLOR_FORMAT_YUV420_FLEXIBLE);
        raw.image_data = Some(blob);
        let _ = plan_output(&raw, None);
    }
}
