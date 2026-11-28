//! The tab ribbon (azul's `Ribbon`): File (the LOCATION, `ui/location.rs`), Home, Reports and View.
//!
//! ```text
//! File:    the location - the module and the page as links over the page's name, big and bold
//! Home:    List     the page's own commands, as its view names them (New asset, Import CSV,
//!                   Export CSV, Depreciation run; in a form Save and Cancel)
//!          Record   the commands of the record the selection is on: Open and, for an asset,
//!                   what its page offers (Edit, Check out, Check in, Log maintenance, Dispose,
//!                   Export schedule, Delete), each where its condition holds
//! Reports: Report   the figures and charts, their CSV      Depreciation  the run, the schedule's CSV
//! View:    Panes    the Preview (a toggle), the navigation pane (a toggle)
//! ```
//!
//! The commands come from the ERP's view JSON: a button is a view's action (its icon, its label, its
//! `condition`), so a section the views add shows its commands without a line here. A phone shows the
//! tabs as icon squares and the buttons by their icons (a button shows EITHER its words OR its icon),
//! the band scrolls sideways where it does not fit and its ONE chevron hides it.

use azul::{
    callbacks::{ButtonOnClickCallbackType, RibbonOnTabClickCallbackType},
    prelude::*,
    widgets::{
        Ribbon, RibbonButton, RibbonButtonFace, RibbonGroup, RibbonItem, RibbonTab, SidebarState,
        TabLabels, TabStyle,
    },
};
use azul_appkit::l10n::{label, t};

use super::{frame, location, with_erp, Erp};
use crate::{
    ids,
    views::{
        rows::ViewRecord,
        spec::{self, ActionKind, ActionSpec},
        Params, ViewKind,
    },
};

/// What a ribbon button runs.
#[derive(Clone, Debug)]
pub(crate) enum Cmd {
    /// A view's action, on the record its parameters name.
    Run(ActionKind, Params),
    /// The Preview shows or hides.
    TogglePreview,
    /// The navigation pane opens or folds to its rail.
    ToggleSidebar,
    /// Nothing: the button of a page with nothing to run, greyed.
    None,
}

/// The click data of a ribbon button: the app and the command.
struct CmdRef {
    app: RefAny,
    cmd: Cmd,
}

/// A button that runs `cmd` on the app.
fn button(app: &RefAny, icon: &str, words: &str, cmd: Cmd) -> RibbonButton {
    RibbonButton::create(icon, label(words)).with_on_click(
        RefAny::new(CmdRef {
            app: app.clone(),
            cmd,
        }),
        on_cmd as ButtonOnClickCallbackType,
    )
}

/// The button of a view's action, run on the record `params` name; greyed with `why` when it
/// cannot run.
fn action_button(
    app: &RefAny,
    action: &ActionSpec,
    params: &Params,
    why: Option<String>,
) -> RibbonItem {
    let icon = if action.icon.is_empty() || action.icon == "ViewIcon" {
        "touch_app"
    } else {
        action.icon.as_str()
    };
    let mut b = button(
        app,
        icon,
        &action.label,
        Cmd::Run(action.kind.clone(), params.clone()),
    );
    if let Some(why) = why {
        b = b.with_disabled(why);
    }
    RibbonItem::LargeButton(b)
}

/// A button's click: its command on the app.
extern "C" fn on_cmd(mut data: RefAny, mut info: CallbackInfo) -> Update {
    let Some((mut app, cmd)) = data
        .downcast_ref::<CmdRef>()
        .map(|r| (r.app.clone(), r.cmd.clone()))
    else {
        return Update::DoNothing;
    };
    with_erp(&mut app, &mut info, |s, _| match cmd {
        Cmd::Run(kind, params) => super::run_action(s, &kind, &params),
        Cmd::TogglePreview => s.preview_open = !s.preview_open,
        Cmd::ToggleSidebar => {
            s.sidebar = if s.sidebar == SidebarState::Expanded {
                SidebarState::Rail
            } else {
                SidebarState::Expanded
            };
        }
        Cmd::None => {}
    })
}

/// A tab clicked: shown.
extern "C" fn on_tab(mut data: RefAny, mut info: CallbackInfo, index: usize) -> Update {
    with_erp(&mut data, &mut info, |s, _| s.ribbon_tab = index)
}

// ==== What a page offers ====

/// The commands of the record the page shows or the selection is on, as `(action, the parameters
/// it runs with)`: Open (the list's row action), then, where that leads to a record's page, the
/// header actions of that page that hold for the record. Empty with no record.
#[must_use]
pub(crate) fn record_commands(s: &Erp) -> Vec<(ActionSpec, Params)> {
    let labels = &s.state.labels;
    // on a record's own page: its header actions
    if let Some(route) = s.state.route() {
        if route.view.kind == ViewKind::Detail && s.state.form.is_none() {
            return header_commands(s, route.view, &route.params);
        }
    }
    let Some(selected) = s.state.selected.as_ref() else {
        return Vec::new();
    };
    let Some(view) = s.state.route().map(|r| r.view.clone()) else {
        return Vec::new();
    };
    let Some(path) = s.state.row_path(&view, selected.kind, &selected.id) else {
        return Vec::new();
    };
    let row_action = spec::actions(&view.row_actions, labels).into_iter().next();
    let open = ActionSpec {
        label: row_action.map_or_else(|| t("azerp-open"), |a| a.label),
        kind: ActionKind::Link(path.clone()),
        icon: String::from("open_in_full"),
        primary: false,
        condition: None,
    };
    let mut commands = vec![(open, Params::new())];
    // where the row leads to a record's page, what that page offers for the record
    if let Some(target) = s.state.views.route(&path) {
        if target.view.kind == ViewKind::Detail {
            commands.extend(header_commands(s, target.view, &target.params));
        }
    }
    commands
}

/// The header actions of a detail view that hold for the record `params` name.
fn header_commands(
    s: &Erp,
    view: &crate::views::View,
    params: &Params,
) -> Vec<(ActionSpec, Params)> {
    let Some(header) = &view.header else {
        return Vec::new();
    };
    let id = params.get("id").cloned().unwrap_or_default();
    let Some(asset) = s.state.book.get::<crate::model::Asset>(&id) else {
        return Vec::new();
    };
    let ctx = s.state.ctx();
    let value_of = |name: &str| asset.value(name, &ctx).form_text();
    spec::actions(&header.actions, &s.state.labels)
        .into_iter()
        .filter(|a| {
            a.condition
                .as_deref()
                .is_none_or(|c| spec::eval_condition(c, &value_of))
        })
        .map(|a| (a, params.clone()))
        .collect()
}

/// The page's own commands: the view's actions (the list's New / Import / Export, the report's
/// Export, a form's Save and Cancel).
fn page_commands(s: &Erp) -> Vec<(ActionSpec, Params)> {
    let view = match &s.state.form {
        Some(draft) => s.state.views.view(&draft.view),
        None => s.state.route().map(|r| r.view),
    };
    let params = s
        .state
        .form
        .as_ref()
        .map(|d| d.params.clone())
        .unwrap_or_default();
    view.map(|v| spec::actions(&v.actions, &s.state.labels))
        .unwrap_or_default()
        .into_iter()
        .map(|a| (a, params.clone()))
        .collect()
}

/// The path of the first view of `kind` (the reports, the depreciation run).
fn path_of(s: &Erp, kind: ViewKind) -> Option<String> {
    s.state
        .views
        .views
        .values()
        .find(|v| v.kind == kind)
        .and_then(|v| v.path.all().first().map(|p| (*p).to_string()))
}

// ==== The tabs ====

fn home(s: &Erp, app: &RefAny) -> RibbonTab {
    let mut tab = RibbonTab::create(label("azerp-tab-home")).with_icon("home");
    let page = page_commands(s);
    if !page.is_empty() {
        let title = if s.state.form.is_some() {
            "azerp-group-form"
        } else {
            "azerp-group-list"
        };
        let mut group = RibbonGroup::create(label(title));
        for (action, params) in &page {
            group = group.with_item(action_button(app, action, params, None));
        }
        tab = tab.with_group(group);
    }
    let record = record_commands(s);
    if !record.is_empty() {
        let mut group = RibbonGroup::create(label("azerp-group-record"));
        for (action, params) in &record {
            group = group.with_item(action_button(app, action, params, None));
        }
        tab = tab.with_group(group);
    }
    if page.is_empty() && record.is_empty() {
        // a page with nothing to run still has a band: what it says is that nothing is selected
        let none =
            RibbonGroup::create(label("azerp-group-record")).with_item(RibbonItem::LargeButton(
                button(app, "touch_app", "azerp-select-first", Cmd::None)
                    .with_disabled(t("azerp-select-first-why")),
            ));
        tab = tab.with_group(none);
    }
    tab
}

fn reports(s: &Erp, app: &RefAny) -> RibbonTab {
    let selected_asset = s.state.selected_asset().map(str::to_string);
    let mut report = RibbonGroup::create(label("azerp-group-report"));
    if let Some(path) = path_of(s, ViewKind::Report) {
        let on = s
            .state
            .route()
            .is_some_and(|r| r.view.kind == ViewKind::Report);
        report = report.with_item(RibbonItem::LargeButton(
            button(
                app,
                "bar_chart",
                "azerp-cmd-open-reports",
                Cmd::Run(ActionKind::Link(path), Params::new()),
            )
            .with_toggled(on),
        ));
    }
    report = report.with_item(RibbonItem::LargeButton(button(
        app,
        "download",
        "azerp-cmd-export-register",
        Cmd::Run(ActionKind::Named(String::from("export_csv")), Params::new()),
    )));
    let mut run = RibbonGroup::create(label("azerp-group-depreciation"));
    if let Some(path) = path_of(s, ViewKind::Wizard) {
        // the wizard's path has no parameter: the run is the register's, not an asset's
        run = run.with_item(RibbonItem::LargeButton(button(
            app,
            "calculate",
            "azerp-cmd-run",
            Cmd::Run(ActionKind::Link(path), Params::new()),
        )));
    }
    let mut params = Params::new();
    let schedule = match &selected_asset {
        Some(id) => {
            params.insert(String::from("id"), id.clone());
            button(
                app,
                "download",
                "azerp-cmd-export-schedule",
                Cmd::Run(ActionKind::Named(String::from("export_schedule")), params),
            )
        }
        None => button(
            app,
            "download",
            "azerp-cmd-export-schedule",
            Cmd::Run(ActionKind::Named(String::from("export_schedule")), params),
        )
        .with_disabled(t("azerp-select-asset-why")),
    };
    run = run.with_item(RibbonItem::LargeButton(schedule));
    RibbonTab::create(label("azerp-tab-reports"))
        .with_icon("bar_chart")
        .with_group(report)
        .with_group(run)
}

fn view_tab(s: &Erp, app: &RefAny) -> RibbonTab {
    RibbonTab::create(label("azerp-tab-view"))
        .with_icon("visibility")
        .with_group(
            RibbonGroup::create(label("azerp-group-panes"))
                .with_item(RibbonItem::LargeButton(
                    button(app, "view_sidebar", "azerp-cmd-preview", Cmd::TogglePreview)
                        .with_toggled(s.preview_open),
                ))
                .with_item(RibbonItem::LargeButton(
                    button(
                        app,
                        "menu_open",
                        "azerp-cmd-navigation-pane",
                        Cmd::ToggleSidebar,
                    )
                    .with_toggled(s.sidebar == SidebarState::Expanded),
                )),
        )
}

/// The ribbon: its tab row is the window's title bar (`strip`: the room the OS's window controls
/// take).
pub(crate) fn ribbon(s: &Erp, app: &RefAny, strip: &frame::Strip) -> Dom {
    let file = RibbonTab::create(label("azerp-tab-file"))
        .with_icon("folder")
        .with_location(location::location(s, app));
    let phone = strip.phone;
    Ribbon::create(vec![file, home(s, app), reports(s, app), view_tab(s, app)])
        .with_active_tab(s.ribbon_tab)
        .with_on_tab_click(app.clone(), on_tab as RibbonOnTabClickCallbackType)
        // No title row over the ribbon: its tabs are the title bar.
        .with_tabs_in_titlebar(frame::strip_offsets(strip))
        // The tabs are buttons on the leather: a finger's size and ONE ICON each on a phone, text
        // on a desktop.
        .with_tab_style(if phone { TabStyle::TouchPills } else { TabStyle::Pills })
        .with_labels(TabLabels::Auto)
        // a button shows its words OR its icon, never both (the label is the name and the tooltip
        // of an icon): words on a desktop, icons on a phone
        .with_button_face(if phone { RibbonButtonFace::Icon } else { RibbonButtonFace::Text })
        // the phone's 18 px rail stands beside the band, under the tab row: the band keeps clear
        .with_content_inset(strip.content_inset)
        // the window's ONE chevron at the left end of a phone's tab row folds the band away
        .with_content_toggle(phone)
        .with_available_width(strip.width)
        .dom_desktop()
        .with_id(ids::RIBBON)
}

#[cfg(test)]
#[path = "ribbon_test.rs"]
mod ribbon_test;
