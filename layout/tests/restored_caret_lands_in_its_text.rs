//! After a structural edit the app has applied, the caret lands where the
//! edit's resume point says - in the text node it names.
//!
//! The resume point is structural (host key, child path, text child + byte),
//! so it survives the app's re-render. Turning it back into a caret made
//! `(run 0, byte)` of whatever text child it named: for any text that is not
//! the first run of its paragraph - the text after a `<b>`, after a `<br>` -
//! the caret was painted, and typed into, the paragraph's FIRST run.

use azul_core::{
    dom::{Dom, DomId, DomNodeId, IdOrClass, NodeId},
    geom::LogicalSize,
    resources::RendererResources,
    selection::{CursorAffinity, GraphemeClusterId, TextCursor},
    styled_dom::{NodeHierarchyItemId, StyledDom},
    task::Instant,
};
use azul_layout::{
    callbacks::ExternalSystemCallbacks,
    managers::changeset::{
        DocOpRemoveChildren, DocumentChangeset, DocumentOperation, EditResumePoint, NodePosition,
    },
    text3::cache::ShapedItem,
    window::LayoutWindow,
    window_state::FullWindowState,
};
use rust_fontconfig::FcFontCache;

const CSS: &str = r#"
    * { margin: 0; padding: 0; }
    body { font-size: 14px; width: 600px; }
    .p { display: block; }
"#;

fn text(s: &str) -> Dom {
    Dom::create_text_do_not_use_without_block_level_wrapper(s)
}

/// `body(0) > div.p[contenteditable](1) > div.p(2) > ["one "(3),
/// b(4) > "bold"(5), " two"(6)]` - the app's model, rendered afresh each
/// generation.
fn render() -> StyledDom {
    let class = || -> azul_core::dom::IdOrClassVec { vec![IdOrClass::Class("p".into())].into() };
    let mut dom = Dom::create_body().with_child(
        Dom::create_div()
            .with_ids_and_classes(class())
            .with_contenteditable(true)
            .with_child(
                Dom::create_div()
                    .with_ids_and_classes(class())
                    .with_child(text("one "))
                    .with_child(Dom::create_b().with_child(text("bold")))
                    .with_child(text(" two")),
            ),
    );
    let (css, _) = azul_css::parser2::new_from_str(CSS);
    StyledDom::create(&mut dom, css)
}

fn lay_out(lw: &mut LayoutWindow) {
    let mut ws = FullWindowState::default();
    ws.size.dimensions = LogicalSize::new(800.0, 600.0);
    lw.current_window_state = ws.clone();
    let rr = RendererResources::default();
    let sc = ExternalSystemCallbacks::rust_internal();
    let mut dbg = Some(Vec::new());
    lw.layout_and_generate_display_list(render(), &ws, &rr, &sc, &mut dbg)
        .unwrap();
}

fn dnid(n: usize) -> DomNodeId {
    DomNodeId {
        dom: DomId::ROOT_ID,
        node: NodeHierarchyItemId::from_crate_internal(Some(NodeId::new(n))),
    }
}

#[test]
fn a_restored_caret_lands_in_the_text_node_it_names() {
    let mut lw = LayoutWindow::new(FcFontCache::build()).unwrap();
    lay_out(&mut lw);

    // A structural edit whose resume point is byte 1 of " two" - text child 2
    // of the paragraph, the paragraph being child 0 of the host.
    let host_key = {
        let lr = lw.get_layout_result(&DomId::ROOT_ID).expect("layout result");
        azul_core::diff::calculate_contenteditable_key(
            lr.styled_dom.node_data.as_ref(),
            lr.styled_dom.node_hierarchy.as_ref(),
            NodeId::new(1),
        )
    };
    let id = lw.record_document_edit(DocumentChangeset {
        id: 7,
        target: dnid(2),
        // An empty removal: the edit itself changes nothing.
        operation: DocumentOperation::RemoveChildren(DocOpRemoveChildren {
            parent: dnid(2),
            start: 0,
            end: 0,
        }),
        resume: EditResumePoint {
            anchor_key: host_key,
            node_path: vec![0u32].into(),
            position: NodePosition::in_text_child(2, 1),
        },
        timestamp: Instant::from(std::time::Instant::now()),
    });
    // The app applied it and re-renders: the caret is restored against the
    // new generation.
    assert!(lw.mark_document_edit_applied(id));
    lay_out(&mut lw);

    let target = lw
        .session_text_target()
        .expect("premise: the caret was restored into a laid-out block");
    let run_of_two = target
        .layout
        .items
        .iter()
        .find_map(|item| match &item.item {
            ShapedItem::Cluster(c) if c.source_node_id == Some(NodeId::new(6)) => {
                Some(c.source_cluster_id.source_run)
            }
            _ => None,
        })
        .expect("premise: \" two\" is laid out in the paragraph");
    assert_ne!(
        run_of_two, 0,
        "premise: \" two\" is not the paragraph's first run"
    );

    assert_eq!(
        lw.text_edit_manager.get_primary_cursor(),
        Some(TextCursor {
            cluster_id: GraphemeClusterId {
                source_run: run_of_two,
                start_byte_in_run: 1,
            },
            affinity: CursorAffinity::Leading,
        }),
        "the caret is in \" two\", after its first byte"
    );
}
