//! The formats that no default turns on (QOI, farbfeld, DirectDraw Surface, OpenEXR): a build that
//! asks for one decodes it through the same sniffing as every other format, and a build that does
//! not answers with an error, never a panic. The files are written out by hand from the format
//! descriptions, so that the decoders read bytes that another encoder made.

#[cfg(any(feature = "qoi", feature = "ff", feature = "dds", feature = "exr"))]
use azul_core::resources::{RawImageData, RawImageFormat};

use super::decode::decode_raw_image_from_any_bytes;

/// 1 x 1, one opaque red pixel (qoiformat.org, 2022-01-05): the header, one RGBA op, the end marker.
const QOI: [u8; 27] = [
    b'q', b'o', b'i', b'f', 0, 0, 0, 1, 0, 0, 0, 1, 4, 0, // magic, width, height, 4 channels, sRGB
    0xff, 0xff, 0x00, 0x00, 0xff, // QOI_OP_RGBA: red, alpha 255
    0, 0, 0, 0, 0, 0, 0, 1, // the end marker
];

/// 2 x 1 (farbfeld.org): "farbfeld", the size as big-endian u32, then RGBA of 16 bits each.
const FARBFELD: [u8; 32] = [
    b'f', b'a', b'r', b'b', b'f', b'e', b'l', b'd', 0, 0, 0, 2, 0, 0, 0, 1, // magic, width, height
    0xff, 0xff, 0x00, 0x00, 0x00, 0x00, 0xff, 0xff, // red, opaque
    0x00, 0x00, 0x80, 0x00, 0xff, 0xff, 0x80, 0x00, // (0, 0x8000, 0xffff, 0x8000)
];

/// 4 x 4 DXT1 (Microsoft's DDS programming guide): the 124 byte header, one block of solid red.
fn dds() -> Vec<u8> {
    let mut bytes = b"DDS ".to_vec();
    let header: [u32; 31] = [
        124,       // dwSize
        0x0008_1007, // CAPS | HEIGHT | WIDTH | PIXELFORMAT | LINEARSIZE
        4,         // height
        4,         // width
        8,         // the size of the one block
        0,         // depth
        1,         // mip maps
        0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, // reserved
        32,        // the pixel format: size
        4,         // DDPF_FOURCC
        u32::from_le_bytes(*b"DXT1"),
        0, 0, 0, 0, 0, // bits per pixel and the four masks
        0x1000,    // DDSCAPS_TEXTURE
        0, 0, 0, 0, // the other caps and a reserved word
    ];
    for word in header {
        bytes.extend_from_slice(&word.to_le_bytes());
    }
    // colour 0 = RGB565 red, colour 1 = black, every index 0: sixteen red pixels
    bytes.extend_from_slice(&[0x00, 0xf8, 0x00, 0x00, 0, 0, 0, 0]);
    bytes
}

/// The samples of the OpenEXR file below, red, green, blue and alpha of each of its two pixels; the
/// 2.0 is above white, which a high dynamic range format keeps.
#[cfg(feature = "exr")]
const EXR_SAMPLES: [f32; 8] = [1.0, 0.5, 0.25, 1.0, 0.0, 0.0, 2.0, 0.5];

/// 2 x 1 (the file layout of the OpenEXR documentation): the magic number and version 2, the header
/// (channels A, B, G, R as 32 bit floats, no compression, the data and display windows 0,0 to 1,0,
/// increasing lines, aspect 1, window centre 0,0 and width 1), the offset of the one scanline, the
/// scanline: its y, the size of its data, and the samples of A, B, G and R.
const EXR: [u8; 379] = [
    0x76, 0x2f, 0x31, 0x01, 0x02, 0x00, 0x00, 0x00, 0x63, 0x68, 0x61, 0x6e, 0x6e, 0x65, 0x6c, 0x73,
    0x00, 0x63, 0x68, 0x6c, 0x69, 0x73, 0x74, 0x00, 0x49, 0x00, 0x00, 0x00, 0x41, 0x00, 0x02, 0x00,
    0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x42, 0x00,
    0x02, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00,
    0x47, 0x00, 0x02, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x01, 0x00,
    0x00, 0x00, 0x52, 0x00, 0x02, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00,
    0x01, 0x00, 0x00, 0x00, 0x00, 0x63, 0x6f, 0x6d, 0x70, 0x72, 0x65, 0x73, 0x73, 0x69, 0x6f, 0x6e,
    0x00, 0x63, 0x6f, 0x6d, 0x70, 0x72, 0x65, 0x73, 0x73, 0x69, 0x6f, 0x6e, 0x00, 0x01, 0x00, 0x00,
    0x00, 0x00, 0x64, 0x61, 0x74, 0x61, 0x57, 0x69, 0x6e, 0x64, 0x6f, 0x77, 0x00, 0x62, 0x6f, 0x78,
    0x32, 0x69, 0x00, 0x10, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x01,
    0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x64, 0x69, 0x73, 0x70, 0x6c, 0x61, 0x79, 0x57, 0x69,
    0x6e, 0x64, 0x6f, 0x77, 0x00, 0x62, 0x6f, 0x78, 0x32, 0x69, 0x00, 0x10, 0x00, 0x00, 0x00, 0x00,
    0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x6c,
    0x69, 0x6e, 0x65, 0x4f, 0x72, 0x64, 0x65, 0x72, 0x00, 0x6c, 0x69, 0x6e, 0x65, 0x4f, 0x72, 0x64,
    0x65, 0x72, 0x00, 0x01, 0x00, 0x00, 0x00, 0x00, 0x70, 0x69, 0x78, 0x65, 0x6c, 0x41, 0x73, 0x70,
    0x65, 0x63, 0x74, 0x52, 0x61, 0x74, 0x69, 0x6f, 0x00, 0x66, 0x6c, 0x6f, 0x61, 0x74, 0x00, 0x04,
    0x00, 0x00, 0x00, 0x00, 0x00, 0x80, 0x3f, 0x73, 0x63, 0x72, 0x65, 0x65, 0x6e, 0x57, 0x69, 0x6e,
    0x64, 0x6f, 0x77, 0x43, 0x65, 0x6e, 0x74, 0x65, 0x72, 0x00, 0x76, 0x32, 0x66, 0x00, 0x08, 0x00,
    0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x73, 0x63, 0x72, 0x65, 0x65, 0x6e,
    0x57, 0x69, 0x6e, 0x64, 0x6f, 0x77, 0x57, 0x69, 0x64, 0x74, 0x68, 0x00, 0x66, 0x6c, 0x6f, 0x61,
    0x74, 0x00, 0x04, 0x00, 0x00, 0x00, 0x00, 0x00, 0x80, 0x3f, 0x00, 0x53, 0x01, 0x00, 0x00, 0x00,
    0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x20, 0x00, 0x00, 0x00, 0x00, 0x00, 0x80, 0x3f, 0x00,
    0x00, 0x00, 0x3f, 0x00, 0x00, 0x80, 0x3e, 0x00, 0x00, 0x00, 0x40, 0x00, 0x00, 0x00, 0x3f, 0x00,
    0x00, 0x00, 0x00, 0x00, 0x00, 0x80, 0x3f, 0x00, 0x00, 0x00, 0x00,
];

#[test]
fn a_qoi_file_decodes_in_a_build_that_asks_for_it_and_is_an_error_in_one_that_does_not() {
    let result = decode_raw_image_from_any_bytes(&QOI).into_result();
    #[cfg(feature = "qoi")]
    {
        let image = result.expect("a build with the qoi feature decodes QOI");
        assert_eq!((image.width, image.height), (1, 1));
        assert_eq!(image.data_format, RawImageFormat::RGBA8);
        let RawImageData::U8(pixels) = &image.pixels else {
            panic!("8-bit samples");
        };
        assert_eq!(pixels.as_ref(), &[0xff, 0x00, 0x00, 0xff]);
    }
    #[cfg(not(feature = "qoi"))]
    assert!(result.is_err(), "no qoi feature, no QOI decoder");
}

#[test]
fn a_farbfeld_file_decodes_in_a_build_that_asks_for_it_and_is_an_error_in_one_that_does_not() {
    let result = decode_raw_image_from_any_bytes(&FARBFELD).into_result();
    #[cfg(feature = "ff")]
    {
        let image = result.expect("a build with the ff feature decodes farbfeld");
        assert_eq!((image.width, image.height), (2, 1));
        assert_eq!(image.data_format, RawImageFormat::RGBA16);
        let RawImageData::U16(pixels) = &image.pixels else {
            panic!("16-bit samples");
        };
        assert_eq!(
            pixels.as_ref(),
            &[0xffff, 0, 0, 0xffff, 0, 0x8000, 0xffff, 0x8000]
        );
    }
    #[cfg(not(feature = "ff"))]
    assert!(result.is_err(), "no ff feature, no farbfeld decoder");
}

#[test]
fn a_dds_file_decodes_in_a_build_that_asks_for_it_and_is_an_error_in_one_that_does_not() {
    let result = decode_raw_image_from_any_bytes(&dds()).into_result();
    #[cfg(feature = "dds")]
    {
        let image = result.expect("a build with the dds feature decodes DXT1");
        assert_eq!((image.width, image.height), (4, 4));
        let RawImageData::U8(pixels) = &image.pixels else {
            panic!("8-bit samples");
        };
        let samples: &[u8] = pixels.as_ref();
        let channels = samples.len() / 16;
        assert!(channels >= 3, "colour samples: {channels} per pixel");
        // sixteen red pixels (RGB565 red expands to 255 in the red channel)
        for pixel in samples.chunks_exact(channels) {
            assert!(pixel[0] >= 248 && pixel[1] <= 8 && pixel[2] <= 8, "{pixel:?}");
        }
    }
    #[cfg(not(feature = "dds"))]
    assert!(result.is_err(), "no dds feature, no DDS decoder");
}

#[test]
fn an_openexr_file_decodes_in_a_build_that_asks_for_it_and_is_an_error_in_one_that_does_not() {
    let result = decode_raw_image_from_any_bytes(&EXR).into_result();
    #[cfg(feature = "exr")]
    {
        let image = result.expect("a build with the exr feature decodes OpenEXR");
        assert_eq!((image.width, image.height), (2, 1));
        assert_eq!(image.data_format, RawImageFormat::RGBAF32);
        let RawImageData::F32(pixels) = &image.pixels else {
            panic!("float samples");
        };
        assert_eq!(
            pixels.as_ref(),
            &EXR_SAMPLES,
            "every sample comes back, the 2.0 above white too"
        );
    }
    #[cfg(not(feature = "exr"))]
    assert!(result.is_err(), "no exr feature, no OpenEXR decoder");
}

#[test]
fn the_header_of_a_file_with_nothing_behind_it_is_an_error_and_never_a_panic() {
    // the first bytes of an OpenEXR file (magic and version) and of the other three, and nothing more
    let heads: [&[u8]; 4] = [
        &[0x76, 0x2f, 0x31, 0x01, 0x02, 0x00, 0x00, 0x00],
        b"qoif\0\0\0\x01\0\0\0\x01\x04\0",
        b"farbfeld\0\0\0\x02\0\0\0\x01",
        b"DDS \x7c\0\0\0",
    ];
    for head in heads {
        assert!(
            decode_raw_image_from_any_bytes(head).into_result().is_err(),
            "{head:?}"
        );
    }
}
