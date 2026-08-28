#[cfg(test)]
mod assign_tests {
    use super::*;

    /// A patch that sets only a NAME must leave the widget's role, value and
    /// states intact. This is the whole contract: an app names a control, and
    /// the slider keeps reporting where its thumb is.
    #[test]
    fn assign_takes_only_what_the_patch_sets() {
        let mut widget = AccessibilityInfo {
            role: AccessibilityRole::Slider,
            accessibility_value: OptionString::Some("42".into()),
            states: AccessibilityStateVec::from_vec(vec![AccessibilityState::Focusable]),
            ..Default::default()
        };

        widget.assign(AccessibilityInfo {
            accessibility_name: OptionString::Some("Volume".into()),
            ..Default::default()
        });

        assert_eq!(
            widget.accessibility_name.as_ref().map(|s| s.as_str()),
            Some("Volume")
        );
        assert!(
            matches!(widget.role, AccessibilityRole::Slider),
            "an unset role in the patch (Unknown) must not clobber a real one"
        );
        assert_eq!(
            widget.accessibility_value.as_ref().map(|s| s.as_str()),
            Some("42"),
            "the live value must survive an app setting the name"
        );
        assert_eq!(widget.states.as_ref().len(), 1, "states must survive too");
    }

    /// A patch CAN override a field the base already set — that is the point of
    /// an override.
    #[test]
    fn assign_overrides_a_field_the_patch_does_set() {
        let mut base = AccessibilityInfo::named("Old", AccessibilityRole::PushButton);
        base.assign(AccessibilityInfo {
            accessibility_name: OptionString::Some("New".into()),
            role: AccessibilityRole::CheckButton,
            ..Default::default()
        });
        assert_eq!(
            base.accessibility_name.as_ref().map(|s| s.as_str()),
            Some("New")
        );
        assert!(matches!(base.role, AccessibilityRole::CheckButton));
    }

    /// `is_live_region: false` in a partial struct is indistinguishable from
    /// "not specified", so it must never clear a `true` the widget set.
    #[test]
    fn assign_never_clears_a_live_region_flag() {
        let mut base = AccessibilityInfo {
            is_live_region: true,
            ..Default::default()
        };
        base.assign(AccessibilityInfo::default());
        assert!(
            base.is_live_region,
            "a default-constructed patch must not turn a live region off"
        );
    }
}

#[cfg(test)]
mod autotest_generated {
    use super::*;
    use alloc::string::String;

    // ---- small helpers to reach into the FFI-style option wrappers ----

    fn name_str(o: &OptionString) -> Option<&str> {
        o.as_ref().map(|s| s.as_str())
    }

    fn f32_of(o: &OptionF32) -> Option<f32> {
        o.as_ref().copied()
    }

    /// A battery of adversarial strings: empty, embedded NUL, control chars,
    /// combining unicode, emoji, RTL, and a very large allocation.
    fn adversarial_strings() -> Vec<String> {
        vec![
            String::new(),
            String::from(" "),
            String::from("\0"),
            String::from("a\0b\0c"),
            String::from("\t\r\n\x1b[0m"),
            String::from("日本語のテキスト"),
            String::from("🎉👨‍👩‍👧‍👦🇺🇳"),
            String::from("e\u{0301}\u{0301}\u{0301}"), // combining accents
            String::from("\u{202e}reversed\u{202d}"),  // RTL override
            String::from("\u{FFFD}\u{10FFFF}"),        // replacement + max scalar
            "x".repeat(100_000),                       // huge
        ]
    }

    /// Numeric edge values for f32 fields.
    fn adversarial_f32() -> Vec<f32> {
        vec![
            0.0,
            -0.0,
            1.0,
            -1.0,
            f32::MIN,
            f32::MAX,
            f32::MIN_POSITIVE,
            -f32::MIN_POSITIVE,
            f32::EPSILON,
            f32::NAN,
            f32::INFINITY,
            f32::NEG_INFINITY,
        ]
    }

    // =====================================================================
    // 1. SmallAriaInfo::label — no_panic_smoke
    // =====================================================================

    #[test]
    fn small_label_no_panic_smoke() {
        for s in adversarial_strings() {
            let expected = s.clone();
            let info = SmallAriaInfo::label(s);
            // The label must round-trip verbatim and the other fields default to None.
            assert_eq!(name_str(&info.label), Some(expected.as_str()));
            assert!(info.role.is_none());
            assert!(info.description.is_none());
            // to_full_info must not panic even for pathological labels.
            let full = info.to_full_info();
            assert_eq!(name_str(&full.accessibility_name), Some(expected.as_str()));
        }
        // `&str` input path as well.
        let info = SmallAriaInfo::label("hello");
        assert_eq!(name_str(&info.label), Some("hello"));
    }

    // =====================================================================
    // 2. SmallAriaInfo::with_role — no_panic + invariants
    // =====================================================================

    fn representative_roles() -> Vec<AccessibilityRole> {
        vec![
            AccessibilityRole::TitleBar, // first variant
            AccessibilityRole::PushButton,
            AccessibilityRole::CheckButton,
            AccessibilityRole::Slider,
            AccessibilityRole::Link,
            AccessibilityRole::Nothing,
            AccessibilityRole::Unknown, // last variant
        ]
    }

    #[test]
    fn small_with_role_invariants() {
        for role in representative_roles() {
            let info = SmallAriaInfo::label("base").with_role(role);
            // Only the role field changes; label preserved, description untouched.
            assert_eq!(info.role, OptionAccessibilityRole::Some(role));
            assert_eq!(name_str(&info.label), Some("base"));
            assert!(info.description.is_none());
        }
        // Last-write-wins when applied twice.
        let info = SmallAriaInfo::label("x")
            .with_role(AccessibilityRole::Link)
            .with_role(AccessibilityRole::Slider);
        assert_eq!(
            info.role,
            OptionAccessibilityRole::Some(AccessibilityRole::Slider)
        );
    }

    // =====================================================================
    // 3. SmallAriaInfo::with_description — no_panic + invariants
    // =====================================================================

    #[test]
    fn small_with_description_invariants() {
        for s in adversarial_strings() {
            let expected = s.clone();
            let info = SmallAriaInfo::label("base").with_description(s);
            assert_eq!(name_str(&info.description), Some(expected.as_str()));
            // label untouched, role still None.
            assert_eq!(name_str(&info.label), Some("base"));
            assert!(info.role.is_none());
        }
    }

    // =====================================================================
    // 4. SmallAriaInfo::to_full_info — basic + edge
    // =====================================================================

    #[test]
    fn small_to_full_info_basic() {
        let info = SmallAriaInfo::label("Submit")
            .with_role(AccessibilityRole::PushButton)
            .with_description("primary action")
            .to_full_info();
        assert_eq!(name_str(&info.accessibility_name), Some("Submit"));
        assert_eq!(info.role, AccessibilityRole::PushButton);
        assert_eq!(name_str(&info.description), Some("primary action"));
        assert!(info.accessibility_value.is_none());
        assert_eq!(info.states.len(), 0);
        assert_eq!(info.supported_actions.len(), 0);
        assert!(!info.is_live_region);
        assert!(info.labelled_by.is_none());
        assert!(info.described_by.is_none());
    }

    #[test]
    fn small_to_full_info_edge_missing_role_maps_to_unknown() {
        // No role set => full info must fall back to `Unknown`, never panic.
        let info = SmallAriaInfo::label("").to_full_info();
        assert_eq!(info.role, AccessibilityRole::Unknown);
        assert_eq!(name_str(&info.accessibility_name), Some(""));
        assert!(info.description.is_none());
    }

    // =====================================================================
    // 5. ProgressAriaInfo::create — no_panic_smoke
    // =====================================================================

    #[test]
    fn progress_create_no_panic_smoke() {
        for s in adversarial_strings() {
            let expected = s.clone();
            let p = ProgressAriaInfo::create(s.into());
            assert_eq!(name_str(&p.label), Some(expected.as_str()));
            // Documented defaults.
            assert!(p.current_value.is_none());
            assert!(p.max.is_none());
            assert!(!p.indeterminate);
            assert!(p.description.is_none());
        }
    }

    // =====================================================================
    // 6. ProgressAriaInfo::with_current_value — no_panic + invariants (numeric)
    // =====================================================================

    #[test]
    fn progress_with_current_value_numeric() {
        for v in adversarial_f32() {
            let p = ProgressAriaInfo::create("p".into()).with_current_value(v);
            match f32_of(&p.current_value) {
                Some(got) if v.is_nan() => assert!(got.is_nan()),
                Some(got) => assert_eq!(got, v),
                None => panic!("current_value should be Some after with_current_value"),
            }
            // to_full_info must not panic for any float, and (since determinate)
            // must emit a value string.
            let full = p.to_full_info();
            assert!(full.accessibility_value.is_some());
        }
    }

    // =====================================================================
    // 7. ProgressAriaInfo::with_max — no_panic + invariants (numeric)
    // =====================================================================

    #[test]
    fn progress_with_max_numeric() {
        for v in adversarial_f32() {
            let p = ProgressAriaInfo::create("p".into()).with_max(v);
            match f32_of(&p.max) {
                Some(got) if v.is_nan() => assert!(got.is_nan()),
                Some(got) => assert_eq!(got, v),
                None => panic!("max should be Some after with_max"),
            }
            // max does not influence the value string; current_value stays None.
            assert!(p.current_value.is_none());
        }
    }

    // =====================================================================
    // 8. ProgressAriaInfo::with_indeterminate — no_panic + invariants
    // =====================================================================

    #[test]
    fn progress_with_indeterminate_invariants() {
        for flag in [true, false] {
            let p = ProgressAriaInfo::create("p".into()).with_indeterminate(flag);
            assert_eq!(p.indeterminate, flag);
        }
        // indeterminate must override a present current_value in to_full_info.
        let p = ProgressAriaInfo::create("p".into())
            .with_current_value(0.5)
            .with_indeterminate(true);
        assert!(p.to_full_info().accessibility_value.is_none());
    }

    // =====================================================================
    // 9. ProgressAriaInfo::with_description — no_panic + invariants
    // =====================================================================

    #[test]
    fn progress_with_description_invariants() {
        for s in adversarial_strings() {
            let expected = s.clone();
            let p = ProgressAriaInfo::create("p".into()).with_description(s.into());
            assert_eq!(name_str(&p.description), Some(expected.as_str()));
            assert_eq!(name_str(&p.label), Some("p"));
        }
    }

    // =====================================================================
    // 10. ProgressAriaInfo::to_full_info — basic + edge
    // =====================================================================

    #[test]
    fn progress_to_full_info_basic() {
        let info = ProgressAriaInfo::create("Loading".into())
            .with_current_value(0.5)
            .to_full_info();
        assert_eq!(name_str(&info.accessibility_name), Some("Loading"));
        assert_eq!(info.role, AccessibilityRole::ProgressBar);
        assert_eq!(name_str(&info.accessibility_value), Some("0.5"));
        assert_eq!(info.states.len(), 0);
        assert_eq!(info.supported_actions.len(), 0);
    }

    #[test]
    fn progress_to_full_info_edge() {
        // No current value => value string is None.
        let info = ProgressAriaInfo::create("x".into()).to_full_info();
        assert!(info.accessibility_value.is_none());
        assert_eq!(info.role, AccessibilityRole::ProgressBar);

        // NaN / inf current values format to defined strings, no panic.
        assert_eq!(
            name_str(
                &ProgressAriaInfo::create("x".into())
                    .with_current_value(f32::NAN)
                    .to_full_info()
                    .accessibility_value
            ),
            Some("NaN")
        );
        assert_eq!(
            name_str(
                &ProgressAriaInfo::create("x".into())
                    .with_current_value(f32::INFINITY)
                    .to_full_info()
                    .accessibility_value
            ),
            Some("inf")
        );
        assert_eq!(
            name_str(
                &ProgressAriaInfo::create("x".into())
                    .with_current_value(f32::NEG_INFINITY)
                    .to_full_info()
                    .accessibility_value
            ),
            Some("-inf")
        );
    }

    // =====================================================================
    // 11. MeterAriaInfo::create — numeric (zero / min_max / negative / nan_inf)
    // =====================================================================

    #[test]
    fn meter_create_zero() {
        let m = MeterAriaInfo::create("z".into(), 0.0, 0.0, 0.0);
        assert_eq!(m.current_value, 0.0);
        assert_eq!(m.min, 0.0);
        assert_eq!(m.max, 0.0);
        assert_eq!(name_str(&m.to_full_info().accessibility_value), Some("0"));
    }

    #[test]
    fn meter_create_min_max() {
        let m = MeterAriaInfo::create("mm".into(), f32::MAX, f32::MIN, f32::MAX);
        assert_eq!(m.current_value, f32::MAX);
        assert_eq!(m.min, f32::MIN);
        assert_eq!(m.max, f32::MAX);
        // Formatting an extreme (but finite) float must not panic.
        assert!(m.to_full_info().accessibility_value.is_some());
    }

    #[test]
    fn meter_create_negative() {
        let m = MeterAriaInfo::create("neg".into(), -5.0, -10.0, -1.0);
        assert_eq!(m.current_value, -5.0);
        assert_eq!(m.min, -10.0);
        assert_eq!(m.max, -1.0);
        assert_eq!(name_str(&m.to_full_info().accessibility_value), Some("-5"));
        // Inverted range (min > max) is accepted verbatim; no panic, no clamping.
        let inv = MeterAriaInfo::create("inv".into(), 5.0, 100.0, 0.0);
        assert_eq!(inv.min, 100.0);
        assert_eq!(inv.max, 0.0);
        assert!(inv.to_full_info().accessibility_value.is_some());
    }

    #[test]
    fn meter_create_overflow_saturates_to_inf() {
        // f32 arithmetic saturates rather than panicking; feed the saturated
        // result straight in and confirm formatting stays defined.
        let over = f32::MAX * 2.0; // == +inf
        assert!(over.is_infinite());
        let m = MeterAriaInfo::create("o".into(), over, -over, over);
        assert_eq!(name_str(&m.to_full_info().accessibility_value), Some("inf"));
    }

    #[test]
    fn meter_create_nan_inf() {
        // NaN preserved as NaN, no panic constructing or formatting.
        let m = MeterAriaInfo::create("n".into(), f32::NAN, 0.0, 1.0);
        assert!(m.current_value.is_nan());
        assert_eq!(name_str(&m.to_full_info().accessibility_value), Some("NaN"));

        let pos = MeterAriaInfo::create("n".into(), f32::INFINITY, 0.0, 1.0);
        assert_eq!(
            name_str(&pos.to_full_info().accessibility_value),
            Some("inf")
        );

        let neg = MeterAriaInfo::create("n".into(), f32::NEG_INFINITY, 0.0, 1.0);
        assert_eq!(
            name_str(&neg.to_full_info().accessibility_value),
            Some("-inf")
        );

        // Non-finite bounds must not panic either.
        let bounds = MeterAriaInfo::create("n".into(), 0.5, f32::NEG_INFINITY, f32::INFINITY);
        assert!(bounds.min.is_infinite());
        assert!(bounds.max.is_infinite());
        assert!(bounds.to_full_info().accessibility_value.is_some());
    }

    // =====================================================================
    // 12-14. MeterAriaInfo::with_low / with_high / with_optimum — numeric invariants
    // =====================================================================

    #[test]
    fn meter_with_low_high_optimum_numeric() {
        for v in adversarial_f32() {
            let m = MeterAriaInfo::create("m".into(), 0.5, 0.0, 1.0)
                .with_low(v)
                .with_high(v)
                .with_optimum(v);
            for opt in [&m.low, &m.high, &m.optimum] {
                match f32_of(opt) {
                    Some(got) if v.is_nan() => assert!(got.is_nan()),
                    Some(got) => assert_eq!(got, v),
                    None => panic!("threshold should be Some after builder"),
                }
            }
            // Core value/range untouched by the threshold builders.
            assert_eq!(m.current_value, 0.5);
            assert_eq!(m.min, 0.0);
            assert_eq!(m.max, 1.0);
        }
    }

    // =====================================================================
    // 15. MeterAriaInfo::with_description — no_panic + invariants
    // =====================================================================

    #[test]
    fn meter_with_description_invariants() {
        for s in adversarial_strings() {
            let expected = s.clone();
            let m = MeterAriaInfo::create("m".into(), 1.0, 0.0, 2.0).with_description(s.into());
            assert_eq!(name_str(&m.description), Some(expected.as_str()));
            assert_eq!(m.current_value, 1.0);
        }
    }

    // =====================================================================
    // 16. MeterAriaInfo::to_full_info — basic + edge
    // =====================================================================

    #[test]
    fn meter_to_full_info_basic() {
        let info = MeterAriaInfo::create("Disk".into(), 42.0, 0.0, 100.0)
            .with_description("usage".into())
            .to_full_info();
        assert_eq!(name_str(&info.accessibility_name), Some("Disk"));
        assert_eq!(info.role, AccessibilityRole::Indicator);
        assert_eq!(name_str(&info.accessibility_value), Some("42"));
        assert_eq!(name_str(&info.description), Some("usage"));
        assert_eq!(info.states.len(), 0);
        assert_eq!(info.supported_actions.len(), 0);
    }

    #[test]
    fn meter_to_full_info_edge() {
        // Meter always emits a value string (unlike progress). Even for an
        // extreme/empty-label instance it must not panic.
        let info = MeterAriaInfo::create("".into(), f32::MIN, f32::MIN, f32::MAX).to_full_info();
        assert!(info.accessibility_value.is_some());
        assert_eq!(info.role, AccessibilityRole::Indicator);
        assert_eq!(name_str(&info.accessibility_name), Some(""));
    }

    // =====================================================================
    // 17. DialogAriaInfo::create — no_panic_smoke
    // =====================================================================

    #[test]
    fn dialog_create_no_panic_smoke() {
        for s in adversarial_strings() {
            let expected = s.clone();
            let d = DialogAriaInfo::create(s.into());
            assert_eq!(name_str(&d.label), Some(expected.as_str()));
            // Documented defaults: non-modal, role Dialog, no describers.
            assert!(!d.modal);
            assert_eq!(d.role, AccessibilityRole::Dialog);
            assert!(d.described_by.is_none());
            assert!(d.description.is_none());
        }
    }

    // =====================================================================
    // 18. DialogAriaInfo::with_modal — no_panic + invariants
    // =====================================================================

    #[test]
    fn dialog_with_modal_invariants() {
        for flag in [true, false] {
            let d = DialogAriaInfo::create("t".into()).with_modal(flag);
            assert_eq!(d.modal, flag);
            // Unrelated fields keep their defaults.
            assert_eq!(d.role, AccessibilityRole::Dialog);
            assert_eq!(name_str(&d.label), Some("t"));
        }
    }

    // =====================================================================
    // 19. DialogAriaInfo::with_described_by — no_panic + invariants
    // =====================================================================

    #[test]
    fn dialog_with_described_by_invariants() {
        for s in adversarial_strings() {
            let expected = s.clone();
            let d = DialogAriaInfo::create("t".into()).with_described_by(s.into());
            assert_eq!(name_str(&d.described_by), Some(expected.as_str()));
            assert_eq!(name_str(&d.label), Some("t"));
        }
    }

    // =====================================================================
    // 20. DialogAriaInfo::with_role — no_panic + invariants
    // =====================================================================

    #[test]
    fn dialog_with_role_invariants() {
        for role in representative_roles() {
            let d = DialogAriaInfo::create("t".into()).with_role(role);
            assert_eq!(d.role, role);
            assert!(!d.modal);
        }
        // to_full_info propagates the overridden role verbatim.
        let info = DialogAriaInfo::create("t".into())
            .with_role(AccessibilityRole::Alert)
            .to_full_info();
        assert_eq!(info.role, AccessibilityRole::Alert);
    }

    // =====================================================================
    // 21. DialogAriaInfo::with_description — no_panic + invariants
    // =====================================================================

    #[test]
    fn dialog_with_description_invariants() {
        for s in adversarial_strings() {
            let expected = s.clone();
            let d = DialogAriaInfo::create("t".into()).with_description(s.into());
            assert_eq!(name_str(&d.description), Some(expected.as_str()));
            assert_eq!(name_str(&d.label), Some("t"));
        }
    }

    // =====================================================================
    // 22. DialogAriaInfo::to_full_info — basic + edge
    // =====================================================================

    #[test]
    fn dialog_to_full_info_basic() {
        let info = DialogAriaInfo::create("Confirm".into())
            .with_modal(true)
            .with_role(AccessibilityRole::Alert)
            .with_described_by("body-node".into())
            .with_description("Are you sure?".into())
            .to_full_info();
        assert_eq!(name_str(&info.accessibility_name), Some("Confirm"));
        assert_eq!(info.role, AccessibilityRole::Alert);
        assert_eq!(name_str(&info.description), Some("Are you sure?"));
        // The string `described_by` node-ref is NOT propagated into the DomNodeId field.
        assert!(info.described_by.is_none());
        assert!(info.labelled_by.is_none());
        assert!(info.accessibility_value.is_none());
        assert_eq!(info.states.len(), 0);
        assert_eq!(info.supported_actions.len(), 0);
    }

    #[test]
    fn dialog_to_full_info_edge() {
        // Default (non-modal, empty) instance must convert without panic.
        let info = DialogAriaInfo::create("".into()).to_full_info();
        assert_eq!(info.role, AccessibilityRole::Dialog);
        assert_eq!(name_str(&info.accessibility_name), Some(""));
        assert!(info.description.is_none());
    }

    // #####################################################################
    // Appended: round-trip, total-order and FFI-vec coverage.
    //
    // The block above exercises the 22 listed builder/getter fns. What it
    // does NOT cover is the machinery those fns feed into: the FFI vec/option
    // wrappers, and the `Eq`/`Ord`/`Hash` impls that `AccessibilityInfo`
    // derives *through* f32-carrying payloads (`LogicalPosition`,
    // `FloatValue`). Those derives are where a total-order contract can
    // silently break, so they get the adversarial treatment here.
    // #####################################################################

    use core::hash::{Hash, Hasher};

    use crate::{
        dom::{DomId, DomNodeId},
        styled_dom::NodeHierarchyItemId,
    };

    /// FNV-1a. Hand-rolled rather than `DefaultHasher` so these tests still
    /// build when azul-core is compiled `--no-default-features` (i.e. `no_std`,
    /// where `std::collections::hash_map` does not exist).
    struct Fnv(u64);

    impl Default for Fnv {
        fn default() -> Self {
            Self(0xcbf2_9ce4_8422_2325) // offset basis
        }
    }

    impl Hasher for Fnv {
        fn finish(&self) -> u64 {
            self.0
        }
        fn write(&mut self, bytes: &[u8]) {
            for b in bytes {
                self.0 ^= u64::from(*b);
                self.0 = self.0.wrapping_mul(0x0000_0100_0000_01b3); // FNV prime
            }
        }
    }

    fn hash_of<T: Hash>(t: &T) -> u64 {
        let mut h = Fnv::default();
        t.hash(&mut h);
        h.finish()
    }

    /// Every `AccessibilityRole`, in declaration order.
    ///
    /// `Ord` is derived, so declaration order *is* the sort order — the tests
    /// below pin that. Kept in sync with the enum by `role_exhaustiveness_canary`.
    fn all_roles() -> Vec<AccessibilityRole> {
        use AccessibilityRole::*;
        vec![
            TitleBar,
            MenuBar,
            ScrollBar,
            Grip,
            Sound,
            Cursor,
            Caret,
            Alert,
            Window,
            Client,
            MenuPopup,
            MenuItem,
            Tooltip,
            Application,
            Document,
            Pane,
            Chart,
            Dialog,
            Border,
            Grouping,
            Separator,
            Toolbar,
            StatusBar,
            Table,
            ColumnHeader,
            RowHeader,
            Column,
            Row,
            Cell,
            Link,
            HelpBalloon,
            Character,
            List,
            ListItem,
            Outline,
            OutlineItem,
            PageTab,
            PropertyPage,
            Indicator,
            Graphic,
            StaticText,
            Text,
            PushButton,
            CheckButton,
            RadioButton,
            ComboBox,
            DropList,
            ProgressBar,
            Dial,
            HotkeyField,
            Slider,
            SpinButton,
            Diagram,
            Animation,
            Equation,
            ButtonDropdown,
            ButtonMenu,
            ButtonDropdownGrid,
            Whitespace,
            PageTabList,
            Clock,
            SplitButton,
            IpAddress,
            Nothing,
            Unknown,
        ]
    }

    /// Every `AccessibilityState`, in declaration order.
    fn all_states() -> Vec<AccessibilityState> {
        use AccessibilityState::*;
        vec![
            Unavailable,
            Selected,
            Focused,
            CheckedTrue,
            CheckedFalse,
            Readonly,
            Default,
            Expanded,
            Collapsed,
            Busy,
            Offscreen,
            Focusable,
            Selectable,
            Linked,
            Traversed,
            Multiselectable,
            Protected,
        ]
    }

    /// Exhaustive `match`es: if a variant is added upstream without being added
    /// to `all_roles()` / `all_states()`, this stops compiling. That is the
    /// point — it keeps the ordering tests below honest instead of letting them
    /// silently degrade into partial coverage.
    #[test]
    fn role_exhaustiveness_canary() {
        use AccessibilityRole::*;
        for r in all_roles() {
            let known = match r {
                TitleBar | MenuBar | ScrollBar | Grip | Sound | Cursor | Caret | Alert | Window
                | Client | MenuPopup | MenuItem | Tooltip | Application | Document | Pane
                | Chart | Dialog | Border | Grouping | Separator | Toolbar | StatusBar | Table
                | ColumnHeader | RowHeader | Column | Row | Cell | Link | HelpBalloon
                | Character | List | ListItem | Outline | OutlineItem | PageTab | PropertyPage
                | Indicator | Graphic | StaticText | Text | PushButton | CheckButton
                | RadioButton | ComboBox | DropList | ProgressBar | Dial | HotkeyField | Slider
                | SpinButton | Diagram | Animation | Equation | ButtonDropdown | ButtonMenu
                | ButtonDropdownGrid | Whitespace | PageTabList | Clock | SplitButton
                | IpAddress | Nothing | Unknown => true,
            };
            assert!(known);
        }

        use AccessibilityState::*;
        for s in all_states() {
            let known = match s {
                Unavailable | Selected | Focused | CheckedTrue | CheckedFalse | Readonly
                | Default | Expanded | Collapsed | Busy | Offscreen | Focusable | Selectable
                | Linked | Traversed | Multiselectable | Protected => true,
            };
            assert!(known);
        }
    }

    // =====================================================================
    // Total-order / Eq / Hash contracts on the plain C-like enums
    // =====================================================================

    #[test]
    fn role_ord_is_strict_declaration_order() {
        let roles = all_roles();
        // Strictly increasing => derived Ord follows declaration order AND the
        // list has no duplicates.
        for pair in roles.windows(2) {
            assert!(
                pair[0] < pair[1],
                "roles must sort in declaration order: {:?} !< {:?}",
                pair[0],
                pair[1]
            );
        }
        // Trichotomy: for every ordered pair exactly one of <, ==, > holds.
        for a in &roles {
            for b in &roles {
                let lt = a < b;
                let eq = a == b;
                let gt = a > b;
                assert_eq!(
                    u8::from(lt) + u8::from(eq) + u8::from(gt),
                    1,
                    "trichotomy violated for {a:?} vs {b:?}"
                );
            }
        }
        // Reflexivity + the documented endpoints.
        assert_eq!(roles[0], AccessibilityRole::TitleBar);
        assert_eq!(*roles.last().unwrap(), AccessibilityRole::Unknown);
        assert!(AccessibilityRole::TitleBar < AccessibilityRole::Unknown);
    }

    #[test]
    fn state_ord_is_strict_declaration_order() {
        let states = all_states();
        for pair in states.windows(2) {
            assert!(pair[0] < pair[1], "{:?} !< {:?}", pair[0], pair[1]);
        }
        // CheckedTrue / CheckedFalse are adjacent but must never compare equal —
        // aliasing them would make a checked and unchecked box indistinguishable.
        assert_ne!(
            AccessibilityState::CheckedTrue,
            AccessibilityState::CheckedFalse
        );
        assert_ne!(
            hash_of(&AccessibilityState::CheckedTrue),
            hash_of(&AccessibilityState::CheckedFalse)
        );
    }

    #[test]
    fn role_and_state_hash_agrees_with_eq() {
        // Eq => equal hashes (the direction the Hash contract actually requires),
        // and Hash is deterministic across calls.
        for r in all_roles() {
            let copy = r;
            assert_eq!(hash_of(&r), hash_of(&copy));
        }
        for s in all_states() {
            let copy = s;
            assert_eq!(hash_of(&s), hash_of(&copy));
        }

        // Stronger: no two distinct variants may collide. A collision here would
        // let two different roles/states alias as the same HashMap key. The
        // derive hashes the (necessarily distinct) discriminant, and FNV-1a's
        // multiply step is invertible mod 2^64, so distinctness is guaranteed —
        // this pins that no variant is ever given a duplicate discriminant.
        let role_hashes: Vec<u64> = all_roles().iter().map(hash_of).collect();
        for (i, a) in role_hashes.iter().enumerate() {
            for (j, b) in role_hashes.iter().enumerate() {
                assert_eq!(i == j, a == b, "role hash collision at {i}/{j}");
            }
        }

        let state_hashes: Vec<u64> = all_states().iter().map(hash_of).collect();
        for (i, a) in state_hashes.iter().enumerate() {
            for (j, b) in state_hashes.iter().enumerate() {
                assert_eq!(i == j, a == b, "state hash collision at {i}/{j}");
            }
        }
    }

    // =====================================================================
    // AccessibilityStateVec — FFI vec round-trip
    // =====================================================================

    #[test]
    fn state_vec_round_trips_through_ffi_wrapper() {
        let cases: Vec<Vec<AccessibilityState>> = vec![
            Vec::new(),
            vec![AccessibilityState::Focused],
            all_states(),
            // duplicates must survive verbatim (this is a Vec, not a Set)
            vec![
                AccessibilityState::Busy,
                AccessibilityState::Busy,
                AccessibilityState::Busy,
            ],
            // large allocation: the FFI wrapper owns the buffer, so this is the
            // shape most likely to trip a bad len/cap or double-free.
            std::iter::repeat_n(AccessibilityState::Selected, 10_000).collect(),
        ];

        for original in cases {
            let wrapped: AccessibilityStateVec = original.clone().into();

            // len / is_empty stay consistent with the source Vec.
            assert_eq!(wrapped.len(), original.len());
            assert_eq!(wrapped.is_empty(), original.is_empty());
            assert_eq!(wrapped.as_slice(), original.as_slice());
            assert_eq!(wrapped.iter().count(), original.len());

            // Clone must deep-copy: equal content, and dropping the clone must
            // not invalidate the original (both are dropped at end of scope).
            let cloned = wrapped.clone();
            assert_eq!(cloned.as_slice(), original.as_slice());
            assert_eq!(cloned, wrapped);
            assert_eq!(hash_of(&cloned), hash_of(&wrapped));
            drop(cloned);
            assert_eq!(wrapped.as_slice(), original.as_slice());

            // Round-trip back out: decode(encode(x)) == x.
            let back = wrapped.into_library_owned_vec();
            assert_eq!(back, original);
        }
    }

    #[test]
    fn state_vec_indexing_is_bounds_safe() {
        let v: AccessibilityStateVec = all_states().into();
        let len = v.len();

        for (i, expected) in all_states().into_iter().enumerate() {
            assert_eq!(v.get(i), Some(&expected));
        }
        // One-past-the-end and the pathological index must return None, not panic.
        assert_eq!(v.get(len), None);
        assert_eq!(v.get(len + 1), None);
        assert_eq!(v.get(usize::MAX), None);
        assert!(v.c_get(usize::MAX).is_none());
        assert!(v.c_get(len).is_none());
        assert!(v.c_get(0).is_some());

        // The empty vec has no valid index at all.
        let empty = AccessibilityStateVec::new();
        assert!(empty.is_empty());
        assert_eq!(empty.get(0), None);
        assert_eq!(empty.get(usize::MAX), None);
        assert!(empty.c_get(0).is_none());
    }

    #[test]
    fn state_vec_from_vec_preserves_order_len_and_lookup() {
        // The C-ABI vec is built from a Rust Vec and is then read-only — it has
        // no push/pop. Assert the round-trip is lossless and lookups agree.
        let empty = AccessibilityStateVec::new();
        assert_eq!(empty.len(), 0);
        assert!(empty.is_empty());
        assert_eq!(empty.get(0), None);

        let states = all_states();
        let v = AccessibilityStateVec::from_vec(states.clone());
        assert_eq!(v.len(), states.len());
        assert!(!v.is_empty());
        assert!(v.capacity() >= v.len(), "capacity must never trail len");

        // Order is preserved and every index is reachable.
        assert_eq!(v.as_slice(), states.as_slice());
        for (i, s) in states.iter().enumerate() {
            assert_eq!(v.get(i), Some(s));
        }
        assert_eq!(
            v.get(states.len()),
            None,
            "out-of-bounds must be None, not a panic"
        );
        assert!(v.iter().eq(states.iter()));
    }

    // =====================================================================
    // AccessibilityAction — payload-carrying variants
    // =====================================================================

    /// One instance of every `AccessibilityAction` variant, in declaration order.
    fn all_actions() -> Vec<AccessibilityAction> {
        use AccessibilityAction::*;
        vec![
            Default,
            Focus,
            Blur,
            Collapse,
            Expand,
            ScrollIntoView,
            Increment,
            Decrement,
            ShowContextMenu,
            HideTooltip,
            ShowTooltip,
            ScrollUp,
            ScrollDown,
            ScrollLeft,
            ScrollRight,
            ReplaceSelectedText("replacement".into()),
            ScrollToPoint(LogicalPosition::new(1.0, 2.0)),
            SetScrollOffset(LogicalPosition::new(-3.0, 4.0)),
            SetTextSelection(TextSelectionStartEnd {
                selection_start: 0,
                selection_end: 5,
            }),
            SetSequentialFocusNavigationStartingPoint,
            SetValue("value".into()),
            SetNumericValue(FloatValue::new(1.5)),
            CustomAction(42),
        ]
    }

    #[test]
    fn action_vec_round_trips_with_payloads() {
        let original = all_actions();
        let wrapped: AccessibilityActionVec = original.clone().into();

        assert_eq!(wrapped.len(), original.len());
        assert_eq!(wrapped.as_slice(), original.as_slice());

        // The payload variants own heap data (AzString). Cloning must deep-copy;
        // dropping the clone must leave the original intact (no double-free).
        let cloned = wrapped.clone();
        assert_eq!(cloned, wrapped);
        assert_eq!(hash_of(&cloned), hash_of(&wrapped));
        drop(cloned);
        assert_eq!(wrapped.as_slice(), original.as_slice());

        let back = wrapped.into_library_owned_vec();
        assert_eq!(back, original);

        // Variant order dominates payload in the derived Ord.
        for pair in original.windows(2) {
            assert!(pair[0] < pair[1], "{:?} !< {:?}", pair[0], pair[1]);
        }
    }

    #[test]
    fn action_string_payloads_survive_adversarial_strings() {
        for s in adversarial_strings() {
            let expected = s.clone();

            let replace = AccessibilityAction::ReplaceSelectedText(s.clone().into());
            let set = AccessibilityAction::SetValue(s.into());

            // Payload preserved verbatim — including interior NUL and lone
            // combining marks, which a C-string round-trip would truncate.
            match &replace {
                AccessibilityAction::ReplaceSelectedText(got) => {
                    assert_eq!(got.as_str(), expected.as_str());
                    assert_eq!(got.as_str().len(), expected.len());
                }
                other => panic!("wrong variant: {other:?}"),
            }
            match &set {
                AccessibilityAction::SetValue(got) => assert_eq!(got.as_str(), expected.as_str()),
                other => panic!("wrong variant: {other:?}"),
            }

            // Clone/Eq/Hash agree even for the pathological payloads.
            assert_eq!(replace.clone(), replace);
            assert_eq!(hash_of(&replace.clone()), hash_of(&replace));
            // Different variants with the *same* payload must never alias.
            assert_ne!(replace, set);
        }
    }

    #[test]
    fn action_custom_action_i32_limits() {
        let min = AccessibilityAction::CustomAction(i32::MIN);
        let zero = AccessibilityAction::CustomAction(0);
        let max = AccessibilityAction::CustomAction(i32::MAX);

        // Signed ordering, not a bit-pattern/unsigned ordering.
        assert!(min < zero, "i32::MIN must sort below 0");
        assert!(zero < max);
        assert!(min < max);

        assert_eq!(min, AccessibilityAction::CustomAction(i32::MIN));
        assert_ne!(min, max);
        assert_eq!(
            hash_of(&min),
            hash_of(&AccessibilityAction::CustomAction(i32::MIN))
        );

        // -1 must not alias u32::MAX-style onto anything.
        assert_ne!(
            AccessibilityAction::CustomAction(-1),
            AccessibilityAction::CustomAction(i32::MAX)
        );
    }

    #[test]
    fn text_selection_start_end_limits() {
        // usize::MAX bounds: constructing and comparing must not overflow.
        let huge = TextSelectionStartEnd {
            selection_start: usize::MAX,
            selection_end: usize::MAX,
        };
        assert_eq!(huge.selection_start, usize::MAX);
        assert_eq!(huge.selection_end, usize::MAX);
        assert_eq!(huge, huge);

        // Inverted range (start > end) is accepted verbatim — the type does not
        // normalise or clamp, so downstream consumers must not assume start<=end.
        let inverted = TextSelectionStartEnd {
            selection_start: 10,
            selection_end: 0,
        };
        assert_eq!(inverted.selection_start, 10);
        assert_eq!(inverted.selection_end, 0);
        assert_ne!(
            inverted,
            TextSelectionStartEnd {
                selection_start: 0,
                selection_end: 10,
            }
        );

        // Collapsed (zero-length) selection is distinct from an empty-at-zero one.
        let collapsed = TextSelectionStartEnd {
            selection_start: 7,
            selection_end: 7,
        };
        assert_ne!(
            collapsed,
            TextSelectionStartEnd {
                selection_start: 0,
                selection_end: 0,
            }
        );

        // Ord is lexicographic (start, then end).
        let a = TextSelectionStartEnd {
            selection_start: 1,
            selection_end: 99,
        };
        let b = TextSelectionStartEnd {
            selection_start: 2,
            selection_end: 0,
        };
        assert!(a < b, "selection_start must dominate the ordering");

        // Wrapped in the action, the same invariants hold.
        let action = AccessibilityAction::SetTextSelection(huge);
        assert_eq!(action.clone(), action);
        assert_eq!(hash_of(&action.clone()), hash_of(&action));
    }

    // =====================================================================
    // f32-carrying payloads: the Eq/Ord/Hash total-order contract
    //
    // `AccessibilityAction` *derives* Eq + Ord + Hash while carrying
    // `LogicalPosition` (two f32s) and `FloatValue`. f32 is not Eq/Ord, so
    // those inner types must supply total impls. These tests pin the actual
    // behaviour at NaN / inf / overflow, where a naive impl breaks the
    // reflexivity (a == a) that HashMap and BTreeMap rely on.
    // =====================================================================

    #[test]
    fn scroll_to_point_nan_is_reflexive_and_totally_ordered() {
        let nan = AccessibilityAction::ScrollToPoint(LogicalPosition::new(f32::NAN, f32::NAN));
        let origin = AccessibilityAction::ScrollToPoint(LogicalPosition::new(0.0, 0.0));

        // Reflexivity: `Eq` promises a == a. Raw f32 PartialEq would return
        // false here and quietly corrupt any HashMap keyed on this action.
        assert_eq!(nan, nan.clone());
        assert_eq!(hash_of(&nan), hash_of(&nan.clone()));
        assert_eq!(nan.cmp(&nan.clone()), core::cmp::Ordering::Equal);

        // NaN must NOT alias onto the origin (LogicalPosition::quantize maps NaN
        // to a dedicated i64::MIN sentinel precisely to avoid that collision).
        assert_ne!(nan, origin);
        assert_ne!(hash_of(&nan), hash_of(&origin));
        assert!(nan < origin, "NaN sorts below every real coordinate");

        // Ord is total: every pair of these is comparable and antisymmetric.
        let neg = AccessibilityAction::ScrollToPoint(LogicalPosition::new(-1.0, -1.0));
        let pos = AccessibilityAction::ScrollToPoint(LogicalPosition::new(1.0, 1.0));
        let mut sorted = vec![pos.clone(), origin.clone(), nan.clone(), neg.clone()];
        sorted.sort();
        assert_eq!(sorted, vec![nan, neg, origin, pos]);
    }

    #[test]
    fn scroll_to_point_infinite_coords_saturate_without_panic() {
        let inf = AccessibilityAction::SetScrollOffset(LogicalPosition::new(
            f32::INFINITY,
            f32::NEG_INFINITY,
        ));
        let finite = AccessibilityAction::SetScrollOffset(LogicalPosition::new(1.0, 1.0));

        // Defined, reflexive, no panic on the fixed-point conversion.
        assert_eq!(inf, inf.clone());
        assert_eq!(hash_of(&inf), hash_of(&inf.clone()));
        assert!(
            inf > finite,
            "+inf x-coordinate must sort above a finite one"
        );

        // Documented saturation: the fixed-point quantisation clamps, so
        // f32::MAX and +inf land in the same bucket. Asserted so a future
        // change to the quantiser has to consciously break this.
        let max = AccessibilityAction::SetScrollOffset(LogicalPosition::new(f32::MAX, f32::MAX));
        let plus_inf = AccessibilityAction::SetScrollOffset(LogicalPosition::new(
            f32::INFINITY,
            f32::INFINITY,
        ));
        assert_eq!(
            max, plus_inf,
            "f32::MAX and +inf both saturate to the same quantised coordinate"
        );
    }

    #[test]
    fn set_numeric_value_float_edges_are_defined() {
        // Representable-under-quantisation values round-trip exactly
        // (FloatValue is fixed-point with a 1/1000 quantum).
        for v in [0.0_f32, 1.5, -1.5, 2.25, -3.75, 1000.0] {
            let f = FloatValue::new(v);
            assert_eq!(f.get(), v, "FloatValue must round-trip {v}");
            let action = AccessibilityAction::SetNumericValue(f);
            assert_eq!(action.clone(), action);
        }

        // Non-finite input must not panic. `as isize` saturates, so:
        assert_eq!(FloatValue::new(f32::INFINITY).number(), isize::MAX);
        assert_eq!(FloatValue::new(f32::NEG_INFINITY).number(), isize::MIN);

        // NOTE (reported, not a weakened assertion): FloatValue::new maps NaN to
        // 0 via a raw `as isize` cast, so a NaN numeric value is INDISTINGUISHABLE
        // from 0.0. LogicalPosition::quantize explicitly fixed this same aliasing
        // (NaN -> i64::MIN sentinel); FloatValue still has it. Pinning the current
        // behaviour so the aliasing is visible and a fix has to update this test.
        assert_eq!(FloatValue::new(f32::NAN).number(), 0);
        assert_eq!(
            AccessibilityAction::SetNumericValue(FloatValue::new(f32::NAN)),
            AccessibilityAction::SetNumericValue(FloatValue::new(0.0)),
            "KNOWN ALIASING: NaN numeric value collides with 0.0"
        );

        // Reflexivity still holds for the NaN case (it is Eq-safe, just aliased).
        let nan_action = AccessibilityAction::SetNumericValue(FloatValue::new(f32::NAN));
        assert_eq!(hash_of(&nan_action), hash_of(&nan_action.clone()));

        // f32::MAX overflows the fixed-point scale and saturates rather than wrapping.
        assert_eq!(FloatValue::new(f32::MAX).number(), isize::MAX);
        assert_eq!(FloatValue::new(f32::MIN).number(), isize::MIN);
    }

    // =====================================================================
    // Float -> value-string encoding: format/parse round-trip
    // =====================================================================

    #[test]
    fn progress_value_string_round_trips_through_parse() {
        for v in adversarial_f32() {
            let full = ProgressAriaInfo::create("p".into())
                .with_current_value(v)
                .to_full_info();
            let s = name_str(&full.accessibility_value).expect("determinate => Some");

            if v.is_nan() {
                assert_eq!(s, "NaN");
                assert!(s.parse::<f32>().unwrap().is_nan());
            } else if v.is_infinite() {
                assert_eq!(s, if v > 0.0 { "inf" } else { "-inf" });
            } else {
                // Display for f32 is shortest-round-trip: decode(encode(v)) == v.
                let parsed: f32 = s.parse().expect("emitted value string must re-parse");
                assert_eq!(parsed, v, "round-trip failed for {v} via {s:?}");
                if v != 0.0 {
                    // Bit-exact for everything except +0.0/-0.0, which compare
                    // equal under `==` by definition.
                    assert_eq!(parsed.to_bits(), v.to_bits(), "lossy round-trip for {v}");
                }
            }
        }
    }

    #[test]
    fn meter_value_string_round_trips_through_parse() {
        for v in adversarial_f32() {
            let full = MeterAriaInfo::create("m".into(), v, 0.0, 1.0).to_full_info();
            // Meter ALWAYS emits a value string (unlike progress).
            let s = name_str(&full.accessibility_value).expect("meter always emits a value");

            if v.is_nan() {
                assert_eq!(s, "NaN");
            } else if v.is_infinite() {
                assert_eq!(s, if v > 0.0 { "inf" } else { "-inf" });
            } else {
                let parsed: f32 = s.parse().expect("emitted value string must re-parse");
                assert_eq!(parsed, v);
                if v != 0.0 {
                    assert_eq!(parsed.to_bits(), v.to_bits());
                }
            }
        }
    }

    // =====================================================================
    // Builder algebra: purity, idempotence, last-write-wins, order-independence
    // =====================================================================

    #[test]
    fn to_full_info_is_pure_and_idempotent() {
        let small = SmallAriaInfo::label("s")
            .with_role(AccessibilityRole::Slider)
            .with_description("d");
        let progress = ProgressAriaInfo::create("p".into())
            .with_current_value(0.25)
            .with_max(10.0);
        let meter = MeterAriaInfo::create("m".into(), 1.0, 0.0, 2.0).with_low(0.5);
        let dialog = DialogAriaInfo::create("d".into()).with_modal(true);

        // &self getters must not mutate the receiver, and must be deterministic:
        // f(x) == f(x) for repeated calls.
        let (s0, p0, m0, d0) = (
            small.clone(),
            progress.clone(),
            meter.clone(),
            dialog.clone(),
        );

        assert_eq!(small.to_full_info(), small.to_full_info());
        assert_eq!(progress.to_full_info(), progress.to_full_info());
        assert_eq!(meter.to_full_info(), meter.to_full_info());
        assert_eq!(dialog.to_full_info(), dialog.to_full_info());

        assert_eq!(small, s0, "to_full_info must not mutate SmallAriaInfo");
        assert_eq!(
            progress, p0,
            "to_full_info must not mutate ProgressAriaInfo"
        );
        assert_eq!(meter, m0, "to_full_info must not mutate MeterAriaInfo");
        assert_eq!(dialog, d0, "to_full_info must not mutate DialogAriaInfo");

        // Idempotent even when the value is NaN — the AccessibilityInfo carries a
        // *string* ("NaN"), which is Eq-comparable, so this holds where a raw f32
        // comparison would not.
        let nan_meter = MeterAriaInfo::create("m".into(), f32::NAN, 0.0, 1.0);
        assert_eq!(nan_meter.to_full_info(), nan_meter.to_full_info());
    }

    #[test]
    fn progress_max_is_never_surfaced_in_full_info() {
        // `max` has no representation in AccessibilityInfo, so setting it to
        // anything at all — including inf/NaN — must not perturb the conversion.
        let baseline = ProgressAriaInfo::create("p".into())
            .with_current_value(0.5)
            .to_full_info();

        for v in adversarial_f32() {
            let with_max = ProgressAriaInfo::create("p".into())
                .with_current_value(0.5)
                .with_max(v)
                .to_full_info();
            assert_eq!(with_max, baseline, "with_max({v}) leaked into to_full_info");
        }
    }

    #[test]
    fn meter_threshold_builders_are_order_independent_and_last_write_wins() {
        // Order-independence: the three threshold setters touch disjoint fields.
        let a = MeterAriaInfo::create("m".into(), 0.5, 0.0, 1.0)
            .with_low(0.1)
            .with_high(0.9)
            .with_optimum(0.7);
        let b = MeterAriaInfo::create("m".into(), 0.5, 0.0, 1.0)
            .with_optimum(0.7)
            .with_high(0.9)
            .with_low(0.1);
        assert_eq!(a, b);

        // Last-write-wins, including when the second write is a non-finite value.
        let m = MeterAriaInfo::create("m".into(), 0.5, 0.0, 1.0)
            .with_low(0.1)
            .with_low(f32::INFINITY);
        assert_eq!(f32_of(&m.low), Some(f32::INFINITY));

        // Nonsensical-but-accepted config: low > high, optimum outside [min,max].
        // The type performs no validation; assert it stores them verbatim rather
        // than silently clamping (downstream code must do its own validation).
        let weird = MeterAriaInfo::create("m".into(), 5.0, 0.0, 1.0)
            .with_low(100.0)
            .with_high(-100.0)
            .with_optimum(-1.0);
        assert_eq!(f32_of(&weird.low), Some(100.0));
        assert_eq!(f32_of(&weird.high), Some(-100.0));
        assert_eq!(f32_of(&weird.optimum), Some(-1.0));
        assert_eq!(weird.current_value, 5.0); // out of [min,max], not clamped
        assert!(weird.to_full_info().accessibility_value.is_some());
    }

    #[test]
    fn progress_and_dialog_builders_last_write_wins() {
        let p = ProgressAriaInfo::create("p".into())
            .with_current_value(1.0)
            .with_current_value(2.0)
            .with_indeterminate(true)
            .with_indeterminate(false)
            .with_description("a".into())
            .with_description("b".into());
        assert_eq!(f32_of(&p.current_value), Some(2.0));
        assert!(!p.indeterminate);
        assert_eq!(name_str(&p.description), Some("b"));
        // Not indeterminate => the (last) current value is surfaced.
        assert_eq!(name_str(&p.to_full_info().accessibility_value), Some("2"));

        let d = DialogAriaInfo::create("d".into())
            .with_modal(true)
            .with_modal(false)
            .with_role(AccessibilityRole::Alert)
            .with_role(AccessibilityRole::Dialog)
            .with_described_by("x".into())
            .with_described_by("y".into());
        assert!(!d.modal);
        assert_eq!(d.role, AccessibilityRole::Dialog);
        assert_eq!(name_str(&d.described_by), Some("y"));
    }

    // =====================================================================
    // AccessibilityInfo — the fully-populated aggregate
    // =====================================================================

    fn full_info_fixture() -> AccessibilityInfo {
        AccessibilityInfo {
            accessibility_name: OptionString::Some("name".into()),
            accessibility_value: OptionString::Some("value".into()),
            description: OptionString::Some("desc".into()),
            accelerator: OptionVirtualKeyCodeCombo::None,
            default_action: OptionString::Some("activate".into()),
            states: all_states().into(),
            supported_actions: all_actions().into(),
            labelled_by: OptionDomNodeId::Some(DomNodeId::ROOT),
            described_by: OptionDomNodeId::Some(DomNodeId {
                dom: DomId { inner: 3 },
                node: NodeHierarchyItemId::from_raw(7),
            }),
            role: AccessibilityRole::PushButton,
            is_live_region: true,
        }
    }

    #[test]
    fn full_info_clone_eq_hash_ord_are_consistent() {
        let a = full_info_fixture();
        let b = a.clone();

        // Deep clone: equal, equally hashed, mutually Equal under Ord.
        assert_eq!(a, b);
        assert_eq!(hash_of(&a), hash_of(&b));
        assert_eq!(a.cmp(&b), core::cmp::Ordering::Equal);

        // The clone owns its own heap buffers — dropping it must leave `a` intact.
        drop(b);
        assert_eq!(a.states.len(), all_states().len());
        assert_eq!(a.supported_actions.len(), all_actions().len());
        assert_eq!(name_str(&a.accessibility_name), Some("name"));

        // Perturbing any single field must break equality (no field is ignored
        // by the derived PartialEq — a field silently dropped from the derive
        // would let two different a11y nodes compare equal).
        let mut differs = a.clone();
        differs.is_live_region = false;
        assert_ne!(a, differs);

        let mut differs = a.clone();
        differs.role = AccessibilityRole::Unknown;
        assert_ne!(a, differs);

        let mut differs = a.clone();
        differs.labelled_by = OptionDomNodeId::None;
        assert_ne!(a, differs);

        let mut differs = a.clone();
        differs.states = Vec::new().into();
        assert_ne!(a, differs);

        let mut differs = a.clone();
        differs.supported_actions = Vec::new().into();
        assert_ne!(a, differs);

        let mut differs = a.clone();
        differs.default_action = OptionString::None;
        assert_ne!(a, differs);
    }

    // =====================================================================
    // Option<T> FFI wrappers — Some/None round-trip
    // =====================================================================

    #[test]
    fn option_wrappers_round_trip() {
        // Copy payloads.
        for r in all_roles() {
            let opt = OptionAccessibilityRole::Some(r);
            assert!(opt.is_some());
            assert!(!opt.is_none());
            assert_eq!(opt.as_ref(), Some(&r));
            assert_eq!(opt.into_option(), Some(r));
        }
        assert!(OptionAccessibilityRole::None.is_none());
        assert_eq!(OptionAccessibilityRole::None.into_option(), None);

        for s in all_states() {
            assert_eq!(OptionAccessibilityState::Some(s).into_option(), Some(s));
        }
        assert_eq!(OptionAccessibilityState::None.into_option(), None);

        // Non-Copy payloads (heap-owning) must round-trip without a double-free.
        for a in all_actions() {
            let opt = OptionAccessibilityAction::Some(a.clone());
            assert!(opt.is_some());
            assert_eq!(opt.into_option(), Some(a));
        }
        assert!(OptionAccessibilityAction::None.is_none());

        let small = SmallAriaInfo::label("s").with_role(AccessibilityRole::Link);
        assert_eq!(
            OptionSmallAriaInfo::Some(small.clone()).into_option(),
            Some(small)
        );
        assert!(OptionSmallAriaInfo::None.is_none());

        let progress = ProgressAriaInfo::create("p".into()).with_current_value(0.5);
        assert_eq!(
            OptionProgressAriaInfo::Some(progress.clone()).into_option(),
            Some(progress)
        );

        let meter = MeterAriaInfo::create("m".into(), 1.0, 0.0, 2.0);
        assert_eq!(
            OptionMeterAriaInfo::Some(meter.clone()).into_option(),
            Some(meter)
        );

        let dialog = DialogAriaInfo::create("d".into()).with_modal(true);
        assert_eq!(
            OptionDialogAriaInfo::Some(dialog.clone()).into_option(),
            Some(dialog)
        );

        // The big aggregate, which owns two FFI vecs.
        let info = full_info_fixture();
        assert_eq!(
            OptionAccessibilityInfo::Some(info.clone()).into_option(),
            Some(info)
        );
        assert!(OptionAccessibilityInfo::None.is_none());
    }
}
