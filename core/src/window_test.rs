#[allow(unused_imports)]
pub use super::*;
#[cfg(test)]
#[allow(clippy::float_cmp)] // exact-value assertions on hidpi scale factors
mod audit_tests {
    use super::*;

    #[test]
    fn hidpi_factor_guards_zero_dpi() {
        // dpi == 0 must not produce a 0.0 scale factor (later divide-by-zero
        // in to_logical); it falls back to 96 DPI (scale 1.0).
        let ws = WindowSize {
            dpi: 0,
            ..WindowSize::default()
        };
        let factor = ws.get_hidpi_factor().inner.get();
        assert_eq!(factor, 1.0);

        let ws2 = WindowSize {
            dpi: 192,
            ..WindowSize::default()
        };
        assert_eq!(ws2.get_hidpi_factor().inner.get(), 2.0);
    }

    #[test]
    fn virtual_keycode_from_u32_roundtrips() {
        // A representative spread across the enum, including first/last.
        for vk in [
            VirtualKeyCode::Key1,
            VirtualKeyCode::A,
            VirtualKeyCode::Z,
            VirtualKeyCode::Left,
            VirtualKeyCode::Back,
            VirtualKeyCode::Delete,
            VirtualKeyCode::LControl,
            VirtualKeyCode::Cut,
        ] {
            assert_eq!(VirtualKeyCode::from_u32(vk as u32), Some(vk));
        }
        // Out of range -> None (no UB, no panic).
        assert_eq!(VirtualKeyCode::from_u32(10_000), None);
    }
}

#[cfg(test)]
#[allow(clippy::float_cmp)] // exact-value assertions on saturating float->int conversions
mod autotest_generated {
    use alloc::{format, string::String, vec};

    use super::*;

    /// Highest valid `VirtualKeyCode` discriminant (`Cut`, the last declared variant).
    const LAST_VK: u32 = 162;

    /// Deterministic, `no_std`-safe hasher so hash/eq consistency can be checked
    /// without pulling in `std::collections::hash_map::DefaultHasher`.
    #[derive(Default)]
    struct TestHasher(u64);

    impl Hasher for TestHasher {
        fn write(&mut self, bytes: &[u8]) {
            for &b in bytes {
                self.0 = self.0.rotate_left(5) ^ u64::from(b);
            }
        }
        fn finish(&self) -> u64 {
            self.0
        }
    }

    fn hash_of<T: Hash>(value: &T) -> u64 {
        let mut h = TestHasher::default();
        value.hash(&mut h);
        h.finish()
    }

    fn keyboard_with(keys: &[VirtualKeyCode]) -> KeyboardState {
        KeyboardState {
            pressed_virtual_keycodes: keys.to_vec().into(),
            ..KeyboardState::default()
        }
        ..Default::default()
    }

    fn pair(key: &str, value: &str) -> AzStringPair {
        AzStringPair {
            key: key.into(),
            value: value.into(),
        }
    }

    // ---------------------------------------------------------------------
    // Constructors: WindowId / IconKey (atomic counters)
    // ---------------------------------------------------------------------

    #[test]
    fn window_id_new_is_unique_and_monotonic() {
        // The counter is process-global and shared with other tests in this
        // binary, so only *relative* properties may be asserted.
        let mut ids = BTreeSet::new();
        let mut prev = WindowId::new();
        assert!(ids.insert(prev));
        for _ in 0..1000 {
            let next = WindowId::new();
            assert!(next.id > prev.id, "WindowId counter must strictly increase");
            assert!(ids.insert(next), "WindowId::new() handed out a duplicate");
            prev = next;
        }
        // Default delegates to new(): two defaults are never the same window.
        assert_ne!(WindowId::default(), WindowId::default());
    }

    #[test]
    fn icon_key_new_is_unique_and_monotonic() {
        let mut keys = BTreeSet::new();
        let mut prev = IconKey::new();
        assert!(keys.insert(prev));
        for _ in 0..1000 {
            let next = IconKey::new();
            assert!(
                next.icon_id > prev.icon_id,
                "IconKey counter must strictly increase"
            );
            assert!(keys.insert(next), "IconKey::new() handed out a duplicate");
            prev = next;
        }
        assert_ne!(IconKey::default(), IconKey::default());
    }

    // ---------------------------------------------------------------------
    // RendererOptions + Vsync / Srgb / HwAcceleration predicates
    // ---------------------------------------------------------------------

    #[test]
    fn renderer_options_new_preserves_every_combination() {
        let vsyncs = [Vsync::Enabled, Vsync::Disabled, Vsync::DontCare];
        let srgbs = [Srgb::Enabled, Srgb::Disabled, Srgb::DontCare];
        let accels = [
            HwAcceleration::Enabled,
            HwAcceleration::Disabled,
            HwAcceleration::DontCare,
        ];
        for v in vsyncs {
            for s in srgbs {
                for a in accels {
                    let o = RendererOptions::new(v, s, a);
                    assert_eq!(o.vsync, v);
                    assert_eq!(o.srgb, s);
                    assert_eq!(o.hw_accel, a);
                    // Constructed value must round-trip through equality/copy.
                    assert_eq!(o, RendererOptions::new(v, s, a));
                }
            }
        }
    }

    #[test]
    fn renderer_options_default_does_not_force_cpu_rendering() {
        // Regression guard: hw_accel must be DontCare (auto), NOT Disabled -
        // Disabled silently forced every app into CPU rendering.
        let d = RendererOptions::default();
        assert_eq!(d.hw_accel, HwAcceleration::DontCare);
        assert!(!d.hw_accel.is_enabled());
        assert!(d.vsync.is_enabled());
        assert!(!d.srgb.is_enabled());
    }

    #[test]
    fn tri_state_is_enabled_only_for_enabled_variant() {
        assert!(Vsync::Enabled.is_enabled());
        assert!(!Vsync::Disabled.is_enabled());
        assert!(!Vsync::DontCare.is_enabled());

        assert!(Srgb::Enabled.is_enabled());
        assert!(!Srgb::Disabled.is_enabled());
        assert!(!Srgb::DontCare.is_enabled());

        assert!(HwAcceleration::Enabled.is_enabled());
        assert!(!HwAcceleration::Disabled.is_enabled());
        assert!(!HwAcceleration::DontCare.is_enabled());
    }

    // ---------------------------------------------------------------------
    // KeyboardState getters / predicates
    // ---------------------------------------------------------------------

    #[test]
    fn keyboard_state_default_has_no_key_down() {
        let k = KeyboardState::default();
        assert!(!k.shift_down());
        assert!(!k.ctrl_down());
        assert!(!k.alt_down());
        assert!(!k.super_down());
        assert!(!k.primary_down());
        // Every single keycode reports "not down" on an empty state.
        for v in 0..=LAST_VK {
            let vk = VirtualKeyCode::from_u32(v).expect("discriminant in range");
            assert!(!k.is_key_down(vk));
        }
    }

    #[test]
    fn keyboard_state_modifiers_accept_either_side() {
        for (left, right, probe) in [
            (
                VirtualKeyCode::LShift,
                VirtualKeyCode::RShift,
                KeyboardState::shift_down as fn(&KeyboardState) -> bool,
            ),
            (
                VirtualKeyCode::LControl,
                VirtualKeyCode::RControl,
                KeyboardState::ctrl_down as fn(&KeyboardState) -> bool,
            ),
            (
                VirtualKeyCode::LAlt,
                VirtualKeyCode::RAlt,
                KeyboardState::alt_down as fn(&KeyboardState) -> bool,
            ),
            (
                VirtualKeyCode::LWin,
                VirtualKeyCode::RWin,
                KeyboardState::super_down as fn(&KeyboardState) -> bool,
            ),
        ] {
            assert!(probe(&keyboard_with(&[left])), "left variant must register");
            assert!(
                probe(&keyboard_with(&[right])),
                "right variant must register"
            );
            assert!(probe(&keyboard_with(&[left, right])));
            // An unrelated key must not light up a modifier.
            assert!(!probe(&keyboard_with(&[VirtualKeyCode::A])));
        }
    }

    #[test]
    fn keyboard_state_primary_down_follows_platform() {
        let ctrl = keyboard_with(&[VirtualKeyCode::LControl]);
        let cmd = keyboard_with(&[VirtualKeyCode::LWin]);
        if cfg!(target_os = "macos") {
            assert!(cmd.primary_down(), "Cmd (super) is PRIMARY on macOS");
            assert!(!ctrl.primary_down());
        } else {
            assert!(ctrl.primary_down(), "Ctrl is PRIMARY off macOS");
            assert!(!cmd.primary_down());
        }
        // On every platform, primary_down agrees with one of the two modifiers.
        for k in [&ctrl, &cmd, &KeyboardState::default()] {
            assert_eq!(
                k.primary_down(),
                if cfg!(target_os = "macos") {
                    k.super_down()
                } else {
                    k.ctrl_down()
                }
            );
        }
    }

    #[test]
    fn is_key_down_handles_duplicates_and_large_state() {
        // Same key pressed many times (backends can push duplicates).
        let dup = keyboard_with(&[VirtualKeyCode::S; 512]);
        assert!(dup.is_key_down(VirtualKeyCode::S));
        assert!(!dup.is_key_down(VirtualKeyCode::A));

        // Every key held down at once: no panic, all report true.
        let all: alloc::vec::Vec<VirtualKeyCode> = (0..=LAST_VK)
            .map(|v| VirtualKeyCode::from_u32(v).expect("discriminant in range"))
            .collect();
        let everything = keyboard_with(&all);
        for vk in &all {
            assert!(everything.is_key_down(*vk));
        }
        assert!(everything.shift_down() && everything.ctrl_down());
        assert!(everything.alt_down() && everything.super_down());
    }

    // ---------------------------------------------------------------------
    // AcceleratorKey / matches_accelerator
    // ---------------------------------------------------------------------

    #[test]
    fn empty_chord_matches_trivially() {
        // Documented: "An empty chord matches trivially."
        assert!(KeyboardState::default().matches_accelerator(&[]));
        assert!(keyboard_with(&[VirtualKeyCode::A]).matches_accelerator(&[]));
    }

    #[test]
    fn matches_accelerator_requires_every_entry() {
        let state = keyboard_with(&[
            VirtualKeyCode::LControl,
            VirtualKeyCode::LShift,
            VirtualKeyCode::S,
        ]);
        assert!(state.matches_accelerator(&[
            AcceleratorKey::Ctrl,
            AcceleratorKey::Shift,
            AcceleratorKey::Key(VirtualKeyCode::S),
        ]));
        // One missing entry (Alt) is enough to reject the whole chord.
        assert!(!state.matches_accelerator(&[
            AcceleratorKey::Ctrl,
            AcceleratorKey::Alt,
            AcceleratorKey::Key(VirtualKeyCode::S),
        ]));
        // Wrong key, right modifiers.
        assert!(!state
            .matches_accelerator(&[AcceleratorKey::Ctrl, AcceleratorKey::Key(VirtualKeyCode::Q),]));
        // Order must not matter.
        assert!(state.matches_accelerator(&[
            AcceleratorKey::Key(VirtualKeyCode::S),
            AcceleratorKey::Shift,
            AcceleratorKey::Ctrl,
        ]));
    }

    #[test]
    fn matches_accelerator_survives_huge_chord() {
        // A pathologically long chord must terminate (linear scan, no recursion).
        let state = keyboard_with(&[VirtualKeyCode::LShift]);
        let long_ok = vec![AcceleratorKey::Shift; 10_000];
        assert!(state.matches_accelerator(&long_ok));

        // 10k satisfiable entries with a single unsatisfiable one at the very end:
        // `all()` must still reach it and return false.
        let mut long_bad = vec![AcceleratorKey::Shift; 10_000];
        long_bad.push(AcceleratorKey::Ctrl);
        assert!(!state.matches_accelerator(&long_bad));
    }

    #[test]
    fn accelerator_key_matches_each_variant() {
        let empty = KeyboardState::default();
        for a in [
            AcceleratorKey::Ctrl,
            AcceleratorKey::Alt,
            AcceleratorKey::Shift,
            AcceleratorKey::Key(VirtualKeyCode::A),
        ] {
            assert!(
                !a.matches(&empty),
                "nothing matches an empty keyboard state"
            );
        }
        assert!(AcceleratorKey::Ctrl.matches(&keyboard_with(&[VirtualKeyCode::RControl])));
        assert!(AcceleratorKey::Alt.matches(&keyboard_with(&[VirtualKeyCode::RAlt])));
        assert!(AcceleratorKey::Shift.matches(&keyboard_with(&[VirtualKeyCode::RShift])));
        assert!(AcceleratorKey::Key(VirtualKeyCode::F24)
            .matches(&keyboard_with(&[VirtualKeyCode::F24])));
        // Modifier accelerators are NOT satisfied by the letter of the same name.
        assert!(!AcceleratorKey::Ctrl.matches(&keyboard_with(&[VirtualKeyCode::C])));
    }

    // ---------------------------------------------------------------------
    // MouseState / MouseButtonState
    // ---------------------------------------------------------------------

    #[test]
    fn mouse_state_matches_context_button() {
        let base = MouseState::default();
        assert!(!base.matches(&ContextMenuMouseButton::Left));
        assert!(!base.matches(&ContextMenuMouseButton::Right));
        assert!(!base.matches(&ContextMenuMouseButton::Middle));

        for (ctx, ms) in [
            (
                ContextMenuMouseButton::Left,
                MouseState {
                    left_down: true,
                    ..MouseState::default()
                },
            ),
            (
                ContextMenuMouseButton::Right,
                MouseState {
                    right_down: true,
                    ..MouseState::default()
                },
            ),
            (
                ContextMenuMouseButton::Middle,
                MouseState {
                    middle_down: true,
                    ..MouseState::default()
                },
            ),
        ] {
            assert!(ms.matches(&ctx), "{ctx:?} must match its own button");
            // ...and only its own button.
            let others = [
                ContextMenuMouseButton::Left,
                ContextMenuMouseButton::Right,
                ContextMenuMouseButton::Middle,
            ];
            for other in others {
                assert_eq!(ms.matches(&other), other == ctx);
            }
        }
    }

    #[test]
    fn mouse_down_and_button_state_agree_for_all_8_combinations() {
        for bits in 0u8..8 {
            let (l, r, m) = (bits & 1 != 0, bits & 2 != 0, bits & 4 != 0);
            let ms = MouseState {
                left_down: l,
                right_down: r,
                middle_down: m,
                ..MouseState::default()
            };
            assert_eq!(ms.mouse_down(), l || r || m);

            let snapshot = ms.button_state();
            assert_eq!(snapshot.left_down, l);
            assert_eq!(snapshot.right_down, r);
            assert_eq!(snapshot.middle_down, m);
            // any_down is exactly mouse_down, and the From impl is the same snapshot.
            assert_eq!(snapshot.any_down(), ms.mouse_down());
            assert_eq!(crate::events::MouseButtonState::from(&ms), snapshot);
        }
        // Default MouseState has no button held.
        assert!(!MouseState::default().mouse_down());
        assert!(!MouseState::default().button_state().any_down());
    }

    // ---------------------------------------------------------------------
    // process_system_scroll (numeric)
    // ---------------------------------------------------------------------

    #[test]
    fn process_system_scroll_zero_and_negative_zero_consume_nothing() {
        let r = process_system_scroll(LogicalPosition::zero(), false);
        assert_eq!(r.scrolled_nodes, 0);
        assert_eq!(r.remaining_delta, LogicalPosition::zero());
        assert!(!r.hit_scrollbar);

        // -0.0 == 0.0 under IEEE-754, so a negative-zero delta must also be a no-op.
        let neg_zero = process_system_scroll(LogicalPosition::new(-0.0, -0.0), true);
        assert_eq!(neg_zero.scrolled_nodes, 0);
        assert!(neg_zero.hit_scrollbar, "hit_scrollbar is echoed verbatim");
    }

    #[test]
    fn process_system_scroll_counts_any_nonzero_axis() {
        for delta in [
            LogicalPosition::new(1.0, 0.0),
            LogicalPosition::new(0.0, -1.0),
            LogicalPosition::new(-3.5, 7.25),
            LogicalPosition::new(f32::MIN, 0.0),
            LogicalPosition::new(0.0, f32::MAX),
            LogicalPosition::new(f32::INFINITY, f32::NEG_INFINITY),
            LogicalPosition::new(f32::MIN_POSITIVE, 0.0),
        ] {
            let r = process_system_scroll(delta, false);
            assert_eq!(r.scrolled_nodes, 1, "{delta:?} must count as consumed");
            // Overscroll is never reported by this helper.
            assert_eq!(r.remaining_delta, LogicalPosition::zero());
        }
    }

    #[test]
    fn process_system_scroll_does_not_panic_on_nan() {
        // NaN != 0.0 is `true`, so a NaN delta is currently treated as consumed.
        // The contract asserted here is only "terminates, no panic, bounded count".
        for delta in [
            LogicalPosition::new(f32::NAN, 0.0),
            LogicalPosition::new(0.0, f32::NAN),
            LogicalPosition::new(f32::NAN, f32::NAN),
        ] {
            let r = process_system_scroll(delta, true);
            assert!(r.scrolled_nodes <= 1);
            assert_eq!(r.remaining_delta, LogicalPosition::zero());
            assert!(r.hit_scrollbar);
        }
    }

    #[test]
    fn scroll_result_default_is_inert() {
        let d = ScrollResult::default();
        assert_eq!(d.scrolled_nodes, 0);
        assert_eq!(d.remaining_delta, LogicalPosition::zero());
        assert!(!d.hit_scrollbar);
    }

    // ---------------------------------------------------------------------
    // CursorPosition
    // ---------------------------------------------------------------------

    #[test]
    fn cursor_position_get_position_only_inside_window() {
        let p = LogicalPosition::new(12.0, 34.0);
        assert_eq!(CursorPosition::InWindow(p).get_position(), Some(p));
        assert_eq!(CursorPosition::OutOfWindow(p).get_position(), None);
        assert_eq!(CursorPosition::Uninitialized.get_position(), None);
        // Default (as used by MouseState::default) is Uninitialized.
        assert_eq!(CursorPosition::default(), CursorPosition::Uninitialized);
        assert_eq!(CursorPosition::default().get_position(), None);
    }

    #[test]
    fn cursor_position_is_inside_window_agrees_with_get_position() {
        for c in [
            CursorPosition::Uninitialized,
            CursorPosition::InWindow(LogicalPosition::zero()),
            CursorPosition::OutOfWindow(LogicalPosition::zero()),
            CursorPosition::InWindow(LogicalPosition::new(f32::MIN, f32::MAX)),
            CursorPosition::OutOfWindow(LogicalPosition::new(f32::INFINITY, f32::NAN)),
        ] {
            assert_eq!(c.is_inside_window(), c.get_position().is_some());
        }
        // Extreme / non-finite coordinates are passed through, not sanitized.
        let nan_pos = CursorPosition::InWindow(LogicalPosition::new(f32::NAN, f32::INFINITY));
        assert!(nan_pos.is_inside_window());
        let got = nan_pos
            .get_position()
            .expect("InWindow always yields a position");
        assert!(got.x.is_nan());
        assert!(got.y.is_infinite());
    }

    // ---------------------------------------------------------------------
    // MonitorId constructors
    // ---------------------------------------------------------------------

    #[test]
    fn monitor_id_constructors_preserve_fields_at_extremes() {
        assert_eq!(MonitorId::PRIMARY, MonitorId { index: 0, hash: 0 });
        assert_eq!(MonitorId::new(0), MonitorId::PRIMARY);

        for index in [0usize, 1, 42, usize::MAX] {
            let m = MonitorId::new(index);
            assert_eq!(m.index, index);
            assert_eq!(m.hash, 0, "new() documents hash == 0");

            for hash in [0u64, 1, u64::MAX] {
                let m = MonitorId::from_index_and_hash(index, hash);
                assert_eq!(m.index, index);
                assert_eq!(m.hash, hash);
            }
        }
        // index and hash are independent coordinates of identity.
        assert_ne!(MonitorId::new(1), MonitorId::new(2));
        assert_ne!(
            MonitorId::from_index_and_hash(1, 7),
            MonitorId::from_index_and_hash(1, 8)
        );
    }

    #[test]
    fn monitor_id_from_properties_is_stable_and_index_independent() {
        let pos = LayoutPoint::new(-1920, 0);
        let size = LayoutSize::new(2560, 1440);

        let a = MonitorId::from_properties(0, "HDMI-1", pos, size);
        let b = MonitorId::from_properties(0, "HDMI-1", pos, size);
        assert_eq!(a, b, "hash must be stable across calls (persistable)");

        // The hash intentionally covers only the properties, not the runtime index.
        let reindexed = MonitorId::from_properties(7, "HDMI-1", pos, size);
        assert_eq!(reindexed.hash, a.hash);
        assert_eq!(reindexed.index, 7);
        assert_ne!(reindexed, a, "index is still part of identity");
    }

    #[test]
    fn monitor_id_from_properties_is_sensitive_to_each_property() {
        let pos = LayoutPoint::new(0, 0);
        let size = LayoutSize::new(1920, 1080);
        let base = MonitorId::from_properties(0, "DP-1", pos, size);

        // Changing any single property must change the hash.
        assert_ne!(
            base.hash,
            MonitorId::from_properties(0, "DP-2", pos, size).hash
        );
        assert_ne!(
            base.hash,
            MonitorId::from_properties(0, "DP-1", LayoutPoint::new(1, 0), size).hash
        );
        assert_ne!(
            base.hash,
            MonitorId::from_properties(0, "DP-1", LayoutPoint::new(0, 1), size).hash
        );
        assert_ne!(
            base.hash,
            MonitorId::from_properties(0, "DP-1", pos, LayoutSize::new(1921, 1080)).hash
        );
        assert_ne!(
            base.hash,
            MonitorId::from_properties(0, "DP-1", pos, LayoutSize::new(1920, 1081)).hash
        );
        // A swapped width/height is a different monitor, not the same one.
        assert_ne!(
            base.hash,
            MonitorId::from_properties(0, "DP-1", pos, LayoutSize::new(1080, 1920)).hash
        );
    }

    #[test]
    fn monitor_id_from_properties_handles_hostile_inputs() {
        let size = LayoutSize::new(isize::MAX, isize::MIN);
        let pos = LayoutPoint::new(isize::MIN, isize::MAX);

        // Empty / whitespace / unicode / NUL-containing names must not panic.
        for name in ["", "   ", "\t\n", "\u{1F600}", "e\u{301}", "é", "a\0b"] {
            let m = MonitorId::from_properties(3, name, pos, size);
            assert_eq!(m.index, 3);
            // Same input -> same hash, even at isize extremes.
            assert_eq!(m, MonitorId::from_properties(3, name, pos, size));
        }

        // Byte-exact name comparison: combining-mark and precomposed forms differ.
        assert_ne!(
            MonitorId::from_properties(0, "e\u{301}", pos, size).hash,
            MonitorId::from_properties(0, "é", pos, size).hash
        );

        // A 1M-char monitor name must terminate quickly (FNV-1a is linear).
        let huge = "x".repeat(1_000_000);
        let h1 = MonitorId::from_properties(0, &huge, LayoutPoint::zero(), LayoutSize::zero());
        let h2 = MonitorId::from_properties(0, &huge, LayoutPoint::zero(), LayoutSize::zero());
        assert_eq!(h1, h2);
        assert_ne!(
            h1.hash,
            MonitorId::from_properties(0, "x", LayoutPoint::zero(), LayoutSize::zero()).hash
        );
    }

    // ---------------------------------------------------------------------
    // WindowFlags predicates / getters
    // ---------------------------------------------------------------------

    #[test]
    fn window_flags_type_predicates_are_mutually_exclusive() {
        for (ty, menu, tooltip, dialog) in [
            (WindowType::Normal, false, false, false),
            (WindowType::Menu, true, false, false),
            (WindowType::Tooltip, false, true, false),
            (WindowType::Dialog, false, false, true),
        ] {
            let f = WindowFlags {
                window_type: ty,
                ..WindowFlags::default()
            };
            assert_eq!(f.is_menu_window(), menu);
            assert_eq!(f.is_tooltip_window(), tooltip);
            assert_eq!(f.is_dialog_window(), dialog);
            // At most one classification can ever be true at once.
            let count = u8::from(f.is_menu_window())
                + u8::from(f.is_tooltip_window())
                + u8::from(f.is_dialog_window());
            assert!(count <= 1);
        }
    }

    #[test]
    fn window_flags_bool_getters_mirror_their_fields() {
        // Default: focused, no close request, no CSD.
        let d = WindowFlags::default();
        assert!(d.window_has_focus());
        assert!(!d.is_close_requested());
        assert!(!d.has_csd());
        assert_eq!(
            d.use_native_menus(),
            cfg!(any(target_os = "windows", target_os = "macos"))
        );
        assert_eq!(
            d.use_native_context_menus(),
            cfg!(any(target_os = "windows", target_os = "macos"))
        );

        // Every getter is a pure mirror of its field, in both states.
        for b in [false, true] {
            let f = WindowFlags {
                has_focus: b,
                close_requested: b,
                has_decorations: b,
                use_native_menus: b,
                use_native_context_menus: b,
                ..WindowFlags::default()
            };
            assert_eq!(f.window_has_focus(), b);
            assert_eq!(f.is_close_requested(), b);
            assert_eq!(f.has_csd(), b);
            assert_eq!(f.use_native_menus(), b);
            assert_eq!(f.use_native_context_menus(), b);
        }
    }

    // ---------------------------------------------------------------------
    // StringPairVec: get_key / get_key_mut / insert_kv
    // ---------------------------------------------------------------------

    #[test]
    fn get_key_on_empty_vec_is_none() {
        let empty = StringPairVec::new();
        assert!(empty.get_key("").is_none());
        assert!(empty.get_key("anything").is_none());

        let mut empty_mut = StringPairVec::new();
        assert!(empty_mut.get_key_mut("").is_none());
        assert!(empty_mut.get_key_mut("anything").is_none());
    }

    #[test]
    fn get_key_valid_minimal_and_missing() {
        let v = StringPairVec::from_vec(vec![pair("WM_CLASS", "azul")]);
        assert_eq!(
            v.get_key("WM_CLASS").map(AzString::as_str),
            Some("azul"),
            "positive control"
        );
        assert!(v.get_key("wm_class").is_none(), "lookup is case-sensitive");
        assert!(v.get_key("WM_CLAS").is_none());
        assert!(v.get_key("WM_CLASS ").is_none(), "no trimming is performed");
        assert!(v.get_key(" WM_CLASS").is_none());
        assert!(v.get_key("WM_CLASS;garbage").is_none());
    }

    #[test]
    fn get_key_handles_garbage_whitespace_and_boundary_numbers() {
        let v = StringPairVec::from_vec(vec![
            pair("", "empty-key"),
            pair("   ", "spaces"),
            pair("\t\n", "tabs"),
            pair("0", "zero"),
            pair("-0", "neg-zero"),
            pair("9223372036854775807", "i64-max"),
            pair("NaN", "nan"),
            pair("inf", "inf"),
        ]);

        // Empty and whitespace-only keys are ordinary keys - looked up verbatim.
        assert_eq!(v.get_key("").map(AzString::as_str), Some("empty-key"));
        assert_eq!(v.get_key("   ").map(AzString::as_str), Some("spaces"));
        assert_eq!(v.get_key("\t\n").map(AzString::as_str), Some("tabs"));

        // Numeric-looking keys are compared as strings: "0" and "-0" are distinct.
        assert_eq!(v.get_key("0").map(AzString::as_str), Some("zero"));
        assert_eq!(v.get_key("-0").map(AzString::as_str), Some("neg-zero"));
        assert_eq!(
            v.get_key("9223372036854775807").map(AzString::as_str),
            Some("i64-max")
        );
        assert_eq!(v.get_key("NaN").map(AzString::as_str), Some("nan"));
        assert_eq!(v.get_key("inf").map(AzString::as_str), Some("inf"));
        assert!(v.get_key("nan").is_none());

        // Random non-grammar bytes / control chars / deep bracket nesting: None, no panic.
        assert!(v.get_key("\u{0}\u{1}\u{7f}\\x\"';--").is_none());
        assert!(v.get_key(&"[".repeat(10_000)).is_none());
        assert!(v.get_key(&"{\"a\":".repeat(10_000)).is_none());
    }

    #[test]
    fn get_key_handles_unicode_without_panicking() {
        let v = StringPairVec::from_vec(vec![
            pair("\u{1F600}", "grin"),
            pair("é", "precomposed"),
            pair("日本語", "jp"),
        ]);
        assert_eq!(v.get_key("\u{1F600}").map(AzString::as_str), Some("grin"));
        assert_eq!(v.get_key("日本語").map(AzString::as_str), Some("jp"));
        // No unicode normalization: decomposed "e" + combining acute != "é".
        assert_eq!(v.get_key("é").map(AzString::as_str), Some("precomposed"));
        assert!(v.get_key("e\u{301}").is_none());
        // A prefix of a multi-byte key must not match (no byte-slicing bugs).
        assert!(v.get_key("日本").is_none());
    }

    #[test]
    fn get_key_handles_extremely_long_input() {
        let huge = "k".repeat(1_000_000);
        let mut v = StringPairVec::from_vec(vec![pair("short", "1")]);

        // Searching for a 1M-char key that is not present: linear, terminates.
        assert!(v.get_key(&huge).is_none());

        // ...and one that IS present.
        v.push(AzStringPair {
            key: huge.as_str().into(),
            value: "big".into(),
        });
        assert_eq!(v.get_key(&huge).map(AzString::as_str), Some("big"));
        // Off-by-one on a 1M-char key must not match.
        assert!(v.get_key(&"k".repeat(999_999)).is_none());
        assert!(v.get_key(&"k".repeat(1_000_001)).is_none());
    }

    #[test]
    fn get_key_returns_first_of_duplicate_keys() {
        let v = StringPairVec::from_vec(vec![
            pair("dup", "first"),
            pair("dup", "second"),
            pair("dup", "third"),
        ]);
        assert_eq!(v.get_key("dup").map(AzString::as_str), Some("first"));
    }

    #[test]
    fn get_key_mut_mutates_in_place() {
        let mut v = StringPairVec::from_vec(vec![pair("a", "1"), pair("b", "2")]);
        {
            let entry = v.get_key_mut("b").expect("b is present");
            entry.value = "changed".into();
        }
        assert_eq!(v.get_key("b").map(AzString::as_str), Some("changed"));
        assert_eq!(v.get_key("a").map(AzString::as_str), Some("1"));
        assert!(v.get_key_mut("missing").is_none());
        assert_eq!(v.len(), 2, "get_key_mut must not add entries");

        // Mutating the KEY through get_key_mut is possible and re-targets lookups.
        {
            let entry = v.get_key_mut("a").expect("a is present");
            entry.key = "z".into();
        }
        assert!(v.get_key("a").is_none());
        assert_eq!(v.get_key("z").map(AzString::as_str), Some("1"));
    }

    #[test]
    fn insert_kv_updates_existing_and_appends_new() {
        let mut v = StringPairVec::new();
        v.insert_kv("k", "v1");
        assert_eq!(v.len(), 1);
        assert_eq!(v.get_key("k").map(AzString::as_str), Some("v1"));

        // Re-inserting the same key overwrites in place instead of appending.
        v.insert_kv("k", "v2");
        assert_eq!(v.len(), 1, "insert_kv must not duplicate an existing key");
        assert_eq!(v.get_key("k").map(AzString::as_str), Some("v2"));

        // A different key appends.
        v.insert_kv("other", "x");
        assert_eq!(v.len(), 2);
        assert_eq!(v.get_key("k").map(AzString::as_str), Some("v2"));
        assert_eq!(v.get_key("other").map(AzString::as_str), Some("x"));

        // Repeated inserts of the same key never grow the vec.
        for i in 0..100 {
            v.insert_kv(String::from("k"), format!("gen{i}"));
        }
        assert_eq!(v.len(), 2);
        assert_eq!(v.get_key("k").map(AzString::as_str), Some("gen99"));
    }

    #[test]
    fn insert_kv_accepts_hostile_keys_and_values() {
        let mut v = StringPairVec::new();
        v.insert_kv("", "");
        assert_eq!(v.len(), 1);
        assert_eq!(v.get_key("").map(AzString::as_str), Some(""));

        v.insert_kv("\u{1F600}", "😀");
        assert_eq!(v.get_key("\u{1F600}").map(AzString::as_str), Some("😀"));

        v.insert_kv("   ", "\t\n");
        assert_eq!(v.get_key("   ").map(AzString::as_str), Some("\t\n"));

        // Very long key + value: no hang, and the update path still finds it.
        let huge_key = "K".repeat(100_000);
        let huge_val = "V".repeat(100_000);
        v.insert_kv(huge_key.clone(), huge_val.clone());
        let before = v.len();
        assert_eq!(
            v.get_key(&huge_key).map(AzString::as_str),
            Some(huge_val.as_str())
        );
        v.insert_kv(huge_key.clone(), String::from("small"));
        assert_eq!(v.len(), before, "long key must hit the update path");
        assert_eq!(v.get_key(&huge_key).map(AzString::as_str), Some("small"));
    }

    #[test]
    fn insert_kv_only_updates_the_first_of_pre_existing_duplicates() {
        // Duplicates can only arrive via push()/from_vec(); insert_kv updates the
        // first match (get_key_mut semantics) and leaves the shadowed one stale.
        let mut v = StringPairVec::from_vec(vec![pair("dup", "first"), pair("dup", "second")]);
        v.insert_kv("dup", "updated");
        assert_eq!(v.len(), 2, "no new entry is appended");
        assert_eq!(v.get_key("dup").map(AzString::as_str), Some("updated"));
        assert_eq!(
            v.get(1).expect("second entry still present").value.as_str(),
            "second",
            "the shadowed duplicate is left untouched"
        );
    }

    // ---------------------------------------------------------------------
    // WindowSize getters (numeric saturation)
    // ---------------------------------------------------------------------

    #[test]
    fn window_size_get_logical_size_is_the_identity() {
        for dims in [
            LogicalSize::zero(),
            LogicalSize::new(640.0, 480.0),
            LogicalSize::new(-1.0, -2.0),
            LogicalSize::new(f32::MAX, f32::MIN),
            LogicalSize::new(f32::INFINITY, f32::MIN_POSITIVE),
        ] {
            let ws = WindowSize {
                dimensions: dims,
                ..WindowSize::default()
            };
            assert_eq!(ws.get_logical_size(), dims);
        }
        // Default is the documented 640x480 @ 96 DPI.
        let d = WindowSize::default();
        assert_eq!(d.get_logical_size(), LogicalSize::new(640.0, 480.0));
        assert_eq!(d.dpi, 96);
    }

    #[test]
    fn window_size_get_layout_size_rounds_half_away_from_zero() {
        for (w, h, ew, eh) in [
            (0.0f32, 0.0f32, 0isize, 0isize),
            (640.0, 480.0, 640, 480),
            (640.4, 480.4, 640, 480),
            (640.6, 480.6, 641, 481),
            (640.5, 639.5, 641, 640),
            (-0.5, -1.5, -1, -2),
        ] {
            let ws = WindowSize {
                dimensions: LogicalSize::new(w, h),
                ..WindowSize::default()
            };
            assert_eq!(ws.get_layout_size(), LayoutSize::new(ew, eh), "{w}x{h}");
        }
    }

    #[test]
    fn window_size_get_layout_size_saturates_on_non_finite() {
        // `as isize` saturates: NaN -> 0, +inf -> isize::MAX, -inf -> isize::MIN.
        let nan = WindowSize {
            dimensions: LogicalSize::new(f32::NAN, f32::NAN),
            ..WindowSize::default()
        };
        assert_eq!(nan.get_layout_size(), LayoutSize::new(0, 0));

        let inf = WindowSize {
            dimensions: LogicalSize::new(f32::INFINITY, f32::NEG_INFINITY),
            ..WindowSize::default()
        };
        assert_eq!(
            inf.get_layout_size(),
            LayoutSize::new(isize::MAX, isize::MIN)
        );

        let max = WindowSize {
            dimensions: LogicalSize::new(f32::MAX, f32::MIN),
            ..WindowSize::default()
        };
        let ls = max.get_layout_size();
        assert!(ls.width > 0 && ls.height < 0, "sign is preserved: {ls:?}");
    }

    #[test]
    fn window_size_get_physical_size_saturates_instead_of_wrapping() {
        // Negative logical sizes clamp to 0 (u32 cast drops the sign).
        let neg = WindowSize {
            dimensions: LogicalSize::new(-100.0, -0.4),
            ..WindowSize::default()
        };
        assert_eq!(neg.get_physical_size(), PhysicalSize::new(0, 0));

        // NaN -> 0, +inf -> u32::MAX (saturating float->int cast, no UB).
        let nan = WindowSize {
            dimensions: LogicalSize::new(f32::NAN, f32::INFINITY),
            ..WindowSize::default()
        };
        assert_eq!(nan.get_physical_size(), PhysicalSize::new(0, u32::MAX));

        // f32::MAX at 4x scale overflows u32 -> saturates, never wraps to a small value.
        let huge = WindowSize {
            dimensions: LogicalSize::new(f32::MAX, f32::MAX),
            dpi: 384,
            ..WindowSize::default()
        };
        assert_eq!(
            huge.get_physical_size(),
            PhysicalSize::new(u32::MAX, u32::MAX)
        );

        // The normal path: 96 DPI is 1:1, 192 DPI doubles.
        let normal = WindowSize::default();
        assert_eq!(normal.get_physical_size(), PhysicalSize::new(640, 480));
        let retina = WindowSize {
            dpi: 192,
            ..WindowSize::default()
        };
        assert_eq!(retina.get_physical_size(), PhysicalSize::new(1280, 960));
    }

    #[test]
    fn window_size_get_hidpi_factor_is_never_zero_or_negative() {
        // A 0.0 factor would divide-by-zero in to_logical(); the getter guards dpi == 0.
        for dpi in [
            0u32,
            1,
            47,
            48,
            95,
            96,
            97,
            120,
            144,
            192,
            384,
            u32::from(u16::MAX),
            u32::MAX,
        ] {
            let ws = WindowSize {
                dpi,
                ..WindowSize::default()
            };
            let f = ws.get_hidpi_factor().inner.get();
            assert!(
                f.is_finite() && f > 0.0,
                "dpi {dpi} produced a non-positive / non-finite scale factor: {f}"
            );
        }

        // Exactly representable factors must be exact (no quantization drift).
        for (dpi, expected) in [
            (0u32, 1.0f32),
            (96, 1.0),
            (144, 1.5),
            (192, 2.0),
            (384, 4.0),
        ] {
            let ws = WindowSize {
                dpi,
                ..WindowSize::default()
            };
            assert_eq!(ws.get_hidpi_factor().inner.get(), expected, "dpi {dpi}");
        }

        // Non-representable factors stay within FloatValue's 1/1000 quantization.
        let odd = WindowSize {
            dpi: 100,
            ..WindowSize::default()
        };
        let f = odd.get_hidpi_factor().inner.get();
        assert!((f - 100.0 / 96.0).abs() < 0.002, "dpi 100 -> {f}");
    }

    // ---------------------------------------------------------------------
    // VirtualKeyCode: from_u32 / get_lowercase
    // ---------------------------------------------------------------------

    #[test]
    fn virtual_keycode_from_u32_roundtrips_every_discriminant() {
        for v in 0..=LAST_VK {
            let vk = VirtualKeyCode::from_u32(v)
                .unwrap_or_else(|| panic!("discriminant {v} is missing from the from_u32 table"));
            assert_eq!(vk as u32, v, "from_u32({v}) does not round-trip");
        }
        // First and last declared variants anchor the table.
        assert_eq!(VirtualKeyCode::Key1 as u32, 0);
        assert_eq!(VirtualKeyCode::Cut as u32, LAST_VK);
    }

    #[test]
    fn virtual_keycode_from_u32_rejects_out_of_range() {
        for v in [
            LAST_VK + 1,
            LAST_VK + 2,
            255,
            256,
            1024,
            i32::MAX as u32,
            u32::MAX - 1,
            u32::MAX,
        ] {
            assert_eq!(
                VirtualKeyCode::from_u32(v),
                None,
                "{v} must not decode to a keycode"
            );
        }
    }

    #[test]
    fn virtual_keycode_get_lowercase_never_panics_and_maps_letters_and_digits() {
        // Exhaustive: no keycode may panic, and any produced char is ASCII.
        for v in 0..=LAST_VK {
            let vk = VirtualKeyCode::from_u32(v).expect("discriminant in range");
            if let Some(c) = vk.get_lowercase() {
                assert!(c.is_ascii(), "{vk:?} produced a non-ASCII char {c:?}");
                assert!(!c.is_ascii_uppercase(), "{vk:?} must yield lowercase");
            }
        }

        // Letters A..Z are discriminants 10..=35 and map to 'a'..='z'.
        for (i, expected) in ('a'..='z').enumerate() {
            let vk = VirtualKeyCode::from_u32(10 + i as u32).expect("letter range");
            assert_eq!(vk.get_lowercase(), Some(expected));
        }

        // Digits: both the top row and the numpad map to the same char.
        for (top, pad, c) in [
            (VirtualKeyCode::Key0, VirtualKeyCode::Numpad0, '0'),
            (VirtualKeyCode::Key1, VirtualKeyCode::Numpad1, '1'),
            (VirtualKeyCode::Key5, VirtualKeyCode::Numpad5, '5'),
            (VirtualKeyCode::Key9, VirtualKeyCode::Numpad9, '9'),
        ] {
            assert_eq!(top.get_lowercase(), Some(c));
            assert_eq!(pad.get_lowercase(), Some(c));
        }

        // Punctuation that IS mapped.
        assert_eq!(VirtualKeyCode::Minus.get_lowercase(), Some('-'));
        assert_eq!(VirtualKeyCode::Period.get_lowercase(), Some('.'));
        assert_eq!(VirtualKeyCode::Slash.get_lowercase(), Some('/'));
        assert_eq!(VirtualKeyCode::Caret.get_lowercase(), Some('^'));

        // Non-character keys have no lowercase form.
        for vk in [
            VirtualKeyCode::LShift,
            VirtualKeyCode::RControl,
            VirtualKeyCode::Escape,
            VirtualKeyCode::F12,
            VirtualKeyCode::Space,
            VirtualKeyCode::Return,
            VirtualKeyCode::Back,
        ] {
            assert_eq!(vk.get_lowercase(), None, "{vk:?}");
        }
    }

    // ---------------------------------------------------------------------
    // WindowIcon::get_key + key-only Eq/Ord/Hash
    // ---------------------------------------------------------------------

    #[test]
    fn window_icon_get_key_returns_the_stored_key() {
        let small_key = IconKey::new();
        let large_key = IconKey::new();

        let small = WindowIcon::Small(SmallWindowIconBytes {
            key: small_key,
            rgba_bytes: vec![0u8; 16 * 16 * 4].into(),
        });
        let large = WindowIcon::Large(LargeWindowIconBytes {
            key: large_key,
            rgba_bytes: vec![255u8; 32 * 32 * 4].into(),
        });

        assert_eq!(small.get_key(), small_key);
        assert_eq!(large.get_key(), large_key);

        // Empty payloads are legal and must not panic.
        let empty = WindowIcon::Small(SmallWindowIconBytes {
            key: small_key,
            rgba_bytes: vec![].into(),
        });
        assert_eq!(empty.get_key(), small_key);
    }

    #[test]
    fn window_icon_identity_is_the_key_alone() {
        // The whole point of IconKey: diff the key, not the bytes. Two icons with
        // the same key compare equal even though their pixels differ.
        let key = IconKey::new();
        let a = WindowIcon::Small(SmallWindowIconBytes {
            key,
            rgba_bytes: vec![0u8; 4].into(),
        });
        let b = WindowIcon::Small(SmallWindowIconBytes {
            key,
            rgba_bytes: vec![7u8; 1024].into(),
        });
        // ...even across the Small/Large variants.
        let c = WindowIcon::Large(LargeWindowIconBytes {
            key,
            rgba_bytes: vec![9u8; 32 * 32 * 4].into(),
        });

        assert_eq!(a, b);
        assert_eq!(a, c);
        assert_eq!(a.cmp(&c), Ordering::Equal);
        assert_eq!(a.partial_cmp(&b), Some(Ordering::Equal));
        // Hash must agree with Eq, or icons break as BTreeMap/HashMap keys.
        assert_eq!(hash_of(&a), hash_of(&b));
        assert_eq!(hash_of(&a), hash_of(&c));

        // Different keys: unequal, and ordered by key.
        let older = WindowIcon::Small(SmallWindowIconBytes {
            key,
            rgba_bytes: vec![0u8; 4].into(),
        });
        let newer = WindowIcon::Small(SmallWindowIconBytes {
            key: IconKey::new(),
            rgba_bytes: vec![0u8; 4].into(),
        });
        assert_ne!(older, newer);
        assert_eq!(older.cmp(&newer), Ordering::Less);
    }

    // ---------------------------------------------------------------------
    // UpdateFocusWarning: Display
    // ---------------------------------------------------------------------

    #[test]
    fn update_focus_warning_display_is_non_empty_for_every_variant() {
        let dom = format!("{}", UpdateFocusWarning::FocusInvalidDomId(DomId::ROOT_ID));
        assert!(dom.contains("invalid ID"), "{dom}");
        assert!(!dom.is_empty());

        let node = format!(
            "{}",
            UpdateFocusWarning::FocusInvalidNodeId(NodeHierarchyItemId::NONE)
        );
        assert!(node.contains("invalid ID"), "{node}");

        // Edge values: a zero DomId, a raw-encoded huge node id, an empty CssPath.
        let huge = format!(
            "{}",
            UpdateFocusWarning::FocusInvalidNodeId(NodeHierarchyItemId::from_raw(usize::MAX))
        );
        assert!(!huge.is_empty());

        let path = format!(
            "{}",
            UpdateFocusWarning::CouldNotFindFocusNode(CssPath::default())
        );
        assert!(
            path.starts_with("Could not find focus node for path:"),
            "{path}"
        );
    }
}
