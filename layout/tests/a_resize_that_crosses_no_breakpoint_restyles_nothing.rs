//! A resize that crosses no breakpoint restyles nothing.
//!
//! Every relayout hands the DOM the window's dynamic-selector context, and
//! a resize changes its viewport size. A DOM whose stylesheet has any
//! conditional rule (an `@theme` block, an `@media` query) re-ran the whole
//! author cascade and the compact cache for every new size, though no
//! condition's answer changed: half of each resize step of AzDrive's window
//! (sample: `set_dynamic_selector_context` -> `restyle`, 50 % of the
//! relayout), and again for the folder view's rows, whose box the resize
//! moves. Only a size that crosses one of the DOM's viewport thresholds (or
//! changes its shape, for an aspect-ratio / orientation / container query)
//! can change what the cascade selects.
//!
//! The count is `compact_cache_rebuilds_on_this_thread`, not a time.

use azul_core::{
    dom::{Dom, IdOrClass},
    styled_dom::{compact_cache_rebuilds_on_this_thread, StyledDom},
};
use azul_css::{
    dynamic_selector::DynamicSelectorContext,
    props::property::{CssProperty, CssPropertyType},
};

fn page() -> StyledDom {
    let mut body = Dom::create_body();
    for i in 0..20 {
        body = body.with_child(
            Dom::create_div()
                .with_ids_and_classes(vec![IdOrClass::Class("row".into())].into())
                .with_css(if i % 2 == 0 {
                    "padding: 2px; @theme(dark) { color: #eeeeee; }"
                } else {
                    "padding: 3px;"
                }),
        );
    }
    let (css, _) = azul_css::parser2::new_from_str(
        ".row { display: block; } @media (min-width: 900px) { .row { display: none; } }",
    );
    StyledDom::create(&mut body, css)
}

fn ctx(width: f32, height: f32) -> DynamicSelectorContext {
    DynamicSelectorContext::default().with_viewport(width, height)
}

fn display_of_first_row(styled: &StyledDom) -> Option<CssProperty> {
    let node = azul_core::id::NodeId::new(1);
    let cache = styled.get_css_property_cache();
    let node_data = &styled.node_data.as_container()[node];
    let state = &styled.styled_nodes.as_container()[node].styled_node_state;
    cache
        .get_property(node_data, &node, state, &CssPropertyType::Display)
        .cloned()
}

#[test]
fn a_resize_that_crosses_no_breakpoint_restyles_nothing() {
    let mut styled = page();
    styled.set_dynamic_selector_context(ctx(800.0, 600.0));
    let shown = display_of_first_row(&styled);
    for (w, h) in [(820.0, 600.0), (850.0, 580.0), (899.0, 500.0), (700.0, 640.0)] {
        let before = compact_cache_rebuilds_on_this_thread();
        styled.set_dynamic_selector_context(ctx(w, h));
        assert_eq!(
            compact_cache_rebuilds_on_this_thread() - before,
            0,
            "{w}x{h} crosses no threshold of the DOM (900 px): nothing to restyle"
        );
        assert_eq!(display_of_first_row(&styled), shown, "{w}x{h}: the rows stay as they were");
    }
    // Crossing 900 px re-runs the cascade, and the rule now applies.
    let before = compact_cache_rebuilds_on_this_thread();
    styled.set_dynamic_selector_context(ctx(950.0, 600.0));
    assert!(
        compact_cache_rebuilds_on_this_thread() > before,
        "950 px crosses the 900 px query: the cascade runs again"
    );
    assert_ne!(display_of_first_row(&styled), shown, "the min-width rule hides the rows now");
}

#[test]
fn a_mode_change_still_restyles() {
    let mut styled = page();
    styled.set_dynamic_selector_context(ctx(800.0, 600.0));
    let mut dark = ctx(800.0, 600.0);
    dark.mode = azul_css::system::DarkLightMode::Dark;
    let before = compact_cache_rebuilds_on_this_thread();
    styled.set_dynamic_selector_context(dark);
    assert!(
        compact_cache_rebuilds_on_this_thread() > before,
        "a dark switch flips the `@theme(dark)` declarations: the cascade runs again"
    );
}
