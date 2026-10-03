//! A text field takes the font size its app gives it.
//!
//! In a browser `<input style="font-size: 24px">` draws its value 24 px high: the field's font
//! size is the value's. AzNotes' title is a `TextInput` that should read as a title (OFFICE7,
//! the brief's "the title field is too small"); styled `font-size: 24px` on the field, its value
//! stayed at the 11 px UI size, because the widget pins `font-size: 11px` on the value `<p>`
//! (`TEXT_INPUT_LABEL_PROPS`) instead of on the field, so nothing set on the field reaches the
//! text. The only way around it, `with_label_style`, replaces the theme's whole label style
//! (WRITER6 N4). The 11 px default belongs on the field (`TEXT_INPUT_CONTAINER_PROPS`), where
//! an app's `font-size` overrides it and the value inherits it.
//!
//! Owner: WIDGETS7 (layout/src/widgets/text_input.rs). Not compiled by the author (house rule);
//! RED until the widget's value line inherits the field's font size.

use azul_core::{
    dom::{Dom, DomId, DomNodeId, NodeId},
    geom::{LogicalRect, LogicalSize},
    resources::RendererResources,
    styled_dom::{NodeHierarchyItemId, StyledDom},
};
use azul_layout::{
    callbacks::ExternalSystemCallbacks, widgets::text_input::TextInput, window::LayoutWindow,
    window_state::FullWindowState,
};
use rust_fontconfig::FcFontCache;

/// A column with one field holding a title, styled `field_css`.
fn field(field_css: &str) -> StyledDom {
    let mut dom = Dom::create_body()
        .with_css("display: flex; flex-direction: column; margin: 0px; padding: 8px;")
        .with_child(
            TextInput::create()
                .with_text("Offsite agenda".into())
                .with_placeholder("Title".into())
                .dom()
                .with_css(field_css)
                .with_id("field".into()),
        );
    StyledDom::create(&mut dom, azul_css::css::Css::empty())
}

/// The border box of `#field` once `dom` is laid out at 800 x 600.
fn field_rect(dom: StyledDom) -> LogicalRect {
    let mut lw = LayoutWindow::new(FcFontCache::build()).expect("a layout window");
    let mut ws = FullWindowState::default();
    ws.size.dimensions = LogicalSize::new(800.0, 600.0);
    lw.current_window_state = ws.clone();
    let mut debug = None;
    lw.layout_and_generate_display_list(
        dom,
        &ws,
        &RendererResources::default(),
        &ExternalSystemCallbacks::rust_internal(),
        &mut debug,
    )
    .expect("the field lays out");
    let result = lw
        .get_layout_result(&DomId::ROOT_ID)
        .expect("the root DOM is laid out");
    let index = result
        .styled_dom
        .node_data
        .as_container()
        .internal
        .iter()
        .position(|node| node.has_id("field"))
        .expect("no node #field");
    lw.get_node_layout_rect(DomNodeId {
        dom: DomId::ROOT_ID,
        node: NodeHierarchyItemId::from_crate_internal(Some(NodeId::new(index))),
    })
    .expect("#field has no layout rect")
}

#[test]
fn a_text_field_takes_the_font_size_its_app_gives_it() {
    let plain = field_rect(field(""));
    let title = field_rect(field("font-size: 24px;"));
    // One 24 px line is taller than 24 px, plus the field's padding and border.
    assert!(
        title.size.height >= 26.0,
        "a field styled font-size: 24px is {} px tall (an 11 px field is {} px): its value \
         line kept the widget's 11 px",
        title.size.height,
        plain.size.height
    );
    assert!(
        title.size.height > plain.size.height + 6.0,
        "the 24 px field ({}) is not taller than the 11 px one ({})",
        title.size.height,
        plain.size.height
    );
}

#[test]
fn a_text_field_without_a_font_size_keeps_the_ui_size() {
    // The widget's 11 px default stays when the app says nothing: the field
    // is no taller than its minimum of one 11 px line plus the chrome.
    let plain = field_rect(field(""));
    assert!(
        plain.size.height <= 24.0,
        "an unstyled field is {} px tall",
        plain.size.height
    );
}
