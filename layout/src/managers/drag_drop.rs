//! **Node** drag-and-drop *view types* (`DragState` / `DragType`).
//!
//! There is no drag-drop *manager* any more. The single source of truth for an
//! active drag is [`crate::managers::gesture::GestureAndDragManager::active_drag`]
//! (an `azul_core::drag::DragContext`).
//!
//! The former `DragDropManager` held a SECOND `active_drag: Option<DragContext>`
//! — a clone frozen at `InitDragVisualState` that never saw the later
//! drop-target/position updates, and that nothing remapped on a DOM rebuild.
//! Two sources of truth for one drag is a bug by construction, and the mirror
//! was write-only in practice (every reader consulted `gesture_drag_manager`
//! first, and the mirror was only ever populated *from* it), so it has been
//! deleted (2026-07-13). What remains here is the stateless conversion into the
//! public `DragState` API, which is built on demand from the live `DragContext`.

use azul_core::dom::{DomNodeId, OptionDomNodeId};
use azul_core::drag::{ActiveDragType, DragContext};
use azul_css::{impl_option, impl_option_inner, OptionString};

/// Type of drag operation
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(C)]
pub enum DragType {
    /// Dragging a DOM node
    Node,
    /// Dragging a file from OS
    File,
}

/// State of an active drag operation
#[derive(Debug, Clone, PartialEq, Eq)]
#[repr(C)]
pub struct DragState {
    /// Type of drag
    pub drag_type: DragType,
    /// Source node (for node dragging)
    pub source_node: OptionDomNodeId,
    /// Current drop target (if hovering over valid drop zone)
    pub current_drop_target: OptionDomNodeId,
    /// File path (for file dragging)
    pub file_path: OptionString,
}

impl DragState {
    /// Create `DragState` from a `DragContext` (for backwards compatibility)
    #[must_use] pub fn from_context(ctx: &DragContext) -> Option<Self> {
        match &ctx.drag_type {
            ActiveDragType::Node(node_drag) => Some(Self {
                drag_type: DragType::Node,
                source_node: OptionDomNodeId::Some(DomNodeId {
                    dom: node_drag.dom_id,
                    node: azul_core::styled_dom::NodeHierarchyItemId::from_crate_internal(Some(node_drag.node_id)),
                }),
                current_drop_target: node_drag.current_drop_target,
                file_path: OptionString::None,
            }),
            ActiveDragType::FileDrop(file_drop) => Some(Self {
                drag_type: DragType::File,
                source_node: OptionDomNodeId::None,
                current_drop_target: file_drop.drop_target,
                file_path: file_drop.files.as_ref().first().cloned().into(),
            }),
            _ => None, // Other drag types don't map to the old API
        }
    }
}

impl_option!(
    DragState,
    OptionDragState,
    copy = false,
    [Debug, Clone, PartialEq, Eq]
);

#[cfg(test)]
mod autotest_generated {
    use azul_core::{
        dom::{DomId, NodeId},
        drag::{
            ActiveDragType, DragData, DragEffect, DropEffect, FileDropDrag, NodeDrag,
            ScrollbarAxis, WindowResizeDrag, WindowResizeEdge,
        },
        geom::LogicalPosition,
        styled_dom::NodeHierarchyItemId,
        window::WindowPosition,
    };
    use azul_css::AzString;

    use super::*;

    // ---------------------------------------------------------------------
    // helpers
    // ---------------------------------------------------------------------

    fn s(text: &str) -> AzString {
        AzString::from(String::from(text))
    }

    fn node_ctx(dom: usize, node: usize) -> DragContext {
        DragContext::node_drag(
            DomId { inner: dom },
            NodeId::new(node),
            LogicalPosition::new(1.0, 2.0),
            DragData::new(),
            7,
        )
    }

    /// Mutable access to the `NodeDrag` inside a context built by [`node_ctx`].
    fn node_drag_of(ctx: &mut DragContext) -> &mut NodeDrag {
        match &mut ctx.drag_type {
            ActiveDragType::Node(n) => n,
            _ => unreachable!("node_ctx always builds ActiveDragType::Node"),
        }
    }

    fn file_ctx(files: &[&str]) -> DragContext {
        DragContext::file_drop(
            files.iter().copied().map(s).collect(),
            LogicalPosition::new(3.0, 4.0),
            1,
        )
    }

    fn file_drop_of(ctx: &mut DragContext) -> &mut FileDropDrag {
        match &mut ctx.drag_type {
            ActiveDragType::FileDrop(f) => f,
            _ => unreachable!("file_ctx always builds ActiveDragType::FileDrop"),
        }
    }

    fn dom_node(dom: usize, node: usize) -> DomNodeId {
        DomNodeId {
            dom: DomId { inner: dom },
            node: NodeHierarchyItemId::from_crate_internal(Some(NodeId::new(node))),
        }
    }

    /// The `NodeId` the produced `DragState` points at, decoded back out of the
    /// 1-based `NodeHierarchyItemId` encoding.
    fn source_node_id(state: &DragState) -> Option<NodeId> {
        state
            .source_node
            .as_option()
            .and_then(|d| d.node.into_crate_internal())
    }

    fn file_path_str(state: &DragState) -> Option<&str> {
        state.file_path.as_option().map(AzString::as_str)
    }

    // ---------------------------------------------------------------------
    // node drags: field mapping + id-encoding boundaries
    // ---------------------------------------------------------------------

    #[test]
    fn node_drag_maps_every_field() {
        let mut ctx = node_ctx(3, 42);
        node_drag_of(&mut ctx).current_drop_target = OptionDomNodeId::Some(dom_node(3, 9));

        let state = DragState::from_context(&ctx).expect("node drag must map to DragState");

        assert_eq!(state.drag_type, DragType::Node);
        assert_eq!(state.source_node.as_option().map(|d| d.dom.inner), Some(3));
        assert_eq!(source_node_id(&state), Some(NodeId::new(42)));
        assert_eq!(
            state.current_drop_target,
            OptionDomNodeId::Some(dom_node(3, 9))
        );
        // A node drag carries no file, ever.
        assert!(state.file_path.is_none());
    }

    /// Node index 0 is a *real* node, not "absent". The 1-based encoding used by
    /// `NodeHierarchyItemId` exists precisely so that these two cannot collide —
    /// if `from_context` ever stored the raw index, node 0 would decode as `None`.
    #[test]
    fn node_id_zero_is_not_encoded_as_none() {
        let ctx = node_ctx(0, 0);
        let state = DragState::from_context(&ctx).expect("node 0 is a valid drag source");

        let source = state.source_node.as_option().expect("source must be Some");
        assert_ne!(source.node, NodeHierarchyItemId::NONE);
        assert_eq!(source.node.into_raw(), 1, "0-based 0 encodes to 1-based 1");
        assert_eq!(source_node_id(&state), Some(NodeId::new(0)));
    }

    /// The largest node index that the 1-based encoding can represent
    /// (`usize::MAX - 1` → raw `usize::MAX`). Must round-trip exactly, with no
    /// wrap to a small (aliasing) index.
    #[test]
    fn node_id_max_encodable_round_trips() {
        let max = usize::MAX - 1;
        let ctx = node_ctx(usize::MAX, max);
        let state = DragState::from_context(&ctx).expect("extreme ids still map");

        let source = state.source_node.as_option().expect("source must be Some");
        assert_eq!(source.dom.inner, usize::MAX);
        assert_eq!(source.node.into_raw(), usize::MAX);
        assert_eq!(source_node_id(&state), Some(NodeId::new(max)));
    }

    /// `from_context` must read `current_drop_target`, never `previous_drop_target`
    /// (the latter only exists to synthesize DragEnter/DragLeave events).
    #[test]
    fn node_drag_uses_current_not_previous_drop_target() {
        let mut ctx = node_ctx(1, 5);
        {
            let drag = node_drag_of(&mut ctx);
            drag.previous_drop_target = OptionDomNodeId::Some(dom_node(1, 100));
            drag.current_drop_target = OptionDomNodeId::Some(dom_node(1, 200));
        }

        let state = DragState::from_context(&ctx).unwrap();
        assert_eq!(
            state.current_drop_target,
            OptionDomNodeId::Some(dom_node(1, 200))
        );
    }

    #[test]
    fn node_drag_without_drop_target_stays_none() {
        let ctx = node_ctx(1, 5);
        let state = DragState::from_context(&ctx).unwrap();
        assert!(state.current_drop_target.is_none());
    }

    /// A drop target living in a *different* DOM than the source must be carried
    /// through verbatim — `from_context` must not "helpfully" rewrite the dom id.
    #[test]
    fn node_drag_cross_dom_drop_target_is_not_rewritten() {
        let mut ctx = node_ctx(1, 5);
        node_drag_of(&mut ctx).current_drop_target = OptionDomNodeId::Some(dom_node(9, 5));

        let state = DragState::from_context(&ctx).unwrap();
        assert_eq!(state.source_node.as_option().map(|d| d.dom.inner), Some(1));
        assert_eq!(
            state.current_drop_target.as_option().map(|d| d.dom.inner),
            Some(9)
        );
    }

    /// Non-finite / extreme drag coordinates are not part of `DragState`, so they
    /// must neither panic nor leak into the conversion.
    #[test]
    fn node_drag_with_nan_and_infinite_positions_does_not_panic() {
        for pos in [
            LogicalPosition::new(f32::NAN, f32::NAN),
            LogicalPosition::new(f32::INFINITY, f32::NEG_INFINITY),
            LogicalPosition::new(f32::MAX, f32::MIN),
            LogicalPosition::new(-0.0, f32::MIN_POSITIVE),
        ] {
            let mut ctx = DragContext::node_drag(
                DomId::ROOT_ID,
                NodeId::new(1),
                pos,
                DragData::new(),
                0,
            );
            {
                let drag = node_drag_of(&mut ctx);
                drag.current_position = pos;
                drag.drag_offset = pos;
            }

            let state = DragState::from_context(&ctx).expect("positions never gate the mapping");
            assert_eq!(state.drag_type, DragType::Node);
            assert_eq!(source_node_id(&state), Some(NodeId::new(1)));
        }
    }

    /// Even when the payload *looks* like a file (a `text/uri-list` MIME entry),
    /// a node drag must not populate `file_path` — that field is FileDrop-only.
    #[test]
    fn node_drag_with_file_like_payload_still_has_no_file_path() {
        let mut data = DragData::new();
        data.set_data("text/uri-list", b"file:///etc/passwd".to_vec());
        data.set_text("/etc/passwd");
        data.effect_allowed = DragEffect::All;

        let ctx = DragContext::node_drag(
            DomId::ROOT_ID,
            NodeId::new(2),
            LogicalPosition::zero(),
            data,
            0,
        );

        let state = DragState::from_context(&ctx).unwrap();
        assert_eq!(state.drag_type, DragType::Node);
        assert!(
            state.file_path.is_none(),
            "file_path must stay None for node drags regardless of payload"
        );
    }

    /// `drop_accepted` / `drop_effect` have no representation in the old API and
    /// must not change whether (or how) the drag maps.
    #[test]
    fn node_drag_drop_effect_flags_do_not_affect_mapping() {
        let baseline = DragState::from_context(&node_ctx(1, 5)).unwrap();

        for (accepted, effect) in [
            (true, DropEffect::Move),
            (true, DropEffect::Copy),
            (false, DropEffect::Link),
            (false, DropEffect::None),
        ] {
            let mut ctx = node_ctx(1, 5);
            {
                let drag = node_drag_of(&mut ctx);
                drag.drop_accepted = accepted;
                drag.drop_effect = effect;
            }
            assert_eq!(DragState::from_context(&ctx).unwrap(), baseline);
        }
    }

    // ---------------------------------------------------------------------
    // file drops: path handling
    // ---------------------------------------------------------------------

    #[test]
    fn file_drop_maps_first_path_and_has_no_source_node() {
        let ctx = file_ctx(&["/tmp/a.txt"]);
        let state = DragState::from_context(&ctx).expect("file drop must map");

        assert_eq!(state.drag_type, DragType::File);
        assert!(
            state.source_node.is_none(),
            "a file drop has no source DOM node"
        );
        assert_eq!(file_path_str(&state), Some("/tmp/a.txt"));
    }

    /// Multi-file drops are lossy in the old API: only the *first* path survives.
    /// Pin that down so a future "take the last one" regression is caught.
    #[test]
    fn file_drop_takes_the_first_path_not_the_last() {
        let ctx = file_ctx(&["/first", "/second", "/third"]);
        let state = DragState::from_context(&ctx).unwrap();
        assert_eq!(file_path_str(&state), Some("/first"));
    }

    #[test]
    fn file_drop_with_empty_file_list_yields_none_path() {
        let ctx = file_ctx(&[]);
        let state = DragState::from_context(&ctx).expect("an empty file drop still maps");

        assert_eq!(state.drag_type, DragType::File);
        assert!(state.source_node.is_none());
        assert!(
            state.file_path.is_none(),
            "no files => no path (must not panic on first())"
        );
    }

    /// An empty *string* is a present-but-empty path, which is a different thing
    /// from "no file at all". `Option::first().cloned()` must not collapse them.
    #[test]
    fn file_drop_empty_string_path_is_some_not_none() {
        let ctx = file_ctx(&["", "/ignored"]);
        let state = DragState::from_context(&ctx).unwrap();

        assert!(state.file_path.is_some());
        assert_eq!(file_path_str(&state), Some(""));
    }

    /// Paths are opaque bytes to azul: emoji, RTL overrides, combining marks,
    /// newlines and embedded NULs must survive byte-exactly, not be sanitized
    /// or truncated at the first NUL (a classic C-string bug at this boundary).
    #[test]
    fn file_drop_unicode_and_control_characters_round_trip() {
        for path in [
            "/tmp/\u{1F600}\u{1F3F4}\u{E0067}.png",
            "/tmp/\u{202E}gnp.exe",
            "/tmp/e\u{0301}\u{0327}\u{0308}.txt",
            "/tmp/\u{4F60}\u{597D}/\u{043C}\u{0438}\u{0440}.txt",
            "/tmp/line\nbreak\ttab.txt",
            "/tmp/nul\u{0000}after.txt",
            "\u{FEFF}/tmp/bom.txt",
        ] {
            let ctx = file_ctx(&[path]);
            let state = DragState::from_context(&ctx).unwrap();

            assert_eq!(
                file_path_str(&state),
                Some(path),
                "path must round-trip byte-exactly"
            );
            assert_eq!(
                file_path_str(&state).unwrap().len(),
                path.len(),
                "no truncation (e.g. at an embedded NUL)"
            );
        }
    }

    #[test]
    fn file_drop_with_huge_path_round_trips() {
        let huge = format!("/tmp/{}.txt", "x".repeat(64 * 1024));
        let ctx = file_ctx(&[huge.as_str()]);
        let state = DragState::from_context(&ctx).unwrap();

        assert_eq!(file_path_str(&state), Some(huge.as_str()));
    }

    #[test]
    fn file_drop_with_many_files_still_returns_the_first() {
        let paths: Vec<String> = (0..10_000).map(|i| format!("/tmp/f{i}")).collect();
        let ctx = DragContext::file_drop(
            paths.iter().map(|p| s(p)).collect(),
            LogicalPosition::zero(),
            0,
        );

        let state = DragState::from_context(&ctx).unwrap();
        assert_eq!(file_path_str(&state), Some("/tmp/f0"));
    }

    #[test]
    fn file_drop_drop_target_passes_through() {
        let mut ctx = file_ctx(&["/tmp/a"]);
        file_drop_of(&mut ctx).drop_target = OptionDomNodeId::Some(dom_node(2, 77));

        let state = DragState::from_context(&ctx).unwrap();
        assert_eq!(
            state.current_drop_target,
            OptionDomNodeId::Some(dom_node(2, 77))
        );
        // ...and the source node is still None: a file has no source node.
        assert!(state.source_node.is_none());
    }

    #[test]
    fn file_drop_with_nan_position_does_not_panic() {
        let ctx = DragContext::file_drop(
            vec![s("/tmp/a")],
            LogicalPosition::new(f32::NAN, f32::INFINITY),
            u64::MAX,
        );

        let state = DragState::from_context(&ctx).unwrap();
        assert_eq!(state.drag_type, DragType::File);
        assert_eq!(file_path_str(&state), Some("/tmp/a"));
    }

    // ---------------------------------------------------------------------
    // drag types that deliberately do NOT map to the old API
    // ---------------------------------------------------------------------

    #[test]
    fn text_selection_drag_maps_to_none() {
        let ctx = DragContext::text_selection(
            DomId::ROOT_ID,
            NodeId::new(4),
            LogicalPosition::new(10.0, 10.0),
            1,
        );
        assert!(DragState::from_context(&ctx).is_none());
    }

    /// Degenerate scrollbar geometry (zero track, NaN content length) must still
    /// take the `None` arm rather than dividing / panicking anywhere.
    #[test]
    fn scrollbar_thumb_drag_maps_to_none_even_with_degenerate_metrics() {
        for (track, content, viewport, offset) in [
            (0.0_f32, 0.0_f32, 0.0_f32, 0.0_f32),
            (f32::NAN, f32::NAN, f32::NAN, f32::NAN),
            (f32::INFINITY, f32::NEG_INFINITY, f32::MAX, f32::MIN),
            (-1.0, -1.0, -1.0, -1.0),
        ] {
            for axis in [ScrollbarAxis::Vertical, ScrollbarAxis::Horizontal] {
                let ctx = DragContext::scrollbar_thumb(
                    DomId::ROOT_ID,
                    NodeId::new(0),
                    axis,
                    LogicalPosition::zero(),
                    offset,
                    track,
                    content,
                    viewport,
                    0,
                );
                assert!(DragState::from_context(&ctx).is_none());
            }
        }
    }

    #[test]
    fn window_move_drag_maps_to_none() {
        let ctx = DragContext::window_move(
            LogicalPosition::zero(),
            WindowPosition::Uninitialized,
            0,
        );
        assert!(DragState::from_context(&ctx).is_none());
    }

    #[test]
    fn window_resize_drag_maps_to_none_for_every_edge() {
        for edge in [
            WindowResizeEdge::Top,
            WindowResizeEdge::Bottom,
            WindowResizeEdge::Left,
            WindowResizeEdge::Right,
            WindowResizeEdge::TopLeft,
            WindowResizeEdge::TopRight,
            WindowResizeEdge::BottomLeft,
            WindowResizeEdge::BottomRight,
        ] {
            let ctx = DragContext::new(
                ActiveDragType::WindowResize(WindowResizeDrag {
                    edge,
                    start_position: LogicalPosition::zero(),
                    current_position: LogicalPosition::new(f32::NAN, 0.0),
                    initial_width: u32::MAX,
                    initial_height: 0,
                }),
                u64::MAX,
            );
            assert!(DragState::from_context(&ctx).is_none());
        }
    }

    // ---------------------------------------------------------------------
    // conversion invariants
    // ---------------------------------------------------------------------

    /// `from_context` takes `&DragContext`: it must be a pure read. Converting
    /// twice must yield equal states and leave the context untouched.
    #[test]
    fn from_context_is_pure_and_deterministic() {
        for ctx in [node_ctx(1, 5), file_ctx(&["/tmp/a", "/tmp/b"])] {
            let before = ctx.clone();

            let first = DragState::from_context(&ctx);
            let second = DragState::from_context(&ctx);

            assert_eq!(first, second);
            assert!(ctx == before, "from_context must not mutate the context");
        }
    }

    /// Neither the session id nor the cancelled flag is representable in the old
    /// API — a cancelled drag still converts. Pinned as *current* behaviour: any
    /// caller that wants "no drag after Escape" must check `ctx.cancelled` itself.
    #[test]
    fn cancelled_flag_and_session_id_do_not_change_the_mapping() {
        let mut ctx = node_ctx(1, 5);
        let baseline = DragState::from_context(&ctx).unwrap();

        ctx.cancelled = true;
        ctx.session_id = u64::MAX;

        let cancelled = DragState::from_context(&ctx)
            .expect("cancelled drags still convert (DragState has no cancel bit)");
        assert_eq!(cancelled, baseline);
    }

    /// Distinct sources must produce distinct states — i.e. the mapping is not
    /// collapsing ids somewhere (which the 1-based encoding makes easy to get
    /// wrong at 0 / 1).
    #[test]
    fn different_node_ids_produce_different_states() {
        let a = DragState::from_context(&node_ctx(0, 0)).unwrap();
        let b = DragState::from_context(&node_ctx(0, 1)).unwrap();
        let c = DragState::from_context(&node_ctx(1, 0)).unwrap();

        assert_ne!(a, b);
        assert_ne!(a, c);
        assert_ne!(b, c);
    }

    /// Node drags and file drops must never compare equal, even when both point
    /// at the same drop target.
    #[test]
    fn node_and_file_states_never_collide() {
        let mut node = node_ctx(0, 0);
        node_drag_of(&mut node).current_drop_target = OptionDomNodeId::Some(dom_node(0, 3));
        let mut file = file_ctx(&[]);
        file_drop_of(&mut file).drop_target = OptionDomNodeId::Some(dom_node(0, 3));

        let node_state = DragState::from_context(&node).unwrap();
        let file_state = DragState::from_context(&file).unwrap();

        assert_ne!(node_state, file_state);
        assert_ne!(node_state.drag_type, file_state.drag_type);
        assert_eq!(node_state.current_drop_target, file_state.current_drop_target);
    }

    /// A `DragState` is exactly its four fields: cloning is value-identical and
    /// no field aliases another (a clone must not share the `file_path` buffer in
    /// a way that shows up as inequality after drop).
    #[test]
    fn drag_state_clone_is_equal_and_independent() {
        let ctx = file_ctx(&["/tmp/\u{1F600}.png"]);
        let state = DragState::from_context(&ctx).unwrap();

        let cloned = state.clone();
        drop(state);

        assert_eq!(file_path_str(&cloned), Some("/tmp/\u{1F600}.png"));
        assert_eq!(cloned.drag_type, DragType::File);
    }

    /// `OptionDragState` round-trips through `Option<DragState>` in both
    /// directions without changing the payload.
    #[test]
    fn option_drag_state_round_trips() {
        let state = DragState::from_context(&node_ctx(2, 8)).unwrap();

        let wrapped: OptionDragState = Some(state.clone()).into();
        assert!(wrapped.is_some());
        let unwrapped: Option<DragState> = wrapped.into();
        assert_eq!(unwrapped, Some(state));

        let empty: OptionDragState = None.into();
        assert!(empty.is_none());
        let none_back: Option<DragState> = empty.into();
        assert_eq!(none_back, None);
        assert_eq!(OptionDragState::default(), OptionDragState::None);
    }
}
