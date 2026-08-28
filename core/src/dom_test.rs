#[cfg(test)]
mod audit_tests {
    use super::*;

    #[test]
    fn node_count_matches_recompute() {
        // root + [A(+grandchild), B] = 3 descendants, node_count 4.
        let dom = Dom::create_div()
            .with_child(Dom::create_div().with_child(Dom::create_div()))
            .with_child(Dom::create_div());
        assert_eq!(
            dom.estimated_total_children,
            dom.recompute_estimated_total_children()
        );
        assert_eq!(dom.estimated_total_children, 3);
        assert_eq!(dom.node_count(), 4);
    }

    #[test]
    fn single_node_dom_node_count() {
        let dom = Dom::create_div();
        assert_eq!(dom.estimated_total_children, 0);
        assert_eq!(dom.node_count(), 1);
        assert_eq!(dom.recompute_estimated_total_children(), 0);
    }

    #[test]
    fn fixup_repairs_desynced_estimate() {
        let mut dom = Dom::create_div().with_child(Dom::create_div());
        // Corrupt the public cached field directly.
        dom.estimated_total_children = 999;
        let repaired = dom.fixup_children_estimated();
        assert_eq!(repaired, 1);
        assert_eq!(
            dom.estimated_total_children,
            dom.recompute_estimated_total_children()
        );
    }

    // The debug_assert only fires with debug_assertions enabled.
    #[cfg(debug_assertions)]
    #[test]
    #[should_panic(expected = "desynced")]
    fn add_child_with_stale_estimate_panics_in_debug() {
        let mut child = Dom::create_div().with_child(Dom::create_div());
        child.estimated_total_children = 0; // corrupt: should be 1
        let mut parent = Dom::create_div();
        parent.add_child(child);
    }

    // NodeData carries a manual `unsafe impl Send`. This is a compile-time
    // assertion that the marker holds (fails to build if a non-Send field is
    // ever added), documenting the invariant the unsafe impl relies on.
    #[test]
    fn node_data_is_send() {
        fn assert_send<T: Send>() {}
        assert_send::<NodeData>();
    }

    // Exercises the `core::ptr::write` unsafe path in `copy_special_moving_complex`:
    // the boxed Text `node_type` must be MOVED bitwise into the copy (box pointer
    // preserved, string intact), `self.node_type` must be left as `Div`, and the
    // moved-out `style`/`extra` must land on the copy. Small heap-only tree, so
    // Miri can validate the raw write / box ownership transfer for UB.
    #[test]
    fn copy_special_moving_complex_moves_text_node_type() {
        let mut nd = NodeData::create_text_do_not_use_without_block_level_wrapper("hello")
            .with_css("color: red;");
        assert!(!nd.style.rules.is_empty(), "precondition: style set");

        let copy = nd.copy_special_moving_complex();

        // The Text box was transferred to the copy with its string intact.
        match copy.get_node_type() {
            NodeType::Text(s) => assert_eq!(s.as_ref().as_str(), "hello"),
            other => panic!("expected Text node_type on copy, got {other:?}"),
        }
        // The source's node_type was replaced with the heap-free Div placeholder.
        assert!(matches!(nd.get_node_type(), NodeType::Div));
        // `style` was moved out of `self` onto the copy.
        assert!(nd.style.rules.is_empty());
        assert!(!copy.style.rules.is_empty());
    }

    // A non-boxed (Div) node_type must also survive the ptr::write path unchanged.
    #[test]
    fn copy_special_moving_complex_moves_div_node_type() {
        let mut nd = NodeData::create_div();
        let copy = nd.copy_special_moving_complex();
        assert!(matches!(copy.get_node_type(), NodeType::Div));
        assert!(matches!(nd.get_node_type(), NodeType::Div));
    }
}

#[cfg(test)]
#[allow(clippy::cast_possible_wrap, clippy::too_many_lines)]
mod autotest_generated {
    use super::*;

    // ---------------------------------------------------------------------
    // upsert_inline_css_property
    // ---------------------------------------------------------------------

    /// The runtime-patch upsert must REPLACE the unconditional declaration of
    /// the same type, KEEP every other inline property (the content
    /// chokepoint used to wipe the whole inline style, so a patched panel
    /// lost its `position: absolute`), KEEP conditional declarations, and
    /// stay bounded under repeated toggles.
    #[test]
    fn upsert_inline_css_property_replaces_only_the_unconditional_same_type() {
        use azul_css::dynamic_selector::{
            CssPropertyWithConditions, DynamicSelector, PseudoStateType,
        };
        use azul_css::props::layout::display::LayoutDisplay;
        use azul_css::props::layout::position::LayoutPosition;
        use azul_css::props::property::{CssProperty, CssPropertyType};

        let mut node = NodeData::create_div();
        node.set_css_props(
            vec![
                CssPropertyWithConditions::simple(CssProperty::const_position(
                    LayoutPosition::Absolute,
                )),
                CssPropertyWithConditions::simple(CssProperty::const_display(LayoutDisplay::None)),
                // A conditional (hover) display declaration must survive.
                CssPropertyWithConditions {
                    property: CssProperty::const_display(LayoutDisplay::Block),
                    apply_if: vec![DynamicSelector::PseudoState(PseudoStateType::Hover)].into(),
                },
            ]
            .into(),
        );

        node.upsert_inline_css_property(CssProperty::const_display(LayoutDisplay::Flex));

        let collect = |node: &NodeData| -> Vec<(CssProperty, bool)> {
            node.style
                .iter_inline_properties()
                .map(|(p, conds)| (p.clone(), conds.as_ref().is_empty()))
                .collect()
        };

        let props = collect(&node);
        let unconditional_displays: Vec<&CssProperty> = props
            .iter()
            .filter(|(p, uncond)| *uncond && p.get_type() == CssPropertyType::Display)
            .map(|(p, _)| p)
            .collect();
        assert_eq!(
            unconditional_displays,
            vec![&CssProperty::const_display(LayoutDisplay::Flex)],
            "exactly ONE unconditional display remains: the patched value"
        );
        assert!(
            props.iter().any(|(p, uncond)| *uncond
                && *p == CssProperty::const_position(LayoutPosition::Absolute)),
            "unrelated inline properties must survive the patch"
        );
        assert!(
            props
                .iter()
                .any(|(p, uncond)| !*uncond
                    && *p == CssProperty::const_display(LayoutDisplay::Block)),
            "conditional (hover) declarations must survive the patch"
        );

        // Toggle many times: the style must not grow without bound.
        let len_before = node.style.rules.as_ref().len();
        for i in 0..20 {
            let v = if i % 2 == 0 {
                LayoutDisplay::None
            } else {
                LayoutDisplay::Flex
            };
            node.upsert_inline_css_property(CssProperty::const_display(v));
        }
        assert_eq!(
            node.style.rules.as_ref().len(),
            len_before,
            "repeated upserts of the same type must not grow the inline style"
        );
    }

    // ---------------------------------------------------------------------
    // helpers
    // ---------------------------------------------------------------------

    fn hash_of<T: Hash>(t: &T) -> u64 {
        let mut h = crate::hash::DefaultHasher::new();
        t.hash(&mut h);
        h.finish()
    }

    extern "C" fn merge_cb_a(new_data: RefAny, _old: RefAny) -> RefAny {
        new_data
    }

    extern "C" fn merge_cb_b(_new: RefAny, old_data: RefAny) -> RefAny {
        old_data
    }

    /// A `VirtualViewCallbackType`-shaped stub. Never invoked — the tests only need
    /// a well-typed callback to hang off a `NodeType::VirtualView` node.
    extern "C" fn virtual_view_cb(
        _data: RefAny,
        _info: crate::callbacks::VirtualViewCallbackInfo,
    ) -> crate::callbacks::VirtualViewReturn {
        unreachable!("virtual view callback is never invoked by these tests")
    }

    fn virtual_view_callback() -> VirtualViewCallback {
        VirtualViewCallback {
            cb: virtual_view_cb,
            ctx: OptionRefAny::None,
        }
    }

    /// A ~100k-char string with multi-byte codepoints, for "huge input" cases.
    fn huge_unicode_string() -> String {
        "ä🎉本".repeat(25_000)
    }

    /// Every `AttributeType` variant, so invariant sweeps can't silently miss one.
    fn all_attribute_variants() -> Vec<AttributeType> {
        let nv = || AttributeNameValue {
            attr_name: "data-x".into(),
            value: "v".into(),
        };
        vec![
            AttributeType::Id("i".into()),
            AttributeType::Class("c".into()),
            AttributeType::AriaLabel("l".into()),
            AttributeType::AriaLabelledBy("lb".into()),
            AttributeType::AriaDescribedBy("db".into()),
            AttributeType::AriaRole("r".into()),
            AttributeType::AriaState(nv()),
            AttributeType::AriaProperty(nv()),
            AttributeType::Href("h".into()),
            AttributeType::Rel("rel".into()),
            AttributeType::Target("t".into()),
            AttributeType::Src("s".into()),
            AttributeType::Alt("a".into()),
            AttributeType::Title("ti".into()),
            AttributeType::Name("n".into()),
            AttributeType::Value("v".into()),
            AttributeType::InputType("text".into()),
            AttributeType::Placeholder("p".into()),
            AttributeType::Required,
            AttributeType::Disabled,
            AttributeType::Readonly,
            AttributeType::CheckedTrue,
            AttributeType::CheckedFalse,
            AttributeType::Selected,
            AttributeType::Max("10".into()),
            AttributeType::Min("0".into()),
            AttributeType::Step("1".into()),
            AttributeType::Pattern(".*".into()),
            AttributeType::MinLength(i32::MIN),
            AttributeType::MaxLength(i32::MAX),
            AttributeType::Autocomplete("off".into()),
            AttributeType::Scope("row".into()),
            AttributeType::ColSpan(-1),
            AttributeType::RowSpan(0),
            AttributeType::TabIndex(i32::MIN),
            AttributeType::Focusable,
            AttributeType::Lang("en".into()),
            AttributeType::Dir("rtl".into()),
            AttributeType::ContentEditable(true),
            AttributeType::Draggable(false),
            AttributeType::Hidden,
            AttributeType::Data(nv()),
            AttributeType::Custom(nv()),
        ]
    }

    /// A spread of `NodeType`s, including every payload-carrying variant.
    fn representative_node_types() -> Vec<NodeType> {
        vec![
            NodeType::Html,
            NodeType::Body,
            NodeType::Div,
            NodeType::Br,
            NodeType::Button,
            NodeType::Input,
            NodeType::TextArea,
            NodeType::Select,
            NodeType::A,
            NodeType::H1,
            NodeType::H6,
            NodeType::Table,
            NodeType::Td,
            NodeType::Svg,
            NodeType::SvgPath,
            NodeType::SvgText("svg text".into()),
            NodeType::SvgImage(ImageRef::null_image(
                1,
                1,
                crate::resources::RawImageFormat::R8,
                Vec::new(),
            )),
            NodeType::Before,
            NodeType::After,
            NodeType::Marker,
            NodeType::Placeholder,
            NodeType::Text(BoxOrStatic::heap(AzString::from("hello"))),
            NodeType::Image(BoxOrStatic::heap(ImageRef::null_image(
                2,
                2,
                crate::resources::RawImageFormat::RGBA8,
                Vec::new(),
            ))),
            NodeType::VirtualView,
            NodeType::Icon(BoxOrStatic::heap(AzString::from("home"))),
            NodeType::GeolocationProbe(crate::geolocation::GeolocationProbeConfig::default()),
        ]
    }

    // =====================================================================
    // NodeFlags — bit-packing round-trips, boundaries, field independence
    // =====================================================================

    #[test]
    fn node_flags_new_is_empty_and_matches_default() {
        let f = NodeFlags::new();
        assert_eq!(f.inner, 0);
        assert_eq!(f, NodeFlags::default());
        assert!(!f.is_contenteditable());
        assert!(!f.is_anonymous());
        assert_eq!(f.get_tab_index(), None);
    }

    #[test]
    fn node_flags_tab_index_round_trips_for_all_variants() {
        for ti in [
            None,
            Some(TabIndex::Auto),
            Some(TabIndex::NoKeyboardFocus),
            Some(TabIndex::OverrideInParent(0)),
            Some(TabIndex::OverrideInParent(1)),
            Some(TabIndex::OverrideInParent(1_000)),
        ] {
            let mut f = NodeFlags::new();
            f.set_tab_index(ti);
            assert_eq!(f.get_tab_index(), ti, "round-trip failed for {ti:?}");
        }
    }

    #[test]
    fn node_flags_tab_index_round_trips_at_the_28_bit_boundary() {
        // The value field is bits [27:0], so 2^28 - 1 is the largest exactly
        // representable OverrideInParent value.
        const MAX_EXACT: u32 = (1 << 28) - 1;
        let mut f = NodeFlags::new();
        f.set_tab_index(Some(TabIndex::OverrideInParent(MAX_EXACT)));
        assert_eq!(
            f.get_tab_index(),
            Some(TabIndex::OverrideInParent(MAX_EXACT))
        );
    }

    #[test]
    fn node_flags_tab_index_above_28_bits_truncates_without_corrupting_other_flags() {
        // AUDIT: `set_tab_index` masks the value with TAB_VALUE_MASK ((1 << 28) - 1),
        // so any OverrideInParent >= 2^28 is SILENTLY TRUNCATED rather than rejected
        // or saturated. That is lossy, but the safety-critical property is that the
        // overflowing bits must not bleed into the anonymous / contenteditable /
        // tab-variant bits. Pin both facts.
        const OVERFLOW: u32 = 1 << 28;
        let mut f = NodeFlags::new();
        f.set_tab_index(Some(TabIndex::OverrideInParent(OVERFLOW)));
        assert_eq!(
            f.get_tab_index(),
            Some(TabIndex::OverrideInParent(0)),
            "2^28 truncates to 0 (documented lossiness)"
        );
        assert!(!f.is_anonymous(), "overflow bit must not set ANONYMOUS");
        assert!(!f.is_contenteditable());

        let mut f = NodeFlags::new();
        f.set_tab_index(Some(TabIndex::OverrideInParent(u32::MAX)));
        assert_eq!(
            f.get_tab_index(),
            Some(TabIndex::OverrideInParent((1 << 28) - 1)),
            "u32::MAX truncates to the 28-bit mask"
        );
        assert!(!f.is_anonymous(), "u32::MAX must not set ANONYMOUS");
        assert!(
            !f.is_contenteditable(),
            "u32::MAX must not set CONTENTEDITABLE"
        );
    }

    #[test]
    fn node_flags_set_tab_index_preserves_contenteditable_and_anonymous() {
        let mut f = NodeFlags::new();
        f.set_contenteditable_mut(true);
        f.set_anonymous(true);

        for ti in [
            None,
            Some(TabIndex::Auto),
            Some(TabIndex::NoKeyboardFocus),
            // NodeFlags packs the override into a documented 28-bit field
            // (TAB_VALUE_MASK), so u32::MAX would truncate to (1<<28)-1 and not
            // round-trip. (1<<28)-1 IS the largest encodable value -- still the
            // boundary case, but one this API can actually represent.
            Some(TabIndex::OverrideInParent((1 << 28) - 1)),
            Some(TabIndex::OverrideInParent(7)),
        ] {
            f.set_tab_index(ti);
            assert!(f.is_contenteditable(), "contenteditable lost for {ti:?}");
            assert!(f.is_anonymous(), "anonymous lost for {ti:?}");
            assert_eq!(f.get_tab_index(), ti);
        }
    }

    #[test]
    fn node_flags_set_contenteditable_preserves_tab_index_and_anonymous() {
        let mut f = NodeFlags::new();
        f.set_anonymous(true);
        f.set_tab_index(Some(TabIndex::OverrideInParent(12_345)));

        f.set_contenteditable_mut(true);
        assert!(f.is_contenteditable());
        assert!(f.is_anonymous());
        assert_eq!(f.get_tab_index(), Some(TabIndex::OverrideInParent(12_345)));

        f.set_contenteditable_mut(false);
        assert!(!f.is_contenteditable());
        assert!(f.is_anonymous());
        assert_eq!(f.get_tab_index(), Some(TabIndex::OverrideInParent(12_345)));
    }

    #[test]
    fn node_flags_set_anonymous_preserves_tab_index_and_contenteditable() {
        let mut f = NodeFlags::new();
        f.set_contenteditable_mut(true);
        f.set_tab_index(Some(TabIndex::NoKeyboardFocus));

        f.set_anonymous(true);
        assert!(f.is_anonymous());
        assert!(f.is_contenteditable());
        assert_eq!(f.get_tab_index(), Some(TabIndex::NoKeyboardFocus));

        f.set_anonymous(false);
        assert!(!f.is_anonymous());
        assert!(f.is_contenteditable());
        assert_eq!(f.get_tab_index(), Some(TabIndex::NoKeyboardFocus));
    }

    #[test]
    fn node_flags_consecutive_set_contenteditable_is_idempotent() {
        let mut f = NodeFlags::new();
        f.set_contenteditable_mut(true);
        let once = f;
        f.set_contenteditable_mut(true);
        assert_eq!(f, once, "setting twice must not toggle");
    }

    #[test]
    fn node_flags_builder_and_mut_setter_agree() {
        for v in [true, false] {
            let builder = NodeFlags::new().set_contenteditable(v);
            let mut mutated = NodeFlags::new();
            mutated.set_contenteditable_mut(v);
            assert_eq!(builder, mutated, "builder/mut disagree for {v}");
        }
    }

    #[test]
    fn node_flags_all_bits_set_decodes_without_panicking() {
        // Adversarial: a NodeFlags whose `inner` was never produced by the setters
        // (e.g. deserialized from a hostile FFI caller). Every getter must still
        // return a deterministic value instead of panicking.
        let f = NodeFlags { inner: u32::MAX };
        assert!(f.is_contenteditable());
        assert!(f.is_anonymous());
        // bits [30:29] == 0b11 == TAB_NO_KEYBOARD
        assert_eq!(f.get_tab_index(), Some(TabIndex::NoKeyboardFocus));
    }

    #[test]
    fn node_flags_get_tab_index_is_total_over_the_tag_bits() {
        // The `_ => None` arm of get_tab_index is unreachable (2 bits => 4 patterns,
        // all matched). Prove every tag pattern decodes to Some/None deterministically.
        for tag in 0u32..4 {
            for extra in [0u32, u32::MAX] {
                let inner = (tag << 29) | (extra & !(0b11 << 29));
                let f = NodeFlags { inner };
                let decoded = f.get_tab_index();
                match tag {
                    0 => assert_eq!(decoded, None),
                    1 => assert_eq!(decoded, Some(TabIndex::Auto)),
                    2 => assert!(matches!(decoded, Some(TabIndex::OverrideInParent(_)))),
                    _ => assert_eq!(decoded, Some(TabIndex::NoKeyboardFocus)),
                }
            }
        }
    }

    // =====================================================================
    // TabIndex — numeric limits
    // =====================================================================

    #[test]
    fn tab_index_default_is_auto_with_index_zero() {
        assert_eq!(TabIndex::default(), TabIndex::Auto);
        assert_eq!(TabIndex::default().get_index(), 0);
    }

    #[test]
    fn tab_index_get_index_at_numeric_limits() {
        assert_eq!(TabIndex::Auto.get_index(), 0);
        assert_eq!(TabIndex::NoKeyboardFocus.get_index(), -1);
        assert_eq!(TabIndex::OverrideInParent(0).get_index(), 0);
        // u32 -> isize must widen, never wrap negative (isize is >= 32 bits on all
        // supported targets, so u32::MAX stays positive).
        let max = TabIndex::OverrideInParent(u32::MAX).get_index();
        assert_eq!(max, u32::MAX as isize);
        assert!(max > 0, "u32::MAX must not wrap to a negative isize");
    }

    #[test]
    fn get_effective_tabindex_saturates_into_i32() {
        // Reached through NodeFlags, OverrideInParent is capped at 2^28 - 1, which
        // always fits i32 — so the i32::MAX saturation arm is not reachable via a
        // NodeData. Pin what IS reachable.
        let nd = NodeData::create_div().with_tab_index(TabIndex::OverrideInParent(u32::MAX));
        assert_eq!(nd.get_effective_tabindex(), Some((1 << 28) - 1));

        assert_eq!(
            NodeData::create_div()
                .with_tab_index(TabIndex::Auto)
                .get_effective_tabindex(),
            Some(0)
        );
        assert_eq!(
            NodeData::create_div()
                .with_tab_index(TabIndex::NoKeyboardFocus)
                .get_effective_tabindex(),
            Some(-1)
        );
        assert_eq!(NodeData::create_div().get_effective_tabindex(), None);
    }

    #[test]
    fn get_effective_tabindex_falls_back_to_zero_for_focus_callbacks() {
        let nd = NodeData::create_div().with_callback(
            EventFilter::Focus(FocusEventFilter::MouseDown),
            RefAny::new(0u32),
            0usize,
        );
        assert_eq!(nd.get_effective_tabindex(), Some(0));
    }

    // =====================================================================
    // TagId / ScrollTagId
    // =====================================================================

    #[test]
    fn tag_id_unique_never_returns_zero_and_never_repeats() {
        // 0 is reserved for "no tag". Other tests in this binary also allocate tags,
        // so assert distinctness rather than a specific starting value.
        let ids: Vec<TagId> = (0..512).map(|_| TagId::unique()).collect();
        for id in &ids {
            assert_ne!(id.inner, 0, "TagId 0 is reserved for 'no tag'");
        }
        let mut sorted: Vec<u64> = ids.iter().map(|t| t.inner).collect();
        sorted.sort_unstable();
        sorted.dedup();
        assert_eq!(sorted.len(), 512, "TagId::unique() handed out a duplicate");
    }

    #[test]
    fn tag_id_crate_internal_conversions_are_identity_at_limits() {
        for inner in [0u64, 1, u64::MAX, u64::MAX - 1] {
            let t = TagId { inner };
            assert_eq!(t.into_crate_internal(), t);
            assert_eq!(TagId::from_crate_internal(t), t);
            // Round-trip through both directions.
            assert_eq!(
                TagId::from_crate_internal(t.into_crate_internal()).inner,
                inner
            );
        }
    }

    #[test]
    fn tag_id_display_is_non_empty_at_numeric_limits() {
        for inner in [0u64, 1, u64::MAX] {
            let s = format!("{}", TagId { inner });
            assert!(!s.is_empty());
            assert!(s.contains(&inner.to_string()), "{s} should contain {inner}");
        }
    }

    #[test]
    fn scroll_tag_id_unique_is_distinct_and_debug_matches_display() {
        let a = ScrollTagId::unique();
        let b = ScrollTagId::unique();
        assert_ne!(a, b);
        assert_ne!(a.inner.inner, 0);

        let s = ScrollTagId {
            inner: TagId { inner: u64::MAX },
        };
        assert_eq!(format!("{s:?}"), format!("{s}"));
        assert!(!format!("{s}").is_empty());
    }

    // =====================================================================
    // AttributeType — getters / predicates / serializer invariants
    // =====================================================================

    #[test]
    fn attribute_boolean_attrs_always_have_an_empty_value() {
        // Invariant: is_boolean() means "present == true", so there is nothing to
        // serialize on the right-hand side.
        for attr in all_attribute_variants() {
            if attr.is_boolean() {
                assert_eq!(
                    attr.value().as_str(),
                    "",
                    "boolean attr {} must have an empty value",
                    attr.name()
                );
            }
        }
    }

    #[test]
    fn attribute_name_and_value_never_panic_for_any_variant() {
        for attr in all_attribute_variants() {
            let name = attr.name();
            let value = attr.value();
            // Every built-in variant has a non-empty name; only a Custom/Data
            // attribute can carry a caller-supplied empty name (see next test).
            assert!(!name.is_empty(), "empty name for {attr:?}");
            let _ = value.as_str();
        }
    }

    #[test]
    fn attribute_custom_with_empty_name_returns_empty_name_without_panicking() {
        let attr = AttributeType::Custom(AttributeNameValue {
            attr_name: "".into(),
            value: "".into(),
        });
        assert_eq!(attr.name(), "");
        assert_eq!(attr.value().as_str(), "");
        assert!(!attr.is_boolean());
    }

    #[test]
    fn attribute_as_id_and_as_class_are_mutually_exclusive() {
        for attr in all_attribute_variants() {
            match &attr {
                AttributeType::Id(s) => {
                    assert_eq!(attr.as_id(), Some(s.as_str()));
                    assert_eq!(attr.as_class(), None);
                }
                AttributeType::Class(s) => {
                    assert_eq!(attr.as_class(), Some(s.as_str()));
                    assert_eq!(attr.as_id(), None);
                }
                _ => {
                    assert_eq!(attr.as_id(), None, "as_id must be None for {attr:?}");
                    assert_eq!(attr.as_class(), None, "as_class must be None for {attr:?}");
                }
            }
        }
    }

    #[test]
    fn attribute_numeric_values_serialize_at_i32_limits() {
        assert_eq!(
            AttributeType::MinLength(i32::MIN).value().as_str(),
            "-2147483648"
        );
        assert_eq!(
            AttributeType::MaxLength(i32::MAX).value().as_str(),
            "2147483647"
        );
        assert_eq!(AttributeType::ColSpan(0).value().as_str(), "0");
        assert_eq!(AttributeType::RowSpan(-1).value().as_str(), "-1");
        assert_eq!(
            AttributeType::TabIndex(i32::MIN).value().as_str(),
            "-2147483648"
        );
    }

    #[test]
    fn attribute_focusable_is_tabindex_zero_and_not_boolean() {
        // Boundary: `Focusable` shares the "tabindex" name with TabIndex(i32) but,
        // unlike the boolean attrs, serializes a value ("0").
        let f = AttributeType::Focusable;
        assert_eq!(f.name(), "tabindex");
        assert_eq!(f.value().as_str(), "0");
        assert!(!f.is_boolean());
        assert_eq!(AttributeType::TabIndex(0).name(), "tabindex");
    }

    #[test]
    fn attribute_checked_true_and_false_are_both_boolean_and_share_a_name() {
        // AUDIT: CheckedFalse is_boolean() == true and value() == "", so a serializer
        // that emits boolean attrs as bare names would render `checked` for the
        // *unchecked* state. Pinning current behaviour — see report.
        assert!(AttributeType::CheckedTrue.is_boolean());
        assert!(AttributeType::CheckedFalse.is_boolean());
        assert_eq!(AttributeType::CheckedTrue.name(), "checked");
        assert_eq!(AttributeType::CheckedFalse.name(), "checked");
        assert_eq!(AttributeType::CheckedFalse.value().as_str(), "");
        // The two are still distinguishable as values.
        assert_ne!(AttributeType::CheckedTrue, AttributeType::CheckedFalse);
    }

    #[test]
    fn attribute_content_editable_and_draggable_stringify_bools() {
        assert_eq!(
            AttributeType::ContentEditable(true).value().as_str(),
            "true"
        );
        assert_eq!(
            AttributeType::ContentEditable(false).value().as_str(),
            "false"
        );
        assert_eq!(AttributeType::Draggable(true).value().as_str(), "true");
        assert_eq!(AttributeType::Draggable(false).value().as_str(), "false");
        assert!(!AttributeType::ContentEditable(false).is_boolean());
    }

    #[test]
    fn attribute_round_trips_huge_unicode_values() {
        let big = huge_unicode_string();
        let attr = AttributeType::Value(big.clone().into());
        assert_eq!(attr.value().as_str(), big.as_str());
        assert_eq!(attr.name(), "value");

        let id = AttributeType::Id(big.clone().into());
        assert_eq!(id.as_id(), Some(big.as_str()));
    }

    #[test]
    fn id_or_class_accessors_are_mutually_exclusive() {
        let id = IdOrClass::Id("my-id".into());
        let class = IdOrClass::Class("my-class".into());
        assert_eq!(id.as_id(), Some("my-id"));
        assert_eq!(id.as_class(), None);
        assert_eq!(class.as_class(), Some("my-class"));
        assert_eq!(class.as_id(), None);

        // Empty strings are legal and round-trip as Some("").
        assert_eq!(IdOrClass::Id("".into()).as_id(), Some(""));
        assert_eq!(IdOrClass::Class("".into()).as_class(), Some(""));
    }

    // =====================================================================
    // InputType
    // =====================================================================

    #[test]
    fn input_type_as_str_is_non_empty_and_unique_per_variant() {
        let all = [
            InputType::Text,
            InputType::Button,
            InputType::Checkbox,
            InputType::Color,
            InputType::Date,
            InputType::Datetime,
            InputType::DatetimeLocal,
            InputType::Email,
            InputType::File,
            InputType::Hidden,
            InputType::Image,
            InputType::Month,
            InputType::Number,
            InputType::Password,
            InputType::Radio,
            InputType::Range,
            InputType::Reset,
            InputType::Search,
            InputType::Submit,
            InputType::Tel,
            InputType::Time,
            InputType::Url,
            InputType::Week,
        ];
        let mut seen: Vec<&str> = all.iter().map(InputType::as_str).collect();
        for s in &seen {
            assert!(!s.is_empty());
            assert!(
                !s.contains(char::is_whitespace),
                "{s} is not a valid HTML attribute value"
            );
        }
        let len = seen.len();
        seen.sort_unstable();
        seen.dedup();
        assert_eq!(seen.len(), len, "two InputType variants share an as_str()");

        assert_eq!(InputType::DatetimeLocal.as_str(), "datetime-local");
        assert_eq!(InputType::Text.as_str(), "text");
    }

    // =====================================================================
    // NodeType
    // =====================================================================

    #[test]
    fn node_type_to_library_owned_round_trips_every_variant() {
        // encode == decode: the deep-copy must be value-equal to the original,
        // including the payload-carrying (boxed) variants.
        for nt in representative_node_types() {
            let owned = nt.to_library_owned_nodetype();
            assert_eq!(owned, nt, "to_library_owned_nodetype lost data for {nt:?}");
            assert_eq!(owned.get_path(), nt.get_path());
        }
    }

    #[test]
    fn node_type_get_path_and_format_never_panic() {
        for nt in representative_node_types() {
            let _tag = nt.get_path();
            let _fmt = nt.format();
            let _semantic = nt.is_semantic_for_accessibility();
        }
    }

    #[test]
    fn node_type_format_returns_content_only_for_content_variants() {
        assert_eq!(NodeType::Div.format(), None);
        assert_eq!(NodeType::Br.format(), None);
        assert_eq!(NodeType::Button.format(), None);

        assert_eq!(
            NodeType::Text(BoxOrStatic::heap(AzString::from("hi"))).format(),
            Some("hi".to_string())
        );
        assert_eq!(
            NodeType::VirtualView.format(),
            Some("virtualized-view".to_string())
        );
        assert_eq!(
            NodeType::Icon(BoxOrStatic::heap(AzString::from("home"))).format(),
            Some("icon(home)".to_string())
        );
    }

    #[test]
    fn node_type_format_handles_empty_and_unicode_text() {
        assert_eq!(
            NodeType::Text(BoxOrStatic::heap(AzString::from(""))).format(),
            Some(String::new())
        );
        let unicode = "日本語 🎉 ünïcødé";
        assert_eq!(
            NodeType::Text(BoxOrStatic::heap(AzString::from(unicode))).format(),
            Some(unicode.to_string())
        );
    }

    #[test]
    fn node_type_format_of_geolocation_probe_survives_nan_and_infinity() {
        // Adversarial floats: the probe config is formatted with `{}`, which must
        // print NaN/inf rather than panicking.
        for max_accuracy_m in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY, -0.0, f32::MAX] {
            let cfg = crate::geolocation::GeolocationProbeConfig {
                high_accuracy: true,
                background: true,
                max_accuracy_m,
                min_interval_ms: u32::MAX,
            };
            let out = NodeType::GeolocationProbe(cfg)
                .format()
                .expect("GeolocationProbe always formats");
            assert!(out.starts_with("geolocation-probe("));
            assert!(
                out.contains("4294967295"),
                "min_interval_ms must be printed"
            );
        }
    }

    #[test]
    fn geolocation_probe_nan_is_self_equal_and_hash_consistent() {
        // GeolocationProbeConfig gives f32 a total order via to_bits, so (unlike raw
        // f32) NaN == NaN. Eq and Hash must agree, or NodeType breaks as a HashMap key.
        let cfg = crate::geolocation::GeolocationProbeConfig {
            max_accuracy_m: f32::NAN,
            ..Default::default()
        };
        let a = NodeType::GeolocationProbe(cfg);
        let b = NodeType::GeolocationProbe(cfg);
        assert_eq!(a, b, "bitwise-NaN configs must compare equal");
        assert_eq!(
            hash_of(&a),
            hash_of(&b),
            "Eq == true but hashes differ: violates the Hash/Eq contract"
        );
        assert_eq!(a.cmp(&b), core::cmp::Ordering::Equal);
    }

    #[test]
    fn node_type_is_semantic_for_accessibility_known_true_and_false() {
        for nt in [
            NodeType::Button,
            NodeType::Input,
            NodeType::TextArea,
            NodeType::Select,
            NodeType::A,
            NodeType::H1,
            NodeType::H6,
            NodeType::Article,
            NodeType::Nav,
            NodeType::Main,
        ] {
            assert!(
                nt.is_semantic_for_accessibility(),
                "{nt:?} should be semantic"
            );
        }
        for nt in [
            NodeType::Div,
            NodeType::Span,
            NodeType::Br,
            NodeType::VirtualView,
            NodeType::Text(BoxOrStatic::heap(AzString::from("x"))),
        ] {
            assert!(
                !nt.is_semantic_for_accessibility(),
                "{nt:?} should not be semantic"
            );
        }
    }

    #[test]
    fn node_type_text_variants_are_content_sensitive() {
        let a = NodeType::Text(BoxOrStatic::heap(AzString::from("a")));
        let b = NodeType::Text(BoxOrStatic::heap(AzString::from("b")));
        assert_ne!(a, b);
        assert_eq!(a.get_path(), b.get_path(), "same tag, different content");
    }

    // =====================================================================
    // NodeData — attributes, ids, classes
    // =====================================================================

    #[test]
    fn node_data_default_has_no_attributes_and_no_extra_state() {
        let nd = NodeData::default();
        assert!(nd.is_node_type(NodeType::Div));
        assert!(nd.attributes().as_ref().is_empty());
        assert!(nd.get_ids_and_classes().as_ref().is_empty());
        assert!(nd.get_dataset().is_none());
        assert!(nd.get_key().is_none());
        assert!(nd.get_menu_bar().is_none());
        assert!(nd.get_context_menu().is_none());
        assert!(nd.get_svg_data().is_none());
        assert!(nd.get_image_clip_mask().is_none());
        assert!(nd.get_accessibility_info().is_none());
        assert!(nd.get_merge_callback().is_none());
        assert!(nd.get_component_origin().is_none());
        assert!(!nd.has_context_menu());
        assert!(!nd.is_contenteditable());
        assert!(!nd.is_anonymous());
        assert_eq!(nd.get_tab_index(), None);
    }

    #[test]
    fn attributes_mut_lazily_allocates_but_stays_empty() {
        let mut nd = NodeData::create_div();
        assert!(nd.attributes().as_ref().is_empty());
        let _ = nd.attributes_mut(); // allocates NodeDataExt
        assert!(
            nd.attributes().as_ref().is_empty(),
            "lazy alloc must not invent attributes"
        );
        nd.add_id("x".into());
        assert_eq!(nd.attributes().as_ref().len(), 1);
    }

    #[test]
    fn has_id_and_has_class_match_exactly_not_by_prefix() {
        let mut nd = NodeData::create_div();
        nd.add_id("header".into());
        nd.add_class("btn".into());

        assert!(nd.has_id("header"));
        assert!(nd.has_class("btn"));
        // No prefix/substring matching.
        assert!(!nd.has_id("head"));
        assert!(!nd.has_id("header2"));
        assert!(!nd.has_class("bt"));
        assert!(!nd.has_class(""));
        // Ids and classes must not cross over.
        assert!(!nd.has_class("header"));
        assert!(!nd.has_id("btn"));
    }

    #[test]
    fn has_id_matches_the_empty_string_id() {
        let mut nd = NodeData::create_div();
        assert!(!nd.has_id(""), "no ids at all => empty id must not match");
        nd.add_id("".into());
        assert!(nd.has_id(""), "an explicitly-added empty id must match");
        assert!(!nd.has_id("x"));
    }

    #[test]
    fn has_id_and_has_class_handle_unicode_and_huge_strings() {
        let unicode = "日本語-🎉-ünïcødé";
        let big = huge_unicode_string();

        let mut nd = NodeData::create_div();
        nd.add_id(unicode.into());
        nd.add_class(big.clone().into());

        assert!(nd.has_id(unicode));
        assert!(nd.has_class(big.as_str()));
        // A truncated-at-a-codepoint-boundary prefix must not match.
        assert!(!nd.has_id("日本語"));
    }

    #[test]
    fn duplicate_ids_are_kept_and_still_match() {
        let mut nd = NodeData::create_div();
        nd.add_id("dup".into());
        nd.add_id("dup".into());
        assert!(nd.has_id("dup"));
        assert_eq!(
            nd.get_ids_and_classes().as_ref().len(),
            2,
            "add_id does not deduplicate"
        );
    }

    #[test]
    fn get_ids_and_classes_preserves_insertion_order_and_kind() {
        let mut nd = NodeData::create_div();
        nd.add_id("i1".into());
        nd.add_class("c1".into());
        nd.add_id("i2".into());

        let v = nd.get_ids_and_classes();
        let v = v.as_ref();
        assert_eq!(v.len(), 3);
        assert_eq!(v[0], IdOrClass::Id("i1".into()));
        assert_eq!(v[1], IdOrClass::Class("c1".into()));
        assert_eq!(v[2], IdOrClass::Id("i2".into()));
    }

    #[test]
    fn get_ids_and_classes_ignores_non_id_class_attributes() {
        let mut nd = NodeData::create_div();
        nd.set_attributes(
            vec![
                AttributeType::Href("/x".into()),
                AttributeType::Id("i".into()),
                AttributeType::Disabled,
                AttributeType::Class("c".into()),
            ]
            .into(),
        );
        let v = nd.get_ids_and_classes();
        assert_eq!(v.as_ref().len(), 2);
    }

    #[test]
    fn set_ids_and_classes_replaces_ids_but_preserves_other_attributes() {
        // The dangerous part of set_ids_and_classes: it rebuilds the attribute vec.
        // Non-Id/Class attributes must survive.
        let mut nd = NodeData::create_div();
        nd.set_attributes(
            vec![
                AttributeType::Href("/old".into()),
                AttributeType::Id("old-id".into()),
                AttributeType::Class("old-class".into()),
                AttributeType::Disabled,
            ]
            .into(),
        );

        nd.set_ids_and_classes(vec![IdOrClass::Class("new-class".into())].into());

        assert!(!nd.has_id("old-id"), "old id must be dropped");
        assert!(!nd.has_class("old-class"), "old class must be dropped");
        assert!(nd.has_class("new-class"));
        // Href / Disabled must NOT have been collateral damage.
        let attrs = nd.attributes().as_ref();
        assert!(attrs.contains(&AttributeType::Href("/old".into())));
        assert!(attrs.contains(&AttributeType::Disabled));
        assert_eq!(attrs.len(), 3);
    }

    #[test]
    fn set_ids_and_classes_with_an_empty_vec_clears_all_ids_and_classes() {
        let mut nd = NodeData::create_div();
        nd.add_id("i".into());
        nd.add_class("c".into());
        nd.set_ids_and_classes(Vec::new().into());
        assert!(nd.get_ids_and_classes().as_ref().is_empty());
        assert!(!nd.has_id("i"));
        assert!(!nd.has_class("c"));
    }

    #[test]
    fn set_ids_and_classes_is_idempotent_when_reapplied() {
        let mut nd = NodeData::create_div();
        let ids: IdOrClassVec =
            vec![IdOrClass::Id("i".into()), IdOrClass::Class("c".into())].into();
        nd.set_ids_and_classes(ids.clone());
        let after_first = nd.attributes().clone();
        nd.set_ids_and_classes(ids);
        assert_eq!(
            nd.attributes().as_ref(),
            after_first.as_ref(),
            "re-applying the same ids/classes must not duplicate them"
        );
    }

    #[test]
    fn with_attribute_appends_without_dropping_existing_attributes() {
        // `with_attribute` is private, so this can only be exercised from an inline
        // test module.
        let nd = NodeData::create_div()
            .with_attribute(AttributeType::Href("/a".into()))
            .with_attribute(AttributeType::Alt("alt".into()));
        let attrs = nd.attributes().as_ref();
        assert_eq!(attrs.len(), 2);
        assert_eq!(attrs[0], AttributeType::Href("/a".into()));
        assert_eq!(attrs[1], AttributeType::Alt("alt".into()));
    }

    // =====================================================================
    // NodeData — constructors
    // =====================================================================

    #[test]
    fn create_node_shorthands_produce_the_right_node_type() {
        assert!(NodeData::create_body().is_node_type(NodeType::Body));
        assert!(NodeData::create_div().is_node_type(NodeType::Div));
        assert!(NodeData::create_br().is_node_type(NodeType::Br));
        assert!(NodeData::create_button_no_a11y().is_node_type(NodeType::Button));
        assert!(NodeData::create_table_no_a11y().is_node_type(NodeType::Table));
    }

    #[test]
    fn create_text_accepts_empty_unicode_and_huge_input() {
        for s in ["", "x", "日本語 🎉"] {
            let nd = NodeData::create_text_do_not_use_without_block_level_wrapper(s);
            assert!(nd.is_text_node());
            assert_eq!(nd.get_node_type().format(), Some(s.to_string()));
        }
        let big = huge_unicode_string();
        let nd = NodeData::create_text_do_not_use_without_block_level_wrapper(big.clone());
        assert!(nd.is_text_node());
        assert_eq!(nd.get_node_type().format(), Some(big));
    }

    #[test]
    fn create_a_stores_href_and_accessibility_name() {
        let nd = NodeData::create_a("/home".into(), SmallAriaInfo::label("Home"));
        assert!(nd.is_node_type(NodeType::A));
        assert!(nd
            .attributes()
            .as_ref()
            .contains(&AttributeType::Href("/home".into())));
        let info = nd
            .get_accessibility_info()
            .expect("create_a must set accessibility info");
        assert_eq!(info.accessibility_name, OptionString::Some("Home".into()));
    }

    #[test]
    fn create_a_no_a11y_has_href_but_no_accessibility_info() {
        let nd = NodeData::create_a_no_a11y("/x".into());
        assert!(nd
            .attributes()
            .as_ref()
            .contains(&AttributeType::Href("/x".into())));
        assert!(nd.get_accessibility_info().is_none());
    }

    #[test]
    fn create_a_accepts_an_empty_href() {
        let nd = NodeData::create_a_no_a11y("".into());
        assert_eq!(
            nd.attributes().as_ref()[0],
            AttributeType::Href("".into()),
            "empty href is stored verbatim, not dropped"
        );
    }

    #[test]
    fn create_input_stores_all_three_attributes_in_order() {
        let nd = NodeData::create_input_no_a11y("text".into(), "user".into(), "Username".into());
        assert!(nd.is_node_type(NodeType::Input));
        let attrs = nd.attributes().as_ref();
        assert_eq!(attrs.len(), 3);
        assert_eq!(attrs[0], AttributeType::InputType("text".into()));
        assert_eq!(attrs[1], AttributeType::Name("user".into()));
        assert_eq!(attrs[2], AttributeType::AriaLabel("Username".into()));
    }

    #[test]
    fn create_input_with_a11y_sets_both_attributes_and_accessibility_info() {
        let nd = NodeData::create_input(
            "password".into(),
            "pw".into(),
            "Password".into(),
            SmallAriaInfo::label("Password").with_role(AccessibilityRole::Text),
        );
        assert_eq!(nd.attributes().as_ref().len(), 3);
        let info = nd.get_accessibility_info().expect("a11y info");
        assert_eq!(info.role, AccessibilityRole::Text);
    }

    #[test]
    fn create_textarea_and_select_store_name_and_label() {
        let ta = NodeData::create_textarea_no_a11y("body".into(), "Body".into());
        assert!(ta.is_node_type(NodeType::TextArea));
        assert_eq!(ta.attributes().as_ref().len(), 2);

        let sel = NodeData::create_select_no_a11y("country".into(), "Country".into());
        assert!(sel.is_node_type(NodeType::Select));
        assert_eq!(
            sel.attributes().as_ref()[0],
            AttributeType::Name("country".into())
        );
    }

    #[test]
    fn create_label_uses_a_custom_for_attribute() {
        let nd = NodeData::create_label_no_a11y("email-input".into());
        assert!(nd.is_node_type(NodeType::Label));
        assert_eq!(
            nd.attributes().as_ref()[0],
            AttributeType::Custom(AttributeNameValue {
                attr_name: "for".into(),
                value: "email-input".into(),
            })
        );
        assert_eq!(nd.attributes().as_ref()[0].name(), "for");
        assert_eq!(nd.attributes().as_ref()[0].value().as_str(), "email-input");
    }

    #[test]
    fn create_button_and_table_with_aria_set_accessibility_info() {
        let btn = NodeData::create_button(
            SmallAriaInfo::label("Save").with_role(AccessibilityRole::PushButton),
        );
        let info = btn.get_accessibility_info().expect("a11y info");
        assert_eq!(info.role, AccessibilityRole::PushButton);
        assert_eq!(info.accessibility_name, OptionString::Some("Save".into()));

        let table = NodeData::create_table(SmallAriaInfo::label("Results"));
        assert!(table.is_node_type(NodeType::Table));
        assert!(table.get_accessibility_info().is_some());
    }

    #[test]
    fn a11y_constructors_accept_empty_aria_labels() {
        let btn = NodeData::create_button(SmallAriaInfo::label(""));
        let info = btn.get_accessibility_info().expect("a11y info");
        assert_eq!(info.accessibility_name, OptionString::Some("".into()));
        // An unset role degrades to Unknown rather than panicking.
        assert_eq!(info.role, AccessibilityRole::Unknown);
    }

    #[test]
    fn create_image_and_is_node_type_round_trip() {
        let img = ImageRef::null_image(4, 4, crate::resources::RawImageFormat::RGBA8, Vec::new());
        let nd = NodeData::create_image(img.clone());
        assert!(!nd.is_text_node());
        assert_eq!(nd.get_node_type().get_path(), NodeTypeTag::Img);
        assert!(nd.is_node_type(NodeType::Image(BoxOrStatic::heap(img))));
    }

    // =====================================================================
    // NodeData — predicates
    // =====================================================================

    #[test]
    fn is_node_type_is_content_sensitive_for_text() {
        let nd = NodeData::create_text_do_not_use_without_block_level_wrapper("a");
        assert!(nd.is_node_type(NodeType::Text(BoxOrStatic::heap(AzString::from("a")))));
        assert!(
            !nd.is_node_type(NodeType::Text(BoxOrStatic::heap(AzString::from("b")))),
            "is_node_type compares payloads, not just the discriminant"
        );
        assert!(!nd.is_node_type(NodeType::Div));
    }

    #[test]
    fn is_text_node_and_is_virtual_view_node() {
        assert!(NodeData::create_text_do_not_use_without_block_level_wrapper("x").is_text_node());
        assert!(!NodeData::create_div().is_text_node());

        let vv = NodeData::create_virtual_view(RefAny::new(1u32), virtual_view_callback());
        assert!(vv.is_virtual_view_node());
        assert!(!vv.is_text_node());
        assert!(vv.get_virtual_view_node_ref().is_some());
        assert!(!NodeData::create_div().is_virtual_view_node());
        assert!(NodeData::create_div().get_virtual_view_node_ref().is_none());
    }

    #[test]
    fn has_context_menu_flips_only_after_set_context_menu() {
        let mut nd = NodeData::create_div();
        assert!(!nd.has_context_menu());
        // A menu bar is a different slot and must not be mistaken for a context menu.
        nd.set_menu_bar(Menu::create(Vec::new().into()));
        assert!(
            !nd.has_context_menu(),
            "menu_bar must not satisfy has_context_menu"
        );
        assert!(nd.get_menu_bar().is_some());

        nd.set_context_menu(Menu::create(Vec::new().into()));
        assert!(nd.has_context_menu());
        assert!(nd.get_context_menu().is_some());
    }

    #[test]
    fn with_menu_bar_and_with_context_menu_are_independent_slots() {
        let nd = NodeData::create_div()
            .with_menu_bar(Menu::create(Vec::new().into()))
            .with_context_menu(Menu::create(Vec::new().into()));
        assert!(nd.get_menu_bar().is_some());
        assert!(nd.get_context_menu().is_some());
        assert!(nd.has_context_menu());
    }

    #[test]
    fn is_focusable_for_naturally_focusable_and_opted_in_nodes() {
        for nt in [
            NodeType::A,
            NodeType::Button,
            NodeType::Input,
            NodeType::Select,
            NodeType::TextArea,
        ] {
            assert!(
                NodeData::create_node(nt.clone()).is_focusable(),
                "{nt:?} is naturally focusable"
            );
        }
        assert!(!NodeData::create_div().is_focusable());
        assert!(NodeData::create_div()
            .with_contenteditable(true)
            .is_focusable());
        assert!(NodeData::create_div()
            .with_tab_index(TabIndex::NoKeyboardFocus)
            .is_focusable());
        assert!(NodeData::create_div()
            .with_callback(
                EventFilter::Focus(FocusEventFilter::MouseDown),
                RefAny::new(0u32),
                0usize,
            )
            .is_focusable());
        // A non-focus callback must NOT make a plain div focusable.
        assert!(!NodeData::create_div()
            .with_callback(
                EventFilter::Hover(HoverEventFilter::MouseOver),
                RefAny::new(0u32),
                0usize,
            )
            .is_focusable());
    }

    #[test]
    fn has_activation_behavior_for_elements_callbacks_and_roles() {
        assert!(NodeData::create_node(NodeType::A).has_activation_behavior());
        assert!(NodeData::create_button_no_a11y().has_activation_behavior());
        assert!(!NodeData::create_div().has_activation_behavior());

        for f in [HoverEventFilter::MouseUp, HoverEventFilter::LeftMouseUp] {
            assert!(NodeData::create_div()
                .with_callback(EventFilter::Hover(f), RefAny::new(0u32), 0usize)
                .has_activation_behavior());
        }
        // MouseDown is not a click.
        assert!(!NodeData::create_div()
            .with_callback(
                EventFilter::Hover(HoverEventFilter::MouseDown),
                RefAny::new(0u32),
                0usize,
            )
            .has_activation_behavior());

        let mut nd = NodeData::create_div();
        nd.set_accessibility_info(
            SmallAriaInfo::label("x")
                .with_role(AccessibilityRole::PushButton)
                .to_full_info(),
        );
        assert!(nd.has_activation_behavior(), "role=PushButton activates");
    }

    #[test]
    fn is_activatable_is_false_for_unavailable_elements() {
        let mut nd = NodeData::create_button_no_a11y();
        assert!(nd.is_activatable());

        let mut info = SmallAriaInfo::label("Save")
            .with_role(AccessibilityRole::PushButton)
            .to_full_info();
        info.states = vec![AccessibilityState::Unavailable].into();
        nd.set_accessibility_info(info);

        assert!(nd.has_activation_behavior());
        assert!(
            !nd.is_activatable(),
            "an Unavailable (disabled) button must not be activatable"
        );

        // Something with no activation behaviour at all is never activatable.
        assert!(!NodeData::create_div().is_activatable());
    }

    // =====================================================================
    // NodeData — accessible label / value / placeholder
    // =====================================================================

    #[test]
    fn get_accessible_label_prefers_aria_label_over_alt_and_title() {
        let mut nd = NodeData::create_div();
        nd.set_attributes(
            vec![
                AttributeType::Title("title".into()),
                AttributeType::Alt("alt".into()),
                AttributeType::AriaLabel("aria".into()),
            ]
            .into(),
        );
        assert_eq!(
            nd.get_accessible_label(),
            Some("aria"),
            "aria-label wins regardless of attribute order"
        );
    }

    #[test]
    fn get_accessible_label_alt_vs_title_is_order_dependent() {
        // AUDIT: the doc comment promises `aria-label > alt > title`, but the
        // implementation's second pass matches `Alt(s) | Title(s)` in a single arm,
        // so whichever appears FIRST in the attribute vec wins. With [Title, Alt]
        // that yields "title" — contradicting the documented priority. Pinned here
        // so a fix has to update this test deliberately. See report.
        let mut title_first = NodeData::create_div();
        title_first.set_attributes(
            vec![
                AttributeType::Title("title".into()),
                AttributeType::Alt("alt".into()),
            ]
            .into(),
        );
        assert_eq!(title_first.get_accessible_label(), Some("title"));

        let mut alt_first = NodeData::create_div();
        alt_first.set_attributes(
            vec![
                AttributeType::Alt("alt".into()),
                AttributeType::Title("title".into()),
            ]
            .into(),
        );
        assert_eq!(alt_first.get_accessible_label(), Some("alt"));
    }

    #[test]
    fn get_accessible_label_value_and_placeholder_default_to_none() {
        let nd = NodeData::create_div();
        assert_eq!(nd.get_accessible_label(), None);
        assert_eq!(nd.get_accessible_value(), None);
        assert_eq!(nd.get_placeholder(), None);
    }

    #[test]
    fn get_accessible_value_and_placeholder_return_the_first_match() {
        let mut nd = NodeData::create_div();
        nd.set_attributes(
            vec![
                AttributeType::Value("first".into()),
                AttributeType::Value("second".into()),
                AttributeType::Placeholder("ph".into()),
            ]
            .into(),
        );
        assert_eq!(nd.get_accessible_value(), Some("first"));
        assert_eq!(nd.get_placeholder(), Some("ph"));
    }

    #[test]
    fn get_accessible_label_returns_empty_string_not_none_for_empty_aria_label() {
        // Boundary: an empty aria-label is still "present" — Some("") not None.
        let mut nd = NodeData::create_div();
        nd.set_attributes(vec![AttributeType::AriaLabel("".into())].into());
        assert_eq!(nd.get_accessible_label(), Some(""));
    }

    // =====================================================================
    // NodeData — dataset / key / merge callback / component origin
    // =====================================================================

    #[test]
    fn dataset_set_get_take_round_trip() {
        let mut nd = NodeData::create_div();
        assert!(nd.get_dataset().is_none());
        assert!(nd.take_dataset().is_none(), "take on empty must be None");

        nd.set_dataset(OptionRefAny::Some(RefAny::new(42u32)));
        assert!(nd.get_dataset().is_some());
        assert!(nd.get_dataset_mut().is_some());

        let mut taken = nd.take_dataset().expect("dataset was set");
        assert_eq!(taken.downcast_ref::<u32>().map(|r| *r), Some(42));
        assert!(nd.get_dataset().is_none(), "take must clear the slot");
        assert!(nd.take_dataset().is_none(), "double-take must be None");
    }

    #[test]
    fn set_dataset_none_clears_without_allocating_extra() {
        let mut nd = NodeData::create_div();
        // Setting None on a node that never had a dataset must be a no-op, not a panic.
        nd.set_dataset(OptionRefAny::None);
        assert!(nd.get_dataset().is_none());

        nd.set_dataset(OptionRefAny::Some(RefAny::new(1u8)));
        nd.set_dataset(OptionRefAny::None);
        assert!(nd.get_dataset().is_none());
    }

    #[test]
    fn set_key_is_deterministic_and_input_sensitive() {
        let mut a = NodeData::create_div();
        let mut b = NodeData::create_div();
        a.set_key("user-123");
        b.set_key("user-123");
        assert_eq!(a.get_key(), b.get_key(), "same key input => same hash");
        assert!(a.get_key().is_some());

        let mut c = NodeData::create_div();
        c.set_key("user-124");
        assert_ne!(
            a.get_key(),
            c.get_key(),
            "different inputs => different keys"
        );
    }

    #[test]
    fn set_key_hashes_str_and_string_identically() {
        let mut a = NodeData::create_div();
        let mut b = NodeData::create_div();
        a.set_key("x");
        b.set_key(String::from("x"));
        assert_eq!(
            a.get_key(),
            b.get_key(),
            "&str and String must hash the same (Hash for str)"
        );
    }

    #[test]
    fn set_key_accepts_extreme_inputs() {
        for nd in [
            NodeData::create_div().with_key(""),
            NodeData::create_div().with_key(u64::MAX),
            NodeData::create_div().with_key(i64::MIN),
            NodeData::create_div().with_key(huge_unicode_string()),
        ] {
            assert!(nd.get_key().is_some());
        }
    }

    #[test]
    fn set_key_overwrites_rather_than_accumulating() {
        let mut nd = NodeData::create_div();
        nd.set_key("a");
        let first = nd.get_key();
        nd.set_key("b");
        assert_ne!(nd.get_key(), first, "the last set_key wins");
    }

    #[test]
    fn merge_callback_round_trips_the_function_pointer() {
        let mut nd = NodeData::create_div();
        assert!(nd.get_merge_callback().is_none());

        nd.set_merge_callback(merge_cb_a as DatasetMergeCallbackType);
        let cb = nd.get_merge_callback().expect("merge callback was set");
        assert_eq!(cb.cb as usize, merge_cb_a as usize);
        assert_eq!(cb.callable, OptionRefAny::None);

        // Overwriting swaps the pointer.
        nd.set_merge_callback(merge_cb_b as DatasetMergeCallbackType);
        let cb = nd
            .get_merge_callback()
            .expect("merge callback was replaced");
        assert_eq!(cb.cb as usize, merge_cb_b as usize);
    }

    #[test]
    fn dataset_merge_callback_from_ptr_matches_the_from_impl() {
        let via_ptr = DatasetMergeCallback::from_ptr(merge_cb_a);
        let via_from = DatasetMergeCallback::from(merge_cb_a as DatasetMergeCallbackType);
        assert_eq!(via_ptr, via_from);
        assert_eq!(via_ptr.cb as usize, merge_cb_a as usize);
        assert_eq!(via_ptr.callable, OptionRefAny::None);

        // Distinct functions must not compare equal.
        assert_ne!(via_ptr, DatasetMergeCallback::from_ptr(merge_cb_b));
    }

    #[test]
    fn dataset_merge_callback_debug_is_non_empty_and_names_the_type() {
        let cb = DatasetMergeCallback::from_ptr(merge_cb_a);
        let s = format!("{cb:?}");
        assert!(s.contains("DatasetMergeCallback"));
        assert!(s.contains("cb"));
    }

    #[test]
    fn merge_callback_is_actually_callable_through_the_stored_pointer() {
        let cb = DatasetMergeCallback::from_ptr(merge_cb_b);
        let mut out = (cb.cb)(RefAny::new(1u32), RefAny::new(2u32));
        assert_eq!(
            out.downcast_ref::<u32>().map(|r| *r),
            Some(2),
            "merge_cb_b returns the OLD data"
        );
    }

    #[test]
    fn component_origin_round_trips_and_defaults_to_none() {
        let mut nd = NodeData::create_div();
        assert!(nd.get_component_origin().is_none());

        nd.set_component_origin(ComponentOrigin {
            component_id: "shadcn:card".into(),
            data_model_json: crate::json::Json::null(),
        });
        let origin = nd.get_component_origin().expect("origin was set");
        assert_eq!(origin.component_id.as_str(), "shadcn:card");

        // The Default impl is well-formed and hashable.
        let d = ComponentOrigin::default();
        assert_eq!(d.component_id.as_str(), "");
        assert_eq!(hash_of(&d), hash_of(&ComponentOrigin::default()));
    }

    // =====================================================================
    // NodeData — svg data / clip mask
    // =====================================================================

    #[test]
    fn get_image_clip_mask_returns_none_for_non_mask_svg_data() {
        let mut nd = NodeData::create_div();
        assert!(nd.get_image_clip_mask().is_none());

        nd.set_svg_data(SvgNodeData::Circle {
            cx: 1.0,
            cy: 2.0,
            r: 3.0,
        });
        assert!(nd.get_svg_data().is_some());
        assert!(
            nd.get_image_clip_mask().is_none(),
            "a Circle is not an ImageClipMask"
        );
    }

    #[test]
    fn set_clip_mask_is_readable_through_get_image_clip_mask() {
        let mask = ImageMask {
            image: ImageRef::null_image(2, 2, crate::resources::RawImageFormat::R8, Vec::new()),
            rect: crate::geom::LogicalRect::new(
                LogicalPosition { x: 0.0, y: 0.0 },
                crate::geom::LogicalSize {
                    width: 2.0,
                    height: 2.0,
                },
            ),
            repeat: false,
        };
        let mut nd = NodeData::create_div();
        nd.set_clip_mask(mask.clone());
        assert_eq!(nd.get_image_clip_mask(), Some(&mask));
        // set_clip_mask stores through the same slot as set_svg_data.
        assert!(matches!(
            nd.get_svg_data(),
            Some(SvgNodeData::ImageClipMask(_))
        ));
    }

    #[test]
    fn svg_node_data_with_nan_coords_is_self_equal_and_hash_consistent() {
        // SvgNodeData hashes f32 via to_bits and derives Eq, so a NaN-carrying shape
        // must be equal to (and hash like) itself, or NodeData's Hash/Eq contract
        // breaks for SVG nodes.
        let a = SvgNodeData::Rect {
            x: f32::NAN,
            y: f32::INFINITY,
            width: f32::NEG_INFINITY,
            height: -0.0,
            rx: 0.0,
            ry: f32::MAX,
        };
        let b = a.clone();
        assert_eq!(a, b);
        assert_eq!(hash_of(&a), hash_of(&b));
        assert_eq!(a.cmp(&b), core::cmp::Ordering::Equal);
    }

    #[test]
    fn svg_node_data_line_and_linear_gradient_are_distinct_despite_a_shared_hash_body() {
        // The Hash impl deliberately folds Line and LinearGradient into one arm, so
        // they can hash alike — but Eq must still tell them apart.
        let line = SvgNodeData::Line {
            x1: 1.0,
            y1: 2.0,
            x2: 3.0,
            y2: 4.0,
        };
        let grad = SvgNodeData::LinearGradient {
            x1: 1.0,
            y1: 2.0,
            x2: 3.0,
            y2: 4.0,
        };
        assert_ne!(line, grad, "same field values, different variants");
    }

    // =====================================================================
    // NodeData — hashing
    // =====================================================================

    #[test]
    fn calculate_node_data_hash_is_deterministic_and_equal_for_equal_nodes() {
        let a = NodeData::create_div()
            .with_key("k")
            .with_contenteditable(true);
        let b = a.clone();
        assert_eq!(a, b);
        assert_eq!(a.calculate_node_data_hash(), b.calculate_node_data_hash());
        assert_eq!(
            a.calculate_node_data_hash(),
            a.calculate_node_data_hash(),
            "hashing must not depend on call count"
        );
    }

    #[test]
    fn structural_hash_ignores_text_content_but_data_hash_does_not() {
        // Documented behaviour: Text("Hello") must match Text("Hello World") during
        // reconciliation so the cursor survives an edit.
        let a = NodeData::create_text_do_not_use_without_block_level_wrapper("Hello");
        let b = NodeData::create_text_do_not_use_without_block_level_wrapper("Hello World");

        assert_eq!(
            a.calculate_structural_hash(),
            b.calculate_structural_hash(),
            "structural hash must ignore text content"
        );
        assert_ne!(
            a.calculate_node_data_hash(),
            b.calculate_node_data_hash(),
            "the full data hash must NOT ignore text content"
        );
    }

    #[test]
    fn structural_hash_ignores_contenteditable_but_data_hash_does_not() {
        let plain = NodeData::create_div();
        let editable = NodeData::create_div().with_contenteditable(true);

        assert_eq!(
            plain.calculate_structural_hash(),
            editable.calculate_structural_hash(),
            "contenteditable flips with focus; it must not move the structural hash"
        );
        assert_ne!(
            plain.calculate_node_data_hash(),
            editable.calculate_node_data_hash(),
            "flags ARE part of the full data hash"
        );
    }

    #[test]
    fn structural_hash_is_sensitive_to_ids_classes_and_node_type() {
        let mut a = NodeData::create_div();
        a.add_id("a".into());
        let mut b = NodeData::create_div();
        b.add_id("b".into());
        assert_ne!(a.calculate_structural_hash(), b.calculate_structural_hash());

        let mut c = NodeData::create_div();
        c.add_class("a".into());
        assert_ne!(
            a.calculate_structural_hash(),
            c.calculate_structural_hash(),
            "id=\"a\" and class=\"a\" must not collide"
        );

        assert_ne!(
            NodeData::create_div().calculate_structural_hash(),
            NodeData::create_br().calculate_structural_hash()
        );
    }

    #[test]
    fn node_data_eq_implies_equal_hash_for_a_richly_populated_node() {
        let mut a = NodeData::create_div();
        a.add_id("id".into());
        a.add_class("cls".into());
        a.set_tab_index(TabIndex::OverrideInParent(9));
        a.set_contenteditable(true);
        a.set_anonymous(true);
        a.set_key("key");
        a.set_dataset(OptionRefAny::Some(RefAny::new(7u64)));
        a.set_svg_data(SvgNodeData::GradientStop { offset: 0.5 });
        a.set_context_menu(Menu::create(Vec::new().into()));
        a.set_merge_callback(merge_cb_a as DatasetMergeCallbackType);
        a.set_css("color: red;");

        let b = a.clone();
        assert_eq!(a, b, "clone must be value-equal");
        assert_eq!(
            hash_of(&a),
            hash_of(&b),
            "Eq == true but hashes differ: Hash/Eq contract violated"
        );
        assert_eq!(a.calculate_node_data_hash(), b.calculate_node_data_hash());
        // copy_special must agree with Clone.
        assert_eq!(a.copy_special(), b);
    }

    // =====================================================================
    // NodeData — Display / node_data_to_string (serializer)
    // =====================================================================

    #[test]
    fn node_data_to_string_is_empty_for_a_bare_node() {
        // Private fn — only reachable from an inline test module.
        assert_eq!(node_data_to_string(&NodeData::create_div()), "");
    }

    #[test]
    fn node_data_to_string_emits_ids_classes_and_tabindex() {
        let mut nd = NodeData::create_div();
        nd.add_id("i1".into());
        nd.add_id("i2".into());
        nd.add_class("c1".into());
        nd.set_tab_index(TabIndex::NoKeyboardFocus);

        let s = node_data_to_string(&nd);
        assert!(s.contains(r#"id="i1 i2""#), "ids are space-joined: {s}");
        assert!(s.contains(r#"class="c1""#), "{s}");
        assert!(s.contains(r#"tabindex="-1""#), "{s}");
    }

    #[test]
    fn node_data_display_is_self_closing_without_content() {
        let s = format!("{}", NodeData::create_div());
        assert!(s.starts_with('<'), "{s}");
        assert!(s.ends_with("/>"), "content-less nodes self-close: {s}");
    }

    #[test]
    fn node_data_display_wraps_text_content_in_a_tag_pair() {
        let s = format!(
            "{}",
            NodeData::create_text_do_not_use_without_block_level_wrapper("hello")
        );
        assert!(s.starts_with('<'));
        assert!(s.ends_with('>'));
        assert!(s.contains("hello"), "{s}");
        assert!(
            !s.ends_with("/>"),
            "a node with content must not self-close"
        );
    }

    #[test]
    fn node_data_display_does_not_panic_on_hostile_text() {
        // NOTE: Display is a debug/inspection aid and does NOT escape markup — a text
        // node containing `<script>` reproduces it verbatim. Assert only that it is
        // total (no panic) and round-trips the bytes; see report.
        for text in [
            "",
            "<script>alert(1)</script>",
            "\" onload=\"x",
            "日本語 🎉",
            "line\nbreak\ttab",
        ] {
            let s = format!(
                "{}",
                NodeData::create_text_do_not_use_without_block_level_wrapper(text)
            );
            assert!(s.contains(text), "Display dropped content for {text:?}");
        }
    }

    #[test]
    fn node_data_display_survives_a_huge_text_payload() {
        let big = huge_unicode_string();
        let s = format!(
            "{}",
            NodeData::create_text_do_not_use_without_block_level_wrapper(big.clone())
        );
        assert!(s.len() > big.len());
    }

    #[test]
    fn debug_print_end_matches_the_node_tag() {
        let s = NodeData::create_div().debug_print_end();
        assert!(s.starts_with("</"));
        assert!(s.ends_with('>'));
    }

    // =====================================================================
    // NodeData — setters / builders / swap
    // =====================================================================

    #[test]
    fn set_node_type_replaces_the_type_and_keeps_the_attributes() {
        let mut nd = NodeData::create_div();
        nd.add_id("keep".into());
        nd.set_node_type(NodeType::Span);
        assert!(nd.is_node_type(NodeType::Span));
        assert!(
            nd.has_id("keep"),
            "changing the tag must not drop attributes"
        );
    }

    #[test]
    fn add_callback_appends_and_get_callbacks_reflects_it() {
        let mut nd = NodeData::create_div();
        assert!(nd.get_callbacks().as_ref().is_empty());

        nd.add_callback(
            EventFilter::Hover(HoverEventFilter::MouseUp),
            RefAny::new(1u32),
            0usize,
        );
        nd.add_callback(
            EventFilter::Focus(FocusEventFilter::MouseDown),
            RefAny::new(2u32),
            1usize,
        );
        assert_eq!(nd.get_callbacks().as_ref().len(), 2);
        assert_eq!(
            nd.get_callbacks().as_ref()[0].event,
            EventFilter::Hover(HoverEventFilter::MouseUp)
        );
    }

    #[test]
    fn add_css_property_appends_an_inline_rule() {
        use azul_css::props::property::{CssProperty, CssPropertyType};

        let mut nd = NodeData::create_div();
        assert!(nd.get_style().rules.as_ref().is_empty());

        nd.add_css_property(CssPropertyWithConditions {
            property: CssProperty::const_none(CssPropertyType::Display),
            apply_if: Vec::new().into(),
        });
        assert_eq!(nd.get_style().rules.as_ref().len(), 1);

        nd.add_css_property(CssPropertyWithConditions {
            property: CssProperty::const_none(CssPropertyType::Display),
            apply_if: Vec::new().into(),
        });
        assert_eq!(
            nd.get_style().rules.as_ref().len(),
            2,
            "add_css_property appends, it does not replace"
        );
    }

    #[test]
    fn set_style_replaces_whereas_set_css_appends() {
        let mut nd = NodeData::create_div();
        nd.set_css("color: red;");
        let after_first = nd.get_style().rules.as_ref().len();
        assert!(after_first > 0);

        nd.set_css("color: blue;");
        assert!(
            nd.get_style().rules.as_ref().len() > after_first,
            "set_css appends to the existing inline style"
        );

        nd.set_style(azul_css::css::Css {
            rules: Vec::new().into(),
            keyframes: Vec::new().into(),
        });
        assert!(
            nd.get_style().rules.as_ref().is_empty(),
            "set_style replaces wholesale"
        );
    }

    #[test]
    fn set_css_with_empty_and_malformed_input_does_not_panic() {
        for style in [
            "",
            "   ",
            ";;;;",
            "color",
            "color:",
            ":",
            "}",
            "{",
            "color: ;",
            "not-a-property: not-a-value;",
            ":hover {",
            "@os {",
            "color: red",       // no trailing semicolon
            "\u{0}color: red;", // NUL byte
            "color: 日本語;",
        ] {
            let nd = NodeData::create_div().with_css(style);
            // The only contract for malformed input is "don't panic"; whether a rule
            // survives parsing is the CSS parser's business.
            let _ = nd.get_style().rules.as_ref().len();
        }
    }

    #[test]
    fn swap_with_default_returns_the_original_and_leaves_a_div() {
        let mut nd = NodeData::create_text_do_not_use_without_block_level_wrapper("payload");
        let taken = nd.swap_with_default();
        assert!(taken.is_text_node());
        assert!(
            nd.is_node_type(NodeType::Div),
            "the slot becomes a fresh div"
        );
        assert!(nd.attributes().as_ref().is_empty());
    }

    #[test]
    fn node_data_builders_are_equivalent_to_their_setters() {
        let built = NodeData::create_div()
            .with_tab_index(TabIndex::Auto)
            .with_contenteditable(true)
            .with_node_type(NodeType::Span);

        let mut set = NodeData::create_div();
        set.set_tab_index(TabIndex::Auto);
        set.set_contenteditable(true);
        set.set_node_type(NodeType::Span);

        assert_eq!(built, set);
    }

    // =====================================================================
    // NodeDataVec containers
    // =====================================================================

    #[test]
    fn node_data_vec_as_container_is_empty_for_an_empty_vec() {
        let v: NodeDataVec = Vec::new().into();
        assert_eq!(v.as_container().internal.len(), 0);
    }

    #[test]
    fn node_data_vec_containers_expose_and_mutate_the_backing_slice() {
        let mut v: NodeDataVec = vec![
            NodeData::create_div(),
            NodeData::create_br(),
            NodeData::create_text_do_not_use_without_block_level_wrapper("t"),
        ]
        .into();
        assert_eq!(v.as_container().internal.len(), 3);
        assert!(v.as_container().internal[2].is_text_node());

        v.as_container_mut().internal[0].set_node_type(NodeType::Span);
        assert!(v.as_container().internal[0].is_node_type(NodeType::Span));
    }

    // =====================================================================
    // Dom — child bookkeeping
    // =====================================================================

    #[test]
    fn dom_default_is_an_empty_body() {
        let d = Dom::default();
        assert!(d.root.is_node_type(NodeType::Body));
        assert_eq!(d.estimated_total_children, 0);
        assert_eq!(d.node_count(), 1);
    }

    #[test]
    fn dom_set_children_recomputes_the_estimate_from_scratch() {
        let child = Dom::create_div().with_child(Dom::create_div());
        let mut parent = Dom::create_div();
        parent.add_child(Dom::create_div());
        assert_eq!(parent.estimated_total_children, 1);

        // set_children REPLACES; the old child must not be counted twice.
        parent.set_children(vec![child].into());
        assert_eq!(parent.estimated_total_children, 2);
        assert_eq!(
            parent.estimated_total_children,
            parent.recompute_estimated_total_children()
        );
    }

    #[test]
    fn dom_set_children_with_an_empty_vec_zeroes_the_estimate() {
        let mut d = Dom::create_div().with_child(Dom::create_div().with_child(Dom::create_div()));
        assert_eq!(d.estimated_total_children, 2);
        d.set_children(Vec::new().into());
        assert_eq!(d.estimated_total_children, 0);
        assert_eq!(d.node_count(), 1);
    }

    #[test]
    fn dom_deeply_nested_chain_keeps_an_exact_estimate() {
        // 256-deep chain: every level adds exactly one descendant.
        const DEPTH: usize = 256;
        let mut d = Dom::create_div();
        for _ in 0..DEPTH {
            d = Dom::create_div().with_child(d);
        }
        assert_eq!(d.estimated_total_children, DEPTH);
        assert_eq!(d.node_count(), DEPTH + 1);
        assert_eq!(d.recompute_estimated_total_children(), DEPTH);
    }

    #[test]
    fn dom_very_wide_child_list_keeps_an_exact_estimate() {
        const WIDTH: usize = 5_000;
        let children: Vec<Dom> = (0..WIDTH).map(|_| Dom::create_div()).collect();
        let d = Dom::create_div().with_children(children.into());
        assert_eq!(d.estimated_total_children, WIDTH);
        assert_eq!(d.node_count(), WIDTH + 1);
    }

    #[test]
    fn dom_from_iterator_counts_nested_grandchildren() {
        let empty: Dom = Vec::new().into_iter().collect();
        assert_eq!(empty.estimated_total_children, 0);
        assert!(empty.root.is_node_type(NodeType::Div));

        // Two children, one of which has a child of its own => 3 descendants.
        let d: Dom = vec![
            Dom::create_div().with_child(Dom::create_div()),
            Dom::create_div(),
        ]
        .into_iter()
        .collect();
        assert_eq!(d.estimated_total_children, 3);
        assert_eq!(
            d.estimated_total_children,
            d.recompute_estimated_total_children()
        );
        assert_eq!(d.node_count(), 4);
    }

    #[test]
    fn dom_fixup_repairs_a_corrupted_estimate_at_every_depth() {
        let mut d = Dom::create_div()
            .with_child(Dom::create_div().with_child(Dom::create_div()))
            .with_child(Dom::create_div());

        // Corrupt the cached counter at BOTH levels (the public field makes this
        // reachable from safe code, which is what fixup exists to undo).
        d.estimated_total_children = 0;
        d.children.as_mut()[0].estimated_total_children = 99;

        let repaired = d.fixup_children_estimated();
        assert_eq!(repaired, 3);
        assert_eq!(d.children.as_ref()[0].estimated_total_children, 1);
        assert_eq!(
            d.estimated_total_children,
            d.recompute_estimated_total_children()
        );
    }

    #[test]
    fn dom_fixup_on_a_leaf_zeroes_a_bogus_estimate() {
        let mut d = Dom::create_div();
        d.estimated_total_children = usize::MAX;
        assert_eq!(d.fixup_children_estimated(), 0);
        assert_eq!(d.node_count(), 1, "node_count is safe again after fixup");
    }

    // `estimated_total_children` is a public field, so `usize::MAX` is
    // reachable. This used to be `#[cfg(debug_assertions)] #[should_panic]`,
    // pinning "panics in debug, wraps to 0 in release" — i.e. pinning a
    // divergence where the release answer was the dangerous one (0 reads as
    // "empty DOM"). `node_count` saturates now, so assert the SAME defined
    // answer in both configurations, and assert it is not the empty-DOM value.
    #[test]
    fn dom_node_count_saturates_on_a_corrupted_max_estimate() {
        let mut d = Dom::create_div();
        d.estimated_total_children = usize::MAX;
        assert_eq!(d.node_count(), usize::MAX);
        assert_ne!(
            d.node_count(),
            0,
            "wrapping to 0 would claim an empty DOM — the one answer callers \
             act on without checking"
        );
    }

    #[test]
    fn dom_swap_with_default_returns_the_original_tree() {
        let mut d = Dom::create_div().with_child(Dom::create_div());
        let taken = d.swap_with_default();
        assert_eq!(taken.estimated_total_children, 1);
        assert_eq!(d.estimated_total_children, 0, "the slot is reset");
        assert!(d.root.is_node_type(NodeType::Div));
    }

    // =====================================================================
    // Dom — builders
    // =====================================================================

    #[test]
    fn dom_with_id_and_with_class_apply_to_the_root() {
        let d = Dom::create_div()
            .with_id("root".into())
            .with_class("card".into());
        assert!(d.root.has_id("root"));
        assert!(d.root.has_class("card"));
    }

    #[test]
    fn dom_with_attribute_appends_and_with_attributes_replaces() {
        let d = Dom::create_div()
            .with_attribute(AttributeType::Href("/a".into()))
            .with_attribute(AttributeType::Alt("alt".into()));
        assert_eq!(d.root.attributes().as_ref().len(), 2);

        let d = d.with_attributes(vec![AttributeType::Disabled].into());
        assert_eq!(
            d.root.attributes().as_ref().len(),
            1,
            "with_attributes replaces wholesale"
        );
        assert_eq!(d.root.attributes().as_ref()[0], AttributeType::Disabled);
    }

    #[test]
    fn dom_add_component_css_stacks_stylesheets() {
        let mut d = Dom::create_div();
        assert!(d.css.as_ref().is_empty());
        d.set_css("color: red;");
        d.set_css("color: blue;");
        assert_eq!(d.css.as_ref().len(), 2, "each set_css pushes a stylesheet");

        d.set_component_css(Vec::new().into());
        assert!(d.css.as_ref().is_empty(), "set_component_css replaces");
    }

    #[test]
    fn dom_with_css_does_not_panic_on_malformed_input() {
        for style in ["", "}}}", "@os {", "color:", "\u{0}"] {
            let d = Dom::create_div().with_css(style);
            assert_eq!(
                d.css.as_ref().len(),
                1,
                "a Css is pushed even if it parses empty"
            );
        }
    }

    #[test]
    fn dom_text_helpers_produce_a_text_child() {
        let d = Dom::create_h1_with_text("Title");
        assert!(d.root.is_node_type(NodeType::H1));
        assert_eq!(d.estimated_total_children, 1);
        assert!(d.children.as_ref()[0].root.is_text_node());
    }

    #[test]
    fn dom_create_geolocation_probe_carries_its_config() {
        let cfg = crate::geolocation::GeolocationProbeConfig {
            high_accuracy: true,
            background: false,
            max_accuracy_m: 25.0,
            min_interval_ms: 1_000,
        };
        let d = Dom::create_geolocation_probe(cfg);
        match d.root.get_node_type() {
            NodeType::GeolocationProbe(c) => {
                assert!(c.high_accuracy);
                assert_eq!(c.min_interval_ms, 1_000);
            }
            other => panic!("expected GeolocationProbe, got {other:?}"),
        }
    }

    #[test]
    fn dom_clone_and_eq_agree_on_a_nested_tree() {
        let d = Dom::create_div()
            .with_id("r".into())
            .with_child(Dom::create_text_do_not_use_without_block_level_wrapper("a"))
            .with_child(
                Dom::create_div()
                    .with_child(Dom::create_text_do_not_use_without_block_level_wrapper("b")),
            );
        let c = d.clone();
        assert_eq!(d, c);
        assert_eq!(hash_of(&d), hash_of(&c));
        // text("a") + div + text("b") == 3 descendants.
        assert_eq!(c.estimated_total_children, 3);
        assert_eq!(c.node_count(), 4);
    }

    #[test]
    fn dom_debug_does_not_panic_on_a_nested_tree() {
        let d = Dom::create_div()
            .with_child(Dom::create_text_do_not_use_without_block_level_wrapper(
                "日本語 🎉",
            ))
            .with_child(Dom::create_div().with_child(Dom::create_br()));
        let s = format!("{d:?}");
        assert!(s.contains("Dom"));
        assert!(s.contains("estimated_total_children"));
    }

    // =====================================================================
    // DomId / DomNodeId
    // =====================================================================

    #[test]
    fn dom_id_root_is_zero_and_is_the_default() {
        assert_eq!(DomId::ROOT_ID.inner, 0);
        assert_eq!(DomId::default(), DomId::ROOT_ID);
        assert_eq!(format!("{}", DomId::ROOT_ID), "0");
        assert_eq!(
            format!("{}", DomId { inner: usize::MAX }),
            usize::MAX.to_string()
        );
    }

    #[test]
    fn dom_node_id_root_points_at_the_root_dom_and_no_node() {
        assert_eq!(DomNodeId::ROOT.dom, DomId::ROOT_ID);
        assert_eq!(DomNodeId::ROOT.node, NodeHierarchyItemId::NONE);
    }
}
