#[cfg(test)]
#[allow(clippy::float_cmp)] // GPU cache values must round-trip bit-exactly, not "approximately"
mod autotest_generated {
    use azul_css::css::Css;

    use super::*;
    use crate::dom::Dom;

    /// A `StyledDom` with exactly one node (the body) and no CSS at all:
    /// compact cache present, `has_transform` unset, opacity == `OPACITY_SENTINEL`.
    fn plain_styled_dom() -> StyledDom {
        let mut dom = Dom::create_body();
        StyledDom::create(&mut dom, Css::empty())
    }

    /// body > div.<class>, styled by `css_src`.
    fn styled_dom_from_css(css_src: &str, class: &str) -> StyledDom {
        let css = azul_css::parser2::new_from_str(css_src).0;
        let mut dom = Dom::create_body()
            .with_children(vec![Dom::create_div().with_class(class.to_string().into())].into());
        StyledDom::create(&mut dom, css)
    }

    /// A matrix stuffed with every hostile f32 the transform math can produce.
    fn hostile_matrix() -> ComputedTransform3D {
        ComputedTransform3D::new(
            f32::NAN,
            f32::INFINITY,
            f32::NEG_INFINITY,
            f32::MIN,
            f32::MAX,
            0.0,
            -0.0,
            f32::EPSILON,
            1.0,
            2.0,
            3.0,
            4.0,
            5.0,
            6.0,
            7.0,
            8.0,
        )
    }

    fn scale_matrix(s: f32) -> ComputedTransform3D {
        ComputedTransform3D::new(
            s, 0.0, 0.0, 0.0, 0.0, s, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0,
        )
    }

    // ---------------------------------------------------------------------
    // constructors / neutral elements
    // ---------------------------------------------------------------------

    #[test]
    fn gpu_value_cache_empty_holds_no_keys_and_no_values() {
        let cache = GpuValueCache::empty();
        assert!(cache.transform_keys.is_empty());
        assert!(cache.current_transform_values.is_empty());
        assert!(cache.h_transform_keys.is_empty());
        assert!(cache.h_current_transform_values.is_empty());
        assert!(cache.css_transform_keys.is_empty());
        assert!(cache.css_current_transform_values.is_empty());
        assert!(cache.opacity_keys.is_empty());
        assert!(cache.current_opacity_values.is_empty());
        assert!(cache.scrollbar_v_opacity_keys.is_empty());
        assert!(cache.scrollbar_h_opacity_keys.is_empty());
        assert!(cache.scrollbar_v_opacity_values.is_empty());
        assert!(cache.scrollbar_h_opacity_values.is_empty());
    }

    #[test]
    fn gpu_event_changes_empty_equals_default_and_is_empty() {
        let changes = GpuEventChanges::empty();
        assert_eq!(changes, GpuEventChanges::default());
        assert!(changes.is_empty());
        assert_eq!(changes.transform_key_changes.len(), 0);
        assert_eq!(changes.opacity_key_changes.len(), 0);
        assert_eq!(changes.scrollbar_opacity_changes.len(), 0);
    }

    // ---------------------------------------------------------------------
    // GpuEventChanges::is_empty (predicate)
    // ---------------------------------------------------------------------

    #[test]
    fn is_empty_is_false_when_any_single_event_vec_is_populated() {
        let node = NodeId::ZERO;

        let mut only_transform = GpuEventChanges::empty();
        only_transform
            .transform_key_changes
            .push(GpuTransformKeyEvent::Removed(node, TransformKey::unique()));
        assert!(!only_transform.is_empty());

        let mut only_opacity = GpuEventChanges::empty();
        only_opacity
            .opacity_key_changes
            .push(GpuOpacityKeyEvent::Removed(node, OpacityKey::unique()));
        assert!(!only_opacity.is_empty());

        // scrollbar changes alone must also flip the predicate — is_empty() has to
        // check all three vecs, not just the two the CSS passes fill.
        let mut only_scrollbar = GpuEventChanges::empty();
        only_scrollbar
            .scrollbar_opacity_changes
            .push(GpuScrollbarOpacityEvent::VerticalRemoved(
                DomId::ROOT_ID,
                node,
                OpacityKey::unique(),
            ));
        assert!(!only_scrollbar.is_empty());
    }

    // ---------------------------------------------------------------------
    // GpuEventChanges::merge
    // ---------------------------------------------------------------------

    #[test]
    fn merge_moves_every_event_and_drains_the_source() {
        let node = NodeId::new(7);

        let mut target = GpuEventChanges::empty();
        target
            .transform_key_changes
            .push(GpuTransformKeyEvent::Added(
                node,
                TransformKey::unique(),
                ComputedTransform3D::IDENTITY,
            ));

        let mut source = GpuEventChanges::empty();
        source
            .transform_key_changes
            .push(GpuTransformKeyEvent::Removed(node, TransformKey::unique()));
        source.opacity_key_changes.push(GpuOpacityKeyEvent::Added(
            node,
            OpacityKey::unique(),
            0.25,
        ));
        source
            .scrollbar_opacity_changes
            .push(GpuScrollbarOpacityEvent::HorizontalAdded(
                DomId::ROOT_ID,
                node,
                OpacityKey::unique(),
                1.0,
            ));

        target.merge(&mut source);

        assert!(source.is_empty(), "merge must consume the source");
        assert_eq!(target.transform_key_changes.len(), 2);
        assert_eq!(target.opacity_key_changes.len(), 1);
        assert_eq!(target.scrollbar_opacity_changes.len(), 1);
        // append semantics: self's events keep their position, other's are pushed after.
        assert!(matches!(
            target.transform_key_changes[0],
            GpuTransformKeyEvent::Added(..)
        ));
        assert!(matches!(
            target.transform_key_changes[1],
            GpuTransformKeyEvent::Removed(..)
        ));
    }

    #[test]
    fn merge_with_an_empty_set_is_the_identity_in_both_directions() {
        let node = NodeId::new(1);
        let mut populated = GpuEventChanges::empty();
        populated
            .opacity_key_changes
            .push(GpuOpacityKeyEvent::Added(node, OpacityKey::unique(), 0.5));
        let snapshot = populated.clone();

        // x.merge(empty) == x
        let mut empty = GpuEventChanges::empty();
        populated.merge(&mut empty);
        assert_eq!(populated, snapshot);
        assert!(empty.is_empty());

        // empty.merge(x) == x
        let mut target = GpuEventChanges::empty();
        let mut source = snapshot.clone();
        target.merge(&mut source);
        assert_eq!(target, snapshot);
        assert!(source.is_empty());
    }

    #[test]
    fn merging_large_event_vectors_does_not_panic_and_is_idempotent_when_drained() {
        let mut target = GpuEventChanges::empty();
        let mut source = GpuEventChanges::empty();
        for i in 0..10_000usize {
            target.opacity_key_changes.push(GpuOpacityKeyEvent::Removed(
                NodeId::new(i),
                OpacityKey::unique(),
            ));
            source.opacity_key_changes.push(GpuOpacityKeyEvent::Removed(
                NodeId::new(i),
                OpacityKey::unique(),
            ));
        }

        target.merge(&mut source);
        assert_eq!(target.opacity_key_changes.len(), 20_000);
        assert!(source.is_empty());

        // merging an already-drained source a second time must be a no-op, not a duplicate
        target.merge(&mut source);
        assert_eq!(target.opacity_key_changes.len(), 20_000);
    }

    // ---------------------------------------------------------------------
    // apply_transform_events (private)
    // ---------------------------------------------------------------------

    #[test]
    fn apply_transform_events_on_an_empty_slice_is_a_noop() {
        let mut cache = GpuValueCache::empty();
        cache.apply_transform_events(&[]);
        assert!(cache.css_transform_keys.is_empty());
        assert!(cache.css_current_transform_values.is_empty());
    }

    #[test]
    fn apply_transform_events_keeps_keys_and_values_in_lockstep() {
        let node = NodeId::new(3);
        let key = TransformKey::unique();
        let mut cache = GpuValueCache::empty();

        cache.apply_transform_events(&[GpuTransformKeyEvent::Added(
            node,
            key,
            ComputedTransform3D::IDENTITY,
        )]);
        assert_eq!(cache.css_transform_keys.get(&node), Some(&key));
        assert_eq!(
            cache.css_current_transform_values.get(&node),
            Some(&ComputedTransform3D::IDENTITY)
        );

        // Changed must swap the value and *keep* the existing key (a new key here
        // would orphan the old one on the GPU).
        let scaled = scale_matrix(2.0);
        cache.apply_transform_events(&[GpuTransformKeyEvent::Changed(
            node,
            key,
            ComputedTransform3D::IDENTITY,
            scaled,
        )]);
        assert_eq!(cache.css_transform_keys.get(&node), Some(&key));
        assert_eq!(cache.css_current_transform_values.get(&node), Some(&scaled));

        cache.apply_transform_events(&[GpuTransformKeyEvent::Removed(node, key)]);
        assert!(cache.css_transform_keys.is_empty());
        assert!(cache.css_current_transform_values.is_empty());
    }

    #[test]
    fn removing_an_uncached_or_out_of_range_node_does_not_panic() {
        let mut cache = GpuValueCache::empty();
        // NodeId::new(usize::MAX) is never a valid DOM index — apply_* only hashes it,
        // so it must be tolerated rather than used as an array index.
        cache.apply_transform_events(&[
            GpuTransformKeyEvent::Removed(NodeId::new(usize::MAX), TransformKey::unique()),
            GpuTransformKeyEvent::Removed(NodeId::ZERO, TransformKey::unique()),
        ]);
        cache.apply_opacity_events(&[GpuOpacityKeyEvent::Removed(
            NodeId::new(usize::MAX),
            OpacityKey::unique(),
        )]);
        assert!(cache.css_transform_keys.is_empty());
        assert!(cache.css_current_transform_values.is_empty());
        assert!(cache.opacity_keys.is_empty());
        assert!(cache.current_opacity_values.is_empty());
    }

    #[test]
    fn transform_events_are_applied_in_slice_order_within_one_batch() {
        let node = NodeId::new(2);
        let first = TransformKey::unique();
        let second = TransformKey::unique();

        // Added -> Removed inside one batch must end up removed.
        let mut cache = GpuValueCache::empty();
        cache.apply_transform_events(&[
            GpuTransformKeyEvent::Added(node, first, ComputedTransform3D::IDENTITY),
            GpuTransformKeyEvent::Removed(node, first),
        ]);
        assert!(cache.css_transform_keys.is_empty());
        assert!(cache.css_current_transform_values.is_empty());

        // Removed -> Added inside one batch must end up added.
        let mut cache = GpuValueCache::empty();
        cache.apply_transform_events(&[
            GpuTransformKeyEvent::Removed(node, first),
            GpuTransformKeyEvent::Added(node, second, ComputedTransform3D::IDENTITY),
        ]);
        assert_eq!(cache.css_transform_keys.get(&node), Some(&second));
    }

    #[test]
    fn two_added_events_for_one_node_keep_only_the_last_key() {
        let node = NodeId::ZERO;
        let first = TransformKey::unique();
        let second = TransformKey::unique();
        assert_ne!(
            first, second,
            "TransformKey::unique() must never hand out the same id twice"
        );

        let mut cache = GpuValueCache::empty();
        cache.apply_transform_events(&[
            GpuTransformKeyEvent::Added(node, first, ComputedTransform3D::IDENTITY),
            GpuTransformKeyEvent::Added(node, second, scale_matrix(3.0)),
        ]);
        assert_eq!(cache.css_transform_keys.len(), 1);
        assert_eq!(cache.css_transform_keys.get(&node), Some(&second));
        assert_eq!(
            cache.css_current_transform_values.get(&node),
            Some(&scale_matrix(3.0))
        );
    }

    #[test]
    fn a_changed_event_for_an_uncached_node_inserts_a_value_but_no_key() {
        // compute_transform_events can never emit this (it `?`s on an existing key),
        // but apply_transform_events takes an arbitrary slice and must not panic.
        let node = NodeId::new(5);
        let mut cache = GpuValueCache::empty();
        cache.apply_transform_events(&[GpuTransformKeyEvent::Changed(
            node,
            TransformKey::unique(),
            ComputedTransform3D::IDENTITY,
            scale_matrix(2.0),
        )]);
        assert_eq!(
            cache.css_current_transform_values.get(&node),
            Some(&scale_matrix(2.0))
        );
        assert!(
            cache.css_transform_keys.is_empty(),
            "Changed only writes the value map; the key map stays untouched"
        );
    }

    #[test]
    fn non_finite_matrices_are_stored_verbatim_and_never_compare_equal() {
        let node = NodeId::new(1);
        let mut cache = GpuValueCache::empty();
        cache.apply_transform_events(&[GpuTransformKeyEvent::Added(
            node,
            TransformKey::unique(),
            hostile_matrix(),
        )]);

        let stored = cache
            .css_current_transform_values
            .get(&node)
            .copied()
            .expect("the matrix must be cached even when it is full of NaN/Inf");
        assert!(stored.m[0][0].is_nan());
        assert!(stored.m[0][1].is_infinite() && stored.m[0][1].is_sign_positive());
        assert!(stored.m[0][2].is_infinite() && stored.m[0][2].is_sign_negative());
        assert_eq!(stored.m[0][3], f32::MIN);
        assert_eq!(stored.m[1][0], f32::MAX);

        // Consequence worth pinning: a NaN matrix is not PartialEq-equal to itself, so
        // no caller may use `old == new` to suppress a redundant GPU update.
        let a = hostile_matrix();
        let b = hostile_matrix();
        assert_ne!(a, b);
    }

    // ---------------------------------------------------------------------
    // apply_opacity_events (private)
    // ---------------------------------------------------------------------

    #[test]
    fn apply_opacity_events_add_change_remove_round_trip() {
        let node = NodeId::new(4);
        let key = OpacityKey::unique();
        let mut cache = GpuValueCache::empty();

        cache.apply_opacity_events(&[GpuOpacityKeyEvent::Added(node, key, 0.25)]);
        assert_eq!(cache.opacity_keys.get(&node), Some(&key));
        assert_eq!(cache.current_opacity_values.get(&node), Some(&0.25));

        cache.apply_opacity_events(&[GpuOpacityKeyEvent::Changed(node, key, 0.25, 0.75)]);
        assert_eq!(cache.opacity_keys.get(&node), Some(&key));
        assert_eq!(cache.current_opacity_values.get(&node), Some(&0.75));

        cache.apply_opacity_events(&[GpuOpacityKeyEvent::Removed(node, key)]);
        assert!(cache.opacity_keys.is_empty());
        assert!(cache.current_opacity_values.is_empty());

        // a second Removed for the same node is a no-op, not a panic
        cache.apply_opacity_events(&[GpuOpacityKeyEvent::Removed(node, key)]);
        assert!(cache.opacity_keys.is_empty());
    }

    #[test]
    fn apply_opacity_events_stores_out_of_range_values_verbatim() {
        // The cache does no clamping of its own — pin that, so a future "helpful"
        // clamp shows up as a test change rather than a silent behaviour change.
        let cases = [
            f32::NAN,
            f32::INFINITY,
            f32::NEG_INFINITY,
            -1.0,
            2.0,
            1e30,
            -0.0,
        ];
        let mut cache = GpuValueCache::empty();
        for (i, value) in cases.iter().enumerate() {
            cache.apply_opacity_events(&[GpuOpacityKeyEvent::Added(
                NodeId::new(i),
                OpacityKey::unique(),
                *value,
            )]);
        }

        assert_eq!(cache.current_opacity_values.len(), cases.len());
        assert_eq!(cache.opacity_keys.len(), cache.current_opacity_values.len());
        assert!(cache.current_opacity_values[&NodeId::new(0)].is_nan());
        assert!(cache.current_opacity_values[&NodeId::new(1)].is_infinite());
        assert_eq!(cache.current_opacity_values[&NodeId::new(3)], -1.0);
        assert_eq!(cache.current_opacity_values[&NodeId::new(4)], 2.0);
        assert_eq!(cache.current_opacity_values[&NodeId::new(5)], 1e30);
    }

    // ---------------------------------------------------------------------
    // init_simd_features (private)
    // ---------------------------------------------------------------------

    #[test]
    fn init_simd_features_is_idempotent() {
        GpuValueCache::init_simd_features();
        GpuValueCache::init_simd_features();

        #[cfg(target_arch = "x86_64")]
        {
            assert!(
                INITIALIZED.load(AtomicOrdering::SeqCst),
                "the one-time init flag must be set after the first call"
            );
            let sse = USE_SSE.load(AtomicOrdering::SeqCst);
            let avx = USE_AVX.load(AtomicOrdering::SeqCst);
            GpuValueCache::init_simd_features();
            assert_eq!(sse, USE_SSE.load(AtomicOrdering::SeqCst));
            assert_eq!(avx, USE_AVX.load(AtomicOrdering::SeqCst));
        }
    }

    // ---------------------------------------------------------------------
    // synchronize / compute_* (against a real StyledDom)
    // ---------------------------------------------------------------------

    #[test]
    fn synchronize_on_a_transform_and_opacity_free_dom_emits_nothing() {
        let styled = plain_styled_dom();
        let mut cache = GpuValueCache::empty();

        let changes = cache.synchronize(&styled);

        assert!(changes.is_empty());
        assert!(cache.css_transform_keys.is_empty());
        assert!(cache.opacity_keys.is_empty());
    }

    #[test]
    fn synchronize_never_fills_scrollbar_opacity_changes() {
        // Documented contract: scrollbar opacity is filled by a *separate* pass, so
        // synchronize() must leave that vec empty (and not touch the scrollbar maps).
        let styled = plain_styled_dom();
        let mut cache = GpuValueCache::empty();
        cache
            .scrollbar_v_opacity_keys
            .insert((DomId::ROOT_ID, NodeId::ZERO), OpacityKey::unique());
        cache
            .scrollbar_v_opacity_values
            .insert((DomId::ROOT_ID, NodeId::ZERO), 0.5);

        let changes = cache.synchronize(&styled);

        assert!(changes.scrollbar_opacity_changes.is_empty());
        assert_eq!(cache.scrollbar_v_opacity_keys.len(), 1);
        assert_eq!(cache.scrollbar_v_opacity_values.len(), 1);
    }

    #[test]
    fn opacity_round_trips_from_css_through_the_compact_cache_quantizer() {
        // compact.rs encodes opacity as `(o * 254.0).round() as u8`; gpu.rs decodes it
        // as `raw / 254.0`. Every value must survive that round-trip within one step,
        // and must never leave [0, 1] (which WebRender would reject).
        const STEP: f32 = 1.0 / 254.0;
        for (css_value, expected) in [
            ("0", 0.0f32),
            ("0.25", 0.25),
            ("0.5", 0.5),
            ("1", 1.0),
            ("50%", 0.5),
            ("100%", 1.0),
        ] {
            let styled = styled_dom_from_css(&format!(".fade {{ opacity: {css_value}; }}"), "fade");
            let mut cache = GpuValueCache::empty();

            let changes = cache.synchronize(&styled);

            let decoded = changes
                .opacity_key_changes
                .iter()
                .find_map(|e| match e {
                    GpuOpacityKeyEvent::Added(_, _, v) => Some(*v),
                    _ => None,
                })
                .unwrap_or_else(|| panic!("`opacity: {css_value}` produced no Added event"));

            assert!(
                (decoded - expected).abs() <= STEP,
                "`opacity: {css_value}` decoded as {decoded}, expected ~{expected}"
            );
            assert!(
                (0.0..=1.0).contains(&decoded),
                "`opacity: {css_value}` decoded to {decoded}, outside [0, 1]"
            );
        }
    }

    #[test]
    fn re_synchronizing_an_unchanged_dom_never_mints_a_second_key() {
        let styled = styled_dom_from_css(".fade { opacity: 0.5; }", "fade");
        let mut cache = GpuValueCache::empty();

        let first = cache.synchronize(&styled);
        let added_first = first
            .opacity_key_changes
            .iter()
            .filter(|e| matches!(e, GpuOpacityKeyEvent::Added(..)))
            .count();
        assert_eq!(
            added_first, 1,
            "the .fade div must get exactly one opacity key"
        );
        let keys_after_first = cache.opacity_keys.clone();

        let second = cache.synchronize(&styled);

        assert!(
            !second
                .opacity_key_changes
                .iter()
                .any(|e| matches!(e, GpuOpacityKeyEvent::Added(..))),
            "re-syncing an unchanged DOM must not allocate a new OpacityKey (GPU key leak)"
        );
        assert_eq!(
            cache.opacity_keys, keys_after_first,
            "the OpacityKey of a node must be stable across syncs"
        );
    }

    #[test]
    fn synchronize_evicts_cached_keys_for_nodes_that_no_longer_exist() {
        // Structural shrink: the DOM got smaller, so cached keys for now-missing nodes
        // are never visited by the per-node loop and would leak on the GPU.
        let styled = plain_styled_dom();
        assert_eq!(styled.node_data.len(), 1);

        let ghost_a = NodeId::new(9_999);
        let ghost_b = NodeId::new(usize::MAX); // must be "out of range", not an overflow
        let mut cache = GpuValueCache::empty();
        for ghost in [ghost_a, ghost_b] {
            cache
                .css_transform_keys
                .insert(ghost, TransformKey::unique());
            cache
                .css_current_transform_values
                .insert(ghost, ComputedTransform3D::IDENTITY);
            cache.opacity_keys.insert(ghost, OpacityKey::unique());
            cache.current_opacity_values.insert(ghost, 0.5);
        }

        let changes = cache.synchronize(&styled);

        assert_eq!(changes.transform_key_changes.len(), 2);
        assert_eq!(changes.opacity_key_changes.len(), 2);
        assert!(changes
            .transform_key_changes
            .iter()
            .all(|e| matches!(e, GpuTransformKeyEvent::Removed(..))));
        assert!(changes
            .opacity_key_changes
            .iter()
            .all(|e| matches!(e, GpuOpacityKeyEvent::Removed(..))));
        assert!(cache.css_transform_keys.is_empty());
        assert!(cache.css_current_transform_values.is_empty());
        assert!(cache.opacity_keys.is_empty());
        assert!(cache.current_opacity_values.is_empty());
    }

    #[test]
    fn a_cached_opacity_is_evicted_when_the_node_loses_its_opacity() {
        // Node still exists, but the DOM no longer sets `opacity` on it: the
        // (Some(old), None) arm must fire a Removed so the OpacityKey is freed.
        let styled = plain_styled_dom();
        let node = NodeId::ZERO;
        let key = OpacityKey::unique();

        let mut cache = GpuValueCache::empty();
        cache.opacity_keys.insert(node, key);
        cache.current_opacity_values.insert(node, 0.5);

        let changes = cache.synchronize(&styled);

        assert_eq!(
            changes.opacity_key_changes,
            vec![GpuOpacityKeyEvent::Removed(node, key)]
        );
        assert!(cache.opacity_keys.is_empty());
        assert!(cache.current_opacity_values.is_empty());
    }

    #[test]
    fn a_cached_transform_is_evicted_when_the_node_loses_its_transform() {
        // Same scenario as the opacity test above, for transforms: the node is still in
        // the DOM but no longer carries a `transform` property (e.g. its class was
        // dropped between frames). The (Some(old), None) arm of compute_transform_events
        // must fire a Removed — otherwise the TransformKey leaks on the GPU and the
        // cache keeps serving a stale matrix forever.
        let styled = plain_styled_dom();
        let node = NodeId::ZERO;
        let key = TransformKey::unique();

        let mut cache = GpuValueCache::empty();
        cache.css_transform_keys.insert(node, key);
        cache
            .css_current_transform_values
            .insert(node, ComputedTransform3D::IDENTITY);

        let changes = cache.synchronize(&styled);

        assert_eq!(
            changes.transform_key_changes,
            vec![GpuTransformKeyEvent::Removed(node, key)]
        );
        assert!(
            cache.css_transform_keys.is_empty(),
            "a stale CSS transform key was not evicted"
        );
        assert!(
            cache.css_current_transform_values.is_empty(),
            "a stale CSS transform value was not evicted"
        );
    }

    #[test]
    fn synchronize_survives_malformed_and_unicode_css() {
        for css_src in [
            "",
            ".fade { opacity: ; }",
            ".fade { opacity: 🦀; }",
            ".fade { opacity: -1; }",
            ".fade { opacity: 99999999999; }",
            ".fade { opacity: NaN; }",
            ".fade { transform: }",
            ".fade { transform: rotate(NaN); }",
            ".fade { transform: rotate(1e400deg); }",
            ".fade { transform: rotate(); }",
        ] {
            let styled = styled_dom_from_css(css_src, "fade");
            let mut cache = GpuValueCache::empty();

            let _ = cache.synchronize(&styled);

            // Whatever the parser salvaged, the cache invariants must survive it.
            assert_eq!(
                cache.css_transform_keys.len(),
                cache.css_current_transform_values.len(),
                "transform key/value maps drifted apart (css: {css_src:?})"
            );
            assert_eq!(
                cache.opacity_keys.len(),
                cache.current_opacity_values.len(),
                "opacity key/value maps drifted apart (css: {css_src:?})"
            );
            for value in cache.current_opacity_values.values() {
                assert!(
                    !value.is_nan(),
                    "a NaN opacity reached the GPU cache (css: {css_src:?})"
                );
                assert!(
                    (0.0..=1.0).contains(value),
                    "an out-of-range opacity ({value}) reached the GPU cache (css: {css_src:?})"
                );
            }
        }
    }

    #[test]
    fn synchronize_on_a_500_node_dom_keeps_every_key_in_range() {
        let css = azul_css::parser2::new_from_str(
            ".t { transform: rotate(45deg); } .o { opacity: 0.25; }",
        )
        .0;
        let children = (0..500usize)
            .map(|i| {
                let class = if i % 2 == 0 { "t" } else { "o" };
                Dom::create_div().with_class(class.to_string().into())
            })
            .collect::<Vec<Dom>>();
        let mut dom = Dom::create_body().with_children(children.into());
        let styled = StyledDom::create(&mut dom, css);

        let mut cache = GpuValueCache::empty();
        let changes = cache.synchronize(&styled);

        // On a fresh cache every event has to be an Added, so the event count and the
        // resulting key count must agree exactly.
        assert_eq!(
            changes.transform_key_changes.len(),
            cache.css_transform_keys.len()
        );
        assert_eq!(changes.opacity_key_changes.len(), cache.opacity_keys.len());
        assert_eq!(
            cache.css_transform_keys.len(),
            cache.css_current_transform_values.len()
        );
        assert_eq!(cache.opacity_keys.len(), cache.current_opacity_values.len());

        // No cached key may point outside the DOM.
        let node_count = styled.node_data.len();
        assert!(cache
            .css_transform_keys
            .keys()
            .all(|n| n.index() < node_count));
        assert!(cache.opacity_keys.keys().all(|n| n.index() < node_count));
    }
}
