//! The window's sidebar (azul's `Sidebar`): the ERP's modules as sections, the pages of a module as
//! its rows (the views' menu: Register, Categories, Locations, ...) with the number of records they
//! list as badges. What the sidebar shows and what its tags mean is `nav.rs`; this file turns it
//! into widgets and runs what the sidebar reports.
//!
//! Collapsed it is the 18 px rail: the pages' icons at the top, the quick-access buttons (CSV
//! import, CSV export, and New last, nearest the thumb) at the bottom, then the status slot - a pie
//! for what is saved, a badge for what is overdue - and the toggle. On a phone the drawer pushes
//! the window aside, and choosing a page closes it.

use azul::{
    callbacks::SidebarOnEventCallbackType,
    prelude::*,
    widgets::{
        PieTone, Sidebar, SidebarEvent, SidebarEventKind, SidebarQuickAction, SidebarRow,
        SidebarSection, SidebarStatus, SidebarSurface,
    },
};
use azul_appkit::{
    l10n::{label, t, t_args, Arg},
    ui as kit,
};

use super::{frame, run_action, with_erp, Erp};
use crate::{
    ids, nav, reports,
    views::{
        spec::{ActionKind, ActionSpec},
        Params,
    },
};

/// The ERP's modules as the sidebar lists them now: the asset module, its pages the views' menu,
/// each with the number of records it lists.
pub(crate) fn modules(s: &Erp) -> Vec<nav::Module> {
    let views = &s.state.views;
    let paths: Vec<&str> = views.menu.iter().map(|m| m.path.as_str()).collect();
    let pages: Vec<nav::Page> = views
        .menu
        .iter()
        .map(|entry| nav::Page {
            path: entry.path.clone(),
            label: s.state.labels.get(&entry.key),
            icon: nav::page_icon(&entry.key),
            count: views
                .route(&entry.path)
                .and_then(|route| route.view.kind_of_records())
                .map(|kind| s.state.book.count(kind)),
        })
        .collect();
    vec![nav::Module {
        title: t("azerp-module-assets"),
        pages,
        current: nav::current_page(&paths, &s.state.page),
        open: s.module_open,
    }]
}

/// The sidebar of the window now.
pub(crate) fn sidebar(s: &Erp, app: &RefAny) -> Sidebar {
    let sections: Vec<SidebarSection> = nav::plan(&modules(s))
        .iter()
        .map(|section| {
            SidebarSection::create(section.title.as_str())
                .with_rows(section.rows.iter().map(sidebar_row).collect::<Vec<_>>())
                // a module's title row opens and closes its pages
                .with_collapsible(section.open)
        })
        .collect();
    let mut sidebar = Sidebar::create()
        .with_sections(sections)
        .with_quick_access(quick_actions(s))
        .with_help(label("azerp-help"))
        .with_label(label("azerp-modules"))
        .with_state(s.sidebar)
        .with_surface(SidebarSurface::Theme)
        .with_form(s.form)
        .with_dom_id(ids::SIDEBAR)
        .with_on_event(app.clone(), on_sidebar as SidebarOnEventCallbackType);
    for status in statuses(s) {
        sidebar = sidebar.with_status(status);
    }
    sidebar
}

/// A row of the plan as a row of the sidebar.
fn sidebar_row(row: &nav::NavRow) -> SidebarRow {
    let mut built = SidebarRow::create(row.label.as_str())
        .with_icon(row.icon)
        .with_tag(row.tag)
        .with_selected(row.selected);
    if !row.badge.is_empty() {
        built = built.with_badge(row.badge.clone());
    }
    built
}

// ---- the status slot ----

/// What is overdue: check-outs past their due day and services past theirs.
#[must_use]
pub(crate) fn overdue(s: &Erp) -> (usize, usize) {
    let today = s.state.today;
    let checkouts = reports::overdue_checkouts(&s.state.book, today).len();
    let services = reports::maintenance_due(&s.state.book, today, 0)
        .iter()
        .filter(|d| d.overdue)
        .count();
    (checkouts, services)
}

/// What the status slot holds: the writes' pie and the overdue badge. The slot is the app's data:
/// nothing here is the widget's.
fn statuses(s: &Erp) -> Vec<SidebarStatus> {
    let mut out = Vec::new();
    let writing = s.state.queue.pending() + s.in_flight.len();
    let sync = if writing == 0 {
        SidebarStatus::create(t("azerp-sync"), 1.0, PieTone::Good)
            .with_text(t("azerp-sync-saved").as_str())
            .with_detail(t("azerp-sync-saved-detail").as_str())
    } else {
        SidebarStatus::create(t("azerp-sync"), 0.5, PieTone::Busy)
            .with_text(t("azerp-sync-saving").as_str())
            .with_detail(t_args("azerp-status-saving", &[("count", Arg::from(writing))]).as_str())
    };
    out.push(sync);
    let (checkouts, services) = overdue(s);
    if checkouts + services > 0 {
        out.push(
            SidebarStatus::badge(
                t("azerp-overdue"),
                (checkouts + services).to_string(),
                PieTone::Danger,
            )
            .with_detail(
                t_args(
                    "azerp-overdue-detail",
                    &[
                        ("checkouts", Arg::from(checkouts)),
                        ("services", Arg::from(services)),
                    ],
                )
                .as_str(),
            ),
        );
    }
    out
}

// ---- the quick-access buttons ----

/// Whether the action is the CSV import's page.
fn is_import(action: &ActionSpec) -> bool {
    matches!(&action.kind, ActionKind::Link(path) if path == crate::app::IMPORT)
}

/// Whether the action is the register's CSV export.
fn is_export(action: &ActionSpec) -> bool {
    matches!(&action.kind, ActionKind::Named(name) if name == "export_csv")
}

/// Whether the action makes a new record (the page's primary link).
fn is_new(action: &ActionSpec) -> bool {
    action.primary && matches!(action.kind, ActionKind::Link(_))
}

/// What the page has for the quick access: a form open over the page is the user's business, the
/// rail leaves the page's commands alone until it is closed.
fn quick_input(s: &Erp) -> nav::QuickInput {
    if s.state.form.is_some() {
        return nav::QuickInput::default();
    }
    let actions = super::page_actions(s);
    nav::QuickInput {
        has_new: actions.iter().any(is_new),
        has_import: actions.iter().any(is_import),
        has_export: actions.iter().any(is_export),
    }
}

/// The rail's quick-access buttons: CSV import, CSV export, then New nearest the thumb.
fn quick_actions(s: &Erp) -> Vec<SidebarQuickAction> {
    nav::quick_plan(quick_input(s))
        .into_iter()
        .map(|plan| {
            SidebarQuickAction::create(label(plan.label_key), plan.icon)
                .with_tag(nav::quick_tag(plan.quick))
        })
        .collect()
}

/// Runs what a quick-access button stands for: the page's own action of that kind.
fn run_quick(s: &mut Erp, quick: nav::Quick) {
    let actions = super::page_actions(s);
    let pick = actions.iter().find(|a| match quick {
        nav::Quick::New => is_new(a),
        nav::Quick::ImportCsv => is_import(a),
        nav::Quick::ExportCsv => is_export(a),
    });
    if let Some(action) = pick {
        let kind = action.kind.clone();
        run_action(s, &kind, &Params::new());
    }
}

// ---- what the sidebar reports ----

/// Shows the page the row with `tag` stands for; `false` when the tag is none of the window's.
pub(crate) fn open_page(s: &mut Erp, tag: usize) -> bool {
    let (_, page) = nav::target(tag);
    let Some(path) = s.state.views.menu.get(page).map(|m| m.path.clone()) else {
        return false;
    };
    s.state.open(&path);
    println!("AZERP_PAGE {}", s.state.page);
    true
}

/// Every report of the sidebar: a page chosen, a module's title, the toggle, Help, a
/// quick-access button.
extern "C" fn on_sidebar(mut data: RefAny, mut info: CallbackInfo, event: SidebarEvent) -> Update {
    with_erp(&mut data, &mut info, |s, _| {
        let phone = frame::phone_now(s);
        match event.kind {
            SidebarEventKind::RowClicked => {
                if open_page(s, event.tag) {
                    s.sidebar = frame::after_choice(s.sidebar, phone);
                }
            }
            SidebarEventKind::SectionToggled => s.module_open = event.expand,
            SidebarEventKind::StateChanged => s.sidebar = event.state,
            SidebarEventKind::HelpClicked => {
                kit::open_settings(&s.kit, Some("Shortcuts"));
                s.sidebar = frame::after_choice(s.sidebar, phone);
            }
            SidebarEventKind::QuickAction => {
                if let Some(quick) = nav::quick_of(event.tag) {
                    run_quick(s, quick);
                }
            }
            _ => {}
        }
    })
}

#[cfg(test)]
#[path = "sidebar_test.rs"]
mod sidebar_test;
