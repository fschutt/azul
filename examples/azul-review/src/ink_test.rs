//! How an ink layer's frame catches up with its page: a stroke that ends was painted while it was
//! drawn, so the frame needs no redraw of every other stroke of the page (that took longer with every
//! stroke: 50 ms at the 13th, 150 ms at the 40th, each time the pen lifted). No window.

use super::*;

const SIZE: (u32, u32) = (1000, 730);

/// The keys a page has before and after one stroke (`id`) was kept on it.
fn before_and_after(revisions: &mut InkRevisions, page: usize, id: u64) -> (FrameKey, FrameKey, Option<Append>) {
    let before = revisions.key(page, SIZE);
    revisions.appended(page, id);
    (before, revisions.key(page, SIZE), revisions.append_of(page))
}

#[test]
fn a_stroke_the_frame_already_shows_needs_only_what_was_not_painted_of_it() {
    let mut r = InkRevisions::default();
    let (had, now, append) = before_and_after(&mut r, 0, 7);
    assert_eq!(
        catch_up(Some(had), now, append, Some(Layer::Pen), Layer::Pen, Some(7)),
        CatchUp::Tail,
        "the frame painted this stroke while it was drawn: paint its last segments, not every stroke again"
    );
}

#[test]
fn a_frame_that_never_showed_the_stroke_is_drawn_anew() {
    let mut r = InkRevisions::default();
    let (had, now, append) = before_and_after(&mut r, 0, 7);
    assert_eq!(catch_up(Some(had), now, append, Some(Layer::Pen), Layer::Pen, None), CatchUp::Whole);
    assert_eq!(
        catch_up(Some(had), now, append, Some(Layer::Pen), Layer::Pen, Some(6)),
        CatchUp::Whole,
        "it showed another stroke"
    );
}

#[test]
fn the_other_layer_of_the_page_keeps_its_frame() {
    let mut r = InkRevisions::default();
    let (had, now, append) = before_and_after(&mut r, 0, 7);
    // The stroke is the pen's: the marker's layer under the code shows the same as before.
    assert_eq!(
        catch_up(Some(had), now, append, Some(Layer::Pen), Layer::Marker, None),
        CatchUp::KeyOnly
    );
}

#[test]
fn an_erase_or_an_undo_after_the_stroke_draws_everything_anew() {
    let mut r = InkRevisions::default();
    let (had, _, _) = before_and_after(&mut r, 0, 7);
    r.touch(0);
    let now = r.key(0, SIZE);
    assert_eq!(r.append_of(0), None, "the page changed in another way: no append to take");
    assert_eq!(catch_up(Some(had), now, r.append_of(0), None, Layer::Pen, Some(7)), CatchUp::Whole);
}

#[test]
fn two_strokes_between_two_frames_draw_anew() {
    let mut r = InkRevisions::default();
    let had = r.key(0, SIZE);
    r.appended(0, 7);
    r.appended(0, 8);
    let now = r.key(0, SIZE);
    assert_eq!(
        catch_up(Some(had), now, r.append_of(0), Some(Layer::Pen), Layer::Pen, Some(8)),
        CatchUp::Whole,
        "the frame is a stroke behind the last one: it cannot be told what the first one was"
    );
}

#[test]
fn another_file_another_page_or_another_size_draws_anew() {
    let mut r = InkRevisions::default();
    let (had, _, _) = before_and_after(&mut r, 0, 7);
    r.open();
    r.appended(0, 1);
    let opened = r.key(0, SIZE);
    assert_eq!(catch_up(Some(had), opened, r.append_of(0), Some(Layer::Pen), Layer::Pen, Some(1)), CatchUp::Whole);

    let mut r = InkRevisions::default();
    let (had, now, append) = before_and_after(&mut r, 0, 7);
    let resized = r.key(0, (1200, 730));
    assert_ne!(now, resized);
    assert_eq!(catch_up(Some(had), resized, append, Some(Layer::Pen), Layer::Pen, Some(7)), CatchUp::Whole);
    let elsewhere = r.key(1, SIZE);
    assert_eq!(catch_up(Some(had), elsewhere, append, Some(Layer::Pen), Layer::Pen, Some(7)), CatchUp::Whole);
}

#[test]
fn a_frame_that_is_current_needs_nothing_and_a_new_frame_everything() {
    let r = InkRevisions::default();
    let key = r.key(0, SIZE);
    assert_eq!(catch_up(Some(key), key, None, None, Layer::Pen, None), CatchUp::Same);
    assert_eq!(catch_up(None, key, None, None, Layer::Pen, None), CatchUp::Whole);
}

#[test]
fn what_a_stroke_in_progress_has_painted_is_told() {
    let mut shown = LiveProgress::default();
    assert_eq!(shown.progress(), None, "nothing painted");
    let live = Stroke {
        page: 0,
        semantic: Semantic::Issue,
        points: vec![InkPoint { x: 1.0, y: 1.0, pressure: 0.5, tilt_x: 0.0, tilt_y: 0.0 }; 5],
        id: 9,
        epoch: 0,
    };
    shown.paint(Some(&live), &mut |_, _, _, _, _| {});
    assert_eq!(shown.progress(), Some((9, 5)));
}

#[test]
fn keeping_a_stroke_tells_the_page_it_only_grew() {
    use crate::AppState;
    let mut s = AppState::create(
        std::path::PathBuf::new(),
        vec![crate::code::SourceFile {
            path: std::path::PathBuf::from("f.rs"),
            display: "f.rs".to_string(),
            lines: vec!["fn f() {}".to_string()],
        }],
        azul::prelude::RefAny::new(0u8),
        std::path::PathBuf::new(),
    );
    let stroke = Stroke {
        page: 0,
        semantic: Semantic::Issue,
        points: vec![InkPoint { x: 1.0, y: 1.0, pressure: 0.5, tilt_x: 0.0, tilt_y: 0.0 }; 4],
        id: 3,
        epoch: 0,
    };
    s.keep(stroke);
    assert_eq!(s.ink.append_of(0).map(|a| a.id), Some(3));
    assert!(s.undo());
    assert_eq!(s.ink.append_of(0), None, "taking it back is a change of another kind");
}
