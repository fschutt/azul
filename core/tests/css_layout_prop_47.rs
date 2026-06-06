//! Ground-truth check for the #47 follow-up: do LAYOUT properties from inline
//! (`with_css`) css actually land on the owner node, the same way paint props do?
//!
//! The #47 fix push_front-s a `Root(range)` scope selector onto every inline rule.
//! Paint props (background) are known to apply + scope correctly (css_scope_47.rs).
//! This test isolates whether layout-hot props (display/width/height) also apply.

use azul_core::{dom::{Dom, NodeId}, styled_dom::StyledDom};

#[test]
fn inline_css_layout_props_apply_to_owner() {
    // body[0]
    //   div[1]  with_css("display:flex; width:200px; height:50px; background:red")
    let dom = Dom::create_body().with_child(
        Dom::create_div()
            .with_css("display: flex; width: 200px; height: 50px; background: red"),
    );
    let styled_dom = StyledDom::create_from_dom(dom);

    let cache = styled_dom.get_css_property_cache();
    let nd = styled_dom.node_data.as_container();
    let sn = styled_dom.styled_nodes.as_container();
    let id = NodeId::new(1);
    let st = &sn[id].styled_node_state;

    let display = cache.get_display(&nd[id], &id, st).cloned();
    let width = cache.get_width(&nd[id], &id, st).cloned();
    let height = cache.get_height(&nd[id], &id, st).cloned();
    let bg = cache.get_background_content(&nd[id], &id, st).cloned();

    eprintln!(
        "[layout-prop test] node1: display={:?} width={:?} height={:?} bg.is_some={}",
        display, width, height, bg.is_some()
    );

    // Paint prop is the control (known to work).
    assert!(bg.is_some(), "background should apply to owner (control)");
    // The actual question — layout props from the same inline rule:
    assert!(display.is_some(), "display:flex must apply to owner node");
    assert!(width.is_some(), "width:200px must apply to owner node");
    assert!(height.is_some(), "height:50px must apply to owner node");
}

/// The menu/component-CSS case: `add_component_css` with DESCENDANT class
/// selectors (`.menu-item`) on a container must style the container's children.
/// Under the owner-only `Root` scoping this fails (the items are not the owner),
/// which is why menu popups collapse to 0 width. Containment scoping fixes it.
#[test]
fn component_css_descendant_selectors_apply_in_subtree() {
    use azul_core::dom::{IdOrClass, IdOrClassVec};

    let item = Dom::create_div()
        .with_ids_and_classes(IdOrClassVec::from_vec(vec![IdOrClass::Class("menu-item".into())]));
    let mut container = Dom::create_div()
        .with_ids_and_classes(IdOrClassVec::from_vec(vec![IdOrClass::Class("menu-container".into())]))
        .with_child(item);
    let (css, _errs) = azul_css::parser2::new_from_str(
        ".menu-container { min-width: 160px } .menu-item { display: flex; padding: 8px }",
    );
    container.add_component_css(css);
    let dom = Dom::create_body().with_child(container);
    let styled_dom = StyledDom::create_from_dom(dom);

    // nodes: body[0], container[1], item[2]
    let cache = styled_dom.get_css_property_cache();
    let nd = styled_dom.node_data.as_container();
    let sn = styled_dom.styled_nodes.as_container();
    let c = NodeId::new(1);
    let it = NodeId::new(2);
    let container_minwidth = cache.get_min_width(&nd[c], &c, &sn[c].styled_node_state).cloned();
    let item_display = cache.get_display(&nd[it], &it, &sn[it].styled_node_state).cloned();

    eprintln!(
        "[component-css test] container.min_width={:?} item.display={:?}",
        container_minwidth, item_display
    );
    assert!(container_minwidth.is_some(), ".menu-container min-width must apply to the container (owner)");
    let item_is_flex = matches!(&item_display, Some(v) if format!("{:?}", v).contains("Flex"));
    assert!(
        item_is_flex,
        ".menu-item display:flex must apply to the DESCENDANT item (got {:?})",
        item_display
    );
}
