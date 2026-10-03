//! AzSetup: a complete fake installer for "AzOffice" on the public azul API.
//!
//! The window is the utility shell (S9) with azul's `Titlebar` as its title
//! row (`WindowDecorations::NoTitle`) and the `WizardLayout` as its content,
//! at the classic wizard's size. The pages are the toolkit's reusable wizard
//! pages: welcome, license ("I accept" holds Next until ticked), destination
//! (the folder with Browse, the space the components need and the space the
//! drive has, a warning when it is short), components (a checklist tree, the
//! total follows every tick), options (checkboxes and a radio set), the
//! summary, the progress (a timer "copies" fake files; the log behind "Show
//! details") and finish ("Launch AzOffice now", "Open the AzOffice
//! settings"). The welcome and finish pages take the side panel, the others
//! the Windows installer's banner (`--frame` picks another).
//!
//! Cancel asks first (a `MessageBox` in a `Modal`); F1 opens the About box.
//! Finish with "Open the AzOffice settings" ticked opens the settings window
//! (`ShellSettingsDialog`: every kind of setting, the search across the
//! categories, Apply / OK / Cancel - or instant on macOS - and the theme and
//! mode applied from "Appearance"). It installs nothing and writes nothing.
//!
//! On stdout, for scripts (`scripts/azsetup_e2e.py`): `AZSETUP_STEP <i>
//! <label>` on every step, `AZSETUP_TOTAL <bytes>` when the components
//! change, `AZSETUP_PATH <path>` when the folder changes,
//! `AZSETUP_PROGRESS <percent>` while copying, `AZSETUP_DONE <bytes>` at the end of
//! it, `AZSETUP_FINISHED launch=<bool> settings=<bool>` on Finish,
//! `AZSETUP_SETTINGS <event>` from the settings window.

pub mod model;

use azul::{
    app::{GlobalHotkey, HotkeyModifiers},
    css::DarkLightMode,
    dom::VirtualKeyCode,
    file::FilePath,
    option::OptionDarkLightMode,
    prelude::*,
    shells::{
        ShellSetting, ShellSettingChoice, ShellSettingNumber, ShellSettingShortcut,
        ShellSettingValue, ShellSettingsApplyMode, ShellSettingsDialog, ShellSettingsEvent,
        ShellSettingsEventKind, ShellThemeAccent, ShellThemeScope, UtilityShell,
    },
    str::String as AzString,
    time::SystemTimeDiff,
    vec::{ShellSettingVec, StringPairVec, StringVec, WizardComponentVec, WizardOptionVec},
    widgets::{
        AboutDialog, MessageBox, MessageBoxKind, Modal, ModalState, StandardDialogEvent,
        StandardDialogEventKind, WizardComponent, WizardComponentsPage,
        WizardDestinationPage, WizardEvent, WizardEventKind, WizardFinishPage, WizardLayout,
        WizardLayoutSize, WizardLayoutStyle, WizardLicensePage, WizardOption, WizardOptionsPage,
        WizardPageEvent, WizardPageEventKind, WizardProgressPage, WizardSummaryPage,
        WizardWelcomePage,
    },
    window::WindowDecorations,
};

use azul_appkit::{
    about::AboutInfo,
    args::{AppArgs, AppSpec, ModePref, Theme},
    settings::AppSettings,
    shortcuts::Shortcut,
    ui as kit,
};

use crate::model::{Copier, Frame, Screen, Step, COMPONENTS, LICENSE, MB, SCREENS, STEPS};

/// What azul-appkit's switches know about AzSetup (`--screen` names a
/// wizard step, the settings or the About box).
pub const SPEC: AppSpec = AppSpec {
    name: "AzSetup",
    binary: "AzSetup",
    summary: "a fake installer for AzOffice (installs nothing)",
    screens: &SCREENS,
    files_help: "",
};

/// The About facts (the About box, F1).
pub const ABOUT: AboutInfo = AboutInfo {
    name: "AzSetup",
    version: env!("CARGO_PKG_VERSION"),
    summary: "A demonstration of azul's install wizard pages, settings dialog and standard \
              dialogs. It installs nothing.",
    license: "MIT",
    app_folder: "setup",
};

/// The keys AzSetup answers.
pub const SHORTCUTS: [Shortcut; 3] = [
    Shortcut::new("Setup", "F1", "About AzSetup"),
    Shortcut::new("Setup", "Escape", "Close the About box, or ask to exit Setup"),
    Shortcut::new("Setup", "Tab", "Move between the page's controls and the buttons"),
];

// ==== State ====

/// Everything the window shows.
struct Setup {
    step: Step,
    frame: Frame,
    accepted: bool,
    path: String,
    /// The free bytes on the folder's drive, when known.
    available: Option<u64>,
    /// The components and their ticks (the page holds the rule).
    components: WizardComponentsPage,
    options: WizardOptionsPage,
    finish: WizardFinishPage,
    copier: Copier,
    show_log: bool,
    confirm_cancel: bool,
    about_open: bool,
    settings: ShellSettingsDialog,
    /// azul-appkit's kit: the switches, the data root, settings.json.
    kit: RefAny,
}

impl Setup {
    /// Opens / closes the About box and the exit question (stdout
    /// `AZSETUP_BOXES about=<bool> question=<bool>`, for scripts).
    fn set_boxes(&mut self, about: bool, question: bool) {
        self.about_open = about;
        self.confirm_cancel = question;
        println!("AZSETUP_BOXES about={about} question={question}");
    }
}

fn s(text: &str) -> AzString {
    AzString::from(text)
}

fn strs(items: &[&str]) -> StringVec {
    StringVec::from_vec(items.iter().map(|t| s(t)).collect())
}

/// `<home>/AzOffice`, or a folder next to the program.
fn default_folder() -> String {
    let home = FilePath::get_home_dir()
        .into_option()
        .map(|p| p.inner.as_str().to_string())
        .unwrap_or_else(|| ".".to_string());
    format!("{home}/AzOffice")
}

/// The free bytes on the drive the folder would be created on.
fn free_space(path: &str) -> Option<u64> {
    FilePath::create(s(path))
        .disk_space_for_new()
        .into_option()
        .map(|d| d.free)
}

fn components_page() -> WizardComponentsPage {
    let rows: Vec<WizardComponent> = COMPONENTS
        .iter()
        .map(|(label, mb, depth, checked, required, description)| {
            WizardComponent::create(s(label), mb * MB)
                .with_depth(*depth)
                .with_checked(*checked)
                .with_required(*required)
                .with_description(s(description))
        })
        .collect();
    WizardComponentsPage::create(WizardComponentVec::from_vec(rows)).with_intro(s(
        "Select the components you want to install; clear the ones you do not.",
    ))
}

fn options_page() -> WizardOptionsPage {
    WizardOptionsPage::create(WizardOptionVec::from_vec(vec![
        WizardOption::create(s("Create a desktop shortcut"), true),
        WizardOption::create(s("Add AzOffice to the PATH"), false)
            .with_description(s("So a terminal finds azwriter, azsheets and azslides.")),
        WizardOption::create(s("Just for me"), true).with_group(1),
        WizardOption::create(s("For everyone on this computer"), false)
            .with_group(1)
            .with_description(s("Needs administrator rights.")),
    ]))
}

fn finish_page() -> WizardFinishPage {
    WizardFinishPage::create(
        s("Completing the AzOffice Setup Wizard"),
        s(
            "Setup has finished installing AzOffice on your computer.\n\nThe programs are in your \
           applications folder.",
        ),
    )
    .with_logo(s("check_circle"))
    .with_options(WizardOptionVec::from_vec(vec![
        WizardOption::create(s("Launch AzOffice now"), true),
        WizardOption::create(s("Open the AzOffice settings"), false),
    ]))
}

/// The key a setting's applied value is kept under in settings.json.
fn setting_key(id: &str) -> String {
    format!("setting.{id}")
}

/// A setting's value as settings.json keeps it (`None`: not kept - a
/// shortcut, which the demo does not remember).
fn stored(value: &ShellSettingValue) -> Option<String> {
    match value {
        ShellSettingValue::Toggle(b) => Some(b.to_string()),
        ShellSettingValue::Choice(c) | ShellSettingValue::Radio(c) => Some(c.selected.to_string()),
        ShellSettingValue::Number(n) | ShellSettingValue::Slider(n) => Some(n.value.to_string()),
        ShellSettingValue::Text(t) | ShellSettingValue::Path(t) => Some(t.as_str().to_string()),
        ShellSettingValue::Color(c) => Some(c.to_hex().as_str().to_string()),
        ShellSettingValue::Shortcut(_) => None,
    }
}

/// `text` from settings.json read back as a value of `like`'s kind, inside
/// its range (`None`: unreadable, the default stays).
fn restored(like: &ShellSettingValue, text: &str) -> Option<ShellSettingValue> {
    let choice = |c: &ShellSettingChoice| {
        text.trim()
            .parse::<usize>()
            .ok()
            .filter(|i| *i < c.options.as_ref().len())
            .map(|i| ShellSettingChoice::create(c.options.clone(), i))
    };
    let number = |n: &ShellSettingNumber| {
        text.trim()
            .parse::<f32>()
            .ok()
            .filter(|v| v.is_finite() && *v >= n.min && *v <= n.max)
            .map(|v| {
                let mut m = n.clone();
                m.value = v;
                m
            })
    };
    match like {
        ShellSettingValue::Toggle(_) => text.trim().parse::<bool>().ok().map(ShellSettingValue::Toggle),
        ShellSettingValue::Choice(c) => choice(c).map(ShellSettingValue::Choice),
        ShellSettingValue::Radio(c) => choice(c).map(ShellSettingValue::Radio),
        ShellSettingValue::Number(n) => number(n).map(ShellSettingValue::Number),
        ShellSettingValue::Slider(n) => number(n).map(ShellSettingValue::Slider),
        ShellSettingValue::Text(_) => Some(ShellSettingValue::Text(s(text))),
        ShellSettingValue::Path(_) => Some(ShellSettingValue::Path(s(text))),
        ShellSettingValue::Color(_) => ColorU::parse_hex(s(text)).into_option().map(ShellSettingValue::Color),
        ShellSettingValue::Shortcut(_) => None,
    }
}

/// The values settings.json remembers, laid over the table's defaults: the
/// appearance from the kit's theme and mode, every other setting from its
/// `setting.<id>` key.
fn with_remembered(
    mut settings: Vec<ShellSetting>,
    theme: Theme,
    mode: ModePref,
    saved: &AppSettings,
) -> Vec<ShellSetting> {
    for setting in &mut settings {
        let id = setting.id.as_str().to_string();
        let value = match id.as_str() {
            "appearance.theme" => restored(&setting.value, if theme == Theme::Flora { "1" } else { "0" }),
            "appearance.mode" => restored(
                &setting.value,
                match mode {
                    ModePref::Light => "0",
                    ModePref::Dark => "1",
                    ModePref::System => "2",
                },
            ),
            _ => saved
                .get(&setting_key(&id))
                .and_then(|text| restored(&setting.value, text)),
        };
        if let Some(v) = value {
            setting.value = v.clone();
            setting.applied = v;
        }
    }
    settings
}

/// Keeps the values in effect in settings.json (one write, on a Thread):
/// the appearance as the kit's theme and mode (the next start opens in
/// them), every other setting under `setting.<id>`.
fn remember(st: &Setup, info: &mut CallbackInfo) {
    {
        let mut kit_ref = st.kit.clone();
        let Some(mut k) = kit_ref.downcast_mut::<kit::Kit>() else {
            return;
        };
        for setting in st.settings.settings.as_ref() {
            let id = setting.id.as_str();
            match id {
                "appearance.theme" => {
                    k.settings.theme =
                        if setting.applied.as_index() == 1 { Theme::Flora } else { Theme::Flat };
                    k.args.theme = None;
                }
                "appearance.mode" => {
                    k.settings.mode = match setting.applied.as_index() {
                        0 => ModePref::Light,
                        1 => ModePref::Dark,
                        _ => ModePref::System,
                    };
                    k.args.mode = None;
                }
                _ => {
                    if let Some(text) = stored(&setting.applied) {
                        k.settings.set(&setting_key(id), &text);
                    }
                }
            }
        }
    }
    kit::save_settings(&st.kit, info);
}

/// The settings window's table: every kind of setting.
fn settings_dialog(theme: Theme, mode: ModePref, saved: &AppSettings) -> ShellSettingsDialog {
    let shortcut = |ctrl: bool, shift: bool, key: VirtualKeyCode| {
        ShellSettingValue::Shortcut(ShellSettingShortcut::create(GlobalHotkey::create(
            HotkeyModifiers {
                ctrl,
                alt: false,
                shift,
                meta: false,
            },
            key,
        )))
    };
    let documents = FilePath::get_document_dir()
        .into_option()
        .map(|p| p.inner.as_str().to_string())
        .unwrap_or_else(|| ".".to_string());
    let settings = vec![
        ShellSetting::create(
            s("general.reopen"),
            s("Reopen the last documents"),
            0,
            s("Startup"),
            ShellSettingValue::Toggle(true),
        )
        .with_help(s("Open what was open when AzOffice closed.")),
        ShellSetting::create(
            s("general.start"),
            s("Start screen"),
            0,
            s("Startup"),
            ShellSettingValue::Choice(ShellSettingChoice::create(
                strs(&["Recent documents", "Blank document", "Templates"]),
                0,
            )),
        ),
        ShellSetting::create(
            s("general.folder"),
            s("Documents folder"),
            0,
            s("Files"),
            ShellSettingValue::Path(s(&documents)),
        )
        .with_help(s("Where Save puts a new document.")),
        ShellSetting::create(
            s("general.autosave"),
            s("Autosave every"),
            0,
            s("Files"),
            ShellSettingValue::Number(
                ShellSettingNumber::create(10.0, 1.0, 120.0).with_unit(s("min")),
            ),
        ),
        ShellSetting::create(
            s("general.language"),
            s("Interface language"),
            0,
            s("Language"),
            ShellSettingValue::Choice(ShellSettingChoice::create(
                strs(&["English", "Deutsch", "Francais"]),
                0,
            )),
        )
        .with_requires_restart(true)
        .with_keywords(s("locale translation")),
        ShellSetting::create(
            s("editing.author"),
            s("Author name"),
            1,
            s("Text"),
            ShellSettingValue::Text(s("")),
        )
        .with_help(s("Written into every new document's properties.")),
        ShellSetting::create(
            s("editing.font_size"),
            s("Default font size"),
            1,
            s("Text"),
            ShellSettingValue::Number(
                ShellSettingNumber::create(11.0, 6.0, 72.0).with_unit(s("pt")),
            ),
        )
        .with_keywords(s("zoom text")),
        ShellSetting::create(
            s("editing.palette"),
            s("Command palette"),
            1,
            s("Keyboard"),
            shortcut(true, true, VirtualKeyCode::P),
        ),
        ShellSetting::create(
            s("editing.quick_open"),
            s("Quick open"),
            1,
            s("Keyboard"),
            shortcut(true, false, VirtualKeyCode::P),
        ),
        ShellSetting::create(
            s("appearance.theme"),
            s("Theme"),
            2,
            s("Theme"),
            ShellSettingValue::Choice(ShellSettingChoice::create(
                strs(&["Flat", "Flora"]),
                0,
            )),
        )
        .with_default(ShellSettingValue::Choice(ShellSettingChoice::create(
            strs(&["Flat", "Flora"]),
            0,
        ))),
        ShellSetting::create(
            s("appearance.mode"),
            s("Mode"),
            2,
            s("Theme"),
            ShellSettingValue::Radio(ShellSettingChoice::create(
                strs(&["Light", "Dark", "System"]),
                2,
            )),
        )
        .with_keywords(s("dark light night")),
        ShellSetting::create(
            s("appearance.accent"),
            s("Accent colour"),
            2,
            s("Colours"),
            ShellSettingValue::Color(ColorU::rgba(47, 74, 133, 255)),
        ),
        ShellSetting::create(
            s("appearance.zoom"),
            s("Interface zoom"),
            2,
            s("Size"),
            ShellSettingValue::Slider(
                ShellSettingNumber::create(100.0, 50.0, 200.0).with_unit(s("%")),
            ),
        ),
        ShellSetting::create(
            s("advanced.gpu"),
            s("Hardware acceleration"),
            3,
            s("Performance"),
            ShellSettingValue::Toggle(true),
        )
        .with_requires_restart(true)
        .with_help(s("Draw with the graphics card.")),
        ShellSetting::create(
            s("advanced.telemetry"),
            s("Send usage statistics"),
            3,
            s("Privacy"),
            ShellSettingValue::Toggle(false),
        )
        .with_help(s("AzOffice sends nothing; this is a demonstration.")),
    ];
    ShellSettingsDialog::create(strs(&["General", "Editing", "Appearance", "Advanced"]))
        .with_category_icons(strs(&["tune", "edit", "palette", "build"]))
        .with_settings(ShellSettingVec::from_vec(with_remembered(settings, theme, mode, saved)))
        .with_apply_mode(ShellSettingsApplyMode::platform())
}

fn announce(step: Step) {
    println!("{}", model::step_line(step));
}

// ==== The wizard ====

/// The current page and the reason Next is held on it (empty: it may go).
fn page(s_: &Setup, app: &RefAny) -> (Dom, AzString) {
    match s_.step {
        Step::Welcome => (
            WizardWelcomePage::create(
                s("Welcome to the AzOffice Setup Wizard"),
                s("This will install AzOffice 1.0 on your computer.\n\nIt is recommended that you \
                   close all other applications before continuing.\n\nClick Next to continue, or \
                   Cancel to exit Setup."),
            )
            .with_logo(s("install_desktop"))
            .dom(),
            s(""),
        ),
        Step::License => {
            let p = WizardLicensePage::create(s(LICENSE))
                .with_accepted(s_.accepted)
                .with_on_event(app.clone(), on_page);
            let reason = p.blocked_reason();
            (p.dom(), reason)
        }
        Step::Destination => {
            let mut p = WizardDestinationPage::create(s(&s_.path), s_.components.total_bytes())
                .with_intro(s("Setup will install AzOffice into the following folder. To use \
                               another folder, type it or click Browse."))
                .with_on_event(app.clone(), on_page);
            if let Some(free) = s_.available {
                p = p.with_available(free);
            }
            let reason = p.blocked_reason();
            (p.dom(), reason)
        }
        Step::Components => {
            let p = s_.components.clone().with_on_event(app.clone(), on_page);
            let reason = p.blocked_reason();
            (p.dom(), reason)
        }
        Step::Options => (s_.options.clone().with_on_event(app.clone(), on_page).dom(), s("")),
        Step::Ready => {
            let ticked: Vec<String> = s_
                .components
                .components
                .as_ref()
                .iter()
                .filter(|c| c.checked && c.size_bytes > 0)
                .map(|c| c.label.as_str().to_string())
                .collect();
            let tasks: Vec<String> = s_
                .options
                .options
                .as_ref()
                .iter()
                .filter(|o| o.checked)
                .map(|o| o.label.as_str().to_string())
                .collect();
            let p = WizardSummaryPage::create(StringPairVec::from_vec(Vec::new()))
                .with_intro(s("Setup is ready to copy AzOffice onto this computer. Review the \
                               choices below; go back to change one."))
                .with_row(s("Destination folder"), s(&s_.path))
                .with_row(s("Components"), s(&ticked.join(", ")))
                .with_row(s("Additional tasks"), s(&tasks.join(", ")));
            (p.dom(), s(""))
        }
        Step::Installing => {
            let done = s_.copier.is_done();
            let p = WizardProgressPage::create(s_.copier.percent())
                .with_status(s(if done {
                    "Installation complete."
                } else {
                    "Installing AzOffice..."
                }))
                .with_current_item(s(&s_.copier.current))
                .with_log(StringVec::from_vec(s_.copier.log.iter().map(|l| s(l)).collect()))
                .with_show_log(s_.show_log)
                .with_on_event(app.clone(), on_page);
            (p.dom(), s(if done { "" } else { "Copying files..." }))
        }
        Step::Finish => (s_.finish.clone().with_on_event(app.clone(), on_page).dom(), s("")),
    }
}

fn wizard(s_: &Setup, app: &RefAny) -> Dom {
    let (page, reason) = page(s_, app);
    let style = match s_.frame {
        Frame::Installer if s_.step.is_outer() => WizardLayoutStyle::SidePanel,
        Frame::Installer => WizardLayoutStyle::Banner,
        Frame::Rail => WizardLayoutStyle::Rail,
        Frame::Side => WizardLayoutStyle::SidePanel,
    };
    let labels: Vec<AzString> = STEPS.iter().map(|st| s(st.label())).collect();
    WizardLayout::create(s("AzOffice Setup"), StringVec::from_vec(labels))
        .with_current_step(s_.step.index())
        .with_style(style)
        .with_subtitle(s(s_.step.subtitle()))
        .with_icon(s("install_desktop"))
        .with_labels(
            s("< Back"),
            s(if s_.step == Step::Ready {
                "Install"
            } else {
                "Next >"
            }),
            s("Finish"),
            s(if s_.step == Step::Finish {
                ""
            } else {
                "Cancel"
            }),
        )
        .with_validation(reason)
        .with_can_go_back(s_.step.can_go_back())
        .with_page(page)
        .with_on_event(app.clone(), on_wizard)
        .dom()
}

/// The cancel question and the About box: modals the window always
/// carries, open while the app says so.
fn modals(s_: &Setup, app: &RefAny) -> Vec<Dom> {
    let question = MessageBox::create(
        MessageBoxKind::Question,
        s("Exit Setup?"),
        s(
            "Setup is not complete. If you exit now, AzOffice will not be installed.\n\nYou may \
           run Setup again at another time.",
        ),
    )
    .with_buttons(strs(&["Yes", "No"]), 1)
    .with_on_event(app.clone(), on_confirm)
    .dom();
    let about = AboutDialog::create(s("AzOffice Setup"), s("Version 1.0.0"))
        .with_icon(s("install_desktop"))
        .with_description(s(
            "A demonstration of azul's install wizard pages, settings dialog \
                             and standard dialogs. It installs nothing.",
        ))
        .with_copyright(s("Copyright 2026 the azul contributors"))
        .with_credit(s("azul"), s("MIT"))
        .with_credit(s("Material Icons"), s("Apache-2.0"))
        .with_on_event(app.clone(), on_about)
        .dom();
    vec![
        Modal::create(question)
            .with_title(s("AzOffice Setup"))
            .with_open(s_.confirm_cancel)
            .with_on_close(app.clone(), on_modal_close)
            .dom(),
        Modal::create(about)
            .with_title(s("About AzOffice Setup"))
            .with_open(s_.about_open)
            .with_on_close(app.clone(), on_modal_close)
            .dom(),
    ]
}

/// The window's root: the theme scope over the utility shell (the title row
/// above the content), the modals; the wizard's keys (`wizard`).
fn window_root(content: Dom, title: &str, app: &RefAny, extra: Vec<Dom>, wizard: bool) -> Dom {
    let shell = UtilityShell::create(content)
        .with_title_row(kit::title_row(title))
        .with_label(s(title))
        .dom();
    let mut column = Dom::create_div()
        .with_css("display: flex; flex-direction: column; flex-grow: 1; min-height: 0px;")
        .with_child(shell);
    for m in extra {
        column.add_child(m);
    }
    // The scope as the window's body: no UA margin, the full window height
    // (the wizard's buttons stay in the window).
    let body = ShellThemeScope::create(column)
        .with_accent(ShellThemeAccent::Blue)
        .body();
    if wizard {
        body.with_callback(
            EventFilter::Window(WindowEventFilter::VirtualKeyDown),
            app.clone(),
            on_key,
        )
    } else {
        body
    }
}

extern "C" fn layout(mut data: RefAny, info: LayoutCallbackInfo) -> Dom {
    // Reading the mode makes a light / dark switch rebuild the window.
    let _mode = info.get_mode();
    let app = data.clone();
    let Some(guard) = data.downcast_ref::<Setup>() else {
        return Dom::create_body();
    };
    window_root(
        wizard(&guard, &app),
        "AzOffice Setup",
        &app,
        modals(&guard, &app),
        true,
    )
}

extern "C" fn settings_layout(mut data: RefAny, info: LayoutCallbackInfo) -> Dom {
    let _mode = info.get_mode();
    let app = data.clone();
    let Some(guard) = data.downcast_ref::<Setup>() else {
        return Dom::create_body();
    };
    let dialog = guard
        .settings
        .clone()
        .with_on_event(app.clone(), on_settings)
        .dom();
    window_root(dialog, "AzOffice Settings", &app, Vec::new(), false)
}

// ==== Callbacks ====

/// The ticked components' `(label, bytes)`.
fn ticked(components: &WizardComponentsPage) -> Vec<(String, u64)> {
    components
        .components
        .as_ref()
        .iter()
        .filter(|c| c.checked)
        .map(|c| (c.label.as_str().to_string(), c.size_bytes))
        .collect()
}

extern "C" fn on_wizard(mut data: RefAny, mut info: CallbackInfo, event: WizardEvent) -> Update {
    let app = data.clone();
    let Some(mut st) = data.downcast_mut::<Setup>() else {
        return Update::DoNothing;
    };
    match event.kind {
        WizardEventKind::Next => {
            match st.step {
                Step::Ready => {
                    st.copier = Copier::plan(&st.path, &ticked(&st.components));
                    st.step = Step::Installing;
                    let get_time = info.get_system_time_fn();
                    info.add_timer(
                        TimerId::unique(),
                        Timer::create(app, on_tick, get_time)
                            .with_interval(Duration::System(SystemTimeDiff::from_millis(60))),
                    );
                }
                Step::Installing if !st.copier.is_done() => return Update::DoNothing,
                step => st.step = step.next(),
            }
            if st.step == Step::Destination && st.available.is_none() {
                st.available = free_space(&st.path);
            }
            announce(st.step);
        }
        WizardEventKind::Back => {
            st.step = st.step.back();
            announce(st.step);
        }
        WizardEventKind::Finish => {
            let opts = st.finish.options.as_ref();
            let launch = opts.first().is_some_and(|o| o.checked);
            let settings = opts.get(1).is_some_and(|o| o.checked);
            println!("AZSETUP_FINISHED launch={launch} settings={settings}");
            if settings {
                info.create_window(settings_window());
            }
            info.close_window();
            return Update::DoNothing;
        }
        WizardEventKind::Cancel => {
            let about = st.about_open;
            st.set_boxes(about, true);
        }
        WizardEventKind::Step => {
            // The rail goes back to a step already passed, never forward.
            if event.step < st.step.index() && st.step.can_go_back() {
                st.step = Step::at(event.step);
                announce(st.step);
            }
        }
    }
    Update::RefreshDom
}

extern "C" fn on_page(mut data: RefAny, _info: CallbackInfo, event: WizardPageEvent) -> Update {
    let Some(mut st) = data.downcast_mut::<Setup>() else {
        return Update::DoNothing;
    };
    match event.kind {
        WizardPageEventKind::LicenseAccepted => st.accepted = event.checked,
        WizardPageEventKind::PathChanged => {
            st.path = event.text.as_str().to_string();
            st.available = free_space(&st.path);
            println!("AZSETUP_PATH {}", st.path);
        }
        WizardPageEventKind::ComponentToggled => {
            st.components.toggle(event.index, event.checked);
            println!("{}", model::total_line(st.components.total_bytes()));
        }
        WizardPageEventKind::OptionToggled => {
            if st.step == Step::Finish {
                st.finish.choose(event.index, event.checked);
            } else {
                st.options.choose(event.index, event.checked);
            }
        }
        WizardPageEventKind::DetailsToggled => st.show_log = event.checked,
    }
    Update::RefreshDom
}

extern "C" fn on_tick(mut data: RefAny, _info: TimerCallbackInfo) -> TimerCallbackReturn {
    let Some(mut st) = data.downcast_mut::<Setup>() else {
        return TimerCallbackReturn::terminate_unchanged();
    };
    let done = st.copier.tick(3);
    println!("{}", model::progress_line(st.copier.percent()));
    if done {
        println!("AZSETUP_DONE {}", st.copier.done_bytes);
        TimerCallbackReturn::terminate_and_refresh_dom()
    } else {
        TimerCallbackReturn::continue_and_refresh_dom()
    }
}

extern "C" fn on_confirm(
    mut data: RefAny,
    mut info: CallbackInfo,
    event: StandardDialogEvent,
) -> Update {
    let Some(mut st) = data.downcast_mut::<Setup>() else {
        return Update::DoNothing;
    };
    match (event.kind, event.index) {
        (StandardDialogEventKind::Button, 0) => {
            println!("AZSETUP_CANCELLED");
            info.close_window();
            Update::DoNothing
        }
        _ => {
            let about = st.about_open;
            st.set_boxes(about, false);
            Update::RefreshDom
        }
    }
}

extern "C" fn on_about(
    mut data: RefAny,
    _info: CallbackInfo,
    _event: StandardDialogEvent,
) -> Update {
    let Some(mut st) = data.downcast_mut::<Setup>() else {
        return Update::DoNothing;
    };
    let question = st.confirm_cancel;
    st.set_boxes(false, question);
    Update::RefreshDom
}

extern "C" fn on_modal_close(mut data: RefAny, _info: CallbackInfo, _state: ModalState) -> Update {
    let Some(mut st) = data.downcast_mut::<Setup>() else {
        return Update::DoNothing;
    };
    st.set_boxes(false, false);
    Update::RefreshDom
}

/// The wizard's keys: F1 opens the About box; Escape closes it (or the
/// exit question), else asks to exit Setup - an installer's Cancel.
extern "C" fn on_key(mut data: RefAny, mut info: CallbackInfo) -> Update {
    let key = info
        .get_current_keyboard_state()
        .current_virtual_keycode
        .into_option();
    let Some(mut st) = data.downcast_mut::<Setup>() else {
        return Update::DoNothing;
    };
    match key {
        Some(VirtualKeyCode::F1) if !st.about_open => {
            let question = st.confirm_cancel;
            st.set_boxes(true, question);
        }
        Some(VirtualKeyCode::Escape) if st.about_open || st.confirm_cancel => st.set_boxes(false, false),
        Some(VirtualKeyCode::Escape) if st.step != Step::Finish => st.set_boxes(false, true),
        _ => return Update::DoNothing,
    }
    info.prevent_default();
    Update::RefreshDom
}

/// The settings window's requests: the dialog keeps its rule; the
/// appearance takes effect with the values in effect; OK and Cancel close.
extern "C" fn on_settings(
    mut data: RefAny,
    mut info: CallbackInfo,
    event: ShellSettingsEvent,
) -> Update {
    let Some(mut st) = data.downcast_mut::<Setup>() else {
        return Update::DoNothing;
    };
    let kind = event.kind;
    st.settings.apply_event(event);
    let label = match kind {
        ShellSettingsEventKind::Changed => "changed",
        ShellSettingsEventKind::CategoryChosen => "category",
        ShellSettingsEventKind::SearchChanged => "search",
        ShellSettingsEventKind::RestoreDefaults => "restore",
        ShellSettingsEventKind::Apply => "apply",
        ShellSettingsEventKind::Ok => "ok",
        ShellSettingsEventKind::Cancel => "cancel",
        ShellSettingsEventKind::StartRecording => "recording",
        ShellSettingsEventKind::StopRecording => "stop-recording",
    };
    println!("AZSETUP_SETTINGS {label}");
    // The theme and the mode in effect.
    let theme = st
        .settings
        .value_of(s("appearance.theme"))
        .into_option()
        .map(|v| v.as_index());
    let mode = st
        .settings
        .value_of(s("appearance.mode"))
        .into_option()
        .map(|v| v.as_index());
    if matches!(
        kind,
        ShellSettingsEventKind::Apply
            | ShellSettingsEventKind::Ok
            | ShellSettingsEventKind::Changed
    ) {
        if let Some(t) = theme {
            info.set_theme(s(if t == 1 { "flora" } else { "flat" }));
        }
        match mode {
            Some(0) => info.set_mode(OptionDarkLightMode::Some(DarkLightMode::Light)),
            Some(1) => info.set_mode(OptionDarkLightMode::Some(DarkLightMode::Dark)),
            _ => info.set_mode(OptionDarkLightMode::None),
        }
    }
    let instant = matches!(st.settings.apply_mode, ShellSettingsApplyMode::Instant);
    if matches!(kind, ShellSettingsEventKind::Apply | ShellSettingsEventKind::Ok)
        || (instant && matches!(kind, ShellSettingsEventKind::Changed))
    {
        remember(&st, &mut info);
    }
    if matches!(
        kind,
        ShellSettingsEventKind::Ok | ShellSettingsEventKind::Cancel
    ) {
        info.close_window();
        return Update::DoNothing;
    }
    Update::RefreshDom
}

// ==== Entry ====

/// The wizard's window: the classic wizard's size plus the title row
/// (azul-appkit's window: `NoTitle`, `--size`, a minimum size, `--shot`).
fn setup_window(kit_ref: &RefAny) -> WindowCreateOptions {
    let mut window = kit::window_options(
        kit_ref,
        layout,
        (
            WizardLayoutSize::Classic.width(),
            WizardLayoutSize::Classic.height() + 34.0,
        ),
        (WizardLayoutSize::Compact.width(), WizardLayoutSize::Compact.height() + 34.0),
        on_window_created,
    );
    window.window_state.title = s("AzOffice Setup");
    window
}

/// The settings window, opened by Finish ("Open the AzOffice settings").
fn settings_window() -> WindowCreateOptions {
    let mut window = WindowCreateOptions::create(settings_layout);
    window.window_state.size.dimensions = LogicalSize::create(920.0, 640.0);
    window.window_state.title = s("AzOffice Settings");
    window.window_state.flags.decorations = WindowDecorations::NoTitle;
    window
}

/// The settings window as the first window (`--screen settings`).
fn first_settings_window(kit_ref: &RefAny) -> WindowCreateOptions {
    let mut window =
        kit::window_options(kit_ref, settings_layout, (920.0, 640.0), (640.0, 480.0), on_window_created);
    window.window_state.title = s("AzOffice Settings");
    window
}

/// The first window exists: azul-appkit's `--shot` timer.
extern "C" fn on_window_created(mut data: RefAny, mut info: CallbackInfo) -> Update {
    let Some(kit_ref) = data.downcast_ref::<Setup>().map(|st| st.kit.clone()) else {
        return Update::DoNothing;
    };
    kit::on_window_created(&kit_ref, &mut info);
    Update::DoNothing
}

pub fn start() {
    // AzSetup's own switches first (--frame, --step), then azul-appkit's.
    let (own, rest) = match model::split_switches(std::env::args().skip(1)) {
        Ok(split) => split,
        Err(why) => {
            eprintln!("{why}");
            std::process::exit(2);
        }
    };
    let args = match AppArgs::parse(&SPEC, rest) {
        Ok(a) => a,
        Err(why) => {
            eprintln!("{why}");
            std::process::exit(2);
        }
    };
    let (screen, mut step) = model::open_on(args.screen_or_default(&SPEC));
    if let Some(n) = own.step {
        step = Step::at(n);
    }
    let kit_ref = kit::create_kit(SPEC, ABOUT, &SHORTCUTS, &[], args);
    let (theme, mode, saved) = {
        let mut k = kit_ref.clone();
        k.downcast_ref::<kit::Kit>().map_or(
            (Theme::Flat, ModePref::System, AppSettings::default()),
            |k| {
                let (theme, mode) = k.effective();
                (theme, mode, k.settings.clone())
            },
        )
    };
    let path = default_folder();
    let state = Setup {
        step,
        frame: own.frame,
        accepted: step > Step::License,
        available: None,
        components: components_page(),
        options: options_page(),
        finish: finish_page(),
        copier: Copier::default(),
        show_log: false,
        confirm_cancel: false,
        about_open: screen == Screen::About,
        settings: settings_dialog(theme, mode, &saved),
        path,
        kit: kit_ref.clone(),
    };
    announce(state.step);
    let config = kit::app_config(&kit_ref);
    let app = App::create(RefAny::new(state), config);
    let window = if screen == Screen::Settings {
        println!("AZSETUP_SCREEN settings");
        first_settings_window(&kit_ref)
    } else {
        setup_window(&kit_ref)
    };
    app.run(window);
}

#[cfg(test)]
mod remembered_settings_tests {
    use super::*;

    #[test]
    fn every_kind_of_setting_but_a_shortcut_survives_the_trip_through_settings_json() {
        let values = [
            ShellSettingValue::Toggle(false),
            ShellSettingValue::Choice(ShellSettingChoice::create(strs(&["A", "B", "C"]), 2)),
            ShellSettingValue::Radio(ShellSettingChoice::create(strs(&["Light", "Dark"]), 1)),
            ShellSettingValue::Number(ShellSettingNumber::create(42.0, 1.0, 120.0)),
            ShellSettingValue::Slider(ShellSettingNumber::create(150.0, 50.0, 200.0)),
            ShellSettingValue::Text(s("Ada Lovelace")),
            ShellSettingValue::Path(s("/home/ada/Documents")),
            ShellSettingValue::Color(ColorU::rgba(12, 34, 56, 255)),
        ];
        for v in values {
            let text = stored(&v).expect("kept");
            let back = restored(&v, &text).expect("read back");
            assert_eq!(back.display_text().as_str(), v.display_text().as_str(), "{text}");
        }
        let shortcut = settings_dialog(Theme::Flat, ModePref::System, &AppSettings::default())
            .value_of(s("editing.palette"))
            .into_option()
            .expect("the palette shortcut");
        assert!(stored(&shortcut).is_none(), "a shortcut is not kept");
    }

    #[test]
    fn a_value_out_of_range_or_unreadable_keeps_the_default() {
        let number = ShellSettingValue::Number(ShellSettingNumber::create(10.0, 1.0, 120.0));
        assert!(restored(&number, "500").is_none(), "past the maximum");
        assert!(restored(&number, "ten").is_none());
        let choice = ShellSettingValue::Choice(ShellSettingChoice::create(strs(&["A", "B"]), 0));
        assert!(restored(&choice, "7").is_none(), "no such option");
        assert!(restored(&ShellSettingValue::Toggle(true), "maybe").is_none());
        assert!(restored(&ShellSettingValue::Color(ColorU::rgba(0, 0, 0, 255)), "#zz").is_none());
        assert_eq!(setting_key("general.reopen"), "setting.general.reopen");
    }
}
