//! The owner of a workspace's whiteboard: what the app does with it from the page and from the bar,
//! with the clock and the files as plain values. No window, no azul.

use std::sync::Arc;

use super::*;
use crate::{
    stroke_fixtures as fx,
    whiteboard::{CutoutImage, ImageSlot, Item, LIMITS},
    whiteboard_geom::{Pt, Size},
    whiteboard_store::DecodedPng,
};

const PAGE: Size = Size { w: 1000.0, h: 730.0 };

fn pts(f: &fx::Fixture) -> Vec<Pt> {
    f.samples.iter().map(|s| Pt::new(s.x, s.y)).collect()
}

fn chars() -> Vec<usize> {
    vec![55; 46]
}

fn lines_of_page<'a>(chars: &'a [usize]) -> PageLines<'a> {
    PageLines { first_number: 1, chars, top: 32.0, line_h: 15.0, text_x: 84.0, char_w: 6.6, max_w: 600.0 }
}

fn facts() -> SourceFacts {
    SourceFacts {
        file: "src/lib.rs".to_string(),
        file_hash: 77,
        covered: vec!["pub fn step_012(x: u32) -> u32 { x + 12 }".to_string()],
    }
}

fn bar() -> Bar {
    Bar { rect: Rect::new(980.0, 30.0, 300.0, 760.0), open: true, window_w: 1280.0 }
}

fn pointer_at(p: Pt) -> Pointer {
    Pointer { page: 0, in_page: p, in_window: Pt::new(p.x + 100.0, p.y + 50.0) }
}

fn word() -> Pt {
    Pt::new(fx::WORD_CENTRE.0, fx::WORD_CENTRE.1)
}

/// A loop round the word armed at `now`, lifted by a hold and dropped on the bar: the job it leaves.
fn drop_a_loop(wb: &mut Whiteboard, now: u64) -> CutoutJob {
    let c = chars();
    let region = wb
        .arm_stroke(0, &pts(&fx::closed_loop()), &lines_of_page(&c), PAGE, 5, now)
        .expect("a loop is armed");
    assert!(wb.armed.get(region).is_some());
    assert!(matches!(wb.page_press(pointer_at(word()), now + 100), Press::Inside(r) if r == region));
    assert_eq!(wb.page_tick(now + 100 + 300), Some(Tick::Held(region)));
    let far = Pt::new(word().x + 60.0, word().y);
    assert!(matches!(wb.page_motion(Pt::new(far.x + 100.0, far.y + 50.0), now + 600, &bar()), Motion::Lifted(r) if r == region));
    let drop = Pt::new(1_100.0, 200.0);
    let release = wb.page_release(drop, &bar());
    let Release::Dropped { region: r, at_bar } = release else { panic!("dropped: {release:?}") };
    assert_eq!(r, region);
    wb.drop_region(r, at_bar, facts(), 2.0, PAGE).expect("a cutout")
}

#[test]
fn a_loop_that_is_one_arms_a_region_and_a_stroke_that_is_not_arms_nothing() {
    let mut wb = Whiteboard::new("/work/azul");
    let c = chars();
    for f in [fx::slash(), fx::underline(), fx::scribble(), fx::tap()] {
        assert!(wb.arm_stroke(0, &pts(&f), &lines_of_page(&c), PAGE, 1, 0).is_none(), "{}", f.name);
    }
    let id = wb.arm_stroke(0, &pts(&fx::closed_loop()), &lines_of_page(&c), PAGE, 9, 0).expect("armed");
    let region = wb.armed.get(id).expect("there");
    assert_eq!((region.first_line, region.last_line, region.stroke_id), (12, 12, 9), "the word's line");
}

#[test]
fn a_loop_over_nothing_to_show_arms_nothing() {
    let mut wb = Whiteboard::new("/work/azul");
    let blank = vec![0_usize; 3];
    let page = PageLines { chars: &blank, ..lines_of_page(&[]) };
    assert!(wb.arm_stroke(0, &pts(&fx::closed_loop()), &page, PAGE, 1, 0).is_none(), "the page has three lines and the loop is on none of them");
}

#[test]
fn lifting_a_region_and_dropping_it_on_the_bar_makes_a_cutout_that_waits_for_its_picture() {
    let mut wb = Whiteboard::new("/work/azul");
    let job = drop_a_loop(&mut wb, 1_000);
    assert!(wb.armed.is_empty(), "the region is used up");
    assert_eq!(wb.board.len(), 1);
    let c = wb.board.cutout(job.id).expect("the cutout");
    assert!(matches!(c.image, ImageSlot::Pending));
    assert_eq!((c.source.file.as_str(), c.source.first_line, c.source.last_line), ("src/lib.rs", 12, 12));
    assert_eq!(c.source.file_hash, 77);
    assert_eq!(c.source.lines, facts().covered);
    assert!(c.source.shape.len() >= 8);
    // Its size on the board is the picture's: a 2x picture is shown at half its pixels.
    assert!((c.rect.w - job.plan.logical.w).abs() < 1e-3 && (c.rect.h - job.plan.logical.h).abs() < 1e-3);
    // It lies where it was dropped: centred on the point of the board under the pointer.
    let centre = c.rect.center();
    assert!((centre.x - 120.0).abs() < 1.0 && (centre.y - 170.0).abs() < 1.0, "{centre:?}");
    assert!(wb.board.needs_save());
}

#[test]
fn the_picture_that_comes_is_put_in_its_cutout_and_one_that_could_not_be_drawn_says_why() {
    let mut wb = Whiteboard::new("/work/azul");
    let job = drop_a_loop(&mut wb, 0);
    let image = CutoutImage { width: job.plan.px_w, height: job.plan.px_h, scale: 2.0, rgba: vec![255; (job.plan.px_w * job.plan.px_h * 4) as usize], png: vec![1] };
    assert!(wb.image_ready(job.id, Ok(image)));
    assert!(matches!(wb.board.cutout(job.id).map(|c| &c.image), Some(ImageSlot::Ready(_))));
    let second = drop_a_loop(&mut wb, 20_000);
    assert!(wb.image_ready(second.id, Err("no font".to_string())));
    assert!(matches!(wb.board.cutout(second.id).map(|c| &c.image), Some(ImageSlot::Failed(w)) if w == "no font"));
    assert!(!wb.image_ready(9_999, Err("x".to_string())), "no such cutout any more");
}

#[test]
fn a_cutout_that_is_dropped_when_the_board_is_full_is_refused_and_the_region_stays_armed() {
    let mut wb = Whiteboard::with_limits("/w", Limits { max_items: 1, ..LIMITS });
    drop_a_loop(&mut wb, 0);
    let c = chars();
    let id = wb.arm_stroke(0, &pts(&fx::closed_loop()), &lines_of_page(&c), PAGE, 6, 50_000).expect("armed");
    let refused = wb.drop_region(id, Pt::new(10.0, 10.0), facts(), 2.0, PAGE);
    assert_eq!(refused.err(), Some(Notice::TooManyItems { limit: 1 }));
    assert!(wb.armed.get(id).is_some(), "the person may make room and try again");
}

#[test]
fn a_big_loop_is_drawn_at_a_lower_scale_and_the_person_is_told() {
    let mut wb = Whiteboard::new("/w");
    let c = chars();
    let whole_page = vec![Pt::new(2.0, 2.0), Pt::new(998.0, 2.0), Pt::new(998.0, 728.0), Pt::new(2.0, 728.0)];
    // A loop is a recognised stroke; arm one by hand for the page's size.
    let id = wb.armed.arm(Armed { page: 0, polygon: whole_page, first_line: 1, last_line: 46, stroke_id: 1 }, PAGE, 0);
    let _ = c;
    let job = wb.drop_region(id, Pt::new(50.0, 50.0), facts(), 2.0, PAGE).expect("a cutout");
    assert!(job.plan.scale < 2.0);
    let told = wb.take_notices();
    assert!(told.iter().any(|n| matches!(n, Notice::ScaleLowered { wanted_pct: 200, .. })), "{told:?}");
    assert!(wb.take_notices().is_empty());
}

#[test]
fn the_pen_resting_over_an_armed_region_is_about_that_region() {
    let mut wb = Whiteboard::new("/w");
    let c = chars();
    let id = wb.arm_stroke(0, &pts(&fx::closed_loop()), &lines_of_page(&c), PAGE, 3, 0).expect("armed");
    let about = wb.region_at(0, word(), 1_000).expect("a region");
    assert_eq!((about.id, about.first_line, about.last_line), (id, 12, 12));
    assert!(about.shape_svg().starts_with('M'));
    assert!(wb.region_at(0, Pt::new(700.0, 600.0), 1_000).is_none());
    assert!(wb.region_at(0, word(), 9_000).is_none(), "the time is up");
    wb.expire(9_000);
    assert!(wb.armed.is_empty());
}

#[test]
fn escape_lets_go_of_the_drag_and_then_of_the_regions() {
    let mut wb = Whiteboard::new("/w");
    let c = chars();
    wb.arm_stroke(0, &pts(&fx::closed_loop()), &lines_of_page(&c), PAGE, 3, 0);
    assert!(wb.escape(), "a region was armed");
    assert!(wb.armed.is_empty());
    assert!(!wb.escape(), "nothing left to let go of");
}

// ---- the bar

#[test]
fn the_bar_remembers_whether_it_is_open_and_how_wide_and_keeps_the_width_in_range() {
    let mut wb = Whiteboard::new("/w");
    assert!(!wb.bar.open);
    wb.set_bar(true, 10_000.0, 1_280.0);
    assert!(wb.bar.open);
    assert!(wb.bar.width <= 1_280.0 * BAR_MAX_SHARE + 1e-3, "{}", wb.bar.width);
    wb.set_bar(true, 3.0, 1_280.0);
    assert!((wb.bar.width - BAR_MIN_PX).abs() < 1e-3);
    wb.set_bar(false, f32::NAN, 1_280.0);
    assert!(!wb.bar.open);
    assert!(wb.bar.width >= BAR_MIN_PX);
    // The width as a setting: read back as written, and what is no number is the default.
    wb.set_bar(true, 412.0, 1_280.0);
    assert_eq!(Whiteboard::parse_width(&Whiteboard::width_setting(wb.bar.width)), Some(412.0));
    assert_eq!(Whiteboard::parse_width("wide"), None);
    assert_eq!(Whiteboard::parse_width("-5"), None);
}

#[test]
fn the_bars_rectangle_in_the_window_is_the_thin_edge_closed_and_its_width_open() {
    let mut wb = Whiteboard::new("/w");
    wb.set_bar(false, 340.0, 1_280.0);
    let closed = wb.bar_geometry(Rect::new(0.0, 30.0, 1_280.0, 760.0));
    assert!(!closed.open && closed.rect.w < 20.0 && closed.rect.right() >= 1_279.9);
    wb.set_bar(true, 340.0, 1_280.0);
    let open = wb.bar_geometry(Rect::new(0.0, 30.0, 1_280.0, 760.0));
    assert!(open.open && (open.rect.w - 340.0).abs() < 1e-3 && open.rect.right() >= 1_279.9);
    assert_eq!(open.window_w, 1_280.0);
}

// ---- keeping it

struct Codec;

impl crate::whiteboard_store::ImageCodec for Codec {
    fn decode_png(&self, png: &[u8]) -> Result<DecodedPng, String> {
        Err(format!("{} bytes are no picture", png.len()))
    }
}

#[test]
fn a_save_is_built_only_when_one_is_wanted_and_none_runs_and_the_next_waits() {
    let mut wb = Whiteboard::new("/work/azul");
    assert!(wb.save_job(10).is_none(), "nothing changed: nothing to keep");
    let made = drop_a_loop(&mut wb, 0);
    let job = wb.save_job(10).expect("a save");
    assert_eq!(job.name, crate::whiteboard_store::board_name("/work/azul"));
    assert!(job.entries.iter().any(|e| e.name == crate::whiteboard_store::MANIFEST));
    // A change while it is being written.
    wb.board.set_label(made.id, "the step").expect("a label");
    assert!(wb.save_job(11).is_none(), "one is running: the next waits");
    assert!(wb.save_done(&Ok(())), "it was asked for meanwhile: write again");
    assert!(wb.board.needs_save(), "the label is not in the file yet");
    let again = wb.save_job(12).expect("the second");
    assert!(again.revision > job.revision);
    assert!(!wb.save_done(&Ok(())));
    assert!(!wb.board.needs_save(), "kept");
    assert!(matches!(wb.status, Status::Saved(12)));
    assert!(wb.save_job(13).is_none(), "nothing changed since");
}

#[test]
fn a_save_that_failed_is_told_and_the_board_still_wants_keeping() {
    let mut wb = Whiteboard::new("/work/azul");
    drop_a_loop(&mut wb, 0);
    wb.save_job(10).expect("a save");
    wb.save_done(&Err("the disk is full".to_string()));
    assert!(matches!(&wb.status, Status::SaveFailed(why) if why.contains("full")));
    assert!(wb.board.needs_save());
}

#[test]
fn a_file_that_cannot_be_read_is_kept_aside_and_the_board_starts_empty_and_says_so() {
    let mut wb = Whiteboard::new("/work/azul");
    let effects = wb.loaded(Err(StoreError::NotABoard("a comma was expected (byte 9)".to_string())), Some(b"garbage".to_vec()), 1_700_000_000_000);
    assert_eq!(wb.board.len(), 0);
    assert!(matches!(&wb.status, Status::Unreadable { kept_as: Some(name), .. } if name.contains("corrupt")));
    assert_eq!(effects.len(), 1, "the damaged file is written aside before anything replaces it");
    assert_eq!(effects[0].bytes, b"garbage".to_vec());
    assert!(effects[0].name.starts_with(&crate::whiteboard_store::board_name("/work/azul")));
    // A file of a newer version is left exactly as it is: no copy, and it is not written over either.
    let mut newer = Whiteboard::new("/work/azul");
    let effects = newer.loaded(Err(StoreError::TooNew { found: 3 }), Some(b"PK".to_vec()), 5);
    assert!(effects.is_empty());
    assert!(matches!(newer.status, Status::TooNew { found: 3 }));
    drop_a_loop(&mut newer, 0);
    assert!(newer.board.needs_save());
    assert!(newer.save_job(1).is_none(), "the newer file is never replaced by this version's board");
}

#[test]
fn no_file_yet_is_a_new_board_and_a_good_one_is_the_board() {
    let mut wb = Whiteboard::new("/work/azul");
    assert!(wb.loaded_none().is_empty());
    assert!(matches!(wb.status, Status::New));
    let mut other = Whiteboard::new("/work/azul");
    drop_a_loop(&mut other, 0);
    let job = other.save_job(5).expect("a save");
    let loaded = crate::whiteboard_store::read(
        &job.entries[0].bytes,
        &|n| job.entries.iter().find(|e| e.name == n).map(|e| e.bytes.clone()),
        &Codec,
        &LIMITS,
    );
    let mut fresh = Whiteboard::new("/work/azul");
    let effects = fresh.loaded(loaded, None, 9);
    assert!(effects.is_empty());
    assert_eq!(fresh.board.len(), 1);
    assert!(matches!(fresh.status, Status::Loaded { items: 1, .. }));
    assert!(!fresh.board.needs_save(), "what was read is what is on disk");
    assert!(matches!(fresh.board.items()[0], Item::Cutout(_)));
}

#[test]
fn a_board_read_with_a_missing_picture_or_a_repair_says_how_many() {
    let mut wb = Whiteboard::new("/w");
    let loaded = crate::whiteboard_store::Loaded {
        data: crate::whiteboard::BoardData::default(),
        workspace: "/w".to_string(),
        saved_ms: 1,
        left_out: 2,
        unreadable_pictures: 3,
    };
    wb.loaded(Ok(loaded), None, 1);
    assert!(matches!(wb.status, Status::Repaired { left_out: 2, pictures: 3 }));
    let _ = Arc::new(0);
}
