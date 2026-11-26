//! The findings the ink derives: which lines of the file a stroke is over. No window.

use super::*;
use crate::model::InkPoint;

fn file() -> code::SourceFile {
    code::SourceFile {
        path: PathBuf::from("f.rs"),
        display: "f.rs".to_string(),
        lines: (0..120).map(|i| format!("fn f{i}() {{}}")).collect(),
        big: None,
    }
}

/// A file of the same lines as `file()` under another name.
pub(crate) fn file_named(name: &str) -> code::SourceFile {
    code::SourceFile { path: PathBuf::from(name), display: name.to_string(), ..file() }
}

fn stroke_at(page: usize, y: f32) -> Stroke {
    let point = |x: f32| InkPoint { x, y, pressure: 0.5, tilt_x: 0.0, tilt_y: 0.0, t_ms: 0 };
    Stroke {
        page,
        semantic: Semantic::Issue,
        points: vec![point(40.0), point(90.0)],
        id: 1,
        epoch: 0,
        started_unix_ms: 0,
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

/// A folder of the disk with these files (under the system's temporary folder, one per test name, process and thread: tests
/// that name the same folder and run at once do not remove each other's files).
pub(crate) fn scratch_folder(name: &str, files: &[(&str, &str)]) -> PathBuf {
    let thread = format!("{:?}", std::thread::current().id()).replace(|c: char| !c.is_ascii_alphanumeric(), "");
    let dir = std::env::temp_dir().join(format!("azreview-test-{}-{thread}-{name}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("a folder under the temporary one");
    for (rel, text) in files {
        let path = dir.join(rel);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).expect("a folder");
        }
        std::fs::write(&path, text).expect("a file");
    }
    dir
}

/// A file whose line 12 has a word of ten characters where the fixtures' slash is drawn (column 33 of the page: x 301.8 to
/// 367.8), another word of three after it, and a second page (the lines after the 46th).
pub(crate) fn app_with_slash_file() -> AppState {
    let mut lines: Vec<String> = (0..120).map(|i| format!("fn f{i}() {{}}")).collect();
    lines[11] = format!("{}abcdefghij klm = 1;", " ".repeat(33));
    lines[code::LINES_PER_PAGE + 11] = format!("{}pqrstuvwxy zzz = 2;", " ".repeat(33));
    let f = code::SourceFile { path: PathBuf::from("f.rs"), display: "f.rs".to_string(), lines, big: None };
    AppState::create(PathBuf::new(), vec![f], RefAny::new(0u8), PathBuf::new())
}

/// Draws `f` on `page` as the pen does and returns how it ended: the press at its first sample, a move for each other
/// sample (the times as the fixture has them), the release.
pub(crate) fn draw_fixture(s: &mut AppState, f: &crate::stroke_fixtures::Fixture, page: usize) -> Option<crate::pointer::Ended> {
    let point = |p: &crate::stroke_fixtures::Sample| InkPoint {
        x: p.x,
        y: p.y,
        pressure: p.pressure,
        tilt_x: 0.0,
        tilt_y: 0.0,
        t_ms: p.t_ms,
    };
    s.stroke_begin_at(page, point(&f.samples[0]), true, 1_764_000_000_000);
    for p in &f.samples[1..] {
        s.stroke_move_at(point(p), true);
    }
    s.stroke_end()
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
        vec![file(), file_named("g.rs")],
        RefAny::new(0u8),
        PathBuf::new(),
    );
    s.keep(kept(1, 0));
    assert!(s.open_path("g.rs"));
    assert!(!s.undo(), "the stroke belongs to the file left");
}

// ---- the eraser ----

#[test]
fn the_eraser_takes_the_stroke_it_touches_not_one_whose_box_it_is_inside() {
    // A long diagonal: its bounding box covers the page's corner, its line does not.
    let diagonal = Stroke {
        points: vec![
            InkPoint { x: 10.0, y: 10.0, pressure: 0.5, tilt_x: 0.0, tilt_y: 0.0, t_ms: 0 },
            InkPoint { x: 410.0, y: 410.0, pressure: 0.5, tilt_x: 0.0, tilt_y: 0.0, t_ms: 0 },
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
        points: vec![InkPoint { x: 50.0, y: 50.0, pressure: 0.5, tilt_x: 0.0, tilt_y: 0.0, t_ms: 0 }],
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
    s.panels.tree_open = false;
    let folder = scratch_folder("open-folder", &[("a.rs", "fn a() {}\n")]);

    s.open_folder(folder.clone());

    assert_eq!(s.root, folder);
    assert_eq!(s.tree.file_count(), 1);
    assert_eq!(s.file().map(|f| f.display.as_str()), Some("a.rs"), "its first file is open");
    assert!(s.strokes.is_empty() && s.notes.is_empty(), "nothing of the folder left behind");
    assert!(!s.undo(), "nothing to take back from what was left");
    assert!(!s.panels.tree_open, "a new folder leaves the tree as the person had it");
    assert!(s.findings.is_empty());
}

#[test]
fn a_folder_without_files_leaves_no_file_open() {
    let mut s = app_with_file();
    let folder = scratch_folder("empty", &[]);
    s.open_folder(folder);
    assert_eq!(s.open_node, None);
    assert!(s.file().is_none());
    assert!(!s.has_entries(), "the empty page says there is nothing to review");
    assert!(s.panels.tree_open == AppState::create(PathBuf::new(), Vec::new(), RefAny::new(0u8), PathBuf::new()).panels.tree_open,
        "the way out (Open folder) is in the empty page itself; the tree is as it was");
}

#[test]
fn opening_a_folder_ends_the_dictation_that_listens() {
    let mut s = app_with_file();
    s.voice.running = Some(crate::voice::Running::for_test(1, 1));
    s.open_folder(scratch_folder("listens", &[]));
    assert!(
        s.voice.running.as_ref().is_some_and(|r| r.stopping.is_some()),
        "what listens writes into the file being left: it is asked to stop"
    );
}

// ---- the meanings and the nib ----

#[test]
fn picking_a_pen_takes_its_meaning_and_the_nib_it_starts_with() {
    let mut s = app_with_file();
    assert_eq!(s.tool(), Tool::Marker, "the scope pen with the marker is in hand at the start");
    s.pick(Semantic::Question);
    assert_eq!(s.ink_meaning(), Semantic::Question);
    assert_eq!(s.tool(), Tool::Pen, "the marker only highlights: the question pen writes with the pen");
}

#[test]
fn each_pen_keeps_its_nib_while_another_is_in_hand() {
    let mut s = app_with_file();
    s.pick(Semantic::Issue);
    s.pens.cycle(false);
    assert_eq!(s.tool(), Tool::AudioPen);
    s.pick(Semantic::Scope);
    assert_eq!(s.tool(), Tool::Marker, "scope is the highlighter's");
    s.pick(Semantic::Issue);
    assert_eq!(s.tool(), Tool::AudioPen, "the issue pen still has its audio nib");
}

#[test]
fn the_pen_lit_in_the_top_bar_is_the_one_the_ink_will_have() {
    let mut s = app_with_file();
    s.pick(Semantic::Praise);
    assert_eq!(s.ink_meaning(), Semantic::Praise);
    s.pick(Semantic::Scope);
    assert_eq!(s.ink_meaning(), Semantic::Scope);
}

#[test]
fn leaving_the_audio_nib_ends_the_clip_that_records() {
    let mut s = app_with_file();
    s.pick(Semantic::Issue);
    s.pens.cycle(false);
    s.recording = Some(crate::model::VoiceClip::default());
    s.pick(Semantic::Scope);
    assert!(s.recording.is_none(), "the scope pen's marker does not record");
    assert_eq!(s.clips.len(), 1, "what was recorded is kept");
}

// ---- a saved session comes back ----

fn archive_of(file: &str, strokes: Vec<Stroke>, notes: Vec<crate::model::Note>) -> session::Loaded {
    session::Loaded {
        file: file.to_string(),
        strokes,
        notes,
        slashes: Vec::new(),
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
        talk: crate::conversation::Conversation::default(),
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

// ---- the first screen ----

#[test]
fn a_new_window_has_the_tree_beside_the_page_not_over_it() {
    let s = app_with_file();
    assert!(s.panels.tree_open, "the files are there to pick from");
    assert!(!s.panels.right_open, "the whiteboard's panel is a click away");
    assert_eq!(s.slide, crate::layout::Slide::settled(&s.panels), "nothing slides at the start");
    assert!(!s.sliding && s.grip.is_none());
}

#[test]
fn the_folder_dialog_opens_once_at_a_time() {
    // The menu's Mod+O and the key handler both hear the key: one dialog.
    let mut s = app_with_file();
    assert!(s.ask_for_folder(), "the first request opens it");
    assert!(!s.ask_for_folder(), "a second one while it is open does not");
    s.folder_dialog_closed();
    assert!(s.ask_for_folder(), "after it was answered it can be opened again");
}

#[test]
fn the_window_asks_for_the_gpu_like_the_map_editor_does() {
    // The engine falls back to the CPU renderer by itself where there is no usable GPU.
    assert!(matches!(window_renderer().hw_accel, azul::window::HwAcceleration::Enabled));
}

#[test]
fn the_about_box_names_the_build_the_binary_is_made_from() {
    // `build.rs` puts the commit there (`unknown` without git): a report names its binary.
    assert!(ABOUT.version.starts_with(env!("CARGO_PKG_VERSION")), "{}", ABOUT.version);
    assert!(ABOUT.version.contains("(build ") && ABOUT.version.ends_with(')'), "{}", ABOUT.version);
}

// ---- the quick slash: words deleted without a wait ----

use crate::stroke_fixtures as fx;

#[test]
fn undo_takes_a_deletion_back_and_redo_deletes_the_same_words_again() {
    let mut s = app_with_slash_file();
    draw_fixture(&mut s, &fx::slash(), 0);
    assert_eq!(s.slashes.len(), 1);
    let was = s.slashes[0].clone();

    assert!(s.undo(), "a deletion is an edit");
    assert!(s.slashes.is_empty());
    assert!(s.hidden_ranges(0).iter().all(Vec::is_empty), "the word is back on the page");
    assert_eq!(s.redo_stack.len(), 1);

    assert!(s.redo());
    assert_eq!(s.slashes, vec![was]);
    assert_eq!(s.hidden_ranges(0)[11], vec![(33, 43)]);
}

#[test]
fn undo_takes_back_the_last_edit_whether_it_was_ink_or_a_deletion() {
    let mut s = app_with_slash_file();
    s.keep(kept(1, 0));
    draw_fixture(&mut s, &fx::slash(), 0);
    assert_eq!((s.strokes.len(), s.slashes.len()), (1, 1));
    assert!(s.undo());
    assert_eq!((s.strokes.len(), s.slashes.len()), (1, 0), "the deletion first");
    assert!(s.undo());
    assert_eq!((s.strokes.len(), s.slashes.len()), (0, 0), "then the ink");
}

#[test]
fn a_new_stroke_ends_the_branch_redo_would_have_brought_back_a_deletion_from() {
    let mut s = app_with_slash_file();
    draw_fixture(&mut s, &fx::slash(), 0);
    s.undo();
    assert_eq!(s.redo_stack.len(), 1);
    s.keep(kept(9, 0));
    assert!(s.redo_stack.is_empty());
    assert!(!s.redo());
}

#[test]
fn the_words_of_a_deletion_are_hidden_on_their_own_page_only() {
    let mut s = app_with_slash_file();
    draw_fixture(&mut s, &fx::slash(), 1);
    assert_eq!(s.slashes[0].page, 1);
    assert_eq!(s.slashes[0].words[0].line, code::LINES_PER_PAGE + 12, "the line is the file's, not the page's");
    assert_eq!(s.slashes[0].words[0].text, "pqrstuvwxy");
    assert_eq!(s.hidden_ranges(1)[11], vec![(33, 43)]);
    assert!(s.hidden_ranges(0).iter().all(Vec::is_empty));
}

#[test]
fn opening_another_file_leaves_the_deletions_behind() {
    let mut s = app_with_slash_file();
    draw_fixture(&mut s, &fx::slash(), 0);
    s.tree = crate::tree::Tree::from_paths(&["f.rs", "g.rs"]);
    s.source = std::sync::Arc::new({
        let mut m = crate::symbols_source::MemSource::new();
        m.put("f.rs", b"fn f() {}\n".to_vec());
        m.put("g.rs", b"fn g() {}\n".to_vec());
        m
    });
    assert!(s.open_path("g.rs"));
    assert!(s.slashes.is_empty());
    assert!(s.history.is_empty());
}

#[test]
fn a_session_that_comes_back_brings_its_deletions_and_the_ids_of_what_is_here_follow_them() {
    let mut s = app_with_slash_file();
    draw_fixture(&mut s, &fx::slash(), 0);
    let word = s.slashes[0].clone();
    let mut loaded = archive_of("f.rs", vec![kept(5, 0)], Vec::new());
    loaded.slashes = vec![crate::deletion::Deletion { id: 7, ..word.clone() }];
    assert!(s.adopt(loaded));
    let ids: Vec<u64> = s.slashes.iter().map(|d| d.id).collect();
    assert_eq!(ids.len(), 2);
    assert_eq!(ids[0], 7, "the archive's own keep their ids");
    assert!(ids[1] > 7, "what was drawn here since is renumbered past them: {ids:?}");
    assert!(s.next_stroke_id > 7);
}

#[test]
fn deletions_are_bounded() {
    let mut s = app_with_slash_file();
    let filler = crate::deletion::Deletion { id: 1, page: 0, started_unix_ms: 0, points: Vec::new(), words: Vec::new() };
    s.slashes = vec![filler; crate::deletion::MAX_DELETIONS];
    // at the cap a slash is ordinary ink: nothing is lost and nothing grows
    let ended = draw_fixture(&mut s, &fx::slash(), 0);
    assert_eq!(ended.map(|e| e.up), Some(crate::pointer::Up::Kept));
    assert_eq!(s.slashes.len(), crate::deletion::MAX_DELETIONS);
    assert_eq!(s.strokes.len(), 1);
}

// ---- the tree of a folder on a disk: listed as it is opened ----

fn names(s: &AppState) -> Vec<String> {
    s.rows.iter().map(|r| r.name.clone()).collect()
}

fn on_disk(name: &str, files: &[(&str, &str)]) -> AppState {
    AppState::on_disk(scratch_folder(name, files), RefAny::new(0u8), PathBuf::new())
}

/// [`on_disk`] for the other test modules.
pub(crate) fn on_disk_for_state(name: &str, files: &[(&str, &str)]) -> AppState {
    on_disk(name, files)
}

#[test]
fn a_folder_of_a_disk_lists_its_root_only_and_opens_the_first_file_of_it() {
    let s = on_disk(
        "lazy",
        &[("README.md", "# hi\n"), ("src/lib.rs", "fn a() {}\n"), ("src/deep/b.rs", "fn b() {}\n")],
    );
    assert_eq!(names(&s), vec!["src", "README.md"], "the folder is closed and was not listed");
    assert_eq!(s.file().map(|f| f.display.as_str()), Some("README.md"));
    let src = s.tree.find("src").expect("src");
    assert!(!s.tree.is_listed(src));
    assert_eq!(s.tree.file_count(), 1, "only the root's files are known");
    assert!(s.has_entries());
}

#[test]
fn opening_a_folder_gives_the_job_that_lists_it_and_its_listing_fills_the_rows() {
    let mut s = on_disk("open-dir", &[("README.md", "x\n"), ("src/lib.rs", "fn a() {}\n"), ("src/deep/b.rs", "fn b() {}\n")]);
    let src = s.tree.find("src").expect("src");
    let job = s.toggle_dir(src).expect("a folder that was never listed asks for it");
    assert_eq!((job.dir.as_str(), job.workspace), ("src", s.workspace));
    assert!(s.rows.iter().any(|r| r.name == "src" && r.expanded && r.loading), "the row waits");

    let entries = job.source.list(&job.dir).expect("the folder is there");
    assert!(s.take_listing(job.workspace, "src", Ok(entries)));
    assert_eq!(names(&s), vec!["src", "deep", "lib.rs", "README.md"]);
    assert!(s.rows.iter().all(|r| !r.loading));

    // closed and opened again: no listing is asked for
    assert_eq!(s.toggle_dir(src).map(|j| j.dir), None);
    assert_eq!(names(&s), vec!["src", "README.md"]);
    assert_eq!(s.toggle_dir(src).map(|j| j.dir), None, "it is listed already");
    assert_eq!(names(&s), vec!["src", "deep", "lib.rs", "README.md"]);
}

#[test]
fn a_listing_that_comes_back_after_another_folder_was_chosen_is_not_taken() {
    let mut s = on_disk("stale", &[("a.rs", "x\n"), ("src/lib.rs", "x\n")]);
    let src = s.tree.find("src").expect("src");
    let job = s.toggle_dir(src).expect("job");
    let late = job.source.list("src").expect("listed");
    s.open_folder(scratch_folder("stale-other", &[("b.rs", "x\n"), ("src/other.rs", "x\n")]));
    assert_ne!(job.workspace, s.workspace);
    assert!(!s.take_listing(job.workspace, "src", Ok(late)));
    let src = s.tree.find("src").expect("the other folder's src");
    assert!(!s.tree.is_listed(src), "what was listed for the folder left is not in this tree");
}

#[test]
fn a_listing_that_failed_ends_the_wait_and_leaves_the_folder_open_and_empty() {
    let mut s = on_disk("failed", &[("src/lib.rs", "x\n"), ("a.rs", "x\n")]);
    let src = s.tree.find("src").expect("src");
    let job = s.toggle_dir(src).expect("job");
    assert!(s.take_listing(job.workspace, "src", Err(crate::symbols_source::ReadError::NotFound)));
    assert!(s.rows.iter().all(|r| !r.loading));
    assert_eq!(names(&s), vec!["src", "a.rs"]);
}

#[test]
fn a_file_that_is_no_text_or_too_large_is_not_shown_and_the_window_says_why() {
    crate::l10n::in_english();
    let mut s = on_disk("not-shown", &[("a.rs", "fn a() {}\n"), ("bin.rs", "")]);
    std::fs::write(s.root.join("bin.rs"), b"\x7fELF\0\0\0").expect("write");
    let relist = s.source.list("").expect("list");
    s.take_listing(s.workspace, "", Ok(relist));
    assert!(!s.open_path("bin.rs"));
    assert!(s.problem.as_deref().is_some_and(|p| p.contains("bin.rs") && p.contains("no text")), "{:?}", s.problem);
    assert_eq!(s.file().map(|f| f.display.as_str()), Some("a.rs"), "the file that was open stays open");

    std::fs::write(s.root.join("big.rs"), vec![b'a'; code::MAX_OPEN_BYTES + 1]).expect("write");
    let relist = s.source.list("").expect("list");
    s.take_listing(s.workspace, "", Ok(relist));
    s.problem = None;
    assert!(!s.open_path("big.rs"));
    assert!(s.problem.as_deref().is_some_and(|p| p.contains("big.rs") && p.contains("MB")), "{:?}", s.problem);
}

#[test]
fn a_path_the_tree_does_not_know_opens_nothing() {
    let mut s = on_disk("unknown", &[("a.rs", "x\n"), ("src/lib.rs", "x\n")]);
    assert!(!s.open_path("src/lib.rs"), "the folder was not listed: the tree does not know the file");
    assert!(!s.open_path("nothing.rs"));
    assert!(!s.open_path("src"), "a folder is not a file");
    assert_eq!(s.file().map(|f| f.display.as_str()), Some("a.rs"));
}

#[test]
fn the_lines_of_the_open_file_are_counted_in_the_tree() {
    let s = on_disk("counted", &[("a.rs", "one\ntwo\nthree\n")]);
    let a = s.tree.find("a.rs").expect("a.rs");
    assert_eq!(s.tree.node(a).and_then(|n| n.lines), Some(3));
}

#[test]
fn expanding_everything_lists_every_folder_down_to_its_files_and_says_how_much_it_read() {
    let mut s = on_disk(
        "expand-all",
        &[("README.md", "x\n"), ("src/lib.rs", "x\n"), ("src/deep/b.rs", "x\n"), ("src/deep/er/c.rs", "x\n"), ("tests/t.rs", "x\n")],
    );
    assert_eq!(names(&s), vec!["src", "tests", "README.md"]);
    let read = s.expand_all();
    assert_eq!(read, 4, "src, deep, er and tests were listed");
    assert_eq!((s.tree.file_count(), s.tree.dir_count()), (5, 4));
    assert_eq!(
        names(&s),
        vec!["src", "deep", "er", "c.rs", "b.rs", "lib.rs", "tests", "t.rs", "README.md"]
    );
    assert_eq!(s.expand_all(), 0, "nothing is left to list");
}

#[test]
fn a_first_file_that_cannot_be_shown_is_passed_over_at_the_start_without_a_word() {
    let folder = scratch_folder("first-bad", &[("a.rs", ""), ("b.rs", "fn b() {}\n")]);
    std::fs::write(folder.join("a.rs"), b"\x7fELF\0\0\0").expect("write");
    let s = AppState::on_disk(folder, RefAny::new(0u8), PathBuf::new());
    assert_eq!(s.file().map(|f| f.display.as_str()), Some("b.rs"));
    assert!(s.problem.is_none(), "the reviewer did not ask for a.rs: {:?}", s.problem);
}

// ---- what has been looked at ----

/// A folder of a disk with `a.rs` of 100 lines (three pages) and `b.rs` of 10, `a.rs` open.
fn reviewing_two() -> AppState {
    let a: String = (0..100).map(|i| format!("fn a{i}() {{}}\n")).collect();
    let b: String = (0..10).map(|i| format!("fn b{i}() {{}}\n")).collect();
    on_disk("progress", &[("a.rs", a.as_str()), ("b.rs", b.as_str())])
}

/// Looks at page `page` of the open file long enough for it to be seen (the strip scrolled to it, a viewport of 1,200).
fn read_page(s: &mut AppState, page: usize) {
    for _ in 0..4 {
        s.watch_pages(page as f32 * ui::page_stride(), 1_200.0, 400);
    }
}

fn id_of(s: &AppState, path: &str) -> crate::tree::NodeId {
    s.tree.find(path).expect(path)
}

#[test]
fn a_file_all_of_whose_pages_were_looked_at_is_reviewed_and_the_tree_marks_it_green() {
    let mut s = reviewing_two();
    let a = id_of(&s, "a.rs");
    read_page(&mut s, 0);
    read_page(&mut s, 1);
    s.refresh_rollup();
    assert_eq!(s.file_mark(a), None, "two pages of three");
    read_page(&mut s, 2);
    s.refresh_rollup();
    assert_eq!(s.file_mark(a), Some(chips::State::Done), "every page seen, no conversation");
    let root = s.totals_of(s.tree.root());
    assert_eq!((root.files, root.files_reviewed, root.lines, root.lines_reviewed, root.lines_seen), (2, 1, 100, 100, 100));
    assert_eq!(root.unknown_files, 1, "b.rs has not been counted yet");
    assert!(root.is_partial());
    assert!(s.progress.is_dirty(), "what was seen is to be kept");
}

#[test]
fn a_page_that_carries_ink_is_seen_without_being_looked_at() {
    let mut s = reviewing_two();
    s.keep(kept(1, 2));
    assert_eq!(s.progress.get("a.rs").map(|e| e.seen.ranges()), Some(vec![(2, 2)]));
    s.slashes.clear();
    let mut t = reviewing_two();
    t.keep(kept(1, 0));
    t.undo();
    assert!(t.progress.get("a.rs").is_some_and(|e| e.seen.is_seen(0)), "taking the ink back does not unsee the page");
}

#[test]
fn a_conversation_that_waits_or_works_keeps_a_file_from_being_reviewed_and_shows_its_own_colour() {
    let mut s = reviewing_two();
    for p in 0..3 {
        read_page(&mut s, p);
    }
    let a = id_of(&s, "a.rs");
    let chip = |state| chips::Chip { id: 1, file: "a.rs".to_string(), page: 0, line: 3, state, title: "why".to_string() };
    s.chips.push(chip(chips::State::Suggested));
    s.chips_changed();
    s.refresh_rollup();
    assert_eq!(s.file_mark(a), Some(chips::State::Suggested));
    assert_eq!(s.totals_of(a).files_reviewed, 0);
    s.chips.set_state(1, chips::State::Working);
    s.refresh_rollup();
    assert_eq!(s.file_mark(a), Some(chips::State::Working));
    s.chips.set_state(1, chips::State::Done);
    s.refresh_rollup();
    assert_eq!(s.file_mark(a), Some(chips::State::Done), "resolved, and every page seen");
}

#[test]
fn conversations_all_done_are_not_green_while_a_page_was_never_seen() {
    let mut s = reviewing_two();
    read_page(&mut s, 0);
    s.chips.push(chips::Chip { id: 1, file: "a.rs".to_string(), page: 0, line: 3, state: chips::State::Done, title: "x".to_string() });
    s.refresh_rollup();
    assert_eq!(s.file_mark(id_of(&s, "a.rs")), None);
}

#[test]
fn the_reviewer_can_hold_a_file_back_and_let_it_go_again() {
    let mut s = reviewing_two();
    for p in 0..3 {
        read_page(&mut s, p);
    }
    s.refresh_rollup();
    let a = id_of(&s, "a.rs");
    assert_eq!(s.file_mark(a), Some(chips::State::Done));
    assert!(s.toggle_held("a.rs"), "held back");
    assert_eq!(s.file_mark(a), None, "not reviewed, whatever the pages say");
    assert_eq!(s.totals_of(s.tree.root()).files_reviewed, 0);
    assert!(!s.toggle_held("a.rs"), "let go");
    assert_eq!(s.file_mark(a), Some(chips::State::Done));
}

#[test]
fn nothing_is_seen_while_the_window_is_not_the_one_the_reviewer_is_at() {
    let mut s = reviewing_two();
    s.window_active = false;
    for _ in 0..10 {
        assert!(!s.watch_pages(0.0, 1_200.0, 400));
    }
    assert_eq!(s.progress.get("a.rs").map(|e| e.seen.count()).unwrap_or(0), 0);
    s.window_active = true;
    read_page(&mut s, 0);
    assert!(s.progress.get("a.rs").is_some_and(|e| e.seen.is_seen(0)));
}

#[test]
fn a_file_that_is_not_open_is_not_looked_at() {
    let mut s = AppState::create(PathBuf::new(), Vec::new(), RefAny::new(0u8), PathBuf::new());
    assert!(!s.watch_pages(0.0, 1_200.0, 400));
    s.mark_page_seen(0);
    assert!(s.progress.is_empty());
}

#[test]
fn the_totals_of_a_folder_follow_its_files_once_they_are_counted() {
    let mut s = reviewing_two();
    read_page(&mut s, 0);
    s.refresh_rollup();
    let root = s.totals_of(s.tree.root());
    assert_eq!((root.lines, root.lines_seen, root.files_reviewed), (100, 46, 0));
    // b.rs is counted (the background count tells it): the totals have its lines
    let b = id_of(&s, "b.rs");
    s.tree.set_lines(b, 10);
    s.refresh_rollup();
    let root = s.totals_of(s.tree.root());
    assert_eq!((root.lines, root.unknown_files), (110, 0));
    assert!(!root.is_partial());
}

// ---- page breaks are stable: lines put in grow the page that has them ----

fn suggest_insert(s: &mut AppState, id: u64, page: usize, after_line: usize, rows: &[&str], decision: conversation::Decision) {
    let mut note = Note {
        id,
        page,
        x: 0.0,
        y: 0.0,
        text: String::new(),
        language: String::new(),
        state: model::NoteState::Done,
        talk: conversation::Conversation::default(),
    };
    note.talk.proposals.push(conversation::Proposal {
        suggestion: agent_review::Suggestion::InsertLines { after_line, lines: rows.iter().map(|r| (*r).to_string()).collect() },
        decision,
    });
    s.notes.push(note);
}

#[test]
fn a_page_nobody_put_lines_in_has_the_plain_map_and_the_strip_is_as_tall_as_a_page() {
    let s = AppState::create(PathBuf::new(), vec![file()], RefAny::new(0u8), PathBuf::new());
    for page in 0..3 {
        assert!(s.page_map(page).is_plain(), "page {page}");
    }
    assert_eq!(s.most_extra_rows(), 0);
}

#[test]
fn lines_the_suggestions_would_put_in_grow_the_page_that_has_the_line_they_follow_and_no_other() {
    let mut s = AppState::create(PathBuf::new(), vec![file()], RefAny::new(0u8), PathBuf::new());
    // the file has 120 lines: pages of 46, 46 and 28
    suggest_insert(&mut s, 1, 0, 4, &["a", "b"], conversation::Decision::Waiting);
    suggest_insert(&mut s, 2, 1, 47, &["c"], conversation::Decision::Accepted);
    suggest_insert(&mut s, 3, 1, 50, &["refused"], conversation::Decision::Rejected);
    suggest_insert(&mut s, 4, 2, 200, &["past the end"], conversation::Decision::Waiting);
    assert_eq!(s.page_map(0).extra_rows(), 2);
    assert_eq!(s.page_map(1).extra_rows(), 1, "the refused one is none");
    assert!(s.page_map(2).is_plain(), "a line the file does not have grows no page");
    assert_eq!(s.most_extra_rows(), 2, "the strip is as tall as the tallest page");
    // the first line of every page is the line it was: page breaks do not move
    assert_eq!(code::SourceFile::first_line_of(1), 47);
    assert_eq!(s.page_map(1).block_top(0), ui::PAGE_PAD + ui::LINE_H, "after the first line of its page");
    // a point of page 0 below the lines put in goes down by them; one above stays
    let map = s.page_map(0);
    assert_eq!(map.to_display(ui::PAGE_PAD + 3.0 * ui::LINE_H + 2.0), ui::PAGE_PAD + 3.0 * ui::LINE_H + 2.0);
    assert_eq!(map.to_display(ui::PAGE_PAD + 4.0 * ui::LINE_H + 2.0), ui::PAGE_PAD + 6.0 * ui::LINE_H + 2.0);
}

// ---- the ink an agent is told of ----

#[test]
fn the_agent_is_told_of_the_ink_near_the_note_it_is_asked_about_and_of_none_for_a_note_that_is_not_there() {
    let mut s = app_with_file();
    let near = stroke_at(0, ui::PAGE_PAD + 20.0);
    let mut far = stroke_at(0, ui::PAGE_PAD + 600.0);
    far.id = 2;
    s.strokes.push(near);
    s.strokes.push(far);
    s.notes.push(Note {
        id: 9,
        page: 0,
        x: 60.0,
        y: ui::PAGE_PAD + 30.0,
        text: "this".to_string(),
        language: String::new(),
        state: model::NoteState::Done,
        talk: conversation::Conversation::default(),
    });
    let told = s.strokes_for_request(9);
    assert_eq!(told.iter().map(|t| t.id).collect::<Vec<_>>(), vec![1], "the near stroke, not the far one");
    assert_eq!(told[0].samples.len(), 2);
    assert!(s.strokes_for_request(404).is_empty());
}

// ---- the revision a conversation begins at ----

#[test]
fn a_conversation_that_begins_keeps_the_revision_the_workspace_is_at_then_and_none_where_there_is_no_repository() {
    let dir = std::env::temp_dir().join(format!("azreview-start-{}-{:?}", std::process::id(), std::thread::current().id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.join(".git/refs/heads")).expect("a repository");
    std::fs::write(dir.join(".git/HEAD"), "ref: refs/heads/main\n").expect("HEAD");
    std::fs::write(dir.join(".git/refs/heads/main"), "0123456789abcdef0123456789abcdef01234567\n").expect("a ref");
    let mut s = AppState::create(dir.clone(), vec![file()], RefAny::new(0u8), PathBuf::new());
    let note = |id| Note {
        id,
        page: 0,
        x: 0.0,
        y: 0.0,
        text: "this".to_string(),
        language: String::new(),
        state: model::NoteState::Done,
        talk: conversation::Conversation::default(),
    };
    s.start_conversation(note(1));
    assert_eq!(s.notes[0].talk.base.as_deref(), Some("0123456789abcdef0123456789abcdef01234567"));
    // the branch moves on: the one that began keeps the revision it began at, the next one has the new one
    std::fs::write(dir.join(".git/refs/heads/main"), "fedcba9876543210fedcba9876543210fedcba98\n").expect("a ref");
    s.start_conversation(note(2));
    assert_eq!(s.notes[0].talk.base.as_deref(), Some("0123456789abcdef0123456789abcdef01234567"));
    assert_eq!(s.notes[1].talk.base.as_deref(), Some("fedcba9876543210fedcba9876543210fedcba98"));
    let _ = std::fs::remove_dir_all(&dir);
    // a workspace with no root (the tests'): no repository, no revision
    let mut plain = AppState::create(PathBuf::new(), vec![file()], RefAny::new(0u8), PathBuf::new());
    plain.start_conversation(note(3));
    assert_eq!(plain.notes[0].talk.base, None);
}
