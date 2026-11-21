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
