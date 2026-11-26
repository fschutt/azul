//! The history in the whiteboard's panel: the sheet that asks why, the list of snapshots, a snapshot looked at with its reason
//! in a banner, the comparison with the parent, the restore. Needs azul, no window.

use std::sync::Arc;

use super::*;
use crate::{
    whiteboard::{CutoutDraft, CutoutImage, Source},
    whiteboard_app::Plan,
    whiteboard_geom::Pt,
    whiteboard_git::RepoFacts,
    whiteboard_ui::{on_key, Key},
};

const REV: &str = "0123456789abcdef0123456789abcdef01234567";

fn panel() -> Panel {
    crate::l10n::in_english();
    let mut p = Panel::new("/work/azul");
    let _start = p.wb.index_loaded(None, false, "Board 1", 1_000);
    p.wb.set_repo(Some(RepoFacts { rev: Some(REV.to_string()), branch: Some("main".to_string()), ..RepoFacts::default() }));
    p
}

fn with(app: &mut RefAny, f: &mut dyn FnMut(&mut Panel)) -> bool {
    match app.downcast_mut::<Panel>() {
        Some(mut p) => {
            f(&mut p);
            true
        }
        None => false,
    }
}

fn on_jump(_: &mut RefAny, _: crate::whiteboard::ItemId, _: &mut CallbackInfo) -> Update {
    Update::DoNothing
}

fn on_changed(_: &mut RefAny, _: &mut CallbackInfo) {}

fn on_plan(_: &mut RefAny, _: Plan, _: &mut CallbackInfo) {}

/// A workspace with no files at all (nothing is found again).
struct NoFiles;

impl crate::whiteboard_jump::FileView for NoFiles {
    fn file_hash(&self, _: &str) -> Option<u64> {
        None
    }

    fn line_count(&self, _: &str) -> Option<u32> {
        None
    }

    fn lines(&self, _: &str, _: u32, _: u32) -> Option<Vec<String>> {
        None
    }
}

fn with_files(app: &mut RefAny, f: &mut dyn FnMut(&mut Panel, &dyn crate::whiteboard_jump::FileView)) -> bool {
    match app.downcast_mut::<Panel>() {
        Some(mut p) => {
            f(&mut p, &NoFiles);
            true
        }
        None => false,
    }
}

fn host() -> Host {
    Host { app: RefAny::new(Panel::new("/work/azul")), with, on_jump, on_changed, on_plan, with_files }
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
    nodes(dom).into_iter().filter_map(|n| n.root.node_type.get_text().into_option()).map(|t| t.as_str().to_string()).filter(|t| !t.trim().is_empty()).collect()
}

/// A cutout with its picture on the board.
fn add(p: &mut Panel, x: f32) -> u64 {
    let source = Source {
        rev: REV.to_string(),
        sha256: String::new(),
        file: "src/lib.rs".to_string(),
        page: 0,
        first_line: 1,
        last_line: 2,
        shape: Vec::new(),
        file_hash: 1,
        range_hash: 2,
        lines: vec!["fn a() {}".to_string()],
    };
    let image = Arc::new(CutoutImage { width: 2, height: 2, scale: 2.0, rgba: vec![255; 16], png: vec![0x89, b'P', b'N', b'G'] });
    p.wb.board.add_cutout(CutoutDraft::ready(Pt::new(x, 0.0), image, source)).expect("room")
}

fn snapshot(p: &mut Panel, why: &str) {
    assert_eq!(p.history_action(HistoryAction::OpenSheet), HistoryOutcome::Refresh);
    p.history_action(HistoryAction::SetWhy(why.to_string()));
    assert_eq!(p.history_action(HistoryAction::Save), HistoryOutcome::Changed);
}

#[test]
fn the_header_has_a_button_for_a_snapshot_and_one_for_the_history_with_how_many_there_are() {
    let mut p = panel();
    let header = crate::whiteboard_boards_ui::header_dom(&p, &host());
    assert!(with_id(&header, "__azreview_whiteboard-snapshot").is_some());
    assert!(with_id(&header, "__azreview_whiteboard-history").is_some());
    assert!(texts(&header).iter().any(|t| t == "Snapshot..." ), "{:?}", texts(&header));
    assert!(texts(&header).iter().any(|t| t == "History (0)"));
    add(&mut p, 0.0);
    snapshot(&mut p, "one");
    assert!(texts(&crate::whiteboard_boards_ui::header_dom(&p, &host())).iter().any(|t| t == "History (1)"));
}

#[test]
fn a_snapshot_is_asked_for_in_a_sheet_and_taken_with_its_reason_and_tags_and_then_kept() {
    let mut p = panel();
    add(&mut p, 0.0);
    assert_eq!(p.history_action(HistoryAction::OpenSheet), HistoryOutcome::Refresh);
    assert_eq!(p.history_ui.sheet.as_ref().map(|s| s.kind.clone()), Some(SheetKind::Snapshot));
    p.history_action(HistoryAction::SetWhy("  The loop is the front door.  ".to_string()));
    p.history_action(HistoryAction::SetTags("ADR-7, #123".to_string()));
    assert_eq!(p.history_action(HistoryAction::Save), HistoryOutcome::Changed, "the host keeps it");
    assert!(p.history_ui.sheet.is_none());
    let s = p.wb.history().latest().expect("a snapshot");
    assert_eq!((s.why.as_str(), s.tags.clone(), s.id.as_str()), ("The loop is the front door.", vec!["ADR-7".to_string(), "#123".to_string()], "s1"));
    assert!(p.wb.needs_save());
    // Cancel drops what was typed and takes nothing.
    p.history_action(HistoryAction::OpenSheet);
    p.history_action(HistoryAction::SetWhy("never".to_string()));
    assert_eq!(p.history_action(HistoryAction::Cancel), HistoryOutcome::Refresh);
    assert_eq!(p.wb.history().snapshots.len(), 1);
}

#[test]
fn the_sheet_says_what_the_code_is_at_and_what_changed_since_the_last_snapshot() {
    let mut p = panel();
    add(&mut p, 0.0);
    p.history_action(HistoryAction::OpenSheet);
    let sheet = p.history_ui.sheet.clone().expect("a sheet");
    let words = texts(&sheet_dom(&p, &host(), &sheet));
    assert!(words.iter().any(|w| w.contains("The code is at 0123456 on main")), "{words:?}");
    assert!(words.iter().any(|w| w.contains("The first snapshot of this whiteboard")), "{words:?}");
    assert!(words.iter().any(|w| w.starts_with("+ src/lib.rs:1-2")), "what is new is named: {words:?}");
    assert!(words.iter().any(|w| w.contains("Why?")), "{words:?}");
    p.history_action(HistoryAction::SetWhy("one".to_string()));
    p.history_action(HistoryAction::Save);
    add(&mut p, 300.0);
    p.history_action(HistoryAction::OpenSheet);
    let sheet = p.history_ui.sheet.clone().expect("a sheet");
    let words = texts(&sheet_dom(&p, &host(), &sheet));
    assert!(words.iter().any(|w| w.contains("Since s1: 1 added, 0 taken off, 0 moved, 0 changed")), "{words:?}");
    for id in ["", "-save", "-cancel"] {
        assert!(with_id(&sheet_dom(&p, &host(), &sheet), &format!("__azreview_whiteboard-sheet{id}")).is_some(), "{id}");
    }
    assert!(with_id(&sheet_dom(&p, &host(), &sheet), "__azreview_whiteboard-why").is_some());
    assert!(with_id(&sheet_dom(&p, &host(), &sheet), "__azreview_whiteboard-tags").is_some());
    // No repository: no revision to name.
    let mut bare = Panel::new("/work/other");
    let _start = bare.wb.index_loaded(None, false, "Board 1", 1_000);
    bare.history_action(HistoryAction::OpenSheet);
    let sheet = bare.history_ui.sheet.clone().expect("a sheet");
    assert!(texts(&sheet_dom(&bare, &host(), &sheet)).iter().any(|w| w.contains("no git repository")));
}

#[test]
fn a_snapshot_that_cannot_be_taken_is_told_and_the_sheet_stays_with_what_was_typed() {
    let mut p = panel();
    // A picture still being drawn.
    let source = Source { rev: String::new(), sha256: String::new(), file: "a.rs".to_string(), page: 0, first_line: 1, last_line: 2, shape: Vec::new(), file_hash: 1, range_hash: 2, lines: Vec::new() };
    p.wb.board.add_cutout(CutoutDraft::pending(Pt::new(0.0, 0.0), crate::whiteboard_geom::Size::new(90.0, 40.0), source)).expect("room");
    p.history_action(HistoryAction::OpenSheet);
    p.history_action(HistoryAction::SetWhy("typed".to_string()));
    assert_eq!(p.history_action(HistoryAction::Save), HistoryOutcome::Told);
    assert!(p.hint.as_deref().is_some_and(|h| h.contains("picture")), "{:?}", p.hint);
    assert_eq!(p.history_ui.sheet.as_ref().map(|s| s.why.as_str()), Some("typed"));
    assert!(p.wb.history().is_empty());
    let words = texts(&sheet_dom(&p, &host(), &p.history_ui.sheet.clone().expect("a sheet")));
    assert!(words.iter().any(|w| w.contains("1 pictures are still being drawn")), "{words:?}");
}

#[test]
fn the_list_has_a_row_for_each_snapshot_newest_first_with_its_reason_or_the_lack_of_one() {
    let mut p = panel();
    add(&mut p, 0.0);
    snapshot(&mut p, "First reason.\nMore of it.");
    add(&mut p, 300.0);
    snapshot(&mut p, "");
    p.history_action(HistoryAction::Toggle);
    assert!(p.history_ui.list);
    let list = list_dom(&p, &host());
    for id in ["s1", "s2"] {
        assert!(with_id(&list, &format!("__azreview_whiteboard-snap-{id}")).is_some(), "{id}");
    }
    let words = texts(&list);
    let at = |needle: &str| words.iter().position(|w| w.contains(needle)).unwrap_or_else(|| panic!("{needle}: {words:?}"));
    assert!(at("s2  0123456") < at("s1  0123456"), "the newest first: {words:?}");
    assert!(words.iter().any(|w| w == "First reason."), "the first line of why: {words:?}");
    assert!(words.iter().any(|w| w == "(no reason given)"), "{words:?}");
    assert!(words.iter().any(|w| w.contains("+1 -0 ~0")), "what changed: {words:?}");
    // An empty history says what to do.
    assert!(texts(&list_dom(&panel(), &host())).iter().any(|w| w.contains("No snapshots yet")));
    // Toggling closes it; opening the sheet closes it too.
    p.history_action(HistoryAction::Toggle);
    assert!(!p.history_ui.list);
}

#[test]
fn a_snapshot_that_does_not_check_says_so_in_the_list_and_in_the_banner() {
    use crate::{
        whiteboard_azboard::{seal, Meta},
        whiteboard_history::{Code, History, Snapshot, Summary},
        whiteboard_store::Loaded,
    };
    let mut p = panel();
    let mut s = seal(
        Snapshot {
            id: "s1".to_string(),
            parent: None,
            taken_ms: 1_700_000_000_000,
            author: None,
            code: Code::default(),
            why: "Because.".to_string(),
            tags: Vec::new(),
            summary: Summary::default(),
            evidence: None,
            data: crate::whiteboard::BoardData::default(),
            value: crate::whiteboard_json::Json::Null,
            text: String::new(),
            hash: String::new(),
            verified: false,
            pictures: std::collections::BTreeMap::new(),
        },
        None,
    );
    s.verified = false;
    let loaded = Loaded { data: crate::whiteboard::BoardData::default(), meta: Meta::default(), history: History { snapshots: vec![s] }, snapshots_left_out: 0, left_out: 0, unreadable_pictures: 0 };
    let _effects = p.wb.loaded(Ok(loaded), None, 2_000);
    p.history_action(HistoryAction::Toggle);
    assert!(with_id(&list_dom(&p, &host()), "__azreview_whiteboard-snap-s1-unverified").is_some());
    p.history_action(HistoryAction::View("s1".to_string()));
    let banner = banner_dom(&p, &host()).expect("a banner");
    assert!(texts(&banner).iter().any(|w| w.contains("does not match its hash")), "{:?}", texts(&banner));
}

#[test]
fn a_snapshot_is_looked_at_with_its_reason_in_a_banner_and_the_board_is_left_as_it_is() {
    let mut p = panel();
    add(&mut p, 0.0);
    snapshot(&mut p, "Why the first box is there.");
    add(&mut p, 300.0);
    assert_eq!(p.history_action(HistoryAction::View("s1".to_string())), HistoryOutcome::Refresh);
    let banner = banner_dom(&p, &host()).expect("a banner");
    let words = texts(&banner);
    assert!(words.iter().any(|w| w.contains("Snapshot s1 at 0123456") && w.contains("read-only")), "{words:?}");
    assert!(words.iter().any(|w| w == "Why the first box is there."), "the reason is on the banner: {words:?}");
    for id in ["compare", "restore", "back"] {
        assert!(with_id(&banner, &format!("__azreview_whiteboard-{id}")).is_some(), "{id}");
    }
    assert_eq!(p.wb.display().len(), 1, "the snapshot has one cutout");
    assert_eq!(p.wb.board.len(), 2, "the board has two");
    // Set against its parent (none: all of it is new), then no more.
    p.history_action(HistoryAction::Compare(true));
    assert!(texts(&banner_dom(&p, &host()).expect("a banner")).iter().any(|w| w.contains("The first snapshot: everything in it is new")));
    p.history_action(HistoryAction::Compare(false));
    assert!(with_id(&banner_dom(&p, &host()).expect("a banner"), "__azreview_whiteboard-compare-legend").is_none());
    // The pointer's work is not done on it and the board's keys wait.
    assert!(!on_key(&mut p, Key::Delete), "nothing of a snapshot is taken off");
    assert!(on_key(&mut p, Key::Escape), "Escape goes back to the board");
    assert!(p.wb.viewing().is_none() && banner_dom(&p, &host()).is_none());
    assert_eq!(p.history_action(HistoryAction::View("nope".to_string())), HistoryOutcome::Told);
}

#[test]
fn what_a_snapshot_took_off_is_a_ghost_on_the_board_while_it_is_set_against_its_parent() {
    let mut p = panel();
    let a = add(&mut p, 0.0);
    let b = add(&mut p, 300.0);
    snapshot(&mut p, "both");
    p.wb.board.remove(b).expect("removed");
    snapshot(&mut p, "b went");
    p.history_action(HistoryAction::View("s2".to_string()));
    p.history_action(HistoryAction::Compare(true));
    let area = crate::whiteboard_ui::panel_dom(&mut p, &host());
    assert!(with_id(&area, &format!("__azreview_whiteboard-cutout-{a}")).is_some(), "what is in the snapshot");
    assert!(with_id(&area, &format!("__azreview_whiteboard-ghost-{b}")).is_some(), "what was taken off, as a ghost");
    assert!(with_id(&area, &format!("__azreview_whiteboard-cutout-{b}")).is_none(), "and not as a cutout");
}

#[test]
fn restore_asks_why_with_the_reason_ready_and_makes_the_old_state_a_new_snapshot() {
    let mut p = panel();
    add(&mut p, 0.0);
    snapshot(&mut p, "just one");
    add(&mut p, 300.0);
    snapshot(&mut p, "two boxes");
    assert_eq!(p.history_action(HistoryAction::OpenRestore("s1".to_string())), HistoryOutcome::Refresh);
    let sheet = p.history_ui.sheet.clone().expect("a sheet");
    assert_eq!(sheet.kind, SheetKind::Restore("s1".to_string()));
    assert_eq!(sheet.why, "Restored the board of snapshot s1.");
    let words = texts(&sheet_dom(&p, &host(), &sheet));
    assert!(words.iter().any(|w| w.contains("Restore snapshot s1")), "{words:?}");
    assert_eq!(p.history_action(HistoryAction::Save), HistoryOutcome::Changed);
    assert_eq!(p.wb.board.len(), 1, "the board is what s1 was");
    assert_eq!(p.wb.history().snapshots.len(), 3, "nothing was lost and nothing was rewritten");
    assert_eq!(p.wb.history().latest().map(|s| s.why.as_str()), Some("Restored the board of snapshot s1."));
    assert_eq!(p.history_action(HistoryAction::OpenRestore("nope".to_string())), HistoryOutcome::Told);
}

#[test]
fn what_the_dictation_heard_is_added_to_the_reason_while_the_sheet_is_open() {
    let mut p = panel();
    assert!(!p.dictated("nobody asked"), "no sheet, nothing to add to");
    p.history_action(HistoryAction::OpenSheet);
    assert!(p.dictated("we split the solver"));
    assert!(p.dictated("out of the window"));
    assert_eq!(p.history_ui.sheet.as_ref().map(|s| s.why.as_str()), Some("we split the solver out of the window"));
}

#[test]
fn escape_closes_the_sheet_then_the_list_and_the_two_never_stay_open_together() {
    let mut p = panel();
    p.history_action(HistoryAction::Toggle);
    p.history_action(HistoryAction::OpenSheet);
    assert!(!p.history_ui.list && p.history_ui.sheet.is_some(), "the sheet takes the list's place");
    assert!(on_key(&mut p, Key::Escape));
    assert!(p.history_ui.sheet.is_none());
    p.history_action(HistoryAction::Toggle);
    assert!(on_key(&mut p, Key::Escape));
    assert!(!p.history_ui.list);
    assert!(!on_key(&mut p, Key::Escape), "nothing is left to let go of");
    // The boards' list and the history's are not open together either.
    p.switcher.open = true;
    p.history_action(HistoryAction::Toggle);
    assert!(!p.switcher.open && p.history_ui.list);
}

// ---- the code moves on

const NEWER: &str = "2222222222222222222222222222222222222222";

fn moved_to(p: &mut Panel, rev: &str) {
    p.wb.set_repo(Some(RepoFacts { rev: Some(rev.to_string()), branch: Some("main".to_string()), ..RepoFacts::default() }));
}

/// `src/lib.rs` with a line put above the code the cutouts were circled in.
struct Shifted;

impl crate::whiteboard_jump::FileView for Shifted {
    fn file_hash(&self, file: &str) -> Option<u64> {
        (file == "src/lib.rs").then_some(77)
    }

    fn line_count(&self, _: &str) -> Option<u32> {
        Some(3)
    }

    fn lines(&self, file: &str, first: u32, last: u32) -> Option<Vec<String>> {
        let all = ["// a line above", "fn a() {}", "// below"];
        (file == "src/lib.rs").then(|| all[(first as usize - 1).min(3)..(last as usize).min(3)].iter().map(|l| (*l).to_string()).collect())
    }

    fn find_block(&self, file: &str, block: &[String], near: u32) -> Option<u32> {
        let all: Vec<String> = ["// a line above", "fn a() {}", "// below"].iter().map(|l| (*l).to_string()).collect();
        (file == "src/lib.rs").then(|| crate::whiteboard_jump::find_block_in(&all, block, near)).flatten()
    }
}

#[test]
fn the_code_that_moved_on_is_asked_about_once_for_each_revision_and_never_with_something_else_open() {
    let mut p = panel();
    add(&mut p, 0.0);
    assert_eq!(p.head_question(), None, "no snapshot to have moved from");
    snapshot(&mut p, "one");
    assert_eq!(p.head_question(), None);
    moved_to(&mut p, NEWER);
    assert_eq!(p.head_question(), Some((REV.to_string(), NEWER.to_string())));
    let bar = head_dom(&p, &host()).expect("a bar");
    let words = texts(&bar);
    assert!(words.iter().any(|w| w.contains("moved from 0123456 to 2222222")), "{words:?}");
    for id in ["", "-take", "-dismiss"] {
        assert!(with_id(&bar, &format!("__azreview_whiteboard-head{id}")).is_some(), "{id}");
    }
    // Something else being open, the question waits.
    p.history_ui.list = true;
    assert_eq!(p.head_question(), None);
    p.history_ui.list = false;
    // "Not now" is for that revision; the next revision is asked about again.
    assert_eq!(p.history_action(HistoryAction::DismissHead), HistoryOutcome::Refresh);
    assert_eq!(p.head_question(), None);
    moved_to(&mut p, "3333333333333333333333333333333333333333");
    assert!(p.head_question().is_some());
    // A snapshot at the new revision answers it.
    p.history_action(HistoryAction::OpenSheet);
    p.history_action(HistoryAction::Save);
    assert_eq!(p.head_question(), None);
}

#[test]
fn what_was_done_since_the_last_snapshot_starts_the_reason_when_the_code_moved() {
    let mut p = panel();
    p.history_ui.material = vec!["Conversation \"rename the solver\": done".to_string(), "Patch fix-cache.patch applied".to_string()];
    add(&mut p, 0.0);
    // The code has not moved: the material is not put in the reason.
    p.history_action(HistoryAction::OpenSheet);
    assert_eq!(p.history_ui.sheet.as_ref().map(|s| s.why.as_str()), Some(""));
    p.history_action(HistoryAction::SetWhy("one".to_string()));
    p.history_action(HistoryAction::Save);
    moved_to(&mut p, NEWER);
    p.history_action(HistoryAction::OpenSheet);
    let why = p.history_ui.sheet.as_ref().map(|s| s.why.clone()).expect("a sheet");
    assert_eq!(why, "Since the last snapshot:\n- Conversation \"rename the solver\": done\n- Patch fix-cache.patch applied\n\n");
    assert!(texts(&head_dom(&panel_with_question(), &host()).expect("a bar")).iter().any(|w| w.contains("2 things that were done")));
}

#[test]
fn what_was_done_since_is_cut_to_forty_lines_of_two_hundred_characters_and_says_how_many_are_left_out() {
    let mut p = panel();
    p.history_ui.material = (1..=100).map(|i| format!("{i}: {}\nsecond line\u{7}", "x".repeat(300))).collect();
    add(&mut p, 0.0);
    snapshot(&mut p, "first");
    moved_to(&mut p, NEWER);
    p.history_action(HistoryAction::OpenSheet);
    let why = p.history_ui.sheet.as_ref().map(|s| s.why.clone()).expect("a sheet");
    let lines: Vec<&str> = why.lines().collect();
    assert_eq!(lines[0], "Since the last snapshot:");
    assert_eq!(lines.len(), MAX_MATERIAL_LINES + 3, "the intro, forty lines, the line that counts, the blank line: {why}");
    assert!(lines[1].starts_with("- 1: x") && lines[1].chars().count() == 2 + MAX_MATERIAL_CHARS, "{}", lines[1]);
    assert!(!why.contains("second line") && !why.contains('\u{7}'), "a line is one line, with no control characters");
    assert_eq!(lines[MAX_MATERIAL_LINES + 1], "- ... and 60 more");
    assert!(why.ends_with("\n\n"));
}

/// A panel that has a snapshot and a revision that moved on, with two things done since.
fn panel_with_question() -> Panel {
    let mut p = panel();
    p.history_ui.material = vec!["one".to_string(), "two".to_string()];
    add(&mut p, 0.0);
    snapshot(&mut p, "first");
    moved_to(&mut p, NEWER);
    p
}

#[test]
fn a_cutout_whose_code_moved_wears_a_flag_that_says_so_and_the_sheet_lists_what_was_found() {
    let mut p = panel();
    let id = add(&mut p, 0.0);
    assert!(flag_dom(&p, id).is_none(), "nothing is flagged until the code was looked for");
    assert_eq!(p.wb.relocate_all(&Shifted), 1);
    let flag = flag_dom(&p, id).expect("a flag");
    assert_eq!(texts(&flag), vec!["moved"]);
    assert!(with_id(&flag, &format!("__azreview_whiteboard-flag-{id}")).is_some());
    // The sheet that takes the snapshot says how many, and which.
    p.history_action(HistoryAction::OpenSheet);
    let sheet = p.history_ui.sheet.clone().expect("a sheet");
    let words = texts(&sheet_dom(&p, &host(), &sheet));
    assert!(words.iter().any(|w| w.contains("1 moved, 0 changed, 0 gone")), "{words:?}");
    assert!(words.iter().any(|w| w.starts_with(&format!("! #{id} moved (1-2 -> 2-3)"))), "{words:?}");
    // The snapshot records it: the flag is gone from the board and is on the snapshot.
    p.history_action(HistoryAction::SetWhy("moved".to_string()));
    p.history_action(HistoryAction::Save);
    assert!(flag_dom(&p, id).is_none());
    p.history_action(HistoryAction::View("s1".to_string()));
    assert_eq!(texts(&flag_dom(&p, id).expect("the snapshot's flag")), vec!["moved"]);
    // Code that is nowhere is gone, in red.
    p.history_action(HistoryAction::Back);
    assert_eq!(p.wb.relocate_all(&NoFiles), 1);
    assert_eq!(texts(&flag_dom(&p, id).expect("a flag")), vec!["gone"]);
}

#[test]
fn the_sheet_and_the_snapshot_look_for_the_code_when_the_host_has_files_and_not_when_it_has_none() {
    let mut p = panel();
    let id = add(&mut p, 0.0);
    // With the files: opening the sheet looks.
    assert_eq!(p.history_action_files(HistoryAction::OpenSheet, Some(&Shifted)), HistoryOutcome::Refresh);
    assert!(p.wb.flags().contains_key(&id));
    // Without: nothing is looked for (a host with no files).
    let mut q = panel();
    add(&mut q, 0.0);
    q.history_action(HistoryAction::OpenSheet);
    assert!(q.wb.flags().is_empty());
    // A restore does not look for the code: it is the old state.
    p.history_action(HistoryAction::SetWhy("x".to_string()));
    p.history_action(HistoryAction::Save);
    p.history_action(HistoryAction::OpenRestore("s1".to_string()));
    assert_eq!(p.history_action_files(HistoryAction::Save, Some(&NoFiles)), HistoryOutcome::Changed);
    assert!(p.wb.flags().is_empty(), "the board is the snapshot's, nothing is flagged");
}

