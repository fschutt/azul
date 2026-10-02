//! The "Dialogs" section: the install wizard's frames and pages, the path
//! input, the shortcut recorder, the settings dialog and the standard
//! dialog bodies (message box, About box, progress, login, find / replace).
//!
//! Every value the cards show is the APP's (`DialogsDemo`): each widget is
//! built from it and reports back into it, so a rebuild never loses a tick,
//! a typed text or a recorded shortcut. AzSetup (examples/azul-setup) is the
//! whole installer built from the same pieces.

use azul::{
    app::{GlobalHotkey, HotkeyModifiers},
    dom::VirtualKeyCode,
    prelude::*,
    shells::*,
    str::String as AzString,
    vec::*,
    widgets::*,
};

use crate::{captioned, section, strs, Showcase};

/// The status line.
const NOTE_CSS: &str = "font-size: 12px; color: system:secondary-text; margin: 0px;";
/// A box of fixed height for a page or a dialog body.
const BOX_CSS: &str = "display: flex; flex-direction: column; height: 300px; border: 1px solid \
                       system:separator;";
/// Two boxes side by side.
const ROW_CSS: &str = "display: flex; flex-direction: row; gap: 12px; align-items: stretch;";

const MB: u64 = 1024 * 1024;

/// Every value the dialog cards show.
#[derive(Clone)]
pub(crate) struct DialogsDemo {
    accepted: bool,
    path: AzString,
    components: WizardComponentsPage,
    options: WizardOptionsPage,
    show_log: bool,
    recorder: ShortcutRecorder,
    settings: ShellSettingsDialog,
    dont_ask: bool,
    user: AzString,
    password: AzString,
    remember: bool,
    find: AzString,
    match_case: bool,
    whole_word: bool,
    status: AzString,
}

impl DialogsDemo {
    pub(crate) fn create() -> Self {
        let components = WizardComponentsPage::create(WizardComponentVec::from_vec(vec![
            WizardComponent::create("AzOffice core", 180 * MB).with_required(true),
            WizardComponent::create("Applications", 0),
            WizardComponent::create("AzWriter", 120 * MB).with_depth(1),
            WizardComponent::create("AzSheets", 90 * MB).with_depth(1),
            WizardComponent::create("Templates", 45 * MB)
                .with_description("Invoices, CVs, calendars"),
        ]));
        let options = WizardOptionsPage::create(WizardOptionVec::from_vec(vec![
            WizardOption::create("Create a desktop shortcut", true),
            WizardOption::create("Just for me", true).with_group(1),
            WizardOption::create("For everyone", false).with_group(1),
        ]));
        let recorder = ShortcutRecorder::create()
            .with_accessibility_name("Command palette")
            .with_hotkey(GlobalHotkey::create(
                HotkeyModifiers {
                    ctrl: true,
                    alt: false,
                    shift: true,
                    meta: false,
                },
                VirtualKeyCode::P,
            ));
        let settings =
            ShellSettingsDialog::create(StringVec::from_vec(strs(&["General", "Appearance"])))
                .with_category_icons(StringVec::from_vec(strs(&["tune", "palette"])))
                .with_setting(
                    ShellSetting::create(
                        "general.reopen",
                        "Reopen the last documents",
                        0,
                        "Startup",
                        ShellSettingValue::Toggle(true),
                    )
                    .with_help("Open what was open when the app closed."),
                )
                .with_setting(
                    ShellSetting::create(
                        "general.language",
                        "Language",
                        0,
                        "Region",
                        ShellSettingValue::Choice(ShellSettingChoice::create(
                            StringVec::from_vec(strs(&["English", "Deutsch"])),
                            0,
                        )),
                    )
                    .with_requires_restart(true),
                )
                .with_setting(ShellSetting::create(
                    "appearance.zoom",
                    "Interface zoom",
                    1,
                    "Size",
                    ShellSettingValue::Slider(
                        ShellSettingNumber::create(100.0, 50.0, 200.0).with_unit("%"),
                    ),
                ))
                .with_setting(ShellSetting::create(
                    "appearance.mode",
                    "Mode",
                    1,
                    "Theme",
                    ShellSettingValue::Radio(ShellSettingChoice::create(
                        StringVec::from_vec(strs(&["Light", "Dark", "System"])),
                        2,
                    )),
                ));
        Self {
            accepted: false,
            path: "/opt/AzOffice".into(),
            components,
            options,
            show_log: false,
            recorder,
            settings,
            dont_ask: false,
            user: "".into(),
            password: "".into(),
            remember: false,
            find: "azul".into(),
            match_case: false,
            whole_word: false,
            status: "Nothing reported yet.".into(),
        }
    }
}

/// Keep what a widget reported and rebuild.
fn keep(data: &mut RefAny, put: impl FnOnce(&mut DialogsDemo)) -> Update {
    match data.downcast_mut::<Showcase>() {
        Some(mut s) => {
            put(&mut s.dialogs);
            s.interactions += 1;
            Update::RefreshDom
        }
        None => Update::DoNothing,
    }
}

fn boxed(content: Dom) -> Dom {
    Dom::create_div().with_css(BOX_CSS).with_child(content)
}

fn side_by_side(a: Dom, b: Dom) -> Dom {
    let half = |content: Dom| {
        Dom::create_div()
            .with_css("display: flex; flex-direction: column; flex-grow: 1; flex-basis: 0px; min-width: 0px;")
            .with_child(content)
    };
    Dom::create_div()
        .with_css(ROW_CSS)
        .with_child(half(a))
        .with_child(half(b))
}

/// The "Dialogs" section.
pub(crate) fn dialogs_section(data: &RefAny, d: &DialogsDemo, theme: UiTheme) -> Dom {
    let steps = StringVec::from_vec(strs(&[
        "Welcome",
        "License",
        "Destination",
        "Install",
        "Done",
    ]));

    // The Windows installer's banner over the destination page.
    let destination = WizardDestinationPage::create(d.path.clone(), d.components.total_bytes())
        .with_available(400 * MB)
        .with_on_event(data.clone(), on_page)
        .with_theme(theme);
    let reason = destination.blocked_reason();
    let banner = WizardLayout::create("AzOffice Setup", steps.clone())
        .with_current_step(2)
        .with_style(WizardLayoutStyle::Banner)
        .with_subtitle("Where should AzOffice be installed?")
        .with_icon("install_desktop")
        .with_validation(reason)
        .with_page(destination.dom())
        .with_on_event(data.clone(), on_wizard)
        .with_theme(theme)
        .dom();

    // The macOS installer's side panel around the license page.
    let license = WizardLicensePage::create(
        "Permission is hereby granted, free of charge.\n\nThe software is provided as is.",
    )
    .with_accepted(d.accepted)
    .with_on_event(data.clone(), on_page)
    .with_theme(theme);
    let reason = license.blocked_reason();
    let side = WizardLayout::create("AzOffice Setup", steps)
        .with_current_step(1)
        .with_style(WizardLayoutStyle::SidePanel)
        .with_icon("install_desktop")
        .with_validation(reason)
        .with_page(license.dom())
        .with_on_event(data.clone(), on_wizard)
        .with_theme(theme)
        .dom();

    let components = d
        .components
        .clone()
        .with_on_event(data.clone(), on_page)
        .with_theme(theme)
        .dom();
    let options = d
        .options
        .clone()
        .with_on_event(data.clone(), on_page)
        .with_theme(theme)
        .dom();
    let summary = WizardSummaryPage::create(StringPairVec::from_vec(Vec::new()))
        .with_row("Destination folder", d.path.clone())
        .with_row(
            "Components",
            "AzOffice core, AzWriter, AzSheets",
        )
        .with_theme(theme)
        .dom();
    let progress = WizardProgressPage::create(64.0)
        .with_status("Installing AzOffice...")
        .with_current_item("Copying azwriter/part-07.bin")
        .with_log(StringVec::from_vec(strs(&[
            "Created /opt/AzOffice",
            "Copied azcore/part-01.bin",
        ])))
        .with_show_log(d.show_log)
        .with_on_event(data.clone(), on_page)
        .with_theme(theme)
        .dom();
    let welcome = WizardWelcomePage::create(
        "Welcome to the AzOffice Setup Wizard",
        "This will install AzOffice on your computer.",
    )
    .with_logo("install_desktop")
    .with_theme(theme)
    .dom();
    let finish = WizardFinishPage::create(
        "Completing the AzOffice Setup Wizard",
        "Setup has installed AzOffice.",
    )
    .with_options(WizardOptionVec::from_vec(vec![WizardOption::create(
        "Launch AzOffice now",
        true,
    )]))
    .with_theme(theme)
    .dom();

    let path = PathInput::create(d.path.clone())
        .with_accessibility_name("Destination folder")
        .with_on_change(data.clone(), on_path)
        .with_theme(theme)
        .dom();
    let recorder = d
        .recorder
        .clone()
        .with_on_event(data.clone(), on_recorder)
        .with_theme(theme)
        .dom();
    let settings = d
        .settings
        .clone()
        .with_on_event(data.clone(), on_settings)
        .with_theme(theme)
        .dom();

    let message = MessageBox::create(
        MessageBoxKind::Warning,
        "Replace the existing file?",
        "A file named Report.docx already exists in this folder.",
    )
    .with_buttons(
        StringVec::from_vec(strs(&["Replace", "Keep both", "Cancel"])),
        2,
    )
    .with_dont_ask("Don't ask again", d.dont_ask)
    .with_on_event(data.clone(), on_message)
    .with_theme(theme)
    .dom();
    let about = AboutDialog::create("AzOffice", "Version 1.0.0")
        .with_icon("apps")
        .with_copyright("Copyright 2026 the azul contributors")
        .with_credit("azul", "MIT")
        .with_credit("Material Icons", "Apache-2.0")
        .with_on_event(data.clone(), on_dialog)
        .with_theme(theme)
        .dom();
    let copying = ProgressDialog::create("Copying 12 files", 0.0)
        .with_indeterminate(true)
        .with_detail("Counting the files...")
        .with_on_event(data.clone(), on_dialog)
        .with_theme(theme)
        .dom();
    let login = LoginDialog::create("Sign in to AzOffice")
        .with_credentials(d.user.clone(), d.password.clone())
        .with_remember("Remember me", d.remember)
        .with_on_event(data.clone(), on_login)
        .with_theme(theme)
        .dom();
    let find = FindReplaceDialog::create(d.find.clone())
        .with_replace("Azul")
        .with_options(d.match_case, d.whole_word)
        .with_status("3 of 12")
        .with_on_event(data.clone(), on_find)
        .with_theme(theme)
        .dom();

    section(
        "Dialogs",
        vec![
            captioned(
                "WizardLayout: the banner (Windows) and the side panel (macOS), Next held with a reason",
                side_by_side(boxed(banner), boxed(side)),
            ),
            captioned("Wizard pages: welcome, components", side_by_side(boxed(welcome), boxed(components))),
            captioned("Wizard pages: options, summary", side_by_side(boxed(options), boxed(summary))),
            captioned("Wizard pages: progress, finish", side_by_side(boxed(progress), boxed(finish))),
            captioned("PathInput", path),
            captioned("ShortcutRecorder", Dom::create_div().with_css("width: 220px;").with_child(recorder)),
            captioned(
                "ShellSettingsDialog",
                Dom::create_div()
                    .with_css("display: flex; flex-direction: column; height: 380px; border: 1px solid system:separator;")
                    .with_child(settings),
            ),
            captioned("MessageBox, AboutDialog", side_by_side(boxed(message), boxed(about))),
            captioned("ProgressDialog, LoginDialog", side_by_side(boxed(copying), boxed(login))),
            captioned("FindReplaceDialog", boxed(find)),
            Dom::create_p_with_text(format!("Last reported: {}", d.status.as_str()).as_str()).with_css(NOTE_CSS),
        ],
    )
}

extern "C" fn on_wizard(mut data: RefAny, _: CallbackInfo, event: WizardEvent) -> Update {
    keep(&mut data, |d| {
        d.status = format!("Wizard: {:?} on step {}", event.kind, event.step)
            .as_str()
            .into();
    })
}

extern "C" fn on_page(mut data: RefAny, _: CallbackInfo, event: WizardPageEvent) -> Update {
    keep(&mut data, |d| {
        match event.kind {
            WizardPageEventKind::LicenseAccepted => d.accepted = event.checked,
            WizardPageEventKind::PathChanged => d.path = event.text.clone(),
            WizardPageEventKind::ComponentToggled => {
                d.components.toggle(event.index, event.checked)
            }
            WizardPageEventKind::OptionToggled => d.options.choose(event.index, event.checked),
            WizardPageEventKind::DetailsToggled => d.show_log = event.checked,
        }
        d.status = format!("Page: {:?} {} {}", event.kind, event.index, event.checked)
            .as_str()
            .into();
    })
}

extern "C" fn on_path(mut data: RefAny, _: CallbackInfo, path: AzString) -> Update {
    keep(&mut data, |d| {
        d.status = format!("Path: {}", path.as_str()).as_str().into();
        d.path = path;
    })
}

extern "C" fn on_recorder(
    mut data: RefAny,
    _: CallbackInfo,
    event: ShortcutRecorderEvent,
) -> Update {
    keep(&mut data, |d| {
        d.recorder.apply(event);
        d.status = format!(
            "Shortcut: {:?} {}",
            event.kind,
            d.recorder.display_text().as_str()
        )
        .as_str()
        .into();
    })
}

extern "C" fn on_settings(mut data: RefAny, _: CallbackInfo, event: ShellSettingsEvent) -> Update {
    keep(&mut data, |d| {
        d.status = format!("Settings: {:?} {}", event.kind, event.index)
            .as_str()
            .into();
        d.settings.apply_event(event);
    })
}

/// The About box and the progress dialog: their buttons only report.
extern "C" fn on_dialog(mut data: RefAny, _: CallbackInfo, event: StandardDialogEvent) -> Update {
    keep(&mut data, |d| {
        d.status = format!("Dialog: {:?} {}", event.kind, event.index)
            .as_str()
            .into();
    })
}

extern "C" fn on_message(mut data: RefAny, _: CallbackInfo, event: StandardDialogEvent) -> Update {
    keep(&mut data, |d| {
        if event.kind == StandardDialogEventKind::DontAskAgain {
            d.dont_ask = event.checked;
        }
        d.status = format!(
            "Message box: {:?} {} {}",
            event.kind, event.index, event.checked
        )
        .as_str()
        .into();
    })
}

/// The login dialog keeps nothing itself: the demo holds the two texts (an
/// app would hand them to its keyring or server and clear the password).
extern "C" fn on_login(mut data: RefAny, _: CallbackInfo, event: StandardDialogEvent) -> Update {
    keep(&mut data, |d| {
        match (event.kind, event.index) {
            (StandardDialogEventKind::FieldChanged, 0) => d.user = event.text.clone(),
            (StandardDialogEventKind::FieldChanged, _) => d.password = event.text.clone(),
            (StandardDialogEventKind::OptionToggled, _) => d.remember = event.checked,
            (StandardDialogEventKind::Submit, _) => d.password = "".into(),
            _ => {}
        }
        d.status = format!("Login: {:?}", event.kind).as_str().into();
    })
}

extern "C" fn on_find(mut data: RefAny, _: CallbackInfo, event: StandardDialogEvent) -> Update {
    keep(&mut data, |d| {
        match (event.kind, event.index) {
            (StandardDialogEventKind::FieldChanged, 0) => d.find = event.text.clone(),
            (StandardDialogEventKind::OptionToggled, 0) => d.match_case = event.checked,
            (StandardDialogEventKind::OptionToggled, _) => d.whole_word = event.checked,
            _ => {}
        }
        d.status = format!("Find: {:?} {}", event.kind, event.index)
            .as_str()
            .into();
    })
}
