//! The sidebar's plan: the tags of its rows, the sections, the rail, the quick-access buttons, the
//! message sheet and the top edge of the list.

use std::collections::BTreeSet;

use super::*;
use crate::listing::{DateGroup, ListRow};

fn info(key: &str, role: Role, unread: usize) -> FolderInfo {
    FolderInfo {
        key: key.to_string(),
        display: key.replace('.', "/"),
        role,
        unread,
    }
}

fn inbox() -> FolderInfo {
    info("inbox", Role::Inbox, 12)
}

fn labels() -> [String; MODULES] {
    ["Mail", "Calendar", "Contacts", "Tasks"].map(String::from)
}

/// Two accounts, the first with a small tree: Inbox, Sent, a folder `Work` with `Work.Projects`.
fn folders() -> Vec<Vec<FolderInfo>> {
    vec![
        vec![
            inbox(),
            info("sent", Role::Sent, 0),
            info("Work", Role::Other, 2),
            info("Work.Projects", Role::Other, 1),
        ],
        vec![info("inbox", Role::Inbox, 5)],
        // Local Folders
        vec![info("drafts", Role::Drafts, 0)],
    ]
}

fn input<'a>(
    accounts: &'a [String],
    folders: &'a [Vec<FolderInfo>],
    open: &'a [bool],
    closed: &'a BTreeSet<(usize, String)>,
) -> NavInput<'a> {
    NavInput {
        accounts,
        local: Some((2, String::from("Local Folders"))),
        folders,
        current: Some(0),
        folder: Some("inbox"),
        groups_open: open,
        closed,
        module: 0,
        module_labels: labels(),
        inbox_unread: 12,
    }
}

fn accounts() -> Vec<String> {
    vec![String::from("ada@example.org"), String::from("ben@example.org")]
}

// ---- the tags ----

#[test]
fn a_folder_s_tag_names_its_mailbox_and_its_place_and_a_module_button_s_its_number() {
    for (mailbox, node) in [(0, 0), (0, 9), (1, 3), (7, 2_345), (MAX_MAILBOXES, 0)] {
        let tag = folder_tag(mailbox, node);
        assert_ne!(tag, 0, "0 is no row");
        assert_eq!(target_of(tag), Some(NavTarget::Folder { mailbox, node }), "{mailbox} {node}");
    }
    for module in 0..MODULES {
        assert_eq!(target_of(module_tag(module)), Some(NavTarget::Module(module)));
    }
}

#[test]
fn a_tag_no_row_has_names_nothing() {
    assert_eq!(target_of(0), None);
    assert_eq!(target_of(module_tag(0) + MODULES), None, "past the last module");
    assert_eq!(target_of(usize::MAX), None);
    // a number past every limit is held to it, never wrapped into another mailbox's tag
    assert_eq!(folder_tag(usize::MAX, usize::MAX), folder_tag(MAX_MAILBOXES, NODE_SPAN - 1));
    assert_eq!(module_tag(usize::MAX), module_tag(MODULES - 1));
}

#[test]
fn no_folder_tag_is_a_module_tag() {
    assert!(folder_tag(MAX_MAILBOXES, NODE_SPAN - 1) < module_tag(0));
}

// ---- the plan ----

#[test]
fn the_sections_are_the_accounts_then_local_folders_then_the_module_buttons() {
    crate::l10n::in_english();
    let accounts = accounts();
    let folders = folders();
    let open = vec![true; 4];
    let closed = BTreeSet::new();
    let sections = plan(&input(&accounts, &folders, &open, &closed));
    let titles: Vec<&str> = sections.iter().map(|s| s.title.as_str()).collect();
    assert_eq!(titles, vec!["ada@example.org", "ben@example.org", "Local Folders", ""]);
    assert_eq!(sections[0].mailbox, Some(0));
    assert_eq!(sections[2].mailbox, Some(2));
    assert_eq!(sections[3].mailbox, None);
    let modules: Vec<&str> = sections[3].rows.iter().map(|r| r.label.as_str()).collect();
    assert_eq!(modules, vec!["Mail", "Calendar", "Contacts", "Tasks"]);
}

#[test]
fn local_folders_are_a_section_only_when_they_show() {
    let accounts = accounts();
    let folders = folders();
    let open = vec![true; 4];
    let closed = BTreeSet::new();
    let mut with = input(&accounts, &folders, &open, &closed);
    with.local = None;
    let sections = plan(&with);
    assert_eq!(sections.len(), 3, "two accounts and the modules");
}

#[test]
fn a_folder_row_has_its_icon_its_label_its_unread_count_and_its_depth() {
    crate::l10n::in_english();
    let accounts = accounts();
    let folders = folders();
    let open = vec![true; 4];
    let closed = BTreeSet::new();
    let sections = plan(&input(&accounts, &folders, &open, &closed));
    let rows = &sections[0].rows;
    let by_label = |label: &str| rows.iter().find(|r| r.label == label).expect(label);
    let inbox = by_label("Inbox");
    assert_eq!((inbox.icon, inbox.unread, inbox.depth), ("inbox", 12, 0));
    let projects = by_label("Projects");
    assert_eq!((projects.icon, projects.unread, projects.depth), ("folder", 1, 1));
    assert_eq!(by_label("Sent Items").icon, "send");
}

#[test]
fn only_the_shown_folder_of_the_shown_mailbox_is_selected() {
    crate::l10n::in_english();
    let accounts = accounts();
    let folders = folders();
    let open = vec![true; 4];
    let closed = BTreeSet::new();
    let sections = plan(&input(&accounts, &folders, &open, &closed));
    let selected: Vec<(usize, &str)> = sections
        .iter()
        .enumerate()
        .flat_map(|(i, s)| s.rows.iter().filter(|r| r.selected).map(move |r| (i, r.label.as_str())))
        .collect();
    // the Inbox of account 0, and the Mail button
    assert_eq!(selected, vec![(0, "Inbox"), (3, "Mail")]);
}

#[test]
fn the_rail_has_the_shown_mailbox_s_top_level_and_the_module_buttons_only() {
    crate::l10n::in_english();
    let accounts = accounts();
    let folders = folders();
    let open = vec![true; 4];
    let closed = BTreeSet::new();
    let sections = plan(&input(&accounts, &folders, &open, &closed));
    let rail: Vec<(usize, &str)> = sections
        .iter()
        .enumerate()
        .flat_map(|(i, s)| s.rows.iter().filter(|r| r.in_rail).map(move |r| (i, r.label.as_str())))
        .collect();
    let in_account_0: Vec<&str> = rail.iter().filter(|(i, _)| *i == 0).map(|(_, l)| *l).collect();
    assert_eq!(in_account_0, vec!["Inbox", "Sent Items", "Work"], "no nested folder in the rail");
    assert!(rail.iter().all(|(i, _)| *i == 0 || *i == 3), "no second Inbox from another account: {rail:?}");
    assert_eq!(rail.iter().filter(|(i, _)| *i == 3).count(), MODULES);
}

#[test]
fn a_folder_with_folders_under_it_has_a_triangle_and_a_closed_one_hides_them() {
    crate::l10n::in_english();
    let accounts = accounts();
    let folders = folders();
    let open = vec![true; 4];
    let mut closed = BTreeSet::new();
    let sections = plan(&input(&accounts, &folders, &open, &closed));
    let work = sections[0].rows.iter().find(|r| r.label == "Work").expect("Work");
    assert_eq!(work.disclosure, Some(true));
    assert!(sections[0].rows.iter().any(|r| r.label == "Projects"));
    assert_eq!(sections[0].rows.iter().find(|r| r.label == "Inbox").map(|r| r.disclosure), Some(None));
    let projects_tag = sections[0].rows.iter().find(|r| r.label == "Projects").map(|r| r.tag);
    closed.insert((0, String::from("Work")));
    let sections = plan(&input(&accounts, &folders, &open, &closed));
    let work = sections[0].rows.iter().find(|r| r.label == "Work").expect("Work");
    assert_eq!(work.disclosure, Some(false), "the triangle points the other way");
    assert!(!sections[0].rows.iter().any(|r| r.label == "Projects"), "the child is hidden");
    // opening and closing a folder moves no other folder's tag
    let again = plan(&input(&accounts, &folders, &open, &BTreeSet::new()));
    assert_eq!(again[0].rows.iter().find(|r| r.label == "Projects").map(|r| r.tag), projects_tag);
}

#[test]
fn a_folder_closed_in_another_mailbox_stays_open_here() {
    crate::l10n::in_english();
    let accounts = accounts();
    let folders = folders();
    let open = vec![true; 4];
    let closed: BTreeSet<(usize, String)> = [(1, String::from("Work"))].into_iter().collect();
    let sections = plan(&input(&accounts, &folders, &open, &closed));
    assert!(sections[0].rows.iter().any(|r| r.label == "Projects"));
}

#[test]
fn a_section_is_open_as_the_mailbox_s_slot_says_and_open_without_one() {
    let accounts = accounts();
    let folders = folders();
    let closed = BTreeSet::new();
    // slot 0 is unused; the first account's is slot 1
    let open = vec![true, false, true, true];
    let sections = plan(&input(&accounts, &folders, &open, &closed));
    assert!(!sections[0].open && sections[1].open && sections[2].open);
    let none: Vec<bool> = Vec::new();
    let sections = plan(&input(&accounts, &folders, &none, &closed));
    assert!(sections.iter().all(|s| s.open), "no slot: open");
}

#[test]
fn the_tag_of_a_row_leads_back_to_its_folder() {
    crate::l10n::in_english();
    let accounts = accounts();
    let folders = folders();
    let open = vec![true; 4];
    let closed = BTreeSet::new();
    let sections = plan(&input(&accounts, &folders, &open, &closed));
    for row in &sections[0].rows {
        let Some(NavTarget::Folder { mailbox, node }) = target_of(row.tag) else {
            panic!("a folder row: {row:?}");
        };
        assert_eq!(mailbox, 0);
        let key = key_at(&folders[mailbox], node).expect("a folder at that place");
        let info = folders[mailbox].iter().find(|f| f.key == key).expect("a known folder");
        assert_eq!(listing::label_of(info), row.label);
    }
    assert_eq!(key_at(&folders[0], 1000), None);
}

#[test]
fn the_mail_button_carries_the_inbox_s_unread_count_and_the_others_none() {
    let accounts = accounts();
    let folders = folders();
    let open = vec![true; 4];
    let closed = BTreeSet::new();
    let sections = plan(&input(&accounts, &folders, &open, &closed));
    let unread: Vec<usize> = sections[3].rows.iter().map(|r| r.unread).collect();
    assert_eq!(unread, vec![12, 0, 0, 0]);
}

#[test]
fn a_window_with_no_account_still_has_local_folders_and_the_modules() {
    let accounts: Vec<String> = Vec::new();
    let folders = vec![vec![info("drafts", Role::Drafts, 0), info("sent", Role::Sent, 0)]];
    let open = vec![true; 2];
    let closed = BTreeSet::new();
    let mut none = input(&accounts, &folders, &open, &closed);
    none.local = Some((0, String::from("Local Folders")));
    none.current = None;
    none.folder = None;
    let sections = plan(&none);
    assert_eq!(sections.len(), 2);
    assert_eq!(sections[0].mailbox, Some(0));
    assert!(sections[0].rows.iter().all(|r| !r.in_rail), "no mailbox is shown: none is in the rail");
}

#[test]
fn more_mailboxes_than_the_limit_are_left_out() {
    let accounts: Vec<String> = (0..MAX_MAILBOXES + 5).map(|i| format!("a{i}@example.org")).collect();
    let folders: Vec<Vec<FolderInfo>> = Vec::new();
    let open = Vec::new();
    let closed = BTreeSet::new();
    let mut many = input(&accounts, &folders, &open, &closed);
    many.local = None;
    let sections = plan(&many);
    assert_eq!(sections.len(), MAX_MAILBOXES + 1, "the limit's mailboxes and the modules");
}

#[test]
fn a_mailbox_with_more_folders_than_the_limit_is_cut_and_says_how_many() {
    let mut list: Vec<FolderInfo> = (0..MAX_FOLDERS + 7).map(|i| info(&format!("f{i}"), Role::Other, 0)).collect();
    assert_eq!(cap_folders(&mut list), 7);
    assert_eq!(list.len(), MAX_FOLDERS);
    assert_eq!(cap_folders(&mut list), 0);
}

// ---- the rail's quick access ----

#[test]
fn the_most_used_quick_button_is_the_last_one() {
    assert_eq!(QUICK.last(), Some(&Quick::Compose));
    let plan = quick_plan(QuickInput { flagged: None, unread: None, syncing: false });
    assert_eq!(plan.iter().map(|p| p.quick).collect::<Vec<_>>(), QUICK.to_vec());
}

#[test]
fn a_tag_leads_back_to_its_quick_button() {
    for quick in QUICK {
        assert_eq!(quick_of(quick_tag(quick)), Some(quick));
    }
    assert_eq!(quick_of(0), None);
    assert_eq!(quick_of(QUICK.len() + 1), None);
}

#[test]
fn flag_and_read_cannot_run_with_no_message_selected_and_toggle_with_one() {
    let state = |plan: &[QuickPlan], quick: Quick| plan.iter().find(|p| p.quick == quick).map(|p| p.state);
    let none = quick_plan(QuickInput { flagged: None, unread: None, syncing: false });
    assert_eq!(state(&none, Quick::ToggleFlag), Some(QuickState::Disabled));
    assert_eq!(state(&none, Quick::ToggleRead), Some(QuickState::Disabled));
    assert_eq!(state(&none, Quick::Compose), Some(QuickState::Plain), "Compose needs no message");
    assert_eq!(state(&none, Quick::SendReceive), Some(QuickState::Plain));
    let one = quick_plan(QuickInput { flagged: Some(true), unread: Some(false), syncing: false });
    assert_eq!(state(&one, Quick::ToggleFlag), Some(QuickState::On), "flagged: lit");
    assert_eq!(state(&one, Quick::ToggleRead), Some(QuickState::Off), "read: not lit");
}

#[test]
fn a_running_send_receive_is_lit() {
    let plan = quick_plan(QuickInput { flagged: None, unread: None, syncing: true });
    assert_eq!(plan[0].state, QuickState::On);
}

// ---- the message's sheet ----

#[test]
fn the_sheet_has_a_destructive_row_and_it_is_the_last() {
    let danger: Vec<&SheetRow> = SHEET.iter().filter(|r| r.danger).collect();
    assert_eq!(danger.len(), 1);
    assert_eq!(danger[0].action, SheetAction::Delete);
    assert_eq!(SHEET.last().map(|r| r.action), Some(SheetAction::Delete));
}

#[test]
fn a_sheet_tag_leads_back_to_its_row() {
    for (i, row) in SHEET.iter().enumerate() {
        assert_eq!(sheet_row_of(sheet_tag(i)).map(|r| r.action), Some(row.action));
    }
    assert!(sheet_row_of(0).is_none() && sheet_row_of(SHEET.len() + 1).is_none());
    // the family's sheet holds at most 24 rows
    assert!(SHEET.len() <= 24);
}

// ---- the preview follows the top edge ----

/// The uid of the message at the top edge.
fn top_message(rows: &[ListRow], first: usize) -> Option<u32> {
    top_message_at(rows, first).map(|(_, uid)| uid)
}

#[test]
fn the_top_edge_is_the_first_visible_message_and_a_group_header_is_none() {
    let rows = vec![
        ListRow::Group(DateGroup::Today),
        ListRow::Message(30),
        ListRow::Message(29),
        ListRow::Group(DateGroup::Yesterday),
        ListRow::Message(20),
    ];
    assert_eq!(top_message(&rows, 0), Some(30), "the header is skipped");
    assert_eq!(top_message(&rows, 1), Some(30));
    assert_eq!(top_message(&rows, 2), Some(29));
    assert_eq!(top_message(&rows, 3), Some(20), "the header at the top edge: the message under it");
    assert_eq!(top_message(&rows, 5), None, "past the last row");
    assert_eq!(top_message(&rows, usize::MAX), None);
    assert_eq!(top_message(&[], 0), None);
}

#[test]
fn a_tap_holds_the_preview_until_the_top_edge_has_really_moved() {
    // no tap holds it: it follows
    assert!(follows_top_edge(None, 0));
    assert!(follows_top_edge(None, 100));
    // a tap at top row 10: the list nudging itself to keep the row in view is not the reader's
    assert!(!follows_top_edge(Some(10), 10));
    assert!(!follows_top_edge(Some(10), 11));
    assert!(!follows_top_edge(Some(10), 9));
    // two rows or more, either way, is a scroll
    assert!(follows_top_edge(Some(10), 12));
    assert!(follows_top_edge(Some(10), 8));
    assert!(follows_top_edge(Some(10), usize::MAX));
    assert!(follows_top_edge(Some(usize::MAX), 0));
}

// ---- a scroll moves the selection and the preview on a phone, a tap holds them ----

fn mail_rows() -> Vec<ListRow> {
    vec![
        ListRow::Group(DateGroup::Today),
        ListRow::Message(30),
        ListRow::Message(29),
        ListRow::Message(28),
        ListRow::Group(DateGroup::Yesterday),
        ListRow::Message(20),
        ListRow::Message(19),
    ]
}

#[test]
fn scrolling_moves_the_selection_and_the_preview_to_the_message_at_the_top_edge() {
    let rows = mail_rows();
    assert_eq!(follow(&rows, 2, None, Some(30), true), Follow::Move { row: 2, uid: 29 });
    assert_eq!(follow(&rows, 3, None, Some(29), true), Follow::Move { row: 3, uid: 28 });
}

#[test]
fn a_group_header_at_the_top_edge_hands_it_to_the_first_message_under_it() {
    let rows = mail_rows();
    assert_eq!(follow(&rows, 4, None, Some(28), true), Follow::Move { row: 5, uid: 20 });
    assert_eq!(follow(&rows, 0, None, None, true), Follow::Move { row: 1, uid: 30 });
}

#[test]
fn a_scroll_that_leaves_the_same_message_at_the_top_changes_nothing() {
    let rows = mail_rows();
    assert_eq!(follow(&rows, 1, None, Some(30), true), Follow::Keep);
    assert_eq!(follow(&rows, 0, None, Some(30), true), Follow::Keep, "the header over it: the same message");
}

#[test]
fn a_scroll_past_the_last_message_changes_nothing() {
    let rows = mail_rows();
    assert_eq!(follow(&rows, 7, None, Some(19), true), Follow::Keep);
    assert_eq!(follow(&rows, usize::MAX, None, None, true), Follow::Keep);
    assert_eq!(follow(&[], 0, None, None, true), Follow::Keep);
}

#[test]
fn a_tap_holds_the_selection_and_the_preview_until_the_list_is_scrolled_two_rows() {
    let rows = mail_rows();
    // a tap at top row 1 shows message 29 (row 2); the list nudges itself by one row: still held
    assert_eq!(follow(&rows, 2, Some(1), Some(29), true), Follow::Keep);
    assert_eq!(follow(&rows, 0, Some(1), Some(29), true), Follow::Keep);
    // the reader scrolls on: the selection and the preview follow again
    assert_eq!(follow(&rows, 3, Some(1), Some(29), true), Follow::Move { row: 3, uid: 28 });
    assert_eq!(follow(&rows, 5, Some(1), Some(29), true), Follow::Move { row: 5, uid: 20 });
}

#[test]
fn a_desktop_scroll_moves_nothing() {
    let rows = mail_rows();
    for first in 0..rows.len() {
        assert_eq!(follow(&rows, first, None, Some(1), false), Follow::Keep, "{first}");
    }
}
