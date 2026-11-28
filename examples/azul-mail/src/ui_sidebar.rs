//! The main window's sidebar (azul's `Sidebar`): every account's folder tree as a section (its
//! address the title, the unread counts as badges, a triangle for a folder with folders under
//! it), Local Folders, then the four module buttons. What the sidebar shows and what its tags
//! mean is `nav.rs`; this file turns it into widgets and runs what the sidebar reports.
//!
//! Collapsed it is the 18 px rail: the shown mailbox's top-level folders and the module buttons at
//! the top, the quick-access buttons (the app's own: Send / Receive, flag, read, and Compose
//! last, nearest the thumb) at the bottom, then the status slot - a pie for the Send / Receive
//! that runs or came to its end, a badge for the unread mail - and the toggle. On a phone the
//! drawer pushes the window aside, and choosing a folder closes it.

use azul::{
    callbacks::SidebarOnEventCallbackType,
    prelude::*,
    widgets::{
        PieTone, Sidebar, SidebarEvent, SidebarEventKind, SidebarQuickAction, SidebarQuickState,
        SidebarRow, SidebarSection, SidebarState, SidebarStatus, SidebarSurface, StatusBarSyncKind,
    },
};
use azul_appkit::l10n::{label, t, t_args, Arg};

use crate::{
    folders::Role,
    listing,
    nav::{self, NavRow, NavTarget, Quick, QuickState},
    ui_backstage, ui_frame,
    ui_main::{self, Action},
    with_app, MailApp, SyncState,
};

/// The sidebar of the window now.
pub(crate) fn sidebar(s: &MailApp, app: &RefAny) -> Sidebar {
    let accounts: Vec<String> = s.accounts.iter().map(|a| a.email.clone()).collect();
    let local = s.local_visible().then(|| (s.local_index(), listing::local_folders()));
    let inbox_unread = s.current.and_then(|i| s.folders.get(i)).map_or(0, |list| {
        list.iter()
            .filter(|f| f.role == Role::Inbox)
            .map(|f| f.unread)
            .sum()
    });
    let plan = nav::plan(&nav::NavInput {
        accounts: &accounts,
        local,
        folders: &s.folders,
        current: s.current,
        folder: s.folder.as_deref(),
        groups_open: &s.groups_open,
        closed: &s.closed_folders,
        module: s.module,
        module_labels: [
            t("azmail-module-mail"),
            t("azmail-module-calendar"),
            t("azmail-module-contacts"),
            t("azmail-module-tasks"),
        ],
        inbox_unread,
    });
    let sections: Vec<SidebarSection> = plan
        .iter()
        .map(|section| {
            let mut built = SidebarSection::create(section.title.as_str())
                .with_rows(section.rows.iter().map(sidebar_row).collect::<Vec<_>>());
            // a mailbox's title row opens and closes its tree; the module buttons have no title
            if section.mailbox.is_some() {
                built = built.with_collapsible(section.open);
            }
            built
        })
        .collect();

    let mut sidebar = Sidebar::create()
        .with_sections(sections)
        .with_quick_access(quick_actions(s))
        .with_help(label("azmail-file-help"))
        .with_label(label("azmail-module-mail"))
        .with_state(s.sidebar)
        .with_surface(SidebarSurface::Theme)
        .with_form(s.form)
        .with_on_event(app.clone(), on_sidebar as SidebarOnEventCallbackType);
    for status in statuses(s) {
        sidebar = sidebar.with_status(status);
    }
    sidebar
}

/// A row of the plan as a row of the sidebar.
fn sidebar_row(row: &NavRow) -> SidebarRow {
    let mut built = SidebarRow::create(row.label.as_str())
        .with_icon(row.icon)
        .with_tag(row.tag)
        .with_depth(row.depth)
        .with_selected(row.selected)
        .with_in_rail(row.in_rail);
    if row.unread > 0 {
        built = built.with_badge(row.unread.to_string());
    }
    if let Some(open) = row.disclosure {
        built = built.with_disclosure(open);
    }
    built
}

// ---- the status slot ----

/// The pie of a Send / Receive in state `kind`: how far it is and what that means; `None` with no
/// account (nothing is connected, there is nothing to say). `percent` is the running one's.
#[must_use]
pub(crate) fn sync_pie(kind: StatusBarSyncKind, percent: Option<f32>) -> Option<(f32, PieTone)> {
    match kind {
        StatusBarSyncKind::Syncing => {
            let fraction = percent.filter(|p| p.is_finite()).map_or(0.5, |p| (p / 100.0).clamp(0.0, 1.0));
            Some((fraction, PieTone::Busy))
        }
        StatusBarSyncKind::Connected => Some((1.0, PieTone::Good)),
        StatusBarSyncKind::Error => Some((1.0, PieTone::Danger)),
        StatusBarSyncKind::Offline => None,
    }
}

/// The unread mail of every mailbox's Inbox (the slot's badge).
#[must_use]
pub(crate) fn unread_total(s: &MailApp) -> usize {
    s.folders
        .iter()
        .flat_map(|list| list.iter())
        .filter(|f| f.role == Role::Inbox)
        .map(|f| f.unread)
        .sum()
}

/// What the status slot holds: the Send / Receive's pie and the unread badge. The slot is the app's
/// data: nothing here is the widget's.
fn statuses(s: &MailApp) -> Vec<SidebarStatus> {
    let mut out = Vec::new();
    let (line, kind) = ui_main::sync_line(&s.sync, s.accounts.is_empty(), || ui_main::up_to_date(s));
    let percent = match &s.sync {
        SyncState::Running { percent, .. } => Some(*percent),
        _ => None,
    };
    if let Some((fraction, tone)) = sync_pie(kind, percent) {
        out.push(
            SidebarStatus::create(t("azmail-tab-send-receive"), fraction, tone)
                .with_text(line.as_str())
                .with_detail(line.as_str()),
        );
    }
    let unread = unread_total(s);
    if unread > 0 {
        out.push(
            SidebarStatus::badge(t("azmail-sidebar-unread"), unread.to_string(), PieTone::Accent)
                .with_detail(t_args("azmail-status-unread", &[("count", Arg::from(unread))])),
        );
    }
    out
}

// ---- the quick-access buttons ----

/// The quick-access state the selection gives: whether the first selected message is flagged and
/// unread (`None`: no message selected), and whether a Send / Receive runs.
fn quick_input(s: &MailApp) -> nav::QuickInput {
    let selected = ui_main::selected_uids(s);
    let first = selected
        .first()
        .and_then(|uid| s.entries.iter().find(|e| e.uid == *uid));
    nav::QuickInput {
        flagged: first.map(|e| s.flags.is_flagged(e)),
        unread: first.map(|e| !s.flags.is_read(e)),
        syncing: matches!(s.sync, SyncState::Running { .. }),
    }
}

/// The rail's quick-access buttons: Send / Receive, flag, read, then Compose nearest the thumb.
fn quick_actions(s: &MailApp) -> Vec<SidebarQuickAction> {
    nav::quick_plan(quick_input(s))
        .into_iter()
        .map(|plan| {
            SidebarQuickAction::create(label(plan.label), plan.icon)
                .with_tag(nav::quick_tag(plan.quick))
                .with_state(match plan.state {
                    QuickState::Plain => SidebarQuickState::Plain,
                    QuickState::Off => SidebarQuickState::Off,
                    QuickState::On => SidebarQuickState::On,
                    QuickState::Disabled => SidebarQuickState::Disabled,
                })
        })
        .collect()
}

/// What a quick-access button runs.
#[must_use]
pub(crate) const fn quick_command(quick: Quick) -> Action {
    match quick {
        Quick::SendReceive => Action::SendReceive,
        Quick::ToggleFlag => Action::ToggleFlag,
        Quick::ToggleRead => Action::ToggleRead,
        Quick::Compose => Action::NewMail,
    }
}

// ---- what the sidebar reports ----

/// Every report of the sidebar: a folder or a module chosen, a triangle, a mailbox's title, the
/// toggle, Help, a quick-access button.
extern "C" fn on_sidebar(mut data: RefAny, mut info: CallbackInfo, event: SidebarEvent) -> Update {
    if event.kind == SidebarEventKind::QuickAction {
        return match nav::quick_of(event.tag) {
            Some(quick) => ui_main::run_action(&mut data, &mut info, quick_command(quick)),
            None => Update::DoNothing,
        };
    }
    with_app(&mut data, |s, _| {
        let phone = ui_frame::phone_now(s);
        match event.kind {
            SidebarEventKind::RowClicked => {
                if !choose(s, event.tag) {
                    return Update::DoNothing;
                }
                s.sidebar = ui_frame::after_choice(s.sidebar, phone);
            }
            SidebarEventKind::RowToggled => match nav::target_of(event.tag) {
                Some(NavTarget::Folder { mailbox, node }) => {
                    let Some(key) = s.folders.get(mailbox).and_then(|list| nav::key_at(list, node)) else {
                        return Update::DoNothing;
                    };
                    if event.expand {
                        s.closed_folders.remove(&(mailbox, key));
                    } else {
                        s.closed_folders.insert((mailbox, key));
                    }
                }
                _ => return Update::DoNothing,
            },
            SidebarEventKind::SectionToggled => {
                // the sections are the mailboxes, in order, then the module buttons
                let Some(open) = s.groups_open.get_mut(event.index + 1) else {
                    return Update::DoNothing;
                };
                *open = event.expand;
            }
            SidebarEventKind::StateChanged => {
                s.sidebar = event.state;
                // a desktop remembers the sidebar collapsed or open; a phone's drawer is for now
                if !phone {
                    s.nav_collapsed = event.state != SidebarState::Expanded;
                    ui_main::remember_nav(s, &mut info);
                }
            }
            SidebarEventKind::HelpClicked => {
                s.backstage = Some(ui_backstage::PAGE_HELP);
                s.sidebar = ui_frame::after_choice(s.sidebar, phone);
            }
            _ => return Update::DoNothing,
        }
        Update::RefreshDom
    })
    .unwrap_or(Update::DoNothing)
}

/// Shows what the row with `tag` stands for: a folder (its mailbox first) or a module. `false`
/// when the tag is none of the window's.
fn choose(s: &mut MailApp, tag: usize) -> bool {
    match nav::target_of(tag) {
        Some(NavTarget::Folder { mailbox, node }) => {
            let shown = mailbox < s.accounts.len() || (mailbox == s.local_index() && s.local_visible());
            if !shown {
                return false;
            }
            if s.current != Some(mailbox) {
                s.show_account(mailbox);
            }
            let Some(key) = s.folders.get(mailbox).and_then(|list| nav::key_at(list, node)) else {
                return false;
            };
            s.show_folder(&key);
            s.module = 0;
            s.scroll = nav::ListScroll::new();
            true
        }
        Some(NavTarget::Module(module)) => {
            s.module = module;
            true
        }
        None => false,
    }
}

#[cfg(test)]
#[path = "ui_sidebar_test.rs"]
mod ui_sidebar_test;
