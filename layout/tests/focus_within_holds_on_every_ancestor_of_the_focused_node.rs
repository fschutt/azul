//! `:focus-within` holds on an element when it or any of its descendants has
//! the focus (Selectors 4 s9.5). A field inside a token input or a tile inside
//! an icon grid takes the focus, and the frame around it draws the ring.
//!
//! The focus restyle raises the flag on the focused node's whole ancestor
//! chain and drops it from the chain the focus left, so a stylesheet rule
//! ending in `:focus-within` restyles the frame without a rebuild.

use azul_core::{
    id::NodeId,
    styled_dom::{FocusChange, StyledDom},
};
use azul_css::props::{basic::color::ColorU, style::StyleBackgroundContent};

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

fn focus(styled: &mut StyledDom, lost: Option<NodeId>, gained: Option<NodeId>) -> Vec<NodeId> {
    let result = styled.restyle_on_state_change(
        Some(FocusChange {
            lost_focus: lost,
            gained_focus: gained,
        }),
        None,
        None,
    );
    result.changed_nodes.keys().copied().collect()
}

const FRAME: &str = r##"<style>
        .frame { background: #00ff00; width: 100px; height: 20px; }
        .frame:focus-within { background: #ff0000; }
    </style>
    <div class="outer">
      <div class="frame a"><div class="middle"><div class="field-a">a</div></div></div>
      <div class="frame b"><div class="field-b">b</div></div>
    </div>"##;

#[test]
fn a_frame_takes_its_focus_within_style_while_a_field_inside_it_has_the_focus() {
    let mut styled = styled(FRAME);
    let frame = with_class(&styled, "a");
    let field = with_class(&styled, "field-a");

    assert_eq!(background(&styled, frame), Some(GREEN), "resting");

    let changed = focus(&mut styled, None, Some(field));
    let state = |s: &StyledDom, n: NodeId| s.styled_nodes.as_container()[n].styled_node_state;
    assert!(
        state(&styled, frame).focus_within,
        "the frame holds the focused field, so it is :focus-within"
    );
    assert!(
        state(&styled, with_class(&styled, "middle")).focus_within,
        "every ancestor between them too"
    );
    assert!(
        state(&styled, field).focus_within,
        "and the focused node itself (Selectors 4: the element or a descendant)"
    );
    assert!(
        changed.contains(&frame),
        "the frame's style changed, so the restyle reports it: {changed:?}"
    );
    assert_eq!(background(&styled, frame), Some(RED));

    let changed = focus(&mut styled, Some(field), None);
    assert!(!state(&styled, frame).focus_within, "the focus left");
    assert!(!state(&styled, field).focus_within);
    assert!(changed.contains(&frame), "{changed:?}");
    assert_eq!(background(&styled, frame), Some(GREEN), "resting again");
}

#[test]
fn moving_the_focus_between_two_frames_moves_the_style_and_keeps_the_common_ancestor() {
    let mut styled = styled(FRAME);
    let (a, b) = (with_class(&styled, "a"), with_class(&styled, "b"));
    let (field_a, field_b) = (
        with_class(&styled, "field-a"),
        with_class(&styled, "field-b"),
    );
    let outer = with_class(&styled, "outer");

    let _ = focus(&mut styled, None, Some(field_a));
    assert_eq!(background(&styled, a), Some(RED));
    assert_eq!(background(&styled, b), Some(GREEN));

    let _ = focus(&mut styled, Some(field_a), Some(field_b));
    let state = |s: &StyledDom, n: NodeId| s.styled_nodes.as_container()[n].styled_node_state;
    assert!(!state(&styled, a).focus_within, "the first frame lost it");
    assert!(state(&styled, b).focus_within, "the second frame took it");
    assert!(
        state(&styled, outer).focus_within,
        "the frames' common ancestor holds the focus all along"
    );
    assert_eq!(background(&styled, a), Some(GREEN));
    assert_eq!(background(&styled, b), Some(RED));
}
