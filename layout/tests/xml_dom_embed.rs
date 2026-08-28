//! miniword hookup regression: a Dom built by the XML parser
//! (`dom_from_parsed_xml`) must render when EMBEDDED inside an
//! application DOM built with the Rust API. In miniword the parsed
//! document was structurally present (34 children) yet painted nothing
//! and measured zero height, while sibling `create_div`/`create_text`
//! canaries rendered fine.

use azul_core::dom::Dom;
use azul_core::geom::LogicalSize;
use azul_core::resources::RendererResources;
use azul_layout::callbacks::ExternalSystemCallbacks;
use azul_layout::window::LayoutWindow;
use azul_layout::window_state::FullWindowState;
use rust_fontconfig::FcFontCache;

fn xml_content(fragment_body: &str) -> Dom {
    let xml = format!("<html><head></head><body>{fragment_body}</body></html>");
    let parsed = azul_layout::xml::parse_xml_string(&xml).expect("xml parses");
    let full = azul_layout::xml::dom_from_parsed_xml(azul_layout::xml::Xml {
        root: parsed.into(),
    });
    // miniword's unwrap: body children move under a plain div.
    let mut content = Dom::create_div();
    for c in full.children.as_ref() {
        if matches!(c.root.get_node_type(), azul_core::dom::NodeType::Body) {
            content.children = c.children.clone();
        }
    }
    content.fixup_children_estimated();
    content
}

fn text_items_of(dom: Dom) -> usize {
    let mut dom = Dom::create_body().with_child(
        Dom::create_div()
            .with_css("width: 794px; height: 600px; background: white; padding: 96px;")
            .with_child(dom),
    );
    let styled = azul_core::styled_dom::StyledDom::create_from_dom(core::mem::replace(
        &mut dom,
        Dom::create_body(),
    ));
    let mut lw = LayoutWindow::new(FcFontCache::build()).unwrap();
    let mut ws = FullWindowState::default();
    ws.size.dimensions = LogicalSize::new(1280.0, 800.0);
    let rr = RendererResources::default();
    let cb = ExternalSystemCallbacks::rust_internal();
    let mut dbg = Some(Vec::new());
    lw.layout_and_generate_display_list(styled, &ws, &rr, &cb, &mut dbg)
        .unwrap();
    let result = lw
        .get_layout_result(&azul_core::dom::DomId::ROOT_ID)
        .expect("layout result");
    result
        .display_list
        .items
        .iter()
        .filter(|i| {
            matches!(
                i,
                azul_layout::solver3::display_list::DisplayListItem::Text { .. }
            )
        })
        .count()
}

#[test]
fn xml_built_content_renders_inside_an_app_dom() {
    // Control: hand-built equivalent MUST produce text items.
    let control = Dom::create_div()
        .with_child(Dom::create_div().with_child(
            Dom::create_text_do_not_use_without_block_level_wrapper("Title"),
        ))
        .with_child(Dom::create_div().with_child(
            Dom::create_text_do_not_use_without_block_level_wrapper("Hello world paragraph"),
        ));
    let control_texts = text_items_of(control);
    assert!(
        control_texts >= 2,
        "control: hand-built text must render, got {control_texts} text items"
    );

    // XML-built content of the same shape must render TOO.
    let xml_texts = text_items_of(xml_content("<h1>Title</h1><p>Hello world paragraph</p>"));
    assert!(
        xml_texts >= 2,
        "xml-built content must render like the control, got {xml_texts} text items \
         (miniword: parsed document present in the DOM but painted nothing)"
    );
}

/// The FULL miniword shell shape: definite-height flex column, gray canvas
/// (flex column, centered), white sheet (fixed size, 96px padding,
/// box-sizing border-box, overflow hidden), canary + xml content inside.
#[test]
fn xml_built_content_renders_inside_the_word_shell_shape() {
    let xml = xml_content("<h1>Title</h1><p>Hello world paragraph</p>");

    let canary = Dom::create_div()
        .with_css("background: #ff0000; height: 30px; width: 300px;")
        .with_child(Dom::create_text_do_not_use_without_block_level_wrapper(
            "CANARY",
        ));

    let sheet = Dom::create_div()
        .with_css(
            "width: 794px; height: 1123px; background: white; flex-grow: 0; \
             flex-shrink: 0; border: 1px solid #a6a6a6; margin-bottom: 16px; \
             box-sizing: border-box; padding: 96px; overflow: hidden;",
        )
        .with_child(canary)
        .with_child(xml);

    let canvas = Dom::create_div()
        .with_css(
            "flex-grow: 1; min-height: 0px; background: #e3e3e3; display: flex; \
             flex-direction: column; align-items: center; padding-top: 18px; \
             overflow: hidden;",
        )
        .with_child(sheet);

    let editor = Dom::create_div()
        .with_css(
            "display: flex; flex-direction: column; flex-grow: 1; \
             min-height: 0px; background: white; font-family: \"Liberation Sans\";",
        )
        .with_child(Dom::create_div().with_css("height: 148px; flex-shrink: 0;"))
        .with_child(canvas)
        .with_child(Dom::create_div().with_css("height: 23px; flex-shrink: 0;"));

    let mut dom = Dom::create_body()
        .with_css(
            "display: flex; flex-direction: column; margin: 0; padding: 0; \
             height: 100%; background: white; font-family: \"Liberation Sans\"; \
             font-size: 12px; color: #444444;",
        )
        .with_child(editor);

    let styled = azul_core::styled_dom::StyledDom::create_from_dom(core::mem::replace(
        &mut dom,
        Dom::create_body(),
    ));
    let mut lw = LayoutWindow::new(FcFontCache::build()).unwrap();
    let mut ws = FullWindowState::default();
    ws.size.dimensions = LogicalSize::new(1280.0, 800.0);
    let rr = RendererResources::default();
    let cb = ExternalSystemCallbacks::rust_internal();
    let mut dbg = Some(Vec::new());
    lw.layout_and_generate_display_list(styled, &ws, &rr, &cb, &mut dbg)
        .unwrap();
    let result = lw
        .get_layout_result(&azul_core::dom::DomId::ROOT_ID)
        .expect("layout result");
    let texts: Vec<String> = result
        .display_list
        .items
        .iter()
        .filter_map(|i| match i {
            azul_layout::solver3::display_list::DisplayListItem::Text { glyphs, .. } => {
                Some(format!("{} glyphs", glyphs.len()))
            }
            _ => None,
        })
        .collect();
    assert!(
        texts.len() >= 3,
        "canary + h1 + p must all produce text items in the Word shell \
         shape, got {} ({texts:?})",
        texts.len()
    );
}
