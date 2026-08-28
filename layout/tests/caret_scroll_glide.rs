//! Ledger #8: "typed past page 1 → glide to page 2" — cursor reveals ride
//! the physics AnimateTo spring (Keyboard provenance) unless system
//! animations are disabled (e2e determinism keeps the instant jump).

use azul_core::dom::{Dom, DomId, NodeId, TabIndex};
use azul_core::geom::LogicalSize;
use azul_core::resources::{RendererResources, SystemAnimations};
use azul_core::selection::{CursorAffinity, GraphemeClusterId, TextCursor};
use azul_core::styled_dom::StyledDom;
use azul_layout::window::{LayoutWindow, ScrollMode, SelectionScrollType};
use azul_layout::{callbacks::ExternalSystemCallbacks, window_state::FullWindowState};
use rust_fontconfig::FcFontCache;

const CSS: &str = r#"
    * { margin: 0; padding: 0; }
    body { font-size: 16px; }
    .clip { display: block; width: 300px; height: 100px; overflow: auto; }
    .editor { display: block; width: 280px; }
"#;

/// body=0 > div.clip=1 > div.editor=2 > text=3 (tall enough to scroll).
fn build(animations: SystemAnimations) -> LayoutWindow {
    let mut editor = Dom::create_div();
    editor =
        editor.with_ids_and_classes(vec![azul_core::dom::IdOrClass::Class("editor".into())].into());
    editor.set_contenteditable(true);
    editor.set_tab_index(TabIndex::Auto);
    let long = "line of text that wraps and wraps to make many lines ".repeat(20);
    let mut dom = Dom::create_body().with_child(
        Dom::create_div()
            .with_ids_and_classes(vec![azul_core::dom::IdOrClass::Class("clip".into())].into())
            .with_child(editor.with_child(
                Dom::create_text_do_not_use_without_block_level_wrapper(long.as_str()),
            )),
    );
    let (css, _) = azul_css::parser2::new_from_str(CSS);
    let styled_dom = StyledDom::create(&mut dom, css);
    let mut lw = LayoutWindow::new(FcFontCache::build()).unwrap();
    lw.system_animations_override = Some(animations);
    let mut ws = FullWindowState::default();
    ws.size.dimensions = LogicalSize::new(800.0, 600.0);
    lw.current_window_state = ws.clone();
    let rr = RendererResources::default();
    let sc = ExternalSystemCallbacks::rust_internal();
    let mut dbg = Some(Vec::new());
    lw.layout_and_generate_display_list(styled_dom, &ws, &rr, &sc, &mut dbg)
        .unwrap();

    // Caret deep into the text (far below the 100px clip) + focus.
    lw.focus_manager
        .set_focused_node(Some(azul_core::dom::DomNodeId {
            dom: DomId::ROOT_ID,
            node: azul_core::styled_dom::NodeHierarchyItemId::from_crate_internal(Some(
                NodeId::new(2),
            )),
        }));
    // Seed the clip node's scroll STATE (the shells do this via
    // register_scroll_nodes after layout; tests seed it directly).
    lw.scroll_manager.set_scroll_position(
        DomId::ROOT_ID,
        NodeId::new(1),
        azul_core::geom::LogicalPosition::zero(),
        azul_core::task::Instant::from(std::time::Instant::now()),
    );
    lw.text_edit_manager.initialize_editing(
        TextCursor {
            cluster_id: GraphemeClusterId {
                source_run: 0,
                start_byte_in_run: (long.len() - 5) as u32,
            },
            affinity: CursorAffinity::Leading,
        },
        DomId::ROOT_ID,
        NodeId::new(3),
        0,
    );
    lw
}

#[test]
fn cursor_reveal_glides_through_the_physics_queue_by_default() {
    let mut lw = build(SystemAnimations::default());
    let scrolled = lw.scroll_selection_into_view(SelectionScrollType::Cursor, ScrollMode::Instant);
    assert!(scrolled, "the caret is off-screen: a scroll must happen");
    assert!(
        lw.scroll_manager.scroll_input_queue.has_pending(),
        "glide mode queues an AnimateTo input for the physics spring \
         instead of teleporting"
    );
}

#[test]
fn disabled_system_animations_keep_the_instant_jump() {
    let mut lw = build(SystemAnimations::disabled());
    let scrolled = lw.scroll_selection_into_view(SelectionScrollType::Cursor, ScrollMode::Instant);
    assert!(scrolled);
    assert!(
        !lw.scroll_manager.scroll_input_queue.has_pending(),
        "e2e determinism: disabled() must keep the classic instant path"
    );
}
