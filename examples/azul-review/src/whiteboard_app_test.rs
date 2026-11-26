//! The owner of a workspace's whiteboard: what the app does with it from the page and from the bar,
//! with the clock and the files as plain values. No window, no azul.

use std::sync::Arc;

use super::*;
use crate::{
    stroke_fixtures as fx,
    whiteboard::{CutoutDraft, CutoutImage, ImageSlot, Item, Source, LIMITS},
    whiteboard_boards::{self as boards, BoardIndex},
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
    drop_a_loop_at(wb, now, Pt::new(1_100.0, 200.0))
}

/// The same, dropped at `drop` (the window's point).
fn drop_a_loop_at(wb: &mut Whiteboard, now: u64, drop: Pt) -> CutoutJob {
    let c = chars();
    let region = wb
        .arm_stroke(0, &pts(&fx::closed_loop()), &lines_of_page(&c), PAGE, 5, now)
        .expect("a loop is armed");
    assert!(wb.armed.get(region).is_some());
    assert!(matches!(wb.page_press(pointer_at(word()), now + 100), Press::Inside(r) if r == region));
    assert_eq!(wb.page_tick(now + 100 + 300), Some(Tick::Held(region)));
    let far = Pt::new(word().x + 60.0, word().y);
    assert!(matches!(wb.page_motion(Pt::new(far.x + 100.0, far.y + 50.0), now + 600, &bar()), Motion::Lifted(r) if r == region));
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
    // It lies where it was dropped: centred on the point of the board under the pointer (the bar's header is not the board).
    let centre = c.rect.center();
    assert!((centre.x - 120.0).abs() < 1.0 && (centre.y - (170.0 - HEADER_PX)).abs() < 1.0, "{centre:?}");
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

// ---- sent by a button

/// A loop round the word, armed at `now`: its region.
fn arm_a_loop(wb: &mut Whiteboard, now: u64) -> RegionId {
    let c = chars();
    wb.arm_stroke(0, &pts(&fx::closed_loop()), &lines_of_page(&c), PAGE, 5, now).expect("a loop is armed")
}

fn closed_bar() -> Bar {
    Bar { rect: Rect::new(1_268.0, 30.0, 12.0, 760.0), open: false, window_w: 1_280.0 }
}

#[test]
fn a_region_sent_by_a_button_is_the_cutout_a_drop_makes_at_the_first_free_place_of_the_view() {
    let mut dropped = Whiteboard::new("/work/azul");
    let from_drop = drop_a_loop(&mut dropped, 0);
    let mut sent = Whiteboard::new("/work/azul");
    let region = arm_a_loop(&mut sent, 0);
    let job = sent.send_region_now(region, &bar(), facts(), 2.0, PAGE).expect("a cutout");
    assert_eq!(job.plan, from_drop.plan, "the same picture is drawn");
    assert_eq!(job.page, from_drop.page);
    let (a, b) = (sent.board.cutout(job.id).expect("sent"), dropped.board.cutout(from_drop.id).expect("dropped"));
    assert_eq!(a.source, b.source, "the same lines, hashes, revision and shape");
    assert_eq!((a.rect.w, a.rect.h), (b.rect.w, b.rect.h));
    assert!(matches!(a.image, ImageSlot::Pending), "it waits for its picture as a drop's does");
    assert_eq!(a.rect.origin(), Pt::new(16.0, 16.0), "the top left of the view, a margin in");
    assert!(sent.armed.get(region).is_none(), "the region is used up");
    assert_eq!(sent.input.selection(), Some(job.id), "and the cutout is the one selected");
    assert!(sent.board.needs_save());
    assert!(sent.bar.open, "the bar is open: it is seen");
}

#[test]
fn what_is_sent_one_after_the_other_is_put_in_view_a_gap_apart_until_the_view_is_full() {
    let mut wb = Whiteboard::new("/work/azul");
    let gap = whiteboard_place::GAP;
    let mut placed: Vec<Rect> = Vec::new();
    let (mut in_view, mut below) = (0, 0);
    for i in 0..40_u64 {
        let region = arm_a_loop(&mut wb, i * 100);
        let was = wb.board.view();
        let job = wb.send_region_now(region, &bar(), facts(), 2.0, PAGE).expect("room for it");
        let rect = wb.board.cutout(job.id).expect("there").rect;
        for earlier in &placed {
            assert!(!earlier.inflate(gap - 0.5).intersects(&rect), "{i}: {rect:?} is too near {earlier:?}");
        }
        let view = wb.board.view();
        if view == was {
            // Room in the view: all of it, a margin in from every edge (the view is 300 px wide and 700 high, its zoom is 1).
            in_view += 1;
            let seen = Rect::new(view.pan.x, view.pan.y, 300.0, 730.0);
            assert!(rect.x >= seen.x + gap - 0.01 && rect.y >= seen.y + gap - 0.01, "{i}: {rect:?} in {seen:?}");
            assert!(rect.right() + gap <= seen.right() + 0.01 && rect.bottom() + gap <= seen.bottom() + 0.01, "{i}: {rect:?} in {seen:?}");
        } else {
            // The view was full: it is under everything, and the view moved so that it is the first thing seen.
            below += 1;
            assert_eq!(view.to_screen(rect.origin()), Pt::new(gap, gap), "{i}");
            assert!(placed.iter().all(|e| e.bottom() <= rect.y), "{i}: under everything");
        }
        placed.push(rect);
    }
    assert!(below >= 1, "40 cutouts do not fit in one view");
    assert!(in_view > below, "and some do: {in_view} in view, {below} below");
}

#[test]
fn a_view_that_is_zoomed_and_moved_is_the_place_that_is_looked_in() {
    let mut wb = Whiteboard::new("/work/azul");
    wb.board.set_view(View { pan: Pt::new(1_000.0, 500.0), zoom: 2.0 });
    let region = arm_a_loop(&mut wb, 0);
    let job = wb.send_region_now(region, &bar(), facts(), 2.0, PAGE).expect("a cutout");
    let rect = wb.board.cutout(job.id).expect("there").rect;
    let view = wb.board.view();
    assert_eq!(view, View { pan: Pt::new(1_000.0, 500.0), zoom: 2.0 }, "the view stays where it is when there is room");
    assert_eq!(view.to_screen(rect.origin()), Pt::new(16.0, 16.0), "a margin of 16 px on the screen, whatever the zoom");
}

#[test]
fn a_closed_bar_is_opened_by_what_is_sent_and_a_refusal_leaves_everything_as_it_was() {
    let mut wb = Whiteboard::new("/work/azul");
    wb.bar = BarState { open: false, width: 280.0 };
    let region = arm_a_loop(&mut wb, 0);
    wb.send_region_now(region, &closed_bar(), facts(), 2.0, PAGE).expect("a cutout");
    assert!(wb.bar.open);
    assert_eq!(wb.bar.width, 280.0, "the width it had");
    // A full board refuses, the region stays armed, the bar is not opened for what did not happen, the view is not moved.
    let mut full = Whiteboard::with_limits("/w", Limits { max_items: 1, ..LIMITS });
    let first = arm_a_loop(&mut full, 0);
    full.send_region_now(first, &bar(), facts(), 2.0, PAGE).expect("the one it holds");
    full.bar.open = false;
    let (view, region) = (full.board.view(), arm_a_loop(&mut full, 10_000));
    let refused = full.send_region_now(region, &closed_bar(), facts(), 2.0, PAGE);
    assert_eq!(refused.err(), Some(Notice::TooManyItems { limit: 1 }));
    assert!(full.armed.get(region).is_some(), "the person may make room and try again");
    assert!(!full.bar.open);
    assert_eq!(full.board.view(), view);
}

#[test]
fn a_region_that_is_not_there_or_a_board_that_is_being_read_takes_nothing() {
    let mut wb = Whiteboard::new("/work/azul");
    assert_eq!(wb.send_region_now(77, &bar(), facts(), 2.0, PAGE).err(), Some(Notice::NoSuchItem { id: 77 }));
    let region = arm_a_loop(&mut wb, 0);
    wb.loading = true;
    assert_eq!(wb.send_region_now(region, &bar(), facts(), 2.0, PAGE).err(), Some(Notice::BoardBusy));
    assert!(wb.armed.get(region).is_some());
    assert_eq!(wb.board.len(), 0);
    wb.loading = false;
    assert!(wb.send_region_now(region, &bar(), facts(), 2.0, PAGE).is_ok());
    assert_eq!(wb.send_region_now(region, &bar(), facts(), 2.0, PAGE).err(), Some(Notice::NoSuchItem { id: region }), "used up: a second press of the button sends nothing");
    assert_eq!(wb.board.len(), 1);
}

#[test]
fn how_long_a_loop_stays_armed_is_the_setting_for_the_loops_that_come_after_it() {
    let mut wb = Whiteboard::new("/work/azul");
    let before = arm_a_loop(&mut wb, 0);
    let (settings, notes) = Settings::parse(&|key| (key == crate::whiteboard_settings::KEY_ARMED_SECONDS).then(|| "20".to_string()));
    assert!(notes.is_empty());
    wb.set_settings(settings);
    let after = arm_a_loop(&mut wb, 1_000);
    assert_eq!(wb.armed.get(before).map(|r| r.lifetime_ms), Some(8_000), "a loop that was armed keeps its time");
    assert_eq!(wb.armed.get(after).map(|r| (r.lifetime_ms, r.expires_at_ms())), Some((20_000, 21_000)));
    assert!(!wb.expire(7_999));
    assert!(wb.expire(8_000), "the first is up");
    assert!(wb.armed.get(before).is_none() && wb.armed.get(after).is_some());
    assert!(!wb.expire(20_999) && wb.expire(21_000));
    assert!(wb.armed.is_empty());
}

#[test]
fn a_loop_the_pen_holds_is_not_let_go_of_when_its_time_is_up_until_the_pen_lets_go() {
    let mut wb = Whiteboard::new("/work/azul");
    let region = arm_a_loop(&mut wb, 0);
    let grip = wb.armed.get(region).expect("armed").grip.center();
    assert_eq!(wb.page_press(pointer_at(grip), 7_900), Press::Grip(region));
    assert!(!wb.expire(9_000), "its time is up, the hand is on it");
    assert!(wb.armed.get(region).is_some());
    assert!(wb.escape(), "the press is let go of");
    assert!(wb.expire(9_000), "and then it goes");
    assert!(wb.armed.is_empty());
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
        meta: crate::whiteboard_azboard::Meta { modified_ms: 1, ..Default::default() },
        left_out: 2,
        unreadable_pictures: 3,
    };
    wb.loaded(Ok(loaded), None, 1);
    assert!(matches!(wb.status, Status::Repaired { left_out: 2, pictures: 3 }));
    let _ = Arc::new(0);
}

// ---- the boards of a workspace

const WS: &str = "/work/azul";

fn started(raw: Option<&str>, legacy_file: bool) -> (Whiteboard, IndexStart) {
    let mut wb = Whiteboard::new(WS);
    let start = wb.index_loaded(raw.map(|t| t.as_bytes().to_vec()), legacy_file, "Board 1", 1_000);
    (wb, start)
}

fn index_of(effect: &Effect) -> BoardIndex {
    BoardIndex::from_json(std::str::from_utf8(&effect.bytes).expect("text"), WS).expect("an index").0
}

/// The board is changed (a cutout is put on it) and not kept.
fn change(wb: &mut Whiteboard) {
    let source = Source { rev: String::new(), sha256: String::new(), file: "src/lib.rs".to_string(), page: 0, first_line: 1, last_line: 2, shape: Vec::new(), file_hash: 1, range_hash: 2, lines: Vec::new() };
    wb.board.add_cutout(CutoutDraft::pending(Pt::new(0.0, 0.0), Size::new(100.0, 50.0), source)).expect("room");
    assert!(wb.board.needs_save());
}

/// The board is kept (a save built, its write done).
fn keep(wb: &mut Whiteboard, now: u64) {
    let _job = wb.save_job(now).expect("a save");
    assert!(!wb.save_done(&Ok(())), "nothing was asked meanwhile");
    assert!(!wb.board.needs_save());
}

#[test]
fn a_workspace_with_the_one_board_file_of_the_old_version_opens_it_as_board_one_and_writes_the_list() {
    let (wb, start) = started(None, true);
    assert_eq!(start.load, store::board_name(WS), "the file is read where it is");
    assert_eq!(wb.active_board(), ("b1", "Board 1"));
    assert_eq!(start.effects.len(), 1);
    assert_eq!(start.effects[0].name, boards::index_name(WS));
    assert_eq!(index_of(&start.effects[0]).active().file, store::board_name(WS));
    assert!(start.told.is_none() && start.kept_as.is_none());
    assert_eq!(wb.board_file(), store::board_name(WS), "saves go to that file too");
}

#[test]
fn a_workspace_with_no_file_starts_board_one_in_a_file_of_its_own() {
    let (wb, start) = started(None, false);
    assert_eq!(start.load, boards::file_name(WS, "b1"));
    assert_eq!(index_of(&start.effects[0]).get("b1").map(|b| (b.created_ms, b.name.as_str())), Some((1_000, "Board 1")));
    assert_eq!(wb.boards().boards().count(), 1);
}

#[test]
fn a_list_that_is_there_is_read_and_the_open_board_is_the_one_it_says() {
    let mut idx = BoardIndex::migrated(WS, "Board 1", 5);
    idx.create("Design", 6).expect("created");
    idx.set_active("b2").expect("active");
    let (wb, start) = started(Some(&idx.to_json()), true);
    assert_eq!(wb.active_board(), ("b2", "Design"));
    assert_eq!(start.load, boards::file_name(WS, "b2"));
    assert!(start.effects.is_empty(), "nothing to write: the list is as it was");
    assert_eq!(wb.boards().boards().count(), 2);
}

#[test]
fn a_list_that_is_damaged_is_kept_aside_and_a_new_one_started_and_one_of_a_newer_version_is_never_replaced() {
    let (wb, start) = started(Some("{\"format\": \"azreview-boards/1\", "), true);
    assert_eq!(wb.active_board().0, "b1");
    assert!(matches!(start.told, Some(boards::BoardsError::NotAnIndex(_))));
    let aside = start.kept_as.clone().expect("kept");
    assert_eq!(aside, store::quarantine_name(&boards::index_name(WS), 1_000));
    assert_eq!(start.effects.len(), 2, "the damaged bytes, and the list started anew");
    assert_eq!((start.effects[0].name.as_str(), start.effects[1].name.as_str()), (aside.as_str(), boards::index_name(WS).as_str()));
    assert_eq!(start.effects[0].bytes, b"{\"format\": \"azreview-boards/1\", ".to_vec());
    assert_eq!(start.load, store::board_name(WS), "the old file is still the first board's");
    // A newer version wrote it: nothing is written, nothing will be.
    let (mut newer, start) = started(Some("{\"format\": \"azreview-boards/9\", \"boards\": []}"), true);
    assert_eq!(start.told, Some(boards::BoardsError::TooNew { found: 9 }));
    assert!(start.effects.is_empty() && start.kept_as.is_none());
    change(&mut newer);
    assert!(newer.save_job(2_000).is_none(), "a file of a newer version is not replaced");
}

#[test]
fn a_list_with_entries_that_cannot_be_used_is_repaired_with_a_copy_of_what_it_was() {
    let own = store::board_name(WS);
    let text = format!(
        "{{\"format\": \"azreview-boards/1\", \"boards\": [{{\"id\": \"b1\", \"name\": \"One\", \"file\": \"{own}\"}}, {{\"id\": \"b2\", \"name\": \"Bad\", \"file\": \"../x\"}}]}}"
    );
    let (wb, start) = started(Some(&text), true);
    assert_eq!(wb.boards().boards().count(), 1);
    assert_eq!(start.effects.len(), 2, "a copy of the list as it was, then the list as it is");
    assert_eq!(start.effects[0].bytes, text.as_bytes());
    assert_eq!(start.effects[1].name, boards::index_name(WS));
}

#[test]
fn a_save_goes_to_the_file_of_the_open_board() {
    let (mut wb, _) = started(None, false);
    change(&mut wb);
    let job = wb.save_job(2_000).expect("a save");
    assert_eq!(job.name, boards::file_name(WS, "b1"));
    assert!(!wb.save_done(&Ok(())));
}

#[test]
fn another_board_is_opened_only_when_this_one_is_kept_and_is_read_from_its_own_file() {
    let (mut wb, _) = started(None, false);
    let plan = wb.new_board("Design", 2_000).expect("a new board");
    assert_eq!(wb.active_board(), ("b2", "Design"));
    assert!(plan.load.is_none(), "a new board is empty: nothing to read");
    keep_or_nothing(&mut wb);
    change(&mut wb);
    // Changed and not kept: the board is not left.
    assert_eq!(wb.switch_board("b1", 3_000), Err(Notice::BoardBusy));
    assert_eq!(wb.active_board().0, "b2", "nothing happened");
    // A save is running: not left either.
    let _job = wb.save_job(3_100).expect("a save");
    assert_eq!(wb.switch_board("b1", 3_200), Err(Notice::BoardBusy));
    assert!(!wb.save_done(&Ok(())));
    // Kept: it is.
    let plan = wb.switch_board("b1", 3_300).expect("opened");
    assert_eq!(plan.load, Some(boards::file_name(WS, "b1")));
    assert_eq!(plan.effects.len(), 1);
    assert_eq!(index_of(&plan.effects[0]).active().id, "b1");
    assert!(wb.is_loading(), "until its file has been read");
    assert_eq!(wb.board.len(), 0, "and what is shown meanwhile is empty");
    // Nothing is dropped on a board that is being read: it would be replaced by what is read.
    assert_eq!(wb.switch_board("b2", 3_400), Err(Notice::BoardBusy), "one at a time");
    wb.loaded_none();
    assert!(!wb.is_loading());
    // The board that is open is not opened again.
    assert_eq!(wb.switch_board("b1", 3_500).map(|p| p.load), Ok(None));
    assert_eq!(wb.switch_board("nope", 3_600), Err(Notice::NoSuchBoard));
}

/// Keeps the board if it has anything to keep.
fn keep_or_nothing(wb: &mut Whiteboard) {
    if wb.board.needs_save() {
        keep(wb, 9_000);
    }
}

#[test]
fn a_new_board_is_empty_opens_at_once_is_named_and_is_listed_and_a_name_too_long_is_cut_and_told() {
    let (mut wb, _) = started(None, false);
    let plan = wb.new_board(&"n".repeat(200), 2_000).expect("a new board");
    assert_eq!(wb.active_board().0, "b2");
    assert_eq!(wb.board.len(), 0);
    assert!(!wb.is_loading());
    assert_eq!(index_of(&plan.effects[0]).boards().count(), 2);
    assert!(wb.take_notices().contains(&Notice::BoardNameCut { limit: boards::MAX_NAME_CHARS }));
    // A name with nothing in it: no board.
    assert_eq!(wb.new_board("  ", 2_100), Err(Notice::BoardNameEmpty));
    assert_eq!(wb.boards().boards().count(), 2);
}

#[test]
fn a_duplicate_is_a_copy_of_the_board_as_it_is_in_a_file_of_its_own_and_the_open_board_stays_open() {
    let (mut wb, _) = started(None, false);
    change(&mut wb);
    let plan = wb.duplicate_board("Copy of One", 2_000).expect("a duplicate");
    assert_eq!(wb.active_board().0, "b1", "the board that is open stays");
    assert!(wb.board.needs_save(), "its own changes are still to be kept");
    let copy = plan.copy.expect("a copy to write");
    assert_eq!(copy.name, boards::file_name(WS, "b2"));
    assert!(copy.entries.iter().any(|e| e.name == store::MANIFEST));
    assert_eq!(index_of(&plan.effects[0]).get("b2").map(|b| b.name.clone()), Some("Copy of One".to_string()));
    assert!(plan.load.is_none());
}

#[test]
fn deleting_the_open_board_opens_its_neighbour_from_its_file_and_restoring_brings_it_back() {
    let (mut wb, _) = started(None, false);
    wb.new_board("Two", 2_000).expect("a new board");
    keep_or_nothing(&mut wb);
    wb.switch_board("b1", 2_100).expect("opened");
    wb.loaded_none();
    // The open board is deleted: the next one opens, read from its file.
    let plan = wb.delete_board("b1", 2_200, "Board 3").expect("deleted");
    assert_eq!(wb.active_board().0, "b2");
    assert_eq!(plan.load, Some(boards::file_name(WS, "b2")));
    assert!(wb.is_loading());
    wb.loaded_none();
    assert_eq!(wb.boards().boards().count(), 1);
    // Undo: it is back in the list (not opened).
    let plan = wb.restore_board("b1", 2_300).expect("restored");
    assert_eq!(wb.boards().boards().count(), 2);
    assert_eq!(index_of(&plan.effects[0]).boards().count(), 2);
    assert_eq!(wb.active_board().0, "b2");
    // A board that is not open is deleted without leaving the one that is.
    let plan = wb.delete_board("b1", 2_400, "x").expect("deleted");
    assert!(plan.load.is_none() && !wb.is_loading());
    assert_eq!(wb.active_board().0, "b2");
    // The only board: a new empty one takes its place at once.
    let plan = wb.delete_board("b2", 2_500, "Board 9").expect("deleted");
    assert!(plan.load.is_none(), "the new board is empty");
    assert_eq!(wb.active_board().1, "Board 9");
    assert_eq!(wb.board.len(), 0);
}

#[test]
fn renaming_and_moving_change_the_list_and_nothing_else() {
    let (mut wb, _) = started(None, false);
    wb.new_board("Two", 2_000).expect("a new board");
    keep_or_nothing(&mut wb);
    change(&mut wb);
    let plan = wb.rename_board("b1", "Parser", 2_100).expect("renamed");
    assert_eq!(index_of(&plan.effects[0]).get("b1").map(|b| b.name.clone()), Some("Parser".to_string()));
    assert!(plan.load.is_none() && plan.copy.is_none());
    assert!(wb.board.needs_save(), "the board that is open is as it was");
    let plan = wb.move_board("b2", 0, 2_200).expect("moved");
    assert_eq!(index_of(&plan.effects[0]).boards().map(|b| b.id.clone()).collect::<Vec<_>>(), vec!["b2", "b1"]);
    assert_eq!(wb.rename_board("nope", "x", 2_300), Err(Notice::NoSuchBoard));
    assert_eq!(wb.move_board("nope", 0, 2_300), Err(Notice::NoSuchBoard));
}

#[test]
fn the_list_is_written_again_only_when_it_changed_and_a_kept_board_is_told_to_it() {
    let (mut wb, start) = started(None, false);
    assert!(wb.take_index_effect().is_none(), "the start wrote it");
    drop(start);
    change(&mut wb);
    keep(&mut wb, 4_000);
    let effect = wb.take_index_effect().expect("the board was kept: its time is new");
    assert_eq!(index_of(&effect).get("b1").map(|b| b.modified_ms), Some(4_000));
    assert!(wb.take_index_effect().is_none(), "once");
}

// ---- what a cutout knows of the code, and the board as a file to share

const REV: &str = "0123456789abcdef0123456789abcdef01234567";

fn repo_facts() -> RepoFacts {
    RepoFacts {
        rev: Some(REV.to_string()),
        branch: Some("main".to_string()),
        remote_url: Some("https://github.com/fschutt/azul".to_string()),
        web_url: Some("https://github.com/fschutt/azul".to_string()),
        host: crate::whiteboard_git::Host::GitHub,
        author: Some("Ada Lovelace".to_string()),
        prefix: "examples/azul-review/src".to_string(),
        dirty: None,
    }
}

#[test]
fn a_cutout_names_the_revision_it_was_circled_at_and_the_hash_of_all_the_lines_it_covers() {
    let mut wb = Whiteboard::new("/work/azul");
    wb.set_repo(Some(repo_facts()));
    let job = drop_a_loop(&mut wb, 1_000);
    let c = wb.board.cutout(job.id).expect("the cutout");
    assert_eq!(c.source.rev, REV);
    assert_eq!(c.source.sha256, crate::whiteboard_hash::lines_tagged(&facts().covered));
    // A workspace that is no repository has no revision to name (the hash of the lines is there all the same).
    let mut bare = Whiteboard::new("/work/other");
    let job = drop_a_loop(&mut bare, 1_000);
    let c = bare.board.cutout(job.id).expect("the cutout");
    assert_eq!(c.source.rev, "");
    assert!(crate::whiteboard_hash::is_tagged(&c.source.sha256));
    // The hash is of every line covered, also those the board keeps only part of.
    let many: Vec<String> = (0..300).map(|i| format!("line {i}")).collect();
    let limits = Limits { max_source_lines: 50, ..LIMITS };
    let mut small = Whiteboard::with_limits("/work/small", limits);
    let c = chars();
    let region = small.arm_stroke(0, &pts(&fx::closed_loop()), &lines_of_page(&c), PAGE, 5, 0).expect("armed");
    let facts = SourceFacts { file: "a.rs".to_string(), file_hash: 1, covered: many.clone() };
    let job = small.drop_region(region, Pt::new(1_100.0, 200.0), facts, 2.0, PAGE).expect("a cutout");
    let c = small.board.cutout(job.id).expect("the cutout");
    assert_eq!(c.source.lines.len(), 50);
    assert_eq!(c.source.sha256, crate::whiteboard_hash::lines_tagged(&many));
}

#[test]
fn the_author_is_what_the_setting_says_with_git_s_name_as_the_default_and_never_the_email() {
    let mut wb = Whiteboard::new("/work/azul");
    assert_eq!(wb.author_name(), None, "no repository, no name");
    wb.set_repo(Some(repo_facts()));
    assert_eq!(wb.author_name().as_deref(), Some("Ada Lovelace"));
    let (settings, _) = Settings::parse(&|k| (k == crate::whiteboard_settings::KEY_AUTHOR).then(|| "off".to_string()));
    wb.set_settings(settings);
    assert_eq!(wb.author_name(), None);
    let (settings, _) = Settings::parse(&|k| (k == crate::whiteboard_settings::KEY_AUTHOR).then(|| "The Team".to_string()));
    wb.set_settings(settings);
    assert_eq!(wb.author_name().as_deref(), Some("The Team"));
}

#[test]
fn a_save_is_the_document_of_the_open_board_with_its_name_and_the_repository() {
    let mut wb = Whiteboard::new("/work/azul");
    wb.set_repo(Some(repo_facts()));
    drop_a_loop(&mut wb, 1_000);
    let job = wb.save_job(5_000).expect("a save");
    let manifest = job.entries.iter().find(|e| e.name == store::MANIFEST).expect("the manifest");
    let doc = crate::whiteboard_json::parse(std::str::from_utf8(&manifest.bytes).expect("text")).expect("JSON");
    assert_eq!(doc.get("format").and_then(|f| f.as_str()), Some("azboard/1"));
    let board = doc.get("board").expect("board");
    assert_eq!(board.get("id").and_then(|f| f.as_str()), Some("b1"));
    assert_eq!(board.get("modified_ms").and_then(|f| f.as_u64()), Some(5_000));
    assert_eq!(doc.get("workspace").and_then(|w| w.get("name")).and_then(|f| f.as_str()), Some("azul"));
    assert_eq!(doc.get("author").and_then(|f| f.as_str()), Some("Ada Lovelace"));
    let text = std::str::from_utf8(&manifest.bytes).expect("text");
    assert!(text.contains("\"path\": \"examples/azul-review/src/src/lib.rs\""), "the path in the repository: {text}");
    assert!(text.contains(&format!("/blob/{REV}/examples/azul-review/src/src/lib.rs#L12")), "a link to the line at the revision: {text}");
    assert!(!text.contains("/work/"), "nothing of where the folder is");
}

#[test]
fn an_export_is_the_board_as_it_is_now_with_its_unkept_changes_and_where_the_folder_is() {
    let mut wb = Whiteboard::new("whiteboard-demo");
    wb.set_folder(Some(std::path::PathBuf::from("/work/azul/examples/azul-review/src")));
    wb.set_repo(Some(repo_facts()));
    drop_a_loop(&mut wb, 1_000);
    assert!(wb.board.needs_save());
    let req = wb.export_request(ExportPictures::Beside, None, 9_000);
    assert_eq!(req.data.items.len(), 1, "the change that is not kept yet is in it");
    assert_eq!(req.folder, std::path::PathBuf::from("/work/azul/examples/azul-review/src"));
    assert_eq!((req.meta.board_id.as_str(), req.meta.board_name.as_str(), req.meta.modified_ms), ("b1", "Board 1", 9_000));
    assert_eq!(req.meta.workspace_name, "src", "the folder's name");
    assert_eq!(req.pictures, ExportPictures::Beside);
    assert_eq!(req.meta.author.as_deref(), Some("Ada Lovelace"));
    assert_eq!(wb.export_file_name(), "Board-1.azboard.json");
    // The folder of a host whose workspace is the folder itself is the workspace.
    let plain = Whiteboard::new("/work/azul");
    assert_eq!(plain.export_request(ExportPictures::Embedded, None, 1).folder, std::path::PathBuf::from("/work/azul"));
}

fn loaded(name: &str, items: usize, left_out: usize) -> Loaded {
    let mut data = crate::whiteboard::BoardData::default();
    for i in 0..items {
        let source = Source { rev: String::new(), sha256: String::new(), file: format!("src/f{i}.rs"), page: 0, first_line: 1, last_line: 3, shape: Vec::new(), file_hash: 0, range_hash: 0, lines: Vec::new() };
        data.items.push(Item::Cutout(crate::whiteboard::Cutout {
            id: i as u64 + 1,
            rect: Rect::new(i as f32 * 300.0, 0.0, 200.0, 100.0),
            image: ImageSlot::Failed("no picture".to_string()),
            label: format!("c{i}"),
            source,
        }));
    }
    data.next_id = items as u64 + 1;
    Loaded { data, meta: crate::whiteboard_azboard::Meta { board_name: name.to_string(), ..Default::default() }, left_out, unreadable_pictures: 0 }
}

#[test]
fn a_file_that_was_read_becomes_a_new_board_that_is_open_and_not_kept_yet() {
    let mut wb = Whiteboard::new("/work/azul");
    let _ = wb.index_loaded(None, false, "Board 1", 1_000);
    wb.loaded_none();
    let plan = wb.import_board(loaded("Parser", 2, 0), "Imported", 2_000).expect("imported");
    assert_eq!(wb.active_board(), ("b2", "Parser"), "named for what the file calls it");
    assert_eq!(wb.board.len(), 2);
    assert!(wb.board.needs_save(), "it has no file of its own yet");
    assert_eq!(plan.effects.len(), 1, "the list is written");
    assert!(plan.load.is_none() && plan.copy.is_none());
    assert!(matches!(wb.status, Status::Loaded { items: 2, .. }));
    // It is kept in its own file by the next save, and the one-board file is not touched.
    let job = wb.save_job(3_000).expect("a save");
    assert!(job.name.contains(".b2."), "{}", job.name);
    // A file that calls itself nothing gets the name the app gives; one with items left out says so.
    assert!(!wb.save_done(&Ok(())));
    let _ = wb.import_board(loaded("  ", 1, 3), "Imported", 4_000).expect("imported");
    assert_eq!(wb.active_board(), ("b3", "Imported"));
    assert!(matches!(wb.status, Status::Repaired { left_out: 3, .. }));
    assert!(wb.take_notices().iter().any(|n| matches!(n, Notice::BoardRepaired { dropped: 3 })));
}

#[test]
fn a_file_is_not_taken_while_the_open_board_is_not_kept_and_not_past_the_most_boards() {
    let mut wb = Whiteboard::new("/work/azul");
    let _ = wb.index_loaded(None, false, "Board 1", 1_000);
    wb.loaded_none();
    drop_a_loop(&mut wb, 1_000);
    assert_eq!(wb.can_leave(), Err(Notice::BoardBusy));
    assert_eq!(wb.import_board(loaded("x", 1, 0), "Imported", 2_000).err(), Some(Notice::BoardBusy));
    assert_eq!(wb.boards().boards().count(), 1, "nothing changed");
    // Kept: it goes.
    let _job = wb.save_job(3_000).expect("a save");
    assert!(!wb.save_done(&Ok(())));
    assert_eq!(wb.can_leave(), Ok(()));
    for n in 2..=boards::MAX_BOARDS {
        wb.import_board(loaded("b", 0, 0), "Imported", 4_000).unwrap_or_else(|e| panic!("board {n}: {e:?}"));
        let _ = wb.save_job(5_000);
        let _ = wb.save_done(&Ok(()));
    }
    assert_eq!(wb.import_board(loaded("one more", 0, 0), "Imported", 6_000).err(), Some(Notice::TooManyBoards { limit: boards::MAX_BOARDS }));
}

#[test]
fn a_drop_on_the_header_lands_at_the_top_of_the_board_with_the_whole_cutout_in_view() {
    let mut wb = Whiteboard::new("/work/azul");
    // The bar starts at y 30: a drop at 40 is 10 px into the bar, over its 30 px header.
    let job = drop_a_loop_at(&mut wb, 1_000, Pt::new(1_100.0, 40.0));
    let c = wb.board.cutout(job.id).expect("the cutout");
    let top = wb.board.view().to_screen(c.rect.origin()).y;
    assert!(top >= -0.5, "the cutout's top is on the board: {top}");
    assert!(top < 1.0, "and at the top of it: {top}");
    // A drop well under the header is where it was dropped.
    let low = drop_a_loop_at(&mut wb, 20_000, Pt::new(1_100.0, 400.0));
    let c = wb.board.cutout(low.id).expect("the cutout");
    let centre = wb.board.view().to_screen(c.rect.center());
    assert!((centre.y - (400.0 - 30.0 - HEADER_PX)).abs() < 1.0, "{centre:?}");
}

