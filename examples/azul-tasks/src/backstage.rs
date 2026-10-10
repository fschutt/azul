//! FILE: the backstage with three pages - Settings (on `ShellSettingsLayout`: General,
//! Reminders, Appearance, Data), Keyboard shortcuts, About. Element ids for scripts:
//! `#backstage`, `#settings-default-list`, `#settings-week-start`, `#settings-reminder-time`,
//! `#settings-sounds`, `#settings-notifications`, `#settings-theme`, `#settings-mode`.

use std::path::PathBuf;

use azul::{
    callbacks::{
        BackstageOnNavSelectCallbackType, ButtonOnClickCallbackType,
        DropDownOnChoiceChangeCallbackType, SegmentedOnChangeCallbackType,
        ShellSettingsLayoutOnCategoryCallbackType, ShellSettingsLayoutOnSearchCallbackType,
        SwitchOnToggleCallbackType, TextInputOnTextInputCallbackType,
        TimePickerOnChangeCallbackType,
    },
    css::DarkLightMode,
    dialog::{FileDialog, FileOpenResult},
    option::{OptionDarkLightMode, OptionFileTypeList, OptionString},
    prelude::*,
    shells::{ShellSettingsLayout, ShellSettingsSection},
    str::String as AzString,
    vec::StringVec,
    widgets::{
        Backstage, BackstageNavItem, DropDown, OnTextInputReturn, Segmented, SegmentedState,
        Switch, SwitchState, TextInputState, TextInputValid, TimePicker, TimePickerState,
    },
};
use chrono::{NaiveTime, Timelike, Weekday};

use azul_appkit::args::{LanguagePref, ModePref, Theme};
use azul_appkit::l10n::{label, t, t_args, t_label, Arg};

use crate::{
    appearance,
    chrome::Command,
    ids,
    state::{Page, Tasks},
    views,
};

/// The settings' categories (keys of the resources).
pub const CATEGORIES: [&str; 4] = [
    "aztasks-settings-general",
    "aztasks-settings-reminders",
    "aztasks-group-appearance",
    "aztasks-settings-data",
];
/// The week-start choices (the kit's weekday words).
const WEEK_STARTS: [(Weekday, &str); 3] = [
    (Weekday::Mon, "kit-weekday-monday"),
    (Weekday::Sun, "kit-weekday-sunday"),
    (Weekday::Sat, "kit-weekday-saturday"),
];

const PAGE: &str = "display: flex; flex-direction: column; gap: 10px; padding: 24px 32px; \
                    flex-grow: 1; min-height: 0px; overflow-y: auto;";
const TEXT: &str = "font-size: 13px;";
const SOFT: &str = "font-size: 12px; color: system:secondary-text;";

/// Keys of the resources (or words as they are), said.
fn strings(items: &[&str]) -> StringVec {
    azul_appkit::l10n::labels(items)
}

/// A paragraph: a key of the resources, or words as they are.
fn line(text: impl Into<String>, css: &str) -> Dom {
    Dom::create_p_with_text(t_label(&text.into())).with_css(css)
}

fn row(control: Dom, text: &str) -> Dom {
    Dom::create_div()
        .with_css("display: flex; flex-direction: row; align-items: center; gap: 8px;")
        .with_child(control)
        .with_child(line(text, SOFT))
}

/// A section of the settings with its title (a key) said.
fn section(title: &str, content: Dom) -> ShellSettingsSection {
    ShellSettingsSection::create(label(title), content)
}

/// The FILE backstage on `page`; `theme` and `dark` are the window's.
pub fn backstage(s: &Tasks, app: &RefAny, page: Page, theme: &str, dark: bool) -> Dom {
    let items: Vec<BackstageNavItem> = Page::ALL
        .iter()
        .map(|p| BackstageNavItem::create(label(p.label())))
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
        .with_id(ids::BACKSTAGE)
}

// ==== Settings ====

fn settings(s: &Tasks, app: &RefAny, theme: &str, _dark: bool) -> Dom {
    let searching = !s.settings_search.trim().is_empty();
    let mut layout = ShellSettingsLayout::create(strings(&CATEGORIES))
        .with_active_category(s.settings_category)
        .with_search(s.settings_search.as_str())
        .with_search_placeholder(label("aztasks-settings-find"))
        .with_on_category(app.clone(), on_category as ShellSettingsLayoutOnCategoryCallbackType)
        .with_on_search(app.clone(), on_search as ShellSettingsLayoutOnSearchCallbackType);
    for (category, sections) in [
        (0, general(s, app)),
        (1, reminder_settings(s, app)),
        (2, appearance_settings(app, theme, s.appearance.mode, s.language)),
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
        section(
            "aztasks-settings-default-list",
            row(
                DropDown::create(StringVec::from(names))
                    .with_selected(selected)
                    .with_accessibility_name(label("aztasks-settings-default-list"))
                    .with_on_choice_change(app.clone(), on_default_list as DropDownOnChoiceChangeCallbackType)
                    .dom()
                    .with_id(ids::SETTINGS_DEFAULT_LIST),
                "aztasks-settings-default-list-what",
            ),
        ),
        section(
            "aztasks-settings-week-starts",
            Segmented::create(strings(&WEEK_STARTS.map(|(_, n)| n)))
                .with_selected_index(week)
                .with_on_change(app.clone(), on_week_start as SegmentedOnChangeCallbackType)
                .dom()
                .with_id(ids::SETTINGS_WEEK_START),
        ),
        section(
            "aztasks-settings-completed",
            row(
                Switch::create(s.settings.show_completed)
                    .with_accessibility_name(label("aztasks-settings-completed-name"))
                    .with_on_toggle(app.clone(), on_show_completed as SwitchOnToggleCallbackType)
                    .dom()
                    .with_id(ids::SETTINGS_SHOW_COMPLETED),
                "aztasks-settings-completed-what",
            ),
        ),
    ]
}

fn reminder_settings(s: &Tasks, app: &RefAny) -> Vec<ShellSettingsSection> {
    let time = s.settings.reminder_time;
    let (available, why) = &s.os_notifications;
    // `why`: the system's own words.
    let os_line = if *available {
        t_args("aztasks-settings-os-shows", &[("why", Arg::from(why.as_str()))])
    } else {
        t_args("aztasks-settings-os-none", &[("why", Arg::from(why.as_str()))])
    };
    vec![
        section(
            "aztasks-settings-reminder-time",
            row(
                TimePicker::create(time.hour(), time.minute())
                    .with_24h(true)
                    .with_accessibility_name(label("aztasks-settings-reminder-time"))
                    .with_on_change(app.clone(), on_reminder_time as TimePickerOnChangeCallbackType)
                    .dom()
                    .with_id(ids::SETTINGS_REMINDER_TIME),
                "aztasks-settings-reminder-time-what",
            ),
        ),
        section(
            "aztasks-settings-sounds",
            row(
                Switch::create(s.settings.sounds)
                    .with_accessibility_name(label("aztasks-settings-sounds-name"))
                    .with_on_toggle(app.clone(), on_sounds as SwitchOnToggleCallbackType)
                    .dom()
                    .with_id(ids::SETTINGS_SOUNDS),
                "aztasks-settings-sounds-what",
            ),
        ),
        section(
            "aztasks-settings-notifications",
            Dom::create_div()
                .with_css("display: flex; flex-direction: column; gap: 4px;")
                .with_child(row(
                    Switch::create(s.settings.notifications)
                        .with_accessibility_name(label("aztasks-settings-notifications-name"))
                        .with_on_toggle(app.clone(), on_notifications as SwitchOnToggleCallbackType)
                        .dom()
                        .with_id(ids::SETTINGS_NOTIFICATIONS),
                    "aztasks-settings-notifications-what",
                ))
                .with_child(line(os_line, SOFT)),
        ),
    ]
}

fn appearance_settings(
    app: &RefAny,
    theme: &str,
    mode: ModePref,
    language: LanguagePref,
) -> Vec<ShellSettingsSection> {
    let languages: Vec<&str> = LanguagePref::ALL.iter().map(|l| l.key()).collect();
    vec![
        section(
            "kit-general-theme",
            Segmented::create(strings(&["kit-theme-flat", "kit-theme-flora"]))
                // Flora or a spin of it ("flora:green").
                .with_selected_index(usize::from(Theme::parse(theme).is_some_and(Theme::is_flora)))
                .with_on_change(app.clone(), on_theme as SegmentedOnChangeCallbackType)
                .dom()
                .with_id(ids::SETTINGS_THEME),
        ),
        section(
            "kit-general-mode",
            Segmented::create(strings(&["kit-mode-system", "kit-mode-light", "kit-mode-dark"]))
                // The kept choice (System follows the OS), not only what is shown now.
                .with_selected_index(appearance::mode_index(mode))
                .with_on_change(app.clone(), on_mode as SegmentedOnChangeCallbackType)
                .dom()
                .with_id(ids::SETTINGS_MODE),
        ),
        // The words' language: the system's, English or German (appkit's words for them).
        section(
            "kit-general-language",
            Segmented::create(strings(&languages))
                .with_selected_index(language.index())
                .with_on_change(app.clone(), on_language as SegmentedOnChangeCallbackType)
                .dom()
                .with_id(ids::SETTINGS_LANGUAGE),
        ),
    ]
}

fn data(s: &Tasks, app: &RefAny) -> Vec<ShellSettingsSection> {
    let mut contents = Dom::create_div()
        .with_css("display: flex; flex-direction: column; gap: 4px;")
        .with_child(line(
            t_args(
                "aztasks-settings-contents",
                &[
                    ("lists", Arg::from(s.lists.len())),
                    ("tasks", Arg::from(s.tasks.len())),
                    ("open", Arg::from(s.tasks.iter().filter(|t| !t.is_done()).count())),
                    ("skipped", Arg::from(s.skipped.len())),
                ],
            ),
            TEXT,
        ));
    for skipped in &s.skipped {
        contents.add_child(line(format!("{}: {}", skipped.key, skipped.reason), SOFT));
    }
    vec![
        section(
            "aztasks-settings-data-folder",
            Dom::create_div()
                .with_css("display: flex; flex-direction: column; gap: 4px;")
                .with_child(line(s.root.display().to_string(), TEXT))
                .with_child(line("aztasks-settings-data-folder-what", SOFT)),
        ),
        section("aztasks-settings-contents-title", contents),
        section("aztasks-settings-import-export", import_export(s, app)),
        section(
            "aztasks-settings-sample",
            row(
                Button::create(label("aztasks-settings-add-sample"))
                    .with_on_click(app.clone(), on_sample as ButtonOnClickCallbackType)
                    .dom()
                    .with_id(ids::SETTINGS_SAMPLE),
                "aztasks-settings-sample-what",
            ),
        ),
    ]
}

/// iCalendar to-dos (VTODO, `vtodo.rs`): an import from a file into the default list, an
/// export of the list shown into `aztasks/exports/` in the data tree.
fn import_export(s: &Tasks, app: &RefAny) -> Dom {
    let mut out = Dom::create_div()
        .with_css("display: flex; flex-direction: column; gap: 6px;")
        .with_child(
            Dom::create_div()
                .with_css("display: flex; flex-direction: row; align-items: center; gap: 8px;")
                .with_child(
                    TextInput::create()
                        .with_text(s.import_path.as_str())
                        .with_placeholder("/path/to/tasks.ics")
                        .with_accessibility_name(label("aztasks-settings-import-file"))
                        .with_on_text_input(
                            app.clone(),
                            on_import_path as TextInputOnTextInputCallbackType,
                        )
                        .dom()
                        .with_id(ids::SETTINGS_IMPORT_PATH)
                        .with_css("flex-grow: 1; min-width: 200px;"),
                )
                .with_child(
                    Button::create(label("aztasks-settings-browse"))
                        .with_on_click(app.clone(), on_import_browse as ButtonOnClickCallbackType)
                        .dom()
                        .with_id(ids::SETTINGS_IMPORT_BROWSE),
                )
                .with_child(
                    Button::create(label("aztasks-settings-import"))
                        .with_on_click(app.clone(), on_import as ButtonOnClickCallbackType)
                        .dom()
                        .with_id(ids::SETTINGS_IMPORT),
                ),
        )
        .with_child(line("aztasks-settings-import-what", SOFT))
        .with_child(row(
            Button::create(label("aztasks-settings-export"))
                .with_on_click(app.clone(), on_export as ButtonOnClickCallbackType)
                .dom()
                .with_id(ids::SETTINGS_EXPORT),
            "aztasks-settings-export-what",
        ));
    if !s.io_message.is_empty() {
        out.add_child(line(s.io_message.as_str(), TEXT).with_id(ids::SETTINGS_IO_MESSAGE));
    }
    out
}

// ==== Shortcuts and About ====

fn shortcuts() -> Dom {
    let mut page = Dom::create_div()
        .with_id(ids::SHORTCUTS)
        .with_css(PAGE)
        .with_child(Dom::create_h2_with_text(label("aztasks-cmd-shortcuts")).with_css("font-size: 20px;"));
    let mut rows: Vec<(String, String)> = Command::ALL
        .iter()
        .chain(std::iter::once(&Command::Palette))
        .filter(|c| !c.shortcut().is_empty())
        .map(|c| (c.shortcut().to_string(), c.label()))
        .collect();
    rows.extend(
        [
            ("aztasks-keys-up-down", "aztasks-keys-up-down-what"),
            ("aztasks-keys-click", "aztasks-keys-click-what"),
            ("aztasks-keys-drag", "aztasks-keys-drag-what"),
            ("aztasks-keys-enter", "aztasks-keys-enter-what"),
            ("aztasks-keys-esc", "aztasks-keys-esc-what"),
            ("aztasks-keys-lists", "aztasks-keys-lists-what"),
        ]
        .iter()
        .map(|(k, v)| (t(k), t(v))),
    );
    for (keys, what) in rows {
        page.add_child(
            Dom::create_div()
                .with_css("display: flex; flex-direction: row; gap: 16px;")
                .with_child(Dom::create_span_with_text(keys).with_css("min-width: 220px; font-weight: bold; font-size: 13px;"))
                .with_child(Dom::create_span_with_text(what).with_css(TEXT)),
        );
    }
    page.with_child(line("aztasks-keys-note", SOFT))
}

fn about(s: &Tasks) -> Dom {
    let (available, why) = &s.os_notifications;
    Dom::create_div()
        .with_id(ids::ABOUT)
        .with_css(PAGE)
        .with_child(Dom::create_h2_with_text("AzTasks").with_css("font-size: 24px;"))
        .with_child(line(
            t_args("aztasks-about-version", &[("version", Arg::from(env!("CARGO_PKG_VERSION")))]),
            TEXT,
        ))
        .with_child(line("aztasks-about-summary", TEXT))
        .with_child(line(
            t_args("aztasks-about-data", &[("folder", Arg::from(s.root.display().to_string()))]),
            SOFT,
        ))
        .with_child(line(
            if *available {
                t_args("aztasks-about-notifications", &[("why", Arg::from(why.as_str()))])
            } else {
                t_args("aztasks-about-no-notifications", &[("why", Arg::from(why.as_str()))])
            },
            SOFT,
        ))
        .with_child(line("aztasks-about-built", SOFT))
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

/// The language of the words: in effect at once (every window), kept for the next start.
extern "C" fn on_language(mut data: RefAny, mut info: CallbackInfo, state: SegmentedState) -> Update {
    let language = LanguagePref::ALL[state.selected_index.min(LanguagePref::ALL.len() - 1)];
    info.set_locale(language.tag());
    println!("AZTASKS_LANGUAGE {}", language.name());
    crate::with_tasks(&mut data, &mut info, |_info, _app, s| {
        s.language = language;
        s.appearance.set_language(language);
        s.save_appearance();
    })
}

extern "C" fn on_import_path(mut data: RefAny, _info: CallbackInfo, state: TextInputState) -> OnTextInputReturn {
    if let Some(mut s) = data.downcast_mut::<Tasks>() {
        s.import_path = state.get_text().as_str().to_string();
    }
    OnTextInputReturn {
        update: Update::DoNothing,
        valid: TextInputValid::Yes,
    }
}

extern "C" fn on_import_browse(data: RefAny, _info: CallbackInfo) -> Update {
    let _request = FileDialog::open_file(
        label("aztasks-import-dialog"),
        OptionString::None,
        OptionFileTypeList::None,
        data,
        on_import_picked,
    );
    Update::DoNothing
}

extern "C" fn on_import_picked(mut data: RefAny, mut info: CallbackInfo, result: RefAny) -> Update {
    let Some(path) = FileOpenResult::downcast(result)
        .into_option()
        .and_then(|picked| picked.path.into_option())
    else {
        return Update::DoNothing; // cancelled
    };
    let path = path.as_string().as_str().to_string();
    crate::with_tasks(&mut data, &mut info, |_info, _app, s| s.import_path = path)
}

/// Import: the file is read on a job thread (`jobs::Job::ReadImport`).
extern "C" fn on_import(mut data: RefAny, mut info: CallbackInfo) -> Update {
    crate::with_tasks(&mut data, &mut info, |info, app, s| {
        let path = s.import_path.trim().to_string();
        if path.is_empty() {
            s.io_message = t("aztasks-import-give-file");
            return;
        }
        s.io_message = t_args("aztasks-import-reading", &[("path", Arg::from(path.as_str()))]);
        crate::jobs::spawn(info, app, s, crate::jobs::Job::ReadImport(PathBuf::from(path)));
    })
}

/// Export: the list shown into the data tree, through the write queue.
extern "C" fn on_export(mut data: RefAny, mut info: CallbackInfo) -> Update {
    crate::with_tasks(&mut data, &mut info, |_info, _app, s| {
        let (key, count) = s.export_tasks(crate::state::now(), &crate::state::local_to_utc);
        println!("AZTASKS_EXPORTED {count} {key}");
        s.io_message = t_args(
            "aztasks-exported",
            &[
                ("count", Arg::from(count)),
                ("path", Arg::from(s.root.join(&key).display().to_string())),
            ],
        );
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
        assert_eq!(
            CATEGORIES,
            ["aztasks-settings-general", "aztasks-settings-reminders", "aztasks-group-appearance", "aztasks-settings-data"]
        );
        assert_eq!(WEEK_STARTS[0].0, Weekday::Mon);
        assert_eq!(Page::ALL.len(), 3);
    }
}
