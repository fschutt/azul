#[cfg(test)]
#[allow(
    clippy::items_after_statements,
    clippy::redundant_clone,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    trivial_casts,
    clippy::borrow_as_ptr,
    clippy::cast_ptr_alignment,
    clippy::unused_self,
    unused_qualifications,
    unreachable_pub,
    private_interfaces
)] // pedantic lints are noise in unsafe-exercising test code
mod tests {
    use super::*;

    #[test]
    fn normalize_u16_maps_full_range() {
        // 0 -> 0, u16::MAX -> u8::MAX, midpoint -> ~127/128, no div-by-zero.
        assert_eq!(normalize_u16(0), 0);
        assert_eq!(normalize_u16(u16::MAX), 255);
        // Half of u16::MAX should land at ~half of u8::MAX.
        let mid = normalize_u16(u16::MAX / 2);
        assert!((126..=128).contains(&mid), "midpoint normalized to {mid}");
        // Previously `(65535/i)*255` produced near-white garbage for small i;
        // a small input must now map to a small output.
        assert_eq!(normalize_u16(256), 0);
        assert_eq!(normalize_u16(257), 1);
    }

    #[test]
    fn load_bgra8_rejects_wrong_length() {
        // premultiplied branch: buffer shorter than expected must be rejected,
        // not silently accepted (previously missing length guard).
        let short = RawImageData::U8(vec![0u8; 4 * 3].into()); // 3 px worth
        assert!(RawImage::load_bgra8(short, 4, true).is_none());

        // correct length is accepted.
        let ok = RawImageData::U8(vec![255u8; 4 * 4].into()); // 4 px
        assert!(RawImage::load_bgra8(ok, 4, true).is_some());

        // non-premultiplied branch still rejects wrong length.
        let short2 = RawImageData::U8(vec![0u8; 4 * 2].into());
        assert!(RawImage::load_bgra8(short2, 4, false).is_none());
    }

    // --- unsafe-hardening tests (Miri-compatible: pure in-memory, no FFI/GL) ---

    #[test]
    fn imageref_get_data_reads_backing_box() {
        // Exercises the `&*self.data` raw-pointer deref in `get_data`.
        let img = ImageRef::null_image(2, 3, RawImageFormat::RGBA8, vec![7, 8]);
        match img.get_data() {
            DecodedImage::NullImage {
                width, height, tag, ..
            } => {
                assert_eq!((*width, *height), (2, 3));
                assert_eq!(tag.as_slice(), &[7, 8]);
            }
            _ => panic!("expected NullImage"),
        }
    }

    #[test]
    fn imageref_clone_shares_refcount_and_identity() {
        // Clone must bump the shared AtomicUsize (so `into_inner` refuses while a
        // second copy is alive) and preserve the never-reused identity `id`.
        let img = ImageRef::null_image(1, 1, RawImageFormat::R8, Vec::new());
        let c = img.clone();
        assert_eq!(img, c); // same id -> shallow clone
                            // Two live copies: sole-owner extraction must fail (and `c` drops cleanly).
        assert!(c.into_inner().is_none());
        // Back to one owner: extraction now succeeds, forgetting `self` without leak.
        assert!(img.into_inner().is_some());
    }

    #[test]
    fn imageref_deep_copy_has_distinct_identity() {
        // deep_copy allocates a fresh backing Box + fresh id -> not equal, independent
        // drop (Miri would flag any shared/double-freed allocation here).
        let img = ImageRef::null_image(4, 4, RawImageFormat::RGBA8, vec![1]);
        let d = img.deep_copy();
        assert_ne!(img, d);
        drop(img);
        // `d` still valid and independently readable after `img` freed.
        assert_eq!(d.get_size().width as usize, 4);
    }

    #[test]
    fn imageref_last_drop_frees_once() {
        // Clone then drop both: the refcount path must free the two Boxes exactly once
        // on the final drop. Under Miri a double free / leak fails the test.
        let img = ImageRef::null_image(1, 1, RawImageFormat::R8, Vec::new());
        let c = img.clone();
        drop(img);
        drop(c);
    }

    #[test]
    fn imageref_get_callback_none_for_non_callback_and_when_shared() {
        // `get_image_callback` derefs both `copies` and `data`; a NullImage yields None,
        // and a shared (copies != 1) handle also yields None.
        let img = ImageRef::null_image(1, 1, RawImageFormat::R8, Vec::new());
        assert!(img.get_image_callback().is_none());
        let c = img.clone();
        assert!(img.get_image_callback().is_none()); // shared -> not safe
        drop(c);
    }

    #[test]
    fn shared_raw_image_data_read_paths() {
        // Exercises as_ref / len / is_empty / as_ptr raw-pointer derefs.
        let s = SharedRawImageData::new(vec![10u8, 20, 30].into());
        assert_eq!(s.as_ref(), &[10, 20, 30]);
        assert_eq!(s.len(), 3);
        assert!(!s.is_empty());
        assert_eq!(unsafe { *s.as_ptr() }, 10);
        assert!(SharedRawImageData::new(Vec::<u8>::new().into()).is_empty());
    }

    #[test]
    fn shared_raw_image_data_clone_shares_alloc() {
        // Clone shares the backing Box (ptr-equal) and refcount; `into_inner` refuses
        // while a second copy lives, and succeeds once sole owner.
        let s = SharedRawImageData::new(vec![1u8, 2, 3, 4].into());
        let c = s.clone();
        assert_eq!(s, c); // ptr::eq on the shared `data`
        assert_eq!(s.as_ptr(), c.as_ptr());
        assert!(c.into_inner().is_none()); // two owners -> None, `c` drops to refcount 1
        let inner = s.into_inner().expect("sole owner extraction");
        assert_eq!(inner.as_ref(), &[1, 2, 3, 4]);
    }

    #[test]
    fn shared_raw_image_data_last_drop_frees_once() {
        // Refcounted drop must free both Boxes exactly once; Miri flags UB otherwise.
        let s = SharedRawImageData::new(vec![0u8; 8].into());
        let c = s.clone();
        drop(s);
        drop(c);
    }
}

#[cfg(test)]
#[allow(
    clippy::float_cmp,
    clippy::items_after_statements,
    clippy::redundant_clone,
    clippy::cast_possible_truncation,
    clippy::cast_precision_loss,
    clippy::cast_sign_loss,
    clippy::cast_lossless,
    clippy::unreadable_literal,
    clippy::too_many_lines,
    clippy::many_single_char_names,
    clippy::similar_names,
    unused_qualifications,
    unreachable_pub,
    private_interfaces
)] // pedantic lints are noise in adversarial test code
mod autotest_generated {
    use alloc::string::String;

    use super::*;

    // ---------------------------------------------------------------------
    // helpers
    // ---------------------------------------------------------------------

    /// A `FontRef` whose `parsed` pointer addresses a `'static` byte and whose
    /// destructor is a no-op, so nothing is freed on drop. Sound because
    /// `FontRef`'s `Hash`/`get_hash` only read the never-reused `id` and never
    /// dereference `parsed`.
    fn dummy_font_ref() -> FontRef {
        static DUMMY_FONT_DATA: u8 = 0;
        extern "C" fn dummy_destructor(_: *mut core::ffi::c_void) {}
        FontRef::new(
            core::ptr::addr_of!(DUMMY_FONT_DATA).cast::<core::ffi::c_void>(),
            dummy_destructor,
        )
    }

    /// `LoadFontFn` that never resolves a font (simulates a missing font file).
    fn load_font_none(_: &StyleFontFamily, _: &FcFontCache) -> Option<LoadedFontSource> {
        None
    }

    /// `ParseFontFn` that never parses (simulates a corrupt font file).
    fn parse_font_none(_: LoadedFontSource) -> Option<FontRef> {
        None
    }

    /// `GlStoreImageFn` no-op: never invoked for raw / null / callback images.
    fn store_gl_texture_noop(_: DocumentId, _: Epoch, _: Texture, _: ExternalImageId) {}

    fn test_document_id() -> DocumentId {
        DocumentId {
            namespace_id: IdNamespace(7),
            id: 0,
        }
    }

    /// An `RGBA8` image of `w * h` transparent-black pixels.
    fn rgba8_image(w: usize, h: usize) -> RawImage {
        RawImage {
            pixels: RawImageData::U8(vec![0u8; w * h * 4].into()),
            width: w,
            height: h,
            premultiplied_alpha: true,
            data_format: RawImageFormat::RGBA8,
            tag: Vec::new().into(),
        }
    }

    fn opaque_red() -> ColorU {
        ColorU {
            r: 255,
            g: 0,
            b: 0,
            a: 255,
        }
    }

    // =====================================================================
    // PARSERS: match_route / RouteMatch::get_param / AppConfig::match_route_for_path
    // =====================================================================

    #[test]
    fn match_route_valid_minimal_positive_control() {
        // Documented examples must hold (positive control).
        let m = match_route("/user/:id", "/user/42").expect("documented example must match");
        assert_eq!(m.pattern.as_str(), "/user/:id");
        assert_eq!(m.get_param("id").map(AzString::as_str), Some("42"));

        let root = match_route("/", "/").expect("root must match root");
        assert!(root.params.as_ref().is_empty());

        assert!(match_route("/about", "/settings").is_none());
    }

    #[test]
    fn match_route_empty_input_does_not_panic() {
        // Empty pattern/path degrade to zero segments; the segment-count check
        // makes "" and "/" equivalent (both filter to no segments).
        let m = match_route("", "").expect("empty vs empty is a zero-segment match");
        assert!(m.params.as_ref().is_empty());
        assert!(match_route("", "/").is_some()); // "/" also has zero segments
        assert!(match_route("/", "").is_some());
        assert!(match_route("", "/a").is_none()); // 0 segments != 1 segment
        assert!(match_route("/a", "").is_none());
    }

    #[test]
    fn match_route_whitespace_only_is_not_trimmed() {
        // Whitespace is NOT trimmed: it is an ordinary (opaque) path segment,
        // so it only matches itself. Deterministic, no panic.
        assert!(match_route("   ", "   ").is_some());
        assert!(match_route("   ", "\t\n").is_none());
        assert!(match_route("/ ", "/").is_none()); // " " is a real segment
        assert!(match_route("/\t\n", "/\t\n").is_some());
    }

    #[test]
    fn match_route_garbage_never_panics() {
        for pat in [
            "\0\0\0",
            "///////",
            "::::",
            ":",
            "%%%$#@!",
            "\u{feff}",
            "/a/../../etc/passwd",
        ] {
            for path in ["", "/", "\0", "/a/b/c", "%%%$#@!", "\u{feff}"] {
                // Only requirement: a total function that never panics.
                let _ = match_route(pat, path);
            }
        }
        // "///////" collapses to zero segments, so it matches the root.
        assert!(match_route("///////", "/").is_some());
        // A bare ":" is a param with an EMPTY name; the value is still captured.
        let m = match_route("/:", "/hello").expect("empty param name still matches");
        assert_eq!(m.get_param("").map(AzString::as_str), Some("hello"));
    }

    #[test]
    fn match_route_leading_trailing_junk_is_rejected_or_ignored() {
        // Trailing slashes produce empty segments that are filtered out, so a
        // trailing slash is ignored (deterministic).
        assert!(match_route("/user/:id/", "/user/42").is_some());
        assert!(match_route("/user/:id", "/user/42/").is_some());
        // Surrounding spaces are part of the segment -> rejected.
        assert!(match_route(" /about ", "/about").is_none());
        assert!(match_route("/about", "/about;garbage").is_none());
    }

    #[test]
    fn match_route_boundary_number_strings_are_opaque_segments() {
        // Numeric-looking params are never parsed; they round-trip verbatim.
        for v in [
            "0",
            "-0",
            "9223372036854775807",
            "-9223372036854775808",
            "1e400",
            "NaN",
            "inf",
            "-inf",
            "0.0000000000000000001",
        ] {
            let path = String::from("/user/") + v;
            let m = match_route("/user/:id", &path).expect("any segment matches a :param");
            assert_eq!(m.get_param("id").map(AzString::as_str), Some(v));
        }
    }

    #[test]
    fn match_route_unicode_multibyte_does_not_panic() {
        // Splitting on '/' is byte-safe for UTF-8; multibyte segments survive.
        let m = match_route("/user/:id", "/user/\u{1F600}").expect("emoji segment matches");
        assert_eq!(m.get_param("id").map(AzString::as_str), Some("\u{1F600}"));

        // Combining marks + RTL + a unicode param NAME.
        let m = match_route("/:é\u{0301}", "/e\u{0301}\u{202E}x").expect("unicode param name");
        assert_eq!(
            m.get_param("é\u{0301}").map(AzString::as_str),
            Some("e\u{0301}\u{202E}x")
        );
        // A unicode segment does not equal its NFC/NFD-different twin.
        assert!(match_route("/é", "/e\u{0301}").is_none());
    }

    #[test]
    fn match_route_extremely_long_input_does_not_hang() {
        // 1M-char single segment: linear split, no quadratic blowup / panic.
        let huge = String::from("/") + &"a".repeat(1_000_000);
        assert!(match_route("/x", &huge).is_none());
        let m = match_route("/:id", &huge).expect("one long segment is still one segment");
        assert_eq!(m.get_param("id").map(|s| s.as_str().len()), Some(1_000_000));
    }

    #[test]
    fn match_route_deeply_nested_input_does_not_stack_overflow() {
        // match_route is iterative: 10k segments and 10k nested brackets are fine.
        let deep = "/a".repeat(10_000);
        let m = match_route(&deep, &deep).expect("identical deep paths match");
        assert!(m.params.as_ref().is_empty());

        let all_params = "/:p".repeat(10_000);
        let m = match_route(&all_params, &deep).expect("10k params extract");
        assert_eq!(m.params.as_ref().len(), 10_000);
        // Duplicate keys: get_param returns the FIRST binding.
        assert_eq!(m.get_param("p").map(AzString::as_str), Some("a"));

        let brackets = String::from("/") + &"[".repeat(10_000);
        assert!(match_route("/:x", &brackets).is_some());
    }

    #[test]
    fn match_route_segment_count_mismatch_is_none() {
        assert!(match_route("/a/:b", "/a").is_none());
        assert!(match_route("/a", "/a/b").is_none());
        assert!(match_route("/:a/:b/:c", "/1/2").is_none());
    }

    #[test]
    fn route_match_get_param_missing_keys_return_none() {
        let empty = RouteMatch {
            pattern: AzString::from_const_str("/"),
            params: StringPairVec::from_vec(Vec::new()),
        };
        // Empty / whitespace / garbage / unicode / huge keys: None, never a panic.
        assert!(empty.get_param("").is_none());
        assert!(empty.get_param("   ").is_none());
        assert!(empty.get_param("\t\n").is_none());
        assert!(empty.get_param("\u{1F600}").is_none());
        assert!(empty.get_param("\0").is_none());
        assert!(empty.get_param(&"k".repeat(100_000)).is_none());

        // Positive control + near-miss keys on a populated match.
        let m = match_route("/u/:id", "/u/7").expect("valid");
        assert_eq!(m.get_param("id").map(AzString::as_str), Some("7"));
        assert!(m.get_param("ID").is_none()); // case-sensitive
        assert!(m.get_param("i").is_none()); // no prefix matching
        assert!(m.get_param(":id").is_none()); // the ':' is stripped from the key
    }

    #[test]
    fn app_config_match_route_for_path_adversarial_inputs() {
        let mut config = AppConfig::create();
        let cb: crate::callbacks::LayoutCallbackType = autotest_layout;
        extern "C" fn autotest_layout(
            _: RefAny,
            _: crate::callbacks::LayoutCallbackInfo,
        ) -> crate::dom::Dom {
            crate::dom::Dom::create_body()
        }
        config.add_route(AzString::from_const_str("/"), cb);
        config.add_route(AzString::from_const_str("/user/:id"), cb);

        // valid_minimal (positive control)
        let (route, m) = config
            .match_route_for_path("/user/42")
            .expect("registered route must match");
        assert_eq!(route.pattern.as_str(), "/user/:id");
        assert_eq!(m.get_param("id").map(AzString::as_str), Some("42"));

        // "" and "/" both have zero segments -> they hit the "/" route.
        assert!(config.match_route_for_path("").is_some());
        assert!(config.match_route_for_path("/").is_some());

        // garbage / unicode / long / whitespace: deterministic, never panics.
        assert!(config.match_route_for_path("/nope/nope/nope").is_none());
        assert!(config.match_route_for_path("\0\0").is_none());
        assert!(config.match_route_for_path("   ").is_none());
        let long = String::from("/user/") + &"9".repeat(500_000);
        assert!(config.match_route_for_path(&long).is_some());
        let m = config
            .match_route_for_path("/user/\u{1F600}")
            .expect("unicode param");
        assert_eq!(m.1.get_param("id").map(AzString::as_str), Some("\u{1F600}"));
    }

    #[test]
    fn app_config_add_route_replaces_same_pattern_and_orders_by_insertion() {
        extern "C" fn layout_a(
            _: RefAny,
            _: crate::callbacks::LayoutCallbackInfo,
        ) -> crate::dom::Dom {
            crate::dom::Dom::create_body()
        }
        let cb: crate::callbacks::LayoutCallbackType = layout_a;

        let mut config = AppConfig::create();
        assert!(config.routes.as_ref().is_empty());
        config.add_route(AzString::from_const_str("/dup"), cb);
        config.add_route(AzString::from_const_str("/dup"), cb);
        assert_eq!(config.routes.as_ref().len(), 1, "same pattern must replace");

        // First matching route wins: a catch-all registered first shadows later routes.
        let mut config = AppConfig::create();
        config.add_route(AzString::from_const_str("/:anything"), cb);
        config.add_route(AzString::from_const_str("/about"), cb);
        let (route, _) = config.match_route_for_path("/about").expect("matches");
        assert_eq!(route.pattern.as_str(), "/:anything");
    }

    // =====================================================================
    // CONSTRUCTORS / INVARIANTS
    // =====================================================================

    #[test]
    fn dpi_scale_factor_new_handles_nan_and_infinities() {
        // FloatValue::new does a saturating f32 -> isize cast (NaN -> 0).
        assert_eq!(DpiScaleFactor::new(0.0).inner.get(), 0.0);
        assert_eq!(DpiScaleFactor::new(1.0).inner.get(), 1.0);
        assert_eq!(DpiScaleFactor::new(f32::NAN).inner.get(), 0.0);
        assert!(DpiScaleFactor::new(f32::INFINITY).inner.get().is_finite());
        assert!(DpiScaleFactor::new(f32::NEG_INFINITY)
            .inner
            .get()
            .is_finite());
        assert!(DpiScaleFactor::new(f32::MAX).inner.get().is_finite());
        assert!(DpiScaleFactor::new(f32::MIN).inner.get().is_finite());
        // Sub-precision values collapse to 0 (1/1000 fixed point), not to NaN.
        assert_eq!(DpiScaleFactor::new(f32::MIN_POSITIVE).inner.get(), 0.0);
        // Eq/Hash invariant: equal inputs produce equal (hashable) keys.
        assert_eq!(DpiScaleFactor::new(1.5), DpiScaleFactor::new(1.5));
        assert_ne!(DpiScaleFactor::new(1.5), DpiScaleFactor::new(2.0));
    }

    #[test]
    fn named_font_new_keeps_fields_verbatim() {
        let f = NamedFont::new(AzString::from_const_str(""), U8Vec::from_vec(Vec::new()));
        assert_eq!(f.name.as_str(), "");
        assert!(f.bytes.as_ref().is_empty());

        let bytes = vec![0u8, 255, 128];
        let f = NamedFont::new(
            AzString::from(String::from("\u{1F600}")),
            bytes.clone().into(),
        );
        assert_eq!(f.name.as_str(), "\u{1F600}");
        assert_eq!(f.bytes.as_ref(), bytes.as_slice());
    }

    #[test]
    fn loaded_font_new_keeps_fields_verbatim_at_limits() {
        let f = LoadedFont::new(0, AzString::from_const_str(""), 0, false);
        assert_eq!(f.font_hash, 0);
        assert_eq!(f.num_glyphs, 0);
        assert!(!f.has_bytes);

        let f = LoadedFont::new(u64::MAX, AzString::from_const_str("x"), u32::MAX, true);
        assert_eq!(f.font_hash, u64::MAX);
        assert_eq!(f.num_glyphs, u32::MAX);
        assert!(f.has_bytes);
    }

    #[test]
    fn brush_new_defaults_and_extreme_radius() {
        let b = Brush::new(opaque_red(), 4.0);
        assert_eq!(b.radius, 4.0);
        assert_eq!(b.hardness, 0.5);
        assert_eq!(b.flow, 1.0);
        assert_eq!(b.spacing, 0.25);
        assert_eq!(b.color, opaque_red());

        // Extreme radii are stored verbatim (validation happens in paint_dot).
        assert!(Brush::new(opaque_red(), f32::NAN).radius.is_nan());
        assert_eq!(Brush::new(opaque_red(), -0.0).radius, -0.0);
        assert_eq!(
            Brush::new(opaque_red(), f32::INFINITY).radius,
            f32::INFINITY
        );
    }

    #[test]
    fn image_cache_new_is_empty_and_default_is_neutral() {
        let cache = ImageCache::new();
        assert!(cache.image_id_map.is_empty());
        assert!(ImageCache::default().image_id_map.is_empty());
    }

    #[test]
    fn gl_texture_cache_empty_is_neutral() {
        let cache = GlTextureCache::empty();
        assert!(cache.solved_textures.is_empty());
        assert!(cache.hashes.is_empty());
        let d = GlTextureCache::default();
        assert!(d.solved_textures.is_empty());
        assert!(d.hashes.is_empty());
    }

    #[test]
    fn external_image_id_new_is_monotonic() {
        let a = ExternalImageId::new();
        let b = ExternalImageId::new();
        assert!(b.inner > a.inner, "the counter must strictly increase");
        assert!(ExternalImageId::default().inner > b.inner);
    }

    #[test]
    fn shared_raw_image_data_new_invariants() {
        let empty = SharedRawImageData::new(U8Vec::from_vec(Vec::new()));
        assert_eq!(empty.len(), 0);
        assert!(empty.is_empty());
        assert!(empty.as_ref().is_empty());
        assert!(empty.get_bytes().is_empty());
        // An empty Vec still yields a non-null (dangling but aligned) pointer.
        assert!(!empty.as_ptr().is_null());

        let big = SharedRawImageData::new(vec![7u8; 100_000].into());
        assert_eq!(big.len(), 100_000);
        assert!(!big.is_empty());
        assert_eq!(big.as_ref().len(), big.len());
        assert_eq!(big.get_bytes(), big.as_ref());
        // len() must agree with the slice view (no stale-length bug).
        assert_eq!(
            big.into_inner().expect("sole owner").as_ref().len(),
            100_000
        );
    }

    #[test]
    fn app_config_create_registers_builtins_and_defaults() {
        let config = AppConfig::create();
        assert_eq!(config.log_level, AppLogLevel::Error);
        assert!(!config.enable_visual_panic_hook);
        assert!(config.enable_logging_on_panic);
        assert_eq!(
            config.termination_behavior,
            AppTerminationBehavior::EndProcess
        );
        assert!(config.routes.as_ref().is_empty());
        assert!(matches!(
            config.mock_css_environment,
            OptionCssMockEnvironment::None
        ));
        // create() dogfoods add_component_library -> exactly one "builtin" library.
        let libs = config.component_libraries.as_ref();
        assert_eq!(libs.len(), 1);
        assert_eq!(libs[0].name.as_str(), "builtin");
        assert!(!libs[0].components.as_ref().is_empty());
    }

    #[test]
    fn app_config_add_component_library_replaces_same_name() {
        let register: crate::xml::RegisterComponentLibraryFnType =
            crate::xml::register_builtin_components;
        let mut config = AppConfig::create();
        let n_builtin = config.component_libraries.as_ref()[0]
            .components
            .as_ref()
            .len();

        // Same name -> wholesale replacement, NOT a duplicate library.
        config.add_component_library(AzString::from_const_str("builtin"), register);
        assert_eq!(config.component_libraries.as_ref().len(), 1);
        assert_eq!(
            config.component_libraries.as_ref()[0]
                .components
                .as_ref()
                .len(),
            n_builtin
        );

        // A different name (incl. empty / unicode) appends a new library.
        config.add_component_library(AzString::from_const_str(""), register);
        config.add_component_library(AzString::from_const_str("\u{1F600}"), register);
        assert_eq!(config.component_libraries.as_ref().len(), 3);
        assert_eq!(
            config.component_libraries.as_ref()[2].name.as_str(),
            "\u{1F600}"
        );
    }

    #[test]
    fn app_config_with_mock_environment_sets_the_option() {
        let config = AppConfig::create().with_mock_environment(CssMockEnvironment::dark_theme());
        match config.mock_css_environment {
            OptionCssMockEnvironment::Some(env) => {
                assert!(matches!(
                    env.theme,
                    azul_css::dynamic_selector::OptionThemeCondition::Some(
                        azul_css::dynamic_selector::ThemeCondition::Dark
                    )
                ));
            }
            OptionCssMockEnvironment::None => panic!("mock env must be Some"),
        }
        // Last call wins (the field is overwritten, not merged).
        let config = AppConfig::create()
            .with_mock_environment(CssMockEnvironment::linux())
            .with_mock_environment(CssMockEnvironment::windows());
        match config.mock_css_environment {
            OptionCssMockEnvironment::Some(env) => assert!(matches!(
                env.os,
                azul_css::dynamic_selector::OptionOsCondition::Some(
                    azul_css::dynamic_selector::OsCondition::Windows
                )
            )),
            OptionCssMockEnvironment::None => panic!("mock env must be Some"),
        }
    }

    // =====================================================================
    // CssMockEnvironment
    // =====================================================================

    #[test]
    fn css_mock_environment_presets_only_set_their_own_field() {
        use azul_css::dynamic_selector::{
            OptionOsCondition, OptionThemeCondition, OsCondition, ThemeCondition,
        };

        for (mock, os) in [
            (CssMockEnvironment::linux(), OsCondition::Linux),
            (CssMockEnvironment::windows(), OsCondition::Windows),
            (CssMockEnvironment::macos(), OsCondition::MacOS),
        ] {
            assert!(matches!(mock.os, OptionOsCondition::Some(o) if o == os));
            // The other overrides stay unset (auto-detect).
            assert!(matches!(mock.theme, OptionThemeCondition::None));
            assert!(matches!(mock.viewport_width, azul_css::OptionF32::None));
        }

        assert!(matches!(
            CssMockEnvironment::dark_theme().theme,
            OptionThemeCondition::Some(ThemeCondition::Dark)
        ));
        assert!(matches!(
            CssMockEnvironment::light_theme().theme,
            OptionThemeCondition::Some(ThemeCondition::Light)
        ));
        assert!(matches!(
            CssMockEnvironment::dark_theme().os,
            OptionOsCondition::None
        ));
    }

    #[test]
    fn css_mock_environment_apply_to_overrides_only_set_fields() {
        use azul_css::dynamic_selector::{
            BoolCondition, DynamicSelectorContext, OptionOsCondition, OptionThemeCondition,
            OsCondition, ThemeCondition,
        };

        // An all-None mock must leave the context byte-for-byte alone.
        let mut ctx = DynamicSelectorContext::default();
        let before_os = ctx.os;
        let before_lang = ctx.language.clone();
        let before_w = ctx.viewport_width;
        CssMockEnvironment::default().apply_to(&mut ctx);
        assert_eq!(ctx.os, before_os);
        assert_eq!(ctx.language.as_str(), before_lang.as_str());
        assert_eq!(ctx.viewport_width, before_w);

        // A fully-populated mock overrides every field it sets - including
        // adversarial floats (NaN viewport) which must not panic.
        let mock = CssMockEnvironment {
            os: OptionOsCondition::Some(OsCondition::Windows),
            theme: OptionThemeCondition::Some(ThemeCondition::Dark),
            language: azul_css::OptionString::Some(AzString::from_const_str("de-DE")),
            viewport_width: azul_css::OptionF32::Some(f32::NAN),
            viewport_height: azul_css::OptionF32::Some(f32::INFINITY),
            prefers_reduced_motion: azul_css::OptionBool::Some(true),
            prefers_high_contrast: azul_css::OptionBool::Some(false),
            ..Default::default()
        };
        let mut ctx = DynamicSelectorContext::default();
        mock.apply_to(&mut ctx);
        assert_eq!(ctx.os, OsCondition::Windows);
        assert_eq!(ctx.theme, ThemeCondition::Dark);
        assert_eq!(ctx.language.as_str(), "de-DE");
        assert!(ctx.viewport_width.is_nan());
        assert_eq!(ctx.viewport_height, f32::INFINITY);
        assert_eq!(ctx.prefers_reduced_motion, BoolCondition::True);
        assert_eq!(ctx.prefers_high_contrast, BoolCondition::False);

        // apply_to is idempotent.
        let mut ctx2 = ctx.clone();
        mock.apply_to(&mut ctx2);
        assert_eq!(ctx2.os, ctx.os);
        assert_eq!(ctx2.theme, ctx.theme);
    }

    // =====================================================================
    // NUMERIC: brush_dab_coverage / normalize_u16 / premultiply_alpha / Au
    // =====================================================================

    #[test]
    fn brush_dab_coverage_boundaries_and_monotonicity() {
        // Documented profile: 1.0 at the center, 0.0 at (and past) the edge.
        assert_eq!(brush_dab_coverage(0.0, 0.5), 1.0);
        assert_eq!(brush_dab_coverage(1.0, 0.5), 0.0);
        // Out-of-range t is clamped, not extrapolated.
        assert_eq!(brush_dab_coverage(-5.0, 0.5), 1.0);
        assert_eq!(brush_dab_coverage(2.0, 0.5), 0.0);
        assert_eq!(brush_dab_coverage(f32::INFINITY, 0.5), 0.0);
        assert_eq!(brush_dab_coverage(f32::NEG_INFINITY, 0.5), 1.0);

        // Monotonically non-increasing in t, and always inside [0, 1].
        let mut prev = f32::INFINITY;
        for i in 0..=100 {
            let t = i as f32 / 100.0;
            let c = brush_dab_coverage(t, 0.5);
            assert!(
                (0.0..=1.0).contains(&c),
                "coverage {c} out of range at t={t}"
            );
            assert!(c <= prev + 1.0e-6, "not monotonic at t={t}");
            prev = c;
        }
    }

    #[test]
    fn brush_dab_coverage_hardness_limits_never_divide_by_zero() {
        // hardness == 1.0 would make (1 - edge0) == 0; the 1e-4 floor prevents
        // a division by zero -> a hard (but finite) edge instead of inf/NaN.
        assert_eq!(brush_dab_coverage(0.5, 1.0), 1.0);
        assert!(brush_dab_coverage(1.0, 1.0).is_finite());
        assert_eq!(brush_dab_coverage(1.0, 1.0), 1.0); // exactly at edge0 -> x == 0
        assert_eq!(brush_dab_coverage(2.0, 1.0), 0.0);

        // hardness is clamped, so out-of-range hardness behaves like 0.0 / 1.0.
        assert_eq!(brush_dab_coverage(0.5, -10.0), brush_dab_coverage(0.5, 0.0));
        assert_eq!(brush_dab_coverage(0.5, 10.0), brush_dab_coverage(0.5, 1.0));
        assert_eq!(
            brush_dab_coverage(0.5, f32::NEG_INFINITY),
            brush_dab_coverage(0.5, 0.0)
        );
        assert!(brush_dab_coverage(0.5, f32::INFINITY).is_finite());
    }

    #[test]
    fn brush_dab_coverage_nan_propagates_without_panicking() {
        // NaN in -> NaN out (documented-by-behavior); crucially, no panic and no
        // hang. paint_dot's `a <= 0.0` check then skips NaN coverage entirely.
        assert!(brush_dab_coverage(f32::NAN, 0.5).is_nan());
        assert!(brush_dab_coverage(0.5, f32::NAN).is_nan());
        assert!(brush_dab_coverage(f32::NAN, f32::NAN).is_nan());
    }

    #[test]
    fn normalize_u16_is_monotonic_and_saturating() {
        assert_eq!(normalize_u16(u16::MIN), 0);
        assert_eq!(normalize_u16(u16::MAX), u8::MAX);
        let mut prev = 0u8;
        for i in (0..=u16::MAX).step_by(97) {
            let v = normalize_u16(i);
            assert!(v >= prev, "normalize_u16 must be monotonic ({i} -> {v})");
            prev = v;
        }
    }

    #[test]
    fn premultiply_alpha_ignores_non_4_byte_slices() {
        // Documented: only a single 4-byte pixel is touched.
        for len in [0usize, 1, 2, 3, 5, 8] {
            let mut buf = vec![200u8; len];
            let before = buf.clone();
            premultiply_alpha(&mut buf);
            assert_eq!(buf, before, "len {len} must be left untouched");
        }
    }

    #[test]
    fn premultiply_alpha_boundary_values_never_overflow() {
        // a == 255 -> unchanged (rounding must not drift).
        let mut opaque = [255u8, 128, 0, 255];
        premultiply_alpha(&mut opaque);
        assert_eq!(opaque, [255, 128, 0, 255]);

        // a == 0 -> fully transparent -> RGB zeroed, alpha untouched.
        let mut transparent = [255u8, 255, 255, 0];
        premultiply_alpha(&mut transparent);
        assert_eq!(transparent, [0, 0, 0, 0]);

        // a == 128 -> ~half, computed with the +128/255 rounding, never > 255.
        let mut half = [255u8, 128, 0, 128];
        premultiply_alpha(&mut half);
        assert_eq!(half, [128, 64, 0, 128]);

        // The u32 intermediate must not truncate at the maximum product.
        let mut max = [255u8, 255, 255, 255];
        premultiply_alpha(&mut max);
        assert_eq!(max, [255, 255, 255, 255]);
    }

    #[test]
    fn au_from_px_saturates_at_limits_and_nan() {
        assert_eq!(Au::from_px(0.0).0, 0);
        assert_eq!(Au::from_px(-0.0).0, 0);
        assert_eq!(Au::from_px(1.0).0, AU_PER_PX);
        assert_eq!(Au::from_px(-1.0).0, -AU_PER_PX);
        // NaN -> 0 (saturating `as` cast), NOT a panic and NOT garbage.
        assert_eq!(Au::from_px(f32::NAN).0, 0);
        // Infinities / f32 extremes clamp into [MIN_AU, MAX_AU].
        assert_eq!(Au::from_px(f32::INFINITY).0, MAX_AU);
        assert_eq!(Au::from_px(f32::NEG_INFINITY).0, MIN_AU);
        assert_eq!(Au::from_px(f32::MAX).0, MAX_AU);
        assert_eq!(Au::from_px(f32::MIN).0, MIN_AU);
        // Anything in range stays in range.
        for px in [-1.0e9_f32, -1.0, 0.5, 16.0, 1.0e9] {
            let au = Au::from_px(px).0;
            assert!(
                (MIN_AU..=MAX_AU).contains(&au),
                "{px} -> {au} escaped the clamp"
            );
        }
    }

    #[test]
    fn au_px_round_trip_is_stable() {
        // px -> Au -> px must round-trip within one app-unit (1/60 px).
        for px in [0.0_f32, 0.5, 1.0, 12.0, 16.0, 72.5, -3.25, 1000.0] {
            let back = Au::from_px(px).into_px();
            assert!(
                (back - px).abs() <= 1.0 / AU_PER_PX as f32,
                "{px} round-tripped to {back}"
            );
        }
        // Exact for whole pixels.
        assert_eq!(Au::from_px(16.0).into_px(), 16.0);
        // Extremes stay finite.
        assert!(Au(MAX_AU).into_px().is_finite());
        assert!(Au(MIN_AU).into_px().is_finite());
        assert!(Au(i32::MIN).into_px().is_finite());
        assert!(Au(i32::MAX).into_px().is_finite());
    }

    #[test]
    fn font_size_to_au_zero_negative_and_typical() {
        use azul_css::props::basic::PixelValue;
        let au = |px: isize| {
            font_size_to_au(StyleFontSize {
                inner: PixelValue::const_px(px),
            })
            .0
        };
        assert_eq!(au(0), 0);
        assert_eq!(au(16), 16 * AU_PER_PX);
        assert_eq!(au(-10), -10 * AU_PER_PX);
        // Large-but-representable sizes stay inside the clamp.
        assert!((MIN_AU..=MAX_AU).contains(&au(1_000_000)));
        assert!((MIN_AU..=MAX_AU).contains(&au(-1_000_000)));
    }

    // =====================================================================
    // NUMERIC: Epoch
    // =====================================================================

    #[test]
    fn epoch_new_from_and_into_u32() {
        assert_eq!(Epoch::new().into_u32(), 0);
        assert_eq!(Epoch::default().into_u32(), 0);
        assert_eq!(Epoch::from(0).into_u32(), 0);
        assert_eq!(Epoch::from(1).into_u32(), 1);
        assert_eq!(Epoch::from(u32::MAX).into_u32(), u32::MAX);
        assert_eq!(Epoch::from(u32::MAX - 1).into_u32(), u32::MAX - 1);
    }

    #[test]
    fn epoch_increment_wraps_at_max_minus_one_and_never_reaches_max() {
        let mut e = Epoch::new();
        e.increment();
        assert_eq!(e.into_u32(), 1);

        // u32::MAX is reserved as "invalid", so MAX-1 wraps back to 0.
        let mut e = Epoch::from(u32::MAX - 1);
        e.increment();
        assert_eq!(e.into_u32(), 0, "MAX-1 must wrap to 0, never to u32::MAX");

        // An epoch that somehow starts AT u32::MAX saturates (fixpoint) instead
        // of wrapping or overflow-panicking - deterministic, no UB.
        let mut e = Epoch::from(u32::MAX);
        e.increment();
        assert_eq!(e.into_u32(), u32::MAX);

        // A long run of increments never yields the invalid u32::MAX.
        let mut e = Epoch::from(u32::MAX - 3);
        for _ in 0..8 {
            e.increment();
            assert_ne!(e.into_u32(), u32::MAX);
        }
    }

    #[test]
    fn epoch_display_is_non_empty_for_edge_values() {
        assert_eq!(alloc::format!("{}", Epoch::new()), "0");
        assert_eq!(alloc::format!("{}", Epoch::from(42)), "42");
        assert_eq!(
            alloc::format!("{}", Epoch::from(u32::MAX)),
            alloc::format!("{}", u32::MAX)
        );
        assert!(!alloc::format!("{:?}", Epoch::default()).is_empty());
    }

    #[test]
    fn id_namespace_display_and_debug_are_well_formed() {
        assert_eq!(alloc::format!("{}", IdNamespace(0)), "IdNamespace(0)");
        assert_eq!(
            alloc::format!("{}", IdNamespace(u32::MAX)),
            alloc::format!("IdNamespace({})", u32::MAX)
        );
        // Debug delegates to Display (must not recurse / be empty).
        assert_eq!(
            alloc::format!("{:?}", IdNamespace(7)),
            alloc::format!("{}", IdNamespace(7))
        );
    }

    // =====================================================================
    // KEYS: uniqueness / namespace preservation / hash->key derivation
    // =====================================================================

    #[test]
    fn unique_keys_are_strictly_increasing_and_keep_their_namespace() {
        let ns = IdNamespace(u32::MAX);

        let a = ImageKey::unique(ns);
        let b = ImageKey::unique(ns);
        assert_eq!(a.namespace, ns);
        assert!(b.key > a.key, "ImageKey counter must strictly increase");
        // The counter starts at 1 so a live key can never collide with DUMMY.
        assert_eq!(ImageKey::DUMMY.key, 0);
        assert_ne!(a, ImageKey::DUMMY);

        let a = FontKey::unique(ns);
        let b = FontKey::unique(ns);
        assert_eq!(a.namespace, ns);
        assert!(b.key > a.key);

        let a = FontInstanceKey::unique(IdNamespace(0));
        let b = FontInstanceKey::unique(IdNamespace(0));
        assert_eq!(a.namespace, IdNamespace(0));
        assert!(b.key > a.key);
    }

    #[test]
    fn image_ref_id_counter_is_monotonic_and_never_zero() {
        // id == 0 is reserved to flag an un-initialised handle.
        let a = next_image_ref_id();
        let b = next_image_ref_id();
        assert!(a > 0 && b > a);
    }

    #[test]
    fn image_ref_hash_conversions_are_lossless_and_agree() {
        let img = ImageRef::null_image(1, 1, RawImageFormat::RGBA8, Vec::new());
        let hash = img.get_hash();
        assert_eq!(hash, image_ref_get_hash(&img));

        let key = image_ref_hash_to_image_key(hash, IdNamespace(9));
        assert_eq!(key.namespace, IdNamespace(9));
        assert_eq!(key.key, hash.inner, "the u64 id must survive verbatim");

        let ext = image_ref_hash_to_external_image_id(hash);
        assert_eq!(ext.inner, hash.inner);

        // Both derivations agree for boundary hashes too.
        for inner in [0u64, 1, u64::MAX, u64::MAX - 1] {
            let h = ImageRefHash { inner };
            assert_eq!(image_ref_hash_to_image_key(h, IdNamespace(0)).key, inner);
            assert_eq!(image_ref_hash_to_external_image_id(h).inner, inner);
        }
    }

    #[test]
    fn texture_external_image_id_is_deterministic_and_collision_free() {
        let id = |d: usize, n: usize| texture_external_image_id(DomId { inner: d }, NodeId::new(n));

        // Same input -> same id (cached display lists depend on this).
        assert_eq!(id(3, 7), id(3, 7));
        assert_eq!(id(0, 0).inner, 0);
        // The dom goes in the high 32 bits, the node in the low 32.
        assert_eq!(id(1, 2).inner, (1u64 << 32) | 2);
        // (0,1) and (1,0) must not collide.
        assert_ne!(id(0, 1), id(1, 0));
        // Boundary node index inside the 32-bit range.
        assert_eq!(id(0, u32::MAX as usize).inner, u64::from(u32::MAX));
        assert_eq!(id(u32::MAX as usize, 0).inner, u64::from(u32::MAX) << 32);
    }

    // =====================================================================
    // GETTERS / PREDICATES: RawImageData
    // =====================================================================

    #[test]
    fn raw_image_data_typed_getters_only_match_their_own_variant() {
        let u8v = RawImageData::U8(vec![1u8, 2].into());
        let u16v = RawImageData::U16(vec![1u16, 2].into());
        let f32v = RawImageData::F32(vec![1.0f32, 2.0].into());

        assert_eq!(u8v.get_u8_vec_ref().map(|v| v.len()), Some(2));
        assert!(u8v.get_u16_vec_ref().is_none());
        assert!(u8v.get_f32_vec_ref().is_none());

        assert!(u16v.get_u8_vec_ref().is_none());
        assert_eq!(u16v.get_u16_vec_ref().map(|v| v.len()), Some(2));
        assert!(u16v.get_f32_vec_ref().is_none());

        assert!(f32v.get_u8_vec_ref().is_none());
        assert!(f32v.get_u16_vec_ref().is_none());
        assert_eq!(f32v.get_f32_vec_ref().map(|v| v.len()), Some(2));

        // Empty payloads are Some(empty), not None.
        let empty = RawImageData::U8(U8Vec::from_vec(Vec::new()));
        assert_eq!(empty.get_u8_vec_ref().map(|v| v.len()), Some(0));

        // by-value variants agree with the by-ref ones
        assert!(RawImageData::U8(vec![9u8].into()).get_u8_vec().is_some());
        assert!(RawImageData::U16(vec![9u16].into()).get_u8_vec().is_none());
        assert!(RawImageData::U16(vec![9u16].into()).get_u16_vec().is_some());
        assert!(RawImageData::F32(vec![9.0f32].into())
            .get_u16_vec()
            .is_none());
    }

    // =====================================================================
    // NUMERIC / ROUND-TRIP: RawImage::load_* format conversions
    // =====================================================================

    #[test]
    fn load_fns_reject_wrong_payload_type() {
        // Every loader demands a specific RawImageData variant; a mismatch is
        // None (never a panic / never garbage pixels).
        let u16_1px = || RawImageData::U16(vec![0u16; 4].into());
        let f32_1px = || RawImageData::F32(vec![0.0f32; 4].into());
        let u8_1px = || RawImageData::U8(vec![0u8; 4].into());

        assert!(RawImage::load_r8(u16_1px(), 4).is_none());
        assert!(RawImage::load_rg8(f32_1px(), 2, true).is_none());
        assert!(RawImage::load_rgb8(u16_1px(), 1).is_none());
        assert!(RawImage::load_rgba8(f32_1px(), 1, true).is_none());
        assert!(RawImage::load_r16(u8_1px(), 4).is_none());
        assert!(RawImage::load_rg16(f32_1px(), 2).is_none());
        assert!(RawImage::load_rgb16(u8_1px(), 1).is_none());
        assert!(RawImage::load_rgba16(u8_1px(), 1, true).is_none());
        assert!(RawImage::load_bgr8(u16_1px(), 1).is_none());
        assert!(RawImage::load_bgra8(u16_1px(), 1, true).is_none());
        assert!(RawImage::load_rgbf32(u8_1px(), 1).is_none());
        assert!(RawImage::load_rgbaf32(u16_1px(), 1, true).is_none());
    }

    #[test]
    fn load_fns_reject_every_wrong_length() {
        // One byte too few and one too many must BOTH be rejected for each format.
        assert!(RawImage::load_r8(RawImageData::U8(vec![0u8; 3].into()), 4).is_none());
        assert!(RawImage::load_r8(RawImageData::U8(vec![0u8; 5].into()), 4).is_none());
        assert!(RawImage::load_rg8(RawImageData::U8(vec![0u8; 3].into()), 2, true).is_none());
        assert!(RawImage::load_rg8(RawImageData::U8(vec![0u8; 5].into()), 2, true).is_none());
        assert!(RawImage::load_rgb8(RawImageData::U8(vec![0u8; 5].into()), 2).is_none());
        assert!(RawImage::load_rgb8(RawImageData::U8(vec![0u8; 7].into()), 2).is_none());
        assert!(RawImage::load_rgba8(RawImageData::U8(vec![0u8; 7].into()), 2, true).is_none());
        assert!(RawImage::load_rgba8(RawImageData::U8(vec![0u8; 9].into()), 2, false).is_none());
        assert!(RawImage::load_r16(RawImageData::U16(vec![0u16; 3].into()), 4).is_none());
        assert!(RawImage::load_rg16(RawImageData::U16(vec![0u16; 3].into()), 2).is_none());
        assert!(RawImage::load_rgb16(RawImageData::U16(vec![0u16; 5].into()), 2).is_none());
        assert!(RawImage::load_rgba16(RawImageData::U16(vec![0u16; 7].into()), 2, true).is_none());
        assert!(RawImage::load_bgr8(RawImageData::U8(vec![0u8; 5].into()), 2).is_none());
        assert!(RawImage::load_bgra8(RawImageData::U8(vec![0u8; 7].into()), 2, false).is_none());
        assert!(RawImage::load_rgbf32(RawImageData::F32(vec![0.0f32; 5].into()), 2).is_none());
        assert!(
            RawImage::load_rgbaf32(RawImageData::F32(vec![0.0f32; 7].into()), 2, true).is_none()
        );
    }

    #[test]
    fn load_fns_accept_zero_pixels() {
        // expected_len == 0 with an empty buffer: Some(empty), no div-by-zero.
        let empty_u8 = || RawImageData::U8(U8Vec::from_vec(Vec::new()));
        let empty_u16 = || RawImageData::U16(U16Vec::from_vec(Vec::new()));
        let empty_f32 = || RawImageData::F32(F32Vec::from_vec(Vec::new()));

        assert_eq!(
            RawImage::load_r8(empty_u8(), 0).map(|(b, o)| (b.len(), o)),
            Some((0, false))
        );
        assert_eq!(
            RawImage::load_rg8(empty_u8(), 0, true).map(|(b, _)| b.len()),
            Some(0)
        );
        assert_eq!(
            RawImage::load_rgb8(empty_u8(), 0).map(|(b, _)| b.len()),
            Some(0)
        );
        assert_eq!(
            RawImage::load_rgba8(empty_u8(), 0, true).map(|(b, _)| b.len()),
            Some(0)
        );
        assert_eq!(
            RawImage::load_r16(empty_u16(), 0).map(|(b, _)| b.len()),
            Some(0)
        );
        assert_eq!(
            RawImage::load_rg16(empty_u16(), 0).map(|(b, _)| b.len()),
            Some(0)
        );
        assert_eq!(
            RawImage::load_rgb16(empty_u16(), 0).map(|(b, _)| b.len()),
            Some(0)
        );
        assert_eq!(
            RawImage::load_rgba16(empty_u16(), 0, true).map(|(b, _)| b.len()),
            Some(0)
        );
        assert_eq!(
            RawImage::load_bgr8(empty_u8(), 0).map(|(b, _)| b.len()),
            Some(0)
        );
        assert_eq!(
            RawImage::load_bgra8(empty_u8(), 0, true).map(|(b, _)| b.len()),
            Some(0)
        );
        assert_eq!(
            RawImage::load_rgbf32(empty_f32(), 0).map(|(b, _)| b.len()),
            Some(0)
        );
        assert_eq!(
            RawImage::load_rgbaf32(empty_f32(), 0, true).map(|(b, _)| b.len()),
            Some(0)
        );
    }

    #[test]
    fn load_r8_passes_data_through_and_is_never_opaque() {
        // R8 must stay R8 (image masks depend on the single channel surviving).
        let (bytes, is_opaque) =
            RawImage::load_r8(RawImageData::U8(vec![0u8, 128, 255, 1].into()), 4)
                .expect("exact length");
        assert_eq!(bytes.as_ref(), &[0, 128, 255, 1]);
        assert!(!is_opaque, "R8 is documented as never opaque");
    }

    #[test]
    fn load_rgb8_and_bgr8_swizzle_to_bgra_opaque() {
        // RGB8 -> BGRA8: channel order flips, alpha forced to 0xFF, always opaque.
        let (bytes, is_opaque) =
            RawImage::load_rgb8(RawImageData::U8(vec![1u8, 2, 3].into()), 1).expect("1 px");
        assert_eq!(bytes.as_ref(), &[3, 2, 1, 255]);
        assert!(is_opaque);

        // BGR8 -> BGRA8: order preserved, alpha appended.
        let (bytes, is_opaque) =
            RawImage::load_bgr8(RawImageData::U8(vec![1u8, 2, 3].into()), 1).expect("1 px");
        assert_eq!(bytes.as_ref(), &[1, 2, 3, 255]);
        assert!(is_opaque);
    }

    #[test]
    fn load_rgba8_swizzles_and_detects_transparency() {
        // Premultiplied: RGBA -> BGRA swizzle only.
        let (bytes, is_opaque) =
            RawImage::load_rgba8(RawImageData::U8(vec![10u8, 20, 30, 255].into()), 1, true)
                .expect("1 px");
        assert_eq!(bytes.as_ref(), &[30, 20, 10, 255]);
        assert!(is_opaque);

        // A single non-255 alpha flips is_opaque to false.
        let (_, is_opaque) =
            RawImage::load_rgba8(RawImageData::U8(vec![0u8, 0, 0, 254].into()), 1, true)
                .expect("1 px");
        assert!(!is_opaque);

        // Non-premultiplied: swizzle THEN premultiply by alpha.
        let (bytes, is_opaque) =
            RawImage::load_rgba8(RawImageData::U8(vec![10u8, 20, 30, 128].into()), 1, false)
                .expect("1 px");
        assert_eq!(bytes.as_ref(), &[15, 10, 5, 128]);
        assert!(!is_opaque);

        // alpha == 0 must zero the colour (no leftover colour fringe).
        let (bytes, _) =
            RawImage::load_rgba8(RawImageData::U8(vec![255u8, 255, 255, 0].into()), 1, false)
                .expect("1 px");
        assert_eq!(bytes.as_ref(), &[0, 0, 0, 0]);
    }

    #[test]
    fn load_rg8_expands_grey_to_bgra() {
        // Greyscale + alpha -> BGRA with the grey replicated across B/G/R.
        let (bytes, is_opaque) =
            RawImage::load_rg8(RawImageData::U8(vec![100u8, 255].into()), 1, true).expect("1 px");
        assert_eq!(bytes.as_ref(), &[100, 100, 100, 255]);
        assert!(is_opaque);

        let (bytes, is_opaque) =
            RawImage::load_rg8(RawImageData::U8(vec![100u8, 128].into()), 1, false).expect("1 px");
        assert_eq!(bytes.as_ref(), &[50, 50, 50, 128]);
        assert!(!is_opaque);
    }

    #[test]
    fn load_16_bit_formats_normalize_to_8_bit() {
        // u16::MAX -> 255, 0 -> 0 (no wrap-around / no div-by-zero).
        let (bytes, is_opaque) =
            RawImage::load_r16(RawImageData::U16(vec![u16::MAX].into()), 1).expect("1 px");
        assert_eq!(bytes.as_ref(), &[255, 255, 255, 255]);
        assert!(is_opaque);

        let (bytes, is_opaque) =
            RawImage::load_rg16(RawImageData::U16(vec![0u16, u16::MAX].into()), 1).expect("1 px");
        assert_eq!(bytes.as_ref(), &[0, 0, 0, 255]);
        assert!(is_opaque);

        // RGB16 -> BGRA8 swizzle.
        let (bytes, _) =
            RawImage::load_rgb16(RawImageData::U16(vec![u16::MAX, 0, 0].into()), 1).expect("1 px");
        assert_eq!(bytes.as_ref(), &[0, 0, 255, 255]);

        // RGBA16 with a zero alpha -> not opaque; premultiply zeroes the colour.
        let (bytes, is_opaque) = RawImage::load_rgba16(
            RawImageData::U16(vec![u16::MAX, u16::MAX, u16::MAX, 0].into()),
            1,
            false,
        )
        .expect("1 px");
        assert_eq!(bytes.as_ref(), &[0, 0, 0, 0]);
        assert!(!is_opaque);
    }

    #[test]
    fn load_f32_formats_saturate_on_out_of_range_nan_and_inf() {
        // The f32 -> u8 cast is saturating: >1.0 -> 255, <0.0 -> 0, NaN -> 0.
        // (This is the whole "HDR pixel with a garbage float" attack surface.)
        let (bytes, is_opaque) =
            RawImage::load_rgbf32(RawImageData::F32(vec![2.0f32, -1.0, f32::NAN].into()), 1)
                .expect("1 px");
        assert_eq!(
            bytes.as_ref(),
            &[0, 0, 255, 255],
            "b=NaN->0, g=-1->0, r=2.0->255"
        );
        assert!(is_opaque);

        let (bytes, is_opaque) = RawImage::load_rgbaf32(
            RawImageData::F32(vec![f32::INFINITY, f32::NEG_INFINITY, 0.5, 1.0].into()),
            1,
            true,
        )
        .expect("1 px");
        assert_eq!(bytes.as_ref(), &[127, 0, 255, 255]);
        assert!(is_opaque);

        // NaN alpha -> 0 -> not opaque (fails safe, does not claim opacity).
        let (_, is_opaque) = RawImage::load_rgbaf32(
            RawImageData::F32(vec![1.0f32, 1.0, 1.0, f32::NAN].into()),
            1,
            true,
        )
        .expect("1 px");
        assert!(!is_opaque);
    }

    // =====================================================================
    // ROUND-TRIP: RawImage <-> ImageRef
    // =====================================================================

    #[test]
    fn raw_image_null_image_encodes_to_an_empty_bgra8_descriptor() {
        let null = RawImage::null_image();
        assert_eq!(null.width, 0);
        assert_eq!(null.height, 0);
        assert_eq!(null.data_format, RawImageFormat::BGRA8);
        assert!(null.premultiplied_alpha);

        let (data, descriptor) = null
            .into_loaded_image_source()
            .expect("a 0x0 image is still a valid (empty) source");
        assert_eq!(descriptor.width, 0);
        assert_eq!(descriptor.height, 0);
        assert_eq!(descriptor.format, RawImageFormat::BGRA8);
        assert_eq!(descriptor.offset, 0);
        match data {
            ImageData::Raw(bytes) => assert!(bytes.is_empty()),
            ImageData::External(_) => panic!("a RawImage must never encode to External"),
        }
    }

    #[test]
    fn raw_image_allocate_mask_zero_and_negative_sizes() {
        let mask = RawImage::allocate_mask(LayoutSize::zero());
        assert_eq!(mask.data_format, RawImageFormat::R8);
        assert_eq!(mask.width, 0);
        assert_eq!(mask.height, 0);
        assert_eq!(mask.pixels.get_u8_vec_ref().map(|v| v.len()), Some(0));

        let mask = RawImage::allocate_mask(LayoutSize::new(4, 4));
        assert_eq!(mask.pixels.get_u8_vec_ref().map(|v| v.len()), Some(16));
        assert!(mask
            .pixels
            .get_u8_vec_ref()
            .expect("u8")
            .as_ref()
            .iter()
            .all(|b| *b == 0));

        // Negative sizes: the BUFFER is clamped to 0 (no huge alloc, no panic),
        // but the width/height FIELDS keep the wrapped `as usize` value, so the
        // returned RawImage is internally inconsistent. Callers must not feed a
        // negative LayoutSize in. (Buffer-side safety is what matters here.)
        let mask = RawImage::allocate_mask(LayoutSize::new(-4, 4));
        assert_eq!(
            mask.pixels.get_u8_vec_ref().map(|v| v.len()),
            Some(0),
            "a negative extent must never allocate"
        );
        assert!(
            mask.width > 1_000_000,
            "negative width wraps via `as usize`"
        );
    }

    #[test]
    fn raw_image_mask_round_trips_as_r8() {
        // A mask must stay single-channel R8 through the encoder (clip masks
        // break if it silently becomes BGRA8).
        let mask = RawImage::allocate_mask(LayoutSize::new(2, 2));
        let (data, descriptor) = mask.into_loaded_image_source().expect("consistent mask");
        assert_eq!(descriptor.format, RawImageFormat::R8);
        assert_eq!((descriptor.width, descriptor.height), (2, 2));
        assert!(!descriptor.flags.is_opaque, "R8 is never opaque");
        match data {
            ImageData::Raw(bytes) => assert_eq!(bytes.len(), 4),
            ImageData::External(_) => panic!("expected raw bytes"),
        }
    }

    #[test]
    fn raw_image_rgba8_encode_decode_round_trip() {
        // encode: RGBA8 -> BGRA8 bytes; decode: ImageRef::get_rawimage gives the
        // encoded (BGRA8) pixels back verbatim.
        let raw = RawImage {
            pixels: RawImageData::U8(vec![10u8, 20, 30, 255].into()),
            width: 1,
            height: 1,
            premultiplied_alpha: true,
            data_format: RawImageFormat::RGBA8,
            tag: Vec::new().into(),
        };
        let img = ImageRef::new_rawimage(raw).expect("1x1 RGBA8 with 4 bytes is valid");

        assert!(img.is_raw_image());
        assert!(!img.is_null_image());
        assert!(!img.is_gl_texture());
        assert!(!img.is_callback());
        assert_eq!(img.get_size(), LogicalSize::new(1.0, 1.0));
        assert_eq!(img.get_bytes(), Some(&[30u8, 20, 10, 255][..]));
        assert!(!img.get_bytes_ptr().is_null());

        let decoded = img.get_rawimage().expect("raw image round-trips");
        assert_eq!(decoded.width, 1);
        assert_eq!(decoded.height, 1);
        assert_eq!(decoded.data_format, RawImageFormat::BGRA8);
        assert!(decoded.premultiplied_alpha);
        assert_eq!(
            decoded.pixels.get_u8_vec_ref().map(|v| v.as_ref().to_vec()),
            Some(vec![30, 20, 10, 255])
        );
    }

    #[test]
    fn image_ref_new_rawimage_rejects_dimension_mismatch() {
        // 2x2 RGBA8 needs 16 bytes; 4 bytes must be rejected (None, not a crash).
        let too_small = RawImage {
            pixels: RawImageData::U8(vec![0u8; 4].into()),
            width: 2,
            height: 2,
            premultiplied_alpha: true,
            data_format: RawImageFormat::RGBA8,
            tag: Vec::new().into(),
        };
        assert!(ImageRef::new_rawimage(too_small).is_none());

        // Too MANY bytes is equally invalid.
        let too_big = RawImage {
            pixels: RawImageData::U8(vec![0u8; 64].into()),
            width: 2,
            height: 2,
            premultiplied_alpha: true,
            data_format: RawImageFormat::RGBA8,
            tag: Vec::new().into(),
        };
        assert!(ImageRef::new_rawimage(too_big).is_none());

        // Right byte count, wrong payload type -> None.
        let wrong_type = RawImage {
            pixels: RawImageData::U16(vec![0u16; 16].into()),
            width: 2,
            height: 2,
            premultiplied_alpha: true,
            data_format: RawImageFormat::RGBA8,
            tag: Vec::new().into(),
        };
        assert!(ImageRef::new_rawimage(wrong_type).is_none());
    }

    // =====================================================================
    // GETTERS / PREDICATES: ImageRef
    // =====================================================================

    #[test]
    fn image_ref_null_image_predicates_and_accessors() {
        let img = ImageRef::null_image(0, 0, RawImageFormat::BGRA8, Vec::new());
        assert!(img.is_null_image());
        assert!(!img.is_raw_image());
        assert!(!img.is_gl_texture());
        assert!(!img.is_callback());
        assert_eq!(img.get_size(), LogicalSize::new(0.0, 0.0));
        assert!(img.get_bytes().is_none());
        assert!(img.get_rawimage().is_none());
        assert!(img.get_bytes_ptr().is_null());
        assert!(img.get_image_callback().is_none());
        assert!(matches!(img.get_data(), DecodedImage::NullImage { .. }));
    }

    #[test]
    fn image_ref_null_image_at_usize_max_reports_a_finite_size() {
        // usize::MAX as f32 must not produce NaN/inf (it saturates to ~1.8e19).
        let img = ImageRef::null_image(usize::MAX, usize::MAX, RawImageFormat::R8, Vec::new());
        let size = img.get_size();
        assert!(size.width.is_finite() && size.height.is_finite());
        assert!(size.width > 0.0 && size.height > 0.0);
        assert!(img.is_null_image());

        // A large tag survives verbatim.
        let img = ImageRef::null_image(1, 1, RawImageFormat::R8, vec![9u8; 10_000]);
        match img.get_data() {
            DecodedImage::NullImage { tag, .. } => assert_eq!(tag.len(), 10_000),
            _ => panic!("expected NullImage"),
        }
    }

    #[test]
    fn image_ref_hash_identity_rules() {
        let a = ImageRef::null_image(1, 1, RawImageFormat::R8, Vec::new());
        let b = ImageRef::null_image(1, 1, RawImageFormat::R8, Vec::new());
        // Two structurally identical images are still DIFFERENT images.
        assert_ne!(a.get_hash(), b.get_hash());
        assert_ne!(a, b);

        // A shallow clone is the SAME image.
        let a2 = a.clone();
        assert_eq!(a.get_hash(), a2.get_hash());
        assert_eq!(a, a2);

        // A deep copy is a NEW image with a fresh identity.
        let deep = a.deep_copy();
        assert_ne!(a.get_hash(), deep.get_hash());
        assert!(deep.is_null_image());
        assert_eq!(deep.get_size(), a.get_size());
    }

    #[test]
    fn image_ref_callback_accessors() {
        // CoreRenderImageCallbackType is a usize placeholder, so 0 is a valid
        // (if inert) callback token.
        let mut img = ImageRef::callback(0usize, RefAny::new(123u32));
        assert!(img.is_callback());
        assert!(!img.is_null_image());
        assert!(!img.is_raw_image());
        // Documented: a Callback reports a (0, 0) size.
        assert_eq!(img.get_size(), LogicalSize::new(0.0, 0.0));
        assert!(img.get_bytes().is_none());
        assert!(img.get_bytes_ptr().is_null());
        assert!(img.get_rawimage().is_none());

        // Sole owner -> the callback is reachable (shared / mutable).
        assert!(img.get_image_callback().is_some());
        assert!(img.get_image_callback_mut().is_some());

        // While a second handle is alive, aliasing &mut would be unsound, so
        // BOTH accessors must refuse.
        let clone = img.clone();
        assert!(img.get_image_callback().is_none());
        assert!(img.get_image_callback_mut().is_none());
        drop(clone);
        assert!(img.get_image_callback().is_some());
    }

    #[test]
    fn image_ref_deep_copy_of_a_callback_keeps_it_a_callback() {
        let img = ImageRef::callback(0usize, RefAny::new(1u8));
        let deep = img.deep_copy();
        assert!(deep.is_callback());
        assert_ne!(img.get_hash(), deep.get_hash());
    }

    #[test]
    fn image_ref_into_inner_only_when_sole_owner() {
        let img = ImageRef::null_image(2, 2, RawImageFormat::RGBA8, vec![1, 2, 3]);
        let clone = img.clone();
        assert!(clone.into_inner().is_none(), "shared -> must refuse");

        let inner = img.into_inner().expect("sole owner -> takes ownership");
        match inner {
            DecodedImage::NullImage {
                width,
                height,
                format,
                tag,
            } => {
                assert_eq!((width, height), (2, 2));
                assert_eq!(format, RawImageFormat::RGBA8);
                assert_eq!(tag, vec![1, 2, 3]);
            }
            _ => panic!("expected NullImage"),
        }
    }

    // =====================================================================
    // ImageCache
    // =====================================================================

    #[test]
    fn image_cache_add_get_delete_round_trip() {
        let mut cache = ImageCache::new();
        let key = AzString::from_const_str("my_image");
        let img = ImageRef::null_image(1, 1, RawImageFormat::R8, Vec::new());
        let hash = img.get_hash();

        assert!(cache.get_css_image_id(&key).is_none());
        cache.add_css_image_id(key.clone(), img);
        assert_eq!(
            cache.get_css_image_id(&key).map(ImageRef::get_hash),
            Some(hash)
        );

        // Re-inserting the same id replaces (does not duplicate).
        let img2 = ImageRef::null_image(2, 2, RawImageFormat::R8, Vec::new());
        let hash2 = img2.get_hash();
        cache.add_css_image_id(key.clone(), img2);
        assert_eq!(cache.image_id_map.len(), 1);
        assert_eq!(
            cache.get_css_image_id(&key).map(ImageRef::get_hash),
            Some(hash2)
        );

        cache.delete_css_image_id(&key);
        assert!(cache.get_css_image_id(&key).is_none());
        assert!(cache.image_id_map.is_empty());
        // Deleting a missing id is a no-op, not a panic.
        cache.delete_css_image_id(&key);
        cache.delete_css_image_id(&AzString::from_const_str("never-existed"));
    }

    #[test]
    fn image_cache_handles_empty_and_unicode_keys() {
        let mut cache = ImageCache::new();
        let empty = AzString::from_const_str("");
        let unicode = AzString::from(String::from("\u{1F600}\u{0301}"));
        let long = AzString::from("k".repeat(100_000));

        cache.add_css_image_id(
            empty.clone(),
            ImageRef::null_image(1, 1, RawImageFormat::R8, Vec::new()),
        );
        cache.add_css_image_id(
            unicode.clone(),
            ImageRef::null_image(1, 1, RawImageFormat::R8, Vec::new()),
        );
        cache.add_css_image_id(
            long.clone(),
            ImageRef::null_image(1, 1, RawImageFormat::R8, Vec::new()),
        );

        assert_eq!(cache.image_id_map.len(), 3);
        assert!(cache.get_css_image_id(&empty).is_some());
        assert!(cache.get_css_image_id(&unicode).is_some());
        assert!(cache.get_css_image_id(&long).is_some());
        // Distinct keys must not alias each other.
        assert!(cache
            .get_css_image_id(&AzString::from_const_str("\u{1F600}"))
            .is_none());
    }

    // =====================================================================
    // RendererResources
    // =====================================================================

    #[test]
    fn renderer_resources_lookups_on_an_empty_registry_are_none() {
        let rr = RendererResources::default();
        let ns = IdNamespace(1);
        assert!(rr
            .get_renderable_font_data(&FontInstanceKey::unique(ns))
            .is_none());
        let families = StyleFontFamiliesHash::new(&[]);
        assert!(rr
            .get_font_instance_key(&families, Au(0), DpiScaleFactor::new(1.0))
            .is_none());
        assert!(rr
            .get_font_instance_key(&families, Au(MAX_AU), DpiScaleFactor::new(f32::NAN))
            .is_none());
        assert!(rr.get_image(&ImageRefHash { inner: 0 }).is_none());
        assert!(rr
            .get_font_key(&StyleFontFamilyHash::new(&StyleFontFamily::System(
                AzString::from_const_str("Arial")
            )))
            .is_none());
    }

    #[test]
    fn renderer_resources_gc_helper_is_a_noop_on_empty_maps() {
        // The private helper must not panic (or wrongly prune) on empty maps.
        let mut rr = RendererResources::default();
        rr.remove_font_families_with_zero_references();
        assert!(rr.font_id_map.is_empty());
        assert!(rr.font_families_map.is_empty());

        // A family whose FontKey is NOT registered gets pruned; the families
        // map entry pointing at it is pruned too.
        let family = StyleFontFamily::System(AzString::from_const_str("Arial"));
        let family_hash = StyleFontFamilyHash::new(&family);
        let families_hash = StyleFontFamiliesHash::new(core::slice::from_ref(&family));
        rr.font_id_map
            .insert(family_hash, FontKey::unique(IdNamespace(1)));
        rr.font_families_map.insert(families_hash, family_hash);
        rr.remove_font_families_with_zero_references();
        assert!(
            rr.font_id_map.is_empty(),
            "dangling font key must be pruned"
        );
        assert!(rr.font_families_map.is_empty());
    }

    #[test]
    fn get_font_instance_key_for_text_is_none_on_empty_resources_for_all_sane_sizes() {
        // No fonts registered -> every lookup misses, whatever the size / DPI.
        // (0, negative and NaN sizes must not panic on the way to the miss.)
        let rr = RendererResources::default();
        let cache = CssPropertyCache::default();
        let node = NodeData::default();
        let node_id = NodeId::new(0);
        let state = StyledNodeState::default();

        for size in [0.0_f32, -0.0, 1.0, -12.0, f32::NAN, 1.0e6, -1.0e6] {
            for dpi in [1.0_f32, 0.0, -1.0, f32::NAN, f32::INFINITY] {
                assert!(
                    rr.get_font_instance_key_for_text(size, &cache, &node, &node_id, &state, dpi)
                        .is_none(),
                    "size={size} dpi={dpi} must miss cleanly"
                );
            }
        }
    }

    #[test]
    fn bug_get_font_instance_key_for_text_overflow_panics_on_infinite_font_size() {
        // `font_size_px as isize` saturates to isize::MAX for +inf / f32::MAX,
        // and FloatValue::const_new then computes `isize::MAX * 1000`, which
        // panics with "attempt to multiply with overflow" under the (default)
        // dev-profile overflow checks. A miss (None) is the correct behaviour.
        let rr = RendererResources::default();
        let cache = CssPropertyCache::default();
        let node = NodeData::default();
        let node_id = NodeId::new(0);
        let state = StyledNodeState::default();
        assert!(rr
            .get_font_instance_key_for_text(f32::INFINITY, &cache, &node, &node_id, &state, 1.0)
            .is_none());
    }

    // =====================================================================
    // Resource-update builders (end-to-end)
    // =====================================================================

    #[test]
    fn font_ref_get_hash_is_stable_per_font_and_distinct_across_fonts() {
        let a = dummy_font_ref();
        let b = dummy_font_ref();
        assert_eq!(font_ref_get_hash(&a), font_ref_get_hash(&a));
        assert_eq!(font_ref_get_hash(&a), font_ref_get_hash(&a.clone()));
        assert_ne!(
            font_ref_get_hash(&a),
            font_ref_get_hash(&b),
            "two distinct FontRefs must not share a hash"
        );
    }

    #[test]
    fn build_add_font_resource_updates_on_empty_input_is_empty() {
        let mut rr = RendererResources::default();
        let fonts = OrderedMap::new();
        let updates = build_add_font_resource_updates(
            &mut rr,
            DpiScaleFactor::new(1.0),
            &FcFontCache::default(),
            IdNamespace(1),
            &fonts,
            load_font_none,
            parse_font_none,
        );
        assert!(updates.is_empty());
        assert!(rr.font_id_map.is_empty());
    }

    #[test]
    fn build_add_font_resource_updates_skips_unloadable_fonts() {
        // A family that cannot be loaded (missing file) must not register
        // anything - it is retried next frame, not half-registered.
        let mut rr = RendererResources::default();
        let mut fonts = OrderedMap::new();
        let mut sizes = FastBTreeSet::new();
        sizes.insert(Au::from_px(16.0));
        fonts.insert(
            ImmediateFontId::Unresolved(StyleFontFamilyVec::from_vec(vec![
                StyleFontFamily::System(AzString::from_const_str("DoesNotExist")),
            ])),
            sizes,
        );

        let updates = build_add_font_resource_updates(
            &mut rr,
            DpiScaleFactor::new(1.0),
            &FcFontCache::default(),
            IdNamespace(1),
            &fonts,
            load_font_none,
            parse_font_none,
        );
        assert!(
            updates.is_empty(),
            "an unloadable font must add no resources"
        );
        assert!(rr.font_id_map.is_empty());
        assert!(rr.font_families_map.is_empty());
    }

    #[test]
    fn build_add_font_resource_updates_registers_a_font_and_deduplicates_sizes() {
        // A StyleFontFamily::Ref resolves without touching the loader, so this
        // exercises the whole add-font path deterministically.
        let mut rr = RendererResources::default();
        let font = dummy_font_ref();
        let family = StyleFontFamily::Ref(font.clone());
        let dpi = DpiScaleFactor::new(1.0);

        let mut sizes = FastBTreeSet::new();
        sizes.insert(Au::from_px(16.0));
        sizes.insert(Au::from_px(24.0));
        sizes.insert(Au::from_px(16.0)); // duplicate -> set dedups it
        assert_eq!(sizes.len(), 2);

        let mut fonts = OrderedMap::new();
        fonts.insert(
            ImmediateFontId::Unresolved(StyleFontFamilyVec::from_vec(vec![family.clone()])),
            sizes,
        );

        let updates = build_add_font_resource_updates(
            &mut rr,
            dpi,
            &FcFontCache::default(),
            IdNamespace(1),
            &fonts,
            load_font_none,
            parse_font_none,
        );
        // 1 AddFont + 2 AddFontInstance
        assert_eq!(updates.len(), 3);
        assert_eq!(
            updates
                .iter()
                .filter(|(_, m)| matches!(m, AddFontMsg::Font(..)))
                .count(),
            1
        );
        assert_eq!(
            updates
                .iter()
                .filter(|(_, m)| matches!(m, AddFontMsg::Instance(..)))
                .count(),
            2
        );
        assert_eq!(rr.font_id_map.len(), 1);
        assert_eq!(rr.font_families_map.len(), 1);

        // add_resources then makes the instances findable by (families, size, dpi).
        let mut all_updates = Vec::new();
        add_resources(&mut rr, &mut all_updates, updates, Vec::new());
        assert_eq!(all_updates.len(), 3);

        let families_hash = StyleFontFamiliesHash::new(core::slice::from_ref(&family));
        assert!(rr
            .get_font_instance_key(&families_hash, Au::from_px(16.0), dpi)
            .is_some());
        assert!(rr
            .get_font_instance_key(&families_hash, Au::from_px(24.0), dpi)
            .is_some());
        // A size that was never registered misses; so does a different DPI.
        assert!(rr
            .get_font_instance_key(&families_hash, Au::from_px(99.0), dpi)
            .is_none());
        assert!(rr
            .get_font_instance_key(&families_hash, Au::from_px(16.0), DpiScaleFactor::new(2.0))
            .is_none());

        // The instance key resolves back to the font (reverse lookup invariant).
        let key = rr
            .get_font_instance_key(&families_hash, Au::from_px(16.0), dpi)
            .expect("registered");
        let (font_ref, au, got_dpi) = rr
            .get_renderable_font_data(&key)
            .expect("registered instance must be renderable");
        assert_eq!(font_ref.get_hash(), font.get_hash());
        assert_eq!(au, Au::from_px(16.0));
        assert_eq!(got_dpi, dpi);

        // Rebuilding with the same font must not add anything a second time.
        let again = build_add_font_resource_updates(
            &mut rr,
            dpi,
            &FcFontCache::default(),
            IdNamespace(1),
            &fonts,
            load_font_none,
            parse_font_none,
        );
        assert!(
            again.is_empty(),
            "already-registered fonts must not be re-added"
        );
    }

    #[test]
    fn add_font_msg_into_resource_update_preserves_keys() {
        let font = dummy_font_ref();
        let key = FontKey::unique(IdNamespace(3));
        let family_hash = StyleFontFamilyHash::new(&StyleFontFamily::Ref(font.clone()));
        let msg = AddFontMsg::Font(key, family_hash, font.clone());
        match msg.into_resource_update() {
            ResourceUpdate::AddFont(add) => {
                assert_eq!(add.key, key);
                assert_eq!(add.font.get_hash(), font.get_hash());
            }
            other => panic!("expected AddFont, got {other:?}"),
        }
    }

    #[test]
    fn delete_font_msg_into_resource_update_preserves_keys() {
        let fk = FontKey::unique(IdNamespace(1));
        match DeleteFontMsg::Font(fk).into_resource_update() {
            ResourceUpdate::DeleteFont(k) => assert_eq!(k, fk),
            other => panic!("expected DeleteFont, got {other:?}"),
        }
        let fik = FontInstanceKey::unique(IdNamespace(1));
        let size = (Au::from_px(16.0), DpiScaleFactor::new(1.0));
        match DeleteFontMsg::Instance(fik, size).into_resource_update() {
            ResourceUpdate::DeleteFontInstance(k) => assert_eq!(k, fik),
            other => panic!("expected DeleteFontInstance, got {other:?}"),
        }
    }

    #[test]
    fn add_image_msg_into_resource_update_preserves_the_key_and_descriptor() {
        let key = ImageKey::unique(IdNamespace(2));
        let descriptor = ImageDescriptor {
            format: RawImageFormat::BGRA8,
            width: 3,
            height: 5,
            stride: None.into(),
            offset: 0,
            flags: ImageDescriptorFlags {
                is_opaque: false,
                allow_mipmaps: true,
            },
        };
        let msg = AddImageMsg(AddImage {
            key,
            descriptor,
            data: ImageData::Raw(SharedRawImageData::new(vec![0u8; 60].into())),
            tiling: None,
        });
        match msg.into_resource_update() {
            ResourceUpdate::AddImage(add) => {
                assert_eq!(add.key, key);
                assert_eq!(add.descriptor, descriptor);
                assert!(add.tiling.is_none());
            }
            other => panic!("expected AddImage, got {other:?}"),
        }
    }

    #[test]
    fn build_add_image_resource_updates_skips_null_and_callback_images() {
        // NullImage has nothing to upload, Callback runs after layout.
        let rr = RendererResources::default();
        let mut images = FastBTreeSet::new();
        images.insert(ImageRef::null_image(
            4,
            4,
            RawImageFormat::RGBA8,
            Vec::new(),
        ));
        images.insert(ImageRef::callback(0usize, RefAny::new(0u8)));

        let updates = build_add_image_resource_updates(
            &rr,
            IdNamespace(1),
            Epoch::new(),
            &test_document_id(),
            &images,
            store_gl_texture_noop,
        );
        assert!(updates.is_empty());

        // ... and an empty DOM produces no updates at all.
        let empty = FastBTreeSet::new();
        assert!(build_add_image_resource_updates(
            &rr,
            IdNamespace(1),
            Epoch::new(),
            &test_document_id(),
            &empty,
            store_gl_texture_noop,
        )
        .is_empty());
    }

    #[test]
    fn build_add_image_resource_updates_then_add_resources_round_trip() {
        let mut rr = RendererResources::default();
        let img = ImageRef::new_rawimage(rgba8_image(2, 2)).expect("valid 2x2");
        let hash = img.get_hash();
        let ns = IdNamespace(11);

        let mut images = FastBTreeSet::new();
        images.insert(img.clone());

        let updates = build_add_image_resource_updates(
            &rr,
            ns,
            Epoch::new(),
            &test_document_id(),
            &images,
            store_gl_texture_noop,
        );
        assert_eq!(updates.len(), 1);
        assert_eq!(updates[0].0, hash);
        // The ImageKey is derived from the hash (no separate mapping table).
        assert_eq!(updates[0].1 .0.key, image_ref_hash_to_image_key(hash, ns));
        assert_eq!(updates[0].1 .0.descriptor.width, 2);
        assert_eq!(updates[0].1 .0.descriptor.height, 2);

        let key = updates[0].1 .0.key;
        let mut all_updates = Vec::new();
        add_resources(&mut rr, &mut all_updates, Vec::new(), updates);
        assert_eq!(all_updates.len(), 1);
        assert!(matches!(all_updates[0], ResourceUpdate::AddImage(_)));

        // Forward and reverse maps must agree after registration.
        assert_eq!(rr.get_image(&hash).map(|r| r.key), Some(key));
        assert_eq!(rr.image_key_map.get(&key), Some(&hash));

        // An already-registered image is never re-uploaded.
        let again = build_add_image_resource_updates(
            &rr,
            ns,
            Epoch::new(),
            &test_document_id(),
            &images,
            store_gl_texture_noop,
        );
        assert!(again.is_empty());

        // update_image mutates the descriptor in place, keeping the key.
        let new_descriptor = ImageDescriptor {
            format: RawImageFormat::BGRA8,
            width: 8,
            height: 8,
            stride: None.into(),
            offset: 0,
            flags: ImageDescriptorFlags {
                is_opaque: true,
                allow_mipmaps: true,
            },
        };
        rr.update_image(&hash, new_descriptor);
        assert_eq!(rr.get_image(&hash).map(|r| r.descriptor.width), Some(8));
        assert_eq!(rr.get_image(&hash).map(|r| r.key), Some(key));
        // Updating an unknown hash is a silent no-op, not a panic.
        rr.update_image(&ImageRefHash { inner: u64::MAX }, new_descriptor);
    }

    #[test]
    fn add_resources_with_empty_input_changes_nothing() {
        let mut rr = RendererResources::default();
        let mut updates = Vec::new();
        add_resources(&mut rr, &mut updates, Vec::new(), Vec::new());
        assert!(updates.is_empty());
        assert!(rr.currently_registered_images.is_empty());
        assert!(rr.currently_registered_fonts.is_empty());
        assert!(rr.image_key_map.is_empty());
    }

    // =====================================================================
    // NUMERIC: CPU painting (paint_dot / paint_stroke)
    // =====================================================================

    #[test]
    fn paint_dot_composites_at_the_center_and_leaves_far_pixels_alone() {
        let mut img = rgba8_image(4, 4);
        img.paint_dot(2.0, 2.0, Brush::new(opaque_red(), 2.0));
        let px = img.pixels.get_u8_vec_ref().expect("u8").as_ref().to_vec();

        // Pixel (1,1) is 0.71 px from the center -> inside the hard core.
        let idx = (4 + 1) * 4;
        assert_eq!(
            &px[idx..idx + 4],
            &[255, 0, 0, 255],
            "center pixel must be opaque red"
        );
        // Pixel (0,0) is 2.12 px away -> outside the radius -> untouched.
        assert_eq!(
            &px[0..4],
            &[0, 0, 0, 0],
            "pixels beyond the radius stay untouched"
        );
    }

    #[test]
    fn paint_dot_honours_bgra_channel_order() {
        let mut img = rgba8_image(4, 4);
        img.data_format = RawImageFormat::BGRA8;
        img.paint_dot(2.0, 2.0, Brush::new(opaque_red(), 2.0));
        let px = img.pixels.get_u8_vec_ref().expect("u8").as_ref().to_vec();
        let idx = (4 + 1) * 4;
        // BGRA: red lands in byte 2, blue in byte 0.
        assert_eq!(&px[idx..idx + 4], &[0, 0, 255, 255]);
    }

    #[test]
    fn paint_dot_rejects_degenerate_radii_and_sizes() {
        let untouched = |img: &RawImage| {
            img.pixels
                .get_u8_vec_ref()
                .expect("u8")
                .as_ref()
                .iter()
                .all(|b| *b == 0)
        };

        // radius <= 0 and NaN radius are no-ops (the `!(r > 0.0)` guard).
        for r in [0.0_f32, -1.0, -0.0, f32::NAN, f32::NEG_INFINITY] {
            let mut img = rgba8_image(4, 4);
            img.paint_dot(2.0, 2.0, Brush::new(opaque_red(), r));
            assert!(untouched(&img), "radius {r} must not paint");
        }

        // Zero-sized images are no-ops (and must not index out of bounds).
        let mut img = rgba8_image(0, 0);
        img.paint_dot(0.0, 0.0, Brush::new(opaque_red(), 4.0));
        assert_eq!(img.pixels.get_u8_vec_ref().map(|v| v.len()), Some(0));

        // Non-8-bit-RGBA formats are documented as left untouched.
        for format in [
            RawImageFormat::R8,
            RawImageFormat::RGB8,
            RawImageFormat::RGBA16,
            RawImageFormat::RGBAF32,
        ] {
            let mut img = rgba8_image(4, 4);
            img.data_format = format;
            img.paint_dot(2.0, 2.0, Brush::new(opaque_red(), 2.0));
            assert!(untouched(&img), "format {format:?} must not be painted");
        }
    }

    #[test]
    fn paint_dot_with_nan_and_infinite_coordinates_is_a_safe_noop() {
        // NaN / +-inf centers collapse the scan range to nothing instead of
        // producing an out-of-bounds index.
        for (cx, cy) in [
            (f32::NAN, 2.0_f32),
            (2.0, f32::NAN),
            (f32::NAN, f32::NAN),
            (f32::INFINITY, 2.0),
            (f32::NEG_INFINITY, 2.0),
            (2.0, f32::INFINITY),
            (1.0e30, 1.0e30),
            (-1.0e30, -1.0e30),
        ] {
            let mut img = rgba8_image(4, 4);
            img.paint_dot(cx, cy, Brush::new(opaque_red(), 2.0));
            let px = img.pixels.get_u8_vec_ref().expect("u8").as_ref().to_vec();
            assert!(
                px.iter().all(|b| *b == 0),
                "({cx}, {cy}) must not paint anything"
            );
        }
    }

    #[test]
    fn paint_dot_alpha_saturates_and_never_overflows() {
        // Repeatedly stamping an opaque dab must clamp at 255, never wrap.
        let mut img = rgba8_image(4, 4);
        let brush = Brush::new(opaque_red(), 2.0);
        for _ in 0..50 {
            img.paint_dot(2.0, 2.0, brush);
        }
        let px = img.pixels.get_u8_vec_ref().expect("u8").as_ref().to_vec();
        let idx = (4 + 1) * 4;
        assert_eq!(&px[idx..idx + 4], &[255, 0, 0, 255]);

        // A NaN hardness makes the coverage NaN; the `a <= 0.0` check is false
        // for NaN, so the blend runs with a NaN alpha -- the `.clamp(0, 255)`
        // on the result keeps every channel a valid u8 (NaN clamps to 0 here),
        // i.e. garbage-in stays in-range instead of wrapping.
        let mut img = rgba8_image(4, 4);
        let mut nan_brush = Brush::new(opaque_red(), 2.0);
        nan_brush.hardness = f32::NAN;
        img.paint_dot(2.0, 2.0, nan_brush);
        // No assertion on the exact value: the point is that it did not panic
        // and every byte is (trivially) a valid u8.
        assert_eq!(img.pixels.get_u8_vec_ref().map(|v| v.len()), Some(64));
    }

    #[test]
    fn paint_dot_zero_flow_and_transparent_color_do_not_paint() {
        let mut img = rgba8_image(4, 4);
        let mut brush = Brush::new(opaque_red(), 2.0);
        brush.flow = 0.0;
        img.paint_dot(2.0, 2.0, brush);
        assert!(img
            .pixels
            .get_u8_vec_ref()
            .expect("u8")
            .as_ref()
            .iter()
            .all(|b| *b == 0));

        let mut img = rgba8_image(4, 4);
        let transparent = ColorU {
            r: 255,
            g: 0,
            b: 0,
            a: 0,
        };
        img.paint_dot(2.0, 2.0, Brush::new(transparent, 2.0));
        assert!(img
            .pixels
            .get_u8_vec_ref()
            .expect("u8")
            .as_ref()
            .iter()
            .all(|b| *b == 0));

        // A negative / >1 flow is clamped, not extrapolated.
        let mut img = rgba8_image(4, 4);
        let mut brush = Brush::new(opaque_red(), 2.0);
        brush.flow = -5.0;
        img.paint_dot(2.0, 2.0, brush);
        assert!(img
            .pixels
            .get_u8_vec_ref()
            .expect("u8")
            .as_ref()
            .iter()
            .all(|b| *b == 0));
    }

    #[test]
    fn paint_stroke_paints_both_endpoints() {
        let mut img = rgba8_image(8, 8);
        img.paint_stroke(1.5, 1.5, 6.5, 6.5, Brush::new(opaque_red(), 1.5));
        let px = img.pixels.get_u8_vec_ref().expect("u8").as_ref().to_vec();
        let alpha_at = |x: usize, y: usize| px[(y * 8 + x) * 4 + 3];
        assert!(alpha_at(1, 1) > 0, "start of the stroke must be painted");
        assert!(alpha_at(6, 6) > 0, "end of the stroke must be painted");
        assert_eq!(alpha_at(7, 0), 0, "off-line pixels stay untouched");
    }

    #[test]
    fn paint_stroke_zero_length_stamps_a_single_dab() {
        // len == 0 -> n == 0 -> the `n <= 0` branch stamps one dab at (x1, y1).
        let mut img = rgba8_image(4, 4);
        img.paint_stroke(2.0, 2.0, 2.0, 2.0, Brush::new(opaque_red(), 2.0));
        let px = img.pixels.get_u8_vec_ref().expect("u8").as_ref().to_vec();
        let idx = (4 + 1) * 4;
        assert_eq!(&px[idx..idx + 4], &[255, 0, 0, 255]);
    }

    #[test]
    fn paint_stroke_degenerate_brush_params_do_not_divide_by_zero_or_hang() {
        // spacing == 0 / negative: the `.max(0.01)` and `.max(0.5)` floors keep
        // the step positive, so the dab count stays finite.
        for spacing in [0.0_f32, -1.0, f32::NAN] {
            let mut img = rgba8_image(8, 8);
            let mut brush = Brush::new(opaque_red(), 2.0);
            brush.spacing = spacing;
            img.paint_stroke(0.0, 0.0, 7.0, 7.0, brush);
            assert_eq!(
                img.pixels.get_u8_vec_ref().map(|v| v.len()),
                Some(8 * 8 * 4)
            );
        }

        // A NaN endpoint yields a NaN length -> n == 0 -> one (no-op) dab.
        let mut img = rgba8_image(4, 4);
        img.paint_stroke(f32::NAN, 0.0, 1.0, 1.0, Brush::new(opaque_red(), 1.0));
        assert_eq!(img.pixels.get_u8_vec_ref().map(|v| v.len()), Some(64));

        // radius == 0 -> every dab is a no-op, and the loop still terminates.
        let mut img = rgba8_image(4, 4);
        img.paint_stroke(0.0, 0.0, 3.0, 3.0, Brush::new(opaque_red(), 0.0));
        assert!(img
            .pixels
            .get_u8_vec_ref()
            .expect("u8")
            .as_ref()
            .iter()
            .all(|b| *b == 0));
    }

    #[test]
    fn bug_paint_stroke_with_infinite_endpoint_loops_2_billion_times() {
        // `len` is +inf, `step` is finite, so `n = (inf / step).floor() as i32`
        // saturates to i32::MAX and the `for i in 0..=n` loop runs 2^31 times.
        // paint_stroke should clamp `n` (or bail on a non-finite length).
        let mut img = rgba8_image(4, 4);
        img.paint_stroke(0.0, 0.0, f32::INFINITY, 0.0, Brush::new(opaque_red(), 2.0));
    }

    #[test]
    fn bug_paint_dot_indexes_out_of_bounds_when_dims_exceed_the_buffer() {
        // RawImage's fields are all public, so a caller can hand paint_dot a
        // 100x100 image backed by 4 bytes. paint_dot computes the index from
        // width/height and indexes `buf` unchecked -> panic. It should clamp the
        // scan rect to the buffer length (or bail).
        let mut img = RawImage {
            pixels: RawImageData::U8(vec![0u8; 4].into()),
            width: 100,
            height: 100,
            premultiplied_alpha: true,
            data_format: RawImageFormat::RGBA8,
            tag: Vec::new().into(),
        };
        img.paint_dot(50.0, 50.0, Brush::new(opaque_red(), 4.0));
    }

    #[test]
    fn bug_into_loaded_image_source_overflows_on_huge_dimensions() {
        // Documented contract: "Returns None if the width * height * BPP does not
        // match". With width == usize::MAX the multiplication overflows and
        // panics under the dev-profile overflow checks instead.
        let img = RawImage {
            pixels: RawImageData::U8(vec![0u8; 4].into()),
            width: usize::MAX,
            height: 2,
            premultiplied_alpha: true,
            data_format: RawImageFormat::RGBA8,
            tag: Vec::new().into(),
        };
        assert!(img.into_loaded_image_source().is_none());
    }
}
