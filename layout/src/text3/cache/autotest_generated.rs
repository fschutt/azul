    use super::*;

    // ---------------------------------------------------------------------
    // Fixtures
    // ---------------------------------------------------------------------

    fn metrics(upem: u16, ascent: f32, descent: f32, line_gap: f32) -> LayoutFontMetrics {
        LayoutFontMetrics {
            ascent,
            descent,
            line_gap,
            units_per_em: upem,
            x_height: None,
            cap_height: None,
            browser_ascent_boost: false,
        }
    }

    /// 1000 upem, 800 asc, -200 desc, 0 gap → `line-height: normal` == 1.0em.
    fn std_metrics() -> LayoutFontMetrics {
        metrics(1000, 800.0, -200.0, 0.0)
    }

    fn style() -> Arc<StyleProperties> {
        Arc::new(StyleProperties::default())
    }

    fn styled(f: impl FnOnce(&mut StyleProperties)) -> Arc<StyleProperties> {
        let mut s = StyleProperties::default();
        f(&mut s);
        Arc::new(s)
    }

    const fn ci(run: u32, item: u32) -> ContentIndex {
        ContentIndex {
            run_index: run,
            item_index: item,
        }
    }

    const fn gid(run: u32, byte: u32) -> GraphemeClusterId {
        GraphemeClusterId {
            source_run: run,
            start_byte_in_run: byte,
        }
    }

    fn shaped_glyph(st: Arc<StyleProperties>, fm: LayoutFontMetrics, advance: f32) -> ShapedGlyph {
        ShapedGlyph {
            kind: GlyphKind::Character,
            glyph_id: 42,
            cluster_offset: 0,
            advance,
            kerning: 0.0,
            offset: Point { x: 0.0, y: 0.0 },
            vertical_advance: advance,
            vertical_offset: Point { x: 0.0, y: 0.0 },
            script: Script::Latin,
            font_hash: 0xABCD_u64,
            font_metrics: fm,
        }
    }

    fn make_cluster(
        text: &str,
        advance: f32,
        st: Arc<StyleProperties>,
        glyphs: ShapedGlyphVec,
        id: GraphemeClusterId,
    ) -> ShapedItem {
        ShapedItem::Cluster(ShapedCluster {
            flags: ClusterFlags::classify(text),
            source_text: {
                // Test helper: pad so the slice at the given id's offset
                // yields exactly `text` (production stamps a shared Arc
                // whose offsets are real; tests mint ids freely).
                let mut s = String::new();
                for _ in 0..id.start_byte_in_run {
                    s.push(' ');
                }
                s.push_str(text);
                Arc::from(s.as_str())
            },
            source_byte_len: text.len() as u16,
            source_cluster_id: id,
            source_content_index: ci(id.source_run, id.start_byte_in_run),
            source_node_id: None,
            glyphs,
            advance,
            direction: BidiDirection::Ltr,
            style: st,
            marker_position_outside: None,
            is_first_fragment: true,
            is_last_fragment: true,
        })
    }

    /// Single-glyph cluster with the standard 1000-upem metrics.
    fn cl(text: &str, advance: f32) -> ShapedItem {
        let st = style();
        let g = shaped_glyph(st.clone(), std_metrics(), advance);
        make_cluster(text, advance, st, smallvec![g], gid(0, 0))
    }

    /// Single-glyph cluster with an explicit grapheme id (for caret tests).
    fn cl_at(text: &str, advance: f32, run: u32, byte: u32) -> ShapedItem {
        let st = style();
        let g = shaped_glyph(st.clone(), std_metrics(), advance);
        make_cluster(text, advance, st, smallvec![g], gid(run, byte))
    }

    /// Cluster carrying an explicit style (letter/word-spacing tests).
    fn cl_styled(text: &str, advance: f32, st: Arc<StyleProperties>) -> ShapedItem {
        let g = shaped_glyph(st.clone(), std_metrics(), advance);
        make_cluster(text, advance, st, smallvec![g], gid(0, 0))
    }

    /// Cluster with NO glyphs — the CSS "strut" case.
    fn cl_no_glyphs(text: &str, advance: f32) -> ShapedItem {
        make_cluster(text, advance, style(), ShapedGlyphVec::new(), gid(0, 0))
    }

    fn obj(width: f32, height: f32, baseline_offset: f32) -> ShapedItem {
        ShapedItem::Object {
            source: ci(0, 0),
            bounds: Rect {
                x: 0.0,
                y: 0.0,
                width,
                height,
            },
            baseline_offset,
            content: InlineContent::Space(InlineSpace {
                width,
                is_breaking: false,
                is_stretchy: false,
            }),
        }
    }

    fn brk() -> ShapedItem {
        ShapedItem::Break {
            source: ci(0, 0),
            break_info: InlineBreak {
                break_type: BreakType::Hard,
                clear: ClearType::None,
                content_index: 0,
            },
        }
    }

    fn tab(width: f32, height: f32) -> ShapedItem {
        ShapedItem::Tab {
            source: ci(0, 0),
            bounds: Rect {
                x: 0.0,
                y: 0.0,
                width,
                height,
            },
        }
    }

    fn pos(item: ShapedItem, x: f32, y: f32, line_index: usize) -> PositionedItem {
        PositionedItem {
            item,
            position: Point { x, y },
            line_index,
        }
    }

    fn text_content(t: &str, st: Arc<StyleProperties>) -> InlineContent {
        InlineContent::Text(StyledRun {
            text: Arc::from(t),
            style: st,
            logical_start_byte: 0,
            source_node_id: None,
        })
    }

    fn sel(family: &str) -> FontSelector {
        FontSelector {
            family: family.to_string(),
            ..FontSelector::default()
        }
    }

    /// A minimal in-memory `ParsedFontTrait` so `LoadedFonts` / `FontManager`
    /// can be exercised without touching the filesystem or fontconfig.
    #[derive(Debug, Clone)]
    struct TestFont {
        hash: u64,
    }

    impl ShallowClone for TestFont {
        fn shallow_clone(&self) -> Self {
            self.clone()
        }
    }

    impl ParsedFontTrait for TestFont {
        fn shape_text(
            &self,
            _text: &str,
            _script: Script,
            _language: Language,
            _direction: BidiDirection,
            _style: &StyleProperties,
        ) -> Result<Vec<Glyph>, LayoutError> {
            Ok(Vec::new())
        }
        fn get_hash(&self) -> u64 {
            self.hash
        }
        fn get_glyph_size(&self, _glyph_id: u16, font_size: f32) -> Option<LogicalSize> {
            Some(LogicalSize {
                width: font_size,
                height: font_size,
            })
        }
        fn get_hyphen_glyph_and_advance(&self, font_size: f32) -> Option<(u16, f32)> {
            Some((1, font_size * 0.3))
        }
        fn get_kashida_glyph_and_advance(&self, font_size: f32) -> Option<(u16, f32)> {
            Some((2, font_size * 0.2))
        }
        fn has_glyph(&self, _codepoint: u32) -> bool {
            true
        }
        fn get_vertical_metrics(&self, _glyph_id: u16) -> Option<VerticalMetrics> {
            None
        }
        fn get_font_metrics(&self) -> LayoutFontMetrics {
            std_metrics()
        }
        fn num_glyphs(&self) -> u16 {
            10
        }
        fn get_space_width(&self) -> Option<usize> {
            Some(500)
        }
    }

    fn hash_of<T: Hash>(v: &T) -> u64 {
        let mut h = DefaultHasher::new();
        v.hash(&mut h);
        h.finish()
    }

    /// Compare two derived f32s. Used wherever the expected value comes out of a
    /// divide-then-multiply chain, whose last-ulp rounding is not worth pinning.
    #[track_caller]
    fn approx(actual: f32, expected: f32) {
        assert!(
            (actual - expected).abs() < 1e-4,
            "expected ~{expected}, got {actual}"
        );
    }

    // =====================================================================
    // numeric: ruby_reserved_box
    // =====================================================================

    #[test]
    fn ruby_reserved_box_zero_and_negative_are_deterministic() {
        assert_eq!(ruby_reserved_box(0.0, 0.0, 0.0, 0.0), (0.0, 0.0));
        // max() of two negatives is the one closer to zero; the block-size sums.
        let (w, h) = ruby_reserved_box(-10.0, -4.0, -3.0, -2.0);
        assert_eq!(w, -4.0);
        assert_eq!(h, -5.0);
    }

    #[test]
    fn ruby_reserved_box_nan_width_is_ignored_by_max_but_poisons_height() {
        // f32::max propagates the NON-NaN operand, so a NaN advance silently
        // yields the other run's width instead of NaN.
        let (w, h) = ruby_reserved_box(f32::NAN, 30.0, 24.0, f32::NAN);
        assert_eq!(w, 30.0, "f32::max discards the NaN operand");
        assert!(h.is_nan(), "but the additive block-size does propagate NaN");
    }

    #[test]
    fn ruby_reserved_box_infinities_do_not_panic() {
        let (w, h) = ruby_reserved_box(f32::INFINITY, 10.0, f32::INFINITY, f32::NEG_INFINITY);
        assert!(w.is_infinite() && w.is_sign_positive());
        assert!(h.is_nan(), "inf + -inf is NaN, not a panic");
    }

    #[test]
    fn ruby_reserved_box_saturates_to_infinity_at_f32_max() {
        let (w, h) = ruby_reserved_box(f32::MAX, f32::MAX, f32::MAX, f32::MAX);
        assert_eq!(w, f32::MAX);
        assert!(h.is_infinite(), "f32 addition saturates, it does not panic");
    }

    // =====================================================================
    // numeric: LineHeight::resolve / resolve_with_metrics
    // =====================================================================

    #[test]
    fn line_height_px_ignores_every_font_metric() {
        let lh = LineHeight::Px(7.5);
        assert_eq!(lh.resolve(16.0, 800.0, -200.0, 0.0, 1000), 7.5);
        // Even garbage metrics cannot perturb an explicit px value.
        assert_eq!(lh.resolve(f32::NAN, f32::NAN, f32::NAN, f32::NAN, 0), 7.5);
    }

    #[test]
    fn line_height_normal_zero_upem_falls_back_to_1_2_em() {
        let lh = LineHeight::Normal;
        assert_eq!(lh.resolve(16.0, 800.0, -200.0, 0.0, 0), 19.2);
        assert_eq!(lh.resolve(0.0, 800.0, -200.0, 0.0, 0), 0.0);
    }

    #[test]
    fn line_height_normal_scales_ascent_minus_descent_plus_gap() {
        // (800 - (-200) + 0) / 1000 * 16 == 16.0
        approx(
            LineHeight::Normal.resolve(16.0, 800.0, -200.0, 0.0, 1000),
            16.0,
        );
        // line_gap widens the line box.
        approx(
            LineHeight::Normal.resolve(16.0, 800.0, -200.0, 250.0, 1000),
            20.0,
        );
        // A descent given with the WRONG (positive) sign lies ABOVE the
        // baseline: the line keeps no descent at all (floored at zero), and
        // the ascent alone rounds to 13px (MAILENG6: rounded like Chrome).
        approx(
            LineHeight::Normal.resolve(16.0, 800.0, 200.0, 0.0, 1000),
            13.0,
        );
        // Each of A, D and G is rounded to whole pixels BEFORE the sum
        // (Blink): Arial 16px is 14 + 3 + 1 = 18, not the 18.4 of the
        // unrounded sum.
        approx(
            LineHeight::Normal.resolve(16.0, 1854.0, -434.0, 67.0, 2048),
            18.0,
        );
    }

    #[test]
    fn line_height_normal_at_u16_max_upem_does_not_panic() {
        let v = LineHeight::Normal.resolve(16.0, 800.0, -200.0, 0.0, u16::MAX);
        assert!(v.is_finite() && v >= 0.0, "got {v}");
        assert!(
            v < 1.0,
            "a 65535-upem font must produce a tiny scale (0.2px of ascent rounds to 0), got {v}"
        );
    }

    #[test]
    fn line_height_normal_nan_and_inf_inputs_are_defined_not_panics() {
        assert!(LineHeight::Normal
            .resolve(f32::NAN, 800.0, -200.0, 0.0, 1000)
            .is_nan());
        assert!(LineHeight::Normal
            .resolve(f32::INFINITY, 800.0, -200.0, 0.0, 1000)
            .is_infinite());
        // ascent == descent == +inf: the (positive) descent is floored at
        // zero, the ascent stays infinite.
        assert!(LineHeight::Normal
            .resolve(16.0, f32::INFINITY, f32::INFINITY, 0.0, 1000)
            .is_infinite());
    }

    #[test]
    fn line_height_resolve_with_metrics_matches_resolve() {
        let fm = metrics(2048, 1600.0, -400.0, 100.0);
        let lh = LineHeight::Normal;
        assert_eq!(
            lh.resolve_with_metrics(24.0, &fm),
            lh.resolve(24.0, fm.ascent, fm.descent, fm.line_gap, fm.units_per_em)
        );
        // Px path is metric-independent.
        assert_eq!(LineHeight::Px(3.0).resolve_with_metrics(24.0, &fm), 3.0);
    }

    #[test]
    fn line_height_px_nan_is_self_equal_under_the_manual_partialeq() {
        // The manual PartialEq compares raw bits, so Px(NaN) == Px(NaN) even
        // though NaN != NaN — required for Hash/Eq consistency of the cache key.
        assert_eq!(LineHeight::Px(f32::NAN), LineHeight::Px(f32::NAN));
        assert_eq!(
            hash_of(&LineHeight::Px(f32::NAN)),
            hash_of(&LineHeight::Px(f32::NAN))
        );
        assert_ne!(LineHeight::Normal, LineHeight::Px(0.0));
    }

    // =====================================================================
    // AvailableSpace (predicate / numeric / constructor)
    // =====================================================================

    #[test]
    fn available_space_definite_and_indefinite_are_exact_complements() {
        for v in [
            AvailableSpace::Definite(0.0),
            AvailableSpace::Definite(-1.0),
            AvailableSpace::Definite(f32::NAN),
            AvailableSpace::MinContent,
            AvailableSpace::MaxContent,
        ] {
            assert_ne!(v.is_definite(), v.is_indefinite(), "{v:?}");
        }
        assert!(AvailableSpace::Definite(f32::NAN).is_definite());
        assert!(AvailableSpace::default().is_indefinite());
        assert_eq!(AvailableSpace::default(), AvailableSpace::MaxContent);
    }

    #[test]
    fn available_space_unwrap_or_returns_definite_even_when_nan_or_inf() {
        assert_eq!(AvailableSpace::Definite(0.0).unwrap_or(99.0), 0.0);
        assert_eq!(AvailableSpace::Definite(-5.0).unwrap_or(99.0), -5.0);
        assert!(AvailableSpace::Definite(f32::NAN).unwrap_or(99.0).is_nan());
        assert!(AvailableSpace::Definite(f32::INFINITY)
            .unwrap_or(99.0)
            .is_infinite());
        // Indefinite variants hand back the fallback verbatim, NaN included.
        assert_eq!(AvailableSpace::MinContent.unwrap_or(99.0), 99.0);
        assert_eq!(AvailableSpace::MaxContent.unwrap_or(-0.0), -0.0);
        assert!(AvailableSpace::MaxContent.unwrap_or(f32::NAN).is_nan());
    }

    #[test]
    fn available_space_to_f32_for_layout_uses_half_max_for_both_intrinsic_modes() {
        assert_eq!(
            AvailableSpace::MinContent.to_f32_for_layout(),
            f32::MAX / 2.0
        );
        assert_eq!(
            AvailableSpace::MaxContent.to_f32_for_layout(),
            f32::MAX / 2.0
        );
        assert_eq!(AvailableSpace::Definite(12.5).to_f32_for_layout(), 12.5);
        assert!(AvailableSpace::Definite(f32::NAN)
            .to_f32_for_layout()
            .is_nan());
    }

    #[test]
    fn available_space_from_f32_sentinels() {
        assert_eq!(
            AvailableSpace::from_f32(f32::INFINITY),
            AvailableSpace::MaxContent
        );
        assert_eq!(
            AvailableSpace::from_f32(f32::MAX),
            AvailableSpace::MaxContent
        );
        // The documented cut-over point is exactly MAX/2 (inclusive).
        assert_eq!(
            AvailableSpace::from_f32(f32::MAX / 2.0),
            AvailableSpace::MaxContent
        );
        assert_eq!(AvailableSpace::from_f32(0.0), AvailableSpace::MinContent);
        assert_eq!(AvailableSpace::from_f32(-0.0), AvailableSpace::MinContent);
        assert_eq!(AvailableSpace::from_f32(-1.0), AvailableSpace::MinContent);
        assert_eq!(
            AvailableSpace::from_f32(100.0),
            AvailableSpace::Definite(100.0)
        );
    }

    #[test]
    fn available_space_from_f32_negative_infinity_becomes_max_content() {
        // QUIRK worth pinning: the `is_infinite()` guard runs FIRST, so -inf —
        // a nonsensical width — resolves to MaxContent ("no wrapping"), not the
        // MinContent that every other negative value maps to.
        assert_eq!(
            AvailableSpace::from_f32(f32::NEG_INFINITY),
            AvailableSpace::MaxContent
        );
    }

    #[test]
    fn available_space_from_f32_nan_falls_through_to_definite_nan() {
        // NaN fails is_infinite(), fails `>= MAX/2`, and fails `<= 0.0`, so it
        // lands in the Definite arm and a NaN width is smuggled into layout.
        let got = AvailableSpace::from_f32(f32::NAN);
        match got {
            AvailableSpace::Definite(v) => assert!(v.is_nan(), "expected Definite(NaN)"),
            other => panic!("NaN should fall through to Definite, got {other:?}"),
        }
        // ...and Definite(NaN) is not even equal to itself under the derived PartialEq.
        assert_ne!(got, AvailableSpace::from_f32(f32::NAN));
    }

    #[test]
    fn available_space_hash_eq_contract_holds_for_signed_zero() {
        // +0.0 == -0.0 under PartialEq, so their hashes MUST agree.
        assert_eq!(
            AvailableSpace::Definite(0.0),
            AvailableSpace::Definite(-0.0)
        );
        assert_eq!(
            hash_of(&AvailableSpace::Definite(0.0)),
            hash_of(&AvailableSpace::Definite(-0.0))
        );
        // Sub-pixel widths must NOT collide (they wrap lines differently).
        assert_ne!(
            hash_of(&AvailableSpace::Definite(100.1)),
            hash_of(&AvailableSpace::Definite(100.4))
        );
        assert_ne!(
            hash_of(&AvailableSpace::MinContent),
            hash_of(&AvailableSpace::MaxContent)
        );
    }

    // =====================================================================
    // constructor: FontChainKey / FontChainKeyOrRef / FontStack / FontHash
    // =====================================================================

    #[test]
    fn font_chain_key_from_empty_selectors_defaults_to_serif() {
        let k = FontChainKey::from_selectors(&[]);
        assert_eq!(k.font_families, vec!["serif".to_string()]);
        assert_eq!(k.weight, FcWeight::Normal);
        assert!(!k.italic && !k.oblique);
    }

    #[test]
    fn font_chain_key_dedups_first_wins_and_skips_empty_families() {
        let stack = [sel("Arial"), sel("Times"), sel("Arial"), sel("")];
        let k = FontChainKey::from_selectors(&stack);
        assert_eq!(
            k.font_families,
            vec!["Arial".to_string(), "Times".to_string()],
            "duplicate families must collapse first-wins, empty names dropped"
        );
    }

    #[test]
    fn font_chain_key_all_empty_families_still_yields_serif() {
        let stack = [sel(""), sel(""), sel("")];
        let k = FontChainKey::from_selectors(&stack);
        assert_eq!(k.font_families, vec!["serif".to_string()]);
    }

    #[test]
    fn font_chain_key_weight_and_style_come_from_the_first_selector_even_if_it_is_dropped() {
        // QUIRK: the first selector's family is skipped (empty), but its weight
        // and italic flag still win — the key describes a family it does not list.
        let mut first = sel("");
        first.style = FontStyle::Italic;
        first.weight = FcWeight::Bold;
        let stack = [first, sel("Arial")];
        let k = FontChainKey::from_selectors(&stack);
        assert_eq!(k.font_families, vec!["Arial".to_string()]);
        assert_eq!(k.weight, FcWeight::Bold);
        assert!(k.italic, "italic taken from the dropped first selector");
        assert!(!k.oblique);
    }

    #[test]
    fn font_chain_key_oblique_is_exclusive_of_italic() {
        let mut s = sel("Arial");
        s.style = FontStyle::Oblique;
        let k = FontChainKey::from_selectors(&[s]);
        assert!(k.oblique && !k.italic);
    }

    #[test]
    fn font_chain_key_huge_duplicate_stack_does_not_hang() {
        let stack: Vec<FontSelector> = (0..5000).map(|_| sel("Arial")).collect();
        let k = FontChainKey::from_selectors(&stack);
        assert_eq!(k.font_families.len(), 1, "5000 dupes collapse to one entry");
    }

    #[test]
    fn font_chain_key_is_a_stable_hash_map_key() {
        let a = FontChainKey::from_selectors(&[sel("Arial"), sel("Times")]);
        let b = FontChainKey::from_selectors(&[sel("Arial"), sel("Arial"), sel("Times")]);
        assert_eq!(a, b, "dedup makes the two stacks resolve to the same key");
        assert_eq!(hash_of(&a), hash_of(&b));
    }

    #[test]
    fn font_chain_key_or_ref_from_stack_is_a_chain() {
        let fs = FontStack::Stack(vec![sel("Arial")]);
        let k = FontChainKeyOrRef::from_font_stack(&fs);
        assert!(!k.is_ref());
        assert_eq!(k.as_ref_ptr(), None);
        assert_eq!(
            k.as_chain().map(|c| c.font_families.clone()),
            Some(vec!["Arial".to_string()])
        );
    }

    #[test]
    fn font_chain_key_or_ref_ref_variant_accessors_at_boundaries() {
        for ptr in [0_usize, 1, usize::MAX] {
            let k = FontChainKeyOrRef::Ref(ptr);
            assert!(k.is_ref());
            assert_eq!(k.as_ref_ptr(), Some(ptr));
            assert!(k.as_chain().is_none());
        }
        // A null-pointer Ref is still distinguishable from a Chain.
        assert_ne!(
            FontChainKeyOrRef::Ref(0),
            FontChainKeyOrRef::Chain(FontChainKey::from_selectors(&[]))
        );
    }

    #[test]
    fn font_stack_default_is_a_single_serif_selector() {
        let fs = FontStack::default();
        assert!(!fs.is_ref());
        assert!(fs.as_ref().is_none());
        assert_eq!(fs.as_stack().map(<[FontSelector]>::len), Some(1));
        assert_eq!(
            fs.first_selector().map(|s| s.family.as_str()),
            Some("serif")
        );
        assert_eq!(fs.first_family(), "serif");
    }

    #[test]
    fn font_stack_empty_stack_reports_serif_placeholder_but_no_first_selector() {
        let fs = FontStack::Stack(Vec::new());
        assert_eq!(fs.as_stack().map(<[FontSelector]>::len), Some(0));
        assert!(fs.first_selector().is_none());
        assert_eq!(
            fs.first_family(),
            "serif",
            "an EMPTY stack must not panic; it reports the serif fallback"
        );
    }

    #[test]
    fn font_hash_invalid_is_zero_and_is_the_default() {
        assert_eq!(FontHash::invalid().font_hash, 0);
        assert_eq!(FontHash::default(), FontHash::invalid());
        assert_eq!(FontHash::from_hash(0), FontHash::invalid());
        assert_eq!(FontHash::from_hash(u64::MAX).font_hash, u64::MAX);
        assert_ne!(FontHash::from_hash(u64::MAX), FontHash::invalid());
    }

    // =====================================================================
    // numeric/getter: LayoutFontMetrics
    // =====================================================================

    #[test]
    fn layout_font_metrics_baseline_scaled_typical_and_zero_font_size() {
        let fm = std_metrics();
        approx(fm.baseline_scaled(16.0), 12.8); // 800/1000 * 16
        assert_eq!(fm.baseline_scaled(0.0), 0.0);
        approx(fm.baseline_scaled(-16.0), -12.8);
    }

    #[test]
    fn layout_font_metrics_zero_upem_divides_by_zero_instead_of_guarding() {
        // NOTE: `LineHeight::resolve` explicitly guards `units_per_em == 0`, but the
        // *_scaled helpers do not — they divide by zero. Pin the actual behaviour so
        // a future guard shows up as a deliberate change rather than a silent one.
        let fm = metrics(0, 800.0, -200.0, 0.0);
        assert!(fm.baseline_scaled(16.0).is_infinite());
        assert!(fm.cap_height_scaled(16.0).is_infinite());

        // ascent == 0 turns 0/0 into NaN rather than inf.
        let zero = metrics(0, 0.0, 0.0, 0.0);
        assert!(zero.baseline_scaled(16.0).is_nan());
    }

    #[test]
    fn layout_font_metrics_x_height_falls_back_to_half_em() {
        let fm = std_metrics(); // x_height: None
        assert_eq!(fm.x_height_scaled(16.0), 8.0, "fallback is 0.5em");
        assert_eq!(fm.x_height_scaled(0.0), 0.0);

        let mut with_xh = std_metrics();
        with_xh.x_height = Some(500.0);
        approx(with_xh.x_height_scaled(16.0), 8.0);
        with_xh.x_height = Some(0.0);
        assert_eq!(
            with_xh.x_height_scaled(16.0),
            0.0,
            "an explicit sxHeight of 0 must NOT re-trigger the 0.5em fallback"
        );
    }

    #[test]
    fn layout_font_metrics_cap_height_falls_back_to_ascent() {
        let fm = std_metrics(); // cap_height: None
        assert_eq!(fm.cap_height_scaled(16.0), fm.baseline_scaled(16.0));

        let mut with_cap = std_metrics();
        with_cap.cap_height = Some(700.0);
        approx(with_cap.cap_height_scaled(16.0), 11.2);
    }

    #[test]
    fn layout_font_metrics_nan_font_size_propagates_without_panicking() {
        let fm = std_metrics();
        assert!(fm.baseline_scaled(f32::NAN).is_nan());
        assert!(fm.x_height_scaled(f32::NAN).is_nan());
        assert!(fm.cap_height_scaled(f32::NAN).is_nan());
        assert!(fm.baseline_scaled(f32::INFINITY).is_infinite());
    }

    #[test]
    fn layout_font_metrics_synthesized_baselines_span_exactly_one_em() {
        let fm = std_metrics();
        assert_eq!(fm.central_baseline(), 300.0); // midpoint(800, -200)
        assert_eq!(fm.em_over(), 800.0); // 300 + 1000/2
        assert_eq!(fm.em_under(), -200.0); // 300 - 1000/2
        assert_eq!(
            fm.em_over() - fm.em_under(),
            f32::from(fm.units_per_em),
            "em-over minus em-under is by definition 1em"
        );
    }

    #[test]
    fn layout_font_metrics_baselines_at_u16_max_upem_and_zero_upem() {
        let big = metrics(u16::MAX, 0.0, 0.0, 0.0);
        assert_eq!(big.central_baseline(), 0.0);
        assert_eq!(big.em_over(), f32::from(u16::MAX) / 2.0);
        assert_eq!(big.em_under(), -f32::from(u16::MAX) / 2.0);

        let zero = metrics(0, 100.0, -50.0, 0.0);
        assert_eq!(zero.em_over(), zero.central_baseline());
        assert_eq!(zero.em_under(), zero.central_baseline());
    }

    #[test]
    fn layout_font_metrics_central_baseline_with_infinite_extents_is_nan() {
        let fm = metrics(1000, f32::INFINITY, f32::NEG_INFINITY, 0.0);
        assert!(fm.central_baseline().is_nan(), "midpoint(inf, -inf) is NaN");
        assert!(fm.em_over().is_nan());
    }

    // =====================================================================
    // numeric: round_eq (the equality primitive under Rect/Size/Point/Stroke)
    // =====================================================================

    #[test]
    fn round_eq_rounds_half_away_from_zero() {
        assert!(round_eq(0.4, -0.4), "both round to 0");
        assert!(round_eq(1.5, 2.4), "1.5 rounds away from zero to 2");
        assert!(!round_eq(1.4, 1.5));
        assert!(round_eq(-1.5, -2.0));
    }

    #[test]
    fn round_eq_treats_nan_as_equal_to_everything_rounding_to_zero() {
        // `NaN.round() as isize` is a SATURATING cast that yields 0, so NaN
        // compares equal to 0.0 (and to itself). Any Rect/Size/Point carrying a
        // NaN coordinate therefore compares "equal" to a zeroed one — a real
        // cache-key hazard, pinned here.
        assert!(round_eq(f32::NAN, f32::NAN));
        assert!(round_eq(f32::NAN, 0.0));
        assert!(round_eq(f32::NAN, 0.49));
        assert!(!round_eq(f32::NAN, 1.0));

        assert_eq!(
            Rect {
                x: f32::NAN,
                y: 0.0,
                width: 0.0,
                height: 0.0
            },
            Rect::default(),
            "a NaN-x Rect compares equal to the zero Rect"
        );
    }

    #[test]
    fn round_eq_saturates_infinity_and_f32_max_to_the_same_isize() {
        // Both +inf and f32::MAX saturate to isize::MAX, so they are "equal".
        assert!(round_eq(f32::INFINITY, f32::MAX));
        assert!(round_eq(f32::NEG_INFINITY, f32::MIN));
        assert!(!round_eq(f32::INFINITY, f32::NEG_INFINITY));

        assert_eq!(
            Size::new(f32::INFINITY, 0.0),
            Size::new(f32::MAX, 0.0),
            "saturating cast collapses inf and f32::MAX into one bucket"
        );
    }

    // =====================================================================
    // numeric/getter: Size, calculate_bounding_box_size, ShapeDefinition
    // =====================================================================

    #[test]
    fn size_zero_is_the_neutral_element_and_new_preserves_bits() {
        assert_eq!(Size::zero(), Size::new(0.0, 0.0));
        assert_eq!(Size::zero().width, 0.0);
        assert_eq!(Size::zero(), Size::default());

        let weird = Size::new(f32::NAN, f32::INFINITY);
        assert!(weird.width.is_nan(), "the constructor must not sanitize");
        assert!(weird.height.is_infinite());
    }

    #[test]
    fn bounding_box_of_empty_and_single_point_is_zero() {
        assert_eq!(calculate_bounding_box_size(&[]), Size::zero());
        assert_eq!(
            calculate_bounding_box_size(&[Point { x: 5.0, y: -5.0 }]),
            Size::zero()
        );
    }

    #[test]
    fn bounding_box_spans_negative_coordinates() {
        let pts = [
            Point { x: -10.0, y: -20.0 },
            Point { x: 30.0, y: 5.0 },
            Point { x: 0.0, y: 0.0 },
        ];
        assert_eq!(calculate_bounding_box_size(&pts), Size::new(40.0, 25.0));
    }

    #[test]
    fn bounding_box_of_all_nan_points_collapses_to_zero() {
        // min()/max() discard NaN, leaving min > max, which the guard catches.
        let pts = [Point {
            x: f32::NAN,
            y: f32::NAN,
        }];
        assert_eq!(calculate_bounding_box_size(&pts), Size::zero());
    }

    #[test]
    fn bounding_box_of_extreme_points_overflows_to_infinity_without_panicking() {
        let pts = [
            Point {
                x: f32::MIN,
                y: f32::MIN,
            },
            Point {
                x: f32::MAX,
                y: f32::MAX,
            },
        ];
        let s = calculate_bounding_box_size(&pts);
        assert!(s.width.is_infinite() && s.height.is_infinite());
    }

    #[test]
    fn shape_definition_get_size_for_each_variant() {
        assert_eq!(
            ShapeDefinition::Rectangle {
                size: Size::new(3.0, 4.0),
                corner_radius: None
            }
            .get_size(),
            Size::new(3.0, 4.0)
        );
        assert_eq!(
            ShapeDefinition::Circle { radius: 10.0 }.get_size(),
            Size::new(20.0, 20.0)
        );
        assert_eq!(
            ShapeDefinition::Ellipse {
                radii: Size::new(5.0, 2.0)
            }
            .get_size(),
            Size::new(10.0, 4.0)
        );
        assert_eq!(
            ShapeDefinition::Polygon { points: Vec::new() }.get_size(),
            Size::zero()
        );
        assert_eq!(
            ShapeDefinition::Path {
                segments: Vec::new()
            }
            .get_size(),
            Size::zero()
        );
    }

    #[test]
    fn shape_definition_negative_circle_radius_yields_a_negative_size() {
        // Pinned, not endorsed: the constructor never validates the radius, so a
        // negative CSS radius propagates a negative bounding box into layout.
        let s = ShapeDefinition::Circle { radius: -10.0 }.get_size();
        assert_eq!(s.width, -20.0);
        assert_eq!(s.height, -20.0);
    }

    #[test]
    fn shape_definition_path_of_only_close_segments_is_zero_sized() {
        let s = ShapeDefinition::Path {
            segments: vec![PathSegment::Close, PathSegment::Close],
        }
        .get_size();
        assert_eq!(s, Size::zero(), "Close contributes no points");
    }

    #[test]
    fn shape_definition_path_bounding_box_includes_control_points() {
        let s = ShapeDefinition::Path {
            segments: vec![
                PathSegment::MoveTo(Point { x: 0.0, y: 0.0 }),
                PathSegment::QuadTo {
                    control: Point { x: 50.0, y: 100.0 },
                    end: Point { x: 100.0, y: 0.0 },
                },
            ],
        }
        .get_size();
        assert_eq!(
            s,
            Size::new(100.0, 100.0),
            "the control point (not the true curve extremum) sets the height"
        );
    }

    // =====================================================================
    // numeric: ShapeBoundary::inflate
    // =====================================================================

    #[test]
    fn shape_boundary_inflate_by_zero_is_identity() {
        let r = ShapeBoundary::Rectangle(Rect {
            x: 1.0,
            y: 2.0,
            width: 3.0,
            height: 4.0,
        });
        assert_eq!(r.inflate(0.0), r);
        let c = ShapeBoundary::Circle {
            center: Point { x: 0.0, y: 0.0 },
            radius: 5.0,
        };
        assert_eq!(c.inflate(0.0), c);
    }

    #[test]
    fn shape_boundary_inflate_rectangle_clamps_negative_dimensions_to_zero() {
        let r = ShapeBoundary::Rectangle(Rect {
            x: 0.0,
            y: 0.0,
            width: 10.0,
            height: 10.0,
        });
        match r.inflate(-100.0) {
            ShapeBoundary::Rectangle(out) => {
                assert_eq!(out.width, 0.0, "over-deflation must clamp, not go negative");
                assert_eq!(out.height, 0.0);
                assert_eq!(out.x, 100.0, "the origin is NOT clamped");
            }
            other => panic!("expected Rectangle, got {other:?}"),
        }
    }

    #[test]
    fn shape_boundary_inflate_nan_margin_zeroes_the_rectangle_extent() {
        // `margin == 0.0` is false for NaN, so we take the inflate path; then
        // `NaN.max(0.0)` returns 0.0 (f32::max discards NaN) — the box silently
        // collapses to zero width/height with a NaN origin.
        let r = ShapeBoundary::Rectangle(Rect {
            x: 0.0,
            y: 0.0,
            width: 10.0,
            height: 10.0,
        });
        match r.inflate(f32::NAN) {
            ShapeBoundary::Rectangle(out) => {
                assert_eq!(out.width, 0.0);
                assert_eq!(out.height, 0.0);
                assert!(out.x.is_nan());
            }
            other => panic!("expected Rectangle, got {other:?}"),
        }
    }

    #[test]
    fn shape_boundary_inflate_circle_radius_is_unclamped() {
        let c = ShapeBoundary::Circle {
            center: Point { x: 1.0, y: 2.0 },
            radius: 5.0,
        };
        match c.inflate(-50.0) {
            ShapeBoundary::Circle { center, radius } => {
                assert_eq!(center, Point { x: 1.0, y: 2.0 });
                assert_eq!(
                    radius, -45.0,
                    "circle radius is NOT clamped at 0 (unlike Rect)"
                );
            }
            other => panic!("expected Circle, got {other:?}"),
        }
        match c.inflate(f32::INFINITY) {
            ShapeBoundary::Circle { radius, .. } => assert!(radius.is_infinite()),
            other => panic!("expected Circle, got {other:?}"),
        }
    }

    #[test]
    fn shape_boundary_inflate_is_a_documented_no_op_for_polygon_and_path() {
        let p = ShapeBoundary::Polygon {
            points: vec![Point { x: 0.0, y: 0.0 }, Point { x: 1.0, y: 1.0 }],
        };
        assert_eq!(p.inflate(10.0), p, "polygon inflation is not implemented");
        let path = ShapeBoundary::Path {
            segments: vec![PathSegment::MoveTo(Point { x: 0.0, y: 0.0 })],
        };
        assert_eq!(
            path.inflate(10.0),
            path,
            "path inflation is not implemented"
        );
    }

    // =====================================================================
    // other: resolve_effective_alignment
    // =====================================================================

    #[test]
    fn resolve_effective_alignment_passes_through_for_non_last_lines() {
        for ta in [
            TextAlign::Left,
            TextAlign::Right,
            TextAlign::Center,
            TextAlign::Justify,
            TextAlign::Start,
            TextAlign::End,
            TextAlign::JustifyAll,
        ] {
            assert_eq!(
                resolve_effective_alignment(ta, TextAlign::Right, false),
                ta,
                "text-align-last must not touch a non-last line"
            );
        }
    }

    #[test]
    fn resolve_effective_alignment_last_line_justify_degrades_to_start() {
        assert_eq!(
            resolve_effective_alignment(TextAlign::Justify, TextAlign::default(), true),
            TextAlign::Start
        );
        assert_eq!(
            resolve_effective_alignment(TextAlign::Center, TextAlign::default(), true),
            TextAlign::Center,
            "non-justify alignments survive onto the last line"
        );
    }

    #[test]
    fn resolve_effective_alignment_explicit_text_align_last_left_is_indistinguishable_from_auto() {
        // QUIRK: "auto" is encoded as TextAlign::default() == Left, so an author
        // writing `text-align-last: left` on `text-align: center` gets CENTER, not
        // left — the explicit value is swallowed by the auto check.
        assert_eq!(
            resolve_effective_alignment(TextAlign::Center, TextAlign::Left, true),
            TextAlign::Center
        );
        // Any other explicit value does win.
        assert_eq!(
            resolve_effective_alignment(TextAlign::Center, TextAlign::Right, true),
            TextAlign::Right
        );
        assert_eq!(
            resolve_effective_alignment(TextAlign::Justify, TextAlign::Justify, true),
            TextAlign::Justify
        );
    }

    // =====================================================================
    // numeric: Spacing::resolve_px
    // =====================================================================

    #[test]
    fn spacing_resolve_px_default_is_zero_and_font_size_independent() {
        assert_eq!(Spacing::default(), Spacing::Px(0));
        assert_eq!(Spacing::default().resolve_px(16.0), 0.0);
        assert_eq!(Spacing::Px(0).resolve_px(f32::NAN), 0.0);
    }

    #[test]
    fn spacing_resolve_px_at_i32_extremes_stays_finite() {
        let hi = Spacing::Px(i32::MAX).resolve_px(16.0);
        let lo = Spacing::Px(i32::MIN).resolve_px(16.0);
        assert!(hi.is_finite() && hi > 2.0e9, "got {hi}");
        assert!(lo.is_finite() && lo < -2.0e9, "got {lo}");
        assert_eq!(lo, -2147483648.0);
    }

    #[test]
    fn spacing_resolve_px_em_scales_with_font_size() {
        assert_eq!(Spacing::Em(2.0).resolve_px(16.0), 32.0);
        assert_eq!(Spacing::Em(2.0).resolve_px(0.0), 0.0);
        assert_eq!(Spacing::Em(-0.5).resolve_px(16.0), -8.0);
        assert_eq!(
            Spacing::PxF(0.4).resolve_px(999.0),
            0.4,
            "PxF ignores font size"
        );
    }

    #[test]
    fn spacing_resolve_px_nan_and_overflow_are_defined() {
        assert!(Spacing::Em(f32::NAN).resolve_px(16.0).is_nan());
        assert!(Spacing::Em(1.0).resolve_px(f32::NAN).is_nan());
        assert!(Spacing::PxF(f32::NAN).resolve_px(16.0).is_nan());
        assert!(Spacing::Em(f32::MAX).resolve_px(2.0).is_infinite());
        // 0 * inf is NaN, not 0.
        assert!(Spacing::Em(0.0).resolve_px(f32::INFINITY).is_nan());
    }

    #[test]
    fn spacing_px_and_pxf_of_the_same_value_are_distinct_cache_keys() {
        assert_ne!(Spacing::Px(1), Spacing::PxF(1.0));
        assert_ne!(hash_of(&Spacing::Px(1)), hash_of(&Spacing::PxF(1.0)));
        assert_eq!(
            Spacing::Px(1).resolve_px(16.0),
            Spacing::PxF(1.0).resolve_px(16.0)
        );
    }

    // =====================================================================
    // predicate/getter: BidiDirection, BidiLevel, WritingMode
    // =====================================================================

    #[test]
    fn bidi_direction_is_rtl() {
        assert!(!BidiDirection::Ltr.is_rtl());
        assert!(BidiDirection::Rtl.is_rtl());
    }

    #[test]
    fn bidi_level_parity_defines_rtl_across_the_whole_u8_range() {
        for lvl in [0_u8, 1, 2, 3, 126, 127, 254, u8::MAX] {
            let b = BidiLevel::new(lvl);
            assert_eq!(b.level(), lvl, "level() must round-trip new()");
            assert_eq!(b.is_rtl(), lvl % 2 == 1, "odd embedding levels are RTL");
        }
    }

    #[test]
    fn writing_mode_is_advance_horizontal_for_every_variant() {
        assert!(WritingMode::HorizontalTb.is_advance_horizontal());
        assert!(WritingMode::SidewaysRl.is_advance_horizontal());
        assert!(WritingMode::SidewaysLr.is_advance_horizontal());
        assert!(!WritingMode::VerticalRl.is_advance_horizontal());
        assert!(!WritingMode::VerticalLr.is_advance_horizontal());
        assert_eq!(WritingMode::default(), WritingMode::HorizontalTb);
    }

    #[test]
    fn writing_mode_get_direction_only_horizontal_defers_to_content() {
        assert_eq!(WritingMode::HorizontalTb.get_direction(), None);
        assert_eq!(
            WritingMode::VerticalRl.get_direction(),
            Some(BidiDirection::Rtl)
        );
        assert_eq!(
            WritingMode::VerticalLr.get_direction(),
            Some(BidiDirection::Ltr)
        );
        assert_eq!(
            WritingMode::SidewaysRl.get_direction(),
            Some(BidiDirection::Rtl)
        );
        assert_eq!(
            WritingMode::SidewaysLr.get_direction(),
            Some(BidiDirection::Ltr)
        );
    }

    // =====================================================================
    // UnifiedConstraints
    // =====================================================================

    #[test]
    fn unified_constraints_default_is_horizontal_max_content() {
        let c = UnifiedConstraints::default();
        assert!(!c.is_vertical());
        assert_eq!(c.available_width, AvailableSpace::MaxContent);
        assert_eq!(c.columns, 1);
        assert_eq!(c, UnifiedConstraints::default());
        assert_eq!(
            hash_of(&c),
            hash_of(&UnifiedConstraints::default()),
            "Hash/Eq must agree for the default constraints"
        );
    }

    #[test]
    fn unified_constraints_is_vertical_only_for_the_two_vertical_modes() {
        let mut c = UnifiedConstraints::default();
        for (wm, want) in [
            (WritingMode::HorizontalTb, false),
            (WritingMode::VerticalRl, true),
            (WritingMode::VerticalLr, true),
            (WritingMode::SidewaysRl, false),
            (WritingMode::SidewaysLr, false),
        ] {
            c.writing_mode = Some(wm);
            assert_eq!(c.is_vertical(), want, "{wm:?}");
        }
        c.writing_mode = None;
        assert!(!c.is_vertical());
    }

    #[test]
    fn unified_constraints_direction_uses_fallback_unless_the_writing_mode_forces_one() {
        let mut c = UnifiedConstraints::default();
        // No writing mode → fallback wins.
        assert_eq!(c.direction(BidiDirection::Rtl), BidiDirection::Rtl);
        // horizontal-tb → still content-determined → fallback wins.
        c.writing_mode = Some(WritingMode::HorizontalTb);
        assert_eq!(c.direction(BidiDirection::Rtl), BidiDirection::Rtl);
        // vertical-rl OVERRIDES the fallback.
        c.writing_mode = Some(WritingMode::VerticalRl);
        assert_eq!(c.direction(BidiDirection::Ltr), BidiDirection::Rtl);
    }

    #[test]
    fn unified_constraints_resolved_line_height_uses_the_strut_for_normal() {
        let mut c = UnifiedConstraints::default();
        assert_eq!(
            c.resolved_line_height(),
            DEFAULT_STRUT_ASCENT + DEFAULT_STRUT_DESCENT
        );
        assert_eq!(c.resolved_line_height(), 16.0);

        c.line_height = LineHeight::Px(0.0);
        assert_eq!(c.resolved_line_height(), 0.0, "an explicit 0 is honoured");

        // Pinned: a negative / NaN px line-height is passed straight through.
        c.line_height = LineHeight::Px(-5.0);
        assert_eq!(c.resolved_line_height(), -5.0);
        c.line_height = LineHeight::Px(f32::NAN);
        assert!(c.resolved_line_height().is_nan());
    }

    #[test]
    fn unified_constraints_partial_eq_is_rounding_tolerant() {
        // PartialEq rounds f32 fields, so sub-pixel strut differences compare EQUAL.
        let mut a = UnifiedConstraints::default();
        let mut b = UnifiedConstraints::default();
        a.strut_ascent = 12.8;
        b.strut_ascent = 12.9;
        assert_eq!(a, b, "12.8 and 12.9 both round to 13");
        b.strut_ascent = 14.0;
        assert_ne!(a, b);
    }

    // =====================================================================
    // constructor: TextDecoration::from_css
    // =====================================================================

    #[test]
    fn text_decoration_from_css_maps_each_variant_exclusively() {
        use azul_css::props::style::text::StyleTextDecoration;
        let none = TextDecoration::from_css(StyleTextDecoration::None);
        assert_eq!(none, TextDecoration::default());
        assert!(!none.underline && !none.strikethrough && !none.overline);

        let u = TextDecoration::from_css(StyleTextDecoration::Underline);
        assert!(u.underline && !u.strikethrough && !u.overline);

        let o = TextDecoration::from_css(StyleTextDecoration::Overline);
        assert!(!o.underline && !o.strikethrough && o.overline);

        let lt = TextDecoration::from_css(StyleTextDecoration::LineThrough);
        assert!(!lt.underline && lt.strikethrough && !lt.overline);
    }

    // =====================================================================
    // predicate/getter: InlineBorderInfo
    // =====================================================================

    #[test]
    fn inline_border_info_default_has_no_border_and_no_chrome() {
        let b = InlineBorderInfo::default();
        assert!(!b.has_border());
        assert!(!b.has_chrome());
        assert_eq!(b.left_inset(), 0.0);
        assert_eq!(b.right_inset(), 0.0);
        assert_eq!(b.top_inset(), 0.0);
        assert_eq!(b.bottom_inset(), 0.0);
    }

    #[test]
    fn inline_border_info_negative_widths_do_not_count_as_a_border() {
        let b = InlineBorderInfo {
            top: -1.0,
            right: -1.0,
            bottom: -1.0,
            left: -1.0,
            ..InlineBorderInfo::default()
        };
        assert!(!b.has_border(), "the predicate is strictly `> 0.0`");
        assert!(!b.has_chrome());
        // ...but the inset arithmetic still returns the negative value.
        assert_eq!(b.left_inset(), -1.0);
    }

    #[test]
    fn inline_border_info_padding_alone_is_chrome_but_not_a_border() {
        let b = InlineBorderInfo {
            padding_left: 4.0,
            ..InlineBorderInfo::default()
        };
        assert!(!b.has_border());
        assert!(b.has_chrome());
        assert_eq!(b.left_inset(), 4.0);
    }

    #[test]
    fn inline_border_info_nan_border_width_is_not_a_border() {
        let b = InlineBorderInfo {
            top: f32::NAN,
            ..InlineBorderInfo::default()
        };
        assert!(!b.has_border(), "NaN > 0.0 is false");
        assert!(
            b.top_inset().is_nan(),
            "but the inset still carries the NaN"
        );
    }

    #[test]
    fn inline_border_info_split_insets_swap_edges_in_rtl() {
        let base = InlineBorderInfo {
            left: 2.0,
            right: 3.0,
            padding_left: 1.0,
            padding_right: 1.0,
            ..InlineBorderInfo::default()
        };

        // LTR: left edge on the FIRST fragment, right edge on the LAST.
        let ltr_first = InlineBorderInfo {
            is_first_fragment: true,
            is_last_fragment: false,
            ..base
        };
        assert_eq!(ltr_first.left_inset(), 3.0);
        assert_eq!(ltr_first.right_inset(), 0.0);

        let ltr_last = InlineBorderInfo {
            is_first_fragment: false,
            is_last_fragment: true,
            ..base
        };
        assert_eq!(ltr_last.left_inset(), 0.0);
        assert_eq!(ltr_last.right_inset(), 4.0);

        // RTL: mirrored.
        let rtl_first = InlineBorderInfo {
            is_first_fragment: true,
            is_last_fragment: false,
            is_rtl: true,
            ..base
        };
        assert_eq!(rtl_first.left_inset(), 0.0);
        assert_eq!(rtl_first.right_inset(), 4.0);

        let rtl_last = InlineBorderInfo {
            is_first_fragment: false,
            is_last_fragment: true,
            is_rtl: true,
            ..base
        };
        assert_eq!(rtl_last.left_inset(), 3.0);
        assert_eq!(rtl_last.right_inset(), 0.0);

        // A middle fragment (neither first nor last) draws NO horizontal edge.
        let middle = InlineBorderInfo {
            is_first_fragment: false,
            is_last_fragment: false,
            ..base
        };
        assert_eq!(middle.left_inset(), 0.0);
        assert_eq!(middle.right_inset(), 0.0);
        // Vertical insets are never suppressed.
        let tall = InlineBorderInfo {
            top: 1.0,
            bottom: 2.0,
            padding_top: 3.0,
            padding_bottom: 4.0,
            is_first_fragment: false,
            is_last_fragment: false,
            ..InlineBorderInfo::default()
        };
        assert_eq!(tall.top_inset(), 4.0);
        assert_eq!(tall.bottom_inset(), 6.0);
    }

    // =====================================================================
    // getter/other: StyleProperties::layout_hash / layout_eq / apply_override
    // =====================================================================

    #[test]
    fn style_layout_eq_ignores_render_only_properties() {
        let a = StyleProperties::default();
        let b = StyleProperties {
            color: ColorU {
                r: 255,
                g: 0,
                b: 0,
                a: 255,
            },
            background_color: Some(ColorU::TRANSPARENT),
            text_decoration: TextDecoration {
                underline: true,
                strikethrough: false,
                overline: false,
            },
            border: Some(InlineBorderInfo::default()),
            ..StyleProperties::default()
        };
        assert_ne!(a, b, "the full PartialEq DOES see the colour change");
        assert!(
            a.layout_eq(&b),
            "but layout_eq must ignore colour/decoration/border"
        );
        assert_eq!(a.layout_hash(), b.layout_hash());
    }

    #[test]
    fn style_layout_eq_sees_sub_pixel_font_size_changes() {
        let a = StyleProperties::default();
        let b = StyleProperties {
            font_size_px: 16.4,
            ..StyleProperties::default()
        };
        assert!(
            !a.layout_eq(&b),
            "16.0 vs 16.4 must NOT share a shaping-cache entry"
        );
    }

    #[test]
    fn style_layout_eq_sees_spacing_and_font_stack_changes() {
        let base = StyleProperties::default();

        let spaced = StyleProperties {
            letter_spacing: Spacing::PxF(0.5),
            ..StyleProperties::default()
        };
        assert!(!base.layout_eq(&spaced));

        let worded = StyleProperties {
            word_spacing: Spacing::Em(0.1),
            ..StyleProperties::default()
        };
        assert!(!base.layout_eq(&worded));

        let other_font = StyleProperties {
            font_stack: FontStack::Stack(vec![sel("Arial")]),
            ..StyleProperties::default()
        };
        assert!(!base.layout_eq(&other_font));

        let vertical = StyleProperties {
            writing_mode: WritingMode::VerticalRl,
            ..StyleProperties::default()
        };
        assert!(!base.layout_eq(&vertical));
    }

    #[test]
    fn style_layout_hash_is_stable_across_repeated_calls() {
        let s = StyleProperties::default();
        assert_eq!(s.layout_hash(), s.layout_hash());
        assert!(s.layout_eq(&StyleProperties::default()));
    }

    #[test]
    fn style_layout_eq_treats_two_nan_font_sizes_as_equal() {
        // layout_hash hashes the raw bits, so NaN == NaN here (unlike `==` on f32).
        let a = StyleProperties {
            font_size_px: f32::NAN,
            ..StyleProperties::default()
        };
        let b = StyleProperties {
            font_size_px: f32::NAN,
            ..StyleProperties::default()
        };
        assert!(a.layout_eq(&b));
        assert!(!a.layout_eq(&StyleProperties::default()));
    }

    #[test]
    fn style_apply_override_with_an_empty_partial_changes_nothing() {
        let base = StyleProperties::default();
        let out = base.apply_override(&PartialStyleProperties::default());
        assert_eq!(out, base);
    }

    #[test]
    fn style_apply_override_applies_only_the_some_fields() {
        let base = StyleProperties::default();
        let partial = PartialStyleProperties {
            font_size_px: Some(32.0),
            letter_spacing: Some(Spacing::PxF(1.5)),
            ..PartialStyleProperties::default()
        };
        let out = base.apply_override(&partial);
        assert_eq!(out.font_size_px, 32.0);
        assert_eq!(out.letter_spacing, Spacing::PxF(1.5));
        // Untouched fields are inherited verbatim.
        assert_eq!(out.word_spacing, base.word_spacing);
        assert_eq!(out.tab_size, base.tab_size);
        assert_eq!(out.font_stack, base.font_stack);
        assert!(!out.layout_eq(&base));
    }

    #[test]
    fn style_apply_override_can_inject_nan_font_size() {
        let base = StyleProperties::default();
        let partial = PartialStyleProperties {
            font_size_px: Some(f32::NAN),
            ..PartialStyleProperties::default()
        };
        let out = base.apply_override(&partial);
        assert!(out.font_size_px.is_nan(), "no validation happens here");
    }

    // =====================================================================
    // numeric: classify_character / get_justification_priority
    // =====================================================================

    #[test]
    fn classify_character_covers_each_class() {
        assert_eq!(classify_character(0x0020), CharacterClass::Space);
        assert_eq!(classify_character(0x00A0), CharacterClass::Space);
        assert_eq!(classify_character(0x3000), CharacterClass::Space);
        assert_eq!(classify_character('.' as u32), CharacterClass::Punctuation);
        assert_eq!(classify_character('~' as u32), CharacterClass::Punctuation);
        assert_eq!(classify_character('a' as u32), CharacterClass::Letter);
        assert_eq!(classify_character(0x4E00), CharacterClass::Ideograph);
        assert_eq!(classify_character(0x9FFF), CharacterClass::Ideograph);
        assert_eq!(classify_character(0x0301), CharacterClass::Combining);
    }

    #[test]
    fn classify_character_at_u32_extremes_defaults_to_letter() {
        assert_eq!(classify_character(0), CharacterClass::Letter);
        assert_eq!(classify_character(u32::MAX), CharacterClass::Letter);
        // Boundary walk around the ideograph range.
        assert_eq!(classify_character(0x4DFF), CharacterClass::Letter);
        assert_eq!(classify_character(0xA000), CharacterClass::Letter);
    }

    #[test]
    fn get_justification_priority_is_strictly_ordered_space_to_combining() {
        let p = |c| get_justification_priority(c);
        assert_eq!(p(CharacterClass::Space), 0);
        assert_eq!(p(CharacterClass::Combining), 255);
        assert!(p(CharacterClass::Space) < p(CharacterClass::Punctuation));
        assert!(p(CharacterClass::Punctuation) < p(CharacterClass::Ideograph));
        assert!(p(CharacterClass::Ideograph) < p(CharacterClass::Letter));
        assert!(p(CharacterClass::Letter) < p(CharacterClass::Symbol));
        assert!(p(CharacterClass::Symbol) < p(CharacterClass::Combining));
    }

    // =====================================================================
    // predicate: char-level classifiers
    // =====================================================================

    #[test]
    fn is_hanging_punctuation_char_only_stops_and_commas() {
        assert!(is_hanging_punctuation_char(','));
        assert!(is_hanging_punctuation_char('.'));
        assert!(is_hanging_punctuation_char('\u{3001}'));
        assert!(is_hanging_punctuation_char('\u{FF0E}'));
        assert!(!is_hanging_punctuation_char(';'));
        assert!(!is_hanging_punctuation_char(' '));
        assert!(!is_hanging_punctuation_char('\0'));
        assert!(!is_hanging_punctuation_char(char::MAX));
    }

    #[test]
    fn is_word_char_is_alphanumeric_or_underscore() {
        assert!(is_word_char('a'));
        assert!(is_word_char('Z'));
        assert!(is_word_char('9'));
        assert!(is_word_char('_'));
        assert!(is_word_char('é'), "non-ASCII letters are word chars");
        assert!(is_word_char('中'), "ideographs are alphanumeric");
        assert!(!is_word_char(' '));
        assert!(!is_word_char('-'));
        assert!(!is_word_char('.'));
        assert!(!is_word_char('\u{00A0}'));
        assert!(!is_word_char('\0'));
    }

    #[test]
    fn is_word_separator_char_excludes_tabs_and_fixed_width_spaces() {
        assert!(is_word_separator_char(' '));
        assert!(
            is_word_separator_char('\u{00A0}'),
            "NBSP IS a word separator"
        );
        assert!(is_word_separator_char('\u{1680}'));
        assert!(is_word_separator_char('\u{202F}'));
        assert!(is_word_separator_char('\u{10100}'));

        // Per CSS Text §7.1 these are NOT word separators, despite looking like spaces.
        assert!(!is_word_separator_char('\u{2000}'));
        assert!(!is_word_separator_char('\u{200A}'));
        assert!(
            !is_word_separator_char('\u{3000}'),
            "ideographic space excluded"
        );
        // Nor are tab/newline (they are handled by white-space processing instead).
        assert!(!is_word_separator_char('\t'));
        assert!(!is_word_separator_char('\n'));
        assert!(!is_word_separator_char('.'));
        assert!(!is_word_separator_char(char::MAX));
    }

    #[test]
    fn is_cursive_script_char_boundaries() {
        assert!(!is_cursive_script_char('\u{05FF}'), "one below Arabic");
        assert!(is_cursive_script_char('\u{0600}'), "Arabic block start");
        assert!(is_cursive_script_char('\u{06FF}'), "Arabic block end");
        assert!(is_cursive_script_char('\u{0700}'), "Syriac");
        assert!(is_cursive_script_char('\u{1800}'), "Mongolian");
        assert!(
            is_cursive_script_char('\u{10D00}'),
            "Hanifi Rohingya (astral)"
        );
        assert!(!is_cursive_script_char('a'));
        assert!(!is_cursive_script_char('中'));
        assert!(!is_cursive_script_char('\0'));
        assert!(!is_cursive_script_char(char::MAX));
    }

    #[test]
    fn is_cjk_character_boundaries() {
        assert!(is_cjk_character('中')); // U+4E2D
        assert!(is_cjk_character('\u{4E00}'));
        assert!(is_cjk_character('\u{9FFF}'));
        assert!(is_cjk_character('\u{3040}'), "hiragana block");
        assert!(is_cjk_character('\u{30FF}'), "katakana block");
        assert!(is_cjk_character('\u{AC00}'), "hangul syllables");
        assert!(is_cjk_character('\u{FF01}'), "fullwidth forms");
        assert!(
            !is_cjk_character('\u{4DFF}'),
            "one below the ideograph block"
        );
        assert!(!is_cjk_character('a'));
        assert!(!is_cjk_character('\0'));
        assert!(!is_cjk_character(char::MAX));
    }

    #[test]
    fn break_control_predicates_are_disjoint() {
        assert!(is_break_suppressing_control('\u{200D}'));
        assert!(is_break_suppressing_control('\u{2060}'));
        assert!(is_break_suppressing_control('\u{FEFF}'));
        assert!(!is_break_suppressing_control(' '));
        assert!(!is_break_suppressing_control('\u{200B}'));

        assert!(is_break_forcing_control('\u{200B}'));
        assert!(is_break_forcing_control('\u{2028}'));
        assert!(is_break_forcing_control('\u{2029}'));
        assert!(!is_break_forcing_control(' '));
        assert!(!is_break_forcing_control('\u{200D}'));

        for ch in ['\u{200D}', '\u{2060}', '\u{FEFF}', '\u{200B}', '\u{2028}'] {
            assert!(
                !(is_break_suppressing_control(ch) && is_break_forcing_control(ch)),
                "{ch:?} cannot both force and suppress a break"
            );
        }
    }

    #[test]
    fn is_small_kana_matches_only_the_cj_class() {
        assert!(is_small_kana('っ'));
        assert!(is_small_kana('ゃ'));
        assert!(is_small_kana('ッ'));
        assert!(is_small_kana('ー'), "prolonged sound mark is class CJ");
        assert!(!is_small_kana('つ'), "the FULL-size kana is not CJ");
        assert!(!is_small_kana('中'));
        assert!(!is_small_kana('a'));
        assert!(!is_small_kana('\0'));
    }

    #[test]
    fn is_cjk_break_allowed_by_strictness_per_level() {
        use LineBreakStrictness::{Anywhere, Auto, Loose, Normal, Strict};

        // Anywhere / Loose: everything is breakable.
        for ch in ['っ', '\u{301C}', '\u{2010}', '中'] {
            assert!(
                is_cjk_break_allowed_by_strictness(ch, None, Anywhere),
                "{ch:?}"
            );
            assert!(
                is_cjk_break_allowed_by_strictness(ch, None, Loose),
                "{ch:?}"
            );
        }

        // Normal/Auto: hyphens forbidden, small kana allowed.
        for level in [Normal, Auto] {
            assert!(!is_cjk_break_allowed_by_strictness('\u{2010}', None, level));
            assert!(!is_cjk_break_allowed_by_strictness('\u{2013}', None, level));
            assert!(is_cjk_break_allowed_by_strictness('っ', None, level));
            assert!(is_cjk_break_allowed_by_strictness('中', None, level));
        }

        // Strict: small kana and CJK hyphen-likes are forbidden too.
        assert!(!is_cjk_break_allowed_by_strictness('っ', None, Strict));
        assert!(!is_cjk_break_allowed_by_strictness('ー', None, Strict));
        assert!(!is_cjk_break_allowed_by_strictness(
            '\u{301C}', None, Strict
        ));
        assert!(!is_cjk_break_allowed_by_strictness(
            '\u{30A0}', None, Strict
        ));
        assert!(is_cjk_break_allowed_by_strictness('中', None, Strict));

        // prev_ch is currently ignored — pin that so a future use is a deliberate change.
        assert_eq!(
            is_cjk_break_allowed_by_strictness('中', Some('x'), Strict),
            is_cjk_break_allowed_by_strictness('中', None, Strict)
        );
    }

    // =====================================================================
    // getter/predicate: Glyph
    // =====================================================================

    fn plain_glyph(codepoint: char, advance: f32) -> Glyph {
        Glyph {
            glyph_id: 1,
            codepoint,
            font_hash: 7,
            font_metrics: std_metrics(),
            style: style(),
            source: GlyphSource::Char,
            logical_byte_index: 0,
            logical_byte_len: codepoint.len_utf8(),
            content_index: 0,
            cluster: 0,
            advance,
            kerning: 0.0,
            offset: Point { x: 0.0, y: 0.0 },
            vertical_advance: advance,
            vertical_origin_y: 0.0,
            vertical_bearing: Point { x: 0.0, y: 0.0 },
            orientation: GlyphOrientation::Horizontal,
            script: Script::Latin,
            bidi_level: BidiLevel::new(0),
        }
    }

    #[test]
    fn glyph_bounds_is_advance_by_resolved_line_height() {
        let g = plain_glyph('a', 9.5);
        let b = g.bounds();
        assert_eq!(b.x, 0.0);
        assert_eq!(b.y, 0.0);
        assert_eq!(b.width, 9.5);
        approx(b.height, 16.0); // normal line-height on a 1000/800/-200 font @16px
    }

    #[test]
    fn glyph_bounds_with_zero_advance_and_zero_upem_does_not_panic() {
        let mut g = plain_glyph('a', 0.0);
        g.font_metrics = metrics(0, 0.0, 0.0, 0.0);
        let b = g.bounds();
        assert_eq!(b.width, 0.0);
        approx(b.height, 19.2); // zero-upem falls back to 1.2em
    }

    #[test]
    fn glyph_whitespace_and_justification_predicates() {
        let space = plain_glyph(' ', 4.0);
        assert!(space.is_whitespace());
        assert_eq!(space.character_class(), CharacterClass::Space);
        assert!(!space.can_justify(), "whitespace is never itself justified");
        assert_eq!(space.justification_priority(), 0);
        assert!(space.break_opportunity_after());

        let letter = plain_glyph('a', 8.0);
        assert!(!letter.is_whitespace());
        assert!(letter.can_justify());
        assert_eq!(letter.justification_priority(), 192);
        assert!(!letter.break_opportunity_after());

        let combining = plain_glyph('\u{0301}', 0.0);
        assert!(!combining.is_whitespace());
        assert!(
            !combining.can_justify(),
            "combining marks are never justified"
        );
        assert_eq!(combining.justification_priority(), 255);
    }

    #[test]
    fn glyph_break_opportunity_after_covers_every_hyphen_form() {
        assert!(
            plain_glyph('\u{00AD}', 0.0).break_opportunity_after(),
            "soft hyphen"
        );
        assert!(
            plain_glyph('\u{002D}', 4.0).break_opportunity_after(),
            "hyphen-minus"
        );
        assert!(
            plain_glyph('\u{2010}', 4.0).break_opportunity_after(),
            "U+2010"
        );
        assert!(
            plain_glyph('\t', 8.0).break_opportunity_after(),
            "tab is whitespace"
        );
        assert!(
            !plain_glyph('\u{2011}', 4.0).break_opportunity_after(),
            "NON-BREAKING hyphen"
        );
        assert!(!plain_glyph('/', 4.0).break_opportunity_after());
    }

    // =====================================================================
    // getter/predicate: ShapedItem + item helpers
    // =====================================================================

    #[test]
    fn shaped_item_as_cluster_only_matches_clusters() {
        assert!(cl("a", 8.0).as_cluster().is_some());
        assert!(obj(10.0, 10.0, 0.0).as_cluster().is_none());
        assert!(brk().as_cluster().is_none());
        assert!(tab(8.0, 16.0).as_cluster().is_none());
    }

    #[test]
    fn shaped_item_bounds_of_a_break_is_the_zero_rect() {
        assert_eq!(brk().bounds(), Rect::default());
        assert_eq!(obj(10.0, 20.0, 0.0).bounds().width, 10.0);
        assert_eq!(tab(8.0, 16.0).bounds().height, 16.0);
        let c = cl("a", 9.5);
        assert_eq!(c.bounds().width, 9.5, "a cluster's width is its advance");
        approx(c.bounds().height, 16.0); // ascent + descent of the fixture font
    }

    #[test]
    fn get_item_measure_sums_advance_and_kerning() {
        let st = style();
        let mut g1 = shaped_glyph(st.clone(), std_metrics(), 8.0);
        g1.kerning = -1.5;
        let mut g2 = shaped_glyph(st.clone(), std_metrics(), 8.0);
        g2.kerning = 0.5;
        let item = make_cluster("ab", 16.0, st, smallvec![g1, g2], gid(0, 0));
        assert_eq!(get_item_measure(&item, false), 15.0, "16 + (-1.5) + 0.5");
        assert_eq!(
            get_item_measure(&item, true),
            15.0,
            "clusters ignore the is_vertical flag (advance is already axis-relative)"
        );
    }

    #[test]
    fn get_item_measure_of_a_break_is_zero_and_objects_switch_axis() {
        assert_eq!(get_item_measure(&brk(), false), 0.0);
        assert_eq!(get_item_measure(&brk(), true), 0.0);
        let o = obj(30.0, 20.0, 0.0);
        assert_eq!(get_item_measure(&o, false), 30.0);
        assert_eq!(get_item_measure(&o, true), 20.0);
    }

    #[test]
    fn get_item_measure_with_spacing_adds_letter_spacing_but_not_for_cursive() {
        let st = styled(|s| s.letter_spacing = Spacing::PxF(2.0));
        let latin = cl_styled("a", 10.0, st.clone());
        assert_eq!(get_item_measure(&latin, false), 10.0);
        assert_eq!(get_item_measure_with_spacing(&latin, false), 12.0);

        // Cursive (Arabic) clusters must never receive letter-spacing.
        let arabic = cl_styled("\u{0627}", 10.0, st);
        assert_eq!(
            get_item_measure_with_spacing(&arabic, false),
            10.0,
            "letter-spacing is suppressed for cursive scripts (CSS Text 3 App. D)"
        );
    }

    #[test]
    fn get_item_measure_with_spacing_adds_word_spacing_only_on_separators() {
        let st = styled(|s| {
            s.word_spacing = Spacing::PxF(5.0);
            s.letter_spacing = Spacing::PxF(1.0);
        });
        let space = cl_styled(" ", 4.0, st.clone());
        assert_eq!(
            get_item_measure_with_spacing(&space, false),
            10.0,
            "4 + letter(1) + word(5)"
        );
        let letter = cl_styled("a", 8.0, st);
        assert_eq!(
            get_item_measure_with_spacing(&letter, false),
            9.0,
            "no word-spacing on a non-separator"
        );
        // Non-cluster items get no spacing at all.
        assert_eq!(get_item_measure_with_spacing(&brk(), false), 0.0);
    }

    #[test]
    fn is_collapsible_whitespace_is_vacuously_true_for_an_empty_cluster() {
        assert!(is_collapsible_whitespace(&cl(" ", 4.0)));
        assert!(is_collapsible_whitespace(&cl("\t", 8.0)));
        assert!(is_collapsible_whitespace(&cl("\u{1680}", 4.0)));
        assert!(is_collapsible_whitespace(&cl("  \t ", 16.0)));
        assert!(
            !is_collapsible_whitespace(&cl("\n", 0.0)),
            "newline is NOT collapsible here"
        );
        assert!(!is_collapsible_whitespace(&cl("a", 8.0)));
        assert!(
            !is_collapsible_whitespace(&cl("a ", 12.0)),
            "all() — mixed is false"
        );
        assert!(!is_collapsible_whitespace(&obj(1.0, 1.0, 0.0)));
        // QUIRK: `chars().all(..)` on an empty string is vacuously TRUE, so a
        // zero-text cluster is treated as strippable whitespace at line edges.
        assert!(
            is_collapsible_whitespace(&cl("", 0.0)),
            "an empty cluster counts as collapsible whitespace"
        );
    }

    #[test]
    fn is_word_separator_and_zero_width_space_on_items() {
        assert!(is_word_separator(&cl(" ", 4.0)));
        assert!(
            is_word_separator(&cl("a b", 20.0)),
            "any() — one space suffices"
        );
        assert!(!is_word_separator(&cl("", 0.0)), "any() on empty is false");
        assert!(!is_word_separator(&cl("\u{3000}", 16.0)));
        assert!(!is_word_separator(&brk()));
        assert!(!is_word_separator(&obj(1.0, 1.0, 0.0)));

        assert!(is_zero_width_space(&cl("\u{200B}", 0.0)));
        assert!(
            is_zero_width_space(&cl("a\u{200B}", 8.0)),
            "contains(), not equals()"
        );
        assert!(!is_zero_width_space(&cl(" ", 4.0)));
        assert!(!is_zero_width_space(&obj(1.0, 1.0, 0.0)));
    }

    #[test]
    fn can_justify_after_rejects_objects_empty_clusters_and_combining_marks() {
        assert!(can_justify_after(&cl("a", 8.0)));
        assert!(!can_justify_after(&cl(" ", 4.0)));
        assert!(
            !can_justify_after(&cl("a\u{0301}", 8.0)),
            "trailing combining mark"
        );
        assert!(!can_justify_after(&cl("", 0.0)), "no last char → false");
        assert!(
            !can_justify_after(&obj(10.0, 10.0, 0.0)),
            "CSS 2.2 §9.4.2: never stretch after an atomic inline"
        );
        assert!(!can_justify_after(&brk()));
    }

    #[test]
    fn is_hanging_punctuation_requires_a_single_glyph_cluster() {
        assert!(is_hanging_punctuation(&cl(".", 4.0)));
        assert!(is_hanging_punctuation(&cl(",", 4.0)));
        assert!(!is_hanging_punctuation(&cl("a", 8.0)));
        assert!(!is_hanging_punctuation(&cl("", 0.0)), "no first char");
        assert!(!is_hanging_punctuation(&obj(1.0, 1.0, 0.0)));

        // A two-glyph cluster is rejected even if it starts with a full stop.
        let st = style();
        let g1 = shaped_glyph(st.clone(), std_metrics(), 4.0);
        let g2 = shaped_glyph(st.clone(), std_metrics(), 4.0);
        let two = make_cluster(".", 8.0, st, smallvec![g1, g2], gid(0, 0));
        assert!(!is_hanging_punctuation(&two));
    }

    #[test]
    fn cluster_script_predicates() {
        let arabic = cl("\u{0627}", 10.0);
        let latin = cl("a", 8.0);
        let cjk = cl("中", 16.0);

        assert!(is_cursive_script_cluster(arabic.as_cluster().unwrap()));
        assert!(!is_cursive_script_cluster(latin.as_cluster().unwrap()));
        assert!(
            !is_cursive_script_cluster(cl("", 0.0).as_cluster().unwrap()),
            "empty cluster has no first char"
        );

        assert!(is_cjk_cluster(cjk.as_cluster().unwrap()));
        assert!(!is_cjk_cluster(latin.as_cluster().unwrap()));

        // is_arabic_cluster keys off the GLYPH script, not the text.
        assert!(
            !is_arabic_cluster(arabic.as_cluster().unwrap()),
            "the fixture's glyph carries Script::Latin, so the text alone is not enough"
        );
        let st = style();
        let mut g = shaped_glyph(st.clone(), std_metrics(), 10.0);
        g.script = Script::Arabic;
        let real_arabic = make_cluster("\u{0627}", 10.0, st, smallvec![g], gid(0, 0));
        assert!(is_arabic_cluster(real_arabic.as_cluster().unwrap()));
    }

    #[test]
    fn cluster_is_word_boundary_for_punctuation_and_whitespace() {
        assert!(cluster_is_word_boundary(cl(" ", 4.0).as_cluster().unwrap()));
        assert!(cluster_is_word_boundary(cl(".", 4.0).as_cluster().unwrap()));
        assert!(
            cluster_is_word_boundary(cl("", 0.0).as_cluster().unwrap()),
            "vacuous"
        );
        assert!(!cluster_is_word_boundary(
            cl("a", 8.0).as_cluster().unwrap()
        ));
        assert!(!cluster_is_word_boundary(
            cl("_", 8.0).as_cluster().unwrap()
        ));
    }

    #[test]
    fn baseline_in_layout_only_defined_for_clusters_and_boxes() {
        assert_eq!(baseline_in_layout(&pos(brk(), 0.0, 0.0, 0)), None);
        assert_eq!(baseline_in_layout(&pos(tab(8.0, 16.0), 0.0, 0.0, 0)), None);
        // An atomic inline: its top plus its height above `baseline_offset`
        // (which counts from its bottom).
        assert_eq!(
            baseline_in_layout(&pos(obj(10.0, 20.0, 3.0), 0.0, 4.0, 0)),
            Some(4.0 + 17.0)
        );
        // Cluster: its top plus the ascent the line used (13px at 16px, see
        // `get_item_vertical_metrics_approx_for_every_variant`).
        approx(
            baseline_in_layout(&pos(cl("a", 8.0), 0.0, 30.0, 0))
                .expect("a glyph-bearing cluster has a baseline"),
            43.0,
        );
        assert_eq!(
            baseline_in_layout(&pos(cl_no_glyphs("", 0.0), 0.0, 0.0, 0)),
            None,
            "a glyph-less cluster has no baseline"
        );
    }

    #[test]
    fn get_item_vertical_metrics_approx_for_every_variant() {
        // Cluster with real glyphs: 800 / -200 at 1000 upem and 16px is 12.8 /
        // 3.2, rounded to whole pixels as in Chrome (`line_metrics_px`): ascent
        // 13, descent 3, no leading (`normal` == A + D + 0).
        let (a, d) = get_item_vertical_metrics_approx(&cl("a", 8.0));
        approx(a, 13.0);
        approx(d, 3.0);

        // Glyph-less cluster → 80/20 split of the fallback 1.2em line box.
        let (a, d) = get_item_vertical_metrics_approx(&cl_no_glyphs("", 0.0));
        approx(a, 19.2 * FALLBACK_ASCENT_RATIO);
        approx(d, 19.2 * FALLBACK_DESCENT_RATIO);

        // Object → all ascent, no descent.
        assert_eq!(
            get_item_vertical_metrics_approx(&obj(10.0, 20.0, 5.0)),
            (20.0, 0.0)
        );
        // Break → nothing.
        assert_eq!(get_item_vertical_metrics_approx(&brk()), (0.0, 0.0));
        // Tab → 80/20 of its box height.
        let (a, d) = get_item_vertical_metrics_approx(&tab(8.0, 10.0));
        approx(a, 8.0);
        approx(d, 2.0);
    }

    #[test]
    fn get_item_vertical_metrics_approx_skips_zero_upem_glyphs() {
        let st = style();
        let g = shaped_glyph(st.clone(), metrics(0, 800.0, -200.0, 0.0), 8.0);
        let item = make_cluster("a", 8.0, st, smallvec![g], gid(0, 0));
        assert_eq!(
            get_item_vertical_metrics_approx(&item),
            (0.0, 0.0),
            "a zero-upem glyph is skipped rather than producing inf/NaN metrics"
        );
    }

    #[test]
    fn get_item_vertical_metrics_uses_the_strut_for_glyphless_clusters() {
        let c = UnifiedConstraints::default();
        let (a, d) = get_item_vertical_metrics(&cl_no_glyphs("", 0.0), &c);
        // `normal` is the strut's own line height (A + D,
        // `UnifiedConstraints::resolved_line_height`), not a 1.2em guess: no
        // leading, the glyphless cluster IS the strut.
        approx(a, DEFAULT_STRUT_ASCENT);
        approx(d, DEFAULT_STRUT_DESCENT);

        assert_eq!(get_item_vertical_metrics(&brk(), &c), (0.0, 0.0));
        // Objects clamp negative ascent/descent at 0.
        let (a, d) = get_item_vertical_metrics(&obj(10.0, 10.0, 30.0), &c);
        assert_eq!(
            a, 0.0,
            "baseline_offset > height must clamp the ascent at 0"
        );
        assert_eq!(d, 30.0);
    }

    #[test]
    fn get_item_vertical_align_only_for_objects_with_image_or_shape_content() {
        assert_eq!(get_item_vertical_align(&cl("a", 8.0)), None);
        assert_eq!(get_item_vertical_align(&brk()), None);
        // Our fixture Object carries a Space, which has no alignment.
        assert_eq!(get_item_vertical_align(&obj(1.0, 1.0, 0.0)), None);

        let img = ShapedItem::Object {
            source: ci(0, 0),
            bounds: Rect::default(),
            baseline_offset: 0.0,
            content: InlineContent::Image(InlineImage {
                source: ImageSource::Placeholder(Size::new(10.0, 10.0)),
                intrinsic_size: Size::new(10.0, 10.0),
                display_size: None,
                baseline_offset: 0.0,
                alignment: VerticalAlign::Top,
                object_fit: ObjectFit::Fill,
            }),
        };
        assert_eq!(get_item_vertical_align(&img), Some(VerticalAlign::Top));
    }

    #[test]
    fn a_superscript_rises_a_third_of_its_parents_font_size_and_grows_the_line() {
        // Chrome (LayoutNG): `vertical-align: super` raises a box's baseline
        // by its parent's font size / 3 + 1px, `sub` lowers it by / 5 + 1px,
        // and the line box holds the shifted box (CSS 2.1 10.8.1). The
        // parent is the default 16px here. Both were left to the placement
        // (0.4 / 0.3 of the LINE's ascent) and the line box never counted
        // the shift: a `<sup>` reached above its line.
        let c = UnifiedConstraints::default();
        let up = 16.0 / 3.0 + 1.0;
        let down = 16.0 / 5.0 + 1.0;
        let sup = baseline_shift(VerticalAlign::Super, 10.0, 0.0, &c)
            .expect("super is a shift of the baseline");
        assert!((sup + up).abs() < 0.01, "super raises by 16 / 3 + 1: {sup}");
        let sub = baseline_shift(VerticalAlign::Sub, 10.0, 0.0, &c)
            .expect("sub is a shift of the baseline");
        assert!((sub - down).abs() < 0.01, "sub lowers by 16 / 5 + 1: {sub}");

        // A 10px box (all above its baseline) in a line whose strut is
        // 12 above / 4 below.
        let boxed = |alignment| ShapedItem::Object {
            source: ci(0, 0),
            bounds: Rect {
                x: 0.0,
                y: 0.0,
                width: 10.0,
                height: 10.0,
            },
            baseline_offset: 0.0,
            content: InlineContent::Image(InlineImage {
                source: ImageSource::Placeholder(Size::new(10.0, 10.0)),
                intrinsic_size: Size::new(10.0, 10.0),
                display_size: None,
                baseline_offset: 0.0,
                alignment,
                object_fit: ObjectFit::Fill,
            }),
        };
        let strut = (12.0, 4.0);
        let (asc, desc) = calculate_line_metrics(
            &[boxed(VerticalAlign::Super)],
            VerticalAlign::Baseline,
            &c,
            strut,
        );
        assert!(
            (asc - (10.0 + up)).abs() < 0.01 && (desc - 4.0).abs() < 0.01,
            "the raised box grows the line above the strut: {asc} / {desc}"
        );
        let (asc, desc) = calculate_line_metrics(
            &[boxed(VerticalAlign::Sub)],
            VerticalAlign::Baseline,
            &c,
            strut,
        );
        assert!(
            (asc - 12.0).abs() < 0.01 && (desc - down).abs() < 0.01,
            "the lowered box grows the line below the strut: {asc} / {desc}"
        );
    }

    // =====================================================================
    // predicate: break-opportunity logic
    // =====================================================================

    #[test]
    fn no_break_space_is_a_word_separator_but_never_a_break_opportunity() {
        let nbsp = cl("\u{00A0}", 4.0);
        assert!(
            is_word_separator(&nbsp),
            "NBSP participates in word-spacing"
        );
        assert!(
            !is_break_opportunity(&nbsp),
            "...but must NOT offer a soft wrap (10\\u{{00A0}}km must not wrap)"
        );
        assert!(!is_break_opportunity_with_word_break(
            &nbsp,
            WordBreak::BreakAll,
            Hyphens::Auto
        ));
        // Same for NNBSP / word joiner / ZWNBSP.
        for ch in ['\u{202F}', '\u{2060}', '\u{FEFF}'] {
            let item = cl(&ch.to_string(), 4.0);
            assert!(
                !is_break_opportunity_with_word_break(&item, WordBreak::BreakAll, Hyphens::Auto),
                "{ch:?} must suppress breaks"
            );
        }
    }

    #[test]
    fn zero_width_space_breaks_even_under_keep_all() {
        let zwsp = cl("\u{200B}", 0.0);
        assert!(is_break_opportunity(&zwsp));
        for wb in [WordBreak::Normal, WordBreak::BreakAll, WordBreak::KeepAll] {
            assert!(
                is_break_opportunity_with_word_break(&zwsp, wb, Hyphens::None),
                "ZWSP must always break ({wb:?})"
            );
        }
    }

    #[test]
    fn word_break_modes_change_cjk_break_opportunities() {
        let cjk = cl("中", 16.0);
        assert!(is_break_opportunity_with_word_break(
            &cjk,
            WordBreak::Normal,
            Hyphens::Manual
        ));
        assert!(is_break_opportunity_with_word_break(
            &cjk,
            WordBreak::BreakAll,
            Hyphens::Manual
        ));
        assert!(
            !is_break_opportunity_with_word_break(&cjk, WordBreak::KeepAll, Hyphens::Manual),
            "keep-all suppresses inter-ideograph breaks"
        );

        let latin = cl("a", 8.0);
        assert!(!is_break_opportunity_with_word_break(
            &latin,
            WordBreak::Normal,
            Hyphens::Manual
        ));
        assert!(
            is_break_opportunity_with_word_break(&latin, WordBreak::BreakAll, Hyphens::Manual),
            "break-all makes every cluster breakable"
        );
        assert!(!is_break_opportunity_with_word_break(
            &latin,
            WordBreak::KeepAll,
            Hyphens::Manual
        ));
    }

    #[test]
    fn soft_hyphen_break_depends_on_the_hyphens_property() {
        let shy = cl("\u{00AD}", 0.0);
        assert!(!is_break_opportunity_with_word_break(
            &shy,
            WordBreak::Normal,
            Hyphens::None
        ));
        assert!(is_break_opportunity_with_word_break(
            &shy,
            WordBreak::Normal,
            Hyphens::Manual
        ));
        assert!(is_break_opportunity_with_word_break(
            &shy,
            WordBreak::Normal,
            Hyphens::Auto
        ));
    }

    #[test]
    fn trailing_hyphen_and_slash_always_break_regardless_of_hyphens() {
        for text in ["co-", "co\u{2010}", "a/"] {
            let item = cl(text, 10.0);
            assert!(
                is_break_opportunity_with_word_break(&item, WordBreak::KeepAll, Hyphens::None),
                "{text:?} must offer a break after it even with hyphens:none"
            );
        }
        // A LEADING hyphen is not a break opportunity after the cluster.
        assert!(!is_break_opportunity_with_word_break(
            &cl("-a", 10.0),
            WordBreak::Normal,
            Hyphens::None
        ));
    }

    #[test]
    fn atomic_inlines_are_break_opportunities_but_breaks_and_tabs_differ() {
        assert!(
            is_break_opportunity(&obj(10.0, 10.0, 0.0)),
            "CSS Text 3 §5.1"
        );
        assert!(is_break_opportunity(&brk()));
        assert!(
            !is_break_opportunity(&tab(8.0, 16.0)),
            "a Tab is not itself a wrap point"
        );
        assert!(is_break_opportunity(&cl(" ", 4.0)));
        assert!(!is_break_opportunity(&cl("a", 8.0)));
    }

    // =====================================================================
    // numeric: geometry scanline helpers
    // =====================================================================

    #[test]
    fn merge_segments_of_zero_or_one_segment_is_identity() {
        assert!(merge_segments(Vec::new()).is_empty());
        let one = vec![LineSegment {
            start_x: 5.0,
            width: 3.0,
            priority: 0,
        }];
        let out = merge_segments(one);
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].start_x, 5.0);
    }

    #[test]
    fn merge_segments_joins_overlapping_and_touching_spans() {
        let segs = vec![
            LineSegment {
                start_x: 0.0,
                width: 10.0,
                priority: 0,
            },
            LineSegment {
                start_x: 5.0,
                width: 10.0,
                priority: 0,
            }, // overlaps
            LineSegment {
                start_x: 15.0,
                width: 5.0,
                priority: 0,
            }, // exactly adjacent
            LineSegment {
                start_x: 100.0,
                width: 5.0,
                priority: 0,
            }, // disjoint
        ];
        let out = merge_segments(segs);
        assert_eq!(out.len(), 2, "three touching spans collapse into one");
        assert_eq!(out[0].start_x, 0.0);
        assert_eq!(out[0].width, 20.0);
        assert_eq!(out[1].start_x, 100.0);
    }

    #[test]
    fn merge_segments_sorts_unordered_input() {
        let segs = vec![
            LineSegment {
                start_x: 50.0,
                width: 5.0,
                priority: 0,
            },
            LineSegment {
                start_x: 0.0,
                width: 5.0,
                priority: 0,
            },
        ];
        let out = merge_segments(segs);
        assert_eq!(out.len(), 2);
        assert_eq!(out[0].start_x, 0.0);
        assert_eq!(out[1].start_x, 50.0);
    }

    #[test]
    fn merge_segments_is_nan_tolerant() {
        // A NaN start_x used to abort layout via `partial_cmp(...).unwrap()`; the
        // sort now falls back to Equal (like every other float compare in this
        // module), so the merge completes without panicking.
        let segs = vec![
            LineSegment {
                start_x: 0.0,
                width: 10.0,
                priority: 0,
            },
            LineSegment {
                start_x: f32::NAN,
                width: 10.0,
                priority: 0,
            },
        ];
        let out = merge_segments(segs);
        assert!(!out.is_empty(), "NaN input must not abort the merge");
    }

    #[test]
    fn polygon_line_intersection_needs_at_least_three_points() {
        assert!(polygon_line_intersection(&[], 0.0, 1.0).is_empty());
        assert!(polygon_line_intersection(&[Point { x: 0.0, y: 0.0 }], 0.0, 1.0).is_empty());
        assert!(polygon_line_intersection(
            &[Point { x: 0.0, y: 0.0 }, Point { x: 1.0, y: 1.0 }],
            0.0,
            1.0
        )
        .is_empty());
    }

    #[test]
    fn polygon_line_intersection_narrows_across_a_triangle() {
        // Right triangle (0,0) - (100,0) - (0,100): span width ≈ 100 - y.
        let tri = [
            Point { x: 0.0, y: 0.0 },
            Point { x: 100.0, y: 0.0 },
            Point { x: 0.0, y: 100.0 },
        ];
        let top = polygon_line_intersection(&tri, 10.0, 1.0);
        let bot = polygon_line_intersection(&tri, 80.0, 1.0);
        assert_eq!(top.len(), 1);
        assert_eq!(bot.len(), 1);
        assert!(
            top[0].width > bot[0].width,
            "the band must narrow with y ({} !> {})",
            top[0].width,
            bot[0].width
        );
        assert!((top[0].width - 89.5).abs() < 1.0);
    }

    #[test]
    fn polygon_line_intersection_outside_the_shape_and_on_nan_scanlines_is_empty() {
        let tri = [
            Point { x: 0.0, y: 0.0 },
            Point { x: 100.0, y: 0.0 },
            Point { x: 0.0, y: 100.0 },
        ];
        assert!(
            polygon_line_intersection(&tri, 500.0, 1.0).is_empty(),
            "a scanline below the shape yields no spans"
        );
        assert!(
            polygon_line_intersection(&tri, f32::NAN, 1.0).is_empty(),
            "a NaN scanline must not panic — every crossing test is false"
        );
        assert!(polygon_line_intersection(&tri, f32::INFINITY, 1.0).is_empty());
    }

    #[test]
    fn polygon_line_intersection_of_a_degenerate_flat_polygon_is_empty() {
        // All edges horizontal → every edge is skipped.
        let flat = [
            Point { x: 0.0, y: 5.0 },
            Point { x: 10.0, y: 5.0 },
            Point { x: 20.0, y: 5.0 },
        ];
        assert!(polygon_line_intersection(&flat, 4.5, 1.0).is_empty());
    }

    #[test]
    fn path_segments_line_intersection_on_empty_and_degenerate_input() {
        assert!(path_segments_line_intersection(&[], 0.0, 1.0).is_empty());
        // A lone MoveTo cannot form a subpath.
        assert!(path_segments_line_intersection(
            &[PathSegment::MoveTo(Point { x: 0.0, y: 0.0 })],
            0.0,
            1.0
        )
        .is_empty());
    }

    #[test]
    fn path_segments_line_intersection_of_a_square() {
        let sq = vec![
            PathSegment::MoveTo(Point { x: 0.0, y: 0.0 }),
            PathSegment::LineTo(Point { x: 100.0, y: 0.0 }),
            PathSegment::LineTo(Point { x: 100.0, y: 100.0 }),
            PathSegment::LineTo(Point { x: 0.0, y: 100.0 }),
            PathSegment::Close,
        ];
        let spans = path_segments_line_intersection(&sq, 50.0, 1.0);
        assert_eq!(spans.len(), 1);
        assert!((spans[0].0 - 0.0).abs() < 0.01);
        assert!((spans[0].1 - 100.0).abs() < 0.01);
        // Outside the square vertically → nothing.
        assert!(path_segments_line_intersection(&sq, 500.0, 1.0).is_empty());
    }

    #[test]
    fn get_shape_horizontal_spans_rectangle_only_when_the_line_box_overlaps() {
        let r = ShapeBoundary::Rectangle(Rect {
            x: 10.0,
            y: 20.0,
            width: 30.0,
            height: 40.0,
        });
        assert_eq!(
            get_shape_horizontal_spans(&r, 30.0, 10.0),
            vec![(10.0, 40.0)]
        );
        assert!(
            get_shape_horizontal_spans(&r, 0.0, 10.0).is_empty(),
            "above"
        );
        assert!(
            get_shape_horizontal_spans(&r, 100.0, 10.0).is_empty(),
            "below"
        );
        // Exactly touching the top edge: line [10,20) vs rect [20,60) → no overlap.
        assert!(get_shape_horizontal_spans(&r, 10.0, 10.0).is_empty());
        // A zero-height rect can never overlap.
        let flat = ShapeBoundary::Rectangle(Rect {
            x: 0.0,
            y: 0.0,
            width: 10.0,
            height: 0.0,
        });
        assert!(get_shape_horizontal_spans(&flat, 0.0, 10.0).is_empty());
    }

    #[test]
    fn get_shape_horizontal_spans_circle_edges_and_zero_radius() {
        let c = ShapeBoundary::Circle {
            center: Point { x: 50.0, y: 50.0 },
            radius: 10.0,
        };
        let mid = get_shape_horizontal_spans(&c, 49.5, 1.0); // line centre == 50.0
        assert_eq!(mid.len(), 1);
        assert!((mid[0].0 - 40.0).abs() < 0.01);
        assert!((mid[0].1 - 60.0).abs() < 0.01);
        assert!(get_shape_horizontal_spans(&c, 1000.0, 1.0).is_empty());

        // Zero radius: the scanline exactly through the centre yields a zero-width span.
        let dot = ShapeBoundary::Circle {
            center: Point { x: 5.0, y: 5.0 },
            radius: 0.0,
        };
        let spans = get_shape_horizontal_spans(&dot, 4.5, 1.0);
        assert_eq!(spans, vec![(5.0, 5.0)], "degenerate but not a panic");
    }

    #[test]
    fn get_shape_horizontal_spans_ellipse_with_zero_radii_is_empty_not_a_panic() {
        // radii.height == 0 → dy/0 → NaN → `NaN.abs() <= 0.0` is false → no spans.
        let e = ShapeBoundary::Ellipse {
            center: Point { x: 0.0, y: 0.0 },
            radii: Size::zero(),
        };
        assert!(
            get_shape_horizontal_spans(&e, 0.0, 1.0).is_empty(),
            "a zero-sized ellipse divides by zero but must not panic"
        );
    }

    #[test]
    fn get_shape_horizontal_spans_polygon_delegates_to_the_scanline() {
        let p = ShapeBoundary::Polygon {
            points: vec![
                Point { x: 0.0, y: 0.0 },
                Point { x: 100.0, y: 0.0 },
                Point { x: 0.0, y: 100.0 },
            ],
        };
        let spans = get_shape_horizontal_spans(&p, 10.0, 1.0);
        assert_eq!(spans.len(), 1);
        assert!(spans[0].1 > spans[0].0);
    }

    // =====================================================================
    // numeric/other: extract_line_breaks + try_incremental_relayout
    // =====================================================================

    #[test]
    fn extract_line_breaks_of_no_items_is_empty_but_keeps_the_width() {
        let lb = extract_line_breaks(&[], 640.0);
        assert!(lb.line_ranges.is_empty());
        assert!(lb.line_widths.is_empty());
        assert_eq!(lb.available_width, 640.0);
        // Even a NaN constraint round-trips untouched.
        let nan = extract_line_breaks(&[], f32::NAN);
        assert!(nan.available_width.is_nan());
    }

    #[test]
    fn extract_line_breaks_groups_items_by_line_index() {
        let items = vec![
            pos(cl("a", 10.0), 0.0, 0.0, 0),
            pos(cl("b", 10.0), 10.0, 0.0, 0),
            pos(cl("c", 10.0), 0.0, 20.0, 1),
        ];
        let lb = extract_line_breaks(&items, 100.0);
        assert_eq!(lb.line_ranges, vec![(0, 2), (2, 3)]);
        assert_eq!(lb.line_widths, vec![20.0, 10.0]);
        assert_eq!(lb.line_ranges.len(), lb.line_widths.len());
    }

    #[test]
    fn extract_line_breaks_splits_on_every_line_index_change_even_going_backwards() {
        // The scanner is purely edge-triggered: a non-monotonic line_index sequence
        // produces THREE ranges, not two. Pinned so a reorder-tolerant rewrite is visible.
        let items = vec![
            pos(cl("a", 10.0), 0.0, 0.0, 0),
            pos(cl("b", 10.0), 0.0, 20.0, 1),
            pos(cl("c", 10.0), 0.0, 0.0, 0),
        ];
        let lb = extract_line_breaks(&items, 100.0);
        assert_eq!(lb.line_ranges.len(), 3);
        assert_eq!(lb.line_widths, vec![10.0, 10.0, 10.0]);
    }

    #[test]
    fn try_incremental_relayout_no_dirty_items_is_a_glyph_swap() {
        let lb = CachedLineBreaks {
            line_ranges: vec![(0, 2)],
            line_widths: vec![20.0],
            available_width: 100.0,
        };
        assert!(matches!(
            try_incremental_relayout(&[], &[10.0, 10.0], &[10.0, 10.0], &lb),
            IncrementalRelayoutResult::GlyphSwap
        ));
    }

    #[test]
    fn try_incremental_relayout_out_of_range_dirty_index_falls_back_to_full() {
        let lb = CachedLineBreaks {
            line_ranges: vec![(0, 2)],
            line_widths: vec![20.0],
            available_width: 100.0,
        };
        assert!(matches!(
            try_incremental_relayout(&[99], &[10.0, 10.0], &[10.0, 10.0], &lb),
            IncrementalRelayoutResult::FullRelayout
        ));
        assert!(
            matches!(
                try_incremental_relayout(&[usize::MAX], &[10.0], &[10.0], &lb),
                IncrementalRelayoutResult::FullRelayout
            ),
            "usize::MAX must not index-panic"
        );
        // Mismatched advance vectors are also caught by the bounds check.
        assert!(matches!(
            try_incremental_relayout(&[1], &[10.0, 10.0], &[10.0], &lb),
            IncrementalRelayoutResult::FullRelayout
        ));
    }

    #[test]
    fn try_incremental_relayout_same_width_is_a_glyph_swap() {
        let lb = CachedLineBreaks {
            line_ranges: vec![(0, 2)],
            line_widths: vec![20.0],
            available_width: 100.0,
        };
        // Below the 0.001 epsilon → treated as unchanged.
        assert!(matches!(
            try_incremental_relayout(&[0], &[10.0, 10.0], &[10.0005, 10.0], &lb),
            IncrementalRelayoutResult::GlyphSwap
        ));
    }

    #[test]
    fn try_incremental_relayout_shifts_when_it_still_fits_and_reflows_when_it_does_not() {
        let lb = CachedLineBreaks {
            line_ranges: vec![(0, 2), (2, 4)],
            line_widths: vec![20.0, 20.0],
            available_width: 100.0,
        };
        let old = [10.0, 10.0, 10.0, 10.0];

        let grew = [10.0, 30.0, 10.0, 10.0]; // line 0 → 40 ≤ 100
        match try_incremental_relayout(&[1], &old, &grew, &lb) {
            IncrementalRelayoutResult::LineShift {
                affected_item,
                delta,
            } => {
                assert_eq!(affected_item, 1);
                assert_eq!(delta, 20.0);
            }
            other => panic!("expected LineShift, got {other:?}"),
        }

        let exploded = [10.0, 10.0, 10.0, 500.0]; // line 1 → 510 > 100
        match try_incremental_relayout(&[3], &old, &exploded, &lb) {
            IncrementalRelayoutResult::PartialReflow { reflow_from_line } => {
                assert_eq!(reflow_from_line, 1);
            }
            other => panic!("expected PartialReflow, got {other:?}"),
        }
    }

    #[test]
    fn try_incremental_relayout_two_width_changes_are_a_full_relayout() {
        // A whole-run restyle (every advance different) must NEVER become a
        // LineShift: one pivot + one delta cannot describe the line, and the
        // shift pasted new glyphs onto old pen positions — the oscillating
        // "squashed garbage" TextInput (2026-08-29).
        let lb = CachedLineBreaks {
            line_ranges: vec![(0, 4)],
            line_widths: vec![40.0],
            available_width: 1000.0,
        };
        let old = [10.0, 10.0, 10.0, 10.0];
        let restyled = [14.5, 14.5, 14.5, 14.5]; // 16px glyphs on an 11px cache
        assert!(matches!(
            try_incremental_relayout(&[0, 1, 2, 3], &old, &restyled, &lb),
            IncrementalRelayoutResult::FullRelayout
        ));
        // Two changed items out of four: still not describable by one delta.
        let two = [10.0, 12.0, 10.0, 13.0];
        assert!(matches!(
            try_incremental_relayout(&[1, 3], &old, &two, &lb),
            IncrementalRelayoutResult::FullRelayout
        ));
        // Exactly one change keeps the fast path.
        let one = [10.0, 12.0, 10.0, 10.0];
        assert!(matches!(
            try_incremental_relayout(&[1, 3], &old, &one, &lb),
            IncrementalRelayoutResult::LineShift {
                affected_item: 1,
                ..
            }
        ));
    }

    #[test]
    fn try_incremental_relayout_dirty_item_outside_every_line_range_is_a_full_relayout() {
        let lb = CachedLineBreaks {
            line_ranges: vec![(0, 1)],
            line_widths: vec![10.0],
            available_width: 100.0,
        };
        // Item 1 exists in the advance arrays but is on no known line.
        assert!(matches!(
            try_incremental_relayout(&[1], &[10.0, 10.0], &[10.0, 50.0], &lb),
            IncrementalRelayoutResult::FullRelayout
        ));
    }

    #[test]
    fn try_incremental_relayout_with_nan_advances_reflows_rather_than_shifting() {
        let lb = CachedLineBreaks {
            line_ranges: vec![(0, 1)],
            line_widths: vec![10.0],
            available_width: 100.0,
        };
        // delta = NaN: `NaN.abs() < 0.001` is false, and `NaN <= width` is false,
        // so we land in PartialReflow — a defined outcome, not a panic.
        match try_incremental_relayout(&[0], &[10.0], &[f32::NAN], &lb) {
            IncrementalRelayoutResult::PartialReflow { reflow_from_line } => {
                assert_eq!(reflow_from_line, 0);
            }
            other => panic!("NaN advance should reflow, got {other:?}"),
        }
    }

    // =====================================================================
    // getter/other: TextShapingCache + TextCacheMemoryReport + calculate_id
    // =====================================================================

    /// NEGATIVE CONTROL for the Arc dedup in `memory_report`.
    ///
    /// Sharing one `Arc` across several cache keys is the point of the cache,
    /// so a walk that charges `capacity()` per KEY reports bytes the process
    /// does not have. Delete the `counted.insert(..)` guards in
    /// `memory_report` and this test fails on the first assertion: the shared
    /// vec gets charged three times.
    #[test]
    fn shared_arcs_are_counted_once_not_once_per_key() {
        let mut cache = TextShapingCache::new();
        let shared: Arc<Vec<LogicalItem>> = Arc::new(Vec::with_capacity(64));
        let one_copy = 64 * size_of::<LogicalItem>();

        cache.logical_items.insert(1, Arc::clone(&shared));
        let single = cache.memory_report();
        assert_eq!(single.logical_items_bytes, one_copy);
        assert_eq!(single.shared_bytes_avoided, 0);

        // Two MORE keys, same allocation. Nothing was allocated, so nothing
        // reported may grow.
        cache.logical_items.insert(2, Arc::clone(&shared));
        cache.logical_items.insert(3, Arc::clone(&shared));
        let shared_thrice = cache.memory_report();
        assert_eq!(
            shared_thrice.logical_items_bytes, one_copy,
            "three keys, one allocation: bytes must not triple"
        );
        assert_eq!(
            shared_thrice.shared_bytes_avoided,
            2 * one_copy,
            "the two extra keys are exactly the error per-key counting made"
        );
        // Entry COUNT is a count of keys and legitimately does rise; only the
        // BYTES are a claim about memory.
        assert_eq!(shared_thrice.logical_items_entries, 3);

        // POSITIVE control, so the test cannot pass by reporting zero for
        // everything: a genuinely distinct allocation must still be added.
        cache
            .logical_items
            .insert(4, Arc::new(Vec::with_capacity(64)));
        let distinct = cache.memory_report();
        assert_eq!(distinct.logical_items_bytes, 2 * one_copy);
    }

    #[test]
    fn text_cache_memory_report_total_bytes_sums_only_the_byte_fields() {
        let r = TextCacheMemoryReport::default();
        assert_eq!(r.total_bytes(), 0);

        // Powers of two, so a wrong total names the offending field.
        let full = TextCacheMemoryReport {
            per_item_atoms: 0,
            per_item_segments: 0,
            per_item_detail_glyphs: 0,
            logical_items_entries: 1_000_000, // must NOT be counted
            logical_items_bytes: 1,
            visual_items_entries: 1_000_000, // must NOT be counted
            visual_items_bytes: 2,
            shaped_items_entries: 1_000_000, // must NOT be counted
            shaped_items_bytes: 4,
            shaped_glyph_bytes: 8,
            shaped_cluster_text_bytes: 16,
            per_item_shaped_entries: 1_000_000, // must NOT be counted
            per_item_shaped_bytes: 32,
            map_overhead_bytes: 64,
            style_arc_bytes: 128,
            combined_block_glyph_bytes: 256,
            // NOT summed by total_bytes(): it is the size of an error the
            // walk avoided, not memory the process holds.
            shared_bytes_avoided: 1 << 30,
            distinct_style_arcs: 1_000_000, // a COUNT — must NOT be counted
            cluster_count: 1_000_000,       // a COUNT — must NOT be counted
        };
        assert_eq!(full.total_bytes(), 511, "1+2+4+8+16+32+64+128+256");

        // `bytes_per_cluster` divides by the count, so it must not divide by
        // zero and must use the count rather than any byte field.
        assert_eq!(
            TextCacheMemoryReport::default().bytes_per_cluster(),
            None,
            "no clusters cached => no per-cluster figure, not a division by zero"
        );
        assert_eq!(
            full.bytes_per_cluster(),
            Some(0),
            "511 / 1_000_000 truncates to 0"
        );
    }

    #[test]
    fn text_shaping_cache_new_is_empty_and_reports_zero_bytes() {
        let c = TextShapingCache::new();
        let r = c.memory_report();
        assert_eq!(r.total_bytes(), 0);
        assert_eq!(r.logical_items_entries, 0);
        assert_eq!(r.per_item_shaped_entries, 0);
        assert_eq!(c.generation, 0);
        // Default must agree with new().
        let d = TextShapingCache::default();
        assert_eq!(d.memory_report().total_bytes(), 0);
    }

    #[test]
    fn text_shaping_cache_begin_generation_is_idempotent_on_an_empty_cache() {
        let mut c = TextShapingCache::new();
        for expect in 1..=5_u64 {
            c.begin_generation();
            assert_eq!(c.generation, expect);
        }
        assert!(c.per_item_accessed.is_empty());
        assert!(c.per_item_shaped.is_empty());
    }

    /// The three STAGE caches must be swept, not just `per_item_shaped`.
    ///
    /// They had no cap and no sweep of any kind: every distinct piece of
    /// text ever laid out stayed resident for the life of the process, so
    /// editing grew them monotonically — the entry for a wording you deleted
    /// stayed shaped forever. Measured on one 960-line markdown: 792 entries
    /// each, 7.0 MB, ON TOP of the 13.5 MB of the same shaped text already
    /// held per layout node as `warm.inline`.
    ///
    /// Same policy as per-item: touched this generation survives, untouched
    /// goes. Generation 0 never evicts, so a first pass cannot throw away
    /// what it just built.
    ///
    /// NEGATIVE CONTROL: dropping the three `retain` calls from
    /// `begin_generation` makes the eviction assertions fail — run and seen.
    #[test]
    fn text_shaping_cache_begin_generation_evicts_unaccessed_stage_entries() {
        let mut c = TextShapingCache::new();
        c.logical_items.insert(1, Arc::new(Vec::new()));
        c.logical_items.insert(2, Arc::new(Vec::new()));
        c.visual_items.insert(1, Arc::new(Vec::new()));
        c.visual_items.insert(2, Arc::new(Vec::new()));
        // (d7) The monolithic shaped_items stage is deleted; the third
        // stage_entry_counts slot now reports the per-item store, which
        // has its own sweep (covered by the per-item eviction test).

        // Generation 0 never evicts — otherwise the very first layout pass
        // would discard the entries it had just produced.
        c.begin_generation();
        assert_eq!(
            c.stage_entry_counts(),
            (2, 2, 0),
            "gen 0 must keep everything"
        );

        // Touch only id 1. Rolling the generation drops id 2 from both.
        c.touch_stage(1);
        c.begin_generation();
        assert_eq!(
            c.stage_entry_counts(),
            (1, 1, 0),
            "an id not touched this generation must be evicted from every stage cache - these \
             were the unbounded ones"
        );
        assert!(c.logical_items.contains_key(&1));
        assert!(c.visual_items.contains_key(&1));

        // The accessed set resets, so surviving entries must be re-touched
        // to survive again — otherwise nothing would ever be released.
        assert!(c.stage_accessed.is_empty());
        c.begin_generation();
        assert_eq!(
            c.stage_entry_counts(),
            (1, 1, 0),
            "an empty accessed set skips the sweep rather than clearing all"
        );
    }

    /// (d7) The compaction roundtrip gate: expand(build(v)) == v
    /// EXACTLY, across every irregularity class — multi-glyph ligature
    /// details, kerned glyphs, non-Character kinds, fallback-font
    /// clusters, marker/fragment flags, and non-cluster atoms — on
    /// synthetic items where every field is controlled.
    #[test]
    fn d7_compact_shaped_entry_roundtrip_is_exact() {
        let mut base = cl("a", 8.0);
        if let ShapedItem::Cluster(ref mut c) = base {
            // The helper models a vmtx font; zero it so THIS cluster
            // takes the SIMPLE (no-detail) reconstruction arm — the
            // all-atoms vacuity this gate exists to prevent.
            c.glyphs[0].vertical_advance = 0.0;
        }
        let mut items: Vec<ShapedItem> = vec![base.clone(), cl("b", 9.0)];
        // Ligature detail: two glyphs in one cluster.
        if let ShapedItem::Cluster(ref mut c) = items[1] {
            let mut g2 = c.glyphs[0].clone();
            g2.cluster_offset = 1;
            g2.kerning = -0.5;
            c.glyphs.push(g2);
        }
        // Fallback font: same style Arc, different font_hash → atom.
        let mut fallback = cl("c", 7.0);
        if let ShapedItem::Cluster(ref mut c) = fallback {
            c.glyphs[0].font_hash = c.glyphs[0].font_hash.wrapping_add(1);
        }
        items.push(fallback);
        // Marker cluster → atom.
        let mut marker = cl("d", 6.0);
        if let ShapedItem::Cluster(ref mut c) = marker {
            c.marker_position_outside = Some(true);
        }
        items.push(marker);
        // Fragment flags OFF their shaping-stage (true, true) default →
        // atom (the line breaker owns cleared flags).
        let mut frag = cl("e", 5.0);
        if let ShapedItem::Cluster(ref mut c) = frag {
            c.is_first_fragment = false;
        }
        items.push(frag);
        // Non-cluster atom.
        items.push(ShapedItem::Break {
            source: ContentIndex {
                run_index: 0,
                item_index: 0,
            },
            break_info: InlineBreak {
                break_type: BreakType::Hard,
                clear: ClearType::None,
                content_index: 0,
            },
        });

        let compact = CompactShapedEntry::build(&items);
        assert!(
            !compact.clusters.is_empty(),
            "at least the plain 'a' cluster must COMPACT (not atomize) — an all-atoms entry makes \
             this gate vacuous for the simple-cluster reconstruction arm"
        );
        // (d7 segmented) The whole-cluster fallback FONT now starts a
        // new SEGMENT (font is a segment field); only marker, cleared
        // fragment flags, and the Break remain atoms.
        assert_eq!(
            compact.atom_count(),
            3,
            "marker+fragment(false)+break are atoms; the fallback-font cluster must SEGMENT"
        );
        assert!(compact.segment_count() >= 2, "fallback font segments");
        let expanded = compact.expand();
        assert_eq!(expanded.len(), items.len());
        for (i, (e, o)) in expanded.iter().zip(items.iter()).enumerate() {
            assert_eq!(e, o, "roundtrip diverges at item {i}");
        }

        // Empty entry roundtrips too.
        assert!(CompactShapedEntry::build(&[]).expand().is_empty());

        // (d7 segmented) A coalesce-group-like sequence: clusters from
        // TWO different text Arcs must land in TWO SEGMENTS with zero
        // extra atoms — the single-header design atomized everything
        // after the first Arc change (~300 B/cluster on the real
        // corpus, measured 9.1 MiB; the regression this pin prevents).
        let mut a1 = cl("x", 4.0);
        let mut a2 = cl("y", 5.0);
        for it in [&mut a1, &mut a2] {
            if let ShapedItem::Cluster(ref mut c) = it {
                c.glyphs[0].vertical_advance = 0.0;
            }
        }
        let two = vec![a1.clone(), a2.clone()];
        let compact2 = CompactShapedEntry::build(&two);
        assert_eq!(
            compact2.atom_count(),
            0,
            "distinct text Arcs must SEGMENT, not atomize"
        );
        assert_eq!(compact2.segment_count(), 2);
        let exp2 = compact2.expand();
        assert_eq!(exp2.len(), 2);
        for (i, (e, o)) in exp2.iter().zip(two.iter()).enumerate() {
            assert_eq!(e, o, "segmented roundtrip diverges at {i}");
        }
    }

    #[test]
    fn text_shaping_cache_begin_generation_evicts_unaccessed_per_item_entries() {
        let mut c = TextShapingCache::new();
        c.per_item_shaped.insert(
            1,
            Arc::new(PerItemShapedEntry {
                compact: CompactShapedEntry::build(&[cl("a", 8.0)]),
                total_advance: 8.0,
                items: Vec::new(),
            }),
        );
        c.per_item_shaped.insert(
            2,
            Arc::new(PerItemShapedEntry {
                compact: CompactShapedEntry::build(&[]),
                total_advance: 0.0,
                items: Vec::new(),
            }),
        );
        // Generation 0 → the eviction guard is skipped entirely.
        c.begin_generation();
        assert_eq!(c.per_item_shaped.len(), 2, "gen 0 never evicts");

        // Touch only key 1, then roll the generation: key 2 must be dropped.
        c.per_item_accessed.insert(1);
        c.begin_generation();
        assert_eq!(c.per_item_shaped.len(), 1);
        assert!(c.per_item_shaped.contains_key(&1));

        // Nothing accessed this generation → the retain is skipped (NOT a full flush).
        c.begin_generation();
        assert_eq!(
            c.per_item_shaped.len(),
            1,
            "an empty access-set must not wipe the cache"
        );
    }

    #[test]
    fn use_old_layout_accepts_a_render_only_change_and_rejects_layout_changes() {
        let c = UnifiedConstraints::default();
        let red = styled(|s| {
            s.color = ColorU {
                r: 255,
                g: 0,
                b: 0,
                a: 255,
            };
        });
        let old = [text_content("hi", style())];
        let new_colour = [text_content("hi", red)];
        assert!(
            TextShapingCache::use_old_layout(&c, &c, &old, &new_colour),
            "a colour-only change must reuse the cached layout"
        );

        // Different text → no reuse.
        let new_text = [text_content("ho", style())];
        assert!(!TextShapingCache::use_old_layout(&c, &c, &old, &new_text));

        // Different font size → no reuse.
        let bigger = [text_content("hi", styled(|s| s.font_size_px = 32.0))];
        assert!(!TextShapingCache::use_old_layout(&c, &c, &old, &bigger));

        // Different constraints → no reuse.
        let c2 = UnifiedConstraints {
            available_width: AvailableSpace::Definite(100.0),
            ..Default::default()
        };
        assert!(!TextShapingCache::use_old_layout(&c, &c2, &old, &old));
    }

    #[test]
    fn use_old_layout_on_empty_content_and_length_or_variant_mismatch() {
        let c = UnifiedConstraints::default();
        assert!(
            TextShapingCache::use_old_layout(&c, &c, &[], &[]),
            "empty vs empty is trivially reusable"
        );
        let one = [text_content("a", style())];
        assert!(!TextShapingCache::use_old_layout(&c, &c, &[], &one));
        assert!(!TextShapingCache::use_old_layout(&c, &c, &one, &[]));

        // Same length, different variant.
        let space = [InlineContent::Space(InlineSpace {
            width: 4.0,
            is_breaking: true,
            is_stretchy: true,
        })];
        assert!(!TextShapingCache::use_old_layout(&c, &c, &one, &space));
        assert!(TextShapingCache::use_old_layout(&c, &c, &space, &space));
    }

    #[test]
    fn inline_content_layout_eq_recurses_into_ruby() {
        let ruby = |base: &str| InlineContent::Ruby {
            base: vec![text_content(base, style())],
            text: vec![text_content("ふり", style())],
            style: style(),
        };
        assert!(TextShapingCache::inline_content_layout_eq(
            &ruby("漢"),
            &ruby("漢")
        ));
        assert!(!TextShapingCache::inline_content_layout_eq(
            &ruby("漢"),
            &ruby("字")
        ));
    }

    #[test]
    fn calculate_id_is_deterministic_and_discriminating() {
        assert_eq!(calculate_id(&"abc"), calculate_id(&"abc"));
        assert_ne!(calculate_id(&"abc"), calculate_id(&"abd"));
        assert_eq!(calculate_id(&0_u64), calculate_id(&0_u64));
        assert_ne!(calculate_id(&0_u64), calculate_id(&u64::MAX));
        // Empty input must still produce a stable id (not a panic / not zero-by-accident).
        let e: Vec<u8> = Vec::new();
        assert_eq!(calculate_id(&e), calculate_id(&Vec::<u8>::new()));
    }

    #[test]
    fn shaped_items_key_new_on_empty_visual_items_is_stable() {
        let a = ShapedItemsKey::new(7, &[]);
        let b = ShapedItemsKey::new(7, &[]);
        assert_eq!(a, b);
        assert_eq!(hash_of(&a), hash_of(&b));
        // The cache id participates in identity.
        assert_ne!(a, ShapedItemsKey::new(8, &[]));
    }

    #[test]
    fn shaped_items_key_new_hashes_the_text_styles() {
        let vi = |st: Arc<StyleProperties>| VisualItem {
            logical_source: LogicalItem::Text {
                source: ci(0, 0),
                text: Arc::from("a"),
                style: st,
                marker_position_outside: None,
                source_node_id: None,
            },
            bidi_level: BidiLevel::new(0),
            script: Script::Latin,
            text: "a".to_string(),
            run_byte_offset: 0,
        };
        let base = ShapedItemsKey::new(1, &[vi(style())]);
        let same = ShapedItemsKey::new(1, &[vi(style())]);
        let other = ShapedItemsKey::new(1, &[vi(styled(|s| s.font_size_px = 32.0))]);
        assert_eq!(base, same);
        assert_ne!(
            base.style_hash, other.style_hash,
            "font size must change the key"
        );
    }

    // =====================================================================
    // getter/predicate: OverflowInfo + UnifiedLayout
    // =====================================================================

    #[test]
    fn overflow_info_default_has_no_overflow() {
        let o = OverflowInfo::default();
        assert!(!o.has_overflow());
        assert_eq!(o.unclipped_bounds, Rect::default());

        let with = OverflowInfo {
            overflow_items: vec![cl("a", 8.0)],
            unclipped_bounds: Rect::default(),
        };
        assert!(with.has_overflow());
    }

    fn layout_of(items: Vec<PositionedItem>) -> UnifiedLayout {
        UnifiedLayout {
            items,
            overflow: OverflowInfo::default(),
        }
    }

    #[test]
    fn unified_layout_empty_is_inert_across_every_accessor() {
        let l = layout_of(Vec::new());
        assert!(l.is_empty());
        assert_eq!(l.bounds(), Rect::default());
        assert_eq!(l.first_baseline(), None);
        assert_eq!(l.last_baseline(), None);
        assert_eq!(l.get_first_cluster_cursor(), None);
        assert_eq!(l.get_last_cluster_cursor(), None);
        assert!(l.grapheme_stops().is_empty());
        assert_eq!(
            l.hittest_cursor(LogicalPosition { x: 0.0, y: 0.0 }),
            None,
            "hit-testing an empty layout must return None, not index [0]"
        );
        assert_eq!(
            l.hittest_cursor(LogicalPosition {
                x: f32::NAN,
                y: f32::NAN
            }),
            None
        );
    }

    #[test]
    fn unified_layout_bounds_spans_all_items() {
        let l = layout_of(vec![
            pos(cl("a", 10.0), 0.0, 0.0, 0),
            pos(cl("b", 10.0), 90.0, 20.0, 1),
        ]);
        let b = l.bounds();
        assert_eq!(b.x, 0.0);
        assert_eq!(b.y, 0.0);
        assert_eq!(b.width, 100.0, "0 → 90+10");
        approx(b.height, 36.0); // 0 → 20 + the 16px line box
        assert!(!l.is_empty());
    }

    #[test]
    fn unified_layout_baselines_skip_breaks_and_tabs() {
        let l = layout_of(vec![
            pos(brk(), 0.0, 0.0, 0),
            pos(cl("a", 10.0), 0.0, 0.0, 0),
            pos(obj(10.0, 20.0, 5.0), 10.0, 0.0, 0),
            pos(tab(8.0, 16.0), 20.0, 0.0, 0),
        ]);
        // Where each baseline LIES (the item's top plus the ascent the line
        // used): the cluster's 13px ascent, the 20px object's 15px above its
        // `baseline_offset` of 5.
        approx(
            l.first_baseline().expect("the cluster, not the break"),
            13.0,
        );
        assert_eq!(l.last_baseline(), Some(15.0), "the object, not the tab");
    }

    #[test]
    fn the_last_line_baseline_is_the_last_lines_item_top_plus_its_ascent() {
        // Two lines: the baseline is the SECOND line's, where the line put it.
        let text = layout_of(vec![
            pos(cl("a", 8.0), 0.0, 0.0, 0),
            pos(cl("b", 8.0), 0.0, 20.0, 1),
            pos(brk(), 8.0, 20.0, 1),
        ]);
        // cl's ascent is 13 (see get_item_vertical_metrics_approx_for_every_variant).
        approx(text.last_line_baseline().expect("a line with glyphs"), 33.0);
        // An atomic inline: its height above its baseline offset (20 - 5).
        let object = layout_of(vec![
            pos(cl("a", 8.0), 0.0, 0.0, 0),
            pos(obj(10.0, 20.0, 5.0), 0.0, 18.0, 1),
        ]);
        approx(
            object.last_line_baseline().expect("the object's line"),
            33.0,
        );
        // Breaks and tabs alone are no line with a baseline.
        assert_eq!(
            layout_of(vec![
                pos(brk(), 0.0, 0.0, 0),
                pos(tab(8.0, 16.0), 0.0, 0.0, 0)
            ])
            .last_line_baseline(),
            None
        );
    }

    #[test]
    fn unified_layout_cluster_cursors_skip_non_clusters() {
        let l = layout_of(vec![
            pos(brk(), 0.0, 0.0, 0),
            pos(cl_at("a", 10.0, 0, 0), 0.0, 0.0, 0),
            pos(cl_at("b", 10.0, 0, 1), 10.0, 0.0, 0),
            pos(tab(8.0, 16.0), 20.0, 0.0, 0),
        ]);
        assert_eq!(
            l.get_first_cluster_cursor(),
            Some(TextCursor {
                cluster_id: gid(0, 0),
                affinity: CursorAffinity::Leading
            })
        );
        assert_eq!(
            l.get_last_cluster_cursor(),
            Some(TextCursor {
                cluster_id: gid(0, 1),
                affinity: CursorAffinity::Trailing
            })
        );

        // A layout with no clusters at all has no cursors.
        let no_clusters = layout_of(vec![pos(brk(), 0.0, 0.0, 0)]);
        assert_eq!(no_clusters.get_first_cluster_cursor(), None);
        assert_eq!(no_clusters.get_last_cluster_cursor(), None);
    }

    #[test]
    fn unified_layout_grapheme_stops_sorts_dedups_and_folds_combining_marks() {
        // Deliberately out of order, with a duplicate id and a combining mark.
        let l = layout_of(vec![
            pos(cl_at("b", 10.0, 0, 1), 10.0, 0.0, 0),
            pos(cl_at("a", 10.0, 0, 0), 0.0, 0.0, 0),
            pos(cl_at("a", 10.0, 0, 0), 0.0, 0.0, 0), // duplicate id
            pos(cl_at("\u{0301}", 0.0, 0, 2), 20.0, 0.0, 0), // combining acute
        ]);
        let stops = l.grapheme_stops();
        assert_eq!(
            stops,
            vec![gid(0, 0), gid(0, 1)],
            "sorted, de-duplicated, with the combining mark folded away"
        );
    }

    #[test]
    fn unified_layout_cluster_is_grapheme_continuation() {
        assert!(UnifiedLayout::cluster_is_grapheme_continuation("\u{0301}"));
        assert!(
            UnifiedLayout::cluster_is_grapheme_continuation("\u{FE0F}"),
            "VS-16"
        );
        assert!(!UnifiedLayout::cluster_is_grapheme_continuation("a"));
        assert!(!UnifiedLayout::cluster_is_grapheme_continuation("中"));
        assert!(
            !UnifiedLayout::cluster_is_grapheme_continuation(""),
            "an empty cluster must return false, not panic"
        );
    }

    #[test]
    fn unified_layout_grapheme_caret_offset_maps_affinity_and_gaps() {
        let stops = [gid(0, 0), gid(0, 1), gid(0, 2)];
        assert_eq!(
            UnifiedLayout::grapheme_caret_offset(
                &stops,
                &TextCursor {
                    cluster_id: gid(0, 0),
                    affinity: CursorAffinity::Leading
                }
            ),
            Some(0)
        );
        assert_eq!(
            UnifiedLayout::grapheme_caret_offset(
                &stops,
                &TextCursor {
                    cluster_id: gid(0, 2),
                    affinity: CursorAffinity::Trailing
                }
            ),
            Some(3),
            "the document end is len, i.e. one past the last stop"
        );
        // A cursor addressing a folded mark snaps back to the preceding stop.
        assert_eq!(
            UnifiedLayout::grapheme_caret_offset(
                &stops,
                &TextCursor {
                    cluster_id: gid(0, 99),
                    affinity: CursorAffinity::Leading
                }
            ),
            Some(2)
        );
        // A cursor before every stop has no offset.
        assert_eq!(
            UnifiedLayout::grapheme_caret_offset(
                &[gid(5, 5)],
                &TextCursor {
                    cluster_id: gid(0, 0),
                    affinity: CursorAffinity::Leading
                }
            ),
            None
        );
        // Empty stop list → None, no panic.
        assert_eq!(
            UnifiedLayout::grapheme_caret_offset(
                &[],
                &TextCursor {
                    cluster_id: gid(0, 0),
                    affinity: CursorAffinity::Leading
                }
            ),
            None
        );
    }

    #[test]
    fn unified_layout_cursor_from_grapheme_offset_clamps_past_the_end() {
        let stops = [gid(0, 0), gid(0, 1)];
        assert_eq!(
            UnifiedLayout::cursor_from_grapheme_offset(&stops, 0),
            TextCursor {
                cluster_id: gid(0, 0),
                affinity: CursorAffinity::Leading
            }
        );
        assert_eq!(
            UnifiedLayout::cursor_from_grapheme_offset(&stops, 1),
            TextCursor {
                cluster_id: gid(0, 1),
                affinity: CursorAffinity::Leading
            }
        );
        // offset == len and anything beyond clamp to Trailing-on-last.
        let end = TextCursor {
            cluster_id: gid(0, 1),
            affinity: CursorAffinity::Trailing,
        };
        assert_eq!(UnifiedLayout::cursor_from_grapheme_offset(&stops, 2), end);
        assert_eq!(
            UnifiedLayout::cursor_from_grapheme_offset(&stops, usize::MAX),
            end,
            "usize::MAX must clamp, not overflow"
        );
    }

    #[test]
    #[should_panic]
    fn unified_layout_cursor_from_grapheme_offset_panics_on_an_empty_stop_list() {
        // FINDING: with `stops == []`, `offset >= n` is true for offset 0 and the
        // function indexes `stops[n - 1]` → `0usize - 1`. Every public caller happens
        // to guard `stops.is_empty()` first, so this is latent, not live — but the
        // helper itself has no guard.
        let _ = UnifiedLayout::cursor_from_grapheme_offset(&[], 0);
    }

    #[test]
    fn unified_layout_cursor_motion_on_an_empty_layout_returns_the_cursor_unchanged() {
        let l = layout_of(Vec::new());
        let c = TextCursor {
            cluster_id: gid(0, 0),
            affinity: CursorAffinity::Leading,
        };
        let mut dbg = None;
        assert_eq!(l.move_cursor_left(c, &mut dbg), c);
        assert_eq!(l.move_cursor_right(c, &mut dbg), c);
        assert_eq!(l.move_cursor_to_line_start(c, &mut dbg), c);
        assert_eq!(l.move_cursor_to_line_end(c, &mut dbg), c);
        assert_eq!(l.move_cursor_to_prev_word(c, &mut dbg), c);
        assert_eq!(l.move_cursor_to_next_word(c, &mut dbg), c);
        let mut goal = None;
        assert_eq!(l.move_cursor_up(c, &mut goal, &mut dbg), c);
        assert_eq!(l.move_cursor_down(c, &mut goal, &mut dbg), c);
    }

    #[test]
    fn unified_layout_move_cursor_left_right_walk_one_grapheme_at_a_time() {
        let l = layout_of(vec![
            pos(cl_at("a", 10.0, 0, 0), 0.0, 0.0, 0),
            pos(cl_at("b", 10.0, 0, 1), 10.0, 0.0, 0),
            pos(cl_at("c", 10.0, 0, 2), 20.0, 0.0, 0),
        ]);
        let mut dbg = None;
        let start = TextCursor {
            cluster_id: gid(0, 0),
            affinity: CursorAffinity::Leading,
        };

        // Left at the document start is a fixed point (saturating_sub).
        assert_eq!(l.move_cursor_left(start, &mut dbg), start);

        // Right advances one stop at a time and reaches the document end.
        let c1 = l.move_cursor_right(start, &mut dbg);
        assert_eq!(c1.cluster_id, gid(0, 1));
        let c2 = l.move_cursor_right(c1, &mut dbg);
        assert_eq!(c2.cluster_id, gid(0, 2));
        let end = l.move_cursor_right(c2, &mut dbg);
        assert_eq!(end.cluster_id, gid(0, 2));
        assert_eq!(end.affinity, CursorAffinity::Trailing, "document end");
        // ...and is a fixed point there.
        assert_eq!(l.move_cursor_right(end, &mut dbg), end);

        // Left from the end walks back symmetrically.
        assert_eq!(l.move_cursor_left(end, &mut dbg).cluster_id, gid(0, 2));
    }

    #[test]
    fn unified_layout_hittest_cursor_picks_the_nearest_cluster_and_its_half() {
        let l = layout_of(vec![
            pos(cl_at("a", 10.0, 0, 0), 0.0, 0.0, 0),
            pos(cl_at("b", 10.0, 0, 1), 10.0, 0.0, 0),
        ]);
        let hit = |x: f32| l.hittest_cursor(LogicalPosition { x, y: 5.0 }).unwrap();
        assert_eq!(hit(1.0).cluster_id, gid(0, 0));
        assert_eq!(hit(1.0).affinity, CursorAffinity::Leading);
        assert_eq!(
            hit(9.0).affinity,
            CursorAffinity::Trailing,
            "right half of 'a'"
        );
        assert_eq!(hit(11.0).cluster_id, gid(0, 1));
        // Far outside the layout still resolves to the nearest cluster, not None.
        assert_eq!(hit(-1000.0).cluster_id, gid(0, 0));
        assert_eq!(hit(1000.0).cluster_id, gid(0, 1));
    }

    /// THE BUG CLASS, end to end: a click in a PADDED, SCROLLED text box must
    /// resolve to the character under the pointer.
    ///
    /// Two clusters "a" (x 0..10) and "b" (x 10..20) in a box whose border box
    /// starts at (100, 50), with a content inset of (6, 3) and its own
    /// horizontal scroll at 12. The raster therefore paints "a" at window
    /// x 94..104 and "b" at 104..114.
    ///
    /// The three ways this used to go wrong are asserted as explicit
    /// counter-examples, because each was a real production path:
    ///   * dropping the own scroll  = the click path before the hand-fix;
    ///   * dropping the content inset = the WebRender path (and the drag path);
    ///   * dropping both            = what a naive `cursor - node_pos` gives.
    #[test]
    fn a_click_in_a_padded_scrolled_box_lands_on_the_painted_character() {
        use azul_core::spaces::{ContentInset, ScrollOffset, StaticLayoutPoint};

        let l = layout_of(vec![
            pos(cl_at("a", 10.0, 0, 0), 0.0, 0.0, 0),
            pos(cl_at("b", 10.0, 0, 1), 10.0, 0.0, 0),
        ]);

        let node_origin = LogicalPosition { x: 100.0, y: 50.0 };
        let inset = ContentInset::new(6.0, 3.0);
        let own = ScrollOffset::new(12.0, 0.0);
        // No scrolling ancestors here, so window space == static layout space.
        let at = |window_x: f32| {
            StaticLayoutPoint::new(LogicalPosition {
                x: window_x,
                y: 58.0,
            })
            .to_border_box_local(node_origin)
            .to_content_box_local(inset)
            .scrolled_by(own)
        };

        // "a" is painted at 94..104 — its left half, then its right half.
        assert_eq!(l.hittest_point(at(95.0)).unwrap().cluster_id, gid(0, 0));
        assert_eq!(
            l.hittest_point(at(95.0)).unwrap().affinity,
            CursorAffinity::Leading
        );
        assert_eq!(
            l.hittest_point(at(103.0)).unwrap().affinity,
            CursorAffinity::Trailing,
            "right half of the painted 'a'"
        );
        // "b" is painted at 104..114.
        assert_eq!(l.hittest_point(at(105.0)).unwrap().cluster_id, gid(0, 1));

        // Counter-example 1: forget the box's own scroll. The pointer sits on
        // the painted 'b' but resolves to 'a' — 12px to the LEFT.
        let no_scroll = StaticLayoutPoint::new(LogicalPosition { x: 105.0, y: 58.0 })
            .to_border_box_local(node_origin)
            .to_content_box_local(inset)
            .scrolled_by(ScrollOffset::zero());
        assert_eq!(l.hittest_point(no_scroll).unwrap().cluster_id, gid(0, 0));

        // Counter-example 2: forget the content inset. The pointer is on the
        // painted 'a' (94..104) but resolves to 'b' — 6px to the RIGHT.
        let no_inset = StaticLayoutPoint::new(LogicalPosition { x: 103.0, y: 58.0 })
            .to_border_box_local(node_origin)
            .to_content_box_local(ContentInset::ZERO)
            .scrolled_by(own);
        assert_eq!(l.hittest_point(no_inset).unwrap().cluster_id, gid(0, 1));
        assert_eq!(l.hittest_point(at(103.0)).unwrap().cluster_id, gid(0, 0));

        // The conversion is exactly invertible, so nothing is lost on the way.
        assert_eq!(
            at(105.0)
                .unscrolled_by(own)
                .to_border_box_local(inset)
                .to_static_layout(node_origin)
                .get(),
            LogicalPosition { x: 105.0, y: 58.0 },
        );
    }

    #[test]
    fn unified_layout_get_selection_rects_on_an_unknown_range_is_empty() {
        let l = layout_of(vec![pos(cl_at("a", 10.0, 0, 0), 0.0, 0.0, 0)]);
        let unknown = SelectionRange {
            start: TextCursor {
                cluster_id: gid(9, 9),
                affinity: CursorAffinity::Leading,
            },
            end: TextCursor {
                cluster_id: gid(9, 9),
                affinity: CursorAffinity::Trailing,
            },
        };
        assert!(l.get_selection_rects(&unknown).is_empty());

        // A degenerate (collapsed) range over a real cluster must not panic.
        let collapsed = SelectionRange {
            start: TextCursor {
                cluster_id: gid(0, 0),
                affinity: CursorAffinity::Leading,
            },
            end: TextCursor {
                cluster_id: gid(0, 0),
                affinity: CursorAffinity::Leading,
            },
        };
        let _ = l.get_selection_rects(&collapsed);
    }

    #[test]
    fn unified_layout_get_cursor_rect_for_known_and_unknown_cursors() {
        let l = layout_of(vec![pos(cl_at("a", 10.0, 0, 0), 5.0, 7.0, 0)]);
        let leading = l.get_cursor_rect(&TextCursor {
            cluster_id: gid(0, 0),
            affinity: CursorAffinity::Leading,
        });
        let r = leading.expect("the leading edge of a placed cluster must have a rect");
        assert_eq!(r.origin.x, 5.0);
        assert_eq!(r.origin.y, 7.0);
        assert_eq!(r.size.width, 1.0, "the caret is a 1px sliver");

        // A cursor in a run that was never laid out has no rect.
        assert_eq!(
            l.get_cursor_rect(&TextCursor {
                cluster_id: gid(9, 0),
                affinity: CursorAffinity::Leading
            }),
            None
        );
        // An empty layout has no rect for anything.
        assert_eq!(
            layout_of(Vec::new()).get_cursor_rect(&TextCursor {
                cluster_id: gid(0, 0),
                affinity: CursorAffinity::Leading
            }),
            None
        );
    }

    // =====================================================================
    // constructor/getter: BreakCursor
    // =====================================================================

    #[test]
    fn break_cursor_new_on_an_empty_slice_is_both_at_start_and_done() {
        let items: Vec<ShapedItem> = Vec::new();
        let mut c = BreakCursor::new(&items);
        assert!(c.is_at_start());
        assert!(c.is_done());
        assert_eq!(c.word_break, WordBreak::Normal);
        assert_eq!(c.hyphens, Hyphens::default());
        assert_eq!(c.line_break, LineBreakStrictness::default());
        assert!(c.peek_next_unit().is_empty());
        assert!(c.peek_next_single_item().is_empty());
        assert!(c.drain_remaining().is_empty());
    }

    #[test]
    fn break_cursor_with_word_break_stores_the_mode() {
        let items = vec![cl("a", 8.0)];
        let c = BreakCursor::with_word_break(&items, WordBreak::BreakAll);
        assert_eq!(c.word_break, WordBreak::BreakAll);
        assert!(c.is_at_start());
        assert!(!c.is_done());
    }

    #[test]
    fn break_cursor_consume_zero_is_a_no_op() {
        let items = vec![cl("a", 8.0), cl("b", 8.0)];
        let mut c = BreakCursor::new(&items);
        c.consume(0);
        assert!(c.is_at_start());
        assert_eq!(c.next_item_index, 0);
    }

    #[test]
    fn break_cursor_consume_advances_and_ends_the_stream() {
        let items = vec![cl("a", 8.0), cl("b", 8.0)];
        let mut c = BreakCursor::new(&items);
        c.consume(1);
        assert!(!c.is_at_start());
        assert!(!c.is_done());
        assert_eq!(c.peek_next_single_item().len(), 1);
        c.consume(1);
        assert!(c.is_done());
        assert!(c.peek_next_single_item().is_empty());
    }

    #[test]
    fn break_cursor_over_consuming_past_the_end_still_reports_done() {
        let items = vec![cl("a", 8.0)];
        let mut c = BreakCursor::new(&items);
        c.consume(usize::MAX);
        assert!(
            c.is_done(),
            "an over-consume must not wrap around to not-done"
        );
        assert!(
            c.drain_remaining().is_empty(),
            "drain_remaining is bounds-guarded"
        );
    }

    #[test]
    #[should_panic]
    fn break_cursor_peek_next_unit_after_over_consuming_slices_out_of_bounds() {
        // FINDING: `consume()` does not clamp `next_item_index` to `items.len()`, and
        // `peek_next_unit` slices `self.items[self.next_item_index..]` unguarded (unlike
        // `peek_next_single_item` / `drain_remaining`, which both test `<  len`). A caller
        // that over-consumes and then peeks gets a slice-index panic instead of an empty unit.
        let items = vec![cl("a", 8.0)];
        let mut c = BreakCursor::new(&items);
        c.consume(5);
        let _ = c.peek_next_unit();
    }

    #[test]
    fn break_cursor_drains_the_remainder_before_the_main_list() {
        let items = vec![cl("a", 8.0), cl("b", 8.0)];
        let mut c = BreakCursor::new(&items);
        c.partial_remainder = vec![cl("R", 8.0)];
        assert!(
            !c.is_at_start(),
            "a pending remainder means we are mid-stream"
        );
        assert!(!c.is_done());

        assert_eq!(
            c.peek_next_single_item()[0].as_cluster().unwrap().text(),
            "R",
            "the remainder is served first"
        );

        let drained = c.drain_remaining();
        assert_eq!(drained.len(), 3, "remainder + both queued items");
        assert_eq!(drained[0].as_cluster().unwrap().text(), "R");
        assert!(c.is_done());
    }

    #[test]
    fn break_cursor_is_done_is_false_while_a_remainder_is_pending() {
        let items: Vec<ShapedItem> = Vec::new();
        let mut c = BreakCursor::new(&items);
        c.partial_remainder = vec![cl("x", 8.0)];
        assert!(
            !c.is_done(),
            "the main list is exhausted but the remainder is not"
        );
        c.consume(1);
        assert!(c.is_done());
    }

    #[test]
    fn break_cursor_consume_spanning_remainder_and_main_list() {
        let items = vec![cl("a", 8.0), cl("b", 8.0), cl("c", 8.0)];
        let mut c = BreakCursor::new(&items);
        c.partial_remainder = vec![cl("R1", 8.0), cl("R2", 8.0)];
        // Eat both remainder items + one from the main list.
        c.consume(3);
        assert!(c.partial_remainder.is_empty());
        assert_eq!(c.next_item_index, 1);
        assert_eq!(
            c.peek_next_single_item()[0].as_cluster().unwrap().text(),
            "b"
        );
    }

    #[test]
    fn break_cursor_peek_next_unit_returns_a_whole_word_then_the_space() {
        let items = vec![
            cl("h", 8.0),
            cl("i", 8.0),
            cl(" ", 4.0),
            cl("y", 8.0),
            cl("o", 8.0),
        ];
        let mut c = BreakCursor::new(&items);
        let word = c.peek_next_unit();
        assert_eq!(word.len(), 2, "the word stops at the space");
        assert_eq!(word[0].as_cluster().unwrap().text(), "h");

        c.consume(word.len());
        let space = c.peek_next_unit();
        assert_eq!(
            space.len(),
            1,
            "a leading break opportunity is a unit on its own"
        );
        assert_eq!(space[0].as_cluster().unwrap().text(), " ");

        c.consume(1);
        assert_eq!(c.peek_next_unit().len(), 2, "the trailing word");
    }

    #[test]
    fn break_cursor_peek_next_unit_honours_break_all_and_keep_all() {
        let items = vec![cl("中", 16.0), cl("文", 16.0), cl("字", 16.0)];

        let normal = BreakCursor::new(&items);
        assert_eq!(
            normal.peek_next_unit().len(),
            1,
            "word-break:normal — each ideograph is its own unit"
        );

        let all = BreakCursor::with_word_break(&items, WordBreak::BreakAll);
        assert_eq!(
            all.peek_next_unit().len(),
            1,
            "break-all — one cluster per unit"
        );

        let keep = BreakCursor::with_word_break(&items, WordBreak::KeepAll);
        assert_eq!(
            keep.peek_next_unit().len(),
            3,
            "keep-all — the whole CJK run is unbreakable"
        );

        let latin = vec![cl("a", 8.0), cl("b", 8.0)];
        let latin_all = BreakCursor::with_word_break(&latin, WordBreak::BreakAll);
        assert_eq!(
            latin_all.peek_next_unit().len(),
            1,
            "break-all splits Latin too"
        );
    }

    #[test]
    fn break_cursor_peek_next_unit_glues_across_a_word_joiner() {
        // Control: without a joiner the unit ends at the space.
        let plain = vec![cl("a", 8.0), cl(" ", 4.0), cl("b", 8.0)];
        let control = BreakCursor::new(&plain);
        assert_eq!(
            control.peek_next_unit().len(),
            1,
            "the unit normally stops before the space"
        );

        // With a WORD JOINER (U+2060) in between, the break after it is suppressed,
        // so the space is pulled into the same unbreakable unit.
        let glued = vec![
            cl("a", 8.0),
            cl("\u{2060}", 0.0),
            cl(" ", 4.0),
            cl("b", 8.0),
        ];
        let c = BreakCursor::new(&glued);
        let unit = c.peek_next_unit();
        assert!(
            unit.len() > 1,
            "a word joiner must suppress the following break, got {} item(s)",
            unit.len()
        );
    }

    #[test]
    fn break_cursor_peek_next_single_item_prefers_the_remainder() {
        let items = vec![cl("a", 8.0)];
        let mut c = BreakCursor::new(&items);
        assert_eq!(
            c.peek_next_single_item()[0].as_cluster().unwrap().text(),
            "a"
        );
        c.partial_remainder = vec![cl("R", 8.0)];
        assert_eq!(
            c.peek_next_single_item()[0].as_cluster().unwrap().text(),
            "R"
        );
        assert_eq!(
            c.peek_next_single_item().len(),
            1,
            "peek must never return more than one item"
        );
    }

    // =====================================================================
    // constructor/getter: LoadedFonts
    // =====================================================================

    fn tf(hash: u64) -> TestFont {
        TestFont { hash }
    }

    #[test]
    fn loaded_fonts_new_is_empty_and_misses_every_lookup() {
        let lf: LoadedFonts<TestFont> = LoadedFonts::new();
        assert!(lf.is_empty());
        assert_eq!(lf.len(), 0);
        assert_eq!(lf.iter().count(), 0);
        assert!(lf.get(&FontId(0)).is_none());
        assert!(!lf.contains_key(&FontId(u128::MAX)));
        // Hash lookups at the numeric boundaries must miss, not panic.
        for h in [0_u64, 1, u64::MAX] {
            assert!(lf.get_by_hash(h).is_none());
            assert!(lf.get_font_id_by_hash(h).is_none());
            assert!(!lf.contains_hash(h));
        }
        // Default agrees with new().
        let d: LoadedFonts<TestFont> = LoadedFonts::default();
        assert!(d.is_empty());
    }

    #[test]
    fn loaded_fonts_insert_indexes_by_id_and_by_hash() {
        let mut lf: LoadedFonts<TestFont> = LoadedFonts::new();
        lf.insert(FontId(1), tf(0xDEAD));
        assert_eq!(lf.len(), 1);
        assert!(!lf.is_empty());
        assert!(lf.contains_key(&FontId(1)));
        assert!(lf.contains_hash(0xDEAD));
        assert_eq!(lf.get(&FontId(1)).map(TestFont::get_hash), Some(0xDEAD));
        assert_eq!(lf.get_by_hash(0xDEAD).map(TestFont::get_hash), Some(0xDEAD));
        assert_eq!(lf.get_font_id_by_hash(0xDEAD), Some(&FontId(1)));
        assert!(lf.get_by_hash(0).is_none());
    }

    #[test]
    fn loaded_fonts_zero_hash_is_a_valid_key_not_a_sentinel() {
        let mut lf: LoadedFonts<TestFont> = LoadedFonts::new();
        lf.insert(FontId(1), tf(0));
        assert!(lf.contains_hash(0), "hash 0 must be storable and findable");
        assert_eq!(lf.get_by_hash(0).map(TestFont::get_hash), Some(0));
    }

    #[test]
    fn loaded_fonts_two_ids_sharing_a_hash_keep_only_the_last_reverse_mapping() {
        let mut lf: LoadedFonts<TestFont> = LoadedFonts::new();
        lf.insert(FontId(1), tf(7));
        lf.insert(FontId(2), tf(7));
        assert_eq!(lf.len(), 2, "both fonts are stored by id");
        assert_eq!(
            lf.get_font_id_by_hash(7),
            Some(&FontId(2)),
            "the reverse index keeps only the LAST id for a colliding hash"
        );
    }

    #[test]
    fn loaded_fonts_replacing_a_font_id_leaves_a_stale_hash_mapping() {
        // FINDING (staleness, not a crash): `insert` never removes the OLD hash of a
        // replaced FontId, so the reverse index keeps pointing at that id forever.
        // A by-hash lookup for the evicted font therefore succeeds and returns the
        // WRONG font instead of None.
        let mut lf: LoadedFonts<TestFont> = LoadedFonts::new();
        lf.insert(FontId(1), tf(100));
        lf.insert(FontId(1), tf(200)); // same id, new hash
        assert_eq!(lf.len(), 1, "the font map correctly holds one entry");

        assert!(
            lf.contains_hash(100),
            "the old hash is still in the reverse index"
        );
        let stale = lf.get_by_hash(100).expect("stale mapping resolves");
        assert_eq!(
            stale.get_hash(),
            200,
            "looking up the OLD hash hands back the NEW font"
        );
    }

    #[test]
    fn loaded_fonts_from_iterator_matches_repeated_inserts() {
        let lf: LoadedFonts<TestFont> = vec![(FontId(1), tf(10)), (FontId(2), tf(20))]
            .into_iter()
            .collect();
        assert_eq!(lf.len(), 2);
        assert!(lf.contains_hash(10) && lf.contains_hash(20));
        assert_eq!(lf.iter().count(), 2);
    }

    // =====================================================================
    // constructor/other: FontManager + FontContext
    // =====================================================================

    fn manager() -> FontManager<TestFont> {
        FontManager::new(FcFontCache::default()).expect("FontManager::new must not fail")
    }

    #[test]
    fn font_manager_constructors_start_empty() {
        for m in [
            manager(),
            FontManager::from_shared(FcFontCache::default()).unwrap(),
            FontManager::from_arc_shared(
                FcFontCache::default(),
                Arc::new(Mutex::new(HashMap::new())),
            )
            .unwrap(),
        ] {
            assert!(m.get_font_chain_cache().is_empty());
            assert!(m.get_loaded_fonts().is_empty());
            assert!(m.get_loaded_font_ids().is_empty());
            assert!(m.registry.is_none());
            assert_eq!(m.last_resolved_font_stacks_sig, None);
            assert!(m.get_font_by_hash(0).is_none());
            assert!(m.get_embedded_font_by_hash(u64::MAX).is_none());
        }
    }

    #[test]
    fn font_manager_from_arc_shared_sees_writes_through_the_shared_pool() {
        let pool: Arc<Mutex<HashMap<FontId, TestFont>>> = Arc::new(Mutex::new(HashMap::new()));
        let a = FontManager::from_arc_shared(FcFontCache::default(), pool.clone()).unwrap();
        let b = FontManager::from_arc_shared(FcFontCache::default(), pool).unwrap();

        assert!(
            a.insert_font(FontId(1), tf(5)).is_none(),
            "no previous font"
        );
        assert_eq!(
            b.get_loaded_fonts().len(),
            1,
            "the second manager must observe the first's insert"
        );
        assert_eq!(b.get_font_by_hash(5).map(|f| f.get_hash()), Some(5));

        // shared_parsed_fonts hands back the same Arc.
        assert!(Arc::ptr_eq(
            &a.shared_parsed_fonts(),
            &b.shared_parsed_fonts()
        ));
    }

    /// `get_loaded_fonts` is one shared snapshot while the pool is unchanged
    /// (every inline formatting context asks; building the two maps per call was
    /// a fifth of a 300-contact list's layout), and a new one after ANY change -
    /// an insert, a replacement under the same id, a removal straight through
    /// the shared pool the way the rasterizer and other managers reach it.
    #[test]
    fn loaded_fonts_are_one_snapshot_until_the_pool_changes() {
        let m = manager();
        m.insert_font(FontId(1), tf(1));
        let first = m.get_loaded_fonts();
        assert!(Arc::ptr_eq(&first, &m.get_loaded_fonts()), "unchanged pool, same snapshot");

        m.insert_font(FontId(2), tf(2));
        let inserted = m.get_loaded_fonts();
        assert_eq!(inserted.len(), 2);
        assert!(!Arc::ptr_eq(&first, &inserted));

        m.insert_font(FontId(2), tf(3));
        let replaced = m.get_loaded_fonts();
        assert!(!Arc::ptr_eq(&inserted, &replaced), "a face replaced under its id");
        assert!(replaced.contains_hash(3) && !replaced.contains_hash(2));

        m.shared_parsed_fonts().lock().unwrap().remove(&FontId(1));
        let removed = m.get_loaded_fonts();
        assert_eq!(removed.len(), 1, "a removal through the shared pool is seen");

        let other = m.clone_shared();
        assert!(Arc::ptr_eq(&removed, &other.get_loaded_fonts()), "managers share the snapshot");
    }

    #[test]
    fn font_manager_insert_font_returns_the_replaced_font() {
        let m = manager();
        assert!(m.insert_font(FontId(1), tf(1)).is_none());
        let old = m
            .insert_font(FontId(1), tf(2))
            .expect("must return the old font");
        assert_eq!(old.get_hash(), 1);
        assert_eq!(m.get_loaded_fonts().len(), 1);
    }

    #[test]
    fn font_manager_insert_fonts_and_remove_font() {
        let m = manager();
        m.insert_fonts(vec![(FontId(1), tf(1)), (FontId(2), tf(2))]);
        assert_eq!(m.get_loaded_font_ids().len(), 2);

        assert_eq!(m.remove_font(&FontId(1)).map(|f| f.get_hash()), Some(1));
        assert!(
            m.remove_font(&FontId(1)).is_none(),
            "double-remove is a no-op"
        );
        assert!(
            m.remove_font(&FontId(u128::MAX)).is_none(),
            "removing an unknown id must not panic"
        );
        assert_eq!(m.get_loaded_fonts().len(), 1);

        // Inserting an empty iterator is a no-op.
        m.insert_fonts(Vec::new());
        assert_eq!(m.get_loaded_fonts().len(), 1);
    }

    #[test]
    fn font_manager_get_font_by_hash_scans_linearly_and_misses_cleanly() {
        let m = manager();
        m.insert_fonts(vec![(FontId(1), tf(11)), (FontId(2), tf(22))]);
        assert_eq!(m.get_font_by_hash(22).map(|f| f.get_hash()), Some(22));
        assert!(m.get_font_by_hash(33).is_none());
        assert!(m.get_font_by_hash(u64::MAX).is_none());
        assert!(m.get_font_by_hash(0).is_none());
    }

    #[test]
    fn font_manager_chain_cache_set_merge_and_signature() {
        let mut m = manager();
        assert!(m.get_font_chain_cache().is_empty());

        // set_font_chain_cache_with_sig records the signature...
        m.set_font_chain_cache_with_sig(HashMap::new(), Some(42));
        assert_eq!(m.last_resolved_font_stacks_sig, Some(42));

        // ...and the single-arg setter clears it again.
        m.set_font_chain_cache(HashMap::new());
        assert_eq!(
            m.last_resolved_font_stacks_sig, None,
            "a signature-less set must invalidate the recorded signature"
        );

        // merge on an empty cache is a no-op that does not panic.
        m.merge_font_chain_cache(HashMap::new());
        assert!(m.get_font_chain_cache().is_empty());
    }

    #[test]
    fn font_manager_garbage_collect_evicts_everything_not_in_the_keep_set() {
        let mut m = manager();
        m.insert_fonts(vec![
            (FontId(1), tf(1)),
            (FontId(2), tf(2)),
            (FontId(3), tf(3)),
        ]);

        let mut keep = HashSet::new();
        keep.insert(FontId(2));
        let evicted = m.garbage_collect_fonts(&keep, &HashSet::new());
        assert_eq!(evicted, 2);
        assert_eq!(m.get_loaded_font_ids(), keep);

        // GC-ing again evicts nothing (saturating_sub must not underflow).
        assert_eq!(m.garbage_collect_fonts(&keep, &HashSet::new()), 0);

        // An empty keep-set flushes the pool entirely.
        assert_eq!(m.garbage_collect_fonts(&HashSet::new(), &HashSet::new()), 1);
        assert!(m.get_loaded_fonts().is_empty());
        // ...and a GC on an already-empty pool is still 0, not a panic.
        assert_eq!(m.garbage_collect_fonts(&HashSet::new(), &HashSet::new()), 0);
    }

    #[test]
    fn font_manager_load_missing_for_chains_with_no_chains_loads_nothing() {
        use crate::solver3::getters::ResolvedFontChains;
        let m = manager();
        let empty = ResolvedFontChains {
            chains: HashMap::new(),
            ..Default::default()
        };
        let failed =
            m.load_missing_for_chains(&empty, |_bytes, _idx| -> Result<TestFont, LayoutError> {
                panic!("the loader must never be invoked when there is nothing to load")
            });
        assert!(failed.is_empty());
        assert!(m.get_loaded_fonts().is_empty());
    }

    #[test]
    fn font_context_from_fc_cache_starts_empty_and_converts_to_a_manager() {
        let ctx = FontContext::from_fc_cache(FcFontCache::default());
        assert!(ctx.font_chain_cache.is_empty());
        assert!(ctx.embedded_fonts.is_empty());
        assert!(ctx.font_hash_to_families.is_empty());
        assert!(ctx.registry.is_none());
        assert!(ctx.parsed_fonts.lock().unwrap().is_empty());

        // Warming an empty chain set must be a no-op (and must not hit the disk).
        ctx.load_fonts_for_chains();
        assert!(ctx.parsed_fonts.lock().unwrap().is_empty());

        let mgr = ctx.to_font_manager();
        assert!(mgr.get_font_chain_cache().is_empty());
        assert!(mgr.registry.is_none());
        assert_eq!(mgr.last_resolved_font_stacks_sig, None);
        assert!(
            Arc::ptr_eq(&mgr.parsed_fonts, &ctx.parsed_fonts),
            "the manager must share (not copy) the parsed-font pool"
        );
    }

    // =====================================================================
    // other: create_logical_items / bidi entry points
    // =====================================================================

    #[test]
    fn create_logical_items_on_empty_and_whitespace_only_content() {
        let mut dbg = None;
        assert!(create_logical_items(&[], &[], &mut dbg).is_empty());

        // An empty text run is skipped entirely.
        let empty_run = [text_content("", style())];
        assert!(create_logical_items(&empty_run, &[], &mut dbg).is_empty());

        // Whitespace-only text still produces items.
        let ws = [text_content("   \t\n", style())];
        assert!(!create_logical_items(&ws, &[], &mut dbg).is_empty());
    }

    #[test]
    fn create_logical_items_handles_multibyte_and_astral_text_without_panicking() {
        let mut dbg = None;
        for s in ["\u{1F600}", "é\u{0301}", "中文", "a\u{200B}b", "\u{FEFF}"] {
            let content = [text_content(s, style())];
            let items = create_logical_items(&content, &[], &mut dbg);
            assert!(!items.is_empty(), "{s:?} produced no logical items");
        }
    }

    #[test]
    fn create_logical_items_on_a_long_run_does_not_hang() {
        let mut dbg = None;
        let long = "a".repeat(100_000);
        let content = [text_content(&long, style())];
        let items = create_logical_items(&content, &[], &mut dbg);
        assert!(!items.is_empty());
    }

    #[test]
    fn create_logical_items_debug_messages_are_recorded_when_requested() {
        let mut dbg = Some(Vec::new());
        let content = [text_content("hi", style())];
        let _ = create_logical_items(&content, &[], &mut dbg);
        assert!(
            !dbg.expect("Some(..) in → Some(..) out").is_empty(),
            "the debug sink must be populated when it is Some"
        );
    }

    #[test]
    fn get_base_direction_from_logical_defaults_to_ltr_on_empty_input() {
        assert_eq!(get_base_direction_from_logical(&[]), BidiDirection::Ltr);

        let mut dbg = None;
        let ltr = create_logical_items(&[text_content("hello", style())], &[], &mut dbg);
        assert_eq!(get_base_direction_from_logical(&ltr), BidiDirection::Ltr);

        // A Hebrew run must be detected as RTL.
        let rtl = create_logical_items(&[text_content("\u{05D0}\u{05D1}", style())], &[], &mut dbg);
        assert_eq!(get_base_direction_from_logical(&rtl), BidiDirection::Rtl);
    }

    #[test]
    fn reorder_logical_items_on_empty_input_is_ok_and_empty() {
        let mut dbg = None;
        let out = reorder_logical_items(&[], BidiDirection::Ltr, UnicodeBidi::Normal, &mut dbg)
            .expect("reordering nothing must succeed");
        assert!(out.is_empty());
    }

    #[test]
    fn reorder_logical_items_preserves_content_for_pure_ltr_text() {
        let mut dbg = None;
        let logical = create_logical_items(&[text_content("abc", style())], &[], &mut dbg);
        let visual =
            reorder_logical_items(&logical, BidiDirection::Ltr, UnicodeBidi::Normal, &mut dbg)
                .expect("LTR reordering must succeed");
        let joined: String = visual.iter().map(|v| v.text.as_str()).collect();
        assert_eq!(joined, "abc", "pure LTR text must survive bidi unchanged");
        assert!(visual.iter().all(|v| !v.bidi_level.is_rtl()));
    }

    // =====================================================================
    // other: hyphenation stubs (feature-gated)
    // =====================================================================

    #[cfg(not(feature = "text_layout_hyphenation"))]
    #[test]
    fn stub_hyphenate_never_reports_a_break() {
        let s = Standard;
        assert!(s.hyphenate("").breaks.is_empty());
        assert!(s.hyphenate("hyphenation").breaks.is_empty());
        assert!(s.hyphenate(&"a".repeat(10_000)).breaks.is_empty());
        assert!(s.hyphenate("\u{1F600}\u{0301}").breaks.is_empty());
    }

    #[cfg(not(feature = "text_layout_hyphenation"))]
    #[test]
    fn stub_get_hyphenator_always_errors() {
        assert!(matches!(
            get_hyphenator(Language::EnglishUS),
            Err(LayoutError::HyphenationError(_))
        ));
    }

    #[cfg(feature = "text_layout_hyphenation")]
    #[test]
    fn get_hyphenator_loads_an_embedded_language_and_hyphenates() {
        let h = get_hyphenator(HyphenationLanguage::EnglishUS)
            .expect("en-US dictionaries are embedded");

        // Empty / single-char words have no interior break points.
        let empty = h.hyphenate("");
        assert!(empty.breaks.is_empty(), "an empty word must not panic");
        let one = h.hyphenate("a");
        assert!(one.breaks.is_empty());

        // Every reported break must be a valid INTERIOR char boundary.
        let word = "hyphenation";
        let opps = h.hyphenate(word);
        for &b in &opps.breaks {
            assert!(b > 0 && b < word.len(), "break {b} is outside {word:?}");
            assert!(word.is_char_boundary(b), "break {b} splits a char");
        }

        // Astral + combining input must not panic.
        let weird = h.hyphenate("\u{1F600}\u{0301}");
        assert!(weird.breaks.len() < 8, "no runaway break list");
    }

    // =====================================================================
    // FONT GC: the eviction-resurrection contract
    // =====================================================================

    /// THE LAW the azwriter textless-window bug broke: the GC's safety
    /// argument ("eviction is always safe — the next layout re-loads what it
    /// needs") is FALSE when a shaping cache serves cached glyphs without
    /// re-loading, so a face whose hash is still referenced by live shaped
    /// output must remain RESOLVABLE after the GC condemned it. Verified
    /// live: the exact hash the renderer reported as unresolvable
    /// (14253617078591575638) appeared in the GC's eviction trace.
    #[test]
    fn gc_condemned_face_still_resolves_by_hash_and_resurrects() {
        let mut m: FontManager<TestFont> =
            FontManager::new(FcFontCache::default()).expect("FontManager::new");
        let id = FontId(7);
        m.insert_font(id, tf(0xF00D));

        // GC with a keep-set that does NOT contain the face — the state the
        // azwriter run produced (UI chains resolved, fallback face outside).
        let keep_ids = std::collections::HashSet::new();
        let keep_hashes = std::collections::HashSet::new();
        let _ = m.garbage_collect_fonts(&keep_ids, &keep_hashes);

        // A display list / shaping cache still carries hash 0xF00D. It MUST
        // resolve — a dropped face here is user-visible text loss.
        let resurrected = m.get_font_by_hash(0xF00D);
        assert!(
            resurrected.is_some(),
            "a GC pass must not make a still-referenced font hash unresolvable"
        );

        // Resurrection is real: the face is back in the live pool, so a
        // SECOND resolution does not depend on the condemned window.
        let _ = m.garbage_collect_fonts(&[id].into_iter().collect(), &keep_hashes);
        assert!(
            m.get_font_by_hash(0xF00D).is_some(),
            "resurrected face is live again"
        );
    }

    /// The NEGATIVE CONTROL guarding the leak fix the GC exists for: a face
    /// whose hash nobody resolves for TWO consecutive GC generations is
    /// genuinely dropped. Without this, "keep everything forever" would also
    /// pass the law above.
    #[test]
    fn gc_drops_a_face_no_one_referenced_for_two_generations() {
        let mut m: FontManager<TestFont> =
            FontManager::new(FcFontCache::default()).expect("FontManager::new");
        m.insert_font(FontId(9), tf(0xDEAD));

        let keep_ids = std::collections::HashSet::new();
        let keep_hashes = std::collections::HashSet::new();
        // Three GC passes, never resolved in between: condemned -> aged -> gone.
        let _ = m.garbage_collect_fonts(&keep_ids, &keep_hashes);
        let _ = m.garbage_collect_fonts(&keep_ids, &keep_hashes);
        let _ = m.garbage_collect_fonts(&keep_ids, &keep_hashes);
        assert!(
            m.get_font_by_hash(0xDEAD).is_none(),
            "an unreferenced face must eventually be dropped — the GC exists because font-cycling \
             apps leaked every font they ever touched"
        );
    }
