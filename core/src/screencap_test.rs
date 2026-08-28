#[cfg(test)]
mod autotest_generated {
    use core::mem::size_of;

    use super::*;

    /// Representative + extreme sources: both payload boundaries (0, MAX) for
    /// each carrying variant, so a truncating/aliasing constructor cannot pass.
    const ALL_SOURCES: [ScreenCaptureSource; 7] = [
        ScreenCaptureSource::PrimaryDisplay,
        ScreenCaptureSource::Display(0),
        ScreenCaptureSource::Display(1),
        ScreenCaptureSource::Display(u32::MAX),
        ScreenCaptureSource::Window(0),
        ScreenCaptureSource::Window(1),
        ScreenCaptureSource::Window(u64::MAX),
    ];

    // ---- ScreenCaptureConfig::new — constructor: no_panic ----

    #[test]
    fn new_does_not_panic_for_representative_or_extreme_sources() {
        for &source in &ALL_SOURCES {
            let _ = ScreenCaptureConfig::new(source);
        }
    }

    #[test]
    fn new_is_deterministic() {
        // Same argument twice => observably identical configs (no hidden state,
        // no backend probing at construction time).
        for &source in &ALL_SOURCES {
            assert_eq!(
                ScreenCaptureConfig::new(source),
                ScreenCaptureConfig::new(source)
            );
        }
    }

    // ---- ScreenCaptureConfig::new — constructor: invariants_hold ----

    #[test]
    fn new_sets_source_and_leaves_everything_else_at_default() {
        // Documented contract: "a default config for the given `source`
        // (backend-chosen fps, BGRA8)" — so new(s) may differ from Default
        // only in `source`.
        let def = ScreenCaptureConfig::default();
        for &source in &ALL_SOURCES {
            let cfg = ScreenCaptureConfig::new(source);
            assert_eq!(cfg.source, source, "source must equal the argument");
            assert_eq!(cfg.fps, def.fps);
            assert_eq!(cfg.output_format, def.output_format);
            // Absolute invariants, not merely "== whatever Default says":
            assert_eq!(cfg.fps, 0, "0 => backend picks the frame rate");
            assert_eq!(
                cfg.output_format,
                RawImageFormat::BGRA8,
                "BGRA8 is the documented portable default"
            );
        }
    }

    #[test]
    fn new_equals_struct_update_syntax() {
        for &source in &ALL_SOURCES {
            let via_new = ScreenCaptureConfig::new(source);
            let via_update = ScreenCaptureConfig {
                source,
                ..ScreenCaptureConfig::default()
            };
            assert_eq!(via_new, via_update);
        }
    }

    #[test]
    fn default_config_is_new_of_default_source() {
        // The two `Default` impls (derived on the enum, hand-written on the
        // struct) must not drift apart.
        assert_eq!(
            ScreenCaptureConfig::default(),
            ScreenCaptureConfig::new(ScreenCaptureSource::default())
        );
        assert_eq!(
            ScreenCaptureSource::default(),
            ScreenCaptureSource::PrimaryDisplay
        );
        assert_eq!(
            ScreenCaptureConfig::default().source,
            ScreenCaptureSource::PrimaryDisplay
        );
    }

    // ---- source payload: round-trip through the constructor ----

    #[test]
    fn new_preserves_full_payload_width() {
        // A u64 window handle must survive intact: a `as u32` anywhere in the
        // pipeline would collapse u64::MAX to u32::MAX-in-a-u64.
        let cfg = ScreenCaptureConfig::new(ScreenCaptureSource::Window(u64::MAX));
        match cfg.source {
            ScreenCaptureSource::Window(h) => assert_eq!(h, u64::MAX),
            other => panic!("expected Window(u64::MAX), got {other:?}"),
        }

        let cfg = ScreenCaptureConfig::new(ScreenCaptureSource::Display(u32::MAX));
        match cfg.source {
            ScreenCaptureSource::Display(i) => assert_eq!(i, u32::MAX),
            other => panic!("expected Display(u32::MAX), got {other:?}"),
        }

        // Enough room for the widest payload; guards against a narrowed repr.
        assert!(size_of::<ScreenCaptureSource>() >= size_of::<u64>());
    }

    #[test]
    fn distinct_sources_stay_distinct() {
        // Equality must compare the tag too, not just the payload bits.
        assert_ne!(
            ScreenCaptureSource::Display(1),
            ScreenCaptureSource::Window(1)
        );
        // `PrimaryDisplay` is its own variant, NOT a spelling of `Display(0)`.
        assert_ne!(
            ScreenCaptureSource::PrimaryDisplay,
            ScreenCaptureSource::Display(0)
        );
        assert_ne!(
            ScreenCaptureConfig::new(ScreenCaptureSource::PrimaryDisplay),
            ScreenCaptureConfig::new(ScreenCaptureSource::Display(0))
        );

        // Pairwise: no two entries of ALL_SOURCES may collide.
        for (i, a) in ALL_SOURCES.iter().enumerate() {
            for (j, b) in ALL_SOURCES.iter().enumerate() {
                let cfg_a = ScreenCaptureConfig::new(*a);
                let cfg_b = ScreenCaptureConfig::new(*b);
                if i == j {
                    assert_eq!(cfg_a, cfg_b, "Eq must be reflexive for {a:?}");
                } else {
                    assert_ne!(cfg_a, cfg_b, "{a:?} and {b:?} must not compare equal");
                }
            }
        }
    }

    #[test]
    fn config_is_copy_and_fields_are_independently_settable() {
        // The struct is a POD crossing the FFI: mutating a copy must not
        // disturb the original, and overwriting `fps`/`output_format` must not
        // corrupt the (differently aligned) `source` payload next to it.
        let original = ScreenCaptureConfig::new(ScreenCaptureSource::Window(u64::MAX));
        let mut copy = original;
        copy.fps = u32::MAX;
        copy.output_format = RawImageFormat::RGBAF32;

        assert_eq!(original.fps, 0, "original must be untouched (Copy)");
        assert_eq!(original.output_format, RawImageFormat::BGRA8);
        assert_eq!(
            copy.source, original.source,
            "source must survive the writes"
        );
        assert_eq!(copy.fps, u32::MAX);
        assert_eq!(copy.output_format, RawImageFormat::RGBAF32);
    }
}
