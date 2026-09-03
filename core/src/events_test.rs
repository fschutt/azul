#[allow(unused_imports)]
pub use super::*;
#[cfg(test)]
mod tests {
    use super::*;
    use crate::dom::{DomId, DomNodeId};
    use crate::geom::LogicalPosition;
    use crate::id::NodeId;
    use crate::styled_dom::NodeHierarchyItemId;
    use crate::task::{Instant, SystemTick};
    use crate::window::{
        KeyboardState, MouseState, OptionVirtualKeyCode, VirtualKeyCode, VirtualKeyCodeVec,
    };
    use azul_css::AzString;

    struct MockSelectionManager {
        click_count: u8,
        has_sel: bool,
    }
    impl SelectionManagerQuery for MockSelectionManager {
        fn get_click_count(&self) -> u8 {
            self.click_count
        }
        fn get_drag_start_position(&self) -> Option<LogicalPosition> {
            None
        }
        fn has_selection(&self) -> bool {
            self.has_sel
        }
    }

    struct MockFocusManager(Option<DomNodeId>);
    impl FocusManagerQuery for MockFocusManager {
        fn get_focused_node_id(&self) -> Option<DomNodeId> {
            self.0
        }
    }

    fn focused_node(node_idx: usize) -> DomNodeId {
        DomNodeId {
            dom: DomId { inner: 0 },
            node: NodeHierarchyItemId::from_crate_internal(Some(NodeId::new(node_idx))),
        }
    }

    fn make_keyboard_state(vk: VirtualKeyCode) -> KeyboardState {
        KeyboardState {
            current_virtual_keycode: OptionVirtualKeyCode::Some(vk),
            pressed_virtual_keycodes: VirtualKeyCodeVec::from_vec(vec![vk]),
            ..KeyboardState::default()
        }
    }

    fn make_keydown_event(target: DomNodeId) -> SyntheticEvent {
        SyntheticEvent::new(
            EventType::KeyDown,
            EventSource::User,
            target,
            Instant::Tick(SystemTick::new(0)),
            EventData::Keyboard(KeyboardEventData {
                key_code: VirtualKeyCode::Back as u32,
                char_code: None,
                modifiers: KeyModifiers::default(),
                repeat: false,
                ..Default::default()
            }),
        )
    }

    #[test]
    fn backspace_generates_delete_text_selection() {
        let target = focused_node(2);
        let events = vec![make_keydown_event(target)];
        let kb = make_keyboard_state(VirtualKeyCode::Back);
        let mouse = MouseState::default();
        let sel = MockSelectionManager {
            click_count: 0,
            has_sel: false,
        };
        let focus = MockFocusManager(Some(target));

        let result = pre_callback_filter_internal_events(&events, None, &kb, &mouse, &sel, &focus, true);

        let ops: Vec<_> = result
            .system_changes
            .iter()
            .filter(|c| matches!(c, SystemChange::ApplySelectionOp { .. }))
            .collect();
        assert_eq!(ops.len(), 1, "Backspace should generate ApplySelectionOp");
        match &ops[0] {
            SystemChange::ApplySelectionOp { op, .. } => {
                assert_eq!(op.direction, SelectionDirection::Backward);
                assert_eq!(op.step, SelectionStep::Character);
                assert_eq!(op.mode, SelectionMode::Delete);
            }
            _ => unreachable!(),
        }
    }

    #[test]
    fn delete_key_generates_forward_deletion() {
        let target = focused_node(2);
        let event = SyntheticEvent::new(
            EventType::KeyDown,
            EventSource::User,
            target,
            Instant::Tick(SystemTick::new(0)),
            EventData::Keyboard(KeyboardEventData {
                key_code: VirtualKeyCode::Delete as u32,
                char_code: None,
                modifiers: KeyModifiers::default(),
                repeat: false,
                ..Default::default()
            }),
        );
        let kb = make_keyboard_state(VirtualKeyCode::Delete);
        let mouse = MouseState::default();
        let sel = MockSelectionManager {
            click_count: 0,
            has_sel: false,
        };
        let focus = MockFocusManager(Some(target));
        let result = pre_callback_filter_internal_events(&[event], None, &kb, &mouse, &sel, &focus, true);
        let ops: Vec<_> = result
            .system_changes
            .iter()
            .filter(|c| matches!(c, SystemChange::ApplySelectionOp { .. }))
            .collect();
        assert_eq!(ops.len(), 1);
        match &ops[0] {
            SystemChange::ApplySelectionOp { op, .. } => {
                assert_eq!(op.direction, SelectionDirection::Forward);
                assert_eq!(op.step, SelectionStep::Character);
                assert_eq!(op.mode, SelectionMode::Delete);
            }
            _ => unreachable!(),
        }
    }

    #[test]
    fn arrow_left_generates_navigation() {
        let target = focused_node(2);
        let event = SyntheticEvent::new(
            EventType::KeyDown,
            EventSource::User,
            target,
            Instant::Tick(SystemTick::new(0)),
            EventData::Keyboard(KeyboardEventData {
                key_code: VirtualKeyCode::Left as u32,
                char_code: None,
                modifiers: KeyModifiers::default(),
                repeat: false,
                ..Default::default()
            }),
        );
        let kb = make_keyboard_state(VirtualKeyCode::Left);
        let mouse = MouseState::default();
        let sel = MockSelectionManager {
            click_count: 0,
            has_sel: false,
        };
        let focus = MockFocusManager(Some(target));
        let result = pre_callback_filter_internal_events(&[event], None, &kb, &mouse, &sel, &focus, true);
        let ops: Vec<_> = result
            .system_changes
            .iter()
            .filter(|c| matches!(c, SystemChange::ApplySelectionOp { .. }))
            .collect();
        assert_eq!(ops.len(), 1, "Left arrow should generate ApplySelectionOp");
        match &ops[0] {
            SystemChange::ApplySelectionOp { op, .. } => {
                assert_eq!(op.direction, SelectionDirection::Backward);
                assert_eq!(op.step, SelectionStep::Character);
                assert_eq!(op.mode, SelectionMode::Move);
            }
            _ => unreachable!(),
        }
    }

    #[test]
    fn no_focused_node_means_no_keyboard_system_changes() {
        let target = focused_node(2);
        let event = make_keydown_event(target);
        let kb = make_keyboard_state(VirtualKeyCode::Back);
        let mouse = MouseState::default();
        let sel = MockSelectionManager {
            click_count: 0,
            has_sel: false,
        };
        let focus = MockFocusManager(None); // No focus!

        let result = pre_callback_filter_internal_events(&[event], None, &kb, &mouse, &sel, &focus, true);

        assert!(
            result.system_changes.is_empty(),
            "No system changes should be generated without focused node"
        );
    }

    #[test]
    fn keydown_without_keyboard_data_generates_no_system_change() {
        let target = focused_node(2);
        let event = SyntheticEvent::new(
            EventType::KeyDown,
            EventSource::User,
            target,
            Instant::Tick(SystemTick::new(0)),
            EventData::None, // Bug: missing keyboard data
        );
        let kb = make_keyboard_state(VirtualKeyCode::Back);
        let mouse = MouseState::default();
        let sel = MockSelectionManager {
            click_count: 0,
            has_sel: false,
        };
        let focus = MockFocusManager(Some(target));

        let result = pre_callback_filter_internal_events(&[event], None, &kb, &mouse, &sel, &focus, true);

        // This test documents the bug we just fixed: EventData::None causes
        // the handle_key_down function to return None (early exit at line 2737)
        assert!(
            result.system_changes.is_empty(),
            "EventData::None should not generate system changes (documents the old bug)"
        );
    }

    #[test]
    fn ctrl_c_generates_copy() {
        // MWA-A2/MWA-D: shortcuts key off the PRIMARY modifier — Cmd on
        // macOS, Ctrl elsewhere — so this test presses the platform's
        // primary key. (The old version hardcoded LControl and correctly
        // started failing on macOS hosts once primary_down() landed:
        // Ctrl+C must NOT copy on macOS, Cmd+C does.)
        let primary_key = if cfg!(target_os = "macos") {
            VirtualKeyCode::LWin
        } else {
            VirtualKeyCode::LControl
        };
        let target = focused_node(2);
        let event = SyntheticEvent::new(
            EventType::KeyDown,
            EventSource::User,
            target,
            Instant::Tick(SystemTick::new(0)),
            EventData::Keyboard(KeyboardEventData {
                key_code: VirtualKeyCode::C as u32,
                char_code: Some('c'),
                modifiers: KeyModifiers {
                    ctrl: !cfg!(target_os = "macos"),
                    shift: false,
                    alt: false,
                    meta: cfg!(target_os = "macos"),
                },
                repeat: false,
                ..Default::default()
            }),
        );
        let mut kb = make_keyboard_state(VirtualKeyCode::C);
        kb.pressed_virtual_keycodes =
            VirtualKeyCodeVec::from_vec(vec![VirtualKeyCode::C, primary_key]);
        let mouse = MouseState::default();
        let sel = MockSelectionManager {
            click_count: 0,
            has_sel: false,
        };
        let focus = MockFocusManager(Some(target));

        let result = pre_callback_filter_internal_events(&[event], None, &kb, &mouse, &sel, &focus, true);

        let copy_changes = result
            .system_changes
            .iter()
            .filter(|c| matches!(c, SystemChange::CopyToClipboard))
            .count();

        assert_eq!(copy_changes, 1, "primary+C should generate CopyToClipboard");
    }

    fn make_hit_test_with_node(node_idx: usize) -> FullHitTest {
        use crate::dom::OptionDomNodeId;
        use crate::hit_test::{FullHitTest, HitTest, HitTestItem};
        use crate::spaces::ContentBoxLocal;
        use std::collections::BTreeMap;

        let node_id = NodeId::new(node_idx);
        let dom_id = DomId { inner: 0 };

        let mut regular = BTreeMap::new();
        regular.insert(
            node_id,
            HitTestItem {
                point_in_viewport: LogicalPosition::new(100.0, 200.0),
                point_relative_to_item: ContentBoxLocal::new(LogicalPosition::new(50.0, 30.0)),
                is_focusable: true,
                is_virtual_view_hit: None,
                hit_depth: 0,
            },
        );

        let mut hovered = BTreeMap::new();
        hovered.insert(
            dom_id,
            HitTest {
                regular_hit_test_nodes: regular,
                scroll_hit_test_nodes: BTreeMap::new(),
                scrollbar_hit_test_nodes: BTreeMap::new(),
                cursor_hit_test_nodes: BTreeMap::new(),
            },
        );

        FullHitTest {
            hovered_nodes: hovered,
            focused_node: OptionDomNodeId::None,
        }
    }

    #[test]
    fn mousedown_generates_text_selection_click() {
        let target = focused_node(2);
        let event = SyntheticEvent::new(
            EventType::MouseDown,
            EventSource::User,
            target,
            Instant::Tick(SystemTick::new(0)),
            EventData::Mouse(MouseEventData {
                position: LogicalPosition::new(100.0, 200.0),
                button: MouseButton::Left,
                buttons: 1,
                modifiers: KeyModifiers::default(),
                ..Default::default()
            }),
        );
        let hit_test = make_hit_test_with_node(2);
        let kb = KeyboardState::default();
        let mouse = MouseState::default();
        let sel = MockSelectionManager {
            click_count: 1,
            has_sel: false,
        };
        let focus = MockFocusManager(Some(target));

        let result = pre_callback_filter_internal_events(
            &[event],
            Some(&hit_test),
            &kb,
            &mouse,
            &sel,
            &focus,
            true,
        );

        let click_changes = result
            .system_changes
            .iter()
            .filter(|c| matches!(c, SystemChange::TextSelectionClick { .. }))
            .count();

        assert_eq!(
            click_changes, 1,
            "MouseDown with hit_test should generate TextSelectionClick"
        );
    }

    #[test]
    fn process_event_result_max_self_picks_higher_variant() {
        let lo = ProcessEventResult::ShouldReRenderCurrentWindow;
        let hi = ProcessEventResult::ShouldRegenerateDomCurrentWindow;
        assert_eq!(lo.max_self(hi), hi);
        assert_eq!(hi.max_self(lo), hi);
        assert_eq!(lo.max_self(lo), lo);
    }

    #[test]
    fn keyboard_shortcut_keys_off_primary_modifier() {
        use crate::window::VirtualKeyCode::{A, C, V, X, Z};
        // No primary modifier → never a shortcut (MWA-A2).
        assert_eq!(KeyboardShortcut::from_key(C, false, false), None);
        assert_eq!(KeyboardShortcut::from_key(Z, false, true), None);
        // Primary held → the standard editing set.
        assert_eq!(
            KeyboardShortcut::from_key(C, true, false),
            Some(KeyboardShortcut::Copy)
        );
        assert_eq!(
            KeyboardShortcut::from_key(X, true, false),
            Some(KeyboardShortcut::Cut)
        );
        assert_eq!(
            KeyboardShortcut::from_key(V, true, false),
            Some(KeyboardShortcut::Paste)
        );
        assert_eq!(
            KeyboardShortcut::from_key(A, true, false),
            Some(KeyboardShortcut::SelectAll)
        );
        assert_eq!(
            KeyboardShortcut::from_key(Z, true, false),
            Some(KeyboardShortcut::Undo)
        );
        assert_eq!(
            KeyboardShortcut::from_key(Z, true, true),
            Some(KeyboardShortcut::Redo)
        );
    }

    #[test]
    fn primary_modifier_is_platform_correct() {
        use crate::window::{KeyboardState, VirtualKeyCode};
        let cmd_held = KeyboardState {
            pressed_virtual_keycodes: vec![VirtualKeyCode::LWin].into(),
            ..Default::default()
        };
        // Cmd/super is primary ONLY on macOS.
        assert_eq!(cmd_held.primary_down(), cfg!(target_os = "macos"));

        let ctrl_held = KeyboardState {
            pressed_virtual_keycodes: vec![VirtualKeyCode::LControl].into(),
            ..Default::default()
        };
        // Ctrl is primary everywhere EXCEPT macOS.
        assert_eq!(ctrl_held.primary_down(), !cfg!(target_os = "macos"));
    }

    #[test]
    fn arrow_direction_from_key_maps_arrows_and_home_end() {
        use crate::window::VirtualKeyCode::*;
        assert_eq!(
            ArrowDirection::from_key(Left, false),
            Some(ArrowDirection::Left)
        );
        assert_eq!(
            ArrowDirection::from_key(Right, false),
            Some(ArrowDirection::Right)
        );
        assert_eq!(
            ArrowDirection::from_key(Up, false),
            Some(ArrowDirection::Up)
        );
        assert_eq!(
            ArrowDirection::from_key(Down, false),
            Some(ArrowDirection::Down)
        );
        assert_eq!(
            ArrowDirection::from_key(Home, false),
            Some(ArrowDirection::LineStart)
        );
        assert_eq!(
            ArrowDirection::from_key(End, false),
            Some(ArrowDirection::LineEnd)
        );
        assert_eq!(
            ArrowDirection::from_key(Home, true),
            Some(ArrowDirection::DocumentStart)
        );
        assert_eq!(
            ArrowDirection::from_key(End, true),
            Some(ArrowDirection::DocumentEnd)
        );
        assert_eq!(ArrowDirection::from_key(C, false), None);
    }

    #[test]
    fn arrow_direction_to_selection_respects_ctrl() {
        let (d, s) = ArrowDirection::Left.to_selection(false);
        assert_eq!(
            (d, s),
            (SelectionDirection::Backward, SelectionStep::Character)
        );
        let (d, s) = ArrowDirection::Left.to_selection(true);
        assert_eq!((d, s), (SelectionDirection::Backward, SelectionStep::Word));
        let (d, s) = ArrowDirection::Up.to_selection(false);
        assert_eq!(
            (d, s),
            (SelectionDirection::Backward, SelectionStep::VisualLine)
        );
        let (d, s) = ArrowDirection::DocumentEnd.to_selection(false);
        assert_eq!(
            (d, s),
            (SelectionDirection::Forward, SelectionStep::Document)
        );
    }

    #[test]
    fn keyboard_shortcut_from_key_recognizes_editing_combos() {
        use crate::window::VirtualKeyCode::*;
        assert_eq!(
            KeyboardShortcut::from_key(C, true, false),
            Some(KeyboardShortcut::Copy)
        );
        assert_eq!(
            KeyboardShortcut::from_key(X, true, false),
            Some(KeyboardShortcut::Cut)
        );
        assert_eq!(
            KeyboardShortcut::from_key(V, true, false),
            Some(KeyboardShortcut::Paste)
        );
        assert_eq!(
            KeyboardShortcut::from_key(A, true, false),
            Some(KeyboardShortcut::SelectAll)
        );
        assert_eq!(
            KeyboardShortcut::from_key(Z, true, false),
            Some(KeyboardShortcut::Undo)
        );
        assert_eq!(
            KeyboardShortcut::from_key(Z, true, true),
            Some(KeyboardShortcut::Redo)
        );
        assert_eq!(
            KeyboardShortcut::from_key(Y, true, false),
            Some(KeyboardShortcut::Redo)
        );
        // Non-ctrl combos must not match
        assert_eq!(KeyboardShortcut::from_key(C, false, false), None);
        // Unknown keys
        assert_eq!(KeyboardShortcut::from_key(D, true, false), None);
    }

    #[test]
    fn mouse_button_state_round_trips_from_mouse_state() {
        let ms = MouseState {
            left_down: true,
            middle_down: true,
            ..MouseState::default()
        };
        let bs: MouseButtonState = (&ms).into();
        assert!(bs.left_down);
        assert!(!bs.right_down);
        assert!(bs.middle_down);
        assert!(bs.any_down());

        let none = MouseButtonState {
            left_down: false,
            right_down: false,
            middle_down: false,
        };
        assert!(!none.any_down());
    }

    #[test]
    fn callback_to_call_collects_hits_for_dom() {
        let dom_id = DomId { inner: 0 };
        let hit_test = make_hit_test_with_node(2);
        let filter = EventFilter::Hover(HoverEventFilter::MouseDown);
        let calls = CallbackToCall::from_hit_test(&hit_test, dom_id, filter);
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].node_id, NodeId::new(2));
        assert_eq!(calls[0].event_filter, filter);
        assert!(calls[0].hit_test_item.is_some());

        // Unknown DOM id => empty list
        let other = CallbackToCall::from_hit_test(
            &hit_test,
            DomId { inner: 999 },
            EventFilter::Hover(HoverEventFilter::MouseUp),
        );
        assert!(other.is_empty());

        // Direct constructor builds expected fields
        let direct = CallbackToCall::new(
            NodeId::new(7),
            None,
            EventFilter::Focus(FocusEventFilter::FocusReceived),
        );
        assert_eq!(direct.node_id, NodeId::new(7));
        assert!(direct.hit_test_item.is_none());
    }

    #[test]
    fn restyle_relayout_aliases_are_btreemap_compatible() {
        // RestyleNodes / RelayoutNodes are aliases for BTreeMap<NodeId, Vec<ChangedCssProperty>>.
        // Confirm we can construct empty ones via the alias and that they accept the same keys.
        let restyle: RestyleNodes = BTreeMap::new();
        let relayout: RelayoutNodes = BTreeMap::new();
        assert!(restyle.is_empty());
        assert!(relayout.is_empty());

        // RelayoutWords is BTreeMap<NodeId, AzString>.
        let mut words: RelayoutWords = BTreeMap::new();
        words.insert(NodeId::new(1), AzString::from_const_str("hello"));
        assert_eq!(
            words.get(&NodeId::new(1)).map(azul_css::AzString::as_str),
            Some("hello")
        );
    }

    #[test]
    fn detect_lifecycle_events_with_reconciliation_is_callable() {
        // Smoke test: empty old/new node data must produce no events and an
        // empty migration map. This proves the function is callable from
        // the public API and threads through `crate::diff::reconcile_dom`.
        let dom_id = DomId { inner: 0 };
        let old_data: Vec<crate::dom::NodeData> = Vec::new();
        let new_data: Vec<crate::dom::NodeData> = Vec::new();
        let old_hier: Vec<crate::styled_dom::NodeHierarchyItem> = Vec::new();
        let new_hier: Vec<crate::styled_dom::NodeHierarchyItem> = Vec::new();
        let old_layout = OrderedMap::default();
        let new_layout = OrderedMap::default();
        let result: LifecycleEventResult = detect_lifecycle_events_with_reconciliation(
            dom_id,
            &old_data,
            &new_data,
            &old_hier,
            &new_hier,
            &old_layout,
            &new_layout,
            Instant::Tick(SystemTick::new(0)),
        );
        assert!(result.events.is_empty());
        assert!(result.node_id_mapping.is_empty());
    }

    #[test]
    fn nodedata_focusable_and_activation_traits_are_wired() {
        use crate::dom::{NodeData, NodeType};
        use crate::events::{ActivationBehavior as _, Focusable as _};

        // <button> is naturally focusable and has activation behavior.
        let btn = NodeData::create_node(NodeType::Button);
        assert!(<NodeData as Focusable>::is_naturally_focusable(&btn));
        assert!(<NodeData as Focusable>::is_focusable(&btn));
        assert!(<NodeData as ActivationBehavior>::has_activation_behavior(
            &btn
        ));
        assert!(<NodeData as ActivationBehavior>::is_activatable(&btn));

        // A plain <div> is neither naturally focusable nor activatable.
        let div = NodeData::create_node(NodeType::Div);
        assert!(!<NodeData as Focusable>::is_naturally_focusable(&div));
        assert!(!<NodeData as ActivationBehavior>::has_activation_behavior(
            &div
        ));

        // <input> is naturally focusable.
        let input = NodeData::create_node(NodeType::Input);
        assert!(<NodeData as Focusable>::is_naturally_focusable(&input));
    }

    #[test]
    fn first_hovered_node_picks_frontmost_by_depth() {
        use crate::dom::OptionDomNodeId;
        use crate::hit_test::{FullHitTest, HitTest, HitTestItem};
        use crate::spaces::ContentBoxLocal;
        use std::collections::BTreeMap;

        let item = |depth: u32| HitTestItem {
            point_in_viewport: LogicalPosition::zero(),
            point_relative_to_item: ContentBoxLocal::zero(),
            is_focusable: true,
            is_virtual_view_hit: None,
            hit_depth: depth,
        };

        // Front-most node (depth 0) has the HIGHER NodeId; back node (depth 5)
        // has the lower id. The old `.next()` logic returned the lowest id
        // (node 2, the back one). We must now return the front-most (node 5).
        let mut regular = BTreeMap::new();
        regular.insert(NodeId::new(2), item(5));
        regular.insert(NodeId::new(5), item(0));

        let mut hovered = BTreeMap::new();
        hovered.insert(
            DomId { inner: 0 },
            HitTest {
                regular_hit_test_nodes: regular,
                scroll_hit_test_nodes: BTreeMap::new(),
                scrollbar_hit_test_nodes: BTreeMap::new(),
                cursor_hit_test_nodes: BTreeMap::new(),
            },
        );
        let ht = FullHitTest {
            hovered_nodes: hovered,
            focused_node: OptionDomNodeId::None,
        };

        let got = get_first_hovered_node(Some(&ht)).unwrap();
        assert_eq!(got.node.into_crate_internal(), Some(NodeId::new(5)));
    }

    #[test]
    fn size_changed_nan_guard_stops_resize_loop() {
        use crate::geom::LogicalSize;
        // A NaN dimension present on BOTH frames must read as "unchanged" so no
        // Resize is emitted every frame.
        let a = LogicalSize::new(f32::NAN, 100.0);
        let b = LogicalSize::new(f32::NAN, 100.0);
        assert!(!size_changed(a, b));
        // A real change is still detected.
        assert!(size_changed(
            LogicalSize::new(100.0, 100.0),
            LogicalSize::new(100.0, 120.0)
        ));
        // Sub-quantum jitter is ignored.
        assert!(!size_changed(
            LogicalSize::new(100.0, 100.0),
            LogicalSize::new(100.00005, 100.0)
        ));
    }

    #[test]
    fn dom_path_terminates_on_parent_cycle() {
        use crate::id::{Node, NodeHierarchy};
        // Two nodes whose parents point at each other -> a cycle.
        let nodes = vec![
            Node {
                parent: Some(NodeId::new(1)),
                ..Node::ROOT
            },
            Node {
                parent: Some(NodeId::new(0)),
                ..Node::ROOT
            },
        ];
        let hier = NodeHierarchy::new(nodes);
        let target = NodeHierarchyItemId::from_crate_internal(Some(NodeId::new(0)));
        // Must not hang / OOM; bounded by node count + visited-set.
        let path = get_dom_path(&hier, target);
        assert!(path.len() <= 2);
    }

    /// `Click` is ACTIVATION and has its own filter. It must NOT also reach
    /// `MouseUp` listeners: a real pointer release emits BOTH `MouseUp` and
    /// `Click` (W3C, and `event_determination` does exactly that), so a Click
    /// that also fired MouseUp handlers ran every activation TWICE - the
    /// ColorInput opened its picker and instantly closed it again
    /// (2026-09-01).
    #[test]
    fn click_maps_to_the_activation_filter_and_not_to_mouse_up() {
        let filters = event_type_to_filters(EventType::Click, &EventData::None);
        assert!(filters.contains(&EventFilter::Hover(HoverEventFilter::Click)));
        assert!(
            !filters.contains(&EventFilter::Hover(HoverEventFilter::MouseUp)),
            "a Click must not double-fire MouseUp listeners: {filters:?}",
        );
        assert!(!filters.contains(&EventFilter::Hover(HoverEventFilter::LeftMouseUp)));
        assert!(!filters.contains(&EventFilter::Hover(HoverEventFilter::LeftMouseDown)));
    }
}

#[cfg(test)]
#[allow(clippy::float_cmp, clippy::too_many_lines)]
mod autotest_generated {
    use super::*;
    use crate::{
        dom::{DomId, DomNodeId, OptionDomNodeId},
        geom::{LogicalPosition, LogicalRect, LogicalSize},
        hit_test::{FullHitTest, HitTest, HitTestItem},
        id::{Node, NodeHierarchy, NodeId},
        spaces::ContentBoxLocal,
        styled_dom::NodeHierarchyItemId,
        task::{Instant, SystemTick},
        window::{CursorPosition, KeyboardState, MouseState, VirtualKeyCode, VirtualKeyCodeVec},
    };

    // ---------------------------------------------------------------- helpers

    fn tick(n: u64) -> Instant {
        Instant::Tick(SystemTick::new(n))
    }

    fn dnid(dom: usize, node: usize) -> DomNodeId {
        DomNodeId {
            dom: DomId { inner: dom },
            node: NodeHierarchyItemId::from_crate_internal(Some(NodeId::new(node))),
        }
    }

    /// A `DomNodeId` whose node slot is the `None` sentinel (raw inner == 0).
    fn dnid_none(dom: usize) -> DomNodeId {
        DomNodeId {
            dom: DomId { inner: dom },
            node: NodeHierarchyItemId::NONE,
        }
    }

    fn hit_item(depth: u32) -> HitTestItem {
        HitTestItem {
            point_in_viewport: LogicalPosition::new(1.0, 2.0),
            point_relative_to_item: ContentBoxLocal::new(LogicalPosition::new(3.0, 4.0)),
            is_focusable: true,
            is_virtual_view_hit: None,
            hit_depth: depth,
        }
    }

    /// Hit test containing `(node_index, hit_depth)` pairs, all under one DOM.
    fn hit_test_with(dom: usize, nodes: &[(usize, u32)]) -> FullHitTest {
        let mut regular = BTreeMap::new();
        for (idx, depth) in nodes {
            regular.insert(NodeId::new(*idx), hit_item(*depth));
        }
        let mut hovered = BTreeMap::new();
        hovered.insert(
            DomId { inner: dom },
            HitTest {
                regular_hit_test_nodes: regular,
                scroll_hit_test_nodes: BTreeMap::new(),
                scrollbar_hit_test_nodes: BTreeMap::new(),
                cursor_hit_test_nodes: BTreeMap::new(),
            },
        );
        FullHitTest {
            hovered_nodes: hovered,
            focused_node: OptionDomNodeId::None,
        }
    }

    fn empty_hit_test() -> FullHitTest {
        FullHitTest {
            hovered_nodes: BTreeMap::new(),
            focused_node: OptionDomNodeId::None,
        }
    }

    fn mouse_event(ty: EventType, button: MouseButton, pos: LogicalPosition) -> SyntheticEvent {
        SyntheticEvent::new(
            ty,
            EventSource::User,
            dnid(0, 0),
            tick(0),
            EventData::Mouse(MouseEventData {
                position: pos,
                button,
                buttons: 1,
                modifiers: KeyModifiers::default(),
                ..Default::default()
            }),
        )
    }

    fn key_event(key_code: u32, modifiers: KeyModifiers) -> SyntheticEvent {
        SyntheticEvent::new(
            EventType::KeyDown,
            EventSource::User,
            dnid(0, 0),
            tick(0),
            EventData::Keyboard(KeyboardEventData {
                key_code,
                char_code: None,
                modifiers,
                repeat: false,
                ..Default::default()
            }),
        )
    }

    /// Straight parent chain: node 0 = root, node i's parent = node i-1.
    fn hierarchy_chain(len: usize) -> NodeHierarchy {
        let nodes = (0..len)
            .map(|i| Node {
                parent: if i == 0 {
                    None
                } else {
                    Some(NodeId::new(i - 1))
                },
                ..Node::ROOT
            })
            .collect::<Vec<_>>();
        NodeHierarchy::new(nodes)
    }

    /// Modifiers with the platform's PRIMARY modifier held (Cmd on macOS, Ctrl elsewhere).
    fn primary_modifiers() -> KeyModifiers {
        if cfg!(target_os = "macos") {
            KeyModifiers::new().with_meta()
        } else {
            KeyModifiers::new().with_ctrl()
        }
    }

    fn keyboard_with_primary_held() -> KeyboardState {
        let key = if cfg!(target_os = "macos") {
            VirtualKeyCode::LWin
        } else {
            VirtualKeyCode::LControl
        };
        KeyboardState {
            pressed_virtual_keycodes: VirtualKeyCodeVec::from_vec(vec![key]),
            ..KeyboardState::default()
        }
    }

    // ============================================================ numeric edge
    // size_changed / quantization

    #[test]
    fn size_changed_zero_and_identity() {
        assert!(!size_changed(LogicalSize::zero(), LogicalSize::zero()));
        assert!(!size_changed(
            LogicalSize::new(0.0, 0.0),
            LogicalSize::new(-0.0, -0.0)
        ));
        // 0 -> any real size is a change.
        assert!(size_changed(
            LogicalSize::zero(),
            LogicalSize::new(0.0, 1.0)
        ));
        assert!(size_changed(
            LogicalSize::zero(),
            LogicalSize::new(1.0, 0.0)
        ));
    }

    #[test]
    fn size_changed_single_sided_nan_is_a_change() {
        // NaN on ONE side only must register as changed (the both-sides NaN case
        // is the loop-guard covered by `size_changed_nan_guard_stops_resize_loop`).
        assert!(size_changed(
            LogicalSize::new(f32::NAN, 10.0),
            LogicalSize::new(10.0, 10.0)
        ));
        assert!(size_changed(
            LogicalSize::new(10.0, 10.0),
            LogicalSize::new(10.0, f32::NAN)
        ));
        // NaN on both sides in *different* dimensions is still a change in the
        // other dimension only if that dimension actually differs.
        assert!(!size_changed(
            LogicalSize::new(f32::NAN, f32::NAN),
            LogicalSize::new(f32::NAN, f32::NAN)
        ));
    }

    #[test]
    fn size_changed_negative_and_infinite_do_not_panic() {
        // Negative sizes (degenerate layouts) must be handled deterministically.
        assert!(size_changed(
            LogicalSize::new(-100.0, 0.0),
            LogicalSize::new(100.0, 0.0)
        ));
        assert!(!size_changed(
            LogicalSize::new(-100.0, -50.0),
            LogicalSize::new(-100.0, -50.0)
        ));
        // f32 * 1000.0 overflows to +/-inf, and `inf as i64` SATURATES (it does
        // not wrap or UB). So the comparison stays total and panic-free.
        assert!(!size_changed(
            LogicalSize::new(f32::INFINITY, f32::INFINITY),
            LogicalSize::new(f32::INFINITY, f32::INFINITY)
        ));
        assert!(size_changed(
            LogicalSize::new(f32::INFINITY, 0.0),
            LogicalSize::new(f32::NEG_INFINITY, 0.0)
        ));
        // Finite-but-huge values saturate into the same bucket as infinity: the
        // documented quantization trade-off, asserted here so a future change of
        // the quantizer (e.g. to i128 or a float compare) is a deliberate one.
        assert!(!size_changed(
            LogicalSize::new(f32::MAX, 0.0),
            LogicalSize::new(f32::INFINITY, 0.0)
        ));
    }

    #[test]
    fn size_changed_ignores_sub_quantum_jitter_but_sees_one_quantum() {
        // The quantizer is 1/1000, so a 0.0005 wobble must be ignored...
        assert!(!size_changed(
            LogicalSize::new(50.0, 50.0),
            LogicalSize::new(50.0004, 50.0)
        ));
        // ...but a full quantum must be seen.
        assert!(size_changed(
            LogicalSize::new(50.0, 50.0),
            LogicalSize::new(50.002, 50.0)
        ));
    }

    // ------------------------------------------------- create_*_event numerics

    #[test]
    fn create_mount_event_without_layout_entry_falls_back_to_zero_rect() {
        let layout: BTreeMap<NodeId, LogicalRect> = BTreeMap::new();
        let ev = create_mount_event(NodeId::new(3), DomId { inner: 0 }, &layout, &tick(7));
        assert_eq!(ev.event_type, EventType::Mount);
        assert_eq!(ev.source, EventSource::Lifecycle);
        assert_eq!(ev.phase, EventPhase::Target);
        assert_eq!(ev.target, ev.current_target);
        assert_eq!(ev.target.node.into_crate_internal(), Some(NodeId::new(3)));
        match ev.data {
            EventData::Lifecycle(d) => {
                assert_eq!(d.reason, LifecycleReason::InitialMount);
                assert!(d.previous_bounds.is_none());
                assert_eq!(d.current_bounds, LogicalRect::zero());
            }
            _ => panic!("mount event must carry lifecycle data"),
        }
    }

    #[test]
    fn create_unmount_event_reports_previous_bounds_and_zero_current() {
        let mut layout = BTreeMap::new();
        let rect = LogicalRect::new(LogicalPosition::new(1.0, 2.0), LogicalSize::new(3.0, 4.0));
        layout.insert(NodeId::new(1), rect);
        let ev = create_unmount_event(NodeId::new(1), DomId { inner: 2 }, &layout, &tick(9));
        assert_eq!(ev.event_type, EventType::Unmount);
        match ev.data {
            EventData::Lifecycle(d) => {
                assert_eq!(d.reason, LifecycleReason::Unmount);
                assert_eq!(d.previous_bounds, Some(rect));
                assert_eq!(d.current_bounds, LogicalRect::zero());
            }
            _ => panic!("unmount event must carry lifecycle data"),
        }
    }

    #[test]
    fn create_lifecycle_event_survives_extreme_node_ids() {
        // The largest NodeId that survives the 1-based (`n + 1`) FFI encoding.
        // (NodeId::new(usize::MAX) would overflow that encoding — out of scope here.)
        let huge = NodeId::new(usize::MAX - 1);
        let layout: BTreeMap<NodeId, LogicalRect> = BTreeMap::new();
        let ev = create_mount_event(huge, DomId { inner: usize::MAX }, &layout, &tick(0));
        assert_eq!(ev.target.node.into_crate_internal(), Some(huge));
        assert_eq!(ev.target.dom, DomId { inner: usize::MAX });

        // NodeId 0 (the root) must round-trip too — the 1-based encoding makes
        // 0 the value most likely to collide with the `None` sentinel.
        let root = create_mount_event(NodeId::ZERO, DomId { inner: 0 }, &layout, &tick(0));
        assert_eq!(
            root.target.node.into_crate_internal(),
            Some(NodeId::ZERO),
            "NodeId 0 must not decode as `None`"
        );
    }

    #[test]
    fn create_resize_event_returns_none_for_missing_or_unchanged_layout() {
        let dom = DomId { inner: 0 };
        let node = NodeId::new(1);
        let rect = LogicalRect::new(LogicalPosition::zero(), LogicalSize::new(10.0, 10.0));

        let empty: BTreeMap<NodeId, LogicalRect> = BTreeMap::new();
        let mut one = BTreeMap::new();
        one.insert(node, rect);

        // Missing in old, missing in new, missing in both -> None (no panic).
        assert!(create_resize_event(node, dom, &empty, &one, &tick(0)).is_none());
        assert!(create_resize_event(node, dom, &one, &empty, &tick(0)).is_none());
        assert!(create_resize_event(node, dom, &empty, &empty, &tick(0)).is_none());
        // Present on both sides but unchanged -> None.
        assert!(create_resize_event(node, dom, &one, &one, &tick(0)).is_none());
    }

    #[test]
    fn create_resize_event_ignores_pure_origin_moves() {
        // Only the SIZE is compared: moving a node without resizing it must not
        // emit a Resize event.
        let dom = DomId { inner: 0 };
        let node = NodeId::new(0);
        let size = LogicalSize::new(10.0, 10.0);
        let mut old = BTreeMap::new();
        old.insert(node, LogicalRect::new(LogicalPosition::new(0.0, 0.0), size));
        let mut new = BTreeMap::new();
        new.insert(
            node,
            LogicalRect::new(LogicalPosition::new(500.0, 500.0), size),
        );
        assert!(create_resize_event(node, dom, &old, &new, &tick(0)).is_none());
    }

    #[test]
    fn create_resize_event_nan_size_does_not_loop_forever() {
        // Regression guard: a NaN dimension on BOTH frames must NOT emit a Resize
        // every frame (a raw f32 `!=` would, since NaN != NaN).
        let dom = DomId { inner: 0 };
        let node = NodeId::new(0);
        let nan_rect = LogicalRect::new(LogicalPosition::zero(), LogicalSize::new(f32::NAN, 100.0));
        let mut old = BTreeMap::new();
        old.insert(node, nan_rect);
        let mut new = BTreeMap::new();
        new.insert(node, nan_rect);
        assert!(create_resize_event(node, dom, &old, &new, &tick(0)).is_none());
    }

    #[test]
    fn create_resize_event_reports_both_bounds_on_real_change() {
        let dom = DomId { inner: 0 };
        let node = NodeId::new(0);
        let old_rect = LogicalRect::new(LogicalPosition::zero(), LogicalSize::new(10.0, 10.0));
        let new_rect = LogicalRect::new(LogicalPosition::zero(), LogicalSize::new(10.0, 20.0));
        let mut old = BTreeMap::new();
        old.insert(node, old_rect);
        let mut new = BTreeMap::new();
        new.insert(node, new_rect);

        let ev = create_resize_event(node, dom, &old, &new, &tick(3))
            .expect("a real size change must emit a Resize");
        assert_eq!(ev.event_type, EventType::Resize);
        match ev.data {
            EventData::Lifecycle(d) => {
                assert_eq!(d.reason, LifecycleReason::Resize);
                assert_eq!(d.previous_bounds, Some(old_rect));
                assert_eq!(d.current_bounds, new_rect);
            }
            _ => panic!("resize event must carry lifecycle data"),
        }
    }

    // ------------------------------------------------- detect_lifecycle_events

    #[test]
    fn detect_lifecycle_events_all_none_is_empty() {
        let events = detect_lifecycle_events(
            DomId { inner: 0 },
            DomId { inner: 0 },
            None,
            None,
            None,
            None,
            tick(0),
        );
        assert!(events.is_empty());
    }

    #[test]
    fn detect_lifecycle_events_without_layout_emits_nothing() {
        // Hierarchies differ, but no layout maps -> the fn must not fabricate events.
        let old = hierarchy_chain(1);
        let new = hierarchy_chain(4);
        let events = detect_lifecycle_events(
            DomId { inner: 0 },
            DomId { inner: 0 },
            Some(&old),
            Some(&new),
            None,
            None,
            tick(0),
        );
        assert!(events.is_empty());
    }

    #[test]
    fn detect_lifecycle_events_emits_mounts_unmounts_and_resizes() {
        let dom = DomId { inner: 0 };
        let old_hier = hierarchy_chain(2); // nodes 0,1
        let new_hier = hierarchy_chain(3); // nodes 0,1,2

        let r = |h: f32| LogicalRect::new(LogicalPosition::zero(), LogicalSize::new(10.0, h));
        let mut old_layout = BTreeMap::new();
        old_layout.insert(NodeId::new(0), r(10.0));
        old_layout.insert(NodeId::new(1), r(10.0));
        let mut new_layout = BTreeMap::new();
        new_layout.insert(NodeId::new(0), r(10.0)); // unchanged
        new_layout.insert(NodeId::new(1), r(99.0)); // resized
        new_layout.insert(NodeId::new(2), r(10.0)); // mounted

        let events = detect_lifecycle_events(
            dom,
            dom,
            Some(&old_hier),
            Some(&new_hier),
            Some(&old_layout),
            Some(&new_layout),
            tick(5),
        );

        let mounts: Vec<_> = events
            .iter()
            .filter(|e| e.event_type == EventType::Mount)
            .collect();
        let resizes: Vec<_> = events
            .iter()
            .filter(|e| e.event_type == EventType::Resize)
            .collect();
        assert_eq!(mounts.len(), 1, "only node 2 is new");
        assert_eq!(
            mounts[0].target.node.into_crate_internal(),
            Some(NodeId::new(2))
        );
        assert_eq!(resizes.len(), 1, "only node 1 changed size");
        assert_eq!(
            resizes[0].target.node.into_crate_internal(),
            Some(NodeId::new(1))
        );
        assert!(
            !events.iter().any(|e| e.event_type == EventType::Unmount),
            "nothing was removed"
        );
        assert!(events.iter().all(|e| e.source == EventSource::Lifecycle));

        // Reverse direction: the removed node must unmount.
        let events = detect_lifecycle_events(
            dom,
            dom,
            Some(&new_hier),
            Some(&old_hier),
            Some(&new_layout),
            Some(&old_layout),
            tick(6),
        );
        let unmounts: Vec<_> = events
            .iter()
            .filter(|e| e.event_type == EventType::Unmount)
            .collect();
        assert_eq!(unmounts.len(), 1);
        assert_eq!(
            unmounts[0].target.node.into_crate_internal(),
            Some(NodeId::new(2))
        );
    }

    #[test]
    fn detect_lifecycle_events_mount_of_node_missing_from_layout_uses_zero_rect() {
        let dom = DomId { inner: 0 };
        let new_hier = hierarchy_chain(2);
        let new_layout: BTreeMap<NodeId, LogicalRect> = BTreeMap::new(); // empty!
        let events = detect_lifecycle_events(
            dom,
            dom,
            None,
            Some(&new_hier),
            None,
            Some(&new_layout),
            tick(0),
        );
        assert_eq!(events.len(), 2);
        for ev in &events {
            match ev.data {
                EventData::Lifecycle(d) => assert_eq!(d.current_bounds, LogicalRect::zero()),
                _ => panic!("expected lifecycle data"),
            }
        }
    }

    #[test]
    fn collect_node_ids_handles_none_and_empty_hierarchies() {
        assert!(collect_node_ids(None).is_empty());
        let empty = NodeHierarchy::new(Vec::new());
        assert!(collect_node_ids(Some(&empty)).is_empty());
        let three = hierarchy_chain(3);
        let ids = collect_node_ids(Some(&three));
        assert_eq!(ids.len(), 3);
        assert!(ids.contains(&NodeId::ZERO));
        assert!(ids.contains(&NodeId::new(2)));
    }

    // ======================================================== getters/predicates

    #[test]
    fn process_event_result_order_is_dense_and_monotonic() {
        let all = [
            ProcessEventResult::DoNothing,
            ProcessEventResult::ShouldReRenderCurrentWindow,
            ProcessEventResult::ShouldUpdateDisplayListCurrentWindow,
            ProcessEventResult::UpdateHitTesterAndProcessAgain,
            ProcessEventResult::ShouldIncrementalRelayout,
            ProcessEventResult::ShouldRegenerateDomCurrentWindow,
            ProcessEventResult::ShouldRegenerateDomAllWindows,
        ];
        for (i, r) in all.iter().enumerate() {
            assert_eq!(r.order(), i, "order() must match declaration index");
        }
        // Ord/PartialOrd must agree with order(), and max_self must be the join.
        for a in all {
            for b in all {
                assert_eq!(a < b, a.order() < b.order());
                let joined = a.max_self(b);
                assert_eq!(joined.order(), a.order().max(b.order()));
                assert_eq!(joined, b.max_self(a), "max_self must be commutative");
                assert_eq!(a.max_self(a), a, "max_self must be idempotent");
            }
        }
    }

    #[test]
    fn key_modifiers_builders_are_orthogonal_and_is_empty_tracks_them() {
        let empty = KeyModifiers::new();
        assert!(empty.is_empty());
        assert_eq!(empty, KeyModifiers::default());

        // Each builder sets exactly one flag.
        assert_eq!(
            KeyModifiers::new().with_shift(),
            KeyModifiers {
                shift: true,
                ctrl: false,
                alt: false,
                meta: false
            }
        );
        assert_eq!(
            KeyModifiers::new().with_ctrl(),
            KeyModifiers {
                shift: false,
                ctrl: true,
                alt: false,
                meta: false
            }
        );
        assert_eq!(
            KeyModifiers::new().with_alt(),
            KeyModifiers {
                shift: false,
                ctrl: false,
                alt: true,
                meta: false
            }
        );
        assert_eq!(
            KeyModifiers::new().with_meta(),
            KeyModifiers {
                shift: false,
                ctrl: false,
                alt: false,
                meta: true
            }
        );

        // Any single flag defeats is_empty; builders are idempotent and composable.
        assert!(!KeyModifiers::new().with_shift().is_empty());
        assert!(!KeyModifiers::new().with_ctrl().is_empty());
        assert!(!KeyModifiers::new().with_alt().is_empty());
        assert!(!KeyModifiers::new().with_meta().is_empty());
        assert_eq!(
            KeyModifiers::new().with_ctrl().with_ctrl(),
            KeyModifiers::new().with_ctrl()
        );
        let all = KeyModifiers::new()
            .with_shift()
            .with_ctrl()
            .with_alt()
            .with_meta();
        assert!(!all.is_empty());
        assert!(all.shift && all.ctrl && all.alt && all.meta);
    }

    #[test]
    fn scroll_into_view_options_presets_and_behavior_setters() {
        assert_eq!(
            ScrollIntoViewOptions::default(),
            ScrollIntoViewOptions::nearest(),
            "Default must be the `nearest`/`auto` preset"
        );
        for (opts, expected) in [
            (
                ScrollIntoViewOptions::nearest(),
                ScrollLogicalPosition::Nearest,
            ),
            (
                ScrollIntoViewOptions::center(),
                ScrollLogicalPosition::Center,
            ),
            (ScrollIntoViewOptions::start(), ScrollLogicalPosition::Start),
            (ScrollIntoViewOptions::end(), ScrollLogicalPosition::End),
        ] {
            assert_eq!(opts.block, expected);
            assert_eq!(
                opts.inline_axis, expected,
                "both axes must be aligned alike"
            );
            assert_eq!(opts.behavior, ScrollIntoViewBehavior::Auto);

            // The behavior setters must not disturb the axes, and last-writer-wins.
            let instant = opts.with_instant();
            assert_eq!(instant.behavior, ScrollIntoViewBehavior::Instant);
            assert_eq!(instant.block, opts.block);
            assert_eq!(instant.inline_axis, opts.inline_axis);

            let smooth = opts.with_smooth();
            assert_eq!(smooth.behavior, ScrollIntoViewBehavior::Smooth);
            assert_eq!(
                opts.with_instant().with_smooth().behavior,
                ScrollIntoViewBehavior::Smooth
            );
            assert_eq!(
                opts.with_smooth().with_instant().behavior,
                ScrollIntoViewBehavior::Instant
            );
        }
    }

    #[test]
    fn default_action_result_has_action_predicate() {
        assert!(!DefaultActionResult::default().has_action());
        assert!(!DefaultActionResult::prevented().has_action());
        assert!(DefaultActionResult::prevented().prevented);
        assert_eq!(DefaultActionResult::prevented().action, DefaultAction::None);

        // `None` action => nothing to do, even though it was not prevented.
        let none = DefaultActionResult::new(DefaultAction::None);
        assert!(!none.prevented);
        assert!(!none.has_action());

        // Any real action => has_action.
        for action in [
            DefaultAction::FocusNext,
            DefaultAction::FocusPrevious,
            DefaultAction::FocusFirst,
            DefaultAction::FocusLast,
            DefaultAction::ClearFocus,
            DefaultAction::SelectAllText,
            DefaultAction::ActivateFocusedElement { target: dnid(0, 1) },
            DefaultAction::SubmitForm {
                form_node: dnid(0, 1),
            },
            DefaultAction::CloseModal {
                modal_node: dnid(0, 1),
            },
            DefaultAction::ScrollFocusedContainer {
                direction: ScrollDirection::Down,
                amount: ScrollAmount::Page,
            },
        ] {
            let r = DefaultActionResult::new(action);
            assert_eq!(r.action, action);
            assert!(!r.prevented);
            assert!(r.has_action(), "{action:?} must be reported as actionable");
        }
    }

    #[test]
    fn synthetic_event_constructor_invariants_and_flag_transitions() {
        let target = dnid(3, 7);
        let mut ev = SyntheticEvent::new(
            EventType::Click,
            EventSource::Programmatic,
            target,
            tick(42),
            EventData::None,
        );
        // Post-construction invariants.
        assert_eq!(ev.event_type, EventType::Click);
        assert_eq!(ev.source, EventSource::Programmatic);
        assert_eq!(ev.phase, EventPhase::Target);
        assert_eq!(ev.target, target);
        assert_eq!(ev.current_target, target);
        assert_eq!(ev.timestamp, tick(42));
        assert!(!ev.is_propagation_stopped());
        assert!(!ev.is_immediate_propagation_stopped());
        assert!(!ev.is_default_prevented());

        // stop_propagation does NOT imply stop_immediate_propagation...
        ev.stop_propagation();
        assert!(ev.is_propagation_stopped());
        assert!(!ev.is_immediate_propagation_stopped());

        // ...but the reverse implication MUST hold, or propagate_phase's
        // `stopped_immediate` check could be bypassed by the `stopped` fast path.
        let mut ev2 = SyntheticEvent::new(
            EventType::Click,
            EventSource::User,
            target,
            tick(0),
            EventData::None,
        );
        ev2.stop_immediate_propagation();
        assert!(ev2.is_immediate_propagation_stopped());
        assert!(
            ev2.is_propagation_stopped(),
            "immediate stop must also stop normal propagation"
        );

        // All three flags are idempotent and independent.
        let mut ev3 = ev2.clone();
        ev3.stop_immediate_propagation();
        ev3.prevent_default();
        ev3.prevent_default();
        assert!(ev3.is_default_prevented());
        assert!(
            !ev.is_default_prevented(),
            "flags must not leak across events"
        );
    }

    #[test]
    fn hover_filter_is_system_internal_only_for_system_text_clicks() {
        for f in [
            HoverEventFilter::SystemTextSingleClick,
            HoverEventFilter::SystemTextDoubleClick,
            HoverEventFilter::SystemTextTripleClick,
        ] {
            assert!(f.is_system_internal(), "{f:?} is internal");
            assert!(
                f.to_focus_event_filter().is_none(),
                "internal filters must never be exposed as focus callbacks"
            );
        }
        for f in [
            HoverEventFilter::MouseOver,
            HoverEventFilter::MouseDown,
            HoverEventFilter::Drop,
            HoverEventFilter::KeyringResult,
            HoverEventFilter::MouseOut,
        ] {
            assert!(!f.is_system_internal(), "{f:?} is a user-visible filter");
        }
    }

    #[test]
    fn event_filter_kind_predicates_are_mutually_exclusive() {
        let hover = EventFilter::Hover(HoverEventFilter::MouseDown);
        let focus = EventFilter::Focus(FocusEventFilter::FocusReceived);
        let window = EventFilter::Window(WindowEventFilter::Resized);
        let component = EventFilter::Component(ComponentEventFilter::AfterMount);
        let app = EventFilter::Application(ApplicationEventFilter::DeviceConnected);

        assert!(focus.is_focus_callback());
        assert!(window.is_window_callback());
        for f in [hover, window, component, app] {
            assert!(!f.is_focus_callback(), "{f:?} is not a focus callback");
        }
        for f in [hover, focus, component, app] {
            assert!(!f.is_window_callback(), "{f:?} is not a window callback");
        }
        // The `as_*` accessors must agree with the predicates.
        assert_eq!(
            hover.as_hover_event_filter(),
            Some(HoverEventFilter::MouseDown)
        );
        assert_eq!(hover.as_focus_event_filter(), None);
        assert_eq!(hover.as_window_event_filter(), None);
        assert_eq!(
            focus.as_focus_event_filter(),
            Some(FocusEventFilter::FocusReceived)
        );
        assert_eq!(
            window.as_window_event_filter(),
            Some(WindowEventFilter::Resized)
        );
        assert_eq!(component.as_hover_event_filter(), None);
    }

    // ============================================================== round-trips

    #[test]
    fn window_to_hover_filter_mapping_never_yields_an_internal_filter() {
        // Every window filter that has a hover twin must map onto a filter the
        // user is actually allowed to register (never a SystemText* internal).
        for w in [
            WindowEventFilter::MouseOver,
            WindowEventFilter::MouseDown,
            WindowEventFilter::LeftMouseDown,
            WindowEventFilter::RightMouseDown,
            WindowEventFilter::MiddleMouseDown,
            WindowEventFilter::MouseUp,
            WindowEventFilter::LeftMouseUp,
            WindowEventFilter::RightMouseUp,
            WindowEventFilter::MiddleMouseUp,
            WindowEventFilter::Scroll,
            WindowEventFilter::TextInput,
            WindowEventFilter::VirtualKeyDown,
            WindowEventFilter::VirtualKeyUp,
            WindowEventFilter::HoveredFile,
            WindowEventFilter::DroppedFile,
            WindowEventFilter::HoveredFileCancelled,
            WindowEventFilter::TouchStart,
            WindowEventFilter::TouchEnd,
            WindowEventFilter::PenDown,
            WindowEventFilter::DragStart,
            WindowEventFilter::Drop,
            WindowEventFilter::DoubleClick,
            WindowEventFilter::PermissionChanged,
            WindowEventFilter::BiometricResult,
            WindowEventFilter::ScreenColorPicked,
            WindowEventFilter::KeyringResult,
        ] {
            let hover = w
                .to_hover_event_filter()
                .unwrap_or_else(|| panic!("{w:?} should have a hover twin"));
            assert!(
                !hover.is_system_internal(),
                "{w:?} must not map onto an internal filter"
            );
        }

        // Window-only events have deliberately NO hover twin.
        for w in [
            WindowEventFilter::MouseEnter,
            WindowEventFilter::MouseLeave,
            WindowEventFilter::Resized,
            WindowEventFilter::Moved,
            WindowEventFilter::FocusReceived,
            WindowEventFilter::FocusLost,
            WindowEventFilter::CloseRequested,
            WindowEventFilter::ThemeChanged,
            WindowEventFilter::WindowFocusReceived,
            WindowEventFilter::WindowFocusLost,
            WindowEventFilter::DpiChanged,
            WindowEventFilter::MonitorChanged,
        ] {
            assert_eq!(
                w.to_hover_event_filter(),
                None,
                "{w:?} is window-specific and must not map to a hover filter"
            );
        }
    }

    #[test]
    fn window_hover_focus_filter_names_round_trip() {
        // Window -> Hover -> Focus must preserve the *identity* of the event for
        // the shared (mouse / key / drag) subset — a mismatched row here means a
        // callback registered as Focus(X) would fire for hover event Y.
        let pairs = [
            (
                WindowEventFilter::MouseOver,
                HoverEventFilter::MouseOver,
                Some(FocusEventFilter::MouseOver),
            ),
            (
                WindowEventFilter::LeftMouseDown,
                HoverEventFilter::LeftMouseDown,
                Some(FocusEventFilter::LeftMouseDown),
            ),
            (
                WindowEventFilter::RightMouseUp,
                HoverEventFilter::RightMouseUp,
                Some(FocusEventFilter::RightMouseUp),
            ),
            (
                WindowEventFilter::TextInput,
                HoverEventFilter::TextInput,
                Some(FocusEventFilter::TextInput),
            ),
            (
                WindowEventFilter::VirtualKeyDown,
                HoverEventFilter::VirtualKeyDown,
                Some(FocusEventFilter::VirtualKeyDown),
            ),
            (
                WindowEventFilter::DragStart,
                HoverEventFilter::DragStart,
                Some(FocusEventFilter::DragStart),
            ),
            (
                WindowEventFilter::Drop,
                HoverEventFilter::Drop,
                Some(FocusEventFilter::Drop),
            ),
            // File events exist on window + hover, but have no focus twin.
            (
                WindowEventFilter::DroppedFile,
                HoverEventFilter::DroppedFile,
                None,
            ),
            (
                WindowEventFilter::TouchStart,
                HoverEventFilter::TouchStart,
                None,
            ),
        ];
        for (w, h, f) in pairs {
            assert_eq!(
                w.to_hover_event_filter(),
                Some(h),
                "window->hover for {w:?}"
            );
            assert_eq!(h.to_focus_event_filter(), f, "hover->focus for {h:?}");
        }
    }

    #[test]
    fn on_to_event_filter_conversion_is_stable() {
        use crate::dom::On;
        // On::TextInput / FocusReceived / FocusLost are FOCUS filters, and the
        // virtual-key events are WINDOW filters — everything else is Hover.
        assert_eq!(
            EventFilter::from(On::TextInput),
            EventFilter::Focus(FocusEventFilter::TextInput)
        );
        assert_eq!(
            EventFilter::from(On::VirtualKeyDown),
            EventFilter::Window(WindowEventFilter::VirtualKeyDown)
        );
        assert_eq!(
            EventFilter::from(On::MouseOver),
            EventFilter::Hover(HoverEventFilter::MouseOver)
        );
        // The a11y actions all collapse onto the ACTIVATION filter. They used
        // to map to `MouseUp`, which conflated "the user activated this" with
        // a raw pointer release - and a raw release is not something a screen
        // reader can produce.
        for on in [
            On::Default,
            On::Collapse,
            On::Expand,
            On::Increment,
            On::Decrement,
        ] {
            assert_eq!(
                EventFilter::from(on),
                EventFilter::Hover(HoverEventFilter::Click),
                "{on:?} must map to the click filter"
            );
        }
        assert!(EventFilter::from(On::TextInput).is_focus_callback());
        assert!(EventFilter::from(On::VirtualKeyUp).is_window_callback());
    }

    #[test]
    fn virtual_keycode_round_trips_for_every_key_events_rs_interprets() {
        // handle_key_down decodes `KeyboardEventData.key_code` with `from_u32`,
        // while producers write `vk as u32`. If that round-trip ever breaks, every
        // shortcut silently dies — so pin it for the keys this module interprets.
        for vk in [
            VirtualKeyCode::Left,
            VirtualKeyCode::Right,
            VirtualKeyCode::Up,
            VirtualKeyCode::Down,
            VirtualKeyCode::Home,
            VirtualKeyCode::End,
            VirtualKeyCode::Back,
            VirtualKeyCode::Delete,
            VirtualKeyCode::A,
            VirtualKeyCode::C,
            VirtualKeyCode::D,
            VirtualKeyCode::V,
            VirtualKeyCode::X,
            VirtualKeyCode::Y,
            VirtualKeyCode::Z,
        ] {
            assert_eq!(
                VirtualKeyCode::from_u32(vk as u32),
                Some(vk),
                "{vk:?} must survive the as-u32 / from_u32 round trip"
            );
        }
        // Out-of-range key codes must decode to None rather than index out of bounds.
        assert_eq!(VirtualKeyCode::from_u32(u32::MAX), None);
        assert_eq!(VirtualKeyCode::from_u32(100_000), None);
    }

    // ============================================ ArrowDirection / KeyboardShortcut

    #[test]
    fn arrow_direction_from_key_is_total_over_every_decodable_key() {
        // Fuzz every decodable key code (plus the undecodable tail) through both
        // key mappers: they must never panic and must only claim the nav keys.
        let nav = [
            VirtualKeyCode::Left,
            VirtualKeyCode::Right,
            VirtualKeyCode::Up,
            VirtualKeyCode::Down,
            VirtualKeyCode::Home,
            VirtualKeyCode::End,
        ];
        for raw in 0u32..1024 {
            let Some(vk) = VirtualKeyCode::from_u32(raw) else {
                continue;
            };
            for ctrl in [false, true] {
                let got = ArrowDirection::from_key(vk, ctrl);
                assert_eq!(
                    got.is_some(),
                    nav.contains(&vk),
                    "{vk:?} (ctrl={ctrl}) must map to an ArrowDirection iff it is a nav key"
                );
                if let Some(dir) = got {
                    // to_selection is total and never panics for any (dir, ctrl).
                    let (_d, _s) = dir.to_selection(ctrl);
                }
            }
        }
    }

    #[test]
    fn arrow_direction_ctrl_only_upgrades_horizontal_arrows_to_words() {
        // ctrl must upgrade Left/Right to Word steps, and must NOT change the
        // step for Up/Down/Home/End (those are already line/document scoped).
        for (dir, expect_no_ctrl, expect_ctrl) in [
            (
                ArrowDirection::Left,
                (SelectionDirection::Backward, SelectionStep::Character),
                (SelectionDirection::Backward, SelectionStep::Word),
            ),
            (
                ArrowDirection::Right,
                (SelectionDirection::Forward, SelectionStep::Character),
                (SelectionDirection::Forward, SelectionStep::Word),
            ),
            (
                ArrowDirection::Up,
                (SelectionDirection::Backward, SelectionStep::VisualLine),
                (SelectionDirection::Backward, SelectionStep::VisualLine),
            ),
            (
                ArrowDirection::Down,
                (SelectionDirection::Forward, SelectionStep::VisualLine),
                (SelectionDirection::Forward, SelectionStep::VisualLine),
            ),
            (
                ArrowDirection::LineStart,
                (SelectionDirection::Backward, SelectionStep::Line),
                (SelectionDirection::Backward, SelectionStep::Line),
            ),
            (
                ArrowDirection::DocumentEnd,
                (SelectionDirection::Forward, SelectionStep::Document),
                (SelectionDirection::Forward, SelectionStep::Document),
            ),
        ] {
            assert_eq!(dir.to_selection(false), expect_no_ctrl, "{dir:?} plain");
            assert_eq!(dir.to_selection(true), expect_ctrl, "{dir:?} + ctrl");
        }
        // Ctrl+Home/End are distinct DIRECTIONS (not a step upgrade).
        assert_eq!(
            ArrowDirection::from_key(VirtualKeyCode::Home, true),
            Some(ArrowDirection::DocumentStart)
        );
        assert_eq!(
            ArrowDirection::from_key(VirtualKeyCode::End, true),
            Some(ArrowDirection::DocumentEnd)
        );
    }

    #[test]
    fn keyboard_shortcut_from_key_requires_primary_for_every_key() {
        // Without the primary modifier NO key may produce a shortcut — otherwise
        // typing plain "c" into a text field would copy.
        for raw in 0u32..1024 {
            let Some(vk) = VirtualKeyCode::from_u32(raw) else {
                continue;
            };
            for shift in [false, true] {
                assert_eq!(
                    KeyboardShortcut::from_key(vk, false, shift),
                    None,
                    "{vk:?} (shift={shift}) must need the primary modifier"
                );
            }
        }
        // With primary held, exactly the editing set is recognised.
        assert_eq!(
            KeyboardShortcut::from_key(VirtualKeyCode::Z, true, true),
            Some(KeyboardShortcut::Redo),
            "primary+shift+Z is Redo, not Undo"
        );
        assert_eq!(
            KeyboardShortcut::from_key(VirtualKeyCode::Y, true, true),
            Some(KeyboardShortcut::Redo),
            "shift must not disturb primary+Y"
        );
        assert_eq!(
            KeyboardShortcut::from_key(VirtualKeyCode::C, true, true),
            Some(KeyboardShortcut::Copy),
            "shift must not disturb primary+C"
        );
        // D is handled separately (SelectNextOccurrence), not as a KeyboardShortcut.
        assert_eq!(
            KeyboardShortcut::from_key(VirtualKeyCode::D, true, false),
            None
        );
    }

    #[test]
    fn selection_op_new_defaults_to_a_single_repeat() {
        let op = SelectionOp::new(
            SelectionDirection::Forward,
            SelectionStep::Word,
            SelectionMode::Delete,
        );
        assert_eq!(op.direction, SelectionDirection::Forward);
        assert_eq!(op.step, SelectionStep::Word);
        assert_eq!(op.mode, SelectionMode::Delete);
        assert_eq!(op.repeat, 1, "a fresh op must apply exactly once");
    }

    // ================================================== filter/phase matching

    #[test]
    fn capture_phase_never_matches_any_filter() {
        // Regression guard: azul has no capture listeners. If this breaks, every
        // ancestor callback fires TWICE (once capturing, once bubbling).
        let ev = mouse_event(
            EventType::MouseDown,
            MouseButton::Left,
            LogicalPosition::zero(),
        );
        for filter in [
            EventFilter::Hover(HoverEventFilter::MouseDown),
            EventFilter::Hover(HoverEventFilter::LeftMouseDown),
            EventFilter::Focus(FocusEventFilter::MouseDown),
            EventFilter::Window(WindowEventFilter::MouseDown),
            EventFilter::Component(ComponentEventFilter::AfterMount),
            EventFilter::Application(ApplicationEventFilter::DeviceConnected),
        ] {
            assert!(
                !matches_filter_phase(filter, &ev, EventPhase::Capture),
                "{filter:?} must not match in the capture phase"
            );
        }
        // ...but the same filter DOES match at Target and Bubble.
        for phase in [EventPhase::Target, EventPhase::Bubble] {
            assert!(matches_filter_phase(
                EventFilter::Hover(HoverEventFilter::MouseDown),
                &ev,
                phase
            ));
        }
    }

    #[test]
    fn application_filters_never_match_yet() {
        // Documented stub: Application events are not routed through propagation.
        let ev = mouse_event(
            EventType::MouseDown,
            MouseButton::Left,
            LogicalPosition::zero(),
        );
        for phase in [EventPhase::Capture, EventPhase::Target, EventPhase::Bubble] {
            assert!(!matches_filter_phase(
                EventFilter::Application(ApplicationEventFilter::MonitorConnected),
                &ev,
                phase
            ));
        }
    }

    #[test]
    fn check_mouse_button_is_false_for_every_non_mouse_payload() {
        for data in [
            EventData::None,
            EventData::Keyboard(KeyboardEventData {
                key_code: 0,
                char_code: None,
                modifiers: KeyModifiers::default(),
                repeat: false,
                ..Default::default()
            }),
            EventData::Touch(TouchEventData {
                id: u64::MAX,
                position: LogicalPosition::zero(),
                force: f32::NAN,
            }),
            EventData::Clipboard(ClipboardEventData { content: None }),
        ] {
            for button in [MouseButton::Left, MouseButton::Right, MouseButton::Middle] {
                assert!(
                    !check_mouse_button(&data, button),
                    "non-mouse payload must never claim a button"
                );
            }
        }
        // Exotic button ids compare by value, including the u8 boundary.
        let other_max = EventData::Mouse(MouseEventData {
            position: LogicalPosition::zero(),
            button: MouseButton::Other(u8::MAX),
            buttons: u8::MAX,
            modifiers: KeyModifiers::default(),
            ..Default::default()
        });
        assert!(check_mouse_button(&other_max, MouseButton::Other(u8::MAX)));
        assert!(!check_mouse_button(&other_max, MouseButton::Other(0)));
        assert!(!check_mouse_button(&other_max, MouseButton::Left));
    }

    #[test]
    fn button_specific_filters_require_the_matching_button() {
        let left = mouse_event(
            EventType::MouseDown,
            MouseButton::Left,
            LogicalPosition::zero(),
        );
        let right = mouse_event(
            EventType::MouseDown,
            MouseButton::Right,
            LogicalPosition::zero(),
        );
        let middle = mouse_event(
            EventType::MouseDown,
            MouseButton::Middle,
            LogicalPosition::zero(),
        );

        // The generic filter fires for every button...
        for ev in [&left, &right, &middle] {
            assert!(matches_hover_filter(
                HoverEventFilter::MouseDown,
                ev,
                EventPhase::Target
            ));
        }
        // ...the specific ones only for theirs.
        assert!(matches_hover_filter(
            HoverEventFilter::LeftMouseDown,
            &left,
            EventPhase::Target
        ));
        assert!(!matches_hover_filter(
            HoverEventFilter::LeftMouseDown,
            &right,
            EventPhase::Target
        ));
        assert!(matches_hover_filter(
            HoverEventFilter::RightMouseDown,
            &right,
            EventPhase::Target
        ));
        assert!(!matches_hover_filter(
            HoverEventFilter::MiddleMouseDown,
            &right,
            EventPhase::Target
        ));
        assert!(matches_hover_filter(
            HoverEventFilter::MiddleMouseDown,
            &middle,
            EventPhase::Target
        ));

        // A MouseDown filter must never fire on a MouseUp event and vice versa.
        let up = mouse_event(
            EventType::MouseUp,
            MouseButton::Left,
            LogicalPosition::zero(),
        );
        assert!(!matches_hover_filter(
            HoverEventFilter::MouseDown,
            &up,
            EventPhase::Target
        ));
        assert!(!matches_hover_filter(
            HoverEventFilter::MouseUp,
            &left,
            EventPhase::Target
        ));
        assert!(matches_focus_filter(
            FocusEventFilter::LeftMouseUp,
            &up,
            EventPhase::Target
        ));
        assert!(matches_window_filter(
            WindowEventFilter::LeftMouseUp,
            &up,
            EventPhase::Target
        ));

        // A MouseDown event carrying a NON-mouse payload cannot satisfy a
        // button-specific filter (there is no button to compare against).
        let payloadless = SyntheticEvent::new(
            EventType::MouseDown,
            EventSource::Synthetic,
            dnid(0, 0),
            tick(0),
            EventData::None,
        );
        assert!(matches_hover_filter(
            HoverEventFilter::MouseDown,
            &payloadless,
            EventPhase::Target
        ));
        assert!(!matches_hover_filter(
            HoverEventFilter::LeftMouseDown,
            &payloadless,
            EventPhase::Target
        ));
    }

    #[test]
    fn component_filter_matches_only_its_own_lifecycle_event() {
        let lifecycle = |ty: EventType| {
            SyntheticEvent::new(
                ty,
                EventSource::Lifecycle,
                dnid(0, 0),
                tick(0),
                EventData::None,
            )
        };
        let pairs = [
            (ComponentEventFilter::AfterMount, EventType::Mount),
            (ComponentEventFilter::BeforeUnmount, EventType::Unmount),
            (ComponentEventFilter::Updated, EventType::Update),
            (ComponentEventFilter::NodeResized, EventType::Resize),
            (ComponentEventFilter::Dismissed, EventType::Dismiss),
            (ComponentEventFilter::TornOff, EventType::TearOff),
            (ComponentEventFilter::Docked, EventType::Dock),
        ];
        for (filter, ty) in pairs {
            let ev = lifecycle(ty);
            assert!(
                matches_component_filter(filter, &ev, EventPhase::Target),
                "{filter:?} must match {ty:?}"
            );
            // ...and must NOT match any of the other lifecycle event types.
            for (_, other_ty) in pairs.iter().filter(|(_, t)| *t != ty) {
                assert!(
                    !matches_component_filter(filter, &lifecycle(*other_ty), EventPhase::Target),
                    "{filter:?} must not match {other_ty:?}"
                );
            }
        }
        // DefaultAction / Selected have no EventType twin: they must never match
        // a lifecycle event (they are driven by the a11y layer instead).
        for filter in [
            ComponentEventFilter::DefaultAction,
            ComponentEventFilter::Selected,
        ] {
            for (_, ty) in pairs {
                assert!(!matches_component_filter(
                    filter,
                    &lifecycle(ty),
                    EventPhase::Target
                ));
            }
        }
    }

    /// The next `EventType` in declaration order, or `None` at the end.
    ///
    /// THIS IS THE ENUMERATION, and it is sound in a way a hand-written array
    /// is not: the match is exhaustive, so adding a variant fails to compile
    /// until it is spliced into the chain - and splicing it in automatically
    /// puts it in the walk. A parallel `ALL_EVENT_TYPES` array would let a new
    /// variant be added to a match and forgotten in the list, which is how
    /// this ratchet came to cover 65% of the enum while reading as green, and
    /// how `TIER1_SLOTS` let `cursor` overwrite `align-self`.
    fn next_event_type(ty: EventType) -> Option<EventType> {
        Some(match ty {
            EventType::MouseOver => EventType::MouseEnter,
            EventType::MouseEnter => EventType::MouseLeave,
            EventType::MouseLeave => EventType::MouseOut,
            EventType::MouseOut => EventType::MouseDown,
            EventType::MouseDown => EventType::MouseUp,
            EventType::MouseUp => EventType::Click,
            EventType::Click => EventType::DoubleClick,
            EventType::DoubleClick => EventType::ContextMenu,
            EventType::ContextMenu => EventType::KeyDown,
            EventType::KeyDown => EventType::KeyUp,
            EventType::KeyUp => EventType::KeyPress,
            EventType::KeyPress => EventType::CompositionStart,
            EventType::CompositionStart => EventType::CompositionUpdate,
            EventType::CompositionUpdate => EventType::CompositionEnd,
            EventType::CompositionEnd => EventType::Focus,
            EventType::Focus => EventType::Blur,
            EventType::Blur => EventType::FocusIn,
            EventType::FocusIn => EventType::FocusOut,
            EventType::FocusOut => EventType::Input,
            EventType::Input => EventType::Change,
            EventType::Change => EventType::Submit,
            EventType::Submit => EventType::Reset,
            EventType::Reset => EventType::Invalid,
            EventType::Invalid => EventType::Scroll,
            EventType::Scroll => EventType::ScrollStart,
            EventType::ScrollStart => EventType::ScrollEnd,
            EventType::ScrollEnd => EventType::DragStart,
            EventType::DragStart => EventType::Drag,
            EventType::Drag => EventType::DragEnd,
            EventType::DragEnd => EventType::DragEnter,
            EventType::DragEnter => EventType::DragOver,
            EventType::DragOver => EventType::DragLeave,
            EventType::DragLeave => EventType::Drop,
            EventType::Drop => EventType::TouchStart,
            EventType::TouchStart => EventType::TouchMove,
            EventType::TouchMove => EventType::TouchEnd,
            EventType::TouchEnd => EventType::TouchCancel,
            EventType::TouchCancel => EventType::PenDown,
            EventType::PenDown => EventType::PenMove,
            EventType::PenMove => EventType::PenUp,
            EventType::PenUp => EventType::PenEnter,
            EventType::PenEnter => EventType::PenLeave,
            EventType::PenLeave => EventType::LongPress,
            EventType::LongPress => EventType::SwipeLeft,
            EventType::SwipeLeft => EventType::SwipeRight,
            EventType::SwipeRight => EventType::SwipeUp,
            EventType::SwipeUp => EventType::SwipeDown,
            EventType::SwipeDown => EventType::PinchIn,
            EventType::PinchIn => EventType::PinchOut,
            EventType::PinchOut => EventType::RotateClockwise,
            EventType::RotateClockwise => EventType::RotateCounterClockwise,
            EventType::RotateCounterClockwise => EventType::Copy,
            EventType::Copy => EventType::Cut,
            EventType::Cut => EventType::Paste,
            EventType::Paste => EventType::Play,
            EventType::Play => EventType::Pause,
            EventType::Pause => EventType::Ended,
            EventType::Ended => EventType::TimeUpdate,
            EventType::TimeUpdate => EventType::VolumeChange,
            EventType::VolumeChange => EventType::MediaError,
            EventType::MediaError => EventType::Mount,
            EventType::Mount => EventType::Unmount,
            EventType::Unmount => EventType::Update,
            EventType::Update => EventType::Resize,
            EventType::Resize => EventType::Dismiss,
            EventType::Dismiss => EventType::TearOff,
            EventType::TearOff => EventType::Dock,
            EventType::Dock => EventType::WindowResize,
            EventType::WindowResize => EventType::WindowMove,
            EventType::WindowMove => EventType::WindowClose,
            EventType::WindowClose => EventType::WindowFrameChanged,
            EventType::WindowFrameChanged => EventType::WindowFocusIn,
            EventType::WindowFocusIn => EventType::WindowFocusOut,
            EventType::WindowFocusOut => EventType::ThemeChange,
            EventType::ThemeChange => EventType::WindowDpiChanged,
            EventType::WindowDpiChanged => EventType::WindowMonitorChanged,
            EventType::WindowMonitorChanged => EventType::MonitorConnected,
            EventType::MonitorConnected => EventType::MonitorDisconnected,
            EventType::MonitorDisconnected => EventType::FileHover,
            EventType::FileHover => EventType::FileDrop,
            EventType::FileDrop => EventType::FileHoverCancel,
            EventType::FileHoverCancel => EventType::SensorChanged,
            EventType::SensorChanged => EventType::GamepadInput,
            EventType::GamepadInput => EventType::GeolocationFix,
            EventType::GeolocationFix => EventType::GeolocationError,
            EventType::GeolocationError => EventType::PermissionChanged,
            EventType::PermissionChanged => EventType::BiometricResult,
            EventType::BiometricResult => EventType::ScreenColorPicked,
            EventType::ScreenColorPicked => EventType::KeyringResult,
            EventType::KeyringResult => EventType::DocumentEdit,
            EventType::DocumentEdit => EventType::DeviceConnected,
            EventType::DeviceConnected => EventType::DeviceDisconnected,
            EventType::DeviceDisconnected => EventType::PenSqueeze,
            EventType::PenSqueeze => EventType::PenDoubleTap,
            EventType::PenDoubleTap => EventType::PenHover,
            EventType::PenHover => EventType::DefaultAction,
            EventType::DefaultAction => EventType::Selected,
            EventType::Selected => EventType::HidReport,
            EventType::HidReport => EventType::ModifiersChanged,
            EventType::ModifiersChanged => EventType::RawMouseMotion,
            EventType::RawMouseMotion => EventType::DialRotate,
            EventType::DialRotate => EventType::DialClick,
            EventType::DialClick => EventType::MouseMove,
            EventType::MouseMove => EventType::MediaControl,
            EventType::MediaControl => return None,
        })
    }

    /// Every `EventType`, in declaration order.
    fn all_event_types() -> Vec<EventType> {
        let mut out = vec![EventType::MouseOver];
        while let Some(next) = next_event_type(*out.last().expect("seeded above")) {
            out.push(next);
            assert!(
                out.len() < 10_000,
                "next_event_type has a cycle - an arm points backwards"
            );
        }
        out
    }

    #[test]
    fn event_type_to_filters_never_panics_and_stays_synced_with_the_hover_matcher() {
        // ROUND-TRIP INVARIANT: a Hover filter emitted by `event_type_to_filters`
        // is later re-checked by `matches_filter_phase` inside `propagate_event`
        // (see shell2/common/event.rs). If the two tables disagree, the callback
        // is collected and then silently dropped — a dead filter.
        //
        // KNOWN_DESYNC records the pairs that are ALREADY broken today (reported
        // separately). The assertion is a *subset* check, so fixing one of them
        // keeps this test green while any NEW desync fails it.
        // Entries here are pairs that are ALREADY broken (reported
        // separately). The assertion is a *subset* check, so fixing one keeps
        // this test green while any NEW desync fails it.
        //
        // Six entries were stale when this list was audited on 2026-09-01 —
        // `MouseOut`, `FocusIn`, `FocusOut` and the three `Composition*`
        // types name arms that DO exist. A stale entry is worse than no
        // entry: it silences the ratchet for a pair that currently works, so
        // a later regression lands green. Delete an entry the moment its pair
        // syncs, and never add one without a linked report.
        const KNOWN_DESYNC: &[EventType] = &[];

        let mouse_data = EventData::Mouse(MouseEventData {
            position: LogicalPosition::new(1.0, 1.0),
            button: MouseButton::Left,
            buttons: 1,
            modifiers: KeyModifiers::default(),
            ..Default::default()
        });

        let cases: Vec<(EventType, EventData)> = vec![
            (EventType::MouseOver, EventData::None),
            (EventType::MouseEnter, EventData::None),
            (EventType::MouseLeave, EventData::None),
            (EventType::MouseOut, EventData::None),
            (EventType::MouseDown, mouse_data.clone()),
            (EventType::MouseUp, mouse_data.clone()),
            (EventType::Click, mouse_data.clone()),
            (EventType::DoubleClick, mouse_data.clone()),
            (EventType::ContextMenu, mouse_data.clone()),
            (EventType::KeyDown, EventData::None),
            (EventType::KeyUp, EventData::None),
            (EventType::KeyPress, EventData::None),
            (EventType::CompositionStart, EventData::None),
            (EventType::CompositionUpdate, EventData::None),
            (EventType::CompositionEnd, EventData::None),
            (EventType::Focus, EventData::None),
            (EventType::Blur, EventData::None),
            (EventType::FocusIn, EventData::None),
            (EventType::FocusOut, EventData::None),
            (EventType::Input, EventData::None),
            (EventType::Change, EventData::None),
            (EventType::Scroll, EventData::None),
            (EventType::ScrollStart, EventData::None),
            (EventType::ScrollEnd, EventData::None),
            (EventType::DragStart, EventData::None),
            (EventType::Drag, EventData::None),
            (EventType::DragEnd, EventData::None),
            (EventType::DragEnter, EventData::None),
            (EventType::DragOver, EventData::None),
            (EventType::DragLeave, EventData::None),
            (EventType::Drop, EventData::None),
            (EventType::TouchStart, EventData::None),
            (EventType::TouchMove, EventData::None),
            (EventType::TouchEnd, EventData::None),
            (EventType::TouchCancel, EventData::None),
            (EventType::Mount, EventData::None),
            (EventType::Unmount, EventData::None),
            (EventType::Update, EventData::None),
            (EventType::Resize, EventData::None),
            (EventType::Dismiss, EventData::None),
            (EventType::TearOff, EventData::None),
            (EventType::Dock, EventData::None),
            (EventType::WindowResize, EventData::None),
            (EventType::WindowMove, EventData::None),
            (EventType::WindowClose, EventData::None),
            (EventType::ThemeChange, EventData::None),
            (EventType::FileHover, EventData::None),
            (EventType::FileDrop, EventData::None),
            (EventType::FileHoverCancel, EventData::None),
            (EventType::Copy, EventData::None),
            (EventType::Cut, EventData::None),
            (EventType::Paste, EventData::None),
            (EventType::SensorChanged, EventData::None),
            (EventType::GamepadInput, EventData::None),
            (EventType::GeolocationFix, EventData::None),
            (EventType::GeolocationError, EventData::None),
            (EventType::PermissionChanged, EventData::None),
            (EventType::BiometricResult, EventData::None),
            (EventType::ScreenColorPicked, EventData::None),
            (EventType::KeyringResult, EventData::None),
            (EventType::LongPress, EventData::None),
            (EventType::Play, EventData::None),
            (EventType::PenDown, EventData::None),
            (EventType::PenMove, EventData::None),
            (EventType::PenUp, EventData::None),
            (EventType::PenEnter, EventData::None),
            (EventType::PenLeave, EventData::None),
            (EventType::DocumentEdit, EventData::None),
            // ── Added by 13f ──────────────────────────────────────────────
            // THIRTY-SIX of the 104 `EventType`s were absent from this list,
            // so the ratchet was green over 65% of the enum and silent about
            // the rest - including `MouseMove`, and including every type this
            // arc added. An allow-list that is empty proves nothing if the
            // table it guards is not complete; that is the same shape as the
            // `TIER1_SLOTS` table that let `cursor` overwrite `align-self`.
            // The exhaustiveness match below now makes an omission impossible.
            (EventType::MouseMove, mouse_data.clone()),
            (EventType::ModifiersChanged, EventData::None),
            (EventType::RawMouseMotion, EventData::None),
            (EventType::Submit, EventData::None),
            (EventType::Reset, EventData::None),
            (EventType::Invalid, EventData::None),
            (EventType::Selected, EventData::None),
            (EventType::DefaultAction, EventData::None),
            (EventType::SwipeLeft, EventData::None),
            (EventType::SwipeRight, EventData::None),
            (EventType::SwipeUp, EventData::None),
            (EventType::SwipeDown, EventData::None),
            (EventType::PinchIn, EventData::None),
            (EventType::PinchOut, EventData::None),
            (EventType::RotateClockwise, EventData::None),
            (EventType::RotateCounterClockwise, EventData::None),
            (EventType::Pause, EventData::None),
            (EventType::Ended, EventData::None),
            (EventType::TimeUpdate, EventData::None),
            (EventType::VolumeChange, EventData::None),
            (EventType::MediaError, EventData::None),
            (EventType::WindowFrameChanged, EventData::None),
            (EventType::WindowFocusIn, EventData::None),
            (EventType::WindowFocusOut, EventData::None),
            (EventType::WindowDpiChanged, EventData::None),
            (EventType::WindowMonitorChanged, EventData::None),
            (EventType::MonitorConnected, EventData::None),
            (EventType::MonitorDisconnected, EventData::None),
            (EventType::DeviceConnected, EventData::None),
            (EventType::DeviceDisconnected, EventData::None),
            (EventType::PenSqueeze, EventData::None),
            (EventType::PenDoubleTap, EventData::None),
            (EventType::PenHover, EventData::None),
            (EventType::HidReport, EventData::None),
            (EventType::DialRotate, EventData::None),
            (EventType::DialClick, EventData::None),
            (EventType::MediaControl, EventData::None),
        ];

        // COVERAGE PROOF. Not "the list looks complete" - `all_event_types`
        // walks the enum itself, so a variant that exists and is missing here
        // fails the test by name rather than being silently skipped.
        {
            let covered: BTreeSet<EventType> = cases.iter().map(|(ty, _)| *ty).collect();
            assert_eq!(
                covered.len(),
                cases.len(),
                "`cases` lists the same EventType twice"
            );
            let missing: Vec<EventType> = all_event_types()
                .into_iter()
                .filter(|ty| !covered.contains(ty))
                .collect();
            assert!(
                missing.is_empty(),
                "the ratchet does not cover {} of {} EventTypes, so a desync in \
                 them lands green: {missing:?}",
                missing.len(),
                all_event_types().len(),
            );
        }

        for (ty, data) in cases {
            let filters = event_type_to_filters(ty, &data);
            let ev = SyntheticEvent::new(ty, EventSource::User, dnid(0, 0), tick(0), data);

            // No duplicate filters — a duplicate would invoke the callback twice.
            let mut seen = BTreeSet::new();
            for f in &filters {
                assert!(seen.insert(*f), "{ty:?} emitted {f:?} twice");
            }

            // Every family, not just Hover. `propagate_event` re-checks each
            // planned filter through `matches_filter_phase` regardless of
            // which family it belongs to, so a Focus or Window desync drops
            // the callback exactly as silently as a Hover one — and both
            // existed: `matches_focus_filter` had no Pen arm at all, and
            // planning named only the Hover half of Touch.
            for f in &filters {
                if matches_filter_phase(*f, &ev, EventPhase::Target) {
                    continue;
                }
                assert!(
                    KNOWN_DESYNC.contains(&ty),
                    "NEW DESYNC: event_type_to_filters({ty:?}) emits {f:?}, but \
                     matches_filter_phase rejects it at the Target phase, so the \
                     callback would be collected and then silently dropped"
                );
            }
        }
    }

    #[test]
    fn event_type_to_filters_omits_button_specific_filter_for_exotic_buttons() {
        // MouseButton::Other(n) has no dedicated filter: only the generic one.
        let data = EventData::Mouse(MouseEventData {
            position: LogicalPosition::zero(),
            button: MouseButton::Other(u8::MAX),
            buttons: 0,
            modifiers: KeyModifiers::default(),
            ..Default::default()
        });
        let down = event_type_to_filters(EventType::MouseDown, &data);
        assert_eq!(down, vec![
                // Planning is scope-complete now (it is derived from the
                // matcher), so the generic MouseDown appears in all three
                // scopes. The POINT of this test is unchanged: no
                // button-SPECIFIC filter for an exotic button.
                EventFilter::Hover(HoverEventFilter::MouseDown),
                EventFilter::Focus(FocusEventFilter::MouseDown),
                EventFilter::Window(WindowEventFilter::MouseDown),
            ]);
        let up = event_type_to_filters(EventType::MouseUp, &data);
        assert_eq!(
            up,
            vec![
                EventFilter::Hover(HoverEventFilter::MouseUp),
                EventFilter::Focus(FocusEventFilter::MouseUp),
                EventFilter::Window(WindowEventFilter::MouseUp),
            ]
        );

        // Unmapped event types produce an empty filter list (never a panic).
        //
        // Submit / Change / Reset / Invalid left this list when they gained
        // filters. The media six are still here: they have no playback state
        // machine to fire from, so giving them filters would advertise events
        // that cannot happen.
        for ty in [
            EventType::Play,
            EventType::Pause,
            EventType::Ended,
            EventType::TimeUpdate,
            EventType::VolumeChange,
            EventType::MediaError,
        ] {
            assert!(
                event_type_to_filters(ty, &EventData::None).is_empty(),
                "{ty:?} is unmapped and must yield no filters"
            );
        }
        // Gestures ARE mapped (they were not, which is why a Pinch callback
        // never fired) — see every_gesture_event_matches_its_same_named_filter.
        //
        // The pen block and `DocumentEdit` are here for the same reason, found
        // 2026-09-01: they had a producer and a matcher arm but NO planning
        // arm, so they fell through `_ => vec![]` and planned an empty filter
        // list. An empty list is the silent failure this loop exists to catch
        // — nothing is looked up, nothing is rejected, nothing is logged.
        // Touch and the scroll phases were the same shape, one family deep:
        // planning named only the Hover half.
        for ty in [
            EventType::PinchIn,
            EventType::RotateClockwise,
            EventType::SwipeLeft,
            EventType::Submit,
            EventType::Change,
            EventType::Reset,
            EventType::Invalid,
            EventType::PenDown,
            EventType::PenMove,
            EventType::PenUp,
            EventType::PenEnter,
            EventType::PenLeave,
            EventType::DocumentEdit,
            EventType::ScrollStart,
            EventType::ScrollEnd,
            EventType::TouchStart,
            EventType::TouchMove,
            EventType::TouchEnd,
            EventType::TouchCancel,
        ] {
            assert!(
                !event_type_to_filters(ty, &EventData::None).is_empty(),
                "{ty:?} must map to its gesture filters"
            );
        }
    }

    // ================================================== DOM path / propagation

    #[test]
    fn get_dom_path_none_target_yields_empty_path() {
        let hier = hierarchy_chain(3);
        assert!(get_dom_path(&hier, NodeHierarchyItemId::NONE).is_empty());
        // An empty hierarchy with a real target must not index out of bounds.
        let empty = NodeHierarchy::new(Vec::new());
        let path = get_dom_path(
            &empty,
            NodeHierarchyItemId::from_crate_internal(Some(NodeId::ZERO)),
        );
        assert_eq!(
            path,
            vec![NodeId::ZERO],
            "unknown nodes still path to themselves"
        );
    }

    #[test]
    fn get_dom_path_out_of_range_target_does_not_panic() {
        let hier = hierarchy_chain(3);
        let huge = NodeHierarchyItemId::from_crate_internal(Some(NodeId::new(usize::MAX - 1)));
        let path = get_dom_path(&hier, huge);
        assert_eq!(path, vec![NodeId::new(usize::MAX - 1)]);
    }

    #[test]
    fn get_dom_path_returns_root_to_target_order() {
        let hier = hierarchy_chain(4); // 0 <- 1 <- 2 <- 3
        let path = get_dom_path(
            &hier,
            NodeHierarchyItemId::from_crate_internal(Some(NodeId::new(3))),
        );
        assert_eq!(
            path,
            vec![
                NodeId::new(0),
                NodeId::new(1),
                NodeId::new(2),
                NodeId::new(3)
            ],
            "path must run root -> target"
        );
        // The root itself paths to a single-element vec.
        let root_path = get_dom_path(
            &hier,
            NodeHierarchyItemId::from_crate_internal(Some(NodeId::ZERO)),
        );
        assert_eq!(root_path, vec![NodeId::ZERO]);
    }

    #[test]
    fn get_dom_path_terminates_on_a_self_parent_cycle() {
        // A node that is its own parent must not spin forever.
        let hier = NodeHierarchy::new(vec![Node {
            parent: Some(NodeId::ZERO),
            ..Node::ROOT
        }]);
        let path = get_dom_path(
            &hier,
            NodeHierarchyItemId::from_crate_internal(Some(NodeId::ZERO)),
        );
        assert_eq!(path, vec![NodeId::ZERO]);
    }

    #[test]
    fn get_dom_path_handles_a_deep_chain_without_recursing() {
        // 5000 levels deep: an iterative walk copes, a recursive one would blow
        // the stack.
        let hier = hierarchy_chain(5000);
        let path = get_dom_path(
            &hier,
            NodeHierarchyItemId::from_crate_internal(Some(NodeId::new(4999))),
        );
        assert_eq!(path.len(), 5000);
        assert_eq!(path[0], NodeId::ZERO);
        assert_eq!(path[4999], NodeId::new(4999));
    }

    #[test]
    fn propagate_event_visits_each_node_exactly_once() {
        // Regression guard for the double-fire bug: with capture + bubble both
        // walking the ancestors, a node's callback used to be collected TWICE.
        let hier = hierarchy_chain(3); // 0 <- 1 <- 2
        let mut callbacks: BTreeMap<NodeId, Vec<EventFilter>> = BTreeMap::new();
        for i in 0..3 {
            callbacks.insert(
                NodeId::new(i),
                vec![EventFilter::Hover(HoverEventFilter::MouseDown)],
            );
        }
        let mut ev = SyntheticEvent::new(
            EventType::MouseDown,
            EventSource::User,
            dnid(0, 2),
            tick(0),
            EventData::Mouse(MouseEventData {
                position: LogicalPosition::zero(),
                button: MouseButton::Left,
                buttons: 1,
                modifiers: KeyModifiers::default(),
                ..Default::default()
            }),
        );

        let result = propagate_event(&mut ev, &hier, &callbacks);
        let nodes: Vec<NodeId> = result.callbacks_to_invoke.iter().map(|(n, _)| *n).collect();
        assert_eq!(
            nodes,
            vec![NodeId::new(2), NodeId::new(1), NodeId::new(0)],
            "target first, then bubbling up to the root — each node once"
        );
        assert!(!result.default_prevented);
    }

    /// Bug class: a child's enter/leave reaching its parent. W3C `mouseleave`
    /// does not bubble, and every node that lost hover already gets its own
    /// event, so a parent that handles `MouseLeave` must hear ONLY about the
    /// pointer leaving the parent — not a child inside it. (The slider, the
    /// map and the split pane all ended their drags on a bubbled child leave.)
    #[test]
    fn enter_and_leave_events_stop_at_their_target() {
        let hier = hierarchy_chain(3); // 0 <- 1 <- 2
        for ty in [
            EventType::MouseEnter,
            EventType::MouseLeave,
            EventType::PenEnter,
            EventType::PenLeave,
        ] {
            let filter = || {
                EventFilter::Hover(match ty {
                    EventType::MouseEnter => HoverEventFilter::MouseEnter,
                    EventType::MouseLeave => HoverEventFilter::MouseLeave,
                    EventType::PenEnter => HoverEventFilter::PenEnter,
                    _ => HoverEventFilter::PenLeave,
                })
            };
            let mut callbacks: BTreeMap<NodeId, Vec<EventFilter>> = BTreeMap::new();
            for i in 0..3 {
                callbacks.insert(NodeId::new(i), vec![filter()]);
            }
            let mut ev =
                SyntheticEvent::new(ty, EventSource::User, dnid(0, 2), tick(0), EventData::None);
            let result = propagate_event(&mut ev, &hier, &callbacks);
            let nodes: Vec<NodeId> = result.callbacks_to_invoke.iter().map(|(n, _)| *n).collect();
            assert_eq!(
                nodes,
                vec![NodeId::new(2)],
                "{ty:?} reached ancestors — it must stop at its target"
            );
            assert!(!ty.bubbles());
        }
        // The rule is narrow: a move still bubbles to every ancestor.
        assert!(EventType::MouseOver.bubbles());
        assert!(EventType::DragLeave.bubbles(), "W3C dragleave bubbles");
    }

    /// REPORTED (AzMap pinch, 2026-08-21): the gesture detectors produced
    /// `PinchIn`/`PinchOut`, the widget registered `Hover(PinchOut)`, and the
    /// callback never ran — the filter truth tables had no arm for any gesture.
    /// Every gesture event type must match its same-named filter in all three
    /// tables; a new gesture added to the enums without its arm fails here.
    #[test]
    fn every_gesture_event_matches_its_same_named_filter() {
        let gestures: [(
            EventType,
            HoverEventFilter,
            FocusEventFilter,
            WindowEventFilter,
        ); 9] = [
            (
                EventType::LongPress,
                HoverEventFilter::LongPress,
                FocusEventFilter::LongPress,
                WindowEventFilter::LongPress,
            ),
            (
                EventType::SwipeLeft,
                HoverEventFilter::SwipeLeft,
                FocusEventFilter::SwipeLeft,
                WindowEventFilter::SwipeLeft,
            ),
            (
                EventType::SwipeRight,
                HoverEventFilter::SwipeRight,
                FocusEventFilter::SwipeRight,
                WindowEventFilter::SwipeRight,
            ),
            (
                EventType::SwipeUp,
                HoverEventFilter::SwipeUp,
                FocusEventFilter::SwipeUp,
                WindowEventFilter::SwipeUp,
            ),
            (
                EventType::SwipeDown,
                HoverEventFilter::SwipeDown,
                FocusEventFilter::SwipeDown,
                WindowEventFilter::SwipeDown,
            ),
            (
                EventType::PinchIn,
                HoverEventFilter::PinchIn,
                FocusEventFilter::PinchIn,
                WindowEventFilter::PinchIn,
            ),
            (
                EventType::PinchOut,
                HoverEventFilter::PinchOut,
                FocusEventFilter::PinchOut,
                WindowEventFilter::PinchOut,
            ),
            (
                EventType::RotateClockwise,
                HoverEventFilter::RotateClockwise,
                FocusEventFilter::RotateClockwise,
                WindowEventFilter::RotateClockwise,
            ),
            (
                EventType::RotateCounterClockwise,
                HoverEventFilter::RotateCounterClockwise,
                FocusEventFilter::RotateCounterClockwise,
                WindowEventFilter::RotateCounterClockwise,
            ),
        ];
        for (ty, hover, focus, window) in gestures {
            let ev =
                SyntheticEvent::new(ty, EventSource::User, dnid(0, 0), tick(0), EventData::None);
            assert!(
                matches_hover_filter(hover, &ev, EventPhase::Target),
                "Hover({hover:?}) must match {ty:?}"
            );
            assert!(
                matches_focus_filter(focus, &ev, EventPhase::Target),
                "Focus({focus:?}) must match {ty:?}"
            );
            assert!(
                matches_window_filter(window, &ev, EventPhase::Target),
                "Window({window:?}) must match {ty:?}"
            );
            // ...and the table is a truth table, not a wildcard.
            let other = SyntheticEvent::new(
                EventType::MouseDown,
                EventSource::User,
                dnid(0, 0),
                tick(0),
                EventData::None,
            );
            assert!(!matches_hover_filter(hover, &other, EventPhase::Target));
            // The dispatcher asks THIS table which filters to try for an event
            // type; it sent every gesture to `vec![]`.
            let filters = event_type_to_filters(ty, &EventData::None);
            assert!(
                filters.contains(&EventFilter::Hover(hover)),
                "{ty:?} must dispatch to Hover({hover:?})"
            );
            assert!(
                filters.contains(&EventFilter::Focus(focus)),
                "{ty:?} must dispatch to Focus({focus:?})"
            );
            assert!(
                filters.contains(&EventFilter::Window(window)),
                "{ty:?} must dispatch to Window({window:?})"
            );
        }
    }

    #[test]
    fn an_at_target_only_event_reaches_its_target_and_no_ancestor() {
        // The captured release for a pressed node: its ancestors already saw
        // the real release through the hovered node's path.
        let hier = hierarchy_chain(3); // 0 <- 1 <- 2
        let mut callbacks: BTreeMap<NodeId, Vec<EventFilter>> = BTreeMap::new();
        for i in 0..3 {
            callbacks.insert(
                NodeId::new(i),
                vec![EventFilter::Hover(HoverEventFilter::MouseUp)],
            );
        }
        let mut ev = SyntheticEvent::new(
            EventType::MouseUp,
            EventSource::User,
            dnid(0, 2),
            tick(0),
            EventData::None,
        )
        .at_target_only();
        let result = propagate_event(&mut ev, &hier, &callbacks);
        let nodes: Vec<NodeId> = result.callbacks_to_invoke.iter().map(|(n, _)| *n).collect();
        assert_eq!(
            nodes,
            vec![NodeId::new(2)],
            "at-target-only must skip capture and bubble"
        );

        // The plain event still walks the whole path.
        let mut ev = SyntheticEvent::new(
            EventType::MouseUp,
            EventSource::User,
            dnid(0, 2),
            tick(0),
            EventData::None,
        );
        let result = propagate_event(&mut ev, &hier, &callbacks);
        // Hover filters fire in the target and bubble phases only (never capture).
        assert_eq!(
            result.callbacks_to_invoke.len(),
            3,
            "target 2 + bubble 1, 0"
        );
    }

    #[test]
    fn propagate_event_on_a_dangling_target_is_a_no_op() {
        let hier = hierarchy_chain(2);
        let callbacks: BTreeMap<NodeId, Vec<EventFilter>> = BTreeMap::new();

        // Target = the `None` sentinel: the doc comment claims a panic, but the
        // implementation returns a default result. Pin the safe behavior.
        let mut ev = SyntheticEvent::new(
            EventType::MouseDown,
            EventSource::User,
            dnid_none(0),
            tick(0),
            EventData::None,
        );
        let result = propagate_event(&mut ev, &hier, &callbacks);
        assert!(result.callbacks_to_invoke.is_empty());
        assert!(!result.default_prevented);

        // Target = a node id far outside the hierarchy: also a no-op, no panic.
        let mut ev = SyntheticEvent::new(
            EventType::MouseDown,
            EventSource::User,
            dnid(0, 10_000),
            tick(0),
            EventData::None,
        );
        let result = propagate_event(&mut ev, &hier, &callbacks);
        assert!(result.callbacks_to_invoke.is_empty());
    }

    #[test]
    fn propagate_event_respects_a_pre_stopped_event() {
        let hier = hierarchy_chain(3);
        let mut callbacks: BTreeMap<NodeId, Vec<EventFilter>> = BTreeMap::new();
        for i in 0..3 {
            callbacks.insert(
                NodeId::new(i),
                vec![EventFilter::Hover(HoverEventFilter::MouseOver)],
            );
        }
        let base = SyntheticEvent::new(
            EventType::MouseOver,
            EventSource::User,
            dnid(0, 2),
            tick(0),
            EventData::None,
        );

        // stopped => neither target nor bubble collect anything.
        let mut stopped = base.clone();
        stopped.stop_propagation();
        let r = propagate_event(&mut stopped, &hier, &callbacks);
        assert!(
            r.callbacks_to_invoke.is_empty(),
            "a stopped event collects nothing"
        );

        // stopped_immediate => likewise (and it implies `stopped`).
        let mut immediate = base.clone();
        immediate.stop_immediate_propagation();
        let r = propagate_event(&mut immediate, &hier, &callbacks);
        assert!(r.callbacks_to_invoke.is_empty());

        // prevented_default is faithfully reported back out.
        let mut prevented = base;
        prevented.prevent_default();
        let r = propagate_event(&mut prevented, &hier, &callbacks);
        assert!(r.default_prevented);
        assert_eq!(
            r.callbacks_to_invoke.len(),
            3,
            "preventDefault must not stop dispatch"
        );
    }

    #[test]
    fn propagate_event_ignores_filters_that_do_not_match_the_event() {
        let hier = hierarchy_chain(2);
        let mut callbacks: BTreeMap<NodeId, Vec<EventFilter>> = BTreeMap::new();
        callbacks.insert(
            NodeId::new(1),
            vec![
                EventFilter::Hover(HoverEventFilter::MouseUp), // wrong event type
                EventFilter::Hover(HoverEventFilter::RightMouseDown), // wrong button
                EventFilter::Hover(HoverEventFilter::LeftMouseDown), // match
            ],
        );
        let mut ev = mouse_event(
            EventType::MouseDown,
            MouseButton::Left,
            LogicalPosition::zero(),
        );
        ev.target = dnid(0, 1);
        ev.current_target = ev.target;

        let r = propagate_event(&mut ev, &hier, &callbacks);
        assert_eq!(
            r.callbacks_to_invoke,
            vec![(
                NodeId::new(1),
                EventFilter::Hover(HoverEventFilter::LeftMouseDown)
            )]
        );
        // The event is left in the state of the LAST walked phase: bubble, ending
        // on the root ancestor. (`current_target` is only meaningful while a
        // callback is running, so this pins the post-walk residue rather than
        // asserting it is reset.)
        assert_eq!(ev.phase, EventPhase::Bubble);
        assert_eq!(ev.current_target, dnid(0, 0));
        assert_eq!(
            ev.target,
            dnid(0, 1),
            "the target itself must never be rewritten"
        );
    }

    #[test]
    fn collect_matching_callbacks_collects_nothing_once_immediate_stop_is_set() {
        let mut result = PropagationResult::default();
        let mut callbacks: BTreeMap<NodeId, Vec<EventFilter>> = BTreeMap::new();
        callbacks.insert(
            NodeId::ZERO,
            vec![EventFilter::Hover(HoverEventFilter::MouseOver)],
        );
        let mut ev = SyntheticEvent::new(
            EventType::MouseOver,
            EventSource::User,
            dnid(0, 0),
            tick(0),
            EventData::None,
        );
        ev.stop_immediate_propagation();
        collect_matching_callbacks(
            &ev,
            NodeId::ZERO,
            EventPhase::Target,
            &callbacks,
            &mut result,
        );
        assert!(result.callbacks_to_invoke.is_empty());

        // A node with no registered callbacks is simply skipped.
        let mut fresh = PropagationResult::default();
        let clean = SyntheticEvent::new(
            EventType::MouseOver,
            EventSource::User,
            dnid(0, 0),
            tick(0),
            EventData::None,
        );
        collect_matching_callbacks(
            &clean,
            NodeId::new(9),
            EventPhase::Target,
            &callbacks,
            &mut fresh,
        );
        assert!(fresh.callbacks_to_invoke.is_empty());
    }

    #[test]
    fn propagate_phase_over_an_empty_iterator_only_sets_the_phase() {
        let mut result = PropagationResult::default();
        let callbacks: BTreeMap<NodeId, Vec<EventFilter>> = BTreeMap::new();
        let mut ev = SyntheticEvent::new(
            EventType::MouseOver,
            EventSource::User,
            dnid(0, 0),
            tick(0),
            EventData::None,
        );
        propagate_phase(
            &mut ev,
            core::iter::empty(),
            EventPhase::Bubble,
            &callbacks,
            &mut result,
        );
        assert_eq!(ev.phase, EventPhase::Bubble);
        assert!(result.callbacks_to_invoke.is_empty());

        // propagate_target_phase resets phase + current_target to the target.
        propagate_target_phase(&mut ev, NodeId::ZERO, &callbacks, &mut result);
        assert_eq!(ev.phase, EventPhase::Target);
        assert_eq!(ev.current_target, ev.target);
    }

    // ================================================================== dedup

    #[test]
    fn deduplicate_synthetic_events_handles_empty_and_single() {
        assert!(deduplicate_synthetic_events(Vec::new()).is_empty());
        let one = vec![SyntheticEvent::new(
            EventType::Scroll,
            EventSource::User,
            dnid(0, 0),
            tick(1),
            EventData::None,
        )];
        assert_eq!(deduplicate_synthetic_events(one).len(), 1);
    }

    #[test]
    fn deduplicate_synthetic_events_keeps_the_latest_timestamp_per_target_and_type() {
        let mk = |node: usize, ty: EventType, t: u64| {
            SyntheticEvent::new(
                ty,
                EventSource::User,
                dnid(0, node),
                tick(t),
                EventData::None,
            )
        };
        // Same (target, type), out-of-order timestamps -> keep the newest.
        let events = vec![
            mk(1, EventType::Scroll, 5),
            mk(1, EventType::Scroll, 99),
            mk(1, EventType::Scroll, 1),
        ];
        let out = deduplicate_synthetic_events(events);
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].timestamp, tick(99), "the newest event must survive");

        // Different node OR different type -> both survive.
        let events = vec![
            mk(1, EventType::Scroll, 1),
            mk(2, EventType::Scroll, 1),
            mk(1, EventType::MouseOver, 1),
        ];
        assert_eq!(deduplicate_synthetic_events(events).len(), 3);

        // Different DOM with the same node index -> distinct targets.
        let a = SyntheticEvent::new(
            EventType::Scroll,
            EventSource::User,
            dnid(0, 1),
            tick(0),
            EventData::None,
        );
        let b = SyntheticEvent::new(
            EventType::Scroll,
            EventSource::User,
            dnid(1, 1),
            tick(0),
            EventData::None,
        );
        assert_eq!(deduplicate_synthetic_events(vec![a, b]).len(), 2);
    }

    #[test]
    fn deduplicate_synthetic_events_collapses_a_large_duplicate_burst() {
        // 10k identical events (e.g. a scroll storm) must collapse to one, and
        // the result must be the newest — no quadratic blowup, no overflow.
        let events: Vec<SyntheticEvent> = (0..10_000u64)
            .map(|t| {
                SyntheticEvent::new(
                    EventType::Scroll,
                    EventSource::User,
                    dnid(0, 0),
                    tick(t),
                    EventData::None,
                )
            })
            .collect();
        let out = deduplicate_synthetic_events(events);
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].timestamp, tick(9_999));
    }

    #[test]
    fn deduplicate_synthetic_events_preserves_unicode_payloads() {
        // Deduplication keys off (target, event_type) only — the payload must
        // survive untouched, including multi-byte / combining / RTL text.
        let text = "🦀 グラフ é\u{0301} مرحبا \u{1F1E6}\u{1F1F9}".repeat(200);
        let ev = SyntheticEvent::new(
            EventType::Input,
            EventSource::User,
            dnid(0, 0),
            tick(1),
            EventData::TextInput(TextInputEventData {
                inserted_text: text.clone(),
                old_text: String::new(),
            }),
        );
        let newer = SyntheticEvent::new(
            EventType::Input,
            EventSource::User,
            dnid(0, 0),
            tick(2),
            EventData::TextInput(TextInputEventData {
                inserted_text: text.clone(),
                old_text: text.clone(),
            }),
        );
        let out = deduplicate_synthetic_events(vec![ev, newer]);
        assert_eq!(out.len(), 1);
        match &out[0].data {
            EventData::TextInput(d) => {
                assert_eq!(d.inserted_text, text);
                assert_eq!(d.old_text, text, "the newer event won");
            }
            _ => panic!("payload must be preserved"),
        }
    }

    // ====================================================== hit-test extraction

    #[test]
    fn get_first_hovered_node_on_empty_input() {
        assert!(get_first_hovered_node(None).is_none());
        assert!(
            get_first_hovered_node(Some(&empty_hit_test())).is_none(),
            "a hit test with no hovered DOMs has no front-most node"
        );
        // A DOM entry that is present but has zero hit nodes is also `None`.
        let ht = hit_test_with(0, &[]);
        assert!(get_first_hovered_node(Some(&ht)).is_none());
    }

    #[test]
    fn get_first_hovered_node_picks_minimum_depth_and_breaks_ties_deterministically() {
        // Front-most (depth 0) has the HIGHER node id — a naive `.next()` on the
        // BTreeMap would wrongly return node 2.
        let ht = hit_test_with(0, &[(2, 5), (5, 0), (9, 3)]);
        let got = get_first_hovered_node(Some(&ht)).unwrap();
        assert_eq!(got.node.into_crate_internal(), Some(NodeId::new(5)));

        // Equal depths: the first in (DomId, NodeId) iteration order wins, and the
        // choice must be stable across calls.
        let ht = hit_test_with(0, &[(7, 2), (3, 2), (11, 2)]);
        let a = get_first_hovered_node(Some(&ht)).unwrap();
        let b = get_first_hovered_node(Some(&ht)).unwrap();
        assert_eq!(a, b, "tie-breaking must be deterministic");
        assert_eq!(a.node.into_crate_internal(), Some(NodeId::new(3)));

        // u32::MAX depth is still a valid (and only) candidate.
        let ht = hit_test_with(0, &[(1, u32::MAX)]);
        let got = get_first_hovered_node(Some(&ht)).unwrap();
        assert_eq!(got.node.into_crate_internal(), Some(NodeId::new(1)));
        assert_eq!(got.dom, DomId { inner: 0 });
    }

    #[test]
    fn get_mouse_position_with_fallback_prefers_the_event_payload() {
        let mouse = MouseState {
            cursor_position: CursorPosition::InWindow(LogicalPosition::new(9.0, 9.0)),
            ..MouseState::default()
        };
        let ev = mouse_event(
            EventType::MouseDown,
            MouseButton::Left,
            LogicalPosition::new(1.0, 2.0),
        );
        assert_eq!(
            get_mouse_position_with_fallback(&ev, &mouse),
            LogicalPosition::new(1.0, 2.0),
            "the event's own payload wins over the live cursor"
        );

        // Non-mouse payload -> fall back to the live cursor...
        let keyless = SyntheticEvent::new(
            EventType::MouseDown,
            EventSource::Synthetic,
            dnid(0, 0),
            tick(0),
            EventData::None,
        );
        assert_eq!(
            get_mouse_position_with_fallback(&keyless, &mouse),
            LogicalPosition::new(9.0, 9.0)
        );

        // ...and if the cursor is Uninitialized or OutOfWindow, fall back to zero
        // (`CursorPosition::get_position` only yields InWindow positions).
        for cursor in [
            CursorPosition::Uninitialized,
            CursorPosition::OutOfWindow(LogicalPosition::new(-5.0, -5.0)),
        ] {
            let ms = MouseState {
                cursor_position: cursor,
                ..MouseState::default()
            };
            assert_eq!(
                get_mouse_position_with_fallback(&keyless, &ms),
                LogicalPosition::zero()
            );
        }
    }

    #[test]
    fn get_mouse_position_with_fallback_passes_through_extreme_coordinates() {
        let mouse = MouseState::default();
        for pos in [
            LogicalPosition::new(f32::NAN, f32::NAN),
            LogicalPosition::new(f32::INFINITY, f32::NEG_INFINITY),
            LogicalPosition::new(f32::MAX, f32::MIN),
            LogicalPosition::new(-0.0, 0.0),
        ] {
            let ev = mouse_event(EventType::MouseDown, MouseButton::Left, pos);
            let got = get_mouse_position_with_fallback(&ev, &mouse);
            // Compare bitwise so NaN == NaN holds: the value must be forwarded
            // verbatim, never sanitized or panicked on.
            assert_eq!(got.x.to_bits(), pos.x.to_bits());
            assert_eq!(got.y.to_bits(), pos.y.to_bits());
        }
    }

    // ============================================= input-interpreter handlers

    #[test]
    fn handle_mouse_down_treats_zero_click_count_as_one() {
        let ht = hit_test_with(0, &[(0, 0)]);
        let mouse = MouseState::default();
        let kb = KeyboardState::default();
        let ev = mouse_event(
            EventType::MouseDown,
            MouseButton::Left,
            LogicalPosition::new(4.0, 5.0),
        );

        // click_count 0 is normalised to 1 -> a plain text-selection click.
        let action = handle_mouse_down(&ev, Some(&ht), 0, &mouse, &kb)
            .expect("click_count 0 must be treated as a single click");
        match action {
            InternalEventAction::AddAndPass(SystemChange::TextSelectionClick {
                position, ..
            }) => {
                assert_eq!(position, LogicalPosition::new(4.0, 5.0));
            }
            _ => panic!("expected a passed-through TextSelectionClick"),
        }
    }

    #[test]
    fn handle_mouse_down_saturates_above_a_triple_click() {
        let ht = hit_test_with(0, &[(0, 0)]);
        let mouse = MouseState::default();
        let kb = KeyboardState::default();
        let ev = mouse_event(
            EventType::MouseDown,
            MouseButton::Left,
            LogicalPosition::zero(),
        );

        // 1..=3 are real clicks.
        for count in 1u8..=3 {
            assert!(
                handle_mouse_down(&ev, Some(&ht), count, &mouse, &kb).is_some(),
                "click_count {count} must produce a selection click"
            );
        }
        // 4 and above (up to the u8 boundary) are dropped — no wraparound, no panic.
        for count in [4u8, 5, 100, u8::MAX] {
            assert!(
                handle_mouse_down(&ev, Some(&ht), count, &mouse, &kb).is_none(),
                "click_count {count} must be ignored"
            );
        }
    }

    #[test]
    fn handle_mouse_down_without_a_hit_test_is_a_no_op() {
        let mouse = MouseState::default();
        let kb = KeyboardState::default();
        let ev = mouse_event(
            EventType::MouseDown,
            MouseButton::Left,
            LogicalPosition::zero(),
        );
        assert!(handle_mouse_down(&ev, None, 1, &mouse, &kb).is_none());
        assert!(handle_mouse_down(&ev, Some(&empty_hit_test()), 1, &mouse, &kb).is_none());
    }

    #[test]
    fn handle_mouse_down_with_primary_held_adds_a_cursor_only_on_a_single_click() {
        let ht = hit_test_with(0, &[(0, 0)]);
        let mouse = MouseState::default();
        let kb = keyboard_with_primary_held();
        let ev = mouse_event(
            EventType::MouseDown,
            MouseButton::Left,
            LogicalPosition::new(7.0, 8.0),
        );

        // primary + single click -> multi-cursor add.
        match handle_mouse_down(&ev, Some(&ht), 1, &mouse, &kb) {
            Some(InternalEventAction::AddAndPass(SystemChange::AddCursorAtClick { position })) => {
                assert_eq!(position, LogicalPosition::new(7.0, 8.0));
            }
            _ => panic!("primary+click must add a cursor at the click position"),
        }
        // primary + double click -> NOT a cursor add (falls back to selection).
        match handle_mouse_down(&ev, Some(&ht), 2, &mouse, &kb) {
            Some(InternalEventAction::AddAndPass(SystemChange::TextSelectionClick { .. })) => {}
            _ => panic!("primary+double-click must not add a cursor"),
        }
    }

    /// Releasing the button must stop the autoscroll timer. Nothing emitted
    /// `StopAutoScrollTimer` at all, so a lost release left a 60Hz timer
    /// running for the life of the window.
    #[test]
    fn releasing_the_button_stops_the_autoscroll_timer() {
        match handle_mouse_up() {
            InternalEventAction::AddAndPass(SystemChange::StopAutoScrollTimer) => {}
            _ => panic!("expected AddAndPass(StopAutoScrollTimer)"),
        }
    }

    #[test]
    fn handle_mouse_over_requires_a_held_button_and_a_drag_origin() {
        let ht = hit_test_with(0, &[(0, 0)]);
        let start = LogicalPosition::new(1.0, 1.0);
        let ev = mouse_event(
            EventType::MouseOver,
            MouseButton::Left,
            LogicalPosition::new(50.0, 60.0),
        );

        // Button up -> never a drag, even with a drag origin.
        let up = MouseState::default();
        assert!(handle_mouse_move(&ev, Some(&ht), &up, Some(start)).is_none());

        // Button down but no drag origin -> not a drag either.
        let down = MouseState {
            left_down: true,
            ..MouseState::default()
        };
        assert!(handle_mouse_move(&ev, Some(&ht), &down, None).is_none());

        // Button down + origin but nothing under the cursor -> STILL a drag:
        // reaching past the text (into padding, past the last line) is how a
        // selection gets extended, and the endpoint resolves against the
        // anchor block, not against whatever is under the pointer.
        assert!(handle_mouse_move(&ev, None, &down, Some(start)).is_some());
        assert!(handle_mouse_move(&ev, Some(&empty_hit_test()), &down, Some(start)).is_some());

        // All three present -> a drag selection from origin to the current point.
        match handle_mouse_move(&ev, Some(&ht), &down, Some(start)) {
            Some(InternalEventAction::AddAndPass(SystemChange::TextSelectionDrag {
                start_position,
                current_position,
            })) => {
                assert_eq!(start_position, start);
                assert_eq!(current_position, LogicalPosition::new(50.0, 60.0));
            }
            _ => panic!("expected a TextSelectionDrag"),
        }
    }

    #[test]
    fn handle_key_down_needs_a_focused_node_and_a_keyboard_payload() {
        let kb = KeyboardState::default();
        let ev = key_event(VirtualKeyCode::Back as u32, KeyModifiers::default());
        assert!(
            handle_key_down(&ev, &kb, None, true).is_none(),
            "no focus => no keyboard system change"
        );

        // Focused, but the event carries no keyboard payload.
        let payloadless = SyntheticEvent::new(
            EventType::KeyDown,
            EventSource::User,
            dnid(0, 1),
            tick(0),
            EventData::None,
        );
        assert!(handle_key_down(&payloadless, &kb, Some(dnid(0, 1)), true).is_none());
    }

    #[test]
    fn handle_key_down_rejects_undecodable_key_codes() {
        let kb = KeyboardState::default();
        let target = Some(dnid(0, 1));
        // u32::MAX / out-of-table codes must fall out via `from_u32` -> None,
        // never index a table or panic.
        for code in [u32::MAX, u32::MAX - 1, 100_000, 9_999] {
            let ev = key_event(code, KeyModifiers::default());
            assert!(
                handle_key_down(&ev, &kb, target, true).is_none(),
                "key_code {code} must decode to None"
            );
        }
    }

    #[test]
    fn handle_key_down_reads_modifiers_from_the_event_not_the_live_keyboard() {
        // The live KeyboardState is deliberately EMPTY here: the handler must key
        // off the event payload's modifiers (the live state may have advanced
        // between queueing and dispatch).
        let kb = KeyboardState::default();
        let target = dnid(0, 1);
        let ev = key_event(VirtualKeyCode::C as u32, primary_modifiers());
        match handle_key_down(&ev, &kb, Some(target), true) {
            Some(InternalEventAction::AddAndSkip(SystemChange::CopyToClipboard)) => {}
            _ => panic!("primary+C in the payload must copy, regardless of the live state"),
        }

        // ...and conversely, a live primary key must NOT rewrite an unmodified event.
        let live = keyboard_with_primary_held();
        let plain = key_event(VirtualKeyCode::C as u32, KeyModifiers::default());
        assert!(
            handle_key_down(&plain, &live, Some(target), true).is_none(),
            "an unmodified C is plain text input, not a copy"
        );
    }

    #[test]
    fn handle_key_down_maps_backspace_and_delete_to_selection_ops() {
        let kb = KeyboardState::default();
        let target = dnid(0, 1);

        let expect_op = |ev: &SyntheticEvent| -> SelectionOp {
            match handle_key_down(ev, &kb, Some(target), true) {
                Some(InternalEventAction::AddAndSkip(SystemChange::ApplySelectionOp {
                    target: t,
                    op,
                })) => {
                    assert_eq!(t, target);
                    op
                }
                _ => panic!("expected an ApplySelectionOp"),
            }
        };

        let back = expect_op(&key_event(
            VirtualKeyCode::Back as u32,
            KeyModifiers::default(),
        ));
        assert_eq!(back.direction, SelectionDirection::Backward);
        assert_eq!(back.step, SelectionStep::Character);
        assert_eq!(back.mode, SelectionMode::Delete);

        let del = expect_op(&key_event(
            VirtualKeyCode::Delete as u32,
            KeyModifiers::default(),
        ));
        assert_eq!(del.direction, SelectionDirection::Forward);
        assert_eq!(del.step, SelectionStep::Character);
        assert_eq!(del.mode, SelectionMode::Delete);

        // Shift+arrow extends instead of moving.
        let shift_right = expect_op(&key_event(
            VirtualKeyCode::Right as u32,
            KeyModifiers::new().with_shift(),
        ));
        assert_eq!(shift_right.mode, SelectionMode::Extend);
        assert_eq!(shift_right.step, SelectionStep::Character);

        // The word modifier upgrades Backspace to a word delete.
        let word_mod = if cfg!(target_os = "macos") {
            KeyModifiers::new().with_alt()
        } else {
            KeyModifiers::new().with_ctrl()
        };
        let word_back = expect_op(&key_event(VirtualKeyCode::Back as u32, word_mod));
        assert_eq!(word_back.step, SelectionStep::Word);
        assert_eq!(word_back.mode, SelectionMode::Delete);
    }

    #[test]
    fn handle_key_down_ignores_keys_it_does_not_interpret() {
        let kb = KeyboardState::default();
        let target = Some(dnid(0, 1));
        // Ordinary text keys must pass through to the user callbacks untouched.
        for vk in [
            VirtualKeyCode::B,
            VirtualKeyCode::Q,
            VirtualKeyCode::Space,
            VirtualKeyCode::F5,
        ] {
            let ev = key_event(vk as u32, KeyModifiers::default());
            assert!(
                handle_key_down(&ev, &kb, target, true).is_none(),
                "{vk:?} must not generate a system change"
            );
        }
    }

    // ================================================ default_input_interpreter

    #[test]
    fn default_input_interpreter_with_no_events_produces_nothing() {
        let kb = KeyboardState::default();
        let mouse = MouseState::default();
        let info = InputInterpreterInfo {
            events: &[],
            hit_test: None,
            keyboard_state: &kb,
            mouse_state: &mouse,
            state: InputInterpreterState {
                focused_node: None,
                click_count: 0,
                drag_start_position: None,
                has_selection: false,
                focus_is_editable: true,
            },
        };
        let r = default_input_interpreter(&info);
        assert!(r.system_changes.is_empty());
        assert!(r.user_events.is_empty());
    }

    #[test]
    fn default_input_interpreter_skips_shortcut_events_but_passes_clicks_through() {
        let kb = KeyboardState::default();
        let mouse = MouseState::default();
        let ht = hit_test_with(0, &[(0, 0)]);
        let target = dnid(0, 1);

        // A primary+C shortcut is consumed (AddAndSkip) — the user callback must
        // NOT also see the raw key event...
        let copy = key_event(VirtualKeyCode::C as u32, primary_modifiers());
        // ...while a MouseDown is consumed AND forwarded (AddAndPass).
        let click = mouse_event(
            EventType::MouseDown,
            MouseButton::Left,
            LogicalPosition::zero(),
        );
        // ...and an unhandled event type is forwarded untouched.
        let scroll = SyntheticEvent::new(
            EventType::Scroll,
            EventSource::User,
            target,
            tick(0),
            EventData::None,
        );

        let events = vec![copy, click, scroll];
        let info = InputInterpreterInfo {
            events: &events,
            hit_test: Some(&ht),
            keyboard_state: &kb,
            mouse_state: &mouse,
            state: InputInterpreterState {
                focused_node: Some(target),
                click_count: 1,
                drag_start_position: None,
                has_selection: false,
                focus_is_editable: true,
            },
        };
        let r = default_input_interpreter(&info);

        assert_eq!(r.system_changes.len(), 2, "copy + selection click");
        assert!(r.system_changes.contains(&SystemChange::CopyToClipboard));
        assert!(r
            .system_changes
            .iter()
            .any(|c| matches!(c, SystemChange::TextSelectionClick { .. })));

        assert_eq!(
            r.user_events.len(),
            2,
            "the consumed KeyDown must not be forwarded"
        );
        assert!(!r
            .user_events
            .iter()
            .any(|e| e.event_type == EventType::KeyDown));
        assert!(r
            .user_events
            .iter()
            .any(|e| e.event_type == EventType::MouseDown));
        assert!(r
            .user_events
            .iter()
            .any(|e| e.event_type == EventType::Scroll));
    }

    #[test]
    fn default_input_interpreter_extern_survives_a_null_info_pointer() {
        // The C-ABI trampoline must null-check rather than deref garbage.
        let user_data = crate::refany::RefAny::new(0u8);
        let r = default_input_interpreter_extern(user_data, core::ptr::null());
        assert!(r.system_changes.is_empty());
        assert!(r.user_events.is_empty());
    }

    // ==================================================== post-callback filter

    #[test]
    fn post_filter_with_prevent_default_only_lets_focus_changes_through() {
        let old = Some(dnid(0, 1));
        let new = Some(dnid(0, 2));
        let pre = vec![
            SystemChange::TextSelectionClick {
                position: LogicalPosition::zero(),
                timestamp: tick(0),
            },
            SystemChange::PasteFromClipboard,
        ];

        // prevent_default + no focus change -> absolutely nothing (not even the
        // usual ApplyPendingTextInput).
        let out = default_post_filter(true, &pre, old, old);
        assert!(
            out.is_empty(),
            "preventDefault must suppress every side effect"
        );

        // prevent_default + a focus change -> ONLY the focus change.
        let out = default_post_filter(true, &pre, old, new);
        assert_eq!(
            out,
            vec![SystemChange::SetFocus {
                new_focus: new,
                old_focus: old,
                visible: false
            }]
        );
    }

    #[test]
    fn post_filter_maps_pre_changes_to_their_follow_ups() {
        // No pre-changes, no focus change -> just the text-input flush.
        let out = default_post_filter(false, &[], None, None);
        assert_eq!(out, vec![SystemChange::ApplyPendingTextInput]);

        // Cursor-moving ops schedule a scroll-into-view.
        for change in [
            SystemChange::TextSelectionClick {
                position: LogicalPosition::zero(),
                timestamp: tick(0),
            },
            SystemChange::ApplySelectionOp {
                target: dnid(0, 1),
                op: SelectionOp::new(
                    SelectionDirection::Forward,
                    SelectionStep::Character,
                    SelectionMode::Move,
                ),
            },
            SystemChange::AddCursorAtClick {
                position: LogicalPosition::zero(),
            },
            SystemChange::SelectNextOccurrence { target: dnid(0, 1) },
            SystemChange::CutToClipboard { target: dnid(0, 1) },
            SystemChange::PasteFromClipboard,
            SystemChange::UndoTextEdit { target: dnid(0, 1) },
            SystemChange::RedoTextEdit { target: dnid(0, 1) },
            SystemChange::SelectAllText,
        ] {
            let out = default_post_filter(false, core::slice::from_ref(&change), None, None);
            assert!(
                out.contains(&SystemChange::ScrollSelectionIntoView),
                "{change:?} must schedule a scroll-into-view"
            );
            assert_eq!(out[0], SystemChange::ApplyPendingTextInput);
        }

        // A drag starts the auto-scroll timer instead.
        let drag = SystemChange::TextSelectionDrag {
            start_position: LogicalPosition::zero(),
            current_position: LogicalPosition::new(1.0, 1.0),
        };
        let out = default_post_filter(false, core::slice::from_ref(&drag), None, None);
        assert!(out.contains(&SystemChange::StartAutoScrollTimer));
        assert!(!out.contains(&SystemChange::ScrollSelectionIntoView));

        // Changes with no follow-up add nothing beyond the text-input flush.
        let out = default_post_filter(false, &[SystemChange::CopyToClipboard], None, None);
        assert_eq!(out, vec![SystemChange::ApplyPendingTextInput]);
    }

    #[test]
    fn post_filter_emits_set_focus_only_when_focus_actually_moved() {
        let a = Some(dnid(0, 1));
        let b = Some(dnid(0, 2));
        // Unchanged (both Some, both None) -> no SetFocus.
        for (old, new) in [(a, a), (None, None)] {
            let out = default_post_filter(false, &[], old, new);
            assert!(!out
                .iter()
                .any(|c| matches!(c, SystemChange::SetFocus { .. })));
        }
        // Changed (including to/from None) -> exactly one SetFocus, and it is last.
        for (old, new) in [(a, b), (None, a), (a, None)] {
            let out = default_post_filter(false, &[], old, new);
            assert_eq!(
                out.last(),
                Some(&SystemChange::SetFocus {
                    new_focus: new,
                    old_focus: old,
                    visible: false
                })
            );
            assert_eq!(
                out.iter()
                    .filter(|c| matches!(c, SystemChange::SetFocus { .. }))
                    .count(),
                1
            );
        }
    }

    #[test]
    fn post_filter_handles_a_large_pre_change_list_without_blowing_up() {
        // 5000 cursor ops -> 1 flush + 5000 scroll-into-views. Bounded, no overflow.
        let pre: Vec<SystemChange> = (0..5000)
            .map(|_| SystemChange::AddCursorAtClick {
                position: LogicalPosition::zero(),
            })
            .collect();
        let out = default_post_filter(false, &pre, None, None);
        assert_eq!(out.len(), 5001);
        assert_eq!(out[0], SystemChange::ApplyPendingTextInput);
        assert!(out[1..]
            .iter()
            .all(|c| *c == SystemChange::ScrollSelectionIntoView));
    }

    #[test]
    fn default_post_filter_delegates_to_post_callback_filter_system_changes() {
        let pre = vec![
            SystemChange::TextSelectionDrag {
                start_position: LogicalPosition::zero(),
                current_position: LogicalPosition::new(2.0, 2.0),
            },
            SystemChange::SelectAllText,
        ];
        for prevent in [false, true] {
            for (old, new) in [(None, None), (Some(dnid(0, 1)), Some(dnid(0, 2)))] {
                assert_eq!(
                    default_post_filter(prevent, &pre, old, new),
                    post_callback_filter_system_changes(prevent, &pre, old, new),
                    "the two entry points must stay in lock-step"
                );
            }
        }
    }

    /// The default schema must be an empty op LIST, not null and not a string.
    ///
    /// A plugin deciding whether a host is usable has to distinguish "this app
    /// advertises no ops" from "this app returned nothing parseable". Those
    /// are different answers and only one of them means "move on".
    #[test]
    fn default_op_schema_is_an_empty_list_not_a_null() {
        let cb = CustomE2eOpCallback::default();
        assert!(cb.op_schema.is_object(), "schema must be an object");
        assert!(!cb.op_schema.is_null());
        let text = cb.op_schema.internal.string_value.as_str();
        assert!(text.contains("\"ops\""), "got {text}");

        // Positive control: a NON-empty schema must serialize its contents,
        // so this test cannot pass by everything being empty.
        let schema = E2eOpSchema {
            ops: alloc::vec![E2eOpDef {
                name: "load_document".to_string(),
                summary: "Open a file".to_string(),
                description: "Loads a markdown file into the editor.".to_string(),
                args: alloc::vec![E2eOpArg {
                    name: "path".to_string(),
                    arg_type: E2eOpArgType::String,
                    required: true,
                    description: "Absolute path.".to_string(),
                }],
                examples: alloc::vec![E2eOpExample {
                    description: "Open big.md".to_string(),
                    args: crate::json::Json::parse(r#"{"path":"/tmp/big.md"}"#).unwrap(),
                    returns: crate::json::Json::parse(r#"{"success":true,"pages":40}"#).unwrap(),
                }],
            }],
        };
        let j = schema.to_json();
        let t = j.internal.string_value.as_str();
        for needle in [
            "load_document",
            "Open a file",
            "\"type\":\"string\"",
            "big.md",
            "pages",
        ] {
            assert!(t.contains(needle), "missing {needle} in {t}");
        }
    }

    fn sample_op(returns: &str) -> E2eOpDef {
        E2eOpDef {
            name: "load_document".to_string(),
            summary: "Open a markdown file".to_string(),
            description: "Reads and paginates a file.".to_string(),
            args: alloc::vec![E2eOpArg {
                name: "path".to_string(),
                arg_type: E2eOpArgType::String,
                required: true,
                description: "Absolute path.".to_string(),
            }],
            examples: alloc::vec![E2eOpExample {
                description: "Open big.md".to_string(),
                args: crate::json::Json::parse(r#"{"path":"/tmp/big.md"}"#).unwrap(),
                returns: crate::json::Json::parse(returns).unwrap(),
            }],
        }
    }

    /// `success` is REQUIRED, and checked when the schema is installed.
    #[test]
    fn schema_validation_requires_a_success_boolean_in_every_example() {
        let ok = E2eOpSchema {
            ops: alloc::vec![sample_op(r#"{"success":true,"pages":40}"#)],
        };
        assert_eq!(ok.validate(), Ok(()));

        // NEGATIVE CONTROL: drop `success` and validation must reject.
        let missing = E2eOpSchema {
            ops: alloc::vec![sample_op(r#"{"pages":40}"#)],
        };
        assert!(matches!(
            missing.validate(),
            Err(E2eSchemaError::ExampleMissingSuccess { .. })
        ));

        // Present but not a BOOLEAN is still a reject — `"success":"yes"` is
        // what a hand-written schema actually produces and tells a consumer
        // nothing.
        let stringy = E2eOpSchema {
            ops: alloc::vec![sample_op(r#"{"success":"yes"}"#)],
        };
        assert!(stringy.validate().is_err());

        let dupe = E2eOpSchema {
            ops: alloc::vec![
                sample_op(r#"{"success":true}"#),
                sample_op(r#"{"success":true}"#),
            ],
        };
        assert!(matches!(
            dupe.validate(),
            Err(E2eSchemaError::DuplicateOpName { .. })
        ));
    }

    /// Identity fields lead, and examples are NESTED JSON not escaped strings.
    #[test]
    fn schema_json_keeps_declaration_order_and_nests_examples() {
        let schema = E2eOpSchema {
            ops: alloc::vec![sample_op(r#"{"success":true,"pages":40}"#)],
        };
        let text = schema.to_json().internal.string_value.as_str().to_string();

        let name_at = text.find("\"name\"").expect("name present");
        let args_at = text.find("\"args\"").expect("args present");
        assert!(name_at < args_at, "identity fields must lead: {text}");

        // Nested, so NO backslash-escaped quotes anywhere in the payload.
        assert!(
            !text.contains("\\\""),
            "examples must nest, not escape: {text}"
        );
        assert!(text.contains("\"success\":true"), "{text}");
    }

    #[test]
    fn default_custom_op_handler_recognises_nothing() {
        // handled=false is the load-bearing default: an app with no handler
        // must make a scenario naming a custom op FAIL, not pass quietly.
        let r = default_custom_e2e_op_extern(
            crate::refany::RefAny::new(0u8),
            AzString::from_const_str("anything"),
            AzString::from_const_str("{}"),
        );
        assert!(!r.handled);
    }

    fn default_post_filter_extern_decodes_the_none_focus_sentinel() {
        // old_focus = the `None` sentinel, new_focus = a real node => a focus change.
        let pre: Vec<SystemChange> = Vec::new();
        let slice = SystemChangeVecSlice {
            ptr: pre.as_ptr(),
            len: pre.len(),
        };
        let out = default_post_filter_extern(
            crate::refany::RefAny::new(0u8),
            false,
            slice,
            dnid_none(0),
            dnid(0, 4),
        );
        let changes = out.as_slice();
        assert_eq!(changes.first(), Some(&SystemChange::ApplyPendingTextInput));
        assert_eq!(
            changes.last(),
            Some(&SystemChange::SetFocus {
                new_focus: Some(dnid(0, 4)),
                old_focus: None,
                visible: false,
            }),
            "a `NONE` node id must decode to `None`, not to node 0"
        );

        // An empty C-slice must be accepted (ptr may be dangling-but-aligned).
        let out = default_post_filter_extern(
            crate::refany::RefAny::new(0u8),
            true,
            SystemChangeVecSlice::empty(),
            dnid_none(0),
            dnid_none(0),
        );
        assert!(out.as_slice().is_empty());
    }
}

/// KEYBOARD ACTIVATION: Enter / Space on a focused element dispatch a
/// synthetic `EventType::Click`, but every widget in the toolkit registers
/// `HoverEventFilter::MouseUp` (the pointer spelling of "activate"). Without a
/// Click -> MouseUp arm the synthetic event matched NOTHING, so Enter and
/// Space silently did nothing on every focusable widget - the device report
/// of 2026-08-31 ("space did nothing when the item was focused").
#[test]
fn a_synthetic_click_activates_a_click_listener_and_nothing_else() {
    use crate::{
        dom::{DomId, DomNodeId},
        events::{
            EventData, EventFilter, EventPhase, EventSource, EventType, HoverEventFilter,
            SyntheticEvent,
        },
        styled_dom::NodeHierarchyItemId,
        task::{Instant, SystemTick},
    };

    let target = DomNodeId {
        dom: DomId::ROOT_ID,
        node: NodeHierarchyItemId::from_crate_internal(Some(crate::dom::NodeId::new(1))),
    };
    let click = SyntheticEvent::new(
        EventType::Click,
        EventSource::User,
        target,
        Instant::Tick(SystemTick::new(0)),
        EventData::None,
    );

    assert!(
        super::matches_filter_phase(
            EventFilter::Hover(HoverEventFilter::Click),
            &click,
            EventPhase::Bubble
        ),
        "a keyboard-activation Click must reach a Click listener",
    );
    // ...and must NOT reach a raw-release listener, or every activation runs
    // twice for a real pointer click (which emits MouseUp AND Click).
    for filter in [HoverEventFilter::MouseUp, HoverEventFilter::LeftMouseUp] {
        assert!(
            !super::matches_filter_phase(EventFilter::Hover(filter), &click, EventPhase::Bubble),
            "a Click must not also fire {filter:?}",
        );
    }

    // It must NOT masquerade as anything else - a Click is an activation, not
    // a press, and a real pointer click still arrives as MouseDown + MouseUp.
    assert!(
        !super::matches_filter_phase(
            EventFilter::Hover(HoverEventFilter::MouseDown),
            &click,
            EventPhase::Bubble
        ),
        "a Click must not fire MouseDown listeners",
    );
}

/// ARCHITECTURAL INVARIANT: the engine keeps TWO tables that map an event
/// onto listeners, and nothing but discipline kept them in agreement.
///
///  * `event_type_to_filters` drives dispatch PLANNING - which callbacks get
///    collected for an event.
///  * `matches_filter_phase` drives phase MATCHING - whether a collected
///    callback actually fires.
///
/// A de-sync is SILENT: the event simply never reaches the callback. That is
/// exactly how Enter/Space died on every focusable widget - planning listed
/// only `LeftMouseUp` for `EventType::Click` while every widget registers the
/// generic `MouseUp`, so activation collected nothing, and separately the
/// matcher had no `Click` arm at all.
///
/// This test makes the invariant mechanical instead of a comment: for every
/// pointer-ish event, the two tables must agree on every hover filter.
#[test]
fn dispatch_planning_and_phase_matching_agree_on_every_hover_filter() {
    use crate::{
        dom::{DomId, DomNodeId, NodeId},
        events::{
            event_type_to_filters, EventData, EventFilter, EventPhase, EventSource, EventType,
            HoverEventFilter, SyntheticEvent,
        },
        styled_dom::NodeHierarchyItemId,
        task::{Instant, SystemTick},
    };

    const FILTERS: &[HoverEventFilter] = &[
        HoverEventFilter::MouseOver,
        HoverEventFilter::MouseDown,
        HoverEventFilter::LeftMouseDown,
        HoverEventFilter::RightMouseDown,
        HoverEventFilter::MiddleMouseDown,
        HoverEventFilter::MouseUp,
        HoverEventFilter::LeftMouseUp,
        HoverEventFilter::RightMouseUp,
        HoverEventFilter::MiddleMouseUp,
        HoverEventFilter::MouseEnter,
        HoverEventFilter::MouseLeave,
        HoverEventFilter::Scroll,
    ];
    const EVENTS: &[EventType] = &[
        EventType::Click,
        EventType::MouseUp,
        EventType::MouseDown,
        EventType::MouseOver,
        EventType::MouseEnter,
        EventType::MouseLeave,
        EventType::Scroll,
    ];

    let target = DomNodeId {
        dom: DomId::ROOT_ID,
        node: NodeHierarchyItemId::from_crate_internal(Some(NodeId::new(1))),
    };

    for &event_type in EVENTS {
        let event = SyntheticEvent::new(
            event_type,
            EventSource::User,
            target,
            Instant::Tick(SystemTick::new(0)),
            EventData::None,
        );
        let planned = event_type_to_filters(event_type, &event.data);
        for &f in FILTERS {
            let filter = EventFilter::Hover(f);
            let is_planned = planned.contains(&filter);
            let does_match =
                super::matches_filter_phase(filter, &event, EventPhase::Bubble);
            assert_eq!(
                is_planned, does_match,
                "de-sync for {event_type:?} x {f:?}: planning says {is_planned}, \
                 matching says {does_match} - a callback registered on {f:?} would \
                 {} (see the doc on this test)",
                if is_planned { "be collected and then ignored" } else { "never be collected" },
            );
        }
    }
}

/// EXHAUSTIVE INVARIANT over the WHOLE event pipeline.
///
/// The engine keeps two tables that map an event onto listeners and nothing
/// but discipline kept them in agreement:
///
///  * `event_type_to_filters` drives dispatch PLANNING - which callbacks get
///    collected for an event.
///  * `matches_filter_phase` drives phase MATCHING - whether a collected
///    callback actually fires.
///
/// A de-sync is SILENT in both directions: a filter planned but not matched
/// collects a callback and drops it; a filter matched but not planned can
/// never fire at all. That is how Enter/Space died on every widget, and it is
/// how a real pointer click briefly fired activation handlers TWICE (the
/// ColorInput opened its picker and instantly closed it again).
///
/// The narrow version of this test covered pointer events only. This one
/// covers EVERY unit event type against EVERY filter of all FOUR kinds, so
/// the next such bug fails here instead of on a device. Four, not three: the
/// derivation shipped probing Hover/Focus/Window only, and this test - which
/// then knew the same three families - stayed green while every lifecycle
/// callback in the engine was dead (`Mount` planned no filter at all). A
/// family this test does not enumerate is a family the derivation can forget.
#[test]
fn planning_and_matching_agree_for_every_event_and_filter() {
    use crate::{
        dom::{DomId, DomNodeId, NodeId},
        events::{
            event_type_to_filters, ComponentEventFilter, EventData, EventFilter, EventPhase,
            EventSource, EventType, FocusEventFilter, HoverEventFilter, SyntheticEvent,
            WindowEventFilter,
        },
        styled_dom::NodeHierarchyItemId,
        task::{Instant, SystemTick},
    };

    const HOVER: &[HoverEventFilter] = &[
        HoverEventFilter::MouseOver,
        HoverEventFilter::MouseDown,
        HoverEventFilter::LeftMouseDown,
        HoverEventFilter::RightMouseDown,
        HoverEventFilter::MiddleMouseDown,
        HoverEventFilter::Click,
        HoverEventFilter::MouseUp,
        HoverEventFilter::LeftMouseUp,
        HoverEventFilter::RightMouseUp,
        HoverEventFilter::MiddleMouseUp,
        HoverEventFilter::MouseEnter,
        HoverEventFilter::MouseLeave,
        HoverEventFilter::Scroll,
        HoverEventFilter::ScrollStart,
        HoverEventFilter::ScrollEnd,
        HoverEventFilter::TextInput,
        HoverEventFilter::VirtualKeyDown,
        HoverEventFilter::VirtualKeyUp,
        HoverEventFilter::HoveredFile,
        HoverEventFilter::DroppedFile,
        HoverEventFilter::HoveredFileCancelled,
        HoverEventFilter::TouchStart,
        HoverEventFilter::TouchMove,
        HoverEventFilter::TouchEnd,
        HoverEventFilter::TouchCancel,
        HoverEventFilter::PenDown,
        HoverEventFilter::PenMove,
        HoverEventFilter::PenUp,
        HoverEventFilter::PenEnter,
        HoverEventFilter::PenLeave,
        HoverEventFilter::PenSqueeze,
        HoverEventFilter::PenDoubleTap,
        HoverEventFilter::PenHover,
        HoverEventFilter::GeolocationFix,
        HoverEventFilter::GeolocationError,
        HoverEventFilter::SensorChanged,
        HoverEventFilter::GamepadInput,
        HoverEventFilter::DragStart,
        HoverEventFilter::Drag,
        HoverEventFilter::DragEnd,
        HoverEventFilter::DragEnter,
        HoverEventFilter::DragOver,
        HoverEventFilter::DragLeave,
        HoverEventFilter::Drop,
        HoverEventFilter::DoubleClick,
        HoverEventFilter::LongPress,
        HoverEventFilter::SwipeLeft,
        HoverEventFilter::SwipeRight,
        HoverEventFilter::SwipeUp,
        HoverEventFilter::SwipeDown,
        HoverEventFilter::PinchIn,
        HoverEventFilter::PinchOut,
        HoverEventFilter::RotateClockwise,
        HoverEventFilter::RotateCounterClockwise,
        HoverEventFilter::MouseOut,
        HoverEventFilter::FocusIn,
        HoverEventFilter::FocusOut,
        HoverEventFilter::CompositionStart,
        HoverEventFilter::CompositionUpdate,
        HoverEventFilter::CompositionEnd,
        HoverEventFilter::SystemTextSingleClick,
        HoverEventFilter::SystemTextDoubleClick,
        HoverEventFilter::SystemTextTripleClick,
        HoverEventFilter::PermissionChanged,
        HoverEventFilter::BiometricResult,
        HoverEventFilter::ScreenColorPicked,
        HoverEventFilter::KeyringResult,
    ];
    const FOCUS: &[FocusEventFilter] = &[
        FocusEventFilter::MouseOver,
        FocusEventFilter::MouseDown,
        FocusEventFilter::LeftMouseDown,
        FocusEventFilter::RightMouseDown,
        FocusEventFilter::MiddleMouseDown,
        FocusEventFilter::MouseUp,
        FocusEventFilter::LeftMouseUp,
        FocusEventFilter::RightMouseUp,
        FocusEventFilter::MiddleMouseUp,
        FocusEventFilter::MouseEnter,
        FocusEventFilter::MouseLeave,
        FocusEventFilter::Scroll,
        FocusEventFilter::ScrollStart,
        FocusEventFilter::ScrollEnd,
        FocusEventFilter::TextInput,
        FocusEventFilter::VirtualKeyDown,
        FocusEventFilter::VirtualKeyUp,
        FocusEventFilter::FocusReceived,
        FocusEventFilter::FocusLost,
        FocusEventFilter::PenDown,
        FocusEventFilter::PenMove,
        FocusEventFilter::PenUp,
        FocusEventFilter::DragStart,
        FocusEventFilter::Drag,
        FocusEventFilter::DragEnd,
        FocusEventFilter::DragEnter,
        FocusEventFilter::DragOver,
        FocusEventFilter::DragLeave,
        FocusEventFilter::Drop,
        FocusEventFilter::DoubleClick,
        FocusEventFilter::LongPress,
        FocusEventFilter::SwipeLeft,
        FocusEventFilter::SwipeRight,
        FocusEventFilter::SwipeUp,
        FocusEventFilter::SwipeDown,
        FocusEventFilter::PinchIn,
        FocusEventFilter::PinchOut,
        FocusEventFilter::RotateClockwise,
        FocusEventFilter::RotateCounterClockwise,
        FocusEventFilter::FocusIn,
        FocusEventFilter::FocusOut,
        FocusEventFilter::CompositionStart,
        FocusEventFilter::CompositionUpdate,
        FocusEventFilter::CompositionEnd,
        FocusEventFilter::Copy,
        FocusEventFilter::Cut,
        FocusEventFilter::Paste,
        FocusEventFilter::DocumentEdit,
        FocusEventFilter::TextChanged,
    ];
    const WINDOW: &[WindowEventFilter] = &[
        WindowEventFilter::MouseOver,
        WindowEventFilter::MouseDown,
        WindowEventFilter::LeftMouseDown,
        WindowEventFilter::RightMouseDown,
        WindowEventFilter::MiddleMouseDown,
        WindowEventFilter::MouseUp,
        WindowEventFilter::LeftMouseUp,
        WindowEventFilter::RightMouseUp,
        WindowEventFilter::MiddleMouseUp,
        WindowEventFilter::MouseEnter,
        WindowEventFilter::MouseLeave,
        WindowEventFilter::Scroll,
        WindowEventFilter::ScrollStart,
        WindowEventFilter::ScrollEnd,
        WindowEventFilter::TextInput,
        WindowEventFilter::VirtualKeyDown,
        WindowEventFilter::VirtualKeyUp,
        WindowEventFilter::HoveredFile,
        WindowEventFilter::DroppedFile,
        WindowEventFilter::HoveredFileCancelled,
        WindowEventFilter::Resized,
        WindowEventFilter::Moved,
        WindowEventFilter::FrameChanged,
        WindowEventFilter::TouchStart,
        WindowEventFilter::TouchMove,
        WindowEventFilter::TouchEnd,
        WindowEventFilter::TouchCancel,
        WindowEventFilter::FocusReceived,
        WindowEventFilter::FocusLost,
        WindowEventFilter::CloseRequested,
        WindowEventFilter::ThemeChanged,
        WindowEventFilter::WindowFocusReceived,
        WindowEventFilter::WindowFocusLost,
        WindowEventFilter::PenDown,
        WindowEventFilter::PenMove,
        WindowEventFilter::PenUp,
        WindowEventFilter::PenEnter,
        WindowEventFilter::PenLeave,
        WindowEventFilter::PenSqueeze,
        WindowEventFilter::PenDoubleTap,
        WindowEventFilter::PenHover,
        WindowEventFilter::GeolocationFix,
        WindowEventFilter::GeolocationError,
        WindowEventFilter::SensorChanged,
        WindowEventFilter::GamepadInput,
        WindowEventFilter::DragStart,
        WindowEventFilter::Drag,
        WindowEventFilter::DragEnd,
        WindowEventFilter::DragEnter,
        WindowEventFilter::DragOver,
        WindowEventFilter::DragLeave,
        WindowEventFilter::Drop,
        WindowEventFilter::DoubleClick,
        WindowEventFilter::LongPress,
        WindowEventFilter::SwipeLeft,
        WindowEventFilter::SwipeRight,
        WindowEventFilter::SwipeUp,
        WindowEventFilter::SwipeDown,
        WindowEventFilter::PinchIn,
        WindowEventFilter::PinchOut,
        WindowEventFilter::RotateClockwise,
        WindowEventFilter::RotateCounterClockwise,
        WindowEventFilter::DpiChanged,
        WindowEventFilter::MonitorChanged,
        WindowEventFilter::PermissionChanged,
        WindowEventFilter::BiometricResult,
        WindowEventFilter::ScreenColorPicked,
        WindowEventFilter::KeyringResult,
    ];
    const COMPONENT: &[ComponentEventFilter] = &[
        ComponentEventFilter::AfterMount,
        ComponentEventFilter::BeforeUnmount,
        ComponentEventFilter::NodeResized,
        ComponentEventFilter::DefaultAction,
        ComponentEventFilter::Selected,
        ComponentEventFilter::Updated,
        ComponentEventFilter::Dismissed,
        ComponentEventFilter::TornOff,
        ComponentEventFilter::Docked,
    ];
    const EVENTS: &[EventType] = &[
        EventType::MouseOver,
        EventType::MouseEnter,
        EventType::MouseLeave,
        EventType::MouseOut,
        EventType::MouseDown,
        EventType::MouseUp,
        EventType::Click,
        EventType::DoubleClick,
        EventType::ContextMenu,
        EventType::KeyDown,
        EventType::KeyUp,
        EventType::KeyPress,
        EventType::CompositionStart,
        EventType::CompositionUpdate,
        EventType::CompositionEnd,
        EventType::Focus,
        EventType::Blur,
        EventType::FocusIn,
        EventType::FocusOut,
        EventType::Input,
        EventType::Change,
        EventType::Submit,
        EventType::Reset,
        EventType::Invalid,
        EventType::Scroll,
        EventType::ScrollStart,
        EventType::ScrollEnd,
        EventType::DragStart,
        EventType::Drag,
        EventType::DragEnd,
        EventType::DragEnter,
        EventType::DragOver,
        EventType::DragLeave,
        EventType::Drop,
        EventType::TouchStart,
        EventType::TouchMove,
        EventType::TouchEnd,
        EventType::TouchCancel,
        EventType::PenDown,
        EventType::PenMove,
        EventType::PenUp,
        EventType::PenEnter,
        EventType::PenLeave,
        EventType::LongPress,
        EventType::SwipeLeft,
        EventType::SwipeRight,
        EventType::SwipeUp,
        EventType::SwipeDown,
        EventType::PinchIn,
        EventType::PinchOut,
        EventType::RotateClockwise,
        EventType::RotateCounterClockwise,
        EventType::Copy,
        EventType::Cut,
        EventType::Paste,
        EventType::Play,
        EventType::Pause,
        EventType::Ended,
        EventType::TimeUpdate,
        EventType::VolumeChange,
        EventType::MediaError,
        EventType::Mount,
        EventType::Unmount,
        EventType::Update,
        EventType::Resize,
        EventType::Dismiss,
        EventType::TearOff,
        EventType::Dock,
        EventType::WindowResize,
        EventType::WindowMove,
        EventType::WindowClose,
        EventType::WindowFrameChanged,
        EventType::WindowFocusIn,
        EventType::WindowFocusOut,
        EventType::ThemeChange,
        EventType::WindowDpiChanged,
        EventType::WindowMonitorChanged,
        EventType::MonitorConnected,
        EventType::MonitorDisconnected,
        EventType::FileHover,
        EventType::FileDrop,
        EventType::FileHoverCancel,
        EventType::SensorChanged,
        EventType::GamepadInput,
        EventType::GeolocationFix,
        EventType::GeolocationError,
        EventType::PermissionChanged,
        EventType::BiometricResult,
        EventType::ScreenColorPicked,
        EventType::KeyringResult,
        EventType::DocumentEdit,
        EventType::TextChanged,
    ];

    let target = DomNodeId {
        dom: DomId::ROOT_ID,
        node: NodeHierarchyItemId::from_crate_internal(Some(NodeId::new(1))),
    };

    let mut desyncs: Vec<String> = Vec::new();
    for &event_type in EVENTS {
        let event = SyntheticEvent::new(
            event_type,
            EventSource::User,
            target,
            Instant::Tick(SystemTick::new(0)),
            EventData::None,
        );
        let planned = event_type_to_filters(event_type, &event.data);
        let mut check = |filter: EventFilter| {
            let is_planned = planned.contains(&filter);
            let does_match = super::matches_filter_phase(filter, &event, EventPhase::Bubble);
            if is_planned != does_match {
                desyncs.push(format!(
                    "{event_type:?} x {filter:?}: planned={is_planned} matched={does_match}"
                ));
            }
        };
        for &f in HOVER {
            check(EventFilter::Hover(f));
        }
        for &f in FOCUS {
            check(EventFilter::Focus(f));
        }
        for &f in WINDOW {
            check(EventFilter::Window(f));
        }
        for &f in COMPONENT {
            check(EventFilter::Component(f));
        }
    }

    // ZERO de-syncs, permanently: planning is DERIVED from matching
    // (`event_type_to_filters` probes `matches_filter_phase` over the whole
    // filter universe), so the two cannot drift. Before that they were two
    // hand-written tables and this cross-product found 61 disagreements - in
    // both directions: filters the matcher accepted but planning never
    // emitted (pen enter/leave, scroll start/end, every focus-scoped drag
    // event - listeners that could never fire), and filters planned but not
    // matched (IME composition, window double-click - collected, then
    // dropped). If this ever fails again, the two tables have been split
    // apart once more.
    assert!(
        desyncs.is_empty(),
        "dispatch planning and phase matching disagree on {} pair(s):\n{}",
        desyncs.len(),
        desyncs.join("\n"),
    );
}

/// Every lifecycle event plans exactly its own `Component` listener.
///
/// The cross-product test above proves planning and matching agree; this one
/// pins WHAT they agree on for the lifecycle family, in the terms a widget
/// author uses: an `AfterMount` callback is reached by a `Mount` event and by
/// nothing else. It is the unit-level shadow of `dll/tests/headless_lifecycle`,
/// which drives the same contract through a real window and was the test that
/// found the family missing (mount=0 on the very first frame).
#[test]
fn every_lifecycle_event_plans_exactly_its_component_listener() {
    use crate::events::{
        event_type_to_filters, ComponentEventFilter, EventData, EventFilter, EventType,
    };

    const PAIRS: &[(EventType, ComponentEventFilter)] = &[
        (EventType::Mount, ComponentEventFilter::AfterMount),
        (EventType::Unmount, ComponentEventFilter::BeforeUnmount),
        (EventType::Update, ComponentEventFilter::Updated),
        (EventType::Resize, ComponentEventFilter::NodeResized),
        (EventType::Dismiss, ComponentEventFilter::Dismissed),
        (EventType::TearOff, ComponentEventFilter::TornOff),
        (EventType::Dock, ComponentEventFilter::Docked),
    ];

    for &(event_type, listener) in PAIRS {
        let planned = event_type_to_filters(event_type, &EventData::None);
        let components: Vec<ComponentEventFilter> = planned
            .iter()
            .filter_map(|f| match f {
                EventFilter::Component(c) => Some(*c),
                _ => None,
            })
            .collect();
        assert_eq!(
            components,
            vec![listener],
            "{event_type:?} must reach exactly the {listener:?} listener; planned {planned:?}"
        );
    }

    // And the other direction: a pointer event never wakes a lifecycle
    // listener (a Mount callback firing on a click would be as wrong as one
    // never firing).
    for ty in [EventType::Click, EventType::MouseUp, EventType::KeyDown, EventType::Scroll] {
        let planned = event_type_to_filters(ty, &EventData::None);
        assert!(
            !planned.iter().any(|f| matches!(f, EventFilter::Component(_))),
            "{ty:?} must not plan a lifecycle listener; planned {planned:?}"
        );
    }
}

/// A REAL POINTER CLICK must activate a control exactly ONCE.
///
/// `event_determination` emits BOTH `MouseUp` and `Click` for one release (as
/// browsers do). While `Click` also mapped onto MouseUp listeners, every
/// activation handler ran twice - the ColorInput's toggle opened its picker
/// and immediately closed it, so clicking the swatch appeared to do nothing
/// (device report, 2026-09-01).
#[test]
fn a_pointer_release_activates_a_click_listener_exactly_once() {
    use crate::{
        dom::{DomId, DomNodeId, NodeId},
        events::{
            event_type_to_filters, EventData, EventFilter, HoverEventFilter, MouseButton,
            MouseEventData,
        },
        styled_dom::NodeHierarchyItemId,
    };

    let _target = DomNodeId {
        dom: DomId::ROOT_ID,
        node: NodeHierarchyItemId::from_crate_internal(Some(NodeId::new(1))),
    };
    let left = EventData::Mouse(MouseEventData {
        position: crate::geom::LogicalPosition::zero(),
        button: MouseButton::Left,
        buttons: 0,
        modifiers: crate::events::KeyModifiers::new(),
        ..Default::default()
    });

    let activation = EventFilter::Hover(HoverEventFilter::Click);
    let up_filters = event_type_to_filters(EventType::MouseUp, &left);
    let click_filters = event_type_to_filters(EventType::Click, &left);

    // The pair of events a single release produces must together reach the
    // activation listener EXACTLY once.
    let hits = usize::from(up_filters.contains(&activation))
        + usize::from(click_filters.contains(&activation));
    assert_eq!(
        hits, 1,
        "one release must activate once: MouseUp planned {up_filters:?}, Click planned \
         {click_filters:?}",
    );

    // And symmetrically, a raw-release listener must fire once too.
    let raw = EventFilter::Hover(HoverEventFilter::MouseUp);
    let raw_hits =
        usize::from(up_filters.contains(&raw)) + usize::from(click_filters.contains(&raw));
    assert_eq!(raw_hits, 1, "a MouseUp listener must also fire exactly once");
}

/// Keyboard and assistive-technology activation reach the SAME listener a
/// pointer click does - one activation concept, not three near-misses.
#[test]
fn keyboard_and_a11y_activation_reach_the_same_listener_as_a_click() {
    use crate::{
        dom::On,
        events::{event_type_to_filters, EventData, EventFilter, HoverEventFilter},
    };

    let activation = EventFilter::Hover(HoverEventFilter::Click);

    // Enter / Space dispatch a synthetic Click.
    assert!(event_type_to_filters(EventType::Click, &EventData::None).contains(&activation));

    // The a11y default action maps to the same filter.
    assert_eq!(EventFilter::from(On::Default), activation);
    // ...and so does the public `On::Click` sugar.
    assert_eq!(EventFilter::from(On::Click), activation);
}

/// A press is NOT an activation: it must not reach a Click listener, or a
/// control would fire before the user could release somewhere else.
#[test]
fn a_press_alone_never_activates() {
    use crate::events::{event_type_to_filters, EventData, EventFilter, HoverEventFilter};

    let planned = event_type_to_filters(EventType::MouseDown, &EventData::None);
    assert!(
        !planned.contains(&EventFilter::Hover(HoverEventFilter::Click)),
        "MouseDown must not activate: {planned:?}",
    );
}

/// ARROW KEYS BELONG TO THE FOCUSED WIDGET unless a TEXT EDITOR has focus.
///
/// The input interpreter used to claim every arrow for a caret op and return
/// `AddAndSkip`, which SWALLOWS the event - it never reached user callbacks.
/// A focused Slider or colour-picker plane therefore could not implement
/// arrow keys at all: the handler was never called (device report,
/// 2026-09-01, "the arrow keys for sliders do not work at all, nor the four
/// arrow keys for navigating the color gradient"). F1 and letter keys worked,
/// which is what pointed at the interpreter rather than at dispatch.
#[test]
fn arrows_are_claimed_for_the_caret_only_while_editing() {
    use crate::{
        dom::{DomId, DomNodeId},
        events::{
            EventData, EventSource, EventType, KeyModifiers, KeyboardEventData, SyntheticEvent,
        },
        id::NodeId,
        styled_dom::NodeHierarchyItemId,
        task::{Instant, SystemTick},
        window::{
            KeyboardState, OptionVirtualKeyCode, VirtualKeyCode, VirtualKeyCodeVec,
        },
    };

    let target = DomNodeId {
        dom: DomId { inner: 0 },
        node: NodeHierarchyItemId::from_crate_internal(Some(NodeId::new(2))),
    };
    let arrow = || {
        SyntheticEvent::new(
            EventType::KeyDown,
            EventSource::User,
            target,
            Instant::Tick(SystemTick::new(0)),
            EventData::Keyboard(KeyboardEventData {
                key_code: VirtualKeyCode::Left as u32,
                char_code: None,
                modifiers: KeyModifiers::default(),
                repeat: false,
                ..Default::default()
            }),
        )
    };
    let kb = KeyboardState {
        current_virtual_keycode: OptionVirtualKeyCode::Some(VirtualKeyCode::Left),
        pressed_virtual_keycodes: VirtualKeyCodeVec::from_vec(vec![VirtualKeyCode::Left]),
        ..KeyboardState::default()
    };

    // EDITING: the caret owns the arrow.
    assert!(
        super::handle_key_down(&arrow(), &kb, Some(target), true).is_some(),
        "a text-editing focus must still take the arrow for caret movement",
    );

    // NOT EDITING: the interpreter must keep its hands off, so the event
    // reaches the focused widget (and, failing that, the scroll default).
    assert!(
        super::handle_key_down(&arrow(), &kb, Some(target), false).is_none(),
        "outside a text editor the arrow must pass through to the widget",
    );
}

/// EVERY filter category must be probed by planning.
///
/// Planning is DERIVED: `event_type_to_filters` tests each filter in a set of
/// `ALL_*` lists against a probe event. A category that has no list, or a
/// filter missing from one, can never be planned however correct its matcher
/// is — and the failure is silent, because the callback is simply never
/// collected.
///
/// Component and Application had NO list at all, so every lifecycle callback
/// (`AfterMount`, `NodeResized`, `Updated`, `Dismissed`, `TornOff`, `Docked`)
/// and every device/monitor callback in every app was dead. This asserts the
/// round trip for each of them, so a new filter variant cannot be added
/// without being plannable.
#[test]
fn every_component_and_application_filter_is_reachable_from_planning() {
    use crate::events::{
        event_type_to_filters, ApplicationEventFilter as A, ComponentEventFilter as C, EventData,
        EventFilter, EventType,
    };

    let component: &[(C, EventType)] = &[
        (C::AfterMount, EventType::Mount),
        (C::BeforeUnmount, EventType::Unmount),
        (C::NodeResized, EventType::Resize),
        (C::DefaultAction, EventType::DefaultAction),
        (C::Selected, EventType::Selected),
        (C::Updated, EventType::Update),
        (C::Dismissed, EventType::Dismiss),
        (C::TornOff, EventType::TearOff),
        (C::Docked, EventType::Dock),
    ];
    for (filter, event_type) in component {
        let planned = event_type_to_filters(*event_type, &EventData::None);
        assert!(
            planned.contains(&EventFilter::Component(*filter)),
            "{event_type:?} must plan {filter:?}, got {planned:?}",
        );
    }

    let application: &[(A, EventType)] = &[
        (A::DeviceConnected, EventType::DeviceConnected),
        (A::DeviceDisconnected, EventType::DeviceDisconnected),
        (A::MonitorConnected, EventType::MonitorConnected),
        (A::MonitorDisconnected, EventType::MonitorDisconnected),
        (A::MediaControl, EventType::MediaControl),
    ];
    for (filter, event_type) in application {
        let planned = event_type_to_filters(*event_type, &EventData::None);
        assert!(
            planned.contains(&EventFilter::Application(*filter)),
            "{event_type:?} must plan {filter:?}, got {planned:?}",
        );
    }
}

/// The dial's four layers agree: the filters are PLANNABLE and they match.
///
/// `DialState` was readable through `CallbackInfo::get_dial_state()` from the
/// day the type landed, with no `EventType` and no filter behind it — so a
/// dial could only be POLLED from an unrelated callback that happened to run.
/// This pins the round trip, so a filter that exists but is absent from
/// `ALL_HOVER`/`ALL_WINDOW` (and is therefore unplannable) fails here.
#[test]
fn the_dial_filters_are_reachable_from_planning() {
    use crate::events::{
        event_type_to_filters, EventData, EventFilter, EventType, HoverEventFilter as H,
        WindowEventFilter as W,
    };

    for (event_type, hover, window) in [
        (EventType::DialRotate, H::DialRotate, W::DialRotate),
        (EventType::DialClick, H::DialClick, W::DialClick),
    ] {
        let planned = event_type_to_filters(event_type, &EventData::None);
        assert!(
            planned.contains(&EventFilter::Window(window)),
            "{event_type:?} must plan {window:?}, got {planned:?}",
        );
        assert!(
            planned.contains(&EventFilter::Hover(hover)),
            "{event_type:?} must plan {hover:?}, got {planned:?}",
        );
    }
}


#[cfg(test)]
mod seat_dedup_tests {
    use crate::{
        dom::{DomId, DomNodeId},
        events::{
            deduplicate_synthetic_events, EventData, EventSource, EventType, KeyModifiers,
            MouseButton, MouseEventData, SyntheticEvent,
        },
        geom::LogicalPosition,
        id::NodeId,
        styled_dom::NodeHierarchyItemId,
        task::{Instant, SystemTick},
    };

    fn press(seat_id: u64) -> SyntheticEvent {
        SyntheticEvent::new(
            EventType::MouseDown,
            EventSource::User,
            DomNodeId {
                dom: DomId::ROOT_ID,
                node: NodeHierarchyItemId::from_crate_internal(Some(NodeId::new(4))),
            },
            Instant::Tick(SystemTick { tick_counter: 0 }),
            EventData::Mouse(MouseEventData {
                position: LogicalPosition::zero(),
                button: MouseButton::Left,
                buttons: 1,
                modifiers: KeyModifiers::default(),
                seat_id,
                ..Default::default()
            }),
        )
    }

    #[test]
    fn two_seats_pressing_one_node_are_two_presses() {
        // The touch path's documented limit, no longer shared by seats
        // (9b-ii-b): coalescing by (target, type) alone folded them into one.
        let out = deduplicate_synthetic_events(vec![press(0), press(7)]);
        assert_eq!(out.len(), 2, "{out:?}");
        // One seat pressing twice in a pass still coalesces.
        let out = deduplicate_synthetic_events(vec![press(7), press(7)]);
        assert_eq!(out.len(), 1);
    }
}
