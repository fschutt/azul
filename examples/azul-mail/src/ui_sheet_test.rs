//! What a row of the message sheet runs and when it cannot.

use super::*;
use crate::nav::SHEET;

#[test]
fn every_row_of_the_sheet_runs_the_ribbon_command_of_its_name() {
    assert_eq!(command(SheetAction::Reply), Action::Reply);
    assert_eq!(command(SheetAction::ReplyAll), Action::ReplyAll);
    assert_eq!(command(SheetAction::Forward), Action::Forward);
    assert_eq!(command(SheetAction::Archive), Action::Archive);
    assert_eq!(command(SheetAction::Delete), Action::Delete);
    assert_eq!(command(SheetAction::ToggleFlag), Action::ToggleFlag);
    assert_eq!(command(SheetAction::ToggleRead), Action::ToggleRead);
    assert_eq!(command(SheetAction::Move), Action::Move);
    for row in &SHEET {
        let _ = command(row.action);
    }
}

#[test]
fn the_answers_need_the_message_open_and_say_to_select_it_first() {
    for action in [SheetAction::Reply, SheetAction::ReplyAll, SheetAction::Forward] {
        assert_eq!(disabled_reason(action, false, true), Some("azmail-notice-select-first"));
        assert_eq!(disabled_reason(action, true, true), None);
        assert_eq!(disabled_reason(action, true, false), None, "an answer needs no Azlin drive");
    }
}

#[test]
fn archive_and_move_need_an_account_whose_folders_can_change() {
    for action in [SheetAction::Archive, SheetAction::Move] {
        assert_eq!(disabled_reason(action, true, false), Some(ui_main::READ_ONLY));
        assert_eq!(disabled_reason(action, true, true), None);
        assert_eq!(disabled_reason(action, false, true), None, "they act on the selection, not the open message");
    }
}

#[test]
fn delete_flag_and_read_are_always_there() {
    for action in [SheetAction::Delete, SheetAction::ToggleFlag, SheetAction::ToggleRead] {
        for (open, changeable) in [(false, false), (false, true), (true, false), (true, true)] {
            assert_eq!(disabled_reason(action, open, changeable), None, "{action:?} {open} {changeable}");
        }
    }
}
