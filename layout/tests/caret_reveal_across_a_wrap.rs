//! Typed glyphs stay hidden until the gliding caret reaches them - across a
//! line wrap too.
//!
//! The reveal clip only handled a glide along ONE line moving right. When
//! typing wrapped the caret onto the next line, the glide was diagonal and
//! the splitter dropped the reveal altogether: the typed glyph showed on the
//! new line while the caret was still flying in from the end of the old one.
//! Fast typing in a multi-line field hit that at every wrap - and every
//! further keystroke while that diagonal glide was in flight retargeted from
//! a mid-air rect, failing the same test again, so two or three glyphs per
//! wrap appeared ahead of the caret.

use azul_core::{
    dom::{Dom, DomId, DomNodeId, NodeId, TabIndex},
    geom::{LogicalRect, LogicalSize},
    resources::{RendererResources, SystemAnimations},
    selection::{CursorAffinity, GraphemeClusterId, TextCursor},
    styled_dom::{NodeHierarchyItemId, StyledDom},
    task::{advance_test_clock_ms, freeze_test_clock, reset_test_clock},
};
use azul_layout::{
    callbacks::ExternalSystemCallbacks, solver3::display_list::DisplayListItem,
    window::LayoutWindow, window_state::FullWindowState,
};
use rust_fontconfig::FcFontCache;

/// Narrow enough that the text wraps onto several lines.
const CSS: &str = r#"
    * { margin: 0; padding: 0; }
    body { font-size: 16px; width: 600px; }
    .editor { display: block; width: 110px; }
"#;
const TEXT: &str = "hello world tween target and more";
/// Node layout: body=0, div.editor=1, text=2.
const TEXT_NODE: usize = 2;

fn cursor(byte: u32) -> TextCursor {
    TextCursor {
        cluster_id: GraphemeClusterId {
            source_run: 0,
            start_byte_in_run: byte,
        },
        affinity: CursorAffinity::Leading,
    }
}

fn text_dom_node_id() -> DomNodeId {
    DomNodeId {
        dom: DomId::ROOT_ID,
        node: NodeHierarchyItemId::from_crate_internal(Some(NodeId::new(TEXT_NODE))),
    }
}

fn build(animations: SystemAnimations) -> LayoutWindow {
    let mut editor = Dom::create_div()
        .with_ids_and_classes(vec![azul_core::dom::IdOrClass::Class("editor".into())].into());
    editor.set_contenteditable(true);
    editor.set_tab_index(TabIndex::Auto);
    let mut dom = Dom::create_body().with_child(
        editor.with_child(Dom::create_text_do_not_use_without_block_level_wrapper(TEXT)),
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
    lw.text_edit_manager
        .initialize_editing(cursor(0), DomId::ROOT_ID, NodeId::new(TEXT_NODE), 0);
    lw.text_edit_manager.blink.set_visibility(true);
    lw.focus_manager.set_focused_node(Some(text_dom_node_id()));
    lw
}

fn move_caret(lw: &mut LayoutWindow, byte: u32) {
    lw.text_edit_manager.multi_cursor =
        Some(azul_core::selection::MultiCursorState::new_with_cursor(
            cursor(byte),
            text_dom_node_id(),
            0,
        ));
}

fn rebuild(lw: &mut LayoutWindow) {
    lw.regenerate_display_list_for_dom(DomId::ROOT_ID);
}

fn rendered_caret(lw: &LayoutWindow) -> LogicalRect {
    lw.get_layout_result(&DomId::ROOT_ID)
        .unwrap()
        .display_list
        .items
        .iter()
        .rev()
        .find_map(|item| match item {
            DisplayListItem::CursorRect { bounds, .. } => Some(bounds.0),
            _ => None,
        })
        .expect("a CursorRect item")
}

/// The caret rect at `byte` with tweens off: the true target geometry.
fn true_caret_rect_at(byte: u32) -> LogicalRect {
    let mut truth = build(SystemAnimations::disabled());
    move_caret(&mut truth, byte);
    rebuild(&mut truth);
    rendered_caret(&truth)
}

/// Every glyph painted on the line of `line_rect` whose x lies in
/// `[x0, x1)`, together with the right edge of the clip it is painted
/// through (`None` = unclipped / infinite).
fn glyphs_in(lw: &LayoutWindow, line_rect: LogicalRect, x0: f32, x1: f32) -> Vec<(f32, f32)> {
    let (top, bottom) = (line_rect.origin.y, line_rect.origin.y + line_rect.size.height);
    let mut out = Vec::new();
    for item in &lw.get_layout_result(&DomId::ROOT_ID).unwrap().display_list.items {
        if let DisplayListItem::Text {
            glyphs, clip_rect, ..
        } = item
        {
            let clip_right = clip_rect.0.origin.x + clip_rect.0.size.width;
            for g in glyphs.iter() {
                if g.point.y >= top && g.point.y <= bottom && g.point.x >= x0 && g.point.x < x1 {
                    out.push((g.point.x, clip_right));
                }
            }
        }
    }
    out
}

/// A byte on the first line and one on the second, with the second line's
/// caret LEFT of the first's (the shape of a wrap).
fn wrap_pair() -> (u32, LogicalRect, u32, LogicalRect) {
    let first = true_caret_rect_at(5);
    let mut b = 6;
    loop {
        let r = true_caret_rect_at(b);
        if r.origin.y > first.origin.y + first.size.height * 0.5 {
            // A few glyphs INTO the second line, so there is something left
            // of the target to reveal.
            let target = b + 3;
            let r = true_caret_rect_at(target);
            assert!(
                (r.origin.y - true_caret_rect_at(b).origin.y).abs() < 0.5,
                "the target is still on the second line"
            );
            assert!(r.origin.x < first.origin.x, "the fixture wraps: the target is left of the start");
            return (5, first, target, r);
        }
        b += 1;
        assert!(b < TEXT.len() as u32, "the fixture must wrap");
    }
}

#[test]
fn a_glyph_typed_across_a_wrap_waits_for_the_caret_to_land() {
    reset_test_clock();
    freeze_test_clock();
    let (a, _a_rect, b, b_rect) = wrap_pair();

    let mut lw = build(SystemAnimations {
        caret_tween_duration_ms: 10_000,
        ..SystemAnimations::default()
    });
    move_caret(&mut lw, a);
    rebuild(&mut lw);

    // The keystroke that wraps: the caret target jumps to the next line.
    lw.text_edit_manager.tween.reveal_pending = true;
    move_caret(&mut lw, b);
    rebuild(&mut lw);

    let rendered = rendered_caret(&lw);
    assert!(
        rendered.origin.y < b_rect.origin.y - b_rect.size.height * 0.5,
        "premise: the caret is still gliding in from the first line ({rendered:?} vs {b_rect:?})"
    );

    // The glyph just left of the target on the new line is the typed one.
    // The tween-free twin proves it is there; the gliding window must not
    // paint it yet - either by clipping it away or by leaving it out.
    let font_size = 16.0;
    let (x0, x1) = (b_rect.origin.x - font_size, b_rect.origin.x);
    let truth = {
        let mut truth = build(SystemAnimations::disabled());
        move_caret(&mut truth, b);
        rebuild(&mut truth);
        glyphs_in(&truth, b_rect, x0, x1)
    };
    assert!(!truth.is_empty(), "premise: there is a glyph left of the target on the new line");
    for (x, clip_right) in glyphs_in(&lw, b_rect, x0, x1) {
        assert!(
            clip_right <= x,
            "a typed glyph at x={x} must stay hidden until the caret lands (clip right edge {clip_right})"
        );
    }
}

#[test]
fn a_glyph_typed_across_a_wrap_shows_once_the_caret_has_landed() {
    reset_test_clock();
    freeze_test_clock();
    let (a, _a_rect, b, b_rect) = wrap_pair();

    let mut lw = build(SystemAnimations {
        caret_tween_duration_ms: 1_000,
        ..SystemAnimations::default()
    });
    move_caret(&mut lw, a);
    rebuild(&mut lw);
    lw.text_edit_manager.tween.reveal_pending = true;
    move_caret(&mut lw, b);
    rebuild(&mut lw);

    let _ = advance_test_clock_ms(2_000);
    rebuild(&mut lw);
    assert_eq!(rendered_caret(&lw), b_rect, "the glide has retired on its target");

    let font_size = 16.0;
    let typed = glyphs_in(&lw, b_rect, b_rect.origin.x - font_size, b_rect.origin.x);
    assert!(!typed.is_empty());
    for (x, clip_right) in &typed {
        assert!(*clip_right > *x, "once landed the glyph at x={x} is painted");
    }
}
