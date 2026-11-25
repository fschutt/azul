//! The findings the ink derives: which lines of the file a stroke is over. No window.

use super::*;
use crate::model::InkPoint;

fn file() -> code::SourceFile {
    code::SourceFile {
        path: PathBuf::from("f.rs"),
        display: "f.rs".to_string(),
        lines: (0..120).map(|i| format!("fn f{i}() {{}}")).collect(),
    }
}

fn stroke_at(page: usize, y: f32) -> Stroke {
    let point = |x: f32| InkPoint { x, y, pressure: 0.5, tilt_x: 0.0, tilt_y: 0.0 };
    Stroke {
        page,
        semantic: Semantic::Issue,
        points: vec![point(40.0), point(90.0)],
        id: 1,
        epoch: 0,
    }
}

#[test]
fn a_stroke_over_the_third_line_of_a_page_is_a_finding_on_that_line() {
    // The code starts under the page's padding: the third line (number 3) is the one whose box
    // begins 2 lines below it.
    let y = ui::PAGE_PAD + 2.0 * ui::LINE_H + 5.0;
    let findings = derive_findings(&[stroke_at(0, y)], &file(), &None);
    assert_eq!(findings.len(), 1);
    assert_eq!((findings[0].first_line, findings[0].last_line), (3, 3));
}

#[test]
fn a_stroke_on_a_later_page_counts_from_that_pages_first_line() {
    // Page 1 of a file with 46 lines to a page starts at line 47.
    let y = ui::PAGE_PAD + 1.0;
    let findings = derive_findings(&[stroke_at(1, y)], &file(), &None);
    assert_eq!(findings[0].first_line, code::LINES_PER_PAGE + 1);
}

#[test]
fn ink_in_the_pages_padding_above_the_code_is_on_the_first_line() {
    let findings = derive_findings(&[stroke_at(0, 4.0)], &file(), &None);
    assert_eq!(findings[0].first_line, 1);
}

// ---- undo and redo ----

pub(crate) fn app_with_file() -> AppState {
    AppState::create(PathBuf::new(), vec![file()], RefAny::new(0u8), PathBuf::new())
}

fn kept(id: u64, page: usize) -> Stroke {
    Stroke { id, ..stroke_at(page, ui::PAGE_PAD + 1.0) }
}

#[test]
fn undo_takes_the_last_stroke_back_and_redo_puts_it_where_it_was() {
    let mut s = app_with_file();
    s.keep(kept(1, 0));
    s.keep(kept(2, 0));
    assert_eq!(s.strokes.len(), 2);

    assert!(s.undo(), "something to undo");
    assert_eq!(s.strokes.iter().map(|st| st.id).collect::<Vec<_>>(), vec![1]);
    assert_eq!(s.findings.len(), 1, "the findings follow the ink");

    assert!(s.redo(), "something to redo");
    assert_eq!(s.strokes.iter().map(|st| st.id).collect::<Vec<_>>(), vec![1, 2]);
    assert!(!s.redo(), "nothing more to redo");
}

#[test]
fn undo_with_nothing_drawn_does_nothing() {
    let mut s = app_with_file();
    assert!(!s.undo());
    assert!(!s.redo());
}

#[test]
fn a_new_stroke_ends_the_branch_redo_would_have_brought_back() {
    let mut s = app_with_file();
    s.keep(kept(1, 0));
    assert!(s.undo());
    s.keep(kept(2, 0));
    assert!(!s.redo(), "the stroke undone is gone for good once a new one is drawn");
    assert_eq!(s.strokes.iter().map(|st| st.id).collect::<Vec<_>>(), vec![2]);
}

#[test]
fn undoing_an_erase_puts_the_stroke_back_in_its_place() {
    let mut s = app_with_file();
    s.keep(kept(1, 0));
    s.keep(kept(2, 0));
    s.keep(kept(3, 0));
    assert!(s.erase(1).is_some(), "the middle stroke");
    assert_eq!(s.strokes.iter().map(|st| st.id).collect::<Vec<_>>(), vec![1, 3]);
    assert!(s.undo());
    assert_eq!(s.strokes.iter().map(|st| st.id).collect::<Vec<_>>(), vec![1, 2, 3]);
    assert!(s.redo());
    assert_eq!(s.strokes.iter().map(|st| st.id).collect::<Vec<_>>(), vec![1, 3]);
}

#[test]
fn undo_redraws_the_page_the_stroke_is_on() {
    let mut s = app_with_file();
    s.keep(kept(1, 1));
    let before = s.ink.key(1, (10, 10));
    assert!(s.undo());
    assert_ne!(s.ink.key(1, (10, 10)), before, "page 1's frame is drawn anew");
}

#[test]
fn opening_another_file_forgets_what_could_be_undone() {
    let mut s = AppState::create(
        PathBuf::new(),
        vec![file(), file()],
        RefAny::new(0u8),
        PathBuf::new(),
    );
    s.keep(kept(1, 0));
    s.open_file(1);
    assert!(!s.undo(), "the stroke belongs to the file left");
}

// ---- the eraser ----

#[test]
fn the_eraser_takes_the_stroke_it_touches_not_one_whose_box_it_is_inside() {
    // A long diagonal: its bounding box covers the page's corner, its line does not.
    let diagonal = Stroke {
        points: vec![
            InkPoint { x: 10.0, y: 10.0, pressure: 0.5, tilt_x: 0.0, tilt_y: 0.0 },
            InkPoint { x: 410.0, y: 410.0, pressure: 0.5, tilt_x: 0.0, tilt_y: 0.0 },
        ],
        ..kept(1, 0)
    };
    assert!(diagonal.is_near(210.0, 212.0, 6.0), "on the line");
    assert!(!diagonal.is_near(10.0, 400.0, 6.0), "inside the box, far from the line");
    assert!(diagonal.is_near(10.0, 10.0, 6.0), "at its start");
    assert!(!diagonal.is_near(500.0, 500.0, 6.0), "past its end");

    let mut s = app_with_file();
    s.keep(diagonal);
    assert_eq!(s.eraser_target(0, 10.0, 400.0), None, "the box alone is no hit");
    assert_eq!(s.eraser_target(0, 100.0, 100.0), Some(0));
    assert_eq!(s.eraser_target(1, 100.0, 100.0), None, "another page");
}

#[test]
fn a_one_point_stroke_is_hit_around_its_dab() {
    let dab = Stroke {
        points: vec![InkPoint { x: 50.0, y: 50.0, pressure: 0.5, tilt_x: 0.0, tilt_y: 0.0 }],
        ..kept(1, 0)
    };
    assert!(dab.is_near(53.0, 50.0, 6.0));
    assert!(!dab.is_near(70.0, 50.0, 6.0));
}

#[test]
fn the_eraser_takes_the_topmost_of_two_strokes_it_touches() {
    let mut s = app_with_file();
    s.keep(kept(1, 0));
    s.keep(kept(2, 0));
    // Both lie over each other: the one drawn last is on top.
    assert_eq!(s.eraser_target(0, 60.0, ui::PAGE_PAD + 1.0), Some(1));
}

// ---- another folder ----

#[test]
fn opening_a_folder_replaces_the_files_and_starts_afresh() {
    let mut s = app_with_file();
    s.keep(kept(1, 0));
    s.notes.push(crate::model::Note::listening(1, 0, 10.0, 10.0, "en-US"));
    s.sidebar_open = false;
    let other = code::SourceFile {
        path: PathBuf::from("/other/a.rs"),
        display: "a.rs".to_string(),
        lines: vec!["fn a() {}".to_string()],
    };

    s.open_folder(PathBuf::from("/other"), vec![other]);

    assert_eq!(s.root, PathBuf::from("/other"));
    assert_eq!(s.files.len(), 1);
    assert_eq!(s.current, Some(0), "its first file is open");
    assert!(s.strokes.is_empty() && s.notes.is_empty(), "nothing of the folder left behind");
    assert!(!s.undo(), "nothing to take back from what was left");
    assert!(s.sidebar_open, "the files are out: they are what one picks from first");
    assert!(s.findings.is_empty());
}

#[test]
fn a_folder_without_files_leaves_no_file_open() {
    let mut s = app_with_file();
    s.open_folder(PathBuf::from("/empty"), Vec::new());
    assert_eq!(s.current, None);
    assert!(s.file().is_none());
    assert!(s.sidebar_open, "the way out (Open folder) is in the files");
}

#[test]
fn opening_a_folder_ends_the_dictation_that_listens() {
    let mut s = app_with_file();
    s.voice.running = Some(crate::voice::Running::for_test(1, 1));
    s.open_folder(PathBuf::from("/other"), Vec::new());
    assert!(
        s.voice.running.as_ref().is_some_and(|r| r.stopping.is_some()),
        "what listens writes into the file being left: it is asked to stop"
    );
}

// ---- the meanings and the nib ----

#[test]
fn picking_a_meaning_the_marker_cannot_write_takes_the_pen() {
    let mut s = app_with_file();
    assert_eq!(s.tool, Tool::Marker);
    s.pick(Semantic::Question);
    assert_eq!(s.active, Semantic::Question);
    assert_eq!(s.tool, Tool::Pen, "the marker only highlights: the meaning is written with the pen");
    assert_eq!(s.ink_meaning(), Semantic::Question);
}

#[test]
fn picking_scope_takes_the_marker_and_the_audio_pen_stays_when_it_can_write_the_meaning() {
    let mut s = app_with_file();
    s.tool = Tool::AudioPen;
    s.pick(Semantic::Issue);
    assert_eq!(s.tool, Tool::AudioPen, "the audio pen writes any meaning");
    s.pick(Semantic::Scope);
    assert_eq!(s.tool, Tool::Marker, "scope is the highlighter's");
}

#[test]
fn the_meaning_lit_in_the_title_row_is_the_one_the_ink_will_have() {
    let mut s = app_with_file();
    s.pick(Semantic::Praise);
    s.tool = Tool::Marker;
    assert_eq!(s.ink_meaning(), Semantic::Scope, "the marker writes scope whatever was picked before");
    s.tool = Tool::Pen;
    assert_eq!(s.ink_meaning(), Semantic::Praise, "and the pen takes the meaning back up");
}

// ---- a saved session comes back ----

fn archive_of(file: &str, strokes: Vec<Stroke>, notes: Vec<crate::model::Note>) -> session::Loaded {
    session::Loaded {
        file: file.to_string(),
        strokes,
        notes,
    }
}

fn note(id: u64) -> crate::model::Note {
    crate::model::Note {
        id,
        page: 0,
        x: 10.0,
        y: 20.0,
        text: "from before".to_string(),
        language: String::new(),
        state: crate::model::NoteState::Done,
    }
}

#[test]
fn a_saved_session_comes_back_into_the_open_file_and_the_next_ids_follow_it() {
    let mut s = app_with_file();
    let back = archive_of("f.rs", vec![kept(1, 0), kept(2, 1)], vec![note(5)]);
    assert!(s.adopt(back));
    assert_eq!(s.strokes.len(), 2);
    assert_eq!(s.notes.len(), 1);
    assert_eq!(s.next_stroke_id, 3, "after the archive's ids");
    assert_eq!(s.next_note_id, 6);
    assert!(!s.findings.is_empty(), "the findings follow the ink that came back");
}

#[test]
fn an_archive_of_another_file_is_not_taken() {
    let mut s = app_with_file();
    assert!(!s.adopt(archive_of("other.rs", vec![kept(1, 0)], vec![note(1)])));
    assert!(s.strokes.is_empty() && s.notes.is_empty());
}

#[test]
fn what_was_drawn_before_the_session_came_back_is_kept_under_ids_of_its_own() {
    // The read is on a Thread and a pen is quick: a stroke (id 1) is drawn before the archive
    // (strokes 1 and 2) lands. Both survive, nothing shares an id, and the next stroke is a new id.
    let mut s = app_with_file();
    s.keep(kept(1, 0));
    s.next_stroke_id = 2;
    assert!(s.adopt(archive_of("f.rs", vec![kept(1, 0), kept(2, 0)], Vec::new())));
    let ids: Vec<u64> = s.strokes.iter().map(|st| st.id).collect();
    assert_eq!(ids, vec![1, 2, 3], "the archive's first, then what was drawn since, renumbered");
    assert_eq!(s.next_stroke_id, 4);
    assert!(!s.undo(), "the history before the archive came held the old ids: it starts afresh, no wrong stroke goes");
    assert_eq!(s.strokes.len(), 3);
}

#[test]
fn a_stroke_drawn_after_the_session_came_back_is_not_merged_into_its_burst() {
    let mut s = app_with_file();
    let mut old = kept(1, 0);
    old.epoch = 3;
    assert!(s.adopt(archive_of("f.rs", vec![old], Vec::new())));
    assert!(s.epoch > 3, "a new burst of ink: {}", s.epoch);
}

#[test]
fn the_pages_of_the_ink_that_came_back_are_drawn_anew() {
    let mut s = app_with_file();
    let before = s.ink.key(1, (10, 10));
    assert!(s.adopt(archive_of("f.rs", vec![kept(1, 1)], Vec::new())));
    assert_ne!(s.ink.key(1, (10, 10)), before);
}
