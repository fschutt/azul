//! The window's chrome around the three panes: the title row, the ribbon (HOME, VIEW and
//! FILE), the status bar with the save state, the To-Do bar, the command palette and the
//! confirmation bar; the FILE pages are in `backstage.rs`.
//!
//! The ribbon, the palette, the keyboard and the shortcuts page are views of ONE table of
//! commands ([`Command`]), run by [`run`].

use azul::{
    callbacks::{
        ButtonOnClickCallbackType, DropDownOnChoiceChangeCallbackType, RibbonOnTabClickCallbackType,
        ShellCommandPaletteOnQueryCallbackType, ShellCommandPaletteOnRunCallbackType,
        ToDoBarOnEventCallbackType,
    },
    css::DarkLightMode,
    dom::{DomNodeId, FocusTarget},
    option::OptionDarkLightMode,
    prelude::*,
    shells::{ShellCommandPalette, ShellPaletteCommand},
    str::String as AzString,
    vec::StringVec,
    widgets::{
        AlertKind, ButtonType, DropDown, InfoBar, Ribbon, RibbonAppButton, RibbonGroup,
        RibbonItem, RibbonTab, StatusBar, StatusBarSegment,
        StatusBarSync, StatusBarSyncKind, Titlebar, ToDoBar, ToDoBarEvent, ToDoBarEventKind,
        ToDoTask,
    },
};
use azul_appkit::args::Theme;
use azul_appkit::ribbon::{self as ribbon_kit, column, RibbonCommand};
use chrono::{Datelike, NaiveDate, NaiveDateTime};

use crate::{
    ids,
    model::{self, SortMode},
    reminders,
    state::{self, Confirm, Page, Tasks},
    views::{self, Smart, View},
};

// ==== The command table ====

/// Everything the app can be told to do.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Command {
    NewTask,
    NewList,
    Complete,
    Flag,
    Delete,
    MoveUp,
    MoveDown,
    Show(Smart),
    Search,
    Settings,
    Shortcuts,
    About,
    ToggleTodoBar,
    ToggleNavigation,
    ToggleCompleted,
    Sort(SortMode),
    ThemeFlat,
    ThemeFlora,
    ModeLight,
    ModeDark,
    ModeSystem,
    Palette,
}

impl Command {
    /// The palette's table, in its order.
    pub const ALL: [Command; 30] = [
        Command::NewTask,
        Command::NewList,
        Command::Complete,
        Command::Flag,
        Command::Delete,
        Command::MoveUp,
        Command::MoveDown,
        Command::Show(Smart::Today),
        Command::Show(Smart::Upcoming),
        Command::Show(Smart::Scheduled),
        Command::Show(Smart::Flagged),
        Command::Show(Smart::All),
        Command::Show(Smart::Completed),
        Command::Search,
        Command::Settings,
        Command::Shortcuts,
        Command::About,
        Command::ToggleTodoBar,
        Command::ToggleNavigation,
        Command::ToggleCompleted,
        Command::Sort(SortMode::Manual),
        Command::Sort(SortMode::Due),
        Command::Sort(SortMode::Priority),
        Command::Sort(SortMode::Title),
        Command::Sort(SortMode::Created),
        Command::ThemeFlat,
        Command::ThemeFlora,
        Command::ModeLight,
        Command::ModeDark,
        Command::ModeSystem,
    ];

    #[must_use]
    pub fn label(self) -> String {
        match self {
            Command::NewTask => "New task".into(),
            Command::NewList => "New list".into(),
            Command::Complete => "Complete".into(),
            Command::Flag => "Flag".into(),
            Command::Delete => "Delete".into(),
            Command::MoveUp => "Move up".into(),
            Command::MoveDown => "Move down".into(),
            Command::Show(s) => format!("Show {}", s.label()),
            Command::Search => "Search".into(),
            Command::Settings => "Settings".into(),
            Command::Shortcuts => "Keyboard shortcuts".into(),
            Command::About => "About AzTasks".into(),
            Command::ToggleTodoBar => "To-Do bar".into(),
            Command::ToggleNavigation => "Navigation pane".into(),
            Command::ToggleCompleted => "Completed tasks in lists".into(),
            Command::Sort(m) => format!("Sort by {}", m.label().to_lowercase()),
            Command::ThemeFlat => "Flat theme".into(),
            Command::ThemeFlora => "Flora theme".into(),
            Command::ModeLight => "Light mode".into(),
            Command::ModeDark => "Dark mode".into(),
            Command::ModeSystem => "Mode of the system".into(),
            Command::Palette => "Command palette".into(),
        }
    }

    #[must_use]
    pub fn icon(self) -> &'static str {
        match self {
            Command::NewTask => "add_task",
            Command::NewList => "playlist_add",
            Command::Complete => "task_alt",
            Command::Flag => "flag",
            Command::Delete => "delete",
            Command::MoveUp => "arrow_upward",
            Command::MoveDown => "arrow_downward",
            Command::Show(s) => s.icon(),
            Command::Search => "search",
            Command::Settings => "settings",
            Command::Shortcuts => "keyboard",
            Command::About => "info",
            Command::ToggleTodoBar => "view_sidebar",
            Command::ToggleNavigation => "menu_open",
            Command::ToggleCompleted => "checklist",
            Command::Sort(_) => "sort",
            Command::ThemeFlat | Command::ThemeFlora => "palette",
            Command::ModeLight => "light_mode",
            Command::ModeDark => "dark_mode",
            Command::ModeSystem => "contrast",
            Command::Palette => "terminal",
        }
    }

    /// The keys, as the palette and the shortcuts page show them ("Cmd" is Ctrl off macOS).
    #[must_use]
    pub fn shortcut(self) -> &'static str {
        match self {
            Command::NewTask => "N / Cmd+N",
            Command::Complete => "Space",
            Command::Delete => "Delete",
            Command::MoveUp => "Alt+Up",
            Command::MoveDown => "Alt+Down",
            Command::Show(Smart::Today) => "Cmd+1",
            Command::Show(Smart::Upcoming) => "Cmd+2",
            Command::Show(Smart::Scheduled) => "Cmd+3",
            Command::Show(Smart::Flagged) => "Cmd+4",
            Command::Show(Smart::All) => "Cmd+5",
            Command::Show(Smart::Completed) => "Cmd+6",
            Command::Search => "Cmd+F",
            Command::Settings => "Cmd+,",
            Command::Palette => "Cmd+K",
            _ => "",
        }
    }

    #[must_use]
    pub fn category(self) -> &'static str {
        match self {
            Command::NewTask
            | Command::NewList
            | Command::Complete
            | Command::Flag
            | Command::Delete
            | Command::MoveUp
            | Command::MoveDown => "Task",
            Command::Show(_) | Command::Search => "Go",
            Command::Settings | Command::Shortcuts | Command::About | Command::Palette => "App",
            _ => "View",
        }
    }
}

/// Focuses the element with DOM id `id` (an `ids` name) in the window's DOM.
pub fn focus_id(info: &mut CallbackInfo, id: AzString) {
    let dom = info.get_hit_node().dom;
    let node = info.get_node_id_by_id_attribute(dom, id);
    info.set_focus(FocusTarget::Id(DomNodeId { dom, node }));
}

/// Runs `command` (inside `crate::with_tasks`).
pub fn run(info: &mut CallbackInfo, app: &RefAny, s: &mut Tasks, command: Command) {
    s.palette = None;
    let now = state::now();
    match command {
        Command::NewTask => {
            s.page = None;
            s.editing_list = None;
            focus_id(info, ids::QUICK_ADD);
        }
        Command::NewList => {
            let id = s.new_list("New list", "");
            s.show(View::List(id.clone()));
            s.editing_list = Some(id.clone());
            s.drafts.list = id;
            s.drafts.list_name = "New list".to_string();
            s.drafts.list_group.clear();
        }
        Command::Complete => s.toggle_selected(now),
        Command::Flag => {
            let picked = s.selected();
            let flag = !picked.iter().all(|&i| s.tasks[i].flagged);
            for i in picked {
                s.tasks[i].flagged = flag;
                s.save_task(i);
            }
        }
        Command::Delete => {
            let ids = s.selected_ids();
            let gone = s.delete_tasks(&ids);
            crate::jobs::delete_files(info, app, s, gone);
        }
        Command::MoveUp => s.step_order(true),
        Command::MoveDown => s.step_order(false),
        Command::Show(smart) => s.show(View::Smart(smart)),
        Command::Search => {
            s.page = None;
            focus_id(info, ids::SEARCH);
        }
        Command::Settings => s.page = Some(Page::Settings),
        Command::Shortcuts => s.page = Some(Page::Shortcuts),
        Command::About => s.page = Some(Page::About),
        Command::ToggleTodoBar => s.show_todo_bar = !s.show_todo_bar,
        Command::ToggleNavigation => s.nav_collapsed = !s.nav_collapsed,
        Command::ToggleCompleted => {
            s.settings.show_completed = !s.settings.show_completed;
            s.save_settings();
        }
        Command::Sort(mode) => {
            s.settings.sort = mode;
            s.save_settings();
        }
        Command::ThemeFlat => info.set_theme("flat"),
        Command::ThemeFlora => info.set_theme("flora"),
        Command::ModeLight => info.set_mode(OptionDarkLightMode::Some(DarkLightMode::Light)),
        Command::ModeDark => info.set_mode(OptionDarkLightMode::Some(DarkLightMode::Dark)),
        Command::ModeSystem => info.set_mode(OptionDarkLightMode::None),
        Command::Palette => s.palette = Some(String::new()),
    }
}

/// What a ribbon button or a palette row carries.
struct CommandRef {
    app: RefAny,
    command: Command,
}

extern "C" fn on_command(mut data: RefAny, mut info: CallbackInfo) -> Update {
    let Some((mut app, command)) = data
        .downcast_ref::<CommandRef>()
        .map(|r| (r.app.clone(), r.command))
    else {
        return Update::DoNothing;
    };
    crate::with_tasks(&mut app, &mut info, |info, app, s| run(info, app, s, command))
}

// ==== Title row and ribbon ====

/// The window's title row (the window is `NoTitle`; azul draws the controls).
pub fn title_row() -> Dom {
    Titlebar::create("AzTasks").without_border_bottom().dom()
}

/// Every ribbon button runs a [`Command`] through `on_command` (azul-appkit's ribbon
/// builder).
impl RibbonCommand for Command {
    fn click_data(self, app: &RefAny) -> RefAny {
        RefAny::new(CommandRef {
            app: app.clone(),
            command: self,
        })
    }

    fn on_click() -> ButtonOnClickCallbackType {
        on_command
    }
}

fn large(app: &RefAny, command: Command) -> RibbonItem {
    ribbon_kit::large(app, command.icon(), &command.label(), command)
}

fn small(app: &RefAny, command: Command, toggled: bool) -> RibbonItem {
    ribbon_kit::toggle(app, command.icon(), &command.label(), command, toggled)
}

/// A short label for a toggled small button.
fn labelled(app: &RefAny, command: Command, label: &str, toggled: bool) -> RibbonItem {
    ribbon_kit::toggle(app, command.icon(), label, command, toggled)
}

/// HOME (new, manage, arrange, move) and VIEW (sort, show, appearance); FILE opens the
/// backstage.
pub fn ribbon(s: &Tasks, app: &RefAny, theme: &str, dark: bool) -> Dom {
    let order = views::lists_in_nav_order(&s.lists);
    let mut names = vec![AzString::from("Move to...")];
    names.extend(order.iter().map(|&i| AzString::from(s.lists[i].name.as_str())));
    let move_to = DropDown::create(StringVec::from(names))
        .with_selected(0)
        .with_accessibility_name("Move the selected tasks to")
        .with_on_choice_change(app.clone(), on_ribbon_move as DropDownOnChoiceChangeCallbackType);
    let home = RibbonTab::create("HOME")
        .with_group(
            RibbonGroup::create("New")
                .with_item(large(app, Command::NewTask))
                .with_item(column(vec![small(app, Command::NewList, false)])),
        )
        .with_group(
            RibbonGroup::create("Manage")
                .with_item(large(app, Command::Complete))
                .with_item(column(vec![
                    small(app, Command::Flag, false),
                    small(app, Command::Delete, false),
                ])),
        )
        .with_group(RibbonGroup::create("Arrange").with_item(column(vec![
            small(app, Command::MoveUp, false),
            small(app, Command::MoveDown, false),
        ])))
        .with_group(RibbonGroup::create("Move").with_item(RibbonItem::Drop(move_to)));
    let sort = s.settings.sort;
    let flora = Theme::parse(theme).is_some_and(Theme::is_flora);
    let view = RibbonTab::create("VIEW")
        .with_group(RibbonGroup::create("Sort by").with_item(column(vec![
            labelled(app, Command::Sort(SortMode::Manual), "Manual", sort == SortMode::Manual),
            labelled(app, Command::Sort(SortMode::Due), "Due date", sort == SortMode::Due),
            labelled(app, Command::Sort(SortMode::Priority), "Priority", sort == SortMode::Priority),
        ])).with_item(column(vec![
            labelled(app, Command::Sort(SortMode::Title), "Title", sort == SortMode::Title),
            labelled(app, Command::Sort(SortMode::Created), "Created", sort == SortMode::Created),
        ])))
        .with_group(RibbonGroup::create("Show").with_item(column(vec![
            small(app, Command::ToggleCompleted, s.settings.show_completed),
            small(app, Command::ToggleTodoBar, s.show_todo_bar),
            small(app, Command::ToggleNavigation, !s.nav_collapsed),
        ])))
        .with_group(
            RibbonGroup::create("Appearance")
                .with_item(column(vec![
                    // Flora or a spin of it ("flora:green") checks Flora.
                    labelled(app, Command::ThemeFlat, "Flat", !flora),
                    labelled(app, Command::ThemeFlora, "Flora", flora),
                ]))
                .with_item(column(vec![
                    labelled(app, Command::ModeLight, "Light", !dark),
                    labelled(app, Command::ModeDark, "Dark", dark),
                    labelled(app, Command::ModeSystem, "System", false),
                ])),
        );
    let mut ribbon = Ribbon::create(vec![home, view])
        .with_active_tab(s.ribbon_tab)
        .with_app_button(RibbonAppButton::create("FILE").with_on_click(
            RefAny::new(CommandRef {
                app: app.clone(),
                command: Command::Settings,
            }),
            on_command as ButtonOnClickCallbackType,
        ));
    ribbon.set_on_tab_click(app.clone(), on_ribbon_tab as RibbonOnTabClickCallbackType);
    ribbon.dom_desktop()
}

extern "C" fn on_ribbon_tab(mut data: RefAny, _info: CallbackInfo, index: usize) -> Update {
    let Some(mut s) = data.downcast_mut::<Tasks>() else {
        return Update::DoNothing;
    };
    s.ribbon_tab = index;
    Update::RefreshDom
}

extern "C" fn on_ribbon_move(mut data: RefAny, mut info: CallbackInfo, index: usize) -> Update {
    crate::with_tasks(&mut data, &mut info, |info, app, s| {
        let order = views::lists_in_nav_order(&s.lists);
        let Some(&li) = index.checked_sub(1).and_then(|n| order.get(n)) else {
            return;
        };
        let list = s.lists[li].id.clone();
        let ids = s.selected_ids();
        let moves = s.move_tasks(&ids, &list);
        crate::jobs::move_files(info, app, s, moves);
    })
}

// ==== Status bar ====

/// "5 due today · 1 overdue", the open count, and the save state as the sync indicator
/// (a click on a failure retries).
pub fn status_bar(s: &Tasks, app: &RefAny, now: NaiveDateTime) -> Dom {
    let (due_today, overdue) = views::summary(&s.tasks, now);
    let mut first = format!("{due_today} due today");
    if overdue > 0 {
        first.push_str(&format!(" \u{b7} {overdue} overdue"));
    }
    let open = views::smart_count(Smart::All, &s.tasks, now.date());
    let failures = s.queue.failures();
    let waiting = s.queue.pending() + s.queue.in_flight();
    let sync = if !failures.is_empty() {
        StatusBarSync::create(format!("{} not saved - retry", failures.len()), StatusBarSyncKind::Error)
            .with_on_click(app.clone(), on_retry as ButtonOnClickCallbackType)
    } else if waiting > 0 || s.files.running > 0 {
        StatusBarSync::create(format!("Saving {}...", waiting.max(1)), StatusBarSyncKind::Syncing)
    } else if !s.loaded {
        StatusBarSync::create("Reading...", StatusBarSyncKind::Syncing)
    } else {
        StatusBarSync::create("Saved", StatusBarSyncKind::Connected)
    };
    StatusBar::create(vec![
        StatusBarSegment::create(first).with_icon("today"),
        StatusBarSegment::create(format!("{open} open")),
        StatusBarSegment::create(s.root.display().to_string()).with_icon("folder"),
    ])
    .with_sync(sync)
    .dom()
}

extern "C" fn on_retry(mut data: RefAny, mut info: CallbackInfo) -> Update {
    crate::with_tasks(&mut data, &mut info, |_info, _app, s| s.queue.retry())
}

// ==== To-Do bar ====

/// The tasks the To-Do bar lists for its day: due that day (and, on today, the overdue
/// ones), open first.
#[must_use]
pub fn todo_tasks(s: &Tasks, now: NaiveDateTime) -> Vec<usize> {
    let today = now.date();
    let day = s.todo_day;
    let mut idx: Vec<usize> = (0..s.tasks.len())
        .filter(|&i| {
            let t = &s.tasks[i];
            match t.due {
                Some(d) if d == day => true,
                Some(d) if day == today && d < today => !t.is_done(),
                _ => false,
            }
        })
        .collect();
    idx.sort_by_key(|&i| (s.tasks[i].is_done(), s.tasks[i].due, s.tasks[i].due_time, s.tasks[i].order));
    idx
}

/// The To-Do bar: the month with today ringed, the day's reminders, a line for a new task
/// on the day, and the day's tasks.
pub fn todo_bar(s: &Tasks, app: &RefAny, now: NaiveDateTime) -> Dom {
    let today = now.date();
    let day = s.todo_day;
    let ymd = |d: NaiveDate| (u32::try_from(d.year()).unwrap_or(1970), d.month(), d.day());
    let (y, m, d) = ymd(day);
    let (ty, tm, td) = ymd(today);
    let appointments: Vec<AzString> = s
        .tasks
        .iter()
        .filter(|t| !t.is_done())
        .filter_map(|t| {
            let at = reminders::reminder_at(t, s.settings.reminder_time)?;
            (at.date() == day).then(|| AzString::from(format!("{} {}", model::format_time(at.time()), t.title)))
        })
        .collect();
    let tasks: Vec<ToDoTask> = todo_tasks(s, now)
        .into_iter()
        .enumerate()
        .map(|(n, i)| {
            let t = &s.tasks[i];
            let due = views::due_label(t, today).unwrap_or_default();
            ToDoTask::create(u64::try_from(n).unwrap_or(0), t.title.as_str())
                .with_due(due)
                .with_done(t.is_done())
        })
        .collect();
    let line = if day == today {
        "New task for today".to_string()
    } else {
        format!("New task for {}", model::day_label(day, today))
    };
    ToDoBar::create(y, m, d)
        .with_today(ty, tm, td)
        .with_appointments(StringVec::from(appointments))
        .with_appointments_empty("No reminders on this day.")
        // The calendar's rows start on the settings' week start (WIDGETS7's ToDoBar week start).
        .with_week_start(crate::repeat_form::picker_week_start(s.settings.week_start))
        .with_task_line(line, s.drafts.todo.as_str())
        .with_tasks(tasks)
        .with_accessibility_name("To-Do bar")
        .with_on_pick(app.clone(), on_todo as ToDoBarOnEventCallbackType)
        .with_on_task(app.clone(), on_todo as ToDoBarOnEventCallbackType)
        .with_on_appointment(app.clone(), on_todo as ToDoBarOnEventCallbackType)
        .dom()
}

extern "C" fn on_todo(mut data: RefAny, mut info: CallbackInfo, event: ToDoBarEvent) -> Update {
    crate::with_tasks(&mut data, &mut info, |info, _app, s| {
        let now = state::now();
        match event.kind {
            ToDoBarEventKind::DatePicked => {
                let d = &event.date;
                if let Some(day) = NaiveDate::from_ymd_opt(i32::try_from(d.year).unwrap_or(1970), d.month, d.day) {
                    s.todo_day = day;
                }
            }
            ToDoBarEventKind::TaskAdded => {
                let text = event.text.as_str().trim().to_string();
                if text.is_empty() {
                    return;
                }
                let mut parsed = s.parse_quick(&text, now);
                if parsed.due.is_none() {
                    parsed.due = Some(s.todo_day);
                }
                let ignore = std::mem::take(&mut s.quick.ignore);
                s.add_parsed(parsed, now);
                s.quick.ignore = ignore;
                s.drafts.todo.clear();
                let revision = info.get_document_text_revision();
                info.mark_text_revision_synced(revision);
            }
            ToDoBarEventKind::TaskToggled => {
                if let Some(&i) = todo_tasks(s, now).get(event.index) {
                    s.toggle_done(i, now);
                }
            }
            ToDoBarEventKind::TaskOpened => {
                if let Some(&i) = todo_tasks(s, now).get(event.index) {
                    let id = s.tasks[i].id.clone();
                    let list = s.tasks[i].list.clone();
                    if !s.visible_order(now).contains(&i) {
                        s.show(View::List(list));
                    }
                    s.select(&id, false, false);
                    s.sync_drafts();
                }
            }
            ToDoBarEventKind::AppointmentOpened => {}
        }
    })
}

// ==== Command palette ====

/// The palette over the window (closed: an empty node).
pub fn palette(s: &Tasks, app: &RefAny) -> Dom {
    let mut p = ShellCommandPalette::create()
        .with_placeholder("Type a command")
        .with_query(s.palette.clone().unwrap_or_default())
        .with_open(s.palette.is_some())
        .with_on_query(app.clone(), on_palette_query as ShellCommandPaletteOnQueryCallbackType)
        .with_on_run(app.clone(), on_palette_run as ShellCommandPaletteOnRunCallbackType)
        .with_on_close(app.clone(), on_palette_close as ButtonOnClickCallbackType);
    for command in Command::ALL {
        p = p.with_command(
            ShellPaletteCommand::create(command.label())
                .with_icon(command.icon())
                .with_shortcut(command.shortcut())
                .with_category(command.category()),
        );
    }
    p.dom()
}

extern "C" fn on_palette_query(mut data: RefAny, _info: CallbackInfo, query: AzString) -> Update {
    let Some(mut s) = data.downcast_mut::<Tasks>() else {
        return Update::DoNothing;
    };
    s.palette = Some(query.as_str().to_string());
    Update::RefreshDom
}

extern "C" fn on_palette_run(mut data: RefAny, mut info: CallbackInfo, index: usize) -> Update {
    let Some(command) = Command::ALL.get(index).copied() else {
        return Update::DoNothing;
    };
    println!("AZTASKS_RUN {}", command.label());
    crate::with_tasks(&mut data, &mut info, |info, app, s| run(info, app, s, command))
}

extern "C" fn on_palette_close(mut data: RefAny, mut info: CallbackInfo) -> Update {
    crate::with_tasks(&mut data, &mut info, |_info, _app, s| s.palette = None)
}

// ==== Confirmation ====

/// The question a destructive command asks, as a bar over the task list.
pub fn confirm_bar(s: &Tasks, app: &RefAny, now: NaiveDateTime) -> Option<Dom> {
    let question = match s.confirm.as_ref()? {
        Confirm::DeleteList(id) => {
            let n = s.tasks.iter().filter(|t| t.list == *id).count();
            format!(
                "Delete the list \"{}\" and its {n} task(s)? This cannot be undone.",
                s.list_name(id)
            )
        }
        Confirm::ClearCompleted => {
            let cutoff = now - chrono::Duration::days(30);
            let n = s.tasks.iter().filter(|t| t.completed.is_some_and(|c| c < cutoff)).count();
            format!("Delete the {n} task(s) completed more than 30 days ago?")
        }
    };
    Some(
        Dom::create_div()
            .with_id(ids::CONFIRM)
            .with_css("display: flex; flex-direction: row; align-items: center; gap: 8px; padding: 6px 16px 0px 16px;")
            .with_child(
                InfoBar::create(question)
                    .with_kind(AlertKind::Warning)
                    .with_icon("warning")
                    .dom()
                    .with_css("flex-grow: 1;"),
            )
            .with_child(
                Button::with_type("Delete", ButtonType::Danger)
                    .with_on_click(app.clone(), on_confirm_yes as ButtonOnClickCallbackType)
                    .dom()
                    .with_id(ids::CONFIRM_YES),
            )
            .with_child(
                Button::create("Cancel")
                    .with_on_click(app.clone(), on_confirm_no as ButtonOnClickCallbackType)
                    .dom()
                    .with_id(ids::CONFIRM_NO),
            ),
    )
}

extern "C" fn on_confirm_yes(mut data: RefAny, mut info: CallbackInfo) -> Update {
    crate::with_tasks(&mut data, &mut info, |info, app, s| {
        let gone = match s.confirm.take() {
            Some(Confirm::DeleteList(id)) => {
                s.editing_list = None;
                s.delete_list(&id)
            }
            Some(Confirm::ClearCompleted) => s.clear_completed(state::now()),
            None => Vec::new(),
        };
        crate::jobs::delete_files(info, app, s, gone);
    })
}

extern "C" fn on_confirm_no(mut data: RefAny, mut info: CallbackInfo) -> Update {
    crate::with_tasks(&mut data, &mut info, |_info, _app, s| s.confirm = None)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_command_table_names_every_command_once() {
        let labels: Vec<String> = Command::ALL.iter().map(|c| c.label()).collect();
        let mut unique = labels.clone();
        unique.sort();
        unique.dedup();
        assert_eq!(unique.len(), labels.len());
        assert_eq!(Command::Show(Smart::Upcoming).shortcut(), "Cmd+2");
        assert_eq!(Command::NewTask.category(), "Task");
    }
}
