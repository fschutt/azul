//! The header of the whiteboard's panel: what a click on it asks of the boards (done, or waiting for a save), the list
//! of boards as a popover, the bar that tells a deletion and undoes it. Needs azul, no window.

use super::*;
use crate::{
    whiteboard::{CutoutDraft, Source},
    whiteboard_app::HEADER_PX,
    whiteboard_export::ExportPictures,
    whiteboard_geom::{Pt, Size},
};

fn panel() -> Panel {
    crate::l10n::in_english();
    let mut p = Panel::new("/work/azul");
    let _start = p.wb.index_loaded(None, false, "Board 1", 1_000);
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
    nodes(dom)
        .into_iter()
        .filter_map(|n| n.root.node_type.get_text().into_option())
        .map(|t| t.as_str().to_string())
        .filter(|t| !t.trim().is_empty())
        .collect()
}

/// The open board is changed and not kept (a cutout on it).
fn change(p: &mut Panel) {
    let source = Source { rev: String::new(), sha256: String::new(), file: "a.rs".to_string(), page: 0, first_line: 1, last_line: 2, shape: Vec::new(), file_hash: 1, range_hash: 2, lines: Vec::new() };
    p.wb.board.add_cutout(CutoutDraft::pending(Pt::new(0.0, 0.0), Size::new(90.0, 40.0), source)).expect("room");
}

fn done(outcome: BoardOutcome) -> Plan {
    match outcome {
        BoardOutcome::Done(plan) => plan,
        other => panic!("done: {other:?}"),
    }
}

#[test]
fn a_new_board_opens_at_once_and_the_plan_goes_to_the_host_and_the_list_closes() {
    let mut p = panel();
    p.switcher.open = true;
    let plan = done(p.board_action(BoardAction::New));
    assert_eq!(p.wb.active_board(), ("b2", "Board 2"), "named by its number, in the person's language");
    assert_eq!(plan.effects.len(), 1);
    assert!(!p.switcher.open);
}

#[test]
fn a_switch_while_the_open_board_is_not_kept_waits_for_the_save_and_is_asked_again_after_it() {
    let mut p = panel();
    done(p.board_action(BoardAction::New));
    change(&mut p);
    assert_eq!(p.board_action(BoardAction::Switch("b1".to_string())), BoardOutcome::Wait);
    assert_eq!(p.switcher.pending, Some(BoardAction::Switch("b1".to_string())));
    assert_eq!(p.wb.active_board().0, "b2", "nothing happened yet");
    // The save runs and ends: the switch is asked again and now goes through, with its board to be read.
    let _job = p.wb.save_job(2_000).expect("a save");
    assert!(!p.wb.save_done(&Ok(())));
    let plan = done(p.after_wait().expect("it waited"));
    assert_eq!(p.wb.active_board().0, "b1");
    assert!(plan.load.is_some());
    assert!(p.switcher.pending.is_none());
    assert!(p.after_wait().is_none(), "once");
}

#[test]
fn deleting_a_board_is_told_with_undo_which_brings_it_back_and_the_bar_goes_when_its_time_is_up() {
    let mut p = panel();
    done(p.board_action(BoardAction::New));
    let plan = done(p.board_action(BoardAction::Delete("b2".to_string())));
    assert_eq!(p.wb.boards().boards().count(), 1);
    assert!(plan.load.is_some(), "the next board is read");
    p.wb.loaded_none();
    let toast = p.switcher.toast.clone().expect("told");
    assert!(toast.text.contains("Board 2") && toast.text.contains("deleted"), "{}", toast.text);
    assert_eq!(toast.undo.as_deref(), Some("b2"));
    // Undo.
    done(p.board_action(BoardAction::Restore("b2".to_string())));
    assert_eq!(p.wb.boards().boards().count(), 2);
    assert!(p.switcher.toast.is_none());
    // Left alone, the bar goes when its time is up, and not before.
    done(p.board_action(BoardAction::Delete("b2".to_string())));
    let until = p.switcher.toast.as_ref().map(|t| t.until_ms).expect("told");
    assert!(!p.expire_toast(until - 1));
    assert!(p.expire_toast(until));
    assert!(p.switcher.toast.is_none());
    assert!(!p.expire_toast(until + 1), "nothing to take away");
}

#[test]
fn a_refusal_is_said_in_the_hint_and_changes_nothing() {
    let mut p = panel();
    assert_eq!(p.board_action(BoardAction::Rename("b1".to_string(), "  ".to_string())), BoardOutcome::Told);
    assert!(p.hint.as_deref().is_some_and(|h| h.contains("needs a name")), "{:?}", p.hint);
    assert_eq!(p.wb.active_board().1, "Board 1");
    assert_eq!(p.board_action(BoardAction::Switch("nope".to_string())), BoardOutcome::Told);
    assert_eq!(p.board_action(BoardAction::Delete("nope".to_string())), BoardOutcome::Told);
}

#[test]
fn a_board_is_renamed_duplicated_and_moved_and_a_rename_leaves_the_list_open() {
    let mut p = panel();
    p.switcher.open = true;
    p.switcher.renaming = Some("b1".to_string());
    let plan = done(p.board_action(BoardAction::Rename("b1".to_string(), "Parser".to_string())));
    assert_eq!(p.wb.active_board().1, "Parser");
    assert_eq!(plan.effects.len(), 1);
    assert!(p.switcher.open && p.switcher.renaming.is_none(), "the list stays, the field goes");
    // A copy of the open board: named for it, in a file of its own, and the list closes.
    let plan = done(p.board_action(BoardAction::Duplicate));
    assert_eq!(p.wb.boards().boards().map(|b| b.name.clone()).collect::<Vec<_>>(), vec!["Parser", "Parser copy"]);
    assert!(plan.copy.is_some());
    assert!(!p.switcher.open);
    // Moved: down one, then up two (past the top is the top).
    p.switcher.open = true;
    done(p.board_action(BoardAction::Move("b1".to_string(), 1)));
    assert_eq!(p.wb.boards().boards().map(|b| b.id.clone()).collect::<Vec<_>>(), vec!["b2", "b1"]);
    done(p.board_action(BoardAction::Move("b1".to_string(), -2)));
    assert_eq!(p.wb.boards().boards().map(|b| b.id.clone()).collect::<Vec<_>>(), vec!["b1", "b2"]);
    assert!(p.switcher.open, "a move leaves the list open");
}

#[test]
fn the_header_is_the_open_boards_name_as_a_button_and_a_button_for_a_new_board() {
    let mut p = panel();
    done(p.board_action(BoardAction::New));
    let header = header_dom(&p, &host());
    let words = texts(&header);
    assert!(words.iter().any(|w| w.starts_with("Board 2")), "{words:?}");
    assert!(with_id(&header, "__azreview_whiteboard-boards").is_some(), "the switcher");
    assert!(with_id(&header, "__azreview_whiteboard-board-new").is_some());
}

#[test]
fn the_list_has_a_row_for_each_board_with_what_can_be_done_to_it_and_the_open_one_can_be_duplicated() {
    let mut p = panel();
    done(p.board_action(BoardAction::New));
    done(p.board_action(BoardAction::New));
    let h = host();
    let list = popover_dom(&p, &h);
    for id in ["b1", "b2", "b3"] {
        assert!(with_id(&list, &format!("__azreview_whiteboard-board-{id}")).is_some(), "a row for {id}");
        assert!(with_id(&list, &format!("__azreview_whiteboard-board-{id}-rename")).is_some());
        assert!(with_id(&list, &format!("__azreview_whiteboard-board-{id}-delete")).is_some());
    }
    // b3 is open: only it has Duplicate; the first has no Up and the last no Down.
    assert!(with_id(&list, "__azreview_whiteboard-board-b3-duplicate").is_some());
    assert!(with_id(&list, "__azreview_whiteboard-board-b1-duplicate").is_none());
    assert!(with_id(&list, "__azreview_whiteboard-board-b1-up").is_none());
    assert!(with_id(&list, "__azreview_whiteboard-board-b3-down").is_none());
    assert!(with_id(&list, "__azreview_whiteboard-board-b2-up").is_some() && with_id(&list, "__azreview_whiteboard-board-b2-down").is_some());
    // The words on the buttons are the person's language.
    let words = texts(&list);
    for w in ["Rename", "Delete", "Duplicate", "Up", "Down"] {
        assert!(words.iter().any(|t| t == w), "{w}: {words:?}");
    }
}

#[test]
fn a_board_being_renamed_has_a_field_that_takes_the_keyboard_in_place_of_its_name() {
    let mut p = panel();
    p.switcher.renaming = Some("b1".to_string());
    let list = popover_dom(&p, &host());
    let field = with_id(&list, "__azreview_whiteboard-board-b1-field").expect("a field");
    assert!(field.root.get_attributes().as_slice().iter().any(|a| matches!(a, AttributeType::Autofocus)));
    assert!(with_id(&list, "__azreview_whiteboard-board-b1").is_none(), "the name is the field");
    assert!(with_id(&list, "__azreview_whiteboard-board-b1-rename").is_none());
}

#[test]
fn the_bar_that_tells_a_deletion_has_its_words_and_an_undo() {
    crate::l10n::in_english();
    let toast = Toast { text: "Whiteboard \"Ideas\" deleted".to_string(), undo: Some("b2".to_string()), until_ms: 9_000 };
    let bar = toast_dom(&toast, &host());
    assert!(texts(&bar).iter().any(|t| t.contains("Ideas")));
    assert!(with_id(&bar, "__azreview_whiteboard-toast-undo").is_some(), "undo");
    assert!(texts(&bar).iter().any(|t| t == "Undo"));
    let told_only = Toast { text: "x".to_string(), undo: None, until_ms: 1 };
    assert!(with_id(&toast_dom(&told_only, &host()), "__azreview_whiteboard-toast-undo").is_none());
}

#[test]
fn the_header_is_as_high_as_the_owner_takes_off_a_drop_and_the_panel_puts_the_board_under_it() {
    crate::l10n::in_english();
    let h = host();
    let dom = h.run(|p| crate::whiteboard_ui::panel_dom(p, &h)).expect("a panel");
    let children = dom.children.as_slice();
    assert!(children[0].root.has_id(AzString::from("__azreview_whiteboard-boards")) || with_id(&children[0], "__azreview_whiteboard-boards").is_some(), "the header comes first");
    assert!(with_id(&children[1], crate::whiteboard_ui::SURFACE_ID).is_some(), "the board's area, with the surface, is under it");
    assert!((HEADER_PX - 30.0).abs() < f32::EPSILON, "the header's height is part of the contract with the host's bar");
    // The list and the bar of a deletion are over both, only while there is something to show.
    assert_eq!(children.len(), 2);
    let open = h
        .run(|p| {
            p.switcher.open = true;
            p.switcher.toast = Some(Toast { text: "t".to_string(), undo: None, until_ms: 1 });
            crate::whiteboard_ui::panel_dom(p, &h)
        })
        .expect("a panel");
    assert_eq!(open.children.as_slice().len(), 4, "header, board, list, bar");
}

// ---- export and import

fn loaded_board(name: &str) -> crate::whiteboard_store::Loaded {
    crate::whiteboard_store::Loaded {
        data: crate::whiteboard::BoardData::default(),
        meta: crate::whiteboard_azboard::Meta { board_name: name.to_string(), ..Default::default() },
        history: Default::default(),
        snapshots_left_out: 0,
        left_out: 0,
        unreadable_pictures: 0,
    }
}

#[test]
fn the_list_ends_with_export_and_import() {
    let p = panel();
    let list = popover_dom(&p, &host());
    assert!(with_id(&list, "__azreview_whiteboard-board-export").is_some());
    assert!(with_id(&list, "__azreview_whiteboard-board-import").is_some());
    let words = texts(&list);
    assert!(words.iter().any(|w| w == "Export..."), "{words:?}");
    assert!(words.iter().any(|w| w == "Import..."), "{words:?}");
}

#[test]
fn export_asks_first_where_the_pictures_go_and_then_hands_the_host_the_board_to_write() {
    let mut p = panel();
    p.switcher.open = true;
    assert_eq!(done(p.board_action(BoardAction::OpenExport)), Plan::default(), "nothing for the host yet");
    assert_eq!(p.switcher.export, Some(ExportPictures::Embedded), "inside the file is the default");
    assert!(!p.switcher.open, "the sheet takes the list's place");
    done(p.board_action(BoardAction::Pictures(ExportPictures::Beside)));
    assert_eq!(p.switcher.export, Some(ExportPictures::Beside));
    change(&mut p);
    let plan = done(p.board_action(BoardAction::Export));
    let request = plan.export.expect("the board to write");
    assert_eq!((request.pictures, request.meta.board_id.as_str()), (ExportPictures::Beside, "b1"));
    assert_eq!(request.data.items.len(), 1, "the board as it is, with what is not kept yet");
    assert!(p.switcher.export.is_none(), "the sheet is done");
    assert!(!plan.import && plan.load.is_none());
    // Without the sheet there is nothing to export; Cancel drops it.
    assert_eq!(done(p.board_action(BoardAction::Export)).export, None);
    done(p.board_action(BoardAction::OpenExport));
    done(p.board_action(BoardAction::CloseExport));
    assert!(p.switcher.export.is_none());
    // A picture choice with no sheet open opens nothing.
    done(p.board_action(BoardAction::Pictures(ExportPictures::Beside)));
    assert!(p.switcher.export.is_none());
}

#[test]
fn the_sheet_names_the_author_the_file_will_carry_and_says_when_it_names_none() {
    use crate::whiteboard_git::RepoFacts;
    let mut p = panel();
    let h = host();
    assert!(texts(&export_dialog_dom(&p, &h, ExportPictures::Embedded)).iter().any(|w| w.contains("names no author")), "git has no name yet");
    p.wb.set_repo(Some(RepoFacts { author: Some("Ada Lovelace".to_string()), ..RepoFacts::default() }));
    let words = texts(&export_dialog_dom(&p, &h, ExportPictures::Embedded));
    assert!(words.iter().any(|w| w.contains("names its author") && w.contains("Ada Lovelace")), "{words:?}");
    assert!(words.iter().any(|w| w.contains("Export") && w.contains("JSON")), "the title says what is exported: {words:?}");
    let (off, _) = crate::whiteboard_settings::Settings::parse(&|k| (k == crate::whiteboard_settings::KEY_AUTHOR).then(|| "off".to_string()));
    p.wb.set_settings(off);
    assert!(texts(&export_dialog_dom(&p, &h, ExportPictures::Embedded)).iter().any(|w| w.contains("names no author")), "off writes none, whatever git says");
    let sheet = export_dialog_dom(&p, &h, ExportPictures::Beside);
    for id in ["", "-inside", "-beside", "-go", "-cancel"] {
        assert!(with_id(&sheet, &format!("__azreview_whiteboard-export{id}")).is_some(), "{id}");
    }
    assert!(texts(&sheet).iter().any(|w| w.contains('\u{25c9}') && w.contains("folder beside")), "the chosen one is marked");
}

#[test]
fn import_asks_the_host_for_a_file_and_the_file_becomes_a_board_once_the_open_one_is_kept() {
    let mut p = panel();
    assert!(done(p.board_action(BoardAction::Import)).import, "the host asks for a file");
    // The file was read while the open board has a change that is not kept: it waits for the save.
    change(&mut p);
    assert_eq!(p.import_loaded(loaded_board("Parser")), BoardOutcome::Wait);
    assert!(p.switcher.pending_import.is_some());
    assert_eq!(p.wb.boards().boards().count(), 1, "nothing happened yet");
    let _job = p.wb.save_job(2_000).expect("a save");
    assert!(!p.wb.save_done(&Ok(())));
    let plan = done(p.after_wait().expect("it waited"));
    assert_eq!(p.wb.active_board(), ("b2", "Parser"));
    assert_eq!(plan.effects.len(), 1, "the list is written");
    assert!(p.switcher.pending_import.is_none() && !p.switcher.open);
    assert!(p.after_wait().is_none(), "once");
    // A file that names nothing is called what the app calls an imported board (the first one is kept by now).
    let _job = p.wb.save_job(3_000).expect("a save");
    assert!(!p.wb.save_done(&Ok(())));
    let plan = done(p.import_loaded(loaded_board("")));
    assert_eq!(plan.effects.len(), 1);
    assert_eq!(p.wb.active_board().1, "Imported whiteboard");
}

#[test]
fn a_file_that_cannot_be_taken_is_told_and_what_could_not_be_done_is_a_sentence() {
    let mut p = panel();
    for _ in 1..crate::whiteboard_boards::MAX_BOARDS {
        done(p.import_loaded(loaded_board("x")));
        let _ = p.wb.save_job(1);
        let _ = p.wb.save_done(&Ok(()));
    }
    assert_eq!(p.import_loaded(loaded_board("one too many")), BoardOutcome::Told);
    assert!(p.hint.as_deref().is_some_and(|h| h.contains("whiteboards")), "{:?}", p.hint);
    p.tell("it could not be written".to_string());
    assert_eq!(p.hint.as_deref(), Some("it could not be written"));
}

#[test]
fn escape_closes_the_sheet_before_it_closes_anything_else() {
    use crate::whiteboard_ui::{on_key, Key};
    let mut p = panel();
    p.switcher.export = Some(ExportPictures::Embedded);
    assert!(on_key(&mut p, Key::Escape));
    assert!(p.switcher.export.is_none());
    assert!(!on_key(&mut p, Key::Escape), "nothing is left to let go of");
}

#[test]
fn the_panel_puts_the_sheet_over_the_board_only_while_it_is_open() {
    crate::l10n::in_english();
    let h = host();
    let plain = h.run(|p| crate::whiteboard_ui::panel_dom(p, &h)).expect("a panel");
    assert_eq!(plain.children.as_slice().len(), 2);
    let with_sheet = h
        .run(|p| {
            p.switcher.export = Some(ExportPictures::Embedded);
            crate::whiteboard_ui::panel_dom(p, &h)
        })
        .expect("a panel");
    assert_eq!(with_sheet.children.as_slice().len(), 3, "header, board, sheet");
    assert!(with_id(&with_sheet.children.as_slice()[2], "__azreview_whiteboard-export").is_some());
}

