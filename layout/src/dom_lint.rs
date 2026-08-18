//! Post-layout developer warnings for misused raw text nodes.
//!
//! Browsers wrap a raw text run in an ANONYMOUS BLOCK whenever it needs one;
//! azul does not. A bare `NodeType::Text` therefore has no box of its own —
//! no rect, no clip, no layout constraints — and every box-model CSS
//! property, callback, `tab_index` or `dataset` attached to one is silently
//! inert. That silence has shipped real bugs (text escaping its widget,
//! click targets that never fire), which is why the raw constructor is named
//! `create_text_do_not_use_without_block_level_wrapper` and why this pass
//! exists: after layout, every text node in a shape azul cannot honor is
//! reported to the developer, once per unique finding.
//!
//! The checks are structural (DOM + computed display), deliberately not
//! geometric: they fire deterministically on the first layout of a DOM,
//! before any symptom is visible on screen.

use std::collections::BTreeSet;
use std::sync::Mutex;

use azul_core::{dom::NodeType, id::NodeId, styled_dom::StyledDom};
use azul_css::props::layout::LayoutDisplay;

use crate::solver3::getters::{get_display_property, MultiValue};

/// One warning per unique message, process-wide. Layouts re-run constantly
/// (every DOM refresh); a warning that repeats 60 times a second is a warning
/// nobody reads.
static EMITTED: Mutex<BTreeSet<u64>> = Mutex::new(BTreeSet::new());

/// The suppression tag for this lint, honored from the `AZ_SUPPRESS`
/// environment variable (comma-separated list; the common misspelling
/// `AZ_SUPRESS` is accepted too). Every emitted warning names it.
pub const SUPPRESS_TAG: &str = "bare_text";

/// `AZ_SUPPRESS=bare_text` (checked once): the developer has read the
/// warnings and wants them off — e.g. a codebase that deliberately renders
/// raw text and accepts the differences from browser behavior.
fn is_suppressed() -> bool {
    static SUPPRESSED: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *SUPPRESSED.get_or_init(|| {
        let v = std::env::var("AZ_SUPPRESS")
            .or_else(|_| std::env::var("AZ_SUPRESS"))
            .unwrap_or_default();
        v.split(',').any(|t| t.trim().eq_ignore_ascii_case(SUPPRESS_TAG))
    })
}

/// Walk `styled_dom` and return one message per text node that is used in a
/// shape azul cannot honor. Pure — the caller decides how to report.
#[must_use]
pub fn collect_text_placement_warnings(styled_dom: &StyledDom) -> Vec<String> {
    let node_data = styled_dom.node_data.as_container();
    let hierarchy = styled_dom.node_hierarchy.as_container();
    let mut out = Vec::new();

    for idx in 0..node_data.len() {
        let node_id = NodeId::new(idx);
        let data = &node_data[node_id];
        let NodeType::Text(text) = data.get_node_type() else {
            continue;
        };
        let snippet = snippet_of(text.as_str());

        // W1 — state on a box-less node: every one of these is inert on a
        // text node, because only the wrapping block box carries a rect.
        let mut inert = Vec::new();
        if !data.get_style().rules.as_ref().is_empty() {
            inert.push("css properties");
        }
        if !data.get_callbacks().as_ref().is_empty() {
            inert.push("callbacks");
        }
        if data.get_tab_index().is_some() {
            inert.push("a tab_index");
        }
        if data.get_dataset().is_some() {
            inert.push("a dataset");
        }
        let Some(h) = hierarchy.get(node_id) else {
            continue;
        };
        if h.first_child_id(node_id).is_some() {
            inert.push("element children");
        }
        if !inert.is_empty() {
            out.push(format!(
                "text node {idx} ({snippet}) carries {} — INERT: a text node has no box. \
                 Move them onto a block wrapper (create_p_with_text / create_div_with_text) \
                 instead of the raw text node.",
                inert.join(" + "),
            ));
        }

        // W2 — no containing block the text can live in. Browsers would
        // generate an anonymous block here; azul does not.
        let Some(parent_id) = h.parent_id() else {
            out.push(format!(
                "text node {idx} ({snippet}) has no parent — a raw text run needs a \
                 block-level container (p / div / ...) to own its box.",
            ));
            continue;
        };
        let parent_type = node_data[parent_id].get_node_type();
        if matches!(parent_type, NodeType::Text(_)) {
            out.push(format!(
                "text node {idx} ({snippet}) is the child of another text node — \
                 wrap both in a block-level container (p / div / ...).",
            ));
            continue;
        }
        let parent_display = match get_display_property(styled_dom, Some(parent_id)) {
            MultiValue::Exact(d) => d,
            _ => LayoutDisplay::Block,
        };
        if matches!(
            parent_display,
            LayoutDisplay::Flex
                | LayoutDisplay::InlineFlex
                | LayoutDisplay::Grid
                | LayoutDisplay::InlineGrid
        ) {
            // A text leaf as the SOLE child of a flex/grid box is the
            // sanctioned wrapper pattern (the parent is the box that carries
            // the styling — badge, the converted labels). The hazard is text
            // as ONE OF SEVERAL items: it competes in item layout with no
            // box of its own.
            let mut child_count = 0usize;
            let mut c = hierarchy.get(parent_id).and_then(|p| p.first_child_id(parent_id));
            while let Some(cc) = c {
                child_count += 1;
                c = hierarchy.get(cc).and_then(|s| s.next_sibling_id());
            }
            if child_count > 1 {
                out.push(format!(
                    "text node {idx} ({snippet}) is one of {child_count} items in a \
                     {parent_display:?} container — a raw text run competes in flex/grid \
                     layout with no box of its own (browsers auto-wrap it in an anonymous \
                     block; azul does not). Wrap it: create_p_with_text / \
                     create_div_with_text.",
                ));
            }
            continue;
        }

        // Mixed inline + block content under one parent: the text has no
        // dedicated line box of its own next to block siblings.
        let mut sibling = hierarchy
            .get(parent_id)
            .and_then(|p| p.first_child_id(parent_id));
        let mut has_block_sibling = false;
        while let Some(sib) = sibling {
            if sib != node_id && !matches!(node_data[sib].get_node_type(), NodeType::Text(_)) {
                let d = match get_display_property(styled_dom, Some(sib)) {
                    MultiValue::Exact(d) => d,
                    _ => LayoutDisplay::Block,
                };
                if !matches!(
                    d,
                    LayoutDisplay::Inline
                        | LayoutDisplay::InlineBlock
                        | LayoutDisplay::InlineFlex
                        | LayoutDisplay::InlineGrid
                        | LayoutDisplay::InlineTable
                        | LayoutDisplay::None
                ) {
                    has_block_sibling = true;
                    break;
                }
            }
            sibling = hierarchy.get(sib).and_then(|s| s.next_sibling_id());
        }
        if has_block_sibling {
            out.push(format!(
                "text node {idx} ({snippet}) sits NEXT TO block-level siblings — browsers \
                 would wrap it in an anonymous block, azul does not, so it has no line box \
                 of its own. Wrap it: create_p_with_text / create_div_with_text.",
            ));
        }
    }

    out
}

/// Report every finding from [`collect_text_placement_warnings`] to stderr,
/// once per unique message per process. Call after layout.
pub fn warn_text_without_block_container(styled_dom: &StyledDom) {
    if is_suppressed() {
        return;
    }
    let warnings = collect_text_placement_warnings(styled_dom);
    if warnings.is_empty() {
        return;
    }
    let Ok(mut emitted) = EMITTED.lock() else {
        return;
    };
    for w in warnings {
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        std::hash::Hash::hash(&w, &mut hasher);
        if emitted.insert(std::hash::Hasher::finish(&hasher)) {
            eprintln!(
                "[azul][text-without-block] WARNING: {w} \
                 (suppress with AZ_SUPPRESS={SUPPRESS_TAG})"
            );
        }
    }
}

fn snippet_of(text: &str) -> String {
    let mut s: String = text.chars().take(24).collect();
    if text.chars().count() > 24 {
        s.push_str("...");
    }
    format!("{s:?}")
}

#[cfg(test)]
mod autotest_generated {
    use azul_core::dom::{Dom, TabIndex};
    use azul_core::styled_dom::StyledDom;
    use azul_css::css::Css;

    use super::collect_text_placement_warnings;

    fn styled(mut dom: Dom, css: &str) -> StyledDom {
        let css = if css.is_empty() {
            Css::empty()
        } else {
            Css::from_string(css.into())
        };
        StyledDom::create(&mut dom, css)
    }

    fn raw_text(s: &str) -> Dom {
        Dom::create_text_do_not_use_without_block_level_wrapper(s)
    }

    #[test]
    fn a_correctly_wrapped_text_produces_no_warning() {
        let sd = styled(
            Dom::create_body().with_child(Dom::create_p_with_text("hello")),
            "",
        );
        assert_eq!(collect_text_placement_warnings(&sd), Vec::<String>::new());
    }

    #[test]
    fn text_inside_an_inline_span_inside_a_block_is_fine() {
        let sd = styled(
            Dom::create_body()
                .with_child(Dom::create_p().with_child(Dom::create_span_with_text("hi"))),
            "",
        );
        assert_eq!(collect_text_placement_warnings(&sd), Vec::<String>::new());
    }

    #[test]
    fn state_on_a_text_node_is_reported_as_inert() {
        let text = raw_text("styled").with_tab_index(TabIndex::Auto);
        let sd = styled(Dom::create_body().with_child(Dom::create_p().with_child(text)), "");
        let w = collect_text_placement_warnings(&sd);
        assert_eq!(w.len(), 1, "{w:?}");
        assert!(w[0].contains("INERT"), "{w:?}");
        assert!(w[0].contains("tab_index"), "{w:?}");
    }

    #[test]
    fn a_sole_text_leaf_in_a_flex_wrapper_is_the_sanctioned_pattern() {
        // badge / the converted labels: the flex box IS the wrapper.
        let sd = styled(
            Dom::create_body().with_child(Dom::create_div().with_child(raw_text("flexed"))),
            "div { display: flex; }",
        );
        assert_eq!(collect_text_placement_warnings(&sd), Vec::<String>::new());
    }

    #[test]
    fn text_competing_with_other_flex_items_is_reported() {
        // The tree_view/radio_group shape: a raw label beside element items.
        let sd = styled(
            Dom::create_body().with_child(
                Dom::create_div()
                    .with_child(Dom::create_div())
                    .with_child(raw_text("flexed")),
            ),
            "body > div { display: flex; }",
        );
        let w = collect_text_placement_warnings(&sd);
        assert_eq!(w.len(), 1, "{w:?}");
        assert!(w[0].contains("competes in flex/grid"), "{w:?}");
    }

    #[test]
    fn text_next_to_a_block_sibling_is_reported() {
        // The audited frame.rs shape: a title wedged between two divs.
        let sd = styled(
            Dom::create_body().with_child(
                Dom::create_div()
                    .with_child(Dom::create_div())
                    .with_child(raw_text("title"))
                    .with_child(Dom::create_div()),
            ),
            "",
        );
        let w = collect_text_placement_warnings(&sd);
        assert_eq!(w.len(), 1, "{w:?}");
        assert!(w[0].contains("block-level siblings"), "{w:?}");
    }

    #[test]
    fn every_widget_dom_is_warning_free() {
        // The runtime twin of the widgets' label-convention test: none of the
        // shipped widgets may trip the developer warning.
        for (name, dom) in crate::widgets::all_widget_doms_for_lint() {
            let sd = styled(Dom::create_body().with_child(dom), "");
            let w = collect_text_placement_warnings(&sd);
            assert_eq!(w, Vec::<String>::new(), "widget {name} trips the text lint: {w:?}");
        }
    }
}
