//! A window that is rotated between two layout passes paints the background of its canvas across the NEW
//! width, also when the pass makes the display list again from inside (a field the user typed into).
//!
//! Found by the QA's rotation run of AzWidgets (2026-11-27, Android and the iOS GPU path): after portrait to
//! landscape the white page covered the portrait width and a grey band began where the portrait window had
//! ended, while the cards below were laid out across the whole new width. The shells publish the window
//! state of a pass (`LayoutWindow::current_window_state`) AFTER the pass. A display list the pass makes again
//! from inside - the text a typed field holds is applied again, a restored caret, the reveal of the focused
//! caret - took the size of the canvas from that field, which still held the size of the LAST pass: its first
//! rectangle, the canvas background, covered the old width. (A screenshot clears to the page colour and
//! hides it; a renderer that clears to the window colour shows it.)
//!
//! The page is the smallest with a field to type into: `body(background) > div#target(contenteditable) > p >
//! text`.

use azul_core::{
    dom::{Dom, DomId, DomNodeId, IdOrClass, IdOrClassVec, NodeId},
    geom::LogicalSize,
    resources::RendererResources,
    selection::{CursorAffinity, GraphemeClusterId, TextCursor},
    styled_dom::{NodeHierarchyItemId, StyledDom},
};
use azul_css::props::basic::ColorU;
use azul_layout::{
    callbacks::ExternalSystemCallbacks, solver3::display_list::DisplayListItem, window::LayoutWindow,
    window_state::FullWindowState,
};
use rust_fontconfig::FcFontCache;

/// The colour of the page: the canvas background (`body { background }`), the first rectangle of its list.
static PAGE: ColorU = ColorU {
    r: 0x12,
    g: 0x34,
    b: 0x56,
    a: 0xff,
};

const CSS: &str = "* { margin: 0; padding: 0; } body { background: #123456; font-size: 14px; } \
                   .field { display: block; height: 20px; } p { display: block; }";

/// body(0) > div#target(1) > p(2) > text(3)
const FIELD: NodeId = NodeId::new(1);
const TEXT: NodeId = NodeId::new(3);

/// The phone upright and on its side, as the QA's emulator reports them (logical pixels).
static PORTRAIT: LogicalSize = LogicalSize {
    width: 411.0,
    height: 873.0,
};
static LANDSCAPE: LogicalSize = LogicalSize {
    width: 890.0,
    height: 411.0,
};

fn page() -> StyledDom {
    let ids: IdOrClassVec =
        vec![IdOrClass::Id("target".into()), IdOrClass::Class("field".into())].into();
    let field = Dom::create_div()
        .with_ids_and_classes(ids)
        .with_contenteditable(true)
        .with_child(
            Dom::create_p()
                .with_child(Dom::create_text_do_not_use_without_block_level_wrapper("x")),
        );
    let mut dom = Dom::create_body().with_child(field);
    let (css, _) = azul_css::parser2::new_from_str(CSS);
    StyledDom::create(&mut dom, css)
}

fn window_state(size: LogicalSize) -> FullWindowState {
    let mut state = FullWindowState::default();
    state.size.dimensions = size;
    state
}

/// The app renders the page for a window in `state`: the way the shells' `regenerate_layout` installs a new
/// generation (the reconciliation, the layout under the state of the pass, the reconciliation's completion).
/// The state is NOT published here: the shell does that after the pass.
fn render_under(lw: &mut LayoutWindow, state: &FullWindowState) {
    let renderer_resources = RendererResources::default();
    let system_callbacks = ExternalSystemCallbacks::rust_internal();
    let mut debug_messages = Some(Vec::new());
    let mut styled = page();
    let pending =
        lw.begin_reconciliation(DomId::ROOT_ID, &mut styled, azul_core::task::Instant::now());
    lw.layout_new_generation(
        styled,
        state,
        &renderer_resources,
        &system_callbacks,
        &mut debug_messages,
    )
    .unwrap();
    lw.finish_reconciliation(DomId::ROOT_ID, &pending);
}

/// A window with the page laid out and published for an upright phone.
fn window() -> LayoutWindow {
    let mut lw = LayoutWindow::new(FcFontCache::build()).unwrap();
    lw.node_identity.id_is_key = true;
    lw.system_animations_override = Some(azul_core::resources::SystemAnimations::disabled());
    let portrait = window_state(PORTRAIT);
    lw.current_window_state = portrait.clone();
    render_under(&mut lw, &portrait);
    lw
}

/// Types `s` at the start of the field, the way a keystroke does; the app does not adopt it, so the text
/// overlay holds it and every later pass applies it again.
fn type_into_field(lw: &mut LayoutWindow, s: &str) {
    lw.focus_manager.set_focused_node(Some(DomNodeId {
        dom: DomId::ROOT_ID,
        node: NodeHierarchyItemId::from_crate_internal(Some(FIELD)),
    }));
    lw.start_editing_at(
        TextCursor {
            cluster_id: GraphemeClusterId {
                source_run: 0,
                start_byte_in_run: 0,
            },
            affinity: CursorAffinity::Leading,
        },
        DomId::ROOT_ID,
        TEXT,
        0,
    );
    let _ = lw.record_text_input(s);
    let _ = lw.apply_text_changeset();
}

/// The rectangle of the canvas background in the display list the window holds: the first filled rectangle
/// of the page's colour.
fn canvas(lw: &LayoutWindow) -> (f32, f32) {
    let list = &lw
        .layout_results
        .get(&DomId::ROOT_ID)
        .expect("the page was laid out")
        .display_list;
    list.items
        .iter()
        .find_map(|item| match item {
            DisplayListItem::Rect { bounds, color, .. } if *color == PAGE => {
                let rect = bounds.inner();
                Some((rect.size.width, rect.size.height))
            }
            _ => None,
        })
        .expect("the canvas background is in the display list")
}

/// THE REPORT: the user typed into a field, the phone is turned, the page is rendered for the new size. The
/// canvas covers the new width (and height), in the list the pass makes again for the typed field.
///
/// Mutation: the canvas of the list a pass makes again taken from `current_window_state` (the size of the
/// last pass, the state before): the width of the portrait window, 411.
#[test]
fn a_rotated_window_paints_its_canvas_across_the_new_size_under_a_field_the_user_typed_into() {
    let mut lw = window();
    assert_eq!(
        canvas(&lw),
        (PORTRAIT.width, PORTRAIT.height),
        "premise: the canvas covers the upright window"
    );
    type_into_field(&mut lw, "Ada");
    assert_eq!(lw.content_overlay.text_len(), 1, "premise: the typing is in the overlay");

    // The shell lays the page out for the turned phone; it publishes the window state after the pass.
    render_under(&mut lw, &window_state(LANDSCAPE));

    assert_eq!(
        canvas(&lw),
        (LANDSCAPE.width, LANDSCAPE.height),
        "the canvas of the page laid out for the turned phone covers the whole of it"
    );
    assert_eq!(
        lw.current_window_state.size.dimensions, PORTRAIT,
        "the layout pass leaves the window state the shell publishes alone"
    );
}

/// The turn back: the same page, a field with typing in it, landscape to portrait. The canvas does not stay
/// as wide as the window was.
#[test]
fn a_window_turned_back_paints_its_canvas_across_the_upright_size_under_a_typed_field() {
    let mut lw = window();
    type_into_field(&mut lw, "Ada");
    render_under(&mut lw, &window_state(LANDSCAPE));
    lw.current_window_state = window_state(LANDSCAPE);
    assert_eq!(canvas(&lw).0, LANDSCAPE.width, "premise: the canvas covers the window on its side");

    render_under(&mut lw, &window_state(PORTRAIT));

    assert_eq!(canvas(&lw), (PORTRAIT.width, PORTRAIT.height));
}

/// Without anything the pass has to apply again, no list is made from inside it, and the canvas was right
/// before this too: the premise of the two tests above, that the bug needs the list made again.
#[test]
fn a_rotated_window_without_a_typed_field_paints_its_canvas_across_the_new_size() {
    let mut lw = window();
    render_under(&mut lw, &window_state(LANDSCAPE));
    assert_eq!(canvas(&lw), (LANDSCAPE.width, LANDSCAPE.height));
}
