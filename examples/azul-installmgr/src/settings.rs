//! The update settings: auto-update and the channel for every app and per app, and what the
//! manager works with (the install folder, the update source, the signature check, this
//! computer's rollout group, the maintenance window). Every change goes into the shared config's
//! `updates` section on the worker; the apps read it at their start and the update kit before
//! every check.

use azul::{
    callbacks::{
        SegmentedOnChangeCallbackType, SwitchOnToggleCallbackType, TextInputOnFocusLostCallbackType,
        TextInputOnVirtualKeyDownCallbackType,
    },
    dom::VirtualKeyCode,
    prelude::*,
    str::String as AzString,
    widgets::{
        Button, OnTextInputReturn, Segmented, SegmentedState, Switch, SwitchState, TextInput,
        TextInputState, TextInputValid,
    },
};
use azul_appkit::{
    azlin_config::Channel,
    l10n::{labels, t, t_args, Arg},
    pieces,
};

use crate::{
    jobs::{self, ConfigChange, Job},
    model::{self, AppEntry},
    Manager, Screen,
};

/// A setting's control: which app it is for (`None`: every app).
struct SettingTarget {
    /// The manager's data.
    app: RefAny,
    key: Option<&'static str>,
}

fn s(text: &str) -> AzString {
    AzString::from(text)
}

fn target(app: &RefAny, key: Option<&'static str>) -> RefAny {
    RefAny::new(SettingTarget {
        app: app.clone(),
        key,
    })
}

fn channel_index(channel: Channel) -> usize {
    match channel {
        Channel::Stable => 0,
        Channel::Beta => 1,
    }
}

fn channel_control(app: &RefAny, key: Option<&'static str>, channel: Channel, id: String) -> Dom {
    Segmented::create(labels(&["azinstallmgr-channel-stable", "azinstallmgr-channel-beta"]))
        .with_selected_index(channel_index(channel))
        .with_on_change(target(app, key), on_channel as SegmentedOnChangeCallbackType)
        .dom()
        .with_id(s(&id))
}

fn autoupdate_control(app: &RefAny, key: Option<&'static str>, on: bool, id: String) -> Dom {
    Switch::create(on)
        .with_accessibility_name(s(&t("azinstallmgr-autoupdate")))
        .with_on_toggle(target(app, key), on_autoupdate as SwitchOnToggleCallbackType)
        .dom()
        .with_id(s(&id))
}

fn line(label: String, control: Dom) -> Dom {
    pieces::flex_row(
        "gap: 12px; padding: 6px 0px;",
        vec![
            pieces::block("width: 280px; flex-shrink: 0;", pieces::text(label)),
            control,
        ],
    )
}

fn heading(key: &str) -> Dom {
    pieces::block(
        "font-weight: bold; padding: 14px 0px 4px 0px; border-bottom: 1px solid rgba(127, 127, 127, 0.3);",
        pieces::text(t(key)),
    )
}

fn fact(label_key: &str, value: String) -> Dom {
    line(t(label_key), pieces::text(value))
}

/// The update source: the address the apps' update manifests are fetched from, typed in and
/// kept with Enter or when the field is left. Nothing is published at a default address, so
/// while there is none the line under the field says that nothing can be installed or updated
/// and where the address comes from. The settings screen and the wizard's Updates page share
/// it.
pub(crate) fn source_section(m: &Manager, app: &RefAny) -> Dom {
    let current = m.updates.manifest_url.clone().unwrap_or_default();
    let field = TextInput::create()
        .with_text(s(&current))
        .with_placeholder(s(&t("azinstallmgr-source-placeholder")))
        .with_accessibility_name(s(&t("azinstallmgr-settings-source")))
        .with_on_virtual_key_down(app.clone(), on_source_key as TextInputOnVirtualKeyDownCallbackType)
        .with_on_focus_lost(app.clone(), on_source_blur as TextInputOnFocusLostCallbackType)
        .dom()
        .with_id(s("installmgr-source"))
        .with_css("flex-grow: 1; min-width: 240px;");
    let help = if !m.source_problem.is_empty() {
        m.source_problem.clone()
    } else if model::has_source(&m.updates) {
        t("azinstallmgr-source-help")
    } else {
        t("azinstallmgr-source-none")
    };
    pieces::column(
        "gap: 4px; padding: 6px 0px;",
        vec![
            pieces::text(t("azinstallmgr-settings-source")),
            pieces::flex_row("gap: 8px;", vec![field]),
            pieces::block("font-size: 11px; opacity: 0.8;", pieces::text(help))
                .with_id(s("installmgr-source-help")),
        ],
    )
}

/// Enter in the source's field keeps what it holds.
extern "C" fn on_source_key(mut data: RefAny, mut info: CallbackInfo, state: TextInputState) -> OnTextInputReturn {
    let key = info.get_current_keyboard_state().current_virtual_keycode.into_option();
    let update = match key {
        Some(VirtualKeyCode::Return | VirtualKeyCode::NumpadEnter) => {
            info.stop_propagation();
            keep_source(&mut data, &mut info, state.get_text().as_str())
        }
        _ => Update::DoNothing,
    };
    OnTextInputReturn {
        update,
        valid: TextInputValid::Yes,
    }
}

/// Leaving the source's field keeps what it holds (a click on Next or Back included).
extern "C" fn on_source_blur(mut data: RefAny, mut info: CallbackInfo, state: TextInputState) -> Update {
    keep_source(&mut data, &mut info, state.get_text().as_str())
}

/// Keeps `text` as the update source: at once for the jobs (an install started next fetches
/// from it), and in the config on the worker. A text that is no web address is not kept; the
/// line under the field says why.
fn keep_source(data: &mut RefAny, info: &mut CallbackInfo, text: &str) -> Update {
    let app_ref = data.clone();
    let Some(mut m) = data.downcast_mut::<Manager>() else {
        return Update::DoNothing;
    };
    let url = match model::source_from_text(text) {
        Ok(url) => url,
        Err(key) => {
            m.source_problem = t(key);
            return Update::RefreshDom;
        }
    };
    let had_problem = !m.source_problem.is_empty();
    m.source_problem.clear();
    if url == m.updates.manifest_url {
        return if had_problem {
            Update::RefreshDom
        } else {
            Update::DoNothing
        };
    }
    m.updates.manifest_url = url.clone();
    let fetching = m.fetching();
    drop(m);
    let change = ConfigChange::Source(url);
    println!("AZINSTALLMGR_SETTING {}", change.describe());
    jobs::spawn(info, &app_ref, Job::Config(change), fetching);
    Update::RefreshDom
}

/// The settings screen.
pub(crate) fn screen(m: &Manager, app: &RefAny) -> Dom {
    let shared = m.updates.policy_for("");
    let back = Button::create(s(&t("azinstallmgr-back-to-apps")))
        .with_icon(s("arrow_back"))
        .with_on_click(app.clone(), on_back)
        .dom()
        .with_id(s("installmgr-settings-back"));
    let mut children = vec![
        pieces::flex_row("padding: 4px 0px 8px 0px;", vec![back]),
        heading("azinstallmgr-settings-every-app"),
        line(
            t("azinstallmgr-autoupdate"),
            autoupdate_control(app, None, shared.autoupdate, "installmgr-autoupdate-all".into()),
        ),
        line(
            t("azinstallmgr-settings-channel"),
            channel_control(app, None, shared.channel, "installmgr-channel-all".into()),
        ),
        heading("azinstallmgr-settings-each-app"),
    ];
    for a in m.apps.iter().filter(|a| !a.entry.is_runtime()) {
        let key = a.entry.key;
        children.push(pieces::flex_row(
            "gap: 12px; padding: 4px 0px;",
            vec![
                pieces::block("width: 160px; flex-shrink: 0;", pieces::text(a.entry.name)),
                channel_control(app, Some(key), a.channel, format!("installmgr-channel-{key}")),
                autoupdate_control(app, Some(key), a.autoupdate, format!("installmgr-autoupdate-{key}")),
            ],
        ));
    }
    let signatures = t(
        if m.updates
            .root_public_key
            .as_deref()
            .is_some_and(|k| !k.trim().is_empty())
        {
            "azinstallmgr-settings-signed"
        } else {
            "azinstallmgr-settings-digest-only"
        },
    );
    let window = m
        .updates
        .maintenance_window
        .clone()
        .filter(|w| !w.trim().is_empty())
        .unwrap_or_else(|| t("azinstallmgr-settings-any-time"));
    children.extend([
        heading("azinstallmgr-settings-this-computer"),
        fact(
            "azinstallmgr-settings-folder",
            m.places.install_dir.display().to_string(),
        ),
        source_section(m, app),
        fact("azinstallmgr-settings-signatures", signatures),
        fact(
            "azinstallmgr-settings-group",
            t_args(
                "azinstallmgr-cohort",
                &[("cohort", Arg::from(u64::from(m.cohort)))],
            ),
        ),
        fact("azinstallmgr-settings-window", window),
        pieces::block(
            "padding: 10px 0px; font-size: 11px; opacity: 0.75;",
            pieces::text(t_args(
                "azinstallmgr-settings-config",
                &[("path", Arg::from(m.places.config.display().to_string()))],
            )),
        ),
    ]);
    if !m.notice.is_empty() {
        children.push(pieces::text(m.notice.clone()));
    }
    pieces::column(
        "flex-grow: 1; min-height: 0px; overflow-y: auto; padding: 8px 16px;",
        children,
    )
    .with_id(s("installmgr-settings"))
}

/// Opens the settings.
pub(crate) fn open(m: &mut Manager) {
    m.screen = Screen::Settings;
    m.notice.clear();
    println!("AZINSTALLMGR_SCREEN settings");
}

extern "C" fn on_back(mut data: RefAny, _info: CallbackInfo) -> Update {
    let Some(mut m) = data.downcast_mut::<Manager>() else {
        return Update::DoNothing;
    };
    m.screen = Screen::Apps;
    m.notice.clear();
    println!("AZINSTALLMGR_SCREEN apps");
    Update::RefreshDom
}

/// The manager's data and the app of a setting's control.
fn setting_target(data: &mut RefAny) -> Option<(RefAny, Option<&'static AppEntry>)> {
    let target = data.downcast_ref::<SettingTarget>()?;
    let entry = match target.key {
        Some(key) => Some(model::entry(key)?),
        None => None,
    };
    Some((target.app.clone(), entry))
}

/// Writes `change` into the config on the worker.
fn change(data: &mut RefAny, info: &mut CallbackInfo, make: impl FnOnce(Option<&'static str>) -> ConfigChange) -> Update {
    let Some((mut app, entry)) = setting_target(data) else {
        return Update::DoNothing;
    };
    let app_ref = app.clone();
    let Some(m) = app.downcast_ref::<Manager>() else {
        return Update::DoNothing;
    };
    let fetching = m.fetching();
    drop(m);
    let change = make(entry.map(|e| e.key));
    println!("AZINSTALLMGR_SETTING {}", change.describe());
    jobs::spawn(info, &app_ref, Job::Config(change), fetching);
    Update::DoNothing
}

extern "C" fn on_autoupdate(mut data: RefAny, mut info: CallbackInfo, state: SwitchState) -> Update {
    change(&mut data, &mut info, |key| ConfigChange::Autoupdate(key, state.checked))
}

extern "C" fn on_channel(mut data: RefAny, mut info: CallbackInfo, state: SegmentedState) -> Update {
    let channel = if state.selected_index == 1 {
        Channel::Beta
    } else {
        Channel::Stable
    };
    change(&mut data, &mut info, |key| ConfigChange::Channel(key, channel))
}

/// The config was written: the policy every job works with, and the rows' channels and
/// auto-update, from it. A row whose channel changed is checked again from scratch (its
/// available version belonged to the other channel).
pub(crate) fn configured(m: &mut Manager, result: Result<azul_appkit::azlin_config::UpdatesSection, String>) {
    match result {
        Ok(updates) => {
            for a in &mut m.apps {
                let policy = updates.policy_for(a.entry.key);
                if policy.channel != a.channel {
                    a.status = model::Status::Unchecked;
                }
                a.channel = policy.channel;
                a.autoupdate = policy.autoupdate;
            }
            m.updates = updates;
            m.notice = t("azinstallmgr-settings-saved");
        }
        Err(why) => m.notice = why,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn manager(source: Option<&str>) -> Manager {
        let mut updates = azul_appkit::azlin_config::UpdatesSection::default();
        updates.manifest_url = source.map(str::to_string);
        Manager::for_test(std::path::PathBuf::from("/tmp/azlin-test/config.json"), updates)
    }

    /// The update source is a setting the user can type in, with a line that says what it is
    /// for - and, while there is none, that nothing can be installed or updated without it.
    #[test]
    fn the_settings_offer_a_field_for_the_update_source_and_say_what_it_is_for() {
        crate::l10n::in_english();
        let m = manager(None);
        let dom = screen(&m, &RefAny::new(0u8));
        assert!(crate::has_id(&dom, "installmgr-source"), "a field for the update source");
        assert!(crate::has_id(&dom, "installmgr-source-help"), "a line that explains it");
        let m = manager(Some("https://example.org/{app}/{channel}.json"));
        let dom = screen(&m, &RefAny::new(0u8));
        assert!(crate::has_id(&dom, "installmgr-source"), "the field holds the source");
    }
}
