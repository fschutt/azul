//! AzTasks: to-dos and reminders on the public azul API, laid out like Outlook 2010's Tasks
//! on the three-pane PIM shell (`PimShell`, S4):
//!
//! - the navigation pane (`nav.rs`): smart lists with counts (Today, Upcoming, Scheduled,
//!   Flagged, All, Completed), the lists in their groups, the tags; a search line;
//! - the task list (`list.rs`): a quick-add line that reads "Pay rent tomorrow 9am #home
//!   !high" (`parse.rs`, English and German) with chips of what it recognised, the tasks in
//!   sections, each row with its check box, priority, flag, due chip (red when overdue),
//!   repeat, list, steps, tags, files and notes; multi-select, drag to reorder;
//! - the detail pane (`detail.rs`, `listedit.rs`): title, steps, due date and time, repeat
//!   (presets and an editor), reminder, priority, flag, list, tags, notes, attachments;
//! - the ribbon (HOME / VIEW, FILE for the backstage: settings on `ShellSettingsLayout`,
//!   keyboard shortcuts, about), the To-Do bar, the status bar with the save state, the
//!   command palette (Cmd+K) - `chrome.rs`, `backstage.rs`.
//!
//! Durable data are files in the layout of the user's S3 bucket (the azlin cloud storage
//! split): `tasks/<list>/<task>.json`, `tasks/<list>/list.json`, attachments next to their
//! task, `tasks/settings.json` (`model.rs`), written through `azul-storage`'s `LocalDrive`
//! from an azul `Thread` (`store.rs`, `jobs.rs`), never from a callback. A completed
//! repeating task leaves its next occurrence behind (`recur.rs`). Reminders (`reminders.rs`)
//! show as a banner in the window and as an OS notification while the app runs.
//!
//! Environment: `AZTASKS_DATA` - the data folder (default `AZLIN_DATA`, the Azlin apps' data
//! root, else `<user data dir>/Azlin`);
//! `AZTASKS_TICK_MS` - how often reminders are checked (default 5000).
//! Command line: `args.rs` (`--sample`, `--data`, `--screen`, `--theme`, `--mode`, `--view`,
//! `--size`). On stdout, for scripts: see `state.rs` and `jobs.rs`, plus `AZTASKS_REMINDER
//! <task>` and `AZTASKS_NOTIFICATION <kind> <task>`.

pub mod appearance;
pub mod args;
pub mod backstage;
pub mod chrome;
pub mod detail;
pub mod ids;
pub mod jobs;
#[cfg(test)]
mod l10n_tests;
pub mod layouts;
pub mod list;
pub mod listedit;
pub mod model;
pub mod nav;
pub mod parse;
/// A to-do's repeat lives in the shared PIM crate (DEDUP_EDITORS B6).
pub use azul_pim::repeat as recur;
pub mod reminders;
pub mod repeat_form;
pub mod sample;
pub mod state;
pub mod store;
pub mod views;
// To-dos as iCalendar: azul-pim's, shared with the Azlin Bridge's CalDAV task lists.
pub use azul_pim::vtodo;

use std::{path::PathBuf, sync::Arc};

use azul::{
    css::DarkLightMode,
    dom::VirtualKeyCode,
    file::FilePath,
    notification::{Notification, NotificationEventType, NotificationSound},
    option::OptionDarkLightMode,
    prelude::*,
    shells::{PimShell, ShellThemeAccent, ShellThemeScope},
    str::String as AzString,
    time::SystemTimeDiff,
    window::{PlatformCapability, WindowDecorations},
};
use azul_storage::{Drive, LocalDrive};

use crate::{
    args::{Args, Screen},
    chrome::Command,
    model::Reminder,
    state::{Page, Tasks},
    views::{Smart, View},
};

/// The data folder's variable.
const DATA_VAR: &str = "AZTASKS_DATA";
/// The reminder tick's variable and default.
const TICK_VAR: &str = "AZTASKS_TICK_MS";
const TICK_MS: u64 = 5000;
/// The folder in the user's data folder when nothing else names one: the Azlin apps share
/// it (`notes/`, `tasks/`, ...), as they will share the user's bucket.
const DATA_DIR: &str = "Azlin";

/// Runs `f` on the app state; then drains the write queue and rebuilds.
pub(crate) fn with_tasks(
    data: &mut RefAny,
    info: &mut CallbackInfo,
    f: impl FnOnce(&mut CallbackInfo, &RefAny, &mut Tasks),
) -> Update {
    let app = data.clone();
    let Some(mut guard) = data.downcast_mut::<Tasks>() else {
        return Update::DoNothing;
    };
    let s = &mut *guard;
    f(info, &app, s);
    jobs::pump(info, &app, s);
    Update::RefreshDom
}

// ==== Layout ====

extern "C" fn layout(mut data: RefAny, info: LayoutCallbackInfo) -> Dom {
    // Reading the mode and the theme makes a switch of either rebuild the window.
    let dark = matches!(info.get_mode(), DarkLightMode::Dark);
    let theme = info.get_theme().as_str().to_string();
    let app = data.clone();
    let Some(guard) = data.downcast_ref::<Tasks>() else {
        return Dom::create_body();
    };
    let s = &*guard;
    let now = state::now();

    let mut list_column = Dom::create_div()
        .with_css("display: flex; flex-direction: column; flex-grow: 1; min-height: 0px;");
    if let Some(bar) = chrome::confirm_bar(s, &app, now) {
        list_column.add_child(bar);
    }
    list_column.add_child(list::pane(s, &app, now));

    let mut pim = PimShell::create(
        nav::pane(s, &app, now.date()),
        list_column,
        detail::pane(s, &app, now),
    )
    .with_list_label("Tasks")
    // LOOK 2026-10-03: the list was narrow (the reminder banner wrapped, "Dismiss" was cut) and
    // the reading pane far too wide; the navigation pane's lists were cut at its right edge.
    .with_navigation_ratio(0.22)
    .with_list_ratio(0.6);
    if s.show_todo_bar {
        pim = pim.with_todo_bar(chrome::todo_bar(s, &app, now));
    }
    // The chrome is the OfficeShell's.
    let mut shell = pim
        .office_shell()
        .with_title_row(chrome::title_row())
        .with_status_bar(chrome::status_bar(s, &app, now));
    shell = match s.page {
        Some(page) => shell.with_backstage(backstage::backstage(s, &app, page, &theme, dark)),
        None => shell.with_ribbon(chrome::ribbon(s, &app, &theme, dark)),
    };
    let root = Dom::create_div()
        .with_css("position: relative; display: flex; flex-direction: column; flex-grow: 1; min-height: 0px;")
        .with_child(shell.dom())
        .with_child(chrome::palette(s, &app));
    // LOOK: a hand-styled <body> kept the UA's 8 px margin (a white strip at the left and
    // bottom); the theme scope's own body (SMALL6) fills the window.
    ShellThemeScope::create(root)
        .with_accent(ShellThemeAccent::Leaf)
        .body()
        .with_callback(EventFilter::Window(WindowEventFilter::VirtualKeyDown), app, on_key)
}

// ==== Keyboard ====

/// Where the keyboard focus is, for the single-key shortcuts.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Focus {
    /// Nothing has the focus, or a task row has: every single key is a command.
    Free,
    /// A control that is no text field (a button, a tree row): N and Delete are commands;
    /// Space, Backspace and Alt+arrows stay the control's.
    Control,
    /// A text field: only Cmd keys are commands.
    Typing,
}

/// The command a key press means, if any.
fn command_of(key: VirtualKeyCode, cmd: bool, alt: bool, focus: Focus) -> Option<Command> {
    if cmd {
        let smart = |n: usize| Some(Command::Show(Smart::ALL[n]));
        return match key {
            VirtualKeyCode::N => Some(Command::NewTask),
            VirtualKeyCode::K => Some(Command::Palette),
            VirtualKeyCode::F => Some(Command::Search),
            VirtualKeyCode::Comma => Some(Command::Settings),
            VirtualKeyCode::Key1 => smart(0),
            VirtualKeyCode::Key2 => smart(1),
            VirtualKeyCode::Key3 => smart(2),
            VirtualKeyCode::Key4 => smart(3),
            VirtualKeyCode::Key5 => smart(4),
            VirtualKeyCode::Key6 => smart(5),
            _ => None,
        };
    }
    let free = focus == Focus::Free;
    match key {
        VirtualKeyCode::N if focus != Focus::Typing => Some(Command::NewTask),
        VirtualKeyCode::Delete if focus != Focus::Typing => Some(Command::Delete),
        VirtualKeyCode::Back if free => Some(Command::Delete),
        VirtualKeyCode::Space if free => Some(Command::Complete),
        VirtualKeyCode::Up if alt && free => Some(Command::MoveUp),
        VirtualKeyCode::Down if alt && free => Some(Command::MoveDown),
        _ => None,
    }
}

extern "C" fn on_key(mut data: RefAny, mut info: CallbackInfo) -> Update {
    let Some(key) = info.get_current_keyboard_state().current_virtual_keycode.into_option() else {
        return Update::DoNothing;
    };
    let mods = info.get_key_modifiers();
    let cmd = mods.primary_down();
    let focus = match info.get_focused_node().into_option() {
        None => Focus::Free,
        Some(node) => {
            let row = info
                .get_node_id(node)
                .into_option()
                .is_some_and(|id| ids::is_task_row(id.as_str()));
            let typing = info
                .get_node_attribute(node, "contenteditable")
                .into_option()
                .is_some_and(|v| v.as_str() == "true");
            if typing {
                Focus::Typing
            } else if row {
                Focus::Free
            } else {
                Focus::Control
            }
        }
    };
    let free = focus == Focus::Free;
    // Cmd+7 .. Cmd+9: the first three lists.
    let list_key = match key {
        VirtualKeyCode::Key7 => Some(0),
        VirtualKeyCode::Key8 => Some(1),
        VirtualKeyCode::Key9 => Some(2),
        _ => None,
    };
    if let (true, Some(n)) = (cmd, list_key) {
        info.prevent_default();
        return with_tasks(&mut data, &mut info, |_info, _app, s| {
            let order = views::lists_in_nav_order(&s.lists);
            if let Some(&li) = order.get(n) {
                let id = s.lists[li].id.clone();
                s.show(View::List(id));
            }
        });
    }
    if key == VirtualKeyCode::Escape {
        return with_tasks(&mut data, &mut info, |_info, _app, s| {
            if s.palette.is_some() {
                s.palette = None;
            } else if s.confirm.is_some() {
                s.confirm = None;
            } else if s.page.is_some() {
                s.page = None;
            } else if s.editing_list.is_some() {
                s.commit_drafts();
                s.editing_list = None;
            }
        });
    }
    if free && !cmd && matches!(key, VirtualKeyCode::Up | VirtualKeyCode::Down) && !mods.alt {
        info.prevent_default();
        return with_tasks(&mut data, &mut info, |_info, _app, s| {
            s.step_selection(key == VirtualKeyCode::Down, mods.shift);
            s.sync_drafts();
        });
    }
    let Some(command) = command_of(key, cmd, mods.alt, focus) else {
        return Update::DoNothing;
    };
    info.prevent_default();
    with_tasks(&mut data, &mut info, |info, app, s| {
        if command == Command::Palette {
            s.palette = if s.palette.is_some() { None } else { Some(String::new()) };
        } else {
            chrome::run(info, app, s, command);
        }
    })
}

// ==== Reminders ====

/// Shows the reminders due now: in the banner, and as an OS notification when the settings
/// ask for it. Each is marked shown (in its file) so it shows once. Returns whether any did.
pub(crate) fn check_reminders(info: &mut CallbackInfo, app: &RefAny, s: &mut Tasks) -> bool {
    if !s.loaded {
        return false;
    }
    let now = state::now();
    let due = reminders::due_now(&s.tasks, now, s.settings.reminder_time);
    if due.is_empty() {
        return false;
    }
    for i in due {
        let Some(at) = reminders::reminder_at(&s.tasks[i], s.settings.reminder_time) else {
            continue;
        };
        s.tasks[i].reminded = Some(at);
        let id = s.tasks[i].id.clone();
        if !s.banners.contains(&id) {
            s.banners.push(id.clone());
        }
        println!("AZTASKS_REMINDER {id}");
        if s.settings.notifications {
            let t = &s.tasks[i];
            let body = match views::due_label(t, now.date()) {
                Some(due) => format!("{} (due {due})", t.title),
                None => t.title.clone(),
            };
            let sound = if s.settings.sounds {
                NotificationSound::Default
            } else {
                NotificationSound::Silent
            };
            info.post_notification(
                Notification::create(format!("aztasks-{id}"), "Reminder")
                    .with_body(body)
                    .with_action("complete", "Complete")
                    .with_action("snooze", "Snooze 10 min")
                    .with_sound(sound)
                    .with_payload(id.as_str())
                    .with_callback(app.clone(), on_notification),
            );
        }
        s.save_task(i);
    }
    jobs::pump(info, app, s);
    true
}

/// What the user did with a reminder's notification.
extern "C" fn on_notification(mut data: RefAny, mut info: CallbackInfo) -> Update {
    let Some(event) = info.get_notification_event().into_option() else {
        return Update::DoNothing;
    };
    let id = event.payload.as_str().to_string();
    let kind = match event.kind {
        NotificationEventType::Activated => "activated",
        NotificationEventType::ActionInvoked => "action",
        NotificationEventType::Dismissed => "dismissed",
        NotificationEventType::Failed => "failed",
    };
    println!("AZTASKS_NOTIFICATION {kind} {id}");
    let action = event.action_id.as_str().to_string();
    let reason = event.reason.as_str().to_string();
    with_tasks(&mut data, &mut info, |_info, _app, s| {
        let Some(i) = s.index_of(&id) else {
            return;
        };
        match kind {
            "activated" => {
                let list = s.tasks[i].list.clone();
                s.show(View::List(list));
                s.select(&id, false, false);
                s.sync_drafts();
                s.banners.retain(|b| *b != id);
            }
            "action" if action == "complete" => {
                if !s.tasks[i].is_done() {
                    s.toggle_done(i, state::now());
                }
            }
            "action" if action == "snooze" => {
                s.tasks[i].reminder = Some(Reminder::At(state::now() + chrono::Duration::minutes(10)));
                s.tasks[i].reminded = None;
                s.banners.retain(|b| *b != id);
                s.save_task(i);
            }
            "failed" => {
                s.os_notifications.1 = reason.clone();
            }
            _ => {}
        }
    })
}

/// Every few seconds: the reminders due, a new day (the views move on), and the write
/// queue (a safety net; every change pumps it already).
extern "C" fn on_tick(mut data: RefAny, mut info: TimerCallbackInfo) -> TimerCallbackReturn {
    let app = data.clone();
    let Some(mut guard) = data.downcast_mut::<Tasks>() else {
        return TimerCallbackReturn::terminate_unchanged();
    };
    let s = &mut *guard;
    let mut refresh = check_reminders(&mut info.callback_info, &app, s);
    let now = state::now();
    if now.date() != s.clock.date() {
        if s.todo_day == s.clock.date() {
            s.todo_day = now.date();
        }
        s.clock = now;
        refresh = true;
    }
    jobs::pump(&mut info.callback_info, &app, s);
    if refresh {
        TimerCallbackReturn::continue_and_refresh_dom()
    } else {
        TimerCallbackReturn::continue_unchanged()
    }
}

// ==== Start ====

/// The window is up: read the files (on a thread) and start the reminder tick.
extern "C" fn startup(mut data: RefAny, mut info: CallbackInfo) -> Update {
    let tick = std::env::var(TICK_VAR)
        .ok()
        .and_then(|v| v.trim().parse::<u64>().ok())
        .filter(|&ms| ms >= 100)
        .unwrap_or(TICK_MS);
    with_tasks(&mut data, &mut info, |info, app, s| {
        jobs::spawn(info, app, s, jobs::Job::Load);
        let get_time = info.get_system_time_fn();
        info.add_timer(
            TimerId::unique(),
            Timer::create(app.clone(), on_tick, get_time)
                .with_interval(Duration::System(SystemTimeDiff::from_millis(tick))),
        );
        println!("AZTASKS_STARTED {}", s.root.display());
    })
}

/// The data folder: `--data`, else `AZTASKS_DATA`, else `AZLIN_DATA` (the root every Azlin app
/// shares), else `<user data dir>/Azlin`.
fn data_root(args: &Args) -> PathBuf {
    root_from(
        args.data.as_deref(),
        std::env::var(DATA_VAR).ok().as_deref(),
        std::env::var(azul_appkit::data::DATA_VAR).ok().as_deref(),
        FilePath::get_data_dir()
            .into_option()
            .map(|dir| PathBuf::from(dir.inner.as_str())),
    )
}

/// [`data_root`] from its sources: the switch, `AZTASKS_DATA` (`tasks_var`), `AZLIN_DATA`
/// (`azlin_var`) and the user's data folder.
fn root_from(
    flag: Option<&std::path::Path>,
    tasks_var: Option<&str>,
    azlin_var: Option<&str>,
    user_data: Option<PathBuf>,
) -> PathBuf {
    if let Some(dir) = flag {
        return dir.to_path_buf();
    }
    if let Some(dir) = tasks_var.filter(|v| !v.is_empty()) {
        return PathBuf::from(dir);
    }
    if let Some(dir) = azlin_var.map(str::trim).filter(|v| !v.is_empty()) {
        return PathBuf::from(dir);
    }
    user_data
        .map(|dir| dir.join(DATA_DIR))
        .unwrap_or_else(|| PathBuf::from("AzTasks-data"))
}

pub fn start() {
    let args = match Args::parse(std::env::args().skip(1)) {
        Ok(a) => a,
        Err(message) => {
            let help = message == args::HELP;
            if help {
                println!("{message}");
            } else {
                eprintln!("{message}");
            }
            std::process::exit(if help { 0 } else { 2 });
        }
    };
    let root = data_root(&args);
    if let Err(e) = std::fs::create_dir_all(&root) {
        eprintln!("[aztasks] could not make the data folder {}: {e}", root.display());
    }
    let drive: Arc<dyn Drive> = Arc::new(LocalDrive::new(root.clone()));
    let view = args.view.clone().unwrap_or(View::Smart(Smart::Today));
    let mut s = Tasks::new(drive, root.clone(), view);
    s.sample_requested = args.sample;
    match args.screen {
        Screen::Main => {}
        Screen::Settings => s.page = Some(Page::Settings),
        Screen::Shortcuts => s.page = Some(Page::Shortcuts),
        Screen::About => s.page = Some(Page::About),
        Screen::Palette => s.palette = Some(String::new()),
    }
    // Asked once: on Linux the probe is a D-Bus round trip.
    let cap = PlatformCapability::notifications();
    s.os_notifications = if cap.available {
        (true, cap.backend.as_str().to_string())
    } else {
        (
            false,
            format!("{}: {}", cap.backend.as_str(), cap.reason.as_str()),
        )
    };
    eprintln!(
        "[aztasks] data folder {}; notifications: {}",
        root.display(),
        s.os_notifications.1
    );

    // The appearance kept from the last run (read before the window: the first frame needs
    // it); `--theme` / `--mode` win for this run.
    if let Ok(bytes) = s.drive.get(&appearance::settings_key()) {
        let (saved, problem) = azul_appkit::settings::AppSettings::parse(&String::from_utf8_lossy(&bytes));
        if let Some(problem) = problem {
            eprintln!("[aztasks] {}: {problem}", appearance::settings_key());
        }
        s.appearance = saved;
    }
    let (theme, mode) = appearance::effective(&args, &s.appearance);
    let mut config = AppConfig::create()
        .with_theme(theme.name())
        .with_mode(match mode {
            azul_appkit::args::ModePref::Light => OptionDarkLightMode::Some(DarkLightMode::Light),
            azul_appkit::args::ModePref::Dark => OptionDarkLightMode::Some(DarkLightMode::Dark),
            azul_appkit::args::ModePref::System => OptionDarkLightMode::None,
        });
    // The kit's icons: Haiku's under flora, Material under flat.
    azul_appkit::ui::add_kit_icons(&mut config);
    let app = App::create(RefAny::new(s), config);
    let mut window = WindowCreateOptions::create(layout);
    let (w, h) = args.size.unwrap_or((1280.0, 800.0));
    window.window_state.size.dimensions = LogicalSize::create(w, h);
    window.window_state.title = AzString::from("AzTasks");
    window.window_state.flags.decorations = WindowDecorations::NoTitle;
    window.create_callback = Some(Callback::create(startup)).into();
    app.run(window);
}

#[cfg(test)]
mod data_tests {
    use std::path::Path;

    use super::*;

    #[test]
    fn the_data_root_is_the_switch_else_aztasks_data_else_azlin_data_else_the_users_azlin_folder() {
        let user = Some(PathBuf::from("/Users/a/Library/Application Support"));
        assert_eq!(
            root_from(
                Some(Path::new("/tmp/x")),
                Some("/srv/t"),
                Some("/srv/a"),
                user.clone()
            ),
            PathBuf::from("/tmp/x")
        );
        assert_eq!(
            root_from(None, Some("/srv/t"), Some("/srv/a"), user.clone()),
            PathBuf::from("/srv/t")
        );
        assert_eq!(
            root_from(None, None, Some(" /srv/a "), user.clone()),
            PathBuf::from("/srv/a"),
            "AZLIN_DATA, the root every Azlin app shares"
        );
        assert_eq!(
            root_from(None, Some(""), Some(" "), user),
            PathBuf::from("/Users/a/Library/Application Support/Azlin")
        );
        assert_eq!(
            root_from(None, None, None, None),
            PathBuf::from("AzTasks-data")
        );
    }
}

#[cfg(test)]
mod key_tests {
    use super::*;

    #[test]
    fn single_keys_are_commands_only_while_no_text_field_has_the_focus() {
        use Focus::{Control, Free, Typing};
        assert_eq!(command_of(VirtualKeyCode::N, false, false, Free), Some(Command::NewTask));
        assert_eq!(command_of(VirtualKeyCode::N, false, false, Control), Some(Command::NewTask));
        assert_eq!(command_of(VirtualKeyCode::N, false, false, Typing), None, "typing an n");
        assert_eq!(command_of(VirtualKeyCode::Space, false, false, Free), Some(Command::Complete));
        assert_eq!(command_of(VirtualKeyCode::Space, false, false, Control), None, "the button's");
        assert_eq!(command_of(VirtualKeyCode::Delete, false, false, Control), Some(Command::Delete));
        assert_eq!(command_of(VirtualKeyCode::Back, false, false, Typing), None);
        assert_eq!(command_of(VirtualKeyCode::Up, false, true, Free), Some(Command::MoveUp));
        assert_eq!(command_of(VirtualKeyCode::Up, false, false, Free), None, "the selection's");
    }

    #[test]
    fn cmd_keys_work_everywhere() {
        use Focus::{Free, Typing};
        assert_eq!(command_of(VirtualKeyCode::N, true, false, Typing), Some(Command::NewTask));
        assert_eq!(
            command_of(VirtualKeyCode::Key2, true, false, Typing),
            Some(Command::Show(Smart::Upcoming))
        );
        assert_eq!(command_of(VirtualKeyCode::K, true, false, Typing), Some(Command::Palette));
        assert_eq!(command_of(VirtualKeyCode::Comma, true, false, Free), Some(Command::Settings));
    }
}
