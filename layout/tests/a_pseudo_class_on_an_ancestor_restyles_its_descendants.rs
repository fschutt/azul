//! A pseudo-class on an ANCESTOR compound of a selector (`.btn:hover .ink`,
//! `.frame:focus-within .ring`) holds while that ancestor is in the state, so
//! the descendants it selects restyle when the ancestor's state changes - not
//! only when their own does.
//!
//! This is what lets a window control swap its glyph on hover in plain CSS:
//! the pointer over the button's padding hovers the button, not the glyph.

use azul_core::{
    id::NodeId,
    styled_dom::{FocusChange, HoverChange, StyledDom},
};
use azul_css::props::{
    basic::color::ColorU, layout::LayoutDisplay, style::StyleBackgroundContent,
};

fn styled(markup: &str) -> StyledDom {
    let parsed = azul_layout::xml::parse_xml(markup).expect("parses");
    StyledDom::create_from_dom(azul_layout::xml::dom_from_parsed_xml(parsed))
}

fn with_class(styled: &StyledDom, class: &str) -> NodeId {
    (0..styled.node_data.len())
        .map(NodeId::new)
        .find(|id| styled.node_data.as_container()[*id].has_class(class))
        .unwrap_or_else(|| panic!("no .{class} in the document"))
}

fn background(styled: &StyledDom, node: NodeId) -> Option<ColorU> {
    let state = styled.styled_nodes.as_container()[node].styled_node_state;
    azul_layout::solver3::getters::get_background_contents(styled, node, &state)
        .into_iter()
        .find_map(|bg| match bg {
            StyleBackgroundContent::Color(c) => Some(c),
            _ => None,
        })
}

fn display(styled: &StyledDom, node: NodeId) -> Option<LayoutDisplay> {
    azul_layout::solver3::getters::get_display_property(styled, Some(node)).exact()
}

const GREEN: ColorU = ColorU {
    r: 0,
    g: 255,
    b: 0,
    a: 255,
};
const RED: ColorU = ColorU {
    r: 255,
    g: 0,
    b: 0,
    a: 255,
};

fn hover(styled: &mut StyledDom, left: &[NodeId], entered: &[NodeId]) -> Vec<NodeId> {
    let result = styled.restyle_on_state_change(
        None,
        Some(HoverChange {
            left_nodes: left.to_vec(),
            entered_nodes: entered.to_vec(),
        }),
        None,
    );
    result.changed_nodes.keys().copied().collect()
}

#[test]
fn hovering_a_button_restyles_the_glyph_inside_it() {
    let mut styled = styled(
        r##"<style>
            .ink { background: #00ff00; width: 8px; height: 8px; }
            .btn:hover .ink { background: #ff0000; }
        </style>
        <div class="btn"><div class="pad"><div class="ink"></div></div></div>"##,
    );
    let (btn, ink) = (with_class(&styled, "btn"), with_class(&styled, "ink"));

    assert_eq!(background(&styled, ink), Some(GREEN), "resting");

    // The pointer is over the button's padding: the button is hovered, the
    // glyph is not.
    let changed = hover(&mut styled, &[], &[btn]);
    assert!(!styled.styled_nodes.as_container()[ink].styled_node_state.hover);
    assert_eq!(background(&styled, ink), Some(RED));
    assert!(
        changed.contains(&ink),
        "the glyph's style changed, so the restyle reports it: {changed:?}"
    );

    let changed = hover(&mut styled, &[btn], &[]);
    assert_eq!(background(&styled, ink), Some(GREEN), "resting again");
    assert!(changed.contains(&ink), "{changed:?}");
}

#[test]
fn a_window_control_swaps_its_glyph_on_hover_in_plain_css() {
    let mut styled = styled(
        r##"<style>
            .rest { display: block; width: 8px; height: 8px; }
            .lit { display: none; width: 8px; height: 8px; }
            .control:hover > .rest { display: none; }
            .control:hover > .lit { display: block; }
        </style>
        <div class="control"><div class="rest"></div><div class="lit"></div></div>"##,
    );
    let control = with_class(&styled, "control");
    let (rest, lit) = (with_class(&styled, "rest"), with_class(&styled, "lit"));

    assert_eq!(display(&styled, rest), Some(LayoutDisplay::Block));
    assert_eq!(display(&styled, lit), Some(LayoutDisplay::None));

    let result = styled.restyle_on_state_change(
        None,
        Some(HoverChange {
            left_nodes: Vec::new(),
            entered_nodes: vec![control],
        }),
        None,
    );
    assert_eq!(display(&styled, rest), Some(LayoutDisplay::None));
    assert_eq!(display(&styled, lit), Some(LayoutDisplay::Block));
    assert!(
        result.needs_layout,
        "a display change moves boxes, so the restyle asks for a layout"
    );

    let _ = hover(&mut styled, &[control], &[]);
    assert_eq!(display(&styled, rest), Some(LayoutDisplay::Block));
    assert_eq!(display(&styled, lit), Some(LayoutDisplay::None));
}

#[test]
fn a_focus_within_rule_on_a_frame_restyles_the_ring_inside_it() {
    let mut styled = styled(
        r##"<style>
            .ring { background: #00ff00; width: 10px; height: 10px; }
            .frame:focus-within .ring { background: #ff0000; }
        </style>
        <div class="frame"><div class="ring"></div><div class="field">f</div></div>"##,
    );
    let ring = with_class(&styled, "ring");
    let field = with_class(&styled, "field");

    assert_eq!(background(&styled, ring), Some(GREEN), "resting");
    let result = styled.restyle_on_state_change(
        Some(FocusChange {
            lost_focus: None,
            gained_focus: Some(field),
        }),
        None,
        None,
    );
    assert_eq!(
        background(&styled, ring),
        Some(RED),
        "the ring is not an ancestor of the field, it follows its frame"
    );
    assert!(
        result.changed_nodes.contains_key(&ring),
        "{:?}",
        result.changed_nodes.keys()
    );

    let _ = styled.restyle_on_state_change(
        Some(FocusChange {
            lost_focus: Some(field),
            gained_focus: None,
        }),
        None,
        None,
    );
    assert_eq!(background(&styled, ring), Some(GREEN), "resting again");
}

#[test]
fn a_hovered_element_restyles_the_sibling_after_it() {
    let mut styled = styled(
        r##"<style>
            .tip { background: #00ff00; width: 10px; height: 10px; }
            .anchor:hover + .tip { background: #ff0000; }
        </style>
        <div><div class="anchor">a</div><div class="tip"></div></div>"##,
    );
    let (anchor, tip) = (with_class(&styled, "anchor"), with_class(&styled, "tip"));

    let changed = hover(&mut styled, &[], &[anchor]);
    assert_eq!(background(&styled, tip), Some(RED));
    assert!(changed.contains(&tip), "{changed:?}");

    let _ = hover(&mut styled, &[anchor], &[]);
    assert_eq!(background(&styled, tip), Some(GREEN));
}

#[test]
fn a_rule_whose_ancestor_is_not_in_the_state_leaves_the_descendant_alone() {
    let mut styled = styled(
        r##"<style>
            .ink { background: #00ff00; width: 8px; height: 8px; }
            .btn:active .ink { background: #ff0000; }
        </style>
        <div class="btn"><div class="ink"></div></div>"##,
    );
    let (btn, ink) = (with_class(&styled, "btn"), with_class(&styled, "ink"));

    // Hovered is not pressed.
    let changed = hover(&mut styled, &[], &[btn]);
    assert_eq!(background(&styled, ink), Some(GREEN));
    assert!(!changed.contains(&ink), "{changed:?}");
}

/// A rebuilt DOM is cascaded at rest, then the window hands it its states
/// flag by flag (the hovered nodes, the focus and its ancestors). Its rules
/// that name a state left of their subject must then see those states.
#[test]
fn a_dom_given_its_states_after_the_cascade_catches_up_with_them() {
    let mut styled = styled(
        r##"<style>
            .ring { background: #00ff00; width: 10px; height: 10px; }
            .frame:focus-within .ring { background: #ff0000; }
        </style>
        <div class="frame"><div class="ring"></div><div class="field">f</div></div>"##,
    );
    let ring = with_class(&styled, "ring");
    let field = with_class(&styled, "field");
    {
        let mut nodes = styled.styled_nodes.as_container_mut();
        nodes[field].styled_node_state.focused = true;
    }
    styled.raise_focus_within(field);
    let frame = with_class(&styled, "frame");
    assert!(styled.styled_nodes.as_container()[frame].styled_node_state.focus_within);
    assert_eq!(background(&styled, ring), Some(GREEN), "not cascaded yet");
    assert!(styled.restyle_for_raised_states(), "a left-state rule is live");
    assert_eq!(background(&styled, ring), Some(RED));
    let _ = styled.restyle_for_raised_states();
    assert_eq!(background(&styled, ring), Some(RED), "running it again changes nothing");
}
