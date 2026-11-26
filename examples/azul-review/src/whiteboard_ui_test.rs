//! The whiteboard's panel as a DOM and as a picture: what it holds, what takes the pointer, what is
//! drawn over the cutouts at the window's scale. Needs azul, no window.

use std::sync::Arc;

use azul::image::RawImageData;

use super::*;
use crate::{
    whiteboard::{CutoutDraft, CutoutImage, End, Source},
    whiteboard_geom::{Rect, View},
};

fn with(app: &mut RefAny, f: &mut dyn FnMut(&mut Panel)) -> bool {
    match app.downcast_mut::<Panel>() {
        Some(mut p) => {
            f(&mut p);
            true
        }
        None => false,
    }
}

fn on_jump(_: &mut RefAny, _: ItemId, _: &mut CallbackInfo) -> Update {
    Update::DoNothing
}

fn on_changed(_: &mut RefAny, _: &mut CallbackInfo) {}

fn host() -> Host {
    Host { app: RefAny::new(Panel::new("/work/azul")), with, on_jump, on_changed }
}

fn nodes(dom: &Dom) -> Vec<&Dom> {
    let mut out = vec![dom];
    for child in dom.children.as_slice() {
        out.extend(nodes(child));
    }
    out
}

fn with_id<'a>(dom: &'a Dom, id: &str) -> Option<&'a Dom> {
    nodes(dom).into_iter().find(|n| n.root.has_id(AzString::from(id)))
}

fn texts(dom: &Dom) -> Vec<String> {
    nodes(dom)
        .into_iter()
        .filter_map(|n| n.root.node_type.get_text().into_option())
        .map(|t| t.as_str().to_string())
        .filter(|t| !t.trim().is_empty())
        .collect()
}

fn events(node: &Dom, handler: azul::callbacks::CallbackType) -> Vec<EventFilter> {
    node.root.callbacks.as_slice().iter().filter(|c| c.callback.cb == handler as usize).map(|c| c.event).collect()
}

fn picture(w: u32, h: u32) -> Arc<CutoutImage> {
    Arc::new(CutoutImage { width: w, height: h, scale: 2.0, rgba: vec![255; (w * h * 4) as usize], png: vec![1] })
}

fn source() -> Source {
    Source {
        file: "src/lib.rs".to_string(),
        page: 0,
        first_line: 10,
        last_line: 12,
        shape: vec![],
        file_hash: 1,
        range_hash: 2,
        lines: vec!["fn first_line_shown() {}".to_string(), "fn second_line_shown() {}".to_string()],
    }
}

fn with_cutouts(panel: &mut Panel) -> (ItemId, ItemId) {
    let a = panel.wb.board.add_cutout(CutoutDraft::ready(Pt::new(10.0, 10.0), picture(200, 100), source())).expect("room");
    let b = panel.wb.board.add_cutout(CutoutDraft::ready(Pt::new(150.0, 10.0), picture(200, 100), source())).expect("room");
    (a, b)
}

#[test]
fn an_empty_board_says_how_to_fill_it_and_has_the_surface_that_takes_the_pointer() {
    crate::l10n::in_english();
    let host = host();
    let dom = host.run(|p| panel_dom(p, &host)).expect("a panel");
    assert!(texts(&dom).iter().any(|t| t.contains("Circle some code")), "{:?}", texts(&dom));
    assert!(with_id(&dom, SURFACE_ID).is_some(), "the surface");
}

#[test]
fn the_surface_takes_the_pointer_the_double_click_and_the_wheel_and_nothing_else_does() {
    crate::l10n::in_english();
    let host = host();
    let dom = host.run(|p| panel_dom(p, &host)).expect("a panel");
    let surface = with_id(&dom, SURFACE_ID).expect("the surface");
    assert_eq!(events(surface, on_down), vec![EventFilter::Hover(HoverEventFilter::MouseDown)]);
    assert_eq!(events(surface, on_move), vec![EventFilter::Hover(HoverEventFilter::MouseMove)]);
    assert_eq!(events(surface, on_up), vec![EventFilter::Hover(HoverEventFilter::MouseUp)]);
    assert_eq!(events(surface, on_double), vec![EventFilter::Hover(HoverEventFilter::DoubleClick)]);
    assert_eq!(events(surface, on_wheel), vec![EventFilter::Hover(HoverEventFilter::Scroll)]);
    // The cutouts under it answer nothing themselves: the surface decides what is under the pointer.
    for n in nodes(&dom) {
        if n.root.has_id(AzString::from(SURFACE_ID)) {
            continue;
        }
        assert!(n.root.callbacks.as_slice().is_empty(), "{:?}", texts(n));
    }
}

#[test]
fn a_cutout_is_a_node_under_the_surface_with_its_label() {
    crate::l10n::in_english();
    let host = host();
    let dom = host
        .run(|p| {
            let (a, _) = with_cutouts(p);
            p.wb.board.set_label(a, "the parser").expect("a label");
            panel_dom(p, &host)
        })
        .expect("a panel");
    assert!(texts(&dom).iter().any(|t| t == "the parser"), "{:?}", texts(&dom));
    assert!(!texts(&dom).iter().any(|t| t.contains("Circle some code")), "the hint goes when there is something");
    // The surface is the last child of the panel: over everything.
    let last = dom.children.as_slice().last().expect("children");
    assert!(last.root.has_id(AzString::from(SURFACE_ID)));
}

#[test]
fn a_frame_shows_while_a_region_is_carried_over_the_bar_and_the_surface_stays_on_top_of_it() {
    crate::l10n::in_english();
    let host = host();
    let made = |over: bool| {
        host.run(|p| {
            p.drag_over = over;
            panel_dom(p, &host)
        })
        .expect("a panel")
    };
    let (still, over) = (made(false), made(true));
    assert_eq!(over.children.as_slice().len(), still.children.as_slice().len() + 1, "the frame is one node more");
    let last = over.children.as_slice().last().expect("children");
    assert!(last.root.has_id(AzString::from(SURFACE_ID)), "the surface, not the frame, is over everything");
    for n in nodes(&over) {
        if !n.root.has_id(AzString::from(SURFACE_ID)) {
            assert!(n.root.callbacks.as_slice().is_empty(), "the frame answers nothing: the surface keeps the pointer");
        }
    }
}

#[test]
fn a_cutout_without_its_picture_shows_the_lines_it_was_made_from() {
    crate::l10n::in_english();
    let host = host();
    let dom = host
        .run(|p| {
            p.wb
                .board
                .add_cutout(CutoutDraft::pending(Pt::new(0.0, 0.0), Size::new(200.0, 80.0), source()))
                .expect("room");
            panel_dom(p, &host)
        })
        .expect("a panel");
    let said = texts(&dom);
    assert!(said.iter().any(|t| t.contains("Drawing the picture")), "{said:?}");
    assert!(said.iter().any(|t| t.contains("first_line_shown")), "{said:?}");
}

#[test]
fn the_pictures_are_made_once_and_kept_while_the_cutout_has_the_same_one() {
    let mut panel = Panel::new("/w");
    let (a, _) = with_cutouts(&mut panel);
    let c = panel.wb.board.cutout(a).cloned().expect("a cutout");
    assert!(panel.image_of(&c).is_some());
    let key = panel.images.get(&a).map(|(k, _)| *k).expect("kept");
    assert!(panel.image_of(&c).is_some());
    assert_eq!(panel.images.get(&a).map(|(k, _)| *k), Some(key), "the same image, not another upload");
    assert_eq!(panel.images.len(), 1);
    if let ImageSlot::Ready(img) = &c.image {
        assert_eq!(key, Arc::as_ptr(img) as usize, "it is the picture of the cutout");
    }
    // A cutout whose picture is gone lets its image go.
    panel.wb.board.set_image(a, ImageSlot::Failed("x".to_string()));
    let c = panel.wb.board.cutout(a).cloned().expect("a cutout");
    assert!(panel.image_of(&c).is_none());
    assert!(panel.images.is_empty());
}

#[test]
fn a_notice_of_the_board_reaches_the_person_in_their_words() {
    crate::l10n::in_english();
    assert_eq!(say(&Notice::ArrowRepeated), "There is an arrow like this one already.");
    assert!(say(&Notice::TooManyItems { limit: 500 }).contains("500"));
    let mut panel = Panel::new("/w");
    assert!(panel.wb.board.add_arrow(End::Free(Pt::new(0.0, 0.0)), End::Free(Pt::new(5.0, 5.0)), 0.0, "").is_err());
    let (a, b) = with_cutouts(&mut panel);
    panel.wb.board.add_arrow(End::item(a), End::item(b), 0.0, "").expect("an arrow");
    assert!(panel.wb.board.add_arrow(End::item(a), End::item(b), 0.0, "").is_err());
    panel.hint = None;
    panel.collect_notices();
    assert!(panel.hint.is_none(), "a refusal is told by the one who asked, not twice");
}

#[test]
fn a_cutouts_name_for_a_screen_reader_is_where_it_comes_from_and_what_it_says() {
    crate::l10n::in_english();
    let mut panel = Panel::new("/w");
    let (a, _) = with_cutouts(&mut panel);
    let plain = panel.wb.board.cutout(a).cloned().expect("a cutout");
    assert_eq!(cutout_name(&plain), "src/lib.rs, lines 10 to 12");
    panel.wb.board.set_label(a, "parser").expect("a label");
    let labelled = panel.wb.board.cutout(a).cloned().expect("a cutout");
    assert_eq!(cutout_name(&labelled), "parser: src/lib.rs, lines 10 to 12");
}

/// The straight alpha bytes of a surface picture.
fn alpha_of(image: &RawImage) -> Vec<u8> {
    let RawImageData::U8(bytes) = &image.pixels else { panic!("8-bit pixels") };
    bytes.as_ref().chunks_exact(4).map(|p| p[3]).collect()
}

#[test]
fn what_is_drawn_over_the_cutouts_is_drawn_at_the_scale_of_the_window() {
    let mut panel = Panel::new("/w");
    let (a, b) = with_cutouts(&mut panel);
    // a: (10,10) 100 x 50; b: (150,10) 100 x 50. The arrow runs from (110, 35) to (150, 35).
    panel.wb.board.add_arrow(End::item(a), End::item(b), 0.0, "").expect("an arrow");
    let one = paint_surface(&panel, 300, 100, 1.0);
    let two = paint_surface(&panel, 600, 200, 2.0);
    assert_eq!((one.width, one.height, two.width, two.height), (300, 100, 600, 200));
    let (one, two) = (alpha_of(&one), alpha_of(&two));
    let row = |a: &[u8], w: usize, y: usize| a[y * w..(y + 1) * w].iter().filter(|v| **v > 0).count();
    assert_eq!(one[0], 0, "nothing in the corner");
    // The arrow's line is a few pixels thick and about 40 px long: at 2x twice as long and twice as thick.
    assert!(row(&one, 300, 35) >= 30 && row(&one, 300, 35) <= 70, "{}", row(&one, 300, 35));
    assert!(row(&two, 600, 70) >= 60 && row(&two, 600, 70) <= 140, "{}", row(&two, 600, 70));
    let total = |a: &[u8]| a.iter().filter(|v| **v > 0).count();
    assert!(total(&two) > total(&one) * 3, "{} against {}", total(&two), total(&one));
}

#[test]
fn an_arrow_wholly_off_the_surface_paints_nothing_and_one_that_runs_off_it_paints_what_is_on_it() {
    let mut panel = Panel::new("/w");
    let (a, _) = with_cutouts(&mut panel);
    let far = |x: f32| CutoutDraft::ready(Pt::new(x, 10.0), picture(200, 100), source());
    let c = panel.wb.board.add_cutout(far(3_000.0)).expect("room");
    let d = panel.wb.board.add_cutout(far(3_300.0)).expect("room");
    panel.wb.board.add_arrow(End::item(c), End::item(d), 0.0, "").expect("an arrow");
    let painted = |panel: &Panel| alpha_of(&paint_surface(panel, 300, 100, 1.0)).iter().filter(|v| **v > 0).count();
    assert_eq!(painted(&panel), 0, "an arrow far off the surface paints nothing");
    // A straight arrow along a row (its box has no height) is on the surface all the same.
    let near = panel.wb.board.add_arrow(End::item(a), End::item(c), 0.0, "").expect("an arrow");
    assert!(painted(&panel) > 100, "an arrow that runs off the surface paints what is on it: {}", painted(&panel));
    // The same arrow, the board panned so that it is wholly out of view: nothing.
    panel.wb.board.set_view(View { pan: Pt::new(-5_000.0, 0.0), zoom: 1.0 });
    assert_eq!(painted(&panel), 0, "{near:?} is out of view");
}

#[test]
fn a_carried_region_is_drawn_as_its_outline_moved_by_the_carry() {
    let square = [Pt::new(10.0, 10.0), Pt::new(50.0, 10.0), Pt::new(50.0, 40.0), Pt::new(10.0, 40.0)];
    let mut image = RawImage::create_rgba8(300, 100, vec![0_u8; 300 * 100 * 4], true);
    paint_ghost(&mut image, 1.0, &square, Pt::new(100.0, 20.0));
    let alpha = alpha_of(&image);
    let at = |x: usize, y: usize| alpha[y * 300 + x];
    // Now at x 110..150, y 30..60: the top edge, the left edge, the closing edge...
    assert!(at(130, 30) > 0 && at(110, 45) > 0 && at(130, 60) > 0 && at(150, 45) > 0, "the moved outline");
    // ... and not where the region lies, nor inside the outline.
    assert_eq!((at(30, 10), at(10, 25), at(130, 45)), (0, 0, 0));
}

#[test]
fn the_selected_cutout_gets_an_outline_and_a_handle_and_an_arrow_being_drawn_shows() {
    let mut panel = Panel::new("/w");
    let (a, _) = with_cutouts(&mut panel);
    let before = alpha_of(&paint_surface(&panel, 300, 100, 1.0)).iter().filter(|v| **v > 0).count();
    assert_eq!(before, 0, "nothing is drawn over a board of pictures");
    panel.wb.input.select(Some(a));
    let after = alpha_of(&paint_surface(&panel, 300, 100, 1.0)).iter().filter(|v| **v > 0).count();
    assert!(after > 200, "an outline and a handle: {after} pixels");
    // The pointer pressed on the rim and dragged away: the arrow it would make is on the surface.
    let board_rect: Rect = panel.wb.board.cutout(a).map(|c| c.rect).unwrap_or_default();
    let Panel { wb, .. } = &mut panel;
    wb.input.press(&mut wb.board, Pt::new(board_rect.right() - 3.0, board_rect.y + 20.0), PointerKind::Mouse, Mods::default());
    wb.input.motion(&mut wb.board, Pt::new(board_rect.right() + 100.0, board_rect.y + 60.0), Mods::default());
    let drawing = alpha_of(&paint_surface(&panel, 300, 100, 1.0)).iter().filter(|v| **v > 0).count();
    assert!(drawing > after, "{drawing} against {after}");
}

#[test]
fn the_keys_of_the_panel_take_the_selection_off_and_let_go_and_take_a_step_back() {
    let mut panel = Panel::new("/w");
    let (a, _) = with_cutouts(&mut panel);
    panel.wb.input.select(Some(a));
    assert!(on_key(&mut panel, Key::Delete));
    assert_eq!(panel.wb.board.len(), 1);
    assert!(!on_key(&mut panel, Key::Delete), "nothing selected");
    assert!(on_key(&mut panel, Key::Undo));
    assert_eq!(panel.wb.board.len(), 2);
    assert!(on_key(&mut panel, Key::Redo));
    assert!(!on_key(&mut panel, Key::Escape), "nothing to let go of");
}

#[test]
fn the_text_area_of_a_label_is_over_the_surface_so_that_a_click_lands_in_it() {
    crate::l10n::in_english();
    let host = host();
    let (dom, a) = host
        .run(|p| {
            let (a, _) = with_cutouts(p);
            p.editing = Some(a);
            (panel_dom(p, &host), a)
        })
        .expect("a panel");
    let place = |id: &str| dom.children.as_slice().iter().position(|n| n.root.has_id(AzString::from(id)));
    let (surface, area) = (place(SURFACE_ID), place(&format!("__azreview_whiteboard-label-{a}")));
    assert!(surface.is_some() && area.is_some(), "{surface:?} {area:?}");
    assert!(area > surface, "the text area comes after the surface: it is drawn over it and gets the pointer");
}

#[test]
fn a_jump_to_a_file_that_is_not_as_it_was_tells_so_and_one_to_a_file_that_is_takes_it_back() {
    use crate::whiteboard_jump::Freshness;
    crate::l10n::in_english();
    let target = |freshness| JumpTarget { id: 1, file: "src/lib.rs".to_string(), first_line: 20, last_line: 24, freshness };
    assert_eq!(jump_words(&target(Freshness::Fresh)), None, "nothing to say about a file that is as it was");
    assert!(jump_words(&target(Freshness::FileChanged)).expect("told").contains("file has changed"));
    let moved = jump_words(&target(Freshness::LinesMoved)).expect("told");
    assert!(moved.contains("20") && moved.contains("24"), "where the lines went: {moved}");
    assert!(jump_words(&target(Freshness::LinesChanged)).expect("told").contains("have changed"));
    assert!(jump_words(&target(Freshness::FileMissing)).expect("told").contains("no longer there"));
    let mut panel = Panel::new("/w");
    panel.tell_jump(&target(Freshness::LinesChanged));
    assert!(panel.hint.is_some());
    panel.tell_jump(&target(Freshness::Fresh));
    assert!(panel.hint.is_none(), "the word is taken back");
}

#[test]
fn the_panels_keys_are_delete_escape_and_the_two_steps_and_nothing_else() {
    use VirtualKeyCode as K;
    assert_eq!(key_of(K::Delete, false, false), Some(Key::Delete));
    assert_eq!(key_of(K::Back, false, false), Some(Key::Delete));
    assert_eq!(key_of(K::Back, true, false), None, "Mod+Backspace is not Delete");
    assert_eq!(key_of(K::Escape, false, false), Some(Key::Escape));
    assert_eq!(key_of(K::Z, true, false), Some(Key::Undo));
    assert_eq!(key_of(K::Z, true, true), Some(Key::Redo));
    assert_eq!(key_of(K::Y, true, false), Some(Key::Redo));
    assert_eq!(key_of(K::Z, false, false), None, "a plain Z is a letter, not a step");
    assert_eq!(key_of(K::A, true, false), None);
}

#[test]
fn a_label_is_typed_in_a_text_area_under_its_cutout_while_it_is_being_edited() {
    crate::l10n::in_english();
    let host = host();
    let (dom, id) = host
        .run(|p| {
            let (a, _) = with_cutouts(p);
            p.editing = Some(a);
            (panel_dom(p, &host), a)
        })
        .expect("a panel");
    assert!(with_id(&dom, &format!("__azreview_whiteboard-label-{id}")).is_some(), "the text area");
    let plain = host.run(|p| {
        p.editing = None;
        panel_dom(p, &host)
    });
    assert!(with_id(&plain.expect("a panel"), &format!("__azreview_whiteboard-label-{id}")).is_none());
}

#[test]
fn an_arrows_label_is_shown_on_its_line_and_typed_in_a_text_area_that_takes_the_keyboard() {
    crate::l10n::in_english();
    let host = host();
    let arrow = host
        .run(|p| {
            let (a, b) = with_cutouts(p);
            p.wb.board.add_arrow(End::item(a), End::item(b), 0.0, "calls").expect("an arrow")
        })
        .expect("a panel");
    let area = format!("__azreview_whiteboard-label-{arrow}");
    let shown = host.run(|p| panel_dom(p, &host)).expect("a panel");
    assert!(texts(&shown).iter().any(|t| t == "calls"), "{:?}", texts(&shown));
    assert!(with_id(&shown, &area).is_none(), "it is not being typed");
    let typing = host
        .run(|p| {
            p.editing = Some(arrow);
            panel_dom(p, &host)
        })
        .expect("a panel");
    let editor = with_id(&typing, &area).expect("the text area");
    let asks = editor.root.get_attributes().as_slice().iter().any(|a| matches!(a, AttributeType::Autofocus));
    assert!(asks, "the person double-clicked to type: the area takes the keyboard when it mounts");
    // An arrow with no label shows nothing of the kind.
    let bare = host
        .run(|p| {
            p.editing = None;
            p.wb.board.set_label(arrow, "").expect("a label");
            panel_dom(p, &host)
        })
        .expect("a panel");
    assert!(!texts(&bare).iter().any(|t| t == "calls"), "{:?}", texts(&bare));
}

#[test]
fn what_the_board_told_is_shown_in_the_panel_until_the_next_press() {
    crate::l10n::in_english();
    let host = host();
    let dom = host
        .run(|p| {
            p.hint = Some("There is an arrow like this one already.".to_string());
            panel_dom(p, &host)
        })
        .expect("a panel");
    assert!(texts(&dom).iter().any(|t| t.contains("arrow like this one")), "{:?}", texts(&dom));
    let quiet = host.run(|p| {
        p.hint = None;
        panel_dom(p, &host)
    });
    assert!(!texts(&quiet.expect("a panel")).iter().any(|t| t.contains("arrow like this one")));
}

#[test]
fn the_frame_times_are_kept_for_a_line_of_numbers_and_forgotten_once_told() {
    let mut perf = Perf::default();
    assert_eq!(perf.take_summary(), None);
    for us in [100, 300, 200] {
        perf.surface(us);
    }
    perf.layout(1_000);
    let line = perf.take_summary().expect("numbers");
    assert!(line.contains("surface n=3 mean=200 max=300 us") && line.contains("layout n=1 mean=1000 max=1000 us"), "{line}");
    assert_eq!(perf.take_summary(), None, "told once");
    for us in 0..1_000 {
        perf.surface(us);
    }
    assert!(perf.take_summary().is_some_and(|l| l.contains(&format!("n={}", Perf::KEEP))), "only the last ones are kept");
}
