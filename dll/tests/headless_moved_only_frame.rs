//! A frame whose ONLY change is a move is painted by the CPU presenter of the
//! shells, and is the frame a full redraw paints (E2E-R12).
//!
//! `justify-content` and `flex-direction` on a flex container move its items and
//! change no pixel of anything else. The layout pass exports the move
//! (`LayoutCache::last_patch_move`), the display-list diff counts the moved items
//! as MOVED rather than damaged, so such a frame has an EMPTY damage list - and
//! the pixels travel only in the blit of a frame that is not skipped. A presenter
//! that read "empty damage" as "nothing to do" kept the old window on screen while
//! a fresh render differed in tens of thousands of pixels (`gen-e2e` batch 3,
//! lines 27, 38 and 40: `assert_changed` read STALE SCREEN).
//!
//! The in-process runner's presenter had that bug and `layout/src/e2e/moved_frame_tests.rs`
//! pins its fix. This pins the presenter the SHELLS use - `CpuBackend`, the one the
//! headless host and the CPU mode of every native shell run - on the same changes,
//! written the way a callback writes them (`ContentChange::NodeCss`, the chokepoint
//! behind `set_node_css_override`), against the frame a backend with no previous frame
//! paints of the same layout. Zero tolerance, like `headless_patch_blit.rs`.

use azul::desktop::shell2::headless::CpuBackend;
use azul_core::{
    dom::{Dom, DomId, IdOrClass, NodeId, NodeType},
    geom::LogicalSize,
    resources::RendererResources,
    styled_dom::StyledDom,
};
use azul_css::props::{
    layout::flex::{LayoutFlexDirection, LayoutJustifyContent},
    property::CssProperty,
};
use azul_layout::{
    callbacks::ExternalSystemCallbacks, overlay::ContentChange,
    solver3::display_list::set_dl_patching_enabled, window::LayoutWindow,
    window_state::FullWindowState,
};
use rust_fontconfig::FcFontCache;

const W: f32 = 320.0;
const H: f32 = 120.0;
/// The node order of the page: the root 0, the flex row 1, its items 2 and 3.
const ROW: usize = 1;

/// A 300x60 white flex row holding two opaque 60x40 items on a grey page.
fn page() -> StyledDom {
    let node = |class: &str| {
        Dom::create_node(NodeType::Div)
            .with_ids_and_classes(vec![IdOrClass::Class(class.into())].into())
    };
    let dom = node("root").with_child(node("row").with_child(node("a")).with_child(node("b")));
    let css = r#"
        * { margin: 0px; padding: 0px; }
        .root { width: 100%; height: 100%; background: #888; }
        .row { display: flex; width: 300px; height: 60px; background: #fff; }
        .a { width: 60px; height: 40px; background: #cc2222; }
        .b { width: 60px; height: 40px; background: #2255cc; }
    "#;
    let (css, _) = azul_css::parser2::new_from_str(css);
    let mut dom = dom;
    StyledDom::create(&mut dom, css)
}

/// What the presenter painted around one container-only change.
struct Frames {
    /// The first frame, before the change.
    before: Vec<u8>,
    /// The second frame, painted over the first the way a shell paints it.
    incremental: Vec<u8>,
    /// The second layout painted by a backend that holds no previous frame.
    full: Vec<u8>,
    /// The incremental backend reported that it presented nothing.
    presented_nothing: bool,
    /// The layout pass exported a move summary for the change.
    layout_exported_a_move: bool,
}

/// Lay the page out, paint it, write `change` to the flex row as a callback does,
/// paint again - and paint the same layout from scratch.
fn paint_around(change: CssProperty) -> Frames {
    // Honest (refined) damage is REQUIRED: with full-strength damage the whole
    // frame repaints and a skipped move is unfalsifiable.
    azul_layout::cpurender::set_dl_diff_refinements(true);
    set_dl_patching_enabled(true);

    let mut lw = LayoutWindow::new(FcFontCache::default()).expect("a layout window");
    let rr = RendererResources::default();
    let cb = ExternalSystemCallbacks::rust_internal();
    let mut ws = FullWindowState::default();
    ws.size.dimensions = LogicalSize::new(W, H);
    lw.current_window_state = ws.clone();
    lw.layout_and_generate_display_list(page(), &ws, &rr, &cb, &mut None)
        .expect("the page lays out");

    let mut backend = CpuBackend::new();
    backend.render_frame(&lw, &rr, W, H, 1.0);
    let before = backend
        .last_frame
        .as_ref()
        .expect("render_frame retains a pixmap")
        .data()
        .to_vec();

    lw.apply_content_change(ContentChange::NodeCss {
        dom_id: DomId::ROOT_ID,
        node_id: NodeId::new(ROW),
        props: vec![change],
        override_only: false,
    });
    let layout_exported_a_move = lw.layout_cache.last_patch_move.is_some();

    backend.render_frame(&lw, &rr, W, H, 1.0);
    let incremental = backend
        .last_frame
        .as_ref()
        .expect("render_frame retains a pixmap")
        .data()
        .to_vec();
    let presented_nothing = backend.last_present_damage.is_none();

    let mut fresh = CpuBackend::new();
    fresh.render_frame(&lw, &rr, W, H, 1.0);
    let full = fresh
        .last_frame
        .as_ref()
        .expect("render_frame retains a pixmap")
        .data()
        .to_vec();

    // Restore the default for tests running after us in-process.
    set_dl_patching_enabled(true);
    Frames {
        before,
        incremental,
        full,
        presented_nothing,
        layout_exported_a_move,
    }
}

/// How many bytes of `a` differ from `b`.
fn differing_bytes(a: &[u8], b: &[u8]) -> usize {
    assert_eq!(a.len(), b.len(), "frames of one size");
    a.iter().zip(b).filter(|(x, y)| x != y).count()
}

/// The three claims about a container-only change, with `what` named in the failure.
fn check(what: &str, change: CssProperty) {
    let frames = paint_around(change);
    assert!(
        frames.layout_exported_a_move,
        "harness ({what}): the change must reach the presenter as a pending MOVE - a gate that \
         never takes the path proves nothing (a zero is not a measurement)"
    );
    assert!(
        differing_bytes(&frames.before, &frames.full) > 0,
        "harness ({what}): a fresh render of the changed layout must differ from the first frame, \
         or the change moved nothing"
    );
    assert!(
        !frames.presented_nothing,
        "{what}: the presenter reported that it presented nothing for a layout that moved boxes"
    );
    let stale = differing_bytes(&frames.before, &frames.incremental) == 0;
    assert!(
        !stale,
        "{what}: the frame on screen is the frame before the change (STALE SCREEN)"
    );
    let wrong = differing_bytes(&frames.incremental, &frames.full);
    assert_eq!(
        wrong, 0,
        "{what}: the incremental frame differs from a full redraw on {wrong} bytes"
    );
}

/// `flex-end` moves BOTH items by one delta: every moved box translated, the
/// damage list empty.
#[test]
fn a_justify_content_that_moves_every_item_by_one_delta_is_painted() {
    check(
        "justify-content: flex-end",
        CssProperty::justify_content(LayoutJustifyContent::FlexEnd),
    );
}

/// `center` is the same shape with another delta.
#[test]
fn a_justify_content_center_is_painted() {
    check(
        "justify-content: center",
        CssProperty::justify_content(LayoutJustifyContent::Center),
    );
}

/// `space-between` moves the SECOND item and nothing else - the case of the
/// review's probe `p15` (batch 3, line 27).
#[test]
fn a_justify_content_that_moves_only_the_second_item_is_painted() {
    check(
        "justify-content: space-between",
        CssProperty::justify_content(LayoutJustifyContent::SpaceBetween),
    );
}

/// `column` moves the second item diagonally (probe `p8`, line 38).
#[test]
fn a_flex_direction_that_moves_the_second_item_diagonally_is_painted() {
    check(
        "flex-direction: column",
        CssProperty::flex_direction(LayoutFlexDirection::Column),
    );
}
