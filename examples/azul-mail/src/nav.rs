//! What the main window's sidebar shows and what its buttons do, as plain data: the accounts'
//! folder trees as sections of rows, the module buttons, the quick-access buttons of the rail
//! and the rows of a message's action sheet. No azul types here (`ui_sidebar.rs`, `ui_sheet.rs`
//! turn these into widgets), so every rule is a plain test (`nav_test.rs`).
//!
//! A row of the sidebar carries ONE number, its tag, and the tag says what the row is: a folder
//! of a mailbox (the mailbox's index and the folder's place in the mailbox's depth-first order,
//! hidden children counted, so a folder keeps its tag while a parent opens and closes) or one of
//! the four module buttons.

use std::collections::BTreeSet;

use crate::{
    folders::Role,
    listing::{self, FolderInfo, FolderNode},
};

/// The most folders of one mailbox the sidebar lists; the rest is dropped when the folders are
/// read (`MailApp::reload_folders` says so once).
pub(crate) const MAX_FOLDERS: usize = 4_000;
/// The most mailboxes (the accounts, then Local Folders) the sidebar lists.
pub(crate) const MAX_MAILBOXES: usize = 200;
/// How many tags one mailbox's folders take.
const NODE_SPAN: usize = 10_000;
/// The tag of the first module button (Mail); far above every folder's.
const MODULE_BASE: usize = 1 << 28;
/// Mail, Calendar, Contacts, Tasks.
pub(crate) const MODULES: usize = 4;

/// What a row of the sidebar stands for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum NavTarget {
    /// A folder: its mailbox and its place among the mailbox's folders (depth first).
    Folder { mailbox: usize, node: usize },
    /// A module button: 0 Mail, 1 Calendar, 2 Contacts, 3 Tasks.
    Module(usize),
}

/// The tag of the folder at place `node` of mailbox `mailbox`.
#[must_use]
pub(crate) fn folder_tag(mailbox: usize, node: usize) -> usize {
    1 + mailbox.min(MAX_MAILBOXES) * NODE_SPAN + node.min(NODE_SPAN - 1)
}

/// The tag of module button `module`.
#[must_use]
pub(crate) fn module_tag(module: usize) -> usize {
    MODULE_BASE + module.min(MODULES - 1)
}

/// What `tag` stands for; `None` for 0 and for a number no row has.
#[must_use]
pub(crate) fn target_of(tag: usize) -> Option<NavTarget> {
    if tag >= MODULE_BASE {
        let module = tag - MODULE_BASE;
        return (module < MODULES).then_some(NavTarget::Module(module));
    }
    let n = tag.checked_sub(1)?;
    let (mailbox, node) = (n / NODE_SPAN, n % NODE_SPAN);
    (mailbox <= MAX_MAILBOXES).then_some(NavTarget::Folder { mailbox, node })
}

/// A folder's icon in the sidebar.
#[must_use]
pub(crate) fn folder_icon(role: Role) -> &'static str {
    match role {
        Role::Inbox => "inbox",
        Role::Drafts => "drafts",
        Role::Sent => "send",
        Role::Trash => "delete",
        Role::Spam => "report",
        Role::Archive => "archive",
        Role::All => "all_inbox",
        Role::Flagged => "flag",
        Role::Other => "folder",
    }
}

/// The icons of the four module buttons.
pub(crate) static MODULE_ICONS: [&str; MODULES] = ["mail", "calendar_month", "contacts", "task_alt"];

/// One row of the sidebar.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct NavRow {
    pub tag: usize,
    pub depth: usize,
    pub label: String,
    pub icon: &'static str,
    /// The unread count (a mailbox's Outbox: how many wait in it); 0 shows none.
    pub unread: usize,
    /// `Some(open)` for a folder with folders under it.
    pub disclosure: Option<bool>,
    pub selected: bool,
    /// A button of the rail: only the shown mailbox's top level and the module buttons are.
    pub in_rail: bool,
}

/// One section of the sidebar: a mailbox's folder tree, or the module buttons.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct NavSection {
    /// The account's address, "Local Folders"; empty for the module buttons (no title row).
    pub title: String,
    /// The mailbox the section is (its title row opens and closes it); `None` for the modules.
    pub mailbox: Option<usize>,
    pub open: bool,
    pub rows: Vec<NavRow>,
}

/// What the plan reads.
pub(crate) struct NavInput<'a> {
    /// Each account's address, in the accounts' order (mailbox `i` is account `i`).
    pub accounts: &'a [String],
    /// Local Folders, when they show: their mailbox index and their name.
    pub local: Option<(usize, String)>,
    /// Every mailbox's folders with their unread counts.
    pub folders: &'a [Vec<FolderInfo>],
    /// The mailbox shown and its folder's key.
    pub current: Option<usize>,
    pub folder: Option<&'a str>,
    /// Which mailboxes are open: slot 0 is unused, then one per mailbox.
    pub groups_open: &'a [bool],
    /// The folders whose children are hidden: (mailbox, key).
    pub closed: &'a BTreeSet<(usize, String)>,
    /// The module shown (0 Mail).
    pub module: usize,
    /// The module buttons' labels, in the window's language.
    pub module_labels: [String; MODULES],
    /// The unread count of the shown mailbox's Inbox, shown at the Mail button.
    pub inbox_unread: usize,
}

/// The sidebar's sections: every account's folder tree, Local Folders' when they show, then the
/// module buttons.
#[must_use]
pub(crate) fn plan(input: &NavInput<'_>) -> Vec<NavSection> {
    let mut sections = Vec::new();
    let mailboxes = input
        .accounts
        .iter()
        .enumerate()
        .map(|(i, email)| (i, email.clone()))
        .chain(input.local.clone())
        .take(MAX_MAILBOXES);
    for (mailbox, title) in mailboxes {
        let empty: &[FolderInfo] = &[];
        let infos = input.folders.get(mailbox).map_or(empty, Vec::as_slice);
        let here = input.current == Some(mailbox);
        let mut rows = Vec::new();
        let mut place = 0usize;
        let tree = listing::folder_tree(infos);
        let walk = Walk {
            mailbox,
            infos,
            here,
            folder: input.folder,
            closed: input.closed,
        };
        walk.nodes(&tree, 0, true, &mut place, &mut rows);
        sections.push(NavSection {
            title,
            mailbox: Some(mailbox),
            open: input.groups_open.get(mailbox + 1).copied().unwrap_or(true),
            rows,
        });
    }
    let modules = (0..MODULES)
        .map(|module| NavRow {
            tag: module_tag(module),
            depth: 0,
            label: input.module_labels[module].clone(),
            icon: MODULE_ICONS[module],
            unread: if module == 0 { input.inbox_unread } else { 0 },
            disclosure: None,
            selected: input.module == module,
            in_rail: true,
        })
        .collect();
    sections.push(NavSection {
        title: String::new(),
        mailbox: None,
        open: true,
        rows: modules,
    });
    sections
}

/// One mailbox's walk over its folder tree.
struct Walk<'a> {
    mailbox: usize,
    infos: &'a [FolderInfo],
    here: bool,
    folder: Option<&'a str>,
    closed: &'a BTreeSet<(usize, String)>,
}

impl Walk<'_> {
    /// Emits `nodes` (and what is open under them) as rows; `emit` is false under a closed folder:
    /// the places are still counted, so a folder keeps its tag.
    fn nodes(&self, nodes: &[FolderNode], depth: usize, emit: bool, place: &mut usize, out: &mut Vec<NavRow>) {
        for node in nodes {
            let node_place = *place;
            *place += 1;
            let closed = self.closed.contains(&(self.mailbox, node.key.clone()));
            if emit && node_place < MAX_FOLDERS {
                let role = self
                    .infos
                    .iter()
                    .find(|f| f.key == node.key)
                    .map_or(Role::Other, |f| f.role);
                let icon = if node.key == listing::OUTBOX_KEY {
                    "outbox"
                } else {
                    folder_icon(role)
                };
                out.push(NavRow {
                    tag: folder_tag(self.mailbox, node_place),
                    depth,
                    label: node.label.clone(),
                    icon,
                    unread: node.unread,
                    disclosure: (!node.children.is_empty()).then_some(!closed),
                    selected: self.here && self.folder == Some(node.key.as_str()),
                    // the rail is the shown mailbox's top level: another account's Inbox would
                    // be a second Inbox button
                    in_rail: self.here && depth == 0,
                });
            }
            self.nodes(&node.children, depth + 1, emit && !closed, place, out);
        }
    }
}

/// The key of the folder at place `node` of a mailbox whose folders are `infos` (what a tag's
/// `NavTarget::Folder` names).
#[must_use]
pub(crate) fn key_at(infos: &[FolderInfo], node: usize) -> Option<String> {
    listing::preorder_keys(&listing::folder_tree(infos)).into_iter().nth(node)
}

/// Cuts a mailbox's folders to [`MAX_FOLDERS`]; how many were dropped.
pub(crate) fn cap_folders(list: &mut Vec<FolderInfo>) -> usize {
    let dropped = list.len().saturating_sub(MAX_FOLDERS);
    list.truncate(MAX_FOLDERS);
    dropped
}

// ---- the rail's quick-access buttons ----

/// What a quick-access button does.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Quick {
    SendReceive,
    ToggleFlag,
    ToggleRead,
    Compose,
}

/// How a quick-access button stands (the widget's `SidebarQuickState`, without the widget).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum QuickState {
    Plain,
    Off,
    On,
    Disabled,
}

/// One quick-access button: what it does, its icon, its words (a key of the resources) and how it
/// stands.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct QuickPlan {
    pub quick: Quick,
    pub icon: &'static str,
    pub label: &'static str,
    pub state: QuickState,
}

/// The tag of a quick-access button: its place in [`QUICK`] plus one.
#[must_use]
pub(crate) fn quick_tag(quick: Quick) -> usize {
    1 + QUICK.iter().position(|q| *q == quick).unwrap_or(0)
}

/// The quick-access button a tag names.
#[must_use]
pub(crate) fn quick_of(tag: usize) -> Option<Quick> {
    tag.checked_sub(1).and_then(|i| QUICK.get(i)).copied()
}

/// The buttons in the order the rail draws them: the most used LAST, nearest to the thumb.
pub(crate) static QUICK: [Quick; 4] = [Quick::SendReceive, Quick::ToggleFlag, Quick::ToggleRead, Quick::Compose];

/// What the quick-access buttons stand on: whether a message is selected, whether it is flagged
/// and whether it is read, and whether a Send / Receive runs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct QuickInput {
    /// The selected message's flag: `None` with no message selected.
    pub flagged: Option<bool>,
    /// The selected message is unread: `None` with no message selected.
    pub unread: Option<bool>,
    pub syncing: bool,
}

/// The rail's quick-access buttons now.
#[must_use]
pub(crate) fn quick_plan(input: QuickInput) -> Vec<QuickPlan> {
    QUICK
        .iter()
        .map(|&quick| match quick {
            Quick::SendReceive => QuickPlan {
                quick,
                icon: "sync",
                label: "azmail-cmd-send-receive-all",
                // a Send / Receive that runs is shown lit; a press while it runs says so
                state: if input.syncing { QuickState::On } else { QuickState::Plain },
            },
            Quick::ToggleFlag => QuickPlan {
                quick,
                icon: "flag",
                label: "azmail-cmd-follow-up",
                state: match input.flagged {
                    None => QuickState::Disabled,
                    Some(true) => QuickState::On,
                    Some(false) => QuickState::Off,
                },
            },
            Quick::ToggleRead => QuickPlan {
                quick,
                icon: "mark_email_unread",
                label: "azmail-cmd-unread-read",
                state: match input.unread {
                    None => QuickState::Disabled,
                    Some(true) => QuickState::On,
                    Some(false) => QuickState::Off,
                },
            },
            Quick::Compose => QuickPlan {
                quick,
                icon: "edit",
                label: "azmail-cmd-new-mail",
                state: QuickState::Plain,
            },
        })
        .collect()
}

// ---- the message's action sheet (a phone) ----

/// What a row of the action sheet does.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SheetAction {
    Reply,
    ReplyAll,
    Forward,
    Archive,
    Delete,
    ToggleFlag,
    ToggleRead,
    Move,
}

/// One row of the sheet: what it does, its icon and words, and whether it is the destructive one.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct SheetRow {
    pub action: SheetAction,
    pub icon: &'static str,
    pub label: &'static str,
    pub danger: bool,
}

/// The rows, in the order the sheet draws them (the destructive one after the others).
pub(crate) static SHEET: [SheetRow; 8] = [
    SheetRow { action: SheetAction::Reply, icon: "reply", label: "azmail-cmd-reply", danger: false },
    SheetRow { action: SheetAction::ReplyAll, icon: "reply_all", label: "azmail-cmd-reply-all", danger: false },
    SheetRow { action: SheetAction::Forward, icon: "forward", label: "azmail-cmd-forward", danger: false },
    SheetRow { action: SheetAction::ToggleFlag, icon: "flag", label: "azmail-cmd-follow-up", danger: false },
    SheetRow { action: SheetAction::ToggleRead, icon: "mark_email_unread", label: "azmail-cmd-unread-read", danger: false },
    SheetRow { action: SheetAction::Move, icon: "drive_file_move", label: "azmail-cmd-move", danger: false },
    SheetRow { action: SheetAction::Archive, icon: "archive", label: "azmail-cmd-archive", danger: false },
    SheetRow { action: SheetAction::Delete, icon: "delete", label: "azmail-cmd-delete", danger: true },
];

/// The tag of a sheet row: its place in [`SHEET`] plus one.
#[must_use]
pub(crate) fn sheet_tag(index: usize) -> usize {
    index + 1
}

/// The sheet row a tag names.
#[must_use]
pub(crate) fn sheet_row_of(tag: usize) -> Option<&'static SheetRow> {
    tag.checked_sub(1).and_then(|i| SHEET.get(i))
}

// ---- the preview follows the top edge of the list ----

/// The message at the top edge of a list whose first visible row is `first`: that row when it is
/// a message, else the next message below it (a group header is no message).
#[must_use]
pub(crate) fn top_message(rows: &[listing::ListRow], first: usize) -> Option<u32> {
    rows.iter().skip(first).find_map(|row| match row {
        listing::ListRow::Message(uid) => Some(*uid),
        listing::ListRow::Group(_) => None,
    })
}

/// How many rows the list's top edge may move after a tap before the scroll counts as the
/// reader's own: a list that nudges itself to keep the tapped row in view moves it by one.
pub(crate) const PIN_SLACK: usize = 2;

/// Whether the phone's Preview follows the list's top edge now: always, except while a tap
/// holds it (`pinned_top`: the top row at the tap) and the top edge has not moved past the slack.
#[must_use]
pub(crate) fn follows_top_edge(pinned_top: Option<usize>, top: usize) -> bool {
    pinned_top.is_none_or(|pinned| top.abs_diff(pinned) >= PIN_SLACK)
}

#[cfg(test)]
#[path = "nav_test.rs"]
mod nav_test;
