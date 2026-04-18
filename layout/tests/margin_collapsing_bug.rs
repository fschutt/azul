use azul_core::{
    dom::{Dom, DomId, DomNodeId, IdOrClass, NodeId},
    geom::LogicalSize,
    resources::RendererResources,
    styled_dom::{NodeHierarchyItemId, StyledDom},
};
use azul_css::css::Css;
use azul_layout::{
    callbacks::ExternalSystemCallbacks, window::LayoutWindow, window_state::FullWindowState,
};
use rust_fontconfig::FcFontCache;

#[test]
fn test_margin_collapsing() {
    // 1. Create DOM
    let dom = Dom::create_div()
        .with_ids_and_classes(vec![IdOrClass::Class("root".into())].into())
        .with_child(
            Dom::h1("Heading").with_ids_and_classes(vec![IdOrClass::Class("my-h1".into())].into()),
        )
        .with_child(
            Dom::create_p()
                .with_child(Dom::create_text("Paragraph"))
                .with_ids_and_classes(vec![IdOrClass::Class("my-p".into())].into()),
        );

    // 2. Parse CSS
    let css_str = r#"
        .root {
            border: 1px solid black;
            padding: 0px;
        }
        .my-h1 {
            height: 50px;
            margin-bottom: 30px;
            background-color: red;
            margin-top: 0px; /* Override UA style to be sure */
        }
        .my-p {
            height: 50px;
            margin-top: 20px;
            background-color: blue;
        }
    "#;
    let (css, _) = azul_css::parser2::new_from_str(css_str);

    // 3. Create StyledDom
    let mut dom = dom; // needs to be mutable for StyledDom::new
    let styled_dom = StyledDom::create(&mut dom, css);

    // 4. Initialize LayoutWindow
    let font_cache = FcFontCache::build();
    let mut layout_window = LayoutWindow::new(font_cache).unwrap();

    // 5. Run layout
    let mut window_state = FullWindowState::default();
    window_state.size.dimensions = LogicalSize::new(800.0, 600.0);
    let renderer_resources = RendererResources::default();
    let system_callbacks = ExternalSystemCallbacks::rust_internal();
    let mut debug_messages = Some(Vec::new());

    layout_window
        .layout_and_generate_display_list(
            styled_dom,
            &window_state,
            &renderer_resources,
            &system_callbacks,
            &mut debug_messages,
        )
        .unwrap();

    // 6. Check positions
    // Root is at index 0
    let root_id = DomNodeId {
        dom: DomId::ROOT_ID,
        node: NodeHierarchyItemId::from_crate_internal(Some(NodeId::ZERO)),
    };

    let h1_id = layout_window
        .get_first_child(root_id)
        .expect("h1 not found");
    let p_id = layout_window.get_next_sibling(h1_id).expect("p not found");

    let h1_rect = layout_window
        .get_node_layout_rect(h1_id)
        .expect("h1 rect not found");
    let p_rect = layout_window
        .get_node_layout_rect(p_id)
        .expect("p rect not found");

    println!("H1: {:?}", h1_rect);
    println!("P: {:?}", p_rect);

    // Expected behavior with border on root:
    // Root has 1px border.
    // H1 margin-top is 0px (overridden).
    // H1 should be at y=1 (inside border).
    // H1 height 50.
    // H1 margin-bottom 30.
    // P margin-top 20.
    // Collapsed margin = 30.
    // P y = 1 + 50 + 30 = 81.

    assert_eq!(h1_rect.origin.y, 1.0);
    assert_eq!(
        p_rect.origin.y, 81.0,
        "Margins did not collapse correctly! Expected 81.0, got {}",
        p_rect.origin.y
    );
}
