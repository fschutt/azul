//! FILE: the backstage with three pages - Settings (on `ShellSettingsLayout`: General,
//! Reminders, Appearance, Data), Keyboard shortcuts, About. Element ids for scripts:
//! `#backstage`, `#settings-default-list`, `#settings-week-start`, `#settings-reminder-time`,
//! `#settings-sounds`, `#settings-notifications`, `#settings-theme`, `#settings-mode`.

use azul::{
    callbacks::{
        BackstageOnNavSelectCallbackType, ButtonOnClickCallbackType,
        DropDownOnChoiceChangeCallbackType, SegmentedOnChangeCallbackType,
        ShellSettingsLayoutOnCategoryCallbackType, ShellSettingsLayoutOnSearchCallbackType,
        SwitchOnToggleCallbackType, TimePickerOnChangeCallbackType,
    },
    css::DarkLightMode,
    option::OptionDarkLightMode,
    prelude::*,
    shells::{ShellSettingsLayout, ShellSettingsSection},
    str::String as AzString,
    vec::StringVec,
    widgets::{
        Backstage, BackstageNavItem, DropDown, Segmented, SegmentedState, Switch, SwitchState,
        TimePicker, TimePickerState,
    },
};
use chrono::{NaiveTime, Timelike, Weekday};

use azul_appkit::args::{ModePref, Theme};

use crate::{
    appearance,
    chrome::Command,
    state::{Page, Tasks},
    views,
};

/// The settings' categories.
pub const CATEGORIES: [&str; 4] = ["General", "Reminders", "Appearance", "Data"];
/// The week-start choices.
const WEEK_STARTS: [(Weekday, &str); 3] = [
    (Weekday::Mon, "Monday"),
    (Weekday::Sun, "Sunday"),
    (Weekday::Sat, "Saturday"),
];

const PAGE: &str = "display: flex; flex-direction: column; gap: 10px; padding: 24px 32px; \
                    flex-grow: 1; min-height: 0px; overflow-y: auto;";
const TEXT: &str = "font-size: 13px;";
const SOFT: &str = "font-size: 12px; color: system:secondary-text;";

fn strings(items: &[&str]) -> StringVec {
    StringVec::from(items.iter().map(|s| AzString::from(*s)).collect::<Vec<_>>())
}

fn line(text: impl Into<String>, css: &str) -> Dom {
    Dom::create_p_with_text(text.into()).with_css(css)
}

fn row(control: Dom, text: &str) -> Dom {
    Dom::create_div()
        .with_css("display: flex; flex-direction: row; align-items: center; gap: 8px;")
        .with_child(control)
        .with_child(line(text, SOFT))
}

/// The FILE backstage on `page`; `theme` and `dark` are the window's.
pub fn backstage(s: &Tasks, app: &RefAny, page: Page, theme: &str, dark: bool) -> Dom {
    let items: Vec<BackstageNavItem> = Page::ALL
        .iter()
        .map(|p| BackstageNavItem::create(p.label()))
        .collect();
    let content = match page {
        Page::Settings => settings(s, app, theme, dark),
        Page::Shortcuts => shortcuts(),
        Page::About => about(s),
    };
    Backstage::create(items)
        .with_active_item(Page::ALL.iter().position(|p| *p == page).unwrap_or(0))
        .with_on_nav_select(app.clone(), on_nav as BackstageOnNavSelectCallbackType)
        .with_on_back(app.clone(), on_back as ButtonOnClickCallbackType)
        .with_content(content)
        .dom()
        .with_id("backstage")
}

// ==== Settings ====

fn settings(s: &Tasks, app: &RefAny, theme: &str, _dark: bool) -> Dom {
    let searching = !s.settings_search.trim().is_empty();
    let mut layout = ShellSettingsLayout::create(strings(&CATEGORIES))
        .with_active_category(s.settings_category)
        .with_search(s.settings_search.as_str())
        .with_search_placeholder("Find a setting")
        .with_on_category(app.clone(), on_category as ShellSettingsLayoutOnCategoryCallbackType)
        .with_on_search(app.clone(), on_search as ShellSettingsLayoutOnSearchCallbackType);
    for (category, sections) in [
        (0, general(s, app)),
        (1, reminder_settings(s, app)),
        (2, appearance_settings(app, theme, s.appearance.mode)),
        (3, data(s, app)),
    ] {
        if searching || category == s.settings_category {
            for section in sections {
                layout = layout.with_section(section);
            }
        }
    }
    layout.dom()
}

fn general(s: &Tasks, app: &RefAny) -> Vec<ShellSettingsSection> {
    let order = views::lists_in_nav_order(&s.lists);
    let names: Vec<AzString> = order.iter().map(|&i| AzString::from(s.lists[i].name.as_str())).collect();
    let current = s.default_list();
    let selected = order
        .iter()
        .position(|&i| Some(&s.lists[i].id) == current.as_ref())
        .unwrap_or(0);
    let week = WEEK_STARTS
        .iter()
        .position(|(d, _)| *d == s.settings.week_start)
        .unwrap_or(0);
    vec![
        ShellSettingsSection::create(
            "Default list",
            row(
                DropDown::create(StringVec::from(names))
                    .with_selected(selected)
                    .with_accessibility_name("Default list")
                    .with_on_choice_change(app.clone(), on_default_list as DropDownOnChoiceChangeCallbackType)
                    .dom()
                    .with_id("settings-default-list"),
                "where a new task goes from Today, All or a tag",
            ),
        ),
        ShellSettingsSection::create(
            "Week starts on",
            Segmented::create(strings(&WEEK_STARTS.map(|(_, n)| n)))
                .with_selected_index(week)
                .with_on_change(app.clone(), on_week_start as SegmentedOnChangeCallbackType)
                .dom()
                .with_id("settings-week-start"),
        ),
        ShellSettingsSection::create(
            "Completed tasks",
            row(
                Switch::create(s.settings.show_completed)
                    .with_accessibility_name("Show completed tasks under a list")
                    .with_on_toggle(app.clone(), on_show_completed as SwitchOnToggleCallbackType)
                    .dom()
                    .with_id("settings-show-completed"),
                "show them under a list's open tasks (folded)",
            ),
        ),
    ]
}

fn reminder_settings(s: &Tasks, app: &RefAny) -> Vec<ShellSettingsSection> {
    let t = s.settings.reminder_time;
    let (available, why) = &s.os_notifications;
    let os_line = if *available {
        format!("This system shows them ({why}).")
    } else {
        format!("Not on this system: {why}. Reminders show in the window only.")
    };
    vec![
        ShellSettingsSection::create(
            "Reminder time",
            row(
                TimePicker::create(t.hour(), t.minute())
                    .with_24h(true)
                    .with_accessibility_name("Reminder time")
                    .with_on_change(app.clone(), on_reminder_time as TimePickerOnChangeCallbackType)
                    .dom()
                    .with_id("settings-reminder-time"),
                "for tasks due on a day without a time, and a new due time",
            ),
        ),
        ShellSettingsSection::create(
            "Sounds",
            row(
                Switch::create(s.settings.sounds)
                    .with_accessibility_name("Play a sound with a reminder")
                    .with_on_toggle(app.clone(), on_sounds as SwitchOnToggleCallbackType)
                    .dom()
                    .with_id("settings-sounds"),
                "play the system's sound with a reminder",
            ),
        ),
        ShellSettingsSection::create(
            "Notifications",
            Dom::create_div()
                .with_css("display: flex; flex-direction: column; gap: 4px;")
                .with_child(row(
                    Switch::create(s.settings.notifications)
                        .with_accessibility_name("Show reminders as notifications")
                        .with_on_toggle(app.clone(), on_notifications as SwitchOnToggleCallbackType)
                        .dom()
                        .with_id("settings-notifications"),
                    "show a reminder as a notification of the system too",
                ))
                .with_child(line(os_line, SOFT)),
        ),
    ]
}

fn appearance_settings(app: &RefAny, theme: &str, mode: ModePref) -> Vec<ShellSettingsSection> {
    vec![
        ShellSettingsSection::create(
            "Theme",
            Segmented::create(strings(&["Flat", "Flora"]))
                .with_selected_index(usize::from(theme == "flora"))
                .with_on_change(app.clone(), on_theme as SegmentedOnChangeCallbackType)
                .dom()
                .with_id("settings-theme"),
        ),
        ShellSettingsSection::create(
            "Mode",
            Segmented::create(strings(&["System", "Light", "Dark"]))
                // The kept choice (System follows the OS), not only what is shown now.
                .with_selected_index(appearance::mode_index(mode))
                .with_on_change(app.clone(), on_mode as SegmentedOnChangeCallbackType)
                .dom()
                .with_id("settings-mode"),
        ),
    ]
}

fn data(s: &Tasks, app: &RefAny) -> Vec<ShellSettingsSection> {
    let mut contents = Dom::create_div()
        .with_css("display: flex; flex-direction: column; gap: 4px;")
        .with_child(line(
            format!(
                "{} lists, {} tasks ({} open), {} file(s) left out",
                s.lists.len(),
                s.tasks.len(),
                s.tasks.iter().filter(|t| !t.is_done()).count(),
                s.skipped.len()
            ),
            TEXT,
        ));
    for skipped in &s.skipped {
        contents.add_child(line(format!("{}: {}", skipped.key, skipped.reason), SOFT));
    }
    vec![
        ShellSettingsSection::create(
            "Data folder",
            Dom::create_div()
                .with_css("display: flex; flex-direction: column; gap: 4px;")
                .with_child(line(s.root.display().to_string(), TEXT))
                .with_child(line(
                    "One file per task: tasks/<list>/<task>.json, a list's tasks/<list>/list.json, \
                     its attachments next to the task. The same layout as the S3 bucket the files \
                     can move to.",
                    SOFT,
                )),
        ),
        ShellSettingsSection::create("Contents", contents),
        ShellSettingsSection::create(
            "Sample",
            row(
                Button::create("Add the sample tasks")
                    .with_on_click(app.clone(), on_sample as ButtonOnClickCallbackType)
                    .dom()
                    .with_id("settings-sample"),
                "lists and tasks to try AzTasks with",
            ),
        ),
    ]
}

// ==== Shortcuts and About ====

fn shortcuts() -> Dom {
    let mut page = Dom::create_div()
        .with_id("shortcuts")
        .with_css(PAGE)
        .with_child(Dom::create_h2_with_text("Keyboard shortcuts").with_css("font-size: 20px;"));
    let mut rows: Vec<(String, String)> = Command::ALL
        .iter()
        .chain(std::iter::once(&Command::Palette))
        .filter(|c| !c.shortcut().is_empty())
        .map(|c| (c.shortcut().to_string(), c.label()))
        .collect();
    rows.extend(
        [
            ("Up / Down", "Select the task above / below (Shift extends)"),
            ("Click / Shift+click / Cmd+click", "Select a task / a range / add one"),
            ("Drag a task", "Put it before another (onto another list's task: move it there)"),
            ("Enter (quick add)", "Add the typed task; a click on a chip keeps its words"),
            ("Esc", "Close the palette or the backstage; clear the quick-add line"),
            ("Cmd+7 .. Cmd+9", "Your first three lists"),
        ]
        .iter()
        .map(|(k, v)| ((*k).to_string(), (*v).to_string())),
    );
    for (keys, what) in rows {
        page.add_child(
            Dom::create_div()
                .with_css("display: flex; flex-direction: row; gap: 16px;")
                .with_child(Dom::create_span_with_text(keys).with_css("min-width: 220px; font-weight: bold; font-size: 13px;"))
                .with_child(Dom::create_span_with_text(what).with_css(TEXT)),
        );
    }
    page.with_child(line(
        "Cmd is the Command key on macOS and Ctrl elsewhere. Single keys work while no text field has the focus.",
        SOFT,
    ))
}

fn about(s: &Tasks) -> Dom {
    let (available, why) = &s.os_notifications;
    Dom::create_div()
        .with_id("about")
        .with_css(PAGE)
        .with_child(Dom::create_h2_with_text("AzTasks").with_css("font-size: 24px;"))
        .with_child(line(format!("Version {}", env!("CARGO_PKG_VERSION")), TEXT))
        .with_child(line(
            "To-dos and reminders: smart lists, lists in groups, tags, quick add in plain words \
             (English and German), repeating tasks, reminders, steps, notes and files.",
            TEXT,
        ))
        .with_child(line(format!("Data folder: {}", s.root.display()), SOFT))
        .with_child(line(
            if *available {
                format!("Notifications: {why}")
            } else {
                format!("Notifications: not on this system ({why})")
            },
            SOFT,
        ))
        .with_child(line("Built on azul. MIT licensed.", SOFT))
}

// ==== Callbacks ====

extern "C" fn on_nav(mut data: RefAny, _info: CallbackInfo, index: usize) -> Update {
    let Some(mut s) = data.downcast_mut::<Tasks>() else {
        return Update::DoNothing;
    };
    s.page = Page::ALL.get(index).copied();
    Update::RefreshDom
}

extern "C" fn on_back(mut data: RefAny, _info: CallbackInfo) -> Update {
    let Some(mut s) = data.downcast_mut::<Tasks>() else {
        return Update::DoNothing;
    };
    s.page = None;
    Update::RefreshDom
}

extern "C" fn on_category(mut data: RefAny, _info: CallbackInfo, index: usize) -> Update {
    let Some(mut s) = data.downcast_mut::<Tasks>() else {
        return Update::DoNothing;
    };
    s.settings_category = index.min(CATEGORIES.len() - 1);
    Update::RefreshDom
}

extern "C" fn on_search(mut data: RefAny, _info: CallbackInfo, query: AzString) -> Update {
    let Some(mut s) = data.downcast_mut::<Tasks>() else {
        return Update::DoNothing;
    };
    s.settings_search = query.as_str().to_string();
    Update::RefreshDom
}

extern "C" fn on_default_list(mut data: RefAny, mut info: CallbackInfo, index: usize) -> Update {
    crate::with_tasks(&mut data, &mut info, |_info, _app, s| {
        let order = views::lists_in_nav_order(&s.lists);
        if let Some(&li) = order.get(index) {
            s.settings.default_list = s.lists[li].id.clone();
            s.save_settings();
        }
    })
}

extern "C" fn on_week_start(mut data: RefAny, mut info: CallbackInfo, state: SegmentedState) -> Update {
    crate::with_tasks(&mut data, &mut info, |_info, _app, s| {
        if let Some((day, _)) = WEEK_STARTS.get(state.selected_index) {
            s.settings.week_start = *day;
            s.save_settings();
        }
    })
}

extern "C" fn on_show_completed(mut data: RefAny, mut info: CallbackInfo, state: SwitchState) -> Update {
    crate::with_tasks(&mut data, &mut info, |_info, _app, s| {
        s.settings.show_completed = state.checked;
        s.save_settings();
    })
}

extern "C" fn on_reminder_time(mut data: RefAny, mut info: CallbackInfo, state: TimePickerState) -> Update {
    let hour = if state.is_24h {
        state.hour
    } else {
        state.hour % 12 + if state.is_pm { 12 } else { 0 }
    };
    let Some(time) = NaiveTime::from_hms_opt(hour, state.minute, 0) else {
        return Update::DoNothing;
    };
    crate::with_tasks(&mut data, &mut info, |_info, _app, s| {
        s.settings.reminder_time = time;
        s.save_settings();
    })
}

extern "C" fn on_sounds(mut data: RefAny, mut info: CallbackInfo, state: SwitchState) -> Update {
    crate::with_tasks(&mut data, &mut info, |_info, _app, s| {
        s.settings.sounds = state.checked;
        s.save_settings();
    })
}

extern "C" fn on_notifications(mut data: RefAny, mut info: CallbackInfo, state: SwitchState) -> Update {
    crate::with_tasks(&mut data, &mut info, |info, _app, s| {
        s.settings.notifications = state.checked;
        if state.checked {
            info.request_notification_permission();
        }
        s.save_settings();
    })
}

/// The theme: shown now and kept for the next start (`appearance.rs`).
extern "C" fn on_theme(mut data: RefAny, mut info: CallbackInfo, state: SegmentedState) -> Update {
    let theme = if state.selected_index == 1 {
        Theme::Flora
    } else {
        Theme::Flat
    };
    info.set_theme(theme.name());
    crate::with_tasks(&mut data, &mut info, |_info, _app, s| {
        s.appearance.theme = theme;
        s.save_appearance();
    })
}

/// The mode: System (the OS's), Light or Dark, shown now and kept for the next start.
extern "C" fn on_mode(mut data: RefAny, mut info: CallbackInfo, state: SegmentedState) -> Update {
    let mode = appearance::mode_of_index(state.selected_index);
    info.set_mode(match mode {
        ModePref::Light => OptionDarkLightMode::Some(DarkLightMode::Light),
        ModePref::Dark => OptionDarkLightMode::Some(DarkLightMode::Dark),
        ModePref::System => OptionDarkLightMode::None,
    });
    crate::with_tasks(&mut data, &mut info, |_info, _app, s| {
        s.appearance.mode = mode;
        s.save_appearance();
    })
}

extern "C" fn on_sample(mut data: RefAny, mut info: CallbackInfo) -> Update {
    crate::with_tasks(&mut data, &mut info, |_info, _app, s| {
        s.add_sample(crate::state::now());
        s.page = None;
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_settings_have_four_categories_and_three_week_starts() {
        assert_eq!(CATEGORIES, ["General", "Reminders", "Appearance", "Data"]);
        assert_eq!(WEEK_STARTS[0].0, Weekday::Mon);
        assert_eq!(Page::ALL.len(), 3);
    }
}
