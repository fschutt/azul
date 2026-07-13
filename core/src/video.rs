//! POD types for the video-playback surface
//! (SUPER_PLAN_2 §4 Priority 6 + research).
//!
//! Same "dumb widget" architecture as camera/screencap
//! (`azul_layout::widgets::video::VideoWidget`): a background thread decodes
//! the source (vk-video - GPU decode + HTTP-range fetch) and its writeback
//! uploads each frame into the shared GL-texture `ImageRef` + recomposites.
//! Defined here in `azul-core` so the config crosses the FFI without
//! `azul-layout` (or vk-video) as a dependency.
//!
//! Unlike the camera/screencap configs this carries a `source` string, so
//! it's `Clone` but not `Copy`.

use crate::resources::RawImageFormat;
use crate::url::Url;
use azul_css::{AzString, U8Vec};
#[allow(variant_size_differences)] // repr(C,u8) FFI enum: boxing the large variant would change the C ABI (api.json bindings); size disparity accepted
/// Where a video widget pulls its H.264/MP4 data from — strongly typed so the
/// decode worker matches on it directly (no `RefAny` downcast). Mirrors
/// [`crate::screencap::ScreenCaptureSource`].
#[repr(C, u8)]
#[derive(Debug, Clone, PartialEq, Eq)]
#[allow(clippy::large_enum_variant)] // #[repr(C,u8)] FFI enum: boxing a variant changes the C ABI/api.json
pub enum VideoSource {
    /// An HTTP(S) URL, fetched on the decode thread via an HTTP range request.
    Url(Url),
    /// A local filesystem path.
    File(AzString),
    /// Raw MP4 bytes already in memory.
    Bytes(U8Vec),
}

impl Default for VideoSource {
    fn default() -> Self {
        Self::Url(Url::default())
    }
}

/// Requested video-playback configuration.
#[repr(C)]
#[derive(Debug, Clone, PartialEq)]
pub struct VideoConfig {
    /// Where to load the video from (URL / file path / in-memory bytes).
    pub source: VideoSource,
    /// Seek / scrub position in seconds. Changing it across a relayout makes the
    /// widget's merge callback tell the decode worker to seek (scrubbing
    /// timeline) — the decoder survives relayout like the map's tile cache.
    pub timestamp: f32,
    /// Start playing automatically on mount.
    pub autoplay: bool,
    /// Restart from the beginning when the stream ends.
    pub looping: bool,
    /// Texture format the decoder delivers. `BGRA8` is the portable default;
    /// `Nv12` (a later `RawImageFormat` addition) is the zero-copy path.
    pub output_format: RawImageFormat,
}

impl Default for VideoConfig {
    fn default() -> Self {
        Self {
            source: VideoSource::default(),
            timestamp: 0.0,
            autoplay: true,
            looping: false,
            output_format: RawImageFormat::BGRA8,
        }
    }
}

impl VideoConfig {
    /// A default config playing `source` (autoplay on, no loop, BGRA8, t=0).
    #[must_use] pub fn new(source: VideoSource) -> Self {
        Self {
            source,
            ..Self::default()
        }
    }
}

/// One captured or decoded frame - tightly-packed RGBA8 pixels
/// (`width * height * 4`).
///
/// The unit a capture/decode worker produces, the
/// `set_on_frame` hook hands to user code (effects / save / send), and (P8)
/// azul-meet sends over UDP. Defined here (like [`crate::audio::AudioFrame`])
/// so it crosses the FFI without `azul-layout` as a dependency.
#[repr(C)]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VideoFrame {
    /// Frame width in px.
    pub width: u32,
    /// Frame height in px.
    pub height: u32,
    /// Tightly-packed RGBA8 pixel bytes (`width * height * 4`).
    pub bytes: U8Vec,
}

impl VideoFrame {
    /// A frame wrapping `bytes` (tightly-packed RGBA8, `width * height * 4`).
    #[must_use] pub const fn new(width: u32, height: u32, bytes: U8Vec) -> Self {
        Self {
            width,
            height,
            bytes,
        }
    }
}

// FFI Option wrapper for a frame-pull hook / accessor. `copy = false` (U8Vec).
impl_option!(VideoFrame, OptionVideoFrame, copy = false, [Clone, Debug]);

// FFI `Vec<VideoFrame>` wrapper — the list a batch decode (`DecodedVideo`,
// `dll::desktop::extra::video_codec::pipeline`) hands back across the C ABI.
// `VideoFrame` derives Debug + Clone + PartialEq, so mirror exactly those Vec
// trait impls (no PartialOrd: `VideoFrame` isn't `PartialOrd`).
impl_vec!(VideoFrame, VideoFrameVec, VideoFrameVecDestructor, VideoFrameVecDestructorType, VideoFrameVecSlice, OptionVideoFrame);
impl_vec_debug!(VideoFrame, VideoFrameVec);
impl_vec_clone!(VideoFrame, VideoFrameVec, VideoFrameVecDestructor);
impl_vec_partialeq!(VideoFrame, VideoFrameVec);

#[cfg(test)]
mod autotest_generated {
    use alloc::{string::String, vec, vec::Vec};

    use super::*;

    // --- helpers ---------------------------------------------------------

    /// Owned (`DefaultRust`-destructor) `U8Vec` — the shape a real decode
    /// worker hands back, so clone/drop actually exercise the allocator.
    fn u8v(bytes: Vec<u8>) -> U8Vec {
        U8Vec::from_vec(bytes)
    }

    fn frame(width: u32, height: u32, bytes: Vec<u8>) -> VideoFrame {
        VideoFrame::new(width, height, u8v(bytes))
    }

    /// The `bytes` of a `VideoSource::Bytes`; test-only, the other variants
    /// are never passed here.
    fn source_bytes(source: &VideoSource) -> &U8Vec {
        match source {
            VideoSource::Bytes(b) => b,
            other => panic!("expected VideoSource::Bytes, got {other:?}"),
        }
    }

    /// The documented `VideoFrame` size invariant, computed in u128 so that the
    /// expectation itself cannot overflow (u64 does, at max dimensions).
    fn declared_len(width: u32, height: u32) -> u128 {
        u128::from(width) * u128::from(height) * 4
    }

    // --- VideoConfig::new (constructor) ----------------------------------

    /// invariants_hold: every non-source field is the documented default
    /// (autoplay on, no loop, BGRA8, t=0) and `source` is stored verbatim.
    #[test]
    fn config_new_fields_match_args() {
        let url = Url::from_parts("https", "example.com", 443, "/movie.mp4");
        let c = VideoConfig::new(VideoSource::Url(url.clone()));

        assert_eq!(c.source, VideoSource::Url(url));
        assert!((c.timestamp - 0.0).abs() < f32::EPSILON);
        assert!(c.autoplay);
        assert!(!c.looping);
        assert_eq!(c.output_format, RawImageFormat::BGRA8);
    }

    /// invariants_hold: `new` sets ONLY `source` — every other field stays
    /// bit-for-bit identical to `Default`, for each of the three variants.
    #[test]
    fn config_new_only_overrides_source() {
        let d = VideoConfig::default();

        for source in [
            VideoSource::Url(Url::from_parts("http", "a.tld", 8080, "/v")),
            VideoSource::File(AzString::from_const_str("/tmp/clip.mp4")),
            VideoSource::Bytes(u8v(vec![0xFF; 8])),
        ] {
            let c = VideoConfig::new(source.clone());
            assert_eq!(c.source, source, "source must round-trip unchanged");
            assert_eq!(c.timestamp.to_bits(), d.timestamp.to_bits());
            assert_eq!(c.autoplay, d.autoplay);
            assert_eq!(c.looping, d.looping);
            assert_eq!(c.output_format, d.output_format);
        }
    }

    /// invariants_hold: the default source funnels back to the plain `Default`.
    #[test]
    fn config_new_with_default_source_equals_default() {
        assert_eq!(VideoConfig::new(VideoSource::default()), VideoConfig::default());
        assert_eq!(VideoSource::default(), VideoSource::Url(Url::default()));
    }

    /// unicode: a non-ASCII file path (emoji, CJK, RTL, combining marks) must
    /// survive the `AzString` round-trip byte-for-byte — no truncation at a
    /// multi-byte boundary, no re-encoding.
    #[test]
    fn config_new_unicode_file_path_roundtrip() {
        let path = String::from("/tmp/vídeos/𝔘𝔫𝔦/影片 🎬/مقطع/e\u{0301}.mp4");
        let c = VideoConfig::new(VideoSource::File(AzString::from(path.clone())));

        match &c.source {
            VideoSource::File(s) => {
                assert_eq!(s.as_str(), path.as_str());
                assert_eq!(s.as_str().len(), path.len(), "byte length preserved");
                assert_eq!(s.as_str().chars().count(), path.chars().count());
            }
            other => panic!("expected File, got {other:?}"),
        }
    }

    /// malformed: an interior NUL and control bytes are legal in a Rust `str`
    /// and must be preserved verbatim (any C-side truncation is the FFI layer's
    /// problem, not this constructor's).
    #[test]
    fn config_new_file_path_with_interior_nul_preserved() {
        let path = String::from("/tmp/a\0b\r\n\t.mp4");
        let c = VideoConfig::new(VideoSource::File(AzString::from(path.clone())));

        match &c.source {
            VideoSource::File(s) => {
                assert_eq!(s.as_str(), path.as_str());
                assert_eq!(s.as_str().len(), 15);
                assert!(s.as_str().contains('\0'));
            }
            other => panic!("expected File, got {other:?}"),
        }
    }

    /// boundary: an empty path / empty byte source must not panic and must stay empty.
    #[test]
    fn config_new_empty_sources_no_panic() {
        let empty_file = VideoConfig::new(VideoSource::File(AzString::from(String::new())));
        match &empty_file.source {
            VideoSource::File(s) => assert_eq!(s.as_str(), ""),
            other => panic!("expected File, got {other:?}"),
        }

        let empty_bytes = VideoConfig::new(VideoSource::Bytes(u8v(Vec::new())));
        let b = source_bytes(&empty_bytes.source);
        assert!(b.is_empty());
        assert_eq!(b.len(), 0);
        assert_eq!(b.as_ref(), &[] as &[u8]);
    }

    /// huge: a 1 MiB in-memory MP4 must move into the config with no copy loss —
    /// check both ends of the buffer, not just the length.
    #[test]
    fn config_new_huge_bytes_source_preserved() {
        const N: usize = 1 << 20;
        let data: Vec<u8> = (0..N).map(|i| (i % 251) as u8).collect();

        let c = VideoConfig::new(VideoSource::Bytes(u8v(data.clone())));
        let b = source_bytes(&c.source);

        let last = ((N - 1) % 251) as u8;
        assert_eq!(b.len(), N);
        assert_eq!(b.as_ref(), data.as_slice());
        assert_eq!(b.get(0), Some(&0));
        assert_eq!(b.get(N - 1), Some(&last));
        assert_eq!(b.get(N), None, "one past the end must be None, not UB");
    }

    /// invariants_hold: `Clone` of a `Bytes` config is a DEEP copy — the clone
    /// owns its own allocation and stays readable after the original is dropped
    /// (a shallow copy here would be a double-free at the second drop).
    #[test]
    fn config_clone_of_bytes_is_deep_and_survives_original_drop() {
        let original = VideoConfig::new(VideoSource::Bytes(u8v(vec![1, 2, 3, 4, 5])));
        let original_ptr = source_bytes(&original.source).as_ptr();

        let cloned = original.clone();
        let cloned_ptr = source_bytes(&cloned.source).as_ptr();
        assert_ne!(original_ptr, cloned_ptr, "clone must not alias the original buffer");

        drop(original);

        assert_eq!(source_bytes(&cloned.source).as_ref(), &[1, 2, 3, 4, 5]);
    }

    /// invariants_hold: the variants are genuinely distinct — a `File` path and
    /// a `Url` with the same text are NOT equal, so the decode worker's match
    /// can't be spoofed.
    #[test]
    fn config_source_variants_are_distinct() {
        let text = "https://example.com/v.mp4";
        let as_file = VideoConfig::new(VideoSource::File(AzString::from_const_str(text)));
        let as_url = VideoConfig::new(VideoSource::Url(Url::from_parts(
            "https",
            "example.com",
            443,
            "/v.mp4",
        )));

        assert_ne!(as_file, as_url);
        assert_ne!(as_file.source, as_url.source);
        assert_eq!(as_url.source, as_url.source.clone());
    }

    /// numeric/limits: extreme scrub positions are stored verbatim (the POD does
    /// no clamping) — infinities, subnormals and f32::MAX must not panic.
    #[test]
    fn config_extreme_timestamps_stored_verbatim() {
        let base = VideoConfig::new(VideoSource::default());

        for t in [
            f32::INFINITY,
            f32::NEG_INFINITY,
            f32::MAX,
            f32::MIN,
            f32::MIN_POSITIVE,
            f32::MIN_POSITIVE / 2.0, // subnormal
            -1.0,
            1e30,
        ] {
            let c = VideoConfig {
                timestamp: t,
                ..base.clone()
            };
            assert_eq!(c.timestamp.to_bits(), t.to_bits(), "timestamp {t} was altered");
            assert_eq!(c, c.clone(), "non-NaN config must be self-equal");
        }
    }

    /// numeric/NaN: derived `PartialEq` over an f32 field means a NaN timestamp
    /// makes a config unequal to an identical config (reflexivity is broken).
    /// Callers must not use `VideoConfig` equality to decide "did the seek
    /// change?" if a NaN can reach `timestamp` — assert the hazard explicitly.
    #[test]
    fn config_nan_timestamp_breaks_reflexive_equality() {
        let nan_a = VideoConfig {
            timestamp: f32::NAN,
            ..VideoConfig::new(VideoSource::default())
        };
        let nan_b = nan_a.clone();

        assert!(nan_a.timestamp.is_nan());
        assert_ne!(nan_a, nan_b, "NaN timestamp: derived PartialEq is not reflexive");

        // ...and `new` itself never produces such a config.
        let fresh = VideoConfig::new(VideoSource::default());
        assert!(!fresh.timestamp.is_nan());
        assert_eq!(fresh, fresh.clone());
    }

    /// numeric: -0.0 compares EQUAL to +0.0 (IEEE), even though the bits differ —
    /// so a config scrubbed to -0.0 is `==` to a freshly constructed one.
    #[test]
    fn config_negative_zero_timestamp_equals_default() {
        let fresh = VideoConfig::new(VideoSource::default());
        let neg_zero = VideoConfig {
            timestamp: -0.0,
            ..VideoConfig::new(VideoSource::default())
        };

        assert!(neg_zero.timestamp.is_sign_negative());
        assert_ne!(neg_zero.timestamp.to_bits(), fresh.timestamp.to_bits());
        assert_eq!(neg_zero, fresh, "IEEE: -0.0 == +0.0");
    }

    // --- VideoFrame::new (constructor) -----------------------------------

    /// invariants_hold: fields match the exact args and the byte buffer round-trips.
    #[test]
    fn frame_new_fields_match_args() {
        let pixels: Vec<u8> = (0..(3 * 2 * 4)).map(|i| i as u8).collect();
        let f = frame(3, 2, pixels.clone());

        assert_eq!(f.width, 3);
        assert_eq!(f.height, 2);
        assert_eq!(f.bytes.as_ref(), pixels.as_slice());
        assert_eq!(u128::from(f.bytes.len() as u64), declared_len(3, 2));
    }

    /// round-trip: all 256 byte values survive `Vec -> U8Vec -> slice` unchanged
    /// (no sign-extension / no UTF-8 sanitisation on the pixel path).
    #[test]
    fn frame_bytes_roundtrip_all_byte_values() {
        let pixels: Vec<u8> = (0..=255u8).collect(); // 256 B == 8x8 RGBA
        let f = frame(8, 8, pixels.clone());

        assert_eq!(f.bytes.as_ref(), pixels.as_slice());
        assert_eq!(f.bytes.len(), 256);
        assert_eq!(u128::from(f.bytes.len() as u64), declared_len(8, 8));
        assert_eq!(f.bytes.get(0), Some(&0));
        assert_eq!(f.bytes.get(255), Some(&255));
        assert_eq!(f.bytes.get(256), None);
        assert!(f.bytes.iter().copied().eq(pixels.iter().copied()));
    }

    /// boundary: a 0x0 frame with no bytes is legal and must not panic.
    #[test]
    fn frame_new_zero_dimensions_empty_bytes() {
        let f = frame(0, 0, Vec::new());

        assert_eq!(f.width, 0);
        assert_eq!(f.height, 0);
        assert!(f.bytes.is_empty());
        assert_eq!(f.bytes.as_ref(), &[] as &[u8]);
        assert_eq!(declared_len(0, 0), 0);
    }

    /// boundary: a degenerate strip (n x 0 / 0 x n) keeps its nonzero dimension.
    #[test]
    fn frame_new_degenerate_strips() {
        let wide = frame(1920, 0, Vec::new());
        assert_eq!((wide.width, wide.height), (1920, 0));
        assert!(wide.bytes.is_empty());

        let tall = frame(0, 1080, Vec::new());
        assert_eq!((tall.width, tall.height), (0, 1080));
        assert!(tall.bytes.is_empty());
    }

    /// numeric/overflow: `new` accepts u32::MAX dimensions without panicking or
    /// touching `bytes` — it never computes `w * h * 4`. It is therefore the
    /// CALLER that must do the size math in u128: at these dimensions the
    /// declared buffer size overflows even u64.
    #[test]
    fn frame_new_max_dimensions_no_panic_and_size_math_overflows() {
        let f = VideoFrame::new(u32::MAX, u32::MAX, U8Vec::from_vec(Vec::new()));

        assert_eq!(f.width, u32::MAX);
        assert_eq!(f.height, u32::MAX);
        assert!(f.bytes.is_empty(), "new does no allocation of its own");

        // The documented `width * height * 4` is unrepresentable here...
        assert_eq!(u32::MAX.checked_mul(u32::MAX), None);
        assert_eq!(
            u64::from(u32::MAX)
                .checked_mul(u64::from(u32::MAX))
                .and_then(|px| px.checked_mul(4)),
            None,
            "even u64 overflows: callers must size-check before allocating"
        );
        // ...but u128 holds it, and that is what `declared_len` uses.
        assert_eq!(declared_len(u32::MAX, u32::MAX), 4 * (u128::from(u32::MAX)).pow(2));
    }

    /// numeric/boundary: 32768x32768 is the smallest square frame whose RGBA byte
    /// count (2^32) no longer fits in u32 — the constructor still accepts it.
    #[test]
    fn frame_new_u32_pixel_size_overflow_boundary() {
        let f = VideoFrame::new(32_768, 32_768, U8Vec::from_vec(Vec::new()));
        assert_eq!((f.width, f.height), (32_768, 32_768));

        assert_eq!(
            32_768u32.checked_mul(32_768).and_then(|px| px.checked_mul(4)),
            None,
            "2^30 px * 4 == 2^32 overflows u32"
        );
        assert_eq!(declared_len(32_768, 32_768), 1u128 << 32);
        // One row narrower still fits.
        assert_eq!(
            32_767u32.checked_mul(32_768).and_then(|px| px.checked_mul(4)),
            Some(4_294_836_224)
        );
    }

    /// invariants_hold: `new` is a pure wrapper — it does NOT enforce
    /// `bytes.len() == width * height * 4`. A short buffer is accepted as-is, so
    /// consumers must validate before indexing (this is the documented contract,
    /// pinned here so a future "validating" rewrite can't land silently).
    #[test]
    fn frame_new_does_not_validate_buffer_length() {
        let short = frame(1, 1, vec![0xAA, 0xBB, 0xCC]); // 3 B, needs 4
        assert_eq!(short.bytes.len(), 3);
        assert_ne!(u128::from(short.bytes.len() as u64), declared_len(1, 1));

        let long = frame(1, 1, vec![0; 64]); // 64 B, needs 4
        assert_eq!(long.bytes.len(), 64);

        // A frame claiming 4K but carrying nothing is likewise constructible.
        let lying = frame(3840, 2160, Vec::new());
        assert!(lying.bytes.is_empty());
        assert_eq!(declared_len(3840, 2160), 33_177_600);
    }

    /// invariants_hold: `const fn new` really is usable in a const context.
    #[test]
    fn frame_new_is_const_evaluable() {
        const F: VideoFrame = VideoFrame::new(1, 1, U8Vec::from_const_slice(&[1, 2, 3, 4]));

        assert_eq!(F.width, 1);
        assert_eq!(F.height, 1);
        assert_eq!(F.bytes.as_ref(), &[1, 2, 3, 4]);
        assert_eq!(u128::from(F.bytes.len() as u64), declared_len(1, 1));
    }

    /// invariants_hold: equality is field-wise — differing width, height or a
    /// single differing pixel byte all compare unequal.
    #[test]
    fn frame_equality_is_fieldwise() {
        let base = frame(2, 1, vec![1, 2, 3, 4, 5, 6, 7, 8]);

        assert_eq!(base, frame(2, 1, vec![1, 2, 3, 4, 5, 6, 7, 8]));
        assert_ne!(base, frame(1, 2, vec![1, 2, 3, 4, 5, 6, 7, 8]), "w/h swap differs");
        assert_ne!(base, frame(2, 1, vec![1, 2, 3, 4, 5, 6, 7, 9]), "last byte differs");
        assert_ne!(base, frame(2, 1, vec![1, 2, 3, 4]), "truncated buffer differs");
    }

    /// invariants_hold: `Clone` deep-copies the pixels — the clone outlives the
    /// original's drop and owns a distinct allocation.
    #[test]
    fn frame_clone_is_deep_and_survives_original_drop() {
        let original = frame(2, 2, (0..16).collect());
        let original_ptr = original.bytes.as_ptr();

        let cloned = original.clone();
        assert_ne!(original_ptr, cloned.bytes.as_ptr(), "clone must not alias");
        assert_eq!(cloned, original);

        drop(original);

        assert_eq!(cloned.bytes.as_ref(), (0..16u8).collect::<Vec<_>>().as_slice());
        assert_eq!(cloned.width, 2);
        assert_eq!(cloned.height, 2);
    }

    // --- OptionVideoFrame / VideoFrameVec (FFI wrappers) ------------------

    /// round-trip: `Option<VideoFrame> -> OptionVideoFrame -> Option<VideoFrame>`
    /// is the identity for both `Some` and `None`.
    #[test]
    fn option_video_frame_roundtrip() {
        let f = frame(1, 1, vec![9, 8, 7, 6]);

        let some: OptionVideoFrame = Some(f.clone()).into();
        assert!(some.is_some());
        assert!(!some.is_none());
        assert_eq!(some.as_ref(), Some(&f));
        assert_eq!(Option::<VideoFrame>::from(some.clone()), Some(f.clone()));
        assert_eq!(some.into_option(), Some(f));

        let none: OptionVideoFrame = Option::<VideoFrame>::None.into();
        assert!(none.is_none());
        assert_eq!(none.as_ref(), None);
        assert_eq!(Option::<VideoFrame>::from(none), None);
        assert_eq!(OptionVideoFrame::default().into_option(), None);
    }

    /// invariants_hold: `replace` returns the PREVIOUS value (mem::replace semantics).
    #[test]
    fn option_video_frame_replace_returns_previous() {
        let first = frame(1, 1, vec![1, 1, 1, 1]);
        let second = frame(1, 1, vec![2, 2, 2, 2]);

        let mut slot = OptionVideoFrame::None;
        assert_eq!(slot.replace(first.clone()).into_option(), None);
        assert_eq!(slot.replace(second.clone()).into_option(), Some(first));
        assert_eq!(slot.into_option(), Some(second));
    }

    /// round-trip: `Vec<VideoFrame> -> VideoFrameVec -> slice` preserves order,
    /// length and every frame, and out-of-bounds access returns None (not UB).
    #[test]
    fn frame_vec_roundtrip_and_bounds() {
        let frames = vec![
            frame(1, 1, vec![0, 0, 0, 255]),
            frame(2, 1, vec![1; 8]),
            frame(0, 0, Vec::new()),
        ];

        let v = VideoFrameVec::from_vec(frames.clone());

        assert_eq!(v.len(), 3);
        assert!(!v.is_empty());
        assert_eq!(v.as_ref(), frames.as_slice());
        assert_eq!(v.get(0), Some(&frames[0]));
        assert_eq!(v.get(2), Some(&frames[2]));
        assert_eq!(v.get(3), None, "out of bounds must be None");
        assert_eq!(v.get(usize::MAX), None, "usize::MAX index must be None");
        assert!(v.iter().eq(frames.iter()), "iteration order preserved");

        // C-API accessor mirrors the Rust one.
        assert_eq!(v.c_get(1).into_option(), Some(frames[1].clone()));
        assert!(v.c_get(3).is_none());

        // Deep clone: equal contents, distinct backing allocation.
        let cloned = v.clone();
        assert_eq!(cloned, v);
        assert_ne!(cloned.as_ptr(), v.as_ptr());
    }

    /// boundary: the empty frame list is well-formed (empty slice, no panic).
    #[test]
    fn frame_vec_empty_is_well_formed() {
        let v = VideoFrameVec::from_vec(Vec::new());

        assert!(v.is_empty());
        assert_eq!(v.len(), 0);
        assert_eq!(v.as_ref(), &[] as &[VideoFrame]);
        assert_eq!(v.get(0), None);
        assert_eq!(v.clone(), v);
        assert_eq!(VideoFrameVec::new(), v);
    }

    /// invariants_hold: a `'static`-backed (`NoDestructor`) frame list — the
    /// const-data path across the FFI — reads back identically and its drop must
    /// not free the static memory.
    #[test]
    fn frame_vec_from_const_slice_no_destructor() {
        static FRAMES: [VideoFrame; 2] = [
            VideoFrame::new(1, 1, U8Vec::from_const_slice(&[1, 2, 3, 4])),
            VideoFrame::new(1, 1, U8Vec::from_const_slice(&[5, 6, 7, 8])),
        ];

        let v = VideoFrameVec::from_const_slice(&FRAMES);
        assert_eq!(v.len(), 2);
        assert_eq!(v.as_ref(), &FRAMES[..]);
        assert_eq!(v.get(1).map(|f| f.bytes.as_ref()), Some(&[5, 6, 7, 8][..]));

        // Cloning a const-slice vec yields an owned copy; both drop cleanly here.
        let cloned = v.clone();
        assert_eq!(cloned, v);
        drop(v);
        assert_eq!(cloned.len(), 2);
    }
}
