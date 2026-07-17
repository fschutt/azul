//! POD types for the camera-capture surface
//! (SUPER_PLAN_2 §4 Priority 6 + research/01).
//!
//! Camera frames are GPU textures, not scalar samples, so the stateful side
//! is heavier than the sensors': `azul_layout::managers::camera` owns a
//! `CameraStream` per capture, each holding a shared `ImageRef` texture the
//! capture thread writes into (zero-copy - clones see new bytes via the
//! `ImageRef` `Arc`). A `CameraPreview` node renders that texture and, by
//! appearing in the DOM, declares "I need the camera" to the permission
//! layer (research/01 §"permission-as-DOM").
//!
//! Defined here in `azul-core` so the config / id / status types cross the
//! FFI without `azul-layout` (or AVFoundation / Camera2) as a dependency -
//! these are what an app passes to `start_camera` and reads back from a
//! stream. The `Nv12` zero-copy output format is a `RawImageFormat` addition
//! deferred to the backend tick; configs default to `BGRA8`.

use crate::resources::RawImageFormat;

/// Identifies one camera capture stream - assigned by `start_camera`, used
/// to read the stream back (`get_camera_frame`) and to stop / pause / flip it.
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct CaptureStreamId {
    pub id: u64,
}

/// Which physical camera to open.
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum CameraFacing {
    /// User-facing (selfie) camera.
    Front,
    /// World-facing (rear) camera.
    Back,
    /// An external / USB camera (desktop webcams report here).
    External,
}

/// Lifecycle of a capture stream.
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum StreamState {
    /// Opening the device / negotiating the format.
    Starting,
    /// Delivering frames.
    Running,
    /// Temporarily suspended (app backgrounded, `pause_camera`).
    Paused,
    /// Stopped by the app (`stop_camera`) or torn down.
    Stopped,
    /// Failed - see the stream's [`CaptureErrorCode`].
    Error,
}

/// Rotation / mirroring the capture needs relative to the display (the
/// sensor's native orientation rarely matches the UI's).
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum CaptureOrientation {
    /// Upright (0°).
    Up,
    /// Upside down (180°).
    Down,
    /// Rotated 90° counter-clockwise.
    Left,
    /// Rotated 90° clockwise.
    Right,
    /// Horizontally mirrored (typical for the front camera).
    Mirror,
}

/// Why a capture stream failed ([`StreamState::Error`]).
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum CaptureErrorCode {
    /// The user denied (or hasn't granted) camera permission.
    PermissionDenied,
    /// No camera matched the requested [`CameraFacing`].
    DeviceUnavailable,
    /// The device disappeared mid-capture (unplugged / claimed).
    DeviceLost,
    /// The requested format / resolution isn't supported.
    Unsupported,
    /// A platform error not covered above.
    Internal,
}

/// Requested capture configuration - the input to `start_camera`. Zero
/// `width`/`height`/`fps` mean "let the backend pick its default".
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CameraConfig {
    /// Which camera to open.
    pub facing: CameraFacing,
    /// Preferred frame width in px (0 = backend default).
    pub width: u32,
    /// Preferred frame height in px (0 = backend default).
    pub height: u32,
    /// Preferred frame rate (0 = backend default).
    pub fps: u32,
    /// Texture format the backend should deliver. `BGRA8` is the portable
    /// default; `Nv12` (a later `RawImageFormat` addition) is the zero-copy
    /// path on platforms that produce it natively.
    pub output_format: RawImageFormat,
}

impl Default for CameraConfig {
    fn default() -> Self {
        Self {
            facing: CameraFacing::Back,
            width: 0,
            height: 0,
            fps: 0,
            output_format: RawImageFormat::BGRA8,
        }
    }
}

impl CameraConfig {
    /// A default config for the given `facing` (backend-chosen size/fps,
    /// `BGRA8`).
    #[must_use] pub fn new(facing: CameraFacing) -> Self {
        Self {
            facing,
            ..Self::default()
        }
    }
}

/// Runtime stats for a capture stream - surfaced for HUD / debugging.
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CaptureStats {
    /// Measured delivery rate (frames/s), smoothed by the backend.
    pub measured_fps: f32,
    /// Frames delivered to the texture since the stream started.
    pub frames_delivered: u64,
    /// Frames the backend dropped (couldn't keep up / late).
    pub frames_dropped: u64,
}

impl Default for CaptureStats {
    fn default() -> Self {
        Self {
            measured_fps: 0.0,
            frames_delivered: 0,
            frames_dropped: 0,
        }
    }
}

#[cfg(test)]
mod autotest_generated {
    use super::*;

    const ALL_FACINGS: [CameraFacing; 3] =
        [CameraFacing::Front, CameraFacing::Back, CameraFacing::External];

    // ---- CameraConfig::new — constructor: no_panic + invariants_hold ----

    #[test]
    fn new_does_not_panic_for_any_facing() {
        // Constructor over every representative argument must not panic.
        for &facing in &ALL_FACINGS {
            let _ = CameraConfig::new(facing);
        }
    }

    #[test]
    fn new_sets_facing_and_leaves_everything_else_at_default() {
        // Documented contract: default config for the given `facing`
        // (backend-chosen size/fps, BGRA8). So new(f) must differ from
        // Default only in `facing`.
        let def = CameraConfig::default();
        for &facing in &ALL_FACINGS {
            let cfg = CameraConfig::new(facing);
            assert_eq!(cfg.facing, facing, "facing must equal the argument");
            assert_eq!(cfg.width, def.width);
            assert_eq!(cfg.height, def.height);
            assert_eq!(cfg.fps, def.fps);
            assert_eq!(cfg.output_format, def.output_format);
            // Absolute invariants (not just "== default"):
            assert_eq!(cfg.width, 0, "0 => backend default width");
            assert_eq!(cfg.height, 0, "0 => backend default height");
            assert_eq!(cfg.fps, 0, "0 => backend default fps");
            assert_eq!(cfg.output_format, RawImageFormat::BGRA8);
        }
    }

    #[test]
    fn new_equals_struct_update_syntax() {
        // new(f) must be observably identical to `{ facing: f, ..default() }`.
        for &facing in &ALL_FACINGS {
            let via_new = CameraConfig::new(facing);
            let via_update = CameraConfig {
                facing,
                ..CameraConfig::default()
            };
            assert_eq!(via_new, via_update);
        }
    }

    #[test]
    fn new_back_equals_full_default() {
        // Default facing is Back, so new(Back) must equal Default entirely.
        assert_eq!(
            CameraConfig::new(CameraFacing::Back),
            CameraConfig::default()
        );
    }

    #[test]
    fn new_result_is_copy_and_independent() {
        // CameraConfig is Copy; mutating a copy must not touch the original.
        let a = CameraConfig::new(CameraFacing::Front);
        let mut b = a;
        b.width = 1920;
        b.facing = CameraFacing::Back;
        assert_eq!(a.width, 0, "original must be untouched by copy mutation");
        assert_eq!(a.facing, CameraFacing::Front);
        assert_eq!(b.width, 1920);
        assert_eq!(b.facing, CameraFacing::Back);
    }

    #[test]
    fn new_differs_only_by_facing_between_variants() {
        // Two configs built from different facings must be unequal, and equal
        // once their facings are aligned (proves facing is the only variance).
        let front = CameraConfig::new(CameraFacing::Front);
        let mut back = CameraConfig::new(CameraFacing::Back);
        assert_ne!(front, back);
        back.facing = CameraFacing::Front;
        assert_eq!(front, back);
    }

    // ---- CameraConfig field storage: no clamping / no overflow on extremes ----

    #[test]
    fn camera_config_stores_extreme_dimensions_verbatim() {
        // These are POD "requested" values; the type must not clamp/saturate.
        let cfg = CameraConfig {
            width: u32::MAX,
            height: u32::MAX,
            fps: u32::MAX,
            ..CameraConfig::new(CameraFacing::External)
        };
        assert_eq!(cfg.width, u32::MAX);
        assert_eq!(cfg.height, u32::MAX);
        assert_eq!(cfg.fps, u32::MAX);
        assert_eq!(cfg.facing, CameraFacing::External);
    }

    #[test]
    fn camera_config_output_format_can_hold_non_default() {
        // new() forces BGRA8, but the struct must faithfully hold any format.
        let cfg = CameraConfig {
            output_format: RawImageFormat::RGBA8,
            ..CameraConfig::new(CameraFacing::Front)
        };
        assert_eq!(cfg.output_format, RawImageFormat::RGBA8);
        assert_ne!(cfg.output_format, RawImageFormat::BGRA8);
    }

    // ---- Default invariants ----

    #[test]
    fn camera_config_default_is_back_bgra8_zeroed() {
        let d = CameraConfig::default();
        assert_eq!(d.facing, CameraFacing::Back);
        assert_eq!(d.width, 0);
        assert_eq!(d.height, 0);
        assert_eq!(d.fps, 0);
        assert_eq!(d.output_format, RawImageFormat::BGRA8);
    }

    #[test]
    fn camera_config_default_is_idempotent() {
        assert_eq!(CameraConfig::default(), CameraConfig::default());
    }

    #[test]
    fn capture_stats_default_is_fully_zeroed() {
        let s = CaptureStats::default();
        assert_eq!(s.measured_fps, 0.0);
        assert_eq!(s.frames_delivered, 0);
        assert_eq!(s.frames_dropped, 0);
    }

    // ---- CaptureStats numeric / NaN adversarial (PartialEq over f32, NOT Eq) ----

    #[test]
    fn capture_stats_nan_fps_breaks_eq_reflexivity() {
        // measured_fps is f32; NaN != NaN, so a stats value carrying NaN must
        // not compare equal to itself. This confirms CaptureStats is (correctly)
        // PartialEq and not Eq.
        let s = CaptureStats {
            measured_fps: f32::NAN,
            ..CaptureStats::default()
        };
        assert_ne!(s, s, "NaN measured_fps must break PartialEq reflexivity");
        assert!(s.measured_fps.is_nan());
    }

    #[test]
    fn capture_stats_holds_infinities_and_saturated_counters() {
        let s = CaptureStats {
            measured_fps: f32::INFINITY,
            frames_delivered: u64::MAX,
            frames_dropped: u64::MAX,
        };
        assert!(s.measured_fps.is_infinite() && s.measured_fps > 0.0);
        assert_eq!(s.frames_delivered, u64::MAX);
        assert_eq!(s.frames_dropped, u64::MAX);

        let neg = CaptureStats {
            measured_fps: f32::NEG_INFINITY,
            ..CaptureStats::default()
        };
        assert!(neg.measured_fps.is_infinite() && neg.measured_fps < 0.0);
    }

    #[test]
    fn capture_stats_negative_zero_fps_equals_zero() {
        // IEEE: -0.0 == 0.0 for PartialEq; assert the type doesn't surprise us.
        let s = CaptureStats {
            measured_fps: -0.0,
            ..CaptureStats::default()
        };
        assert_eq!(s, CaptureStats::default());
    }

    // ---- CaptureStreamId round-trips its id over boundary values ----

    #[test]
    fn capture_stream_id_roundtrips_boundary_ids() {
        for id in [0u64, 1, 2, u64::MAX / 2, u64::MAX - 1, u64::MAX] {
            let s = CaptureStreamId { id };
            assert_eq!(s.id, id);
            assert_eq!(s, CaptureStreamId { id }, "Eq must hold for equal ids");
        }
    }

    #[test]
    fn capture_stream_id_distinct_ids_are_unequal_and_ordered() {
        let lo = CaptureStreamId { id: 0 };
        let hi = CaptureStreamId { id: u64::MAX };
        assert_ne!(lo, hi);
        assert!(lo < hi, "Ord must follow the inner u64");
    }

    // ---- Enum ordering / total-order / hash-eq contract (derived, but assert it) ----

    #[test]
    fn enum_ord_matches_declaration_order() {
        // CameraFacing: Front < Back < External.
        assert!(CameraFacing::Front < CameraFacing::Back);
        assert!(CameraFacing::Back < CameraFacing::External);
        // StreamState: Starting < Running < Paused < Stopped < Error.
        assert!(StreamState::Starting < StreamState::Running);
        assert!(StreamState::Running < StreamState::Paused);
        assert!(StreamState::Paused < StreamState::Stopped);
        assert!(StreamState::Stopped < StreamState::Error);
        // CaptureOrientation: Up < Down < Left < Right < Mirror.
        assert!(CaptureOrientation::Up < CaptureOrientation::Down);
        assert!(CaptureOrientation::Down < CaptureOrientation::Left);
        assert!(CaptureOrientation::Left < CaptureOrientation::Right);
        assert!(CaptureOrientation::Right < CaptureOrientation::Mirror);
        // CaptureErrorCode: PermissionDenied < DeviceUnavailable < DeviceLost
        //                   < Unsupported < Internal.
        assert!(CaptureErrorCode::PermissionDenied < CaptureErrorCode::DeviceUnavailable);
        assert!(CaptureErrorCode::DeviceUnavailable < CaptureErrorCode::DeviceLost);
        assert!(CaptureErrorCode::DeviceLost < CaptureErrorCode::Unsupported);
        assert!(CaptureErrorCode::Unsupported < CaptureErrorCode::Internal);
    }

    #[test]
    fn camera_facing_ord_is_total_and_antisymmetric() {
        use core::cmp::Ordering;
        for &a in &ALL_FACINGS {
            assert_eq!(a.cmp(&a), Ordering::Equal, "reflexive equality");
            for &b in &ALL_FACINGS {
                // Exactly one of <, ==, > holds (trichotomy / total order).
                let lt = (a < b) as u8;
                let eq = (a == b) as u8;
                let gt = (a > b) as u8;
                assert_eq!(lt + eq + gt, 1, "trichotomy must hold for ({a:?},{b:?})");
                // Antisymmetry: a < b implies !(b < a).
                if a < b {
                    assert!((b >= a), "antisymmetry violated for ({a:?},{b:?})");
                }
            }
        }
    }

    #[test]
    fn enum_hash_eq_consistency() {
        use std::collections::HashSet;
        let mut set = HashSet::new();
        assert!(set.insert(CameraFacing::Front));
        assert!(set.insert(CameraFacing::Back));
        assert!(set.insert(CameraFacing::External));
        // Re-inserting an equal value must be rejected (Hash agrees with Eq).
        assert!(!set.insert(CameraFacing::Front));
        assert_eq!(set.len(), 3);
        assert!(set.contains(&CameraFacing::Back));
    }

    // =====================================================================
    // Appended adversarial tests.
    //
    // NOTE: the module docs mention an `Nv12` output format, but it is an
    // explicitly deferred `RawImageFormat` addition and does not exist yet -
    // it is therefore un-constructible and deliberately NOT tested here.
    // =====================================================================

    const ALL_STATES: [StreamState; 5] = [
        StreamState::Starting,
        StreamState::Running,
        StreamState::Paused,
        StreamState::Stopped,
        StreamState::Error,
    ];

    const ALL_ORIENTATIONS: [CaptureOrientation; 5] = [
        CaptureOrientation::Up,
        CaptureOrientation::Down,
        CaptureOrientation::Left,
        CaptureOrientation::Right,
        CaptureOrientation::Mirror,
    ];

    const ALL_ERRORS: [CaptureErrorCode; 5] = [
        CaptureErrorCode::PermissionDenied,
        CaptureErrorCode::DeviceUnavailable,
        CaptureErrorCode::DeviceLost,
        CaptureErrorCode::Unsupported,
        CaptureErrorCode::Internal,
    ];

    /// Every `RawImageFormat` a `CameraConfig` could carry. `Nv12` is absent
    /// on purpose (see the note above).
    const ALL_FORMATS: [RawImageFormat; 12] = [
        RawImageFormat::R8,
        RawImageFormat::RG8,
        RawImageFormat::RGB8,
        RawImageFormat::RGBA8,
        RawImageFormat::R16,
        RawImageFormat::RG16,
        RawImageFormat::RGB16,
        RawImageFormat::RGBA16,
        RawImageFormat::BGR8,
        RawImageFormat::BGRA8,
        RawImageFormat::RGBF32,
        RawImageFormat::RGBAF32,
    ];

    // ---- Round-trip: enum -> FFI discriminant -> enum (encode == decode) ----
    //
    // These are `#[repr(C)]` fieldless enums that cross the FFI boundary, so
    // their discriminants ARE the wire format. A reordered / inserted variant
    // silently re-numbers the C ABI; these round-trips pin it down. The
    // `match` arms are exhaustive, so adding a variant fails to compile rather
    // than silently shipping an untested value.

    fn facing_from_discriminant(i: i32) -> Option<CameraFacing> {
        match i {
            0 => Some(CameraFacing::Front),
            1 => Some(CameraFacing::Back),
            2 => Some(CameraFacing::External),
            _ => None,
        }
    }

    fn state_from_discriminant(i: i32) -> Option<StreamState> {
        match i {
            0 => Some(StreamState::Starting),
            1 => Some(StreamState::Running),
            2 => Some(StreamState::Paused),
            3 => Some(StreamState::Stopped),
            4 => Some(StreamState::Error),
            _ => None,
        }
    }

    fn orientation_from_discriminant(i: i32) -> Option<CaptureOrientation> {
        match i {
            0 => Some(CaptureOrientation::Up),
            1 => Some(CaptureOrientation::Down),
            2 => Some(CaptureOrientation::Left),
            3 => Some(CaptureOrientation::Right),
            4 => Some(CaptureOrientation::Mirror),
            _ => None,
        }
    }

    fn error_from_discriminant(i: i32) -> Option<CaptureErrorCode> {
        match i {
            0 => Some(CaptureErrorCode::PermissionDenied),
            1 => Some(CaptureErrorCode::DeviceUnavailable),
            2 => Some(CaptureErrorCode::DeviceLost),
            3 => Some(CaptureErrorCode::Unsupported),
            4 => Some(CaptureErrorCode::Internal),
            _ => None,
        }
    }

    #[test]
    fn enum_discriminants_are_the_pinned_ffi_wire_values() {
        // If this fails, the C ABI changed: every C/Python/Kotlin caller that
        // passed an integer facing/state/orientation/error is now wrong.
        assert_eq!(CameraFacing::Front as i32, 0);
        assert_eq!(CameraFacing::Back as i32, 1);
        assert_eq!(CameraFacing::External as i32, 2);

        assert_eq!(StreamState::Starting as i32, 0);
        assert_eq!(StreamState::Running as i32, 1);
        assert_eq!(StreamState::Paused as i32, 2);
        assert_eq!(StreamState::Stopped as i32, 3);
        assert_eq!(StreamState::Error as i32, 4);

        assert_eq!(CaptureOrientation::Up as i32, 0);
        assert_eq!(CaptureOrientation::Down as i32, 1);
        assert_eq!(CaptureOrientation::Left as i32, 2);
        assert_eq!(CaptureOrientation::Right as i32, 3);
        assert_eq!(CaptureOrientation::Mirror as i32, 4);

        assert_eq!(CaptureErrorCode::PermissionDenied as i32, 0);
        assert_eq!(CaptureErrorCode::DeviceUnavailable as i32, 1);
        assert_eq!(CaptureErrorCode::DeviceLost as i32, 2);
        assert_eq!(CaptureErrorCode::Unsupported as i32, 3);
        assert_eq!(CaptureErrorCode::Internal as i32, 4);
    }

    #[test]
    fn enum_discriminant_roundtrip_is_identity() {
        for &f in &ALL_FACINGS {
            assert_eq!(facing_from_discriminant(f as i32), Some(f));
        }
        for &s in &ALL_STATES {
            assert_eq!(state_from_discriminant(s as i32), Some(s));
        }
        for &o in &ALL_ORIENTATIONS {
            assert_eq!(orientation_from_discriminant(o as i32), Some(o));
        }
        for &e in &ALL_ERRORS {
            assert_eq!(error_from_discriminant(e as i32), Some(e));
        }
    }

    #[test]
    fn enum_decode_rejects_out_of_range_discriminants() {
        // A hostile / buggy FFI caller can hand over ANY i32. Decoding must
        // yield None, never a bogus variant (and never wrap around).
        for bad in [-1i32, 3, 5, 99, i32::MIN, i32::MAX] {
            assert_eq!(facing_from_discriminant(bad), None, "facing {bad}");
        }
        for bad in [-1i32, 5, 6, 255, i32::MIN, i32::MAX] {
            assert_eq!(state_from_discriminant(bad), None, "state {bad}");
            assert_eq!(orientation_from_discriminant(bad), None, "orient {bad}");
            assert_eq!(error_from_discriminant(bad), None, "error {bad}");
        }
        // Boundary: the last valid value decodes, one past it does not.
        assert!(facing_from_discriminant(2).is_some());
        assert!(facing_from_discriminant(3).is_none());
        assert!(state_from_discriminant(4).is_some());
        assert!(state_from_discriminant(5).is_none());
    }

    #[test]
    fn enum_discriminant_order_agrees_with_ord() {
        // Ord is derived from declaration order, which is also the FFI
        // numbering; the two must not drift apart.
        for &a in &ALL_STATES {
            for &b in &ALL_STATES {
                assert_eq!(
                    a < b,
                    (a as i32) < (b as i32),
                    "Ord and discriminant disagree for ({a:?}, {b:?})"
                );
            }
        }
    }

    // ---- Round-trip: struct -> fields -> struct (decompose == recompose) ----

    #[test]
    fn camera_config_field_roundtrip_is_identity_over_full_cross_product() {
        // Every facing x every format, with adversarial extents: taking a
        // config apart and rebuilding it must reproduce it exactly.
        for &facing in &ALL_FACINGS {
            for &output_format in &ALL_FORMATS {
                for &(width, height, fps) in &[
                    (0u32, 0u32, 0u32),
                    (1, 1, 1),
                    (1920, 1080, 60),
                    (u32::MAX, u32::MAX, u32::MAX),
                    (u32::MAX, 0, 1),
                ] {
                    let cfg = CameraConfig {
                        facing,
                        width,
                        height,
                        fps,
                        output_format,
                    };
                    let rebuilt = CameraConfig {
                        facing: cfg.facing,
                        width: cfg.width,
                        height: cfg.height,
                        fps: cfg.fps,
                        output_format: cfg.output_format,
                    };
                    assert_eq!(cfg, rebuilt);
                    // ...and the fields survived verbatim - no clamping.
                    assert_eq!(rebuilt.width, width);
                    assert_eq!(rebuilt.height, height);
                    assert_eq!(rebuilt.fps, fps);
                    assert_eq!(rebuilt.facing, facing);
                    assert_eq!(rebuilt.output_format, output_format);
                }
            }
        }
    }

    #[test]
    fn camera_config_does_not_confuse_width_and_height() {
        // Guards against a transposed-field bug in any future manual PartialEq
        // or FFI struct layout: a config is NOT equal to its transpose.
        let a = CameraConfig {
            width: 1920,
            height: 1080,
            ..CameraConfig::default()
        };
        let transposed = CameraConfig {
            width: 1080,
            height: 1920,
            ..CameraConfig::default()
        };
        assert_ne!(a, transposed, "width/height must be distinguishable");
        // A square config IS its own transpose - sanity check on the above.
        let square = CameraConfig {
            width: 512,
            height: 512,
            ..CameraConfig::default()
        };
        let square2 = CameraConfig {
            width: 512,
            height: 512,
            ..CameraConfig::default()
        };
        assert_eq!(square, square2);
    }

    #[test]
    fn camera_config_eq_is_sensitive_to_every_field() {
        // Flipping any single field must break equality - i.e. no field is
        // accidentally excluded from the derived PartialEq.
        let base = CameraConfig::default();
        assert_ne!(
            base,
            CameraConfig {
                facing: CameraFacing::Front,
                ..base
            }
        );
        assert_ne!(base, CameraConfig { width: 1, ..base });
        assert_ne!(base, CameraConfig { height: 1, ..base });
        assert_ne!(base, CameraConfig { fps: 1, ..base });
        assert_ne!(
            base,
            CameraConfig {
                output_format: RawImageFormat::RGBA8,
                ..base
            }
        );
    }

    #[test]
    fn capture_stats_eq_is_sensitive_to_every_field() {
        let base = CaptureStats::default();
        assert_ne!(
            base,
            CaptureStats {
                measured_fps: 1.0,
                ..base
            }
        );
        assert_ne!(
            base,
            CaptureStats {
                frames_delivered: 1,
                ..base
            }
        );
        assert_ne!(
            base,
            CaptureStats {
                frames_dropped: 1,
                ..base
            }
        );
    }

    #[test]
    // These types are Copy; calling .clone() explicitly is the point of the
    // test (Clone must agree with Copy), so the lint is deliberately waived.
    #[allow(clippy::clone_on_copy)]
    fn clone_is_identity_for_all_pod_types_at_extremes() {
        let cfg = CameraConfig {
            facing: CameraFacing::External,
            width: u32::MAX,
            height: u32::MAX,
            fps: u32::MAX,
            output_format: RawImageFormat::RGBAF32,
        };
        assert_eq!(cfg.clone(), cfg);

        let id = CaptureStreamId { id: u64::MAX };
        assert_eq!(id.clone(), id);

        // NaN is excluded here on purpose: clone of NaN cannot compare equal
        // (see capture_stats_nan_fps_breaks_eq_reflexivity); assert bit
        // equality instead, which IS reflexive.
        let nan_stats = CaptureStats {
            measured_fps: f32::NAN,
            frames_delivered: u64::MAX,
            frames_dropped: u64::MAX,
        };
        let cloned = nan_stats.clone();
        assert_eq!(
            cloned.measured_fps.to_bits(),
            nan_stats.measured_fps.to_bits(),
            "clone must preserve the exact NaN bit pattern"
        );
        assert_eq!(cloned.frames_delivered, u64::MAX);
        assert_eq!(cloned.frames_dropped, u64::MAX);
    }

    // ---- Numeric: NaN ordering, saturation, overflow, precision limits ----

    #[test]
    fn capture_stats_nan_fps_makes_partial_cmp_none() {
        // CaptureStats is only PartialEq (no Ord) precisely because of the f32.
        // A NaN fps must yield an *incomparable* float, not a silent ordering.
        let nan = f32::NAN;
        assert!(nan.partial_cmp(&0.0).is_none());
        assert!(nan.partial_cmp(&nan).is_none());

        let s = CaptureStats {
            measured_fps: nan,
            ..CaptureStats::default()
        };
        // A NaN-carrying stats value can never be *found* by equality search,
        // so consumers must not rely on `contains` / dedup for it.
        let haystack = [s, CaptureStats::default()];
        assert!(!haystack.contains(&s), "NaN stats is unfindable by Eq");
        assert!(haystack.contains(&CaptureStats::default()));
    }

    #[test]
    fn measured_fps_to_integer_cast_saturates_instead_of_wrapping() {
        // HUD code casting measured_fps to an integer must not hit UB or wrap.
        // Rust `as` casts are saturating: NaN -> 0, +inf -> MAX, negative -> 0.
        let cases: [(f32, u32); 6] = [
            (f32::NAN, 0),
            (f32::INFINITY, u32::MAX),
            (f32::NEG_INFINITY, 0),
            (-1.0, 0),
            (-0.0, 0),
            (f32::MAX, u32::MAX),
        ];
        for (fps, expected) in cases {
            let s = CaptureStats {
                measured_fps: fps,
                ..CaptureStats::default()
            };
            assert_eq!(
                s.measured_fps as u32, expected,
                "cast of {fps} must saturate to {expected}"
            );
        }
        // Ordinary value is unchanged (truncates toward zero).
        let ok = CaptureStats {
            measured_fps: 59.94,
            ..CaptureStats::default()
        };
        assert_eq!(ok.measured_fps as u32, 59);
    }

    #[test]
    fn frame_counters_overflow_only_via_checked_arithmetic() {
        // `frames_delivered + frames_dropped` is the obvious "total frames"
        // consumers will compute - at the saturated boundary it overflows.
        let s = CaptureStats {
            measured_fps: 30.0,
            frames_delivered: u64::MAX,
            frames_dropped: 1,
        };
        assert_eq!(
            s.frames_delivered.checked_add(s.frames_dropped),
            None,
            "total-frames overflows u64; consumers must saturate/check"
        );
        assert_eq!(
            s.frames_delivered.saturating_add(s.frames_dropped),
            u64::MAX,
            "saturating_add is the safe path"
        );
        // One below the boundary is fine.
        let ok = CaptureStats {
            frames_delivered: u64::MAX - 1,
            frames_dropped: 1,
            ..CaptureStats::default()
        };
        assert_eq!(
            ok.frames_delivered.checked_add(ok.frames_dropped),
            Some(u64::MAX)
        );
    }

    #[test]
    fn drop_rate_over_zeroed_stats_must_be_guarded() {
        // Default stats have delivered == dropped == 0, so the natural drop
        // rate `dropped / (delivered + dropped)` divides by zero: integer
        // division would PANIC, float division yields NaN. Assert both, so the
        // hazard is pinned rather than discovered in a HUD at runtime.
        let s = CaptureStats::default();
        let total = s.frames_delivered + s.frames_dropped;
        assert_eq!(total, 0);
        assert_eq!(
            s.frames_dropped.checked_div(total),
            None,
            "integer drop-rate on zeroed stats must be checked, not raw `/`"
        );
        let rate = s.frames_dropped as f64 / total as f64;
        assert!(rate.is_nan(), "float drop-rate on zeroed stats is NaN");

        // With traffic, the rate is well-defined.
        let live = CaptureStats {
            measured_fps: 30.0,
            frames_delivered: 75,
            frames_dropped: 25,
        };
        let total = live.frames_delivered + live.frames_dropped;
        assert_eq!(live.frames_dropped.checked_div(total), Some(0));
        assert!(((live.frames_dropped as f64 / total as f64) - 0.25).abs() < 1e-12);
    }

    #[test]
    fn config_pixel_count_overflows_u32_at_extreme_dimensions() {
        // width * height is the natural buffer-size computation. At the
        // documented-legal extremes it overflows u32 and must be widened.
        let cfg = CameraConfig {
            width: u32::MAX,
            height: u32::MAX,
            ..CameraConfig::default()
        };
        assert_eq!(
            cfg.width.checked_mul(cfg.height),
            None,
            "u32 pixel count overflows; widen to u64 before multiplying"
        );
        assert_eq!(
            u64::from(cfg.width) * u64::from(cfg.height),
            (u64::from(u32::MAX)) * (u64::from(u32::MAX)),
            "u64 widening is the safe path"
        );
        // A realistic size does not overflow.
        let hd = CameraConfig {
            width: 1920,
            height: 1080,
            ..CameraConfig::default()
        };
        assert_eq!(hd.width.checked_mul(hd.height), Some(2_073_600));
    }

    #[test]
    fn fps_u32_to_f32_roundtrip_is_lossy_above_2_pow_24() {
        // The config's requested `fps` is u32 while the reported `measured_fps`
        // is f32. Comparing them via a cast is exact only below 2^24.
        let exact: u32 = 1 << 24; // 16_777_216
        assert_eq!(exact as f32 as u32, exact, "2^24 round-trips exactly");

        let lossy: u32 = (1 << 24) + 1; // 16_777_217
        assert_ne!(
            lossy as f32 as u32,
            lossy,
            "2^24+1 must NOT round-trip through f32 - precision is lost"
        );
        assert_eq!(lossy as f32 as u32, exact, "it rounds down to 2^24");

        // Realistic fps values are far below the boundary and are exact.
        for fps in [0u32, 1, 24, 30, 60, 120, 240] {
            let cfg = CameraConfig {
                fps,
                ..CameraConfig::default()
            };
            assert_eq!(cfg.fps as f32 as u32, fps, "fps {fps} must be exact");
        }
    }

    #[test]
    fn frames_delivered_u64_to_f32_roundtrip_is_lossy_at_the_boundary() {
        // Same hazard on the counter side: a HUD computing fps as
        // frames_delivered as f32 / secs loses counts past 2^24.
        let s = CaptureStats {
            frames_delivered: (1 << 24) + 1,
            ..CaptureStats::default()
        };
        assert_ne!(s.frames_delivered as f32 as u64, s.frames_delivered);
        // u64::MAX -> f32 rounds UP past u64::MAX, and the cast back saturates.
        let sat = CaptureStats {
            frames_delivered: u64::MAX,
            ..CaptureStats::default()
        };
        assert_eq!(
            sat.frames_delivered as f32 as u64,
            u64::MAX,
            "saturating cast clamps rather than wrapping to 0"
        );
    }

    // ---- Predicates / invariants over the whole variant space ----

    #[test]
    fn all_enum_variants_are_pairwise_distinct_and_hash_consistently() {
        use std::collections::HashSet;
        assert_eq!(
            ALL_STATES.iter().collect::<HashSet<_>>().len(),
            ALL_STATES.len(),
            "StreamState variants must be pairwise distinct"
        );
        assert_eq!(
            ALL_ORIENTATIONS.iter().collect::<HashSet<_>>().len(),
            ALL_ORIENTATIONS.len(),
            "CaptureOrientation variants must be pairwise distinct"
        );
        assert_eq!(
            ALL_ERRORS.iter().collect::<HashSet<_>>().len(),
            ALL_ERRORS.len(),
            "CaptureErrorCode variants must be pairwise distinct"
        );
        assert_eq!(
            ALL_FORMATS.iter().collect::<HashSet<_>>().len(),
            ALL_FORMATS.len(),
            "RawImageFormat variants must be pairwise distinct"
        );
    }

    #[test]
    fn ord_is_a_total_order_for_every_camera_enum() {
        use core::cmp::Ordering;
        // Trichotomy + antisymmetry + transitivity, exhaustively over the
        // (small) variant spaces. CameraFacing is already covered above.
        macro_rules! assert_total_order {
            ($set:expr) => {{
                for &a in $set {
                    assert_eq!(a.cmp(&a), Ordering::Equal);
                    for &b in $set {
                        let lt = (a < b) as u8;
                        let eq = (a == b) as u8;
                        let gt = (a > b) as u8;
                        assert_eq!(lt + eq + gt, 1, "trichotomy ({a:?}, {b:?})");
                        if a < b {
                            assert!(!(b < a), "antisymmetry ({a:?}, {b:?})");
                        }
                        for &c in $set {
                            if a < b && b < c {
                                assert!(a < c, "transitivity ({a:?}, {b:?}, {c:?})");
                            }
                        }
                    }
                }
            }};
        }
        assert_total_order!(&ALL_STATES);
        assert_total_order!(&ALL_ORIENTATIONS);
        assert_total_order!(&ALL_ERRORS);
    }

    #[test]
    fn capture_stream_id_hash_eq_and_copy_independence() {
        use std::collections::HashSet;
        let mut set = HashSet::new();
        for id in [0u64, 1, u64::MAX / 2, u64::MAX] {
            assert!(set.insert(CaptureStreamId { id }), "id {id} is new");
        }
        assert_eq!(set.len(), 4);
        // Re-inserting an equal id must be rejected (Hash agrees with Eq).
        assert!(!set.insert(CaptureStreamId { id: u64::MAX }));
        assert!(set.contains(&CaptureStreamId { id: 0 }));

        // Copy: mutating the copy must not disturb the original.
        let a = CaptureStreamId { id: 7 };
        let mut b = a;
        b.id = 9;
        assert_eq!(a.id, 7);
        assert_eq!(b.id, 9);
    }

    #[test]
    fn capture_stats_copy_is_independent() {
        let a = CaptureStats {
            measured_fps: 30.0,
            frames_delivered: 100,
            frames_dropped: 2,
        };
        let mut b = a;
        b.measured_fps = 0.0;
        b.frames_delivered = 0;
        b.frames_dropped = 0;
        assert_eq!(a.measured_fps, 30.0, "original must survive copy mutation");
        assert_eq!(a.frames_delivered, 100);
        assert_eq!(a.frames_dropped, 2);
        assert_eq!(b, CaptureStats::default());
    }

    #[test]
    fn debug_formatting_never_panics_on_extreme_or_nan_values() {
        // Debug is what a crash report / HUD log prints; it must survive the
        // adversarial values above.
        let stats = CaptureStats {
            measured_fps: f32::NAN,
            frames_delivered: u64::MAX,
            frames_dropped: u64::MAX,
        };
        assert!(!format!("{stats:?}").is_empty());
        assert!(!format!("{:?}", CaptureStats {
            measured_fps: f32::NEG_INFINITY,
            ..CaptureStats::default()
        })
        .is_empty());

        let cfg = CameraConfig {
            facing: CameraFacing::External,
            width: u32::MAX,
            height: u32::MAX,
            fps: u32::MAX,
            output_format: RawImageFormat::RGBAF32,
        };
        let s = format!("{cfg:?}");
        assert!(s.contains("External"), "Debug must name the facing: {s}");
        assert!(s.contains("4294967295"), "Debug must show raw extents: {s}");

        assert!(!format!("{:?}", CaptureStreamId { id: u64::MAX }).is_empty());
        for &st in &ALL_STATES {
            assert!(!format!("{st:?}").is_empty());
        }
        for &o in &ALL_ORIENTATIONS {
            assert!(!format!("{o:?}").is_empty());
        }
        for &e in &ALL_ERRORS {
            assert!(!format!("{e:?}").is_empty());
        }
    }

    #[test]
    fn repr_c_layout_invariants_hold_for_the_ffi_pod_types() {
        use core::mem::{align_of, size_of};
        // Single-field #[repr(C)] newtype: must be exactly its payload, so it
        // can be passed across the FFI as a bare u64.
        assert_eq!(size_of::<CaptureStreamId>(), size_of::<u64>());
        assert_eq!(align_of::<CaptureStreamId>(), align_of::<u64>());

        // No exact byte sizes asserted for the aggregates: u64 alignment (and
        // therefore trailing padding) differs on 32-bit targets. Assert the
        // portable invariants instead - big enough for the payload, and a
        // whole number of alignment units (a valid array element).
        assert!(size_of::<CaptureStats>() >= size_of::<f32>() + 2 * size_of::<u64>());
        assert_eq!(size_of::<CaptureStats>() % align_of::<CaptureStats>(), 0);

        assert!(size_of::<CameraConfig>() >= 3 * size_of::<u32>());
        assert_eq!(size_of::<CameraConfig>() % align_of::<CameraConfig>(), 0);

        // Fieldless repr(C) enums are C ints - never zero-sized, never huge.
        assert_eq!(size_of::<CameraFacing>(), size_of::<StreamState>());
        assert_eq!(size_of::<StreamState>(), size_of::<CaptureOrientation>());
        assert_eq!(size_of::<CaptureOrientation>(), size_of::<CaptureErrorCode>());
        assert!(size_of::<CameraFacing>() > 0);
    }
}
