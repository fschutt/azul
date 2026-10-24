//! The installed apps: the window of every run after the first. A table of every app with its
//! installed version, the version available to this computer (a release the rollout has not
//! brought here yet is not shown), its channel, its size and when it was last updated; per app
//! Update, Revert (to the one previous version kept) and Uninstall; above them Check for updates
//! and Update All. A copy someone else installed (Homebrew, apt, Flatpak, Snap, the Store) shows
//! "Update through ..." with the command to copy instead of an Update button: the manager still
//! checks it, it never touches it.

use azul::{
    callbacks::ButtonOnClickCallbackType,
    dom::ClipboardContent,
    file::DiskSpace,
    option::OptionString,
    prelude::*,
    str::String as AzString,
    vec::StyledTextRunVec,
    widgets::{Button, ButtonType},
};
use azul_appkit::{
    l10n::{date_text, t, t_args, Arg, DateStyle},
    pieces,
};

use crate::{
    jobs::{self, Job},
    model::{self, AppEntry, Installed, Owner, Status},
    Manager,
};

/// A row's button: which app it acts on.
struct RowAction {
    /// The manager's data.
    app: RefAny,
    key: &'static str,
}

fn s(text: &str) -> AzString {
    AzString::from(text)
}

/// The width of each column after the name, in order: installed, available, channel, size,
/// last updated, the actions.
const COLUMNS: [&str; 6] = ["90px", "150px", "70px", "80px", "130px", "250px"];

fn cell(width: &str, child: Dom) -> Dom {
    pieces::block(
        &format!("width: {width}; flex-shrink: 0; padding: 0px 6px; overflow: hidden;"),
        child,
    )
}

fn small(text: String) -> Dom {
    pieces::block("font-size: 11px; opacity: 0.7;", pieces::text(text))
}

fn header() -> Dom {
    let titles = [
        "azinstallmgr-column-installed",
        "azinstallmgr-column-available",
        "azinstallmgr-column-channel",
        "azinstallmgr-column-size",
        "azinstallmgr-column-updated",
        "azinstallmgr-column-actions",
    ];
    let mut children = vec![pieces::block(
        "flex-grow: 1; padding: 0px 6px;",
        pieces::text(t("azinstallmgr-column-app")),
    )];
    children.extend(
        titles
            .iter()
            .zip(COLUMNS)
            .map(|(key, width)| cell(width, pieces::text(t(key)))),
    );
    pieces::flex_row(
        "font-weight: bold; padding: 6px 0px; border-bottom: 1px solid rgba(127, 127, 127, 0.4);",
        children,
    )
}

/// The Available column: the newer version, or where the check stands.
fn available(app: &Installed) -> Dom {
    let (line, note) = match &app.status {
        Status::Unchecked => ("-".to_string(), String::new()),
        Status::Checking => (t("azinstallmgr-status-checking"), String::new()),
        Status::Working => (t("azinstallmgr-status-working"), String::new()),
        Status::UpToDate => (t("azinstallmgr-status-up-to-date"), String::new()),
        Status::Available(r) => (r.version.clone(), t("azinstallmgr-status-available")),
        Status::Staged(r) => (r.version.clone(), t("azinstallmgr-status-staged")),
        Status::Skipped(v) => (v.clone(), t("azinstallmgr-status-skipped")),
        Status::Failed(_) => (t("azinstallmgr-status-failed"), String::new()),
    };
    let mut children = vec![pieces::text(line)];
    if !note.is_empty() {
        children.push(small(note));
    }
    pieces::column("", children)
}

fn button(label: String, key: &'static str, id: String, app: &RefAny, busy: bool, cb: ButtonOnClickCallbackType) -> Dom {
    let mut b = Button::create(s(&label)).with_on_click(
        RefAny::new(RowAction {
            app: app.clone(),
            key,
        }),
        cb,
    );
    if busy {
        b = b.with_disabled(s(&t("azinstallmgr-busy")));
    }
    b.dom().with_id(s(&id))
}

/// The actions of a row: Update / Revert / Uninstall for the manager's own copies, "Update
/// through ..." with Copy for someone else's.
fn actions(m: &Manager, a: &Installed, app: &RefAny) -> Dom {
    let key = a.entry.key;
    let busy = m.busy || matches!(a.status, Status::Working | Status::Checking);
    let mut children = Vec::new();
    match &a.owner {
        Owner::Manager => {
            if matches!(a.status, Status::Available(_) | Status::Staged(_) | Status::Skipped(_)) {
                let mut b = Button::create(s(&t("azinstallmgr-update")))
                    .with_button_type(ButtonType::Primary)
                    .with_on_click(
                        RefAny::new(RowAction {
                            app: app.clone(),
                            key,
                        }),
                        on_update,
                    );
                if busy {
                    b = b.with_disabled(s(&t("azinstallmgr-busy")));
                }
                children.push(b.dom().with_id(s(&format!("installmgr-update-{key}"))));
            }
            if let Some(previous) = &a.previous {
                children.push(button(
                    t_args("azinstallmgr-revert-to", &[("version", Arg::from(previous.as_str()))]),
                    key,
                    format!("installmgr-revert-{key}"),
                    app,
                    busy,
                    on_revert,
                ));
            }
            if !a.entry.is_runtime() {
                children.push(button(
                    t("azinstallmgr-uninstall"),
                    key,
                    format!("installmgr-uninstall-{key}"),
                    app,
                    busy,
                    on_ask_uninstall,
                ));
            }
        }
        owner => {
            let line = match owner {
                Owner::Elsewhere => t("azinstallmgr-owner-elsewhere"),
                Owner::ReadOnly => t("azinstallmgr-owner-readonly"),
                other => t_args(
                    "azinstallmgr-update-through",
                    &[("manager", Arg::from(other.name()))],
                ),
            };
            children.push(small(line));
            if !owner.update_command(a.entry).is_empty() {
                children.push(button(
                    t("azinstallmgr-copy"),
                    key,
                    format!("installmgr-copy-{key}"),
                    app,
                    false,
                    on_copy_update_command,
                ));
            }
        }
    }
    pieces::flex_row("gap: 4px; flex-wrap: wrap;", children)
}

fn row(m: &Manager, a: &Installed, app: &RefAny) -> Dom {
    let name = if a.entry.is_runtime() {
        t("azinstallmgr-component-runtime")
    } else {
        a.entry.name.to_string()
    };
    let r = &a.registration;
    let (year, month, day, weekday) = model::civil_date(r.updated_unix);
    let updated = if r.updated_unix == 0 {
        "-".to_string()
    } else {
        date_text(DateStyle::Date, year, month, day, weekday)
    };
    let channel = t(match a.channel {
        azul_appkit::azlin_config::Channel::Stable => "azinstallmgr-channel-stable",
        azul_appkit::azlin_config::Channel::Beta => "azinstallmgr-channel-beta",
    });
    let size = if r.size_bytes == 0 {
        "-".to_string()
    } else {
        DiskSpace::format_bytes(r.size_bytes).as_str().to_string()
    };
    let name_cell = pieces::flex_row(
        "flex-grow: 1; gap: 8px; padding: 0px 6px; min-width: 0px;",
        vec![
            Dom::create_icon(s(a.entry.icon)),
            pieces::column("", vec![pieces::text(name), small(t(a.entry.summary_key))]),
        ],
    );
    pieces::flex_row(
        "padding: 6px 0px; border-bottom: 1px solid rgba(127, 127, 127, 0.18);",
        vec![
            name_cell,
            cell(COLUMNS[0], pieces::text(r.version.clone())),
            cell(COLUMNS[1], available(a)),
            cell(COLUMNS[2], pieces::text(channel)),
            cell(COLUMNS[3], pieces::text(size)),
            cell(COLUMNS[4], pieces::text(updated)),
            cell(COLUMNS[5], actions(m, a, app)),
        ],
    )
    .with_id(s(&format!("installmgr-row-{}", a.entry.key)))
}

/// The apps that have something to update to (the manager's own copies).
fn updatable(m: &Manager) -> Vec<&Installed> {
    m.apps
        .iter()
        .filter(|a| a.is_managed() && matches!(a.status, Status::Available(_) | Status::Staged(_)))
        .collect()
}

/// The installed apps' screen.
pub(crate) fn screen(m: &Manager, app: &RefAny) -> Dom {
    let pending = updatable(m).len();
    let mut check = Button::create(s(&t("azinstallmgr-check")))
        .with_icon(s("refresh"))
        .with_on_click(app.clone(), on_check);
    let mut update_all = Button::create(s(&t_args(
        "azinstallmgr-update-all",
        &[("count", Arg::from(pending))],
    )))
    .with_button_type(ButtonType::Primary)
    .with_icon(s("system_update_alt"))
    .with_on_click(app.clone(), on_update_all);
    if m.busy {
        check = check.with_disabled(s(&t("azinstallmgr-busy")));
    } else if !model::has_source(&m.updates) {
        check = check.with_disabled(s(&t("azinstallmgr-source-unset-reason")));
    }
    if m.busy || pending == 0 {
        update_all = update_all.with_disabled(s(&t("azinstallmgr-nothing-to-update")));
    }
    let toolbar = pieces::flex_row(
        "gap: 8px; padding: 10px 12px;",
        vec![
            check.dom().with_id(s("installmgr-check")),
            update_all.dom().with_id(s("installmgr-update-all")),
            Button::create(s(&t("azinstallmgr-settings")))
                .with_icon(s("settings"))
                .with_on_click(app.clone(), on_settings)
                .dom()
                .with_id(s("installmgr-settings-open")),
            pieces::block("flex-grow: 1;", Dom::create_div()),
            small(t_args(
                "azinstallmgr-cohort",
                &[("cohort", Arg::from(u64::from(m.cohort)))],
            )),
        ],
    );
    let mut rows = vec![header()];
    if m.apps.is_empty() {
        rows.push(pieces::block(
            "padding: 16px 6px;",
            pieces::text(t("azinstallmgr-no-apps")),
        ));
    }
    rows.extend(m.apps.iter().map(|a| row(m, a, app)));
    let table = pieces::column(
        "flex-grow: 1; min-height: 0px; overflow-y: auto; padding: 0px 12px;",
        rows,
    )
    .with_id(s("installmgr-table"));
    let mut children = vec![toolbar];
    if !model::has_source(&m.updates) {
        // Without an update source nothing can be checked or updated: say so above the table.
        children.push(
            pieces::block("padding: 8px 12px;", pieces::text(t("azinstallmgr-source-none")))
                .with_id(s("installmgr-no-source")),
        );
    }
    children.push(table);
    if !m.notice.is_empty() {
        children.push(
            pieces::block("padding: 8px 12px;", pieces::text(m.notice.clone()))
                .with_id(s("installmgr-notice")),
        );
    }
    pieces::column("flex-grow: 1; min-height: 0px;", children)
}

// ==== Callbacks ====

/// Starts checking every installed app (F5, Check for updates, the periodic check).
pub(crate) fn check_all(m: &mut Manager, info: &mut CallbackInfo, app: &RefAny) {
    if m.busy || m.apps.is_empty() {
        return;
    }
    m.busy = true;
    m.checking = true;
    m.notice.clear();
    m.last_check_unix = azul_appkit::update::unix_now();
    for a in &mut m.apps {
        a.status = Status::Checking;
    }
    let apps = m.apps.clone();
    jobs::spawn(info, app, Job::Check { apps }, m.fetching());
}

extern "C" fn on_check(mut data: RefAny, mut info: CallbackInfo) -> Update {
    let app = data.clone();
    let Some(mut m) = data.downcast_mut::<Manager>() else {
        return Update::DoNothing;
    };
    check_all(&mut *m, &mut info, &app);
    Update::RefreshDom
}

extern "C" fn on_settings(mut data: RefAny, _info: CallbackInfo) -> Update {
    let Some(mut m) = data.downcast_mut::<Manager>() else {
        return Update::DoNothing;
    };
    crate::settings::open(&mut m);
    Update::RefreshDom
}

extern "C" fn on_update_all(mut data: RefAny, mut info: CallbackInfo) -> Update {
    let app = data.clone();
    let Some(mut m) = data.downcast_mut::<Manager>() else {
        return Update::DoNothing;
    };
    if m.busy {
        return Update::DoNothing;
    }
    let apps: Vec<(Installed, Option<azul_appkit::update::Release>)> = updatable(&m)
        .into_iter()
        .map(|a| {
            let release = match &a.status {
                Status::Available(r) | Status::Staged(r) => Some(r.clone()),
                _ => None,
            };
            (a.clone(), release)
        })
        .collect();
    if apps.is_empty() {
        return Update::DoNothing;
    }
    m.busy = true;
    m.notice.clear();
    let fetching = m.fetching();
    jobs::spawn(&mut info, &app, Job::Update { apps }, fetching);
    Update::RefreshDom
}

/// The manager's data and the app of a row's button.
fn row_target(data: &mut RefAny) -> Option<(RefAny, &'static AppEntry)> {
    let action = data.downcast_ref::<RowAction>()?;
    Some((action.app.clone(), model::entry(action.key)?))
}

extern "C" fn on_update(mut data: RefAny, mut info: CallbackInfo) -> Update {
    let Some((mut app, entry)) = row_target(&mut data) else {
        return Update::DoNothing;
    };
    let app_ref = app.clone();
    let Some(mut m) = app.downcast_mut::<Manager>() else {
        return Update::DoNothing;
    };
    let Some(a) = m.apps.iter().find(|a| a.entry.key == entry.key).cloned() else {
        return Update::DoNothing;
    };
    if m.busy || !a.is_managed() {
        return Update::DoNothing;
    }
    let release = match &a.status {
        Status::Available(r) | Status::Staged(r) => Some(r.clone()),
        _ => None,
    };
    m.busy = true;
    m.notice.clear();
    let fetching = m.fetching();
    jobs::spawn(&mut info, &app_ref, Job::Update { apps: vec![(a, release)] }, fetching);
    Update::RefreshDom
}

extern "C" fn on_revert(mut data: RefAny, mut info: CallbackInfo) -> Update {
    let Some((mut app, entry)) = row_target(&mut data) else {
        return Update::DoNothing;
    };
    let app_ref = app.clone();
    let Some(mut m) = app.downcast_mut::<Manager>() else {
        return Update::DoNothing;
    };
    if m.busy {
        return Update::DoNothing;
    }
    m.busy = true;
    m.notice.clear();
    let fetching = m.fetching();
    jobs::spawn(&mut info, &app_ref, Job::Revert { entry }, fetching);
    Update::RefreshDom
}

extern "C" fn on_ask_uninstall(mut data: RefAny, _info: CallbackInfo) -> Update {
    let Some((mut app, entry)) = row_target(&mut data) else {
        return Update::DoNothing;
    };
    let Some(mut m) = app.downcast_mut::<Manager>() else {
        return Update::DoNothing;
    };
    if m.busy {
        return Update::DoNothing;
    }
    m.confirm_uninstall = Some(entry);
    println!("AZINSTALLMGR_ASK_UNINSTALL {}", entry.key);
    Update::RefreshDom
}

/// The answer to "Uninstall <app>?": Yes removes it.
pub(crate) fn uninstall_answered(m: &mut Manager, info: &mut CallbackInfo, app: &RefAny, yes: bool) {
    let Some(entry) = m.confirm_uninstall.take() else {
        return;
    };
    if !yes || m.busy {
        return;
    }
    m.busy = true;
    m.notice.clear();
    let fetching = m.fetching();
    jobs::spawn(info, app, Job::Uninstall { entry }, fetching);
}

extern "C" fn on_copy_update_command(mut data: RefAny, mut info: CallbackInfo) -> Update {
    let Some((mut app, entry)) = row_target(&mut data) else {
        return Update::DoNothing;
    };
    let Some(mut m) = app.downcast_mut::<Manager>() else {
        return Update::DoNothing;
    };
    let Some(a) = m.apps.iter().find(|a| a.entry.key == entry.key) else {
        return Update::DoNothing;
    };
    let command = a.owner.update_command(entry);
    info.set_clipboard_content(ClipboardContent {
        plain_text: s(&command),
        styled_runs: StyledTextRunVec::create(),
        html: OptionString::None,
        files: azul::vec::StringVec::create(),
        image: azul::option::OptionU8Vec::None,
        link: azul::option::OptionString::None,
        concealed: false,
    });
    m.notice = t_args("azinstallmgr-copied-command", &[("command", Arg::from(command.as_str()))]);
    println!("AZINSTALLMGR_COPIED {command}");
    Update::RefreshDom
}

// ==== The worker's steps ====

fn find<'a>(m: &'a mut Manager, key: &str) -> Option<&'a mut Installed> {
    m.apps.iter_mut().find(|a| a.entry.key == key)
}

/// Work on an app began.
pub(crate) fn started(m: &mut Manager, key: &str) {
    let checking = m.checking;
    if let Some(a) = find(m, key) {
        a.status = if checking {
            Status::Checking
        } else {
            Status::Working
        };
    }
}

/// An app was checked.
pub(crate) fn checked(m: &mut Manager, key: &str, status: Status) {
    if let Some(a) = find(m, key) {
        a.status = status;
        println!("{}", model::row_line(a));
    }
}

/// An app was updated (by the table, or by its own "Update now").
pub(crate) fn updated(m: &mut Manager, key: &str, result: Result<azul_appkit::update::Registration, String>) {
    let name = model::entry(key).map_or(key, |e| e.name);
    match result {
        Ok(registration) => {
            println!("AZINSTALLMGR_UPDATED {key} {}", registration.version);
            m.notice = t_args(
                "azinstallmgr-updated",
                &[("app", Arg::from(name)), ("version", Arg::from(registration.version.as_str()))],
            );
            if let Some(a) = find(m, key) {
                a.previous = Some(a.registration.version.clone());
                a.registration = registration;
                a.status = Status::UpToDate;
            }
        }
        Err(why) => {
            println!("AZINSTALLMGR_UPDATE_FAILED {key} {why}");
            m.notice = t_args(
                "azinstallmgr-update-failed",
                &[("app", Arg::from(name)), ("reason", Arg::from(why.as_str()))],
            );
            if let Some(a) = find(m, key) {
                a.status = Status::Failed(why);
            }
        }
    }
}

/// An app was rolled back.
pub(crate) fn reverted(
    m: &mut Manager,
    key: &str,
    result: Result<(String, azul_appkit::update::Registration), String>,
) {
    let name = model::entry(key).map_or(key, |e| e.name);
    match result {
        Ok((from, registration)) => {
            println!("AZINSTALLMGR_REVERTED {key} {}", registration.version);
            m.notice = t_args(
                "azinstallmgr-reverted",
                &[
                    ("app", Arg::from(name)),
                    ("version", Arg::from(registration.version.as_str())),
                    ("skipped", Arg::from(from.as_str())),
                ],
            );
            // The policy now skips the version left: read it again.
            m.updates = azul_appkit::azlin_config::AzlinConfig::load(&m.places.config).0.updates;
            if let Some(a) = find(m, key) {
                a.previous = None;
                a.registration = registration;
                a.status = if from.is_empty() {
                    Status::Unchecked
                } else {
                    Status::Skipped(from)
                };
            }
        }
        Err(why) => {
            println!("AZINSTALLMGR_REVERT_FAILED {key} {why}");
            m.notice = why.clone();
            if let Some(a) = find(m, key) {
                a.status = Status::Failed(why);
            }
        }
    }
}

/// An app was removed.
pub(crate) fn uninstalled(m: &mut Manager, key: &str, result: Result<(), String>) {
    let name = model::entry(key).map_or(key, |e| e.name);
    match result {
        Ok(()) => {
            println!("AZINSTALLMGR_UNINSTALLED {key}");
            m.apps.retain(|a| a.entry.key != key);
            m.notice = t_args("azinstallmgr-uninstalled", &[("app", Arg::from(name))]);
        }
        Err(why) => {
            println!("AZINSTALLMGR_UNINSTALL_FAILED {key} {why}");
            m.notice = why.clone();
            if let Some(a) = find(m, key) {
                a.status = Status::Failed(why);
            }
        }
    }
}

/// A check of every app is done: what it found, in one line.
pub(crate) fn check_finished(m: &mut Manager) {
    m.checking = false;
    let found = updatable(m).len();
    let failed: Vec<String> = m
        .apps
        .iter()
        .filter_map(|a| match &a.status {
            Status::Failed(why) => Some(why.clone()),
            _ => None,
        })
        .collect();
    println!("AZINSTALLMGR_CHECKED available={found} failed={}", failed.len());
    m.notice = match (found, failed.first()) {
        (_, Some(why)) => t_args("azinstallmgr-check-failed", &[("reason", Arg::from(why.as_str()))]),
        (0, None) => t("azinstallmgr-all-up-to-date"),
        (n, None) => t_args("azinstallmgr-updates-found", &[("count", Arg::from(n))]),
    };
}
