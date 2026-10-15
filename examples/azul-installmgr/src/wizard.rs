//! The first run: azul's wizard pages for the real install. Welcome; the package manager's
//! command when brew, apt or flatpak is on the PATH (shown with Copy, never run); the install
//! folder with the space needed and free; the apps (the runtime required, the apps in groups);
//! the channel (stable or beta) and auto-update; the summary; the install itself on a `Thread`
//! (fetch, verify, put in place, shortcuts) with its log; the end, which shows the installed
//! apps.

use azul::{
    dom::ClipboardContent,
    file::FilePath,
    option::OptionString,
    prelude::*,
    str::String as AzString,
    vec::{StringPairVec, StringVec, StyledTextRunVec, WizardComponentVec, WizardOptionVec},
    widgets::{
        Button, WizardComponent, WizardComponentsPage, WizardDestinationPage,
        WizardEvent, WizardEventKind, WizardFinishPage, WizardLayout, WizardLayoutStyle,
        WizardOption, WizardOptionsPage, WizardPageEvent, WizardPageEventKind,
        WizardProgressPage, WizardSummaryPage, WizardWelcomePage,
    },
};
use azul_appkit::{
    azlin_config::Channel,
    l10n::{t, t_args, Arg},
    pieces,
};

use crate::{
    jobs::{self, Job},
    model::{self, component_rows, needed_bytes, AppEntry, ComponentRow, PackageManager, Step},
    Manager, Screen,
};

/// The option rows of the channel page, in order.
const OPTION_STABLE: usize = 0;
const OPTION_BETA: usize = 1;
const OPTION_AUTOUPDATE: usize = 2;

/// The wizard's state.
pub(crate) struct Wizard {
    /// The steps of this run (the package manager's page only when one is found).
    pub(crate) steps: Vec<Step>,
    pub(crate) step: Step,
    /// The package manager on the PATH, for its page.
    pub(crate) package_manager: Option<PackageManager>,
    /// The install folder as typed or browsed.
    pub(crate) path: String,
    /// The free bytes on the folder's drive, when known.
    pub(crate) available: Option<u64>,
    /// The component list with its ticks (the page keeps the tree's rule).
    pub(crate) components: WizardComponentsPage,
    /// The rows behind the page's components, in the same order: which app a tick installs.
    pub(crate) rows: Vec<ComponentRow>,
    /// The channel radio pair and the auto-update box.
    pub(crate) options: WizardOptionsPage,
    pub(crate) finish: WizardFinishPage,
    pub(crate) progress: InstallProgress,
    /// The progress page's log is open.
    pub(crate) show_log: bool,
    /// The package manager's command was copied (the page says so).
    pub(crate) copied: bool,
}

/// How far the install is.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct InstallProgress {
    /// The apps to install (the runtime counted).
    pub(crate) total: usize,
    /// The apps done, installed or not.
    pub(crate) done: usize,
    pub(crate) failed: usize,
    /// The app being installed now (its name).
    pub(crate) current: String,
    /// What happened, oldest first.
    pub(crate) log: Vec<String>,
    /// Every app is done.
    pub(crate) finished: bool,
}

impl InstallProgress {
    /// How far along, 0 to 100.
    #[allow(clippy::cast_precision_loss)]
    pub(crate) fn percent(&self) -> f32 {
        if self.total == 0 {
            return if self.finished { 100.0 } else { 0.0 };
        }
        (self.done as f32 * 100.0 / self.total as f32).min(100.0)
    }
}

fn s(text: &str) -> AzString {
    AzString::from(text)
}

impl Wizard {
    /// A first run whose install folder is `path`, with `package_manager` on the PATH.
    pub(crate) fn new(path: String, package_manager: Option<PackageManager>) -> Wizard {
        let rows = component_rows();
        Wizard {
            steps: model::steps(package_manager, true),
            step: Step::Welcome,
            package_manager,
            available: None,
            components: components_page(&rows),
            rows,
            options: options_page(),
            finish: finish_page(),
            progress: InstallProgress::default(),
            show_log: false,
            copied: false,
            path,
        }
    }

    /// The apps the ticks pick, the runtime first.
    pub(crate) fn selected(&self) -> Vec<&'static AppEntry> {
        self.components
            .components
            .as_ref()
            .iter()
            .zip(&self.rows)
            .filter(|(c, _)| c.checked)
            .filter_map(|(_, row)| row.entry)
            .collect()
    }

    /// The channel the radio pair says.
    pub(crate) fn channel(&self) -> Channel {
        let beta = self
            .options
            .options
            .as_ref()
            .get(OPTION_BETA)
            .is_some_and(|o| o.checked);
        if beta {
            Channel::Beta
        } else {
            Channel::Stable
        }
    }

    /// Whether auto-update is ticked.
    pub(crate) fn autoupdate(&self) -> bool {
        self.options
            .options
            .as_ref()
            .get(OPTION_AUTOUPDATE)
            .is_some_and(|o| o.checked)
    }

    fn announce(&self) {
        println!("{}", model::step_line(&self.steps, self.step));
    }
}

fn components_page(rows: &[ComponentRow]) -> WizardComponentsPage {
    let components: Vec<WizardComponent> = rows
        .iter()
        .map(|row| {
            let label = if row.entry.is_some_and(|e| !e.is_runtime()) {
                row.label.to_string()
            } else {
                t(row.label)
            };
            let mut c = WizardComponent::create(s(&label), row.bytes)
                .with_depth(row.depth)
                .with_checked(true)
                .with_required(row.required);
            if !row.description_key.is_empty() {
                c = c.with_description(s(&t(row.description_key)));
            }
            c
        })
        .collect();
    WizardComponentsPage::create(WizardComponentVec::from_vec(components))
        .with_intro(s(&t("azinstallmgr-components-intro")))
        .with_total_label(s(&t("azinstallmgr-space-required")))
}

fn options_page() -> WizardOptionsPage {
    WizardOptionsPage::create(WizardOptionVec::from_vec(vec![
        WizardOption::create(s(&t("azinstallmgr-channel-stable")), true)
            .with_group(1)
            .with_description(s(&t("azinstallmgr-channel-stable-help"))),
        WizardOption::create(s(&t("azinstallmgr-channel-beta")), false)
            .with_group(1)
            .with_description(s(&t("azinstallmgr-channel-beta-help"))),
        WizardOption::create(s(&t("azinstallmgr-autoupdate")), true)
            .with_description(s(&t("azinstallmgr-autoupdate-help"))),
    ]))
}

fn finish_page() -> WizardFinishPage {
    WizardFinishPage::create(
        s(&t("azinstallmgr-finish-title")),
        s(&t("azinstallmgr-finish-text")),
    )
    .with_logo(s("check_circle"))
    .with_options(WizardOptionVec::from_vec(vec![WizardOption::create(
        s(&t("azinstallmgr-finish-show-apps")),
        true,
    )]))
}

/// The free bytes on the drive the folder would be created on.
pub(crate) fn free_space(path: &str) -> Option<u64> {
    FilePath::create(s(path))
        .disk_space_for_new()
        .into_option()
        .map(|d| d.free)
}

/// The package manager's page: its command for the ticked apps, Copy, and the way on.
fn package_manager_page(w: &Wizard, pm: PackageManager, app: &RefAny) -> Dom {
    let command = pm.install_command(&w.selected());
    let paragraph = |text: String| pieces::block("margin-bottom: 10px;", pieces::text(text));
    let mut children = vec![
        paragraph(t_args("azinstallmgr-pm-found", &[("manager", Arg::from(pm.name()))])),
        paragraph(t("azinstallmgr-pm-how")),
        pieces::block(
            "font-family: monospace; padding: 8px; margin-bottom: 8px; border: 1px solid #c8c8c8; \
             background: rgba(127, 127, 127, 0.08); user-select: text;",
            pieces::text(command.as_str()),
        )
        .with_id(s("installmgr-pm-command")),
        pieces::flex_row(
            "gap: 8px; margin-bottom: 14px;",
            vec![Button::create(s(&t("azinstallmgr-copy")))
                .with_icon(s("content_copy"))
                .with_on_click(app.clone(), on_copy_command)
                .dom()
                .with_id(s("installmgr-pm-copy"))],
        ),
    ];
    if w.copied {
        children.push(paragraph(t("azinstallmgr-copied")));
    }
    children.push(paragraph(t("azinstallmgr-pm-or-direct")));
    pieces::column("padding: 4px 0px;", children)
}

/// The current page and the reason Next is held on it (empty: it may go).
fn page(m: &Manager, app: &RefAny) -> (Dom, String) {
    let w = &m.wizard;
    match w.step {
        Step::Welcome => (
            WizardWelcomePage::create(
                s(&t("azinstallmgr-welcome-title")),
                s(&t("azinstallmgr-welcome-text")),
            )
            .with_logo(s("install_desktop"))
            .dom(),
            String::new(),
        ),
        Step::PackageManager => match w.package_manager {
            Some(pm) => (package_manager_page(w, pm, app), String::new()),
            None => (Dom::create_div(), String::new()),
        },
        Step::Destination => {
            let mut p =
                WizardDestinationPage::create(s(&w.path), needed_bytes(&w.selected()))
                    .with_intro(s(&t("azinstallmgr-destination-intro")))
                    .with_labels(
                        s(&t("azinstallmgr-destination-field")),
                        s(&t("azinstallmgr-browse")),
                        s(&t("azinstallmgr-destination-dialog")),
                        s(&t("azinstallmgr-space-required")),
                        s(&t("azinstallmgr-space-available")),
                    )
                    .with_reasons(
                        s(&t("azinstallmgr-destination-empty")),
                        s(&t("azinstallmgr-destination-short")),
                    )
                    .with_on_event(app.clone(), on_page);
            if let Some(free) = w.available {
                p = p.with_available(free);
            }
            let reason = p.blocked_reason().as_str().to_string();
            (p.dom(), reason)
        }
        Step::Components => {
            let p = w.components.clone().with_on_event(app.clone(), on_page);
            let reason = p.blocked_reason().as_str().to_string();
            (p.dom(), reason)
        }
        // The channel and auto-update, then where the apps come from.
        Step::Channel => (
            pieces::column(
                "flex-grow: 1; min-height: 0px;",
                vec![
                    w.options.clone().with_on_event(app.clone(), on_page).dom(),
                    pieces::block(
                        "padding: 0px 24px 12px 24px;",
                        crate::settings::source_section(m, app),
                    ),
                ],
            ),
            String::new(),
        ),
        Step::Ready => {
            let names: Vec<&str> = w
                .selected()
                .iter()
                .filter(|e| !e.is_runtime())
                .map(|e| e.name)
                .collect();
            let channel = match w.channel() {
                Channel::Stable => t("azinstallmgr-channel-stable"),
                Channel::Beta => t("azinstallmgr-channel-beta"),
            };
            let autoupdate = t(if w.autoupdate() {
                "azinstallmgr-yes"
            } else {
                "azinstallmgr-no"
            });
            let source = m
                .updates
                .manifest_url
                .clone()
                .filter(|_| model::has_source(&m.updates))
                .unwrap_or_else(|| t("azinstallmgr-source-unset"));
            let p = WizardSummaryPage::create(StringPairVec::from_vec(Vec::new()))
                .with_intro(s(&t("azinstallmgr-ready-intro")))
                .with_row(s(&t("azinstallmgr-ready-folder")), s(&w.path))
                .with_row(s(&t("azinstallmgr-ready-apps")), s(&names.join(", ")))
                .with_row(s(&t("azinstallmgr-ready-channel")), s(&channel))
                .with_row(s(&t("azinstallmgr-autoupdate")), s(&autoupdate))
                .with_row(s(&t("azinstallmgr-settings-source")), s(&source));
            // Nothing can be fetched without a source: Install waits and says where to set it.
            let reason = if model::has_source(&m.updates) {
                String::new()
            } else {
                t("azinstallmgr-ready-no-source")
            };
            (p.dom(), reason)
        }
        Step::Installing => {
            let progress = &w.progress;
            let status = if progress.finished {
                t_args(
                    "azinstallmgr-installed-status",
                    &[
                        ("count", Arg::from(progress.done - progress.failed)),
                        ("failed", Arg::from(progress.failed)),
                    ],
                )
            } else {
                t("azinstallmgr-installing-status")
            };
            let p = WizardProgressPage::create(progress.percent())
                .with_status(s(&status))
                .with_current_item(s(&progress.current))
                .with_log(StringVec::from_vec(progress.log.iter().map(|l| s(l)).collect()))
                .with_show_log(w.show_log)
                .with_details_labels(
                    s(&t("azinstallmgr-show-details")),
                    s(&t("azinstallmgr-hide-details")),
                )
                .with_on_event(app.clone(), on_page);
            let reason = if progress.finished {
                String::new()
            } else {
                t("azinstallmgr-installing-wait")
            };
            (p.dom(), reason)
        }
        Step::Finish => (
            w.finish.clone().with_on_event(app.clone(), on_page).dom(),
            String::new(),
        ),
    }
}

/// The wizard in its layout: the side panel on welcome and finish, the banner between.
pub(crate) fn wizard(m: &Manager, app: &RefAny) -> Dom {
    let w = &m.wizard;
    let (page, reason) = page(m, app);
    let labels: Vec<AzString> = w.steps.iter().map(|st| s(&t(st.label_key()))).collect();
    let index = w.steps.iter().position(|st| *st == w.step).unwrap_or(0);
    let subtitle = if w.step.subtitle_key().is_empty() {
        String::new()
    } else {
        t(w.step.subtitle_key())
    };
    WizardLayout::create(s(&t("azinstallmgr-wizard-title")), StringVec::from_vec(labels))
        .with_current_step(index)
        .with_style(if w.step.is_outer() {
            WizardLayoutStyle::SidePanel
        } else {
            WizardLayoutStyle::Banner
        })
        .with_subtitle(s(&subtitle))
        .with_icon(s("install_desktop"))
        .with_labels(
            s(&t("azinstallmgr-button-back")),
            s(&t(if w.step == Step::Ready {
                "azinstallmgr-button-install"
            } else {
                "azinstallmgr-button-next"
            })),
            s(&t("azinstallmgr-button-finish")),
            s(&if w.step == Step::Finish {
                String::new()
            } else {
                t("azinstallmgr-button-cancel")
            }),
        )
        .with_validation(s(&reason))
        .with_can_go_back(!matches!(
            w.step,
            Step::Welcome | Step::Installing | Step::Finish
        ))
        .with_page(page)
        .with_on_event(app.clone(), on_wizard)
        .dom()
}

/// Starts the install of the ticked apps: the choices go into the config on the worker, the
/// apps are fetched one after the other.
fn start_install(m: &mut Manager, info: &mut CallbackInfo, app: &RefAny) {
    let apps = m.wizard.selected();
    m.wizard.progress = InstallProgress {
        total: apps.len(),
        ..InstallProgress::default()
    };
    let mut fetching = m.fetching();
    fetching.places.install_dir = std::path::PathBuf::from(&m.wizard.path);
    let default_dir = azul_appkit::update::install_dir(&m.places.azlin_dir, &Default::default());
    let install_dir = (fetching.places.install_dir != default_dir).then(|| m.wizard.path.clone());
    m.places.install_dir = fetching.places.install_dir.clone();
    m.busy = true;
    jobs::spawn(
        info,
        app,
        Job::Install {
            apps,
            channel: m.wizard.channel(),
            autoupdate: m.wizard.autoupdate(),
            install_dir,
        },
        fetching,
    );
}

extern "C" fn on_wizard(mut data: RefAny, mut info: CallbackInfo, event: WizardEvent) -> Update {
    let app = data.clone();
    let Some(mut m) = data.downcast_mut::<Manager>() else {
        return Update::DoNothing;
    };
    match event.kind {
        WizardEventKind::Next => {
            match m.wizard.step {
                Step::Ready => {
                    start_install(&mut *m, &mut info, &app);
                    m.wizard.step = Step::Installing;
                }
                Step::Installing if !m.wizard.progress.finished => return Update::DoNothing,
                step => m.wizard.step = model::next_step(&m.wizard.steps, step),
            }
            if m.wizard.step == Step::Destination && m.wizard.available.is_none() {
                m.wizard.available = free_space(&m.wizard.path);
            }
            m.wizard.announce();
        }
        WizardEventKind::Back => {
            m.wizard.step = model::previous_step(&m.wizard.steps, m.wizard.step);
            m.wizard.announce();
        }
        WizardEventKind::Finish => {
            let show_apps = m
                .wizard
                .finish
                .options
                .as_ref()
                .first()
                .is_some_and(|o| o.checked);
            println!("AZINSTALLMGR_FINISHED show_apps={show_apps}");
            if !show_apps {
                info.close_window();
                return Update::DoNothing;
            }
            m.screen = Screen::Apps;
            m.load_apps(&mut info, &app);
            // The table wants the room the wizard did not.
            let mut window = info.get_current_window_state();
            window.size.dimensions = LogicalSize::create(960.0, 620.0);
            info.modify_window_state(window);
            println!("AZINSTALLMGR_SCREEN apps");
        }
        WizardEventKind::Cancel => m.confirm_cancel = true,
        WizardEventKind::Step => {
            // The side panel goes back to a step already passed, never forward.
            let at = m.wizard.steps.iter().position(|st| *st == m.wizard.step).unwrap_or(0);
            let back_allowed = !matches!(m.wizard.step, Step::Installing | Step::Finish);
            if event.step < at && back_allowed {
                m.wizard.step = m.wizard.steps[event.step];
                m.wizard.announce();
            }
        }
    }
    Update::RefreshDom
}

extern "C" fn on_page(mut data: RefAny, _info: CallbackInfo, event: WizardPageEvent) -> Update {
    let Some(mut m) = data.downcast_mut::<Manager>() else {
        return Update::DoNothing;
    };
    let w = &mut m.wizard;
    match event.kind {
        WizardPageEventKind::PathChanged => {
            w.path = event.text.as_str().to_string();
            w.available = free_space(&w.path);
            println!("AZINSTALLMGR_PATH {}", w.path);
        }
        WizardPageEventKind::ComponentToggled => {
            w.components.toggle(event.index, event.checked);
            w.copied = false;
            println!("AZINSTALLMGR_TOTAL {}", needed_bytes(&w.selected()));
        }
        WizardPageEventKind::OptionToggled => {
            if w.step == Step::Finish {
                w.finish.choose(event.index, event.checked);
            } else {
                w.options.choose(event.index, event.checked);
                println!(
                    "AZINSTALLMGR_CHANNEL {} autoupdate={}",
                    w.channel().name(),
                    w.autoupdate()
                );
            }
        }
        WizardPageEventKind::DetailsToggled => w.show_log = event.checked,
        WizardPageEventKind::LicenseAccepted => {}
    }
    Update::RefreshDom
}

/// Copy: the package manager's command onto the clipboard.
extern "C" fn on_copy_command(mut data: RefAny, mut info: CallbackInfo) -> Update {
    let Some(mut m) = data.downcast_mut::<Manager>() else {
        return Update::DoNothing;
    };
    let Some(pm) = m.wizard.package_manager else {
        return Update::DoNothing;
    };
    let command = pm.install_command(&m.wizard.selected());
    info.set_clipboard_content(ClipboardContent {
        plain_text: s(&command),
        styled_runs: StyledTextRunVec::create(),
        html: OptionString::None,
    });
    m.wizard.copied = true;
    println!("AZINSTALLMGR_COPIED {command}");
    Update::RefreshDom
}

/// One step of the install, from the worker.
pub(crate) fn installed(w: &mut Wizard, key: &str, result: &Result<String, String>) {
    let name = model::entry(key).map_or(key, |e| e.name);
    w.progress.done += 1;
    match result {
        Ok(version) => {
            w.progress.log.push(t_args(
                "azinstallmgr-log-installed",
                &[("app", Arg::from(name)), ("version", Arg::from(version.as_str()))],
            ));
            println!("AZINSTALLMGR_INSTALLED {key} {version}");
        }
        Err(why) => {
            w.progress.failed += 1;
            w.progress.log.push(t_args(
                "azinstallmgr-log-failed",
                &[("app", Arg::from(name)), ("reason", Arg::from(why.as_str()))],
            ));
            println!("AZINSTALLMGR_INSTALL_FAILED {key} {why}");
        }
    }
    println!("AZINSTALLMGR_PROGRESS {}", w.progress.percent().round());
}

/// Work on an app began.
pub(crate) fn started(w: &mut Wizard, key: &str) {
    let name = model::entry(key).map_or(key, |e| e.name);
    w.progress.current = t_args("azinstallmgr-installing-app", &[("app", Arg::from(name))]);
}

/// The install is done.
pub(crate) fn finished(w: &mut Wizard) {
    w.progress.finished = true;
    w.progress.current.clear();
    println!(
        "AZINSTALLMGR_DONE installed={} failed={}",
        w.progress.done - w.progress.failed,
        w.progress.failed
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    fn manager(source: Option<&str>) -> Manager {
        let mut updates = azul_appkit::azlin_config::UpdatesSection::default();
        updates.manifest_url = source.map(str::to_string);
        Manager::for_test(std::path::PathBuf::from("/tmp/azlin-test/config.json"), updates)
    }

    /// The Updates page asks for the update source beside the channel: a first run has none
    /// configured, and the wizard is where the user meets that.
    #[test]
    fn the_updates_page_asks_for_the_update_source() {
        crate::l10n::in_english();
        let mut m = manager(None);
        m.wizard.step = Step::Channel;
        let (dom, reason) = page(&m, &RefAny::new(0u8));
        assert!(crate::has_id(&dom, "installmgr-source"), "a field for the update source");
        assert!(crate::has_id(&dom, "installmgr-source-help"), "a line that explains it");
        assert_eq!(reason, "", "the page itself never holds Next");
    }

    /// Without an update source nothing can be fetched: Install waits and says why, instead of
    /// failing every app with a path to a config file.
    #[test]
    fn without_an_update_source_install_waits_and_says_why() {
        crate::l10n::in_english();
        let mut m = manager(None);
        m.wizard.step = Step::Ready;
        let (_, reason) = page(&m, &RefAny::new(0u8));
        assert_eq!(reason, t("azinstallmgr-ready-no-source"));
        assert!(!reason.starts_with("azinstallmgr-"), "a sentence, not a key: {reason}");
        let mut m = manager(Some("https://example.org/{app}/{channel}.json"));
        m.wizard.step = Step::Ready;
        let (_, reason) = page(&m, &RefAny::new(0u8));
        assert_eq!(reason, "", "with a source Install may go");
    }
}
