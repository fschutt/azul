//! A6, input and hit-testing.

/// A6-08: a window event's callbacks are found through the styled DOM's list of window
/// listeners, not by a walk over every node of every DOM.
mod window_listeners {
    use azul_core::{
        callbacks::CoreCallback,
        dom::{Dom, EventFilter, HoverEventFilter, NodeId, WindowEventFilter},
        refany::{OptionRefAny, RefAny},
        styled_dom::StyledDom,
    };

    use crate::desktop::shell2::common::event::nodes_listening_to_window_events;

    fn callback() -> CoreCallback {
        CoreCallback {
            cb: 0usize,
            ctx: OptionRefAny::None,
        }
    }

    /// `body` with `n` children. Child `i` (node `i + 1`) has a hover handler when `i % 3 == 0`
    /// (a map's labels), and a window handler when `i` is in `window`.
    fn dom_of(n: usize, window: &[usize]) -> Dom {
        let children: Vec<Dom> = (0..n)
            .map(|i| {
                let mut child = Dom::create_div();
                if i % 3 == 0 {
                    child = child.with_callback(
                        EventFilter::Hover(HoverEventFilter::MouseEnter),
                        RefAny::new(i),
                        callback(),
                    );
                }
                if window.contains(&i) {
                    child = child.with_callback(
                        EventFilter::Window(WindowEventFilter::MouseMove),
                        RefAny::new(i),
                        callback(),
                    );
                }
                child
            })
            .collect();
        Dom::create_body().with_children(children.into())
    }

    /// What the walk over every node finds: the nodes with a window handler, in node order.
    fn walked(styled_dom: &StyledDom) -> Vec<NodeId> {
        let nodes = styled_dom.node_data.as_container();
        (0..nodes.len())
            .map(NodeId::new)
            .filter(|id| {
                nodes.get(*id).is_some_and(|node| {
                    node.get_callbacks()
                        .as_ref()
                        .iter()
                        .any(|cb| matches!(cb.event, EventFilter::Window(_)))
                })
            })
            .collect()
    }

    /// The list names the nodes the walk finds, and as many of them at 4 n nodes as at n: a
    /// window event looks at its listeners, not at the DOM.
    /// Mutation: `nodes_listening_to_window_events` answering every node (the walk) fails the
    /// count; answering an unsorted or doubled list fails the comparison.
    #[test]
    fn a_window_event_looks_at_the_window_listeners_only() {
        let mut visited = Vec::new();
        for n in [300_usize, 1200] {
            let styled = StyledDom::create_from_dom(dom_of(n, &[7, 250]));
            assert_eq!(styled.node_data.as_container().len(), n + 1, "harness");
            let listeners = nodes_listening_to_window_events(&styled);
            assert_eq!(listeners, walked(&styled), "n = {n}");
            assert_eq!(listeners, vec![NodeId::new(8), NodeId::new(251)], "n = {n}");
            visited.push(listeners.len());
        }
        assert_eq!(visited, vec![2, 2], "two listeners at n and at 4 n");
    }

    /// A DOM appended to another keeps its listeners in the list, renumbered: the list follows
    /// the nodes through `append_child`.
    #[test]
    fn appended_doms_keep_their_window_listeners() {
        let mut styled = StyledDom::create_from_dom(dom_of(10, &[2]));
        styled.append_child(StyledDom::create_from_dom(dom_of(5, &[0, 4])));
        let listeners = nodes_listening_to_window_events(&styled);
        assert_eq!(listeners, walked(&styled));
        assert_eq!(listeners.len(), 3);
    }

    /// A rebuild that keeps the DOM carries the fresh handlers onto it
    /// (`diff::carry_fresh_values`): it does so only for a DOM whose nodes have the handler
    /// EVENTS they had (`NodeField::Callbacks` is compared), and then the list still names the
    /// listeners. A DOM where another node listens to the window does not compare alike, so it is
    /// never carried onto the old list.
    /// Mutation: the events of the handlers taken out of the comparison (`reads_alike`)
    /// fails the second half - and would make the list miss a listener.
    #[test]
    fn a_kept_dom_has_the_listeners_it_had() {
        use azul_core::layout_inputs::NodeField;

        let mut kept = StyledDom::create_from_dom(dom_of(20, &[3, 11]));
        let fresh = StyledDom::create_from_dom(dom_of(20, &[3, 11]));
        azul_core::diff::carry_fresh_values(
            kept.node_data.as_mut(),
            fresh.node_data.as_ref(),
        );
        assert_eq!(nodes_listening_to_window_events(&kept), walked(&kept));

        let other = StyledDom::create_from_dom(dom_of(20, &[3, 12]));
        let (old, new) = (kept.node_data.as_container(), other.node_data.as_container());
        let alike = (0..old.len()).map(NodeId::new).all(|id| {
            NodeField::Callbacks.reads_alike(&old[id], &new[id])
        });
        assert!(
            !alike,
            "a node that starts to listen to the window is a change of the DOM, not a carry"
        );
    }
}

/// A6-15: a pointer position a window system reports in device pixels becomes the logical
/// position the frame draws on that pixel: divided by the frame's factor (`dpi / 96`:
/// `present_geometry::frame_dpi_factor`, A10-17 - ONE function for the frame and the pointer
/// since the merge), not by the three-decimal `WindowSize::get_hidpi_factor()`.
mod pointer_positions {
    use azul_layout::window::present_geometry::frame_dpi_factor as pointer_scale;

    const WINDOWS_RS: &str = include_str!("../shell2/windows/mod.rs");

    /// DPI 128 (a custom scale of 133 %, factor 4/3): device pixel (3000, 1500) is the logical
    /// position (2250, 1125), which the frame draws on pixel (3000, 1500). With the factor cut
    /// to 1.333 it was (2250.56, 1125.28), drawn on pixel (3000.75, 1500.38).
    /// Mutation: `pointer_scale` answering the cut factor (`trunc(dpi / 96 * 1000) / 1000`)
    /// fails the first comparison and the sweep (6,292 of its 40,970 pixels land on another
    /// pixel, the worst 2.98 px away).
    #[test]
    fn a_pointer_is_at_the_logical_position_its_pixel_is_drawn_at() {
        let scale = pointer_scale(128);
        assert_eq!(3000.0_f32 / scale, 2250.0);
        assert_eq!(1500.0_f32 / scale, 1125.0);
        // What it was: the same division by the three-decimal factor.
        assert!((3000.0_f32 / 1.333 - 2250.5627).abs() < 0.001);

        // Every pixel column of a 4096 px window, at 100 .. 300 % and at the custom scales a
        // three-decimal factor cannot hold (DPI 106, 110, 128): the position the pointer is
        // given is drawn on the pixel the pointer is on.
        for dpi in [96_u32, 106, 110, 120, 128, 144, 168, 192, 216, 288] {
            let scale = pointer_scale(dpi);
            for px in 0..=4096_u16 {
                let px = f32::from(px);
                let logical = px / scale;
                let drawn = logical * scale;
                assert!(
                    (drawn - px).abs() < 0.001,
                    "dpi {dpi}: pixel {px} is logical {logical}, which is drawn at {drawn}"
                );
            }
        }

        assert_eq!(pointer_scale(144), 1.5);
        assert_eq!(pointer_scale(0), 1.0, "a window that was never told its DPI");
    }

    /// No Win32 handler divides a pointer position by the three-decimal factor (the mouse
    /// messages, the wheel, `WM_POINTER`, `WM_GESTURE`, the OLE drag-over).
    /// Mutation: `get_hidpi_factor()` back in one of them fails this.
    #[test]
    fn no_win32_pointer_position_is_divided_by_the_three_decimal_factor() {
        for cut in [
            "/ hidpi_factor.inner.get()",
            "/ hf.inner.get()",
            "/ hidpi.inner.get()",
        ] {
            assert!(
                !WINDOWS_RS.contains(cut),
                "windows/mod.rs divides a position by the cut factor: `{cut}`"
            );
        }
        assert!(WINDOWS_RS.contains("present_geometry::frame_dpi_factor("));
    }
}

/// A6-11: what a press may start (a text selection drag) is worked out at the press edge,
/// not on every pass.
mod press_edge {
    const EVENT_RS: &str = include_str!("../shell2/common/event.rs");

    /// The hit nodes are collected, and their ancestors and children walked, only when the
    /// left button went down in this pass: a guard on the source, because the analysis sits
    /// in the middle of the event pass (`process_window_events_inner`) and its only reader is
    /// the `if left_down && !lw.prev_left_down` arm below it.
    /// Mutation: `press_edge &&` or `.filter(|_| press_edge)` taken out (the analysis on every
    /// pointer move again) fails this.
    #[test]
    fn the_press_analysis_runs_at_the_press_edge_only() {
        let from = EVENT_RS
            .find("let press_edge = left_down")
            .expect("the press edge is named");
        let analysis = &EVENT_RS[from..];
        let to = analysis
            .find("lw.prev_left_down = left_down;")
            .expect("the block ends where the edge is recorded");
        let analysis = &analysis[..to];
        assert!(analysis.contains(".is_some_and(|lw| !lw.prev_left_down)"));
        assert!(analysis.contains(".filter(|_| press_edge)"));
        assert!(analysis.contains("let press_on_editable = press_edge && press_targets"));
        assert!(
            analysis.contains("if left_down && !lw.prev_left_down {"),
            "the one reader of the answer is the press edge"
        );
    }
}

/// A6-12: macOS sends `-[NSCursor set]` only for a cursor that is not AppKit's current one.
mod macos_cursor {
    const MACOS_RS: &str = include_str!("../shell2/macos/mod.rs");

    /// A guard on the source (the call needs a window server): in `set_cursor`, AppKit's
    /// current cursor is compared and the function left before `set`, and the switch that
    /// brings the old behaviour back is there.
    /// Mutation: the comparison taken out (a `set` per pointer move again) fails this.
    #[test]
    fn the_cursor_is_set_only_when_appkit_has_another_one() {
        let from = MACOS_RS
            .find("pub fn set_cursor(&self, cursor_type: &str)")
            .expect("MacOSWindow::set_cursor");
        let body = &MACOS_RS[from..];
        let set = body.find("cursor.set();").expect("the cursor is set");
        let before_set = &body[..set];
        assert!(before_set.contains("NSCursor::currentCursor()"));
        assert!(before_set.contains("!cursor_is_set_on_every_call()"));
        assert!(before_set.contains("return;"));
        assert!(MACOS_RS.contains("\"AZ_CURSOR_SET_ALWAYS\""));
    }
}

/// A6-17: no hit test is answered by WebRender; macOS does not request its hit tester.
mod one_hit_tester {
    const EVENT_RS: &str = include_str!("../shell2/common/event.rs");
    const MACOS_RS: &str = include_str!("../shell2/macos/mod.rs");

    /// Guards on the source: the shared hit test asks the layout-based tester and never
    /// resolves WebRender's, and the macOS window is created without the blocking request
    /// (behind `AZ_WR_HIT_TESTER`).
    /// Mutation: a `hit_tester.resolve()` or `.hit_test(` on WebRender's tester in
    /// `perform_hit_test`, or the unconditional request back in the macOS shell, fails this.
    #[test]
    fn no_hit_test_is_answered_by_webrender() {
        let from = EVENT_RS.find("pub fn perform_hit_test(").expect("perform_hit_test");
        let body = &EVENT_RS[from..];
        let to = body.find("macro_rules! impl_platform_window_getters").expect("the end");
        let body = &body[..to];
        assert!(body.contains("cpu_ht.hit_test_scrolled_stamped("));
        assert!(!body.contains("self.hit_tester"), "WebRender's tester is not asked");
        assert!(!EVENT_RS.contains("hit_tester.resolve()"));
        assert!(!EVENT_RS.contains(".hit_tester.as_mut()"));

        let request = MACOS_RS.find(".request_hit_tester(").expect("the request is kept");
        let before = &MACOS_RS[..request];
        let switch = before.rfind("AZ_WR_HIT_TESTER").expect("behind the switch");
        assert!(request - switch < 400, "the request follows its switch");
        assert_eq!(MACOS_RS.matches(".request_hit_tester(").count(), 1);
    }
}
