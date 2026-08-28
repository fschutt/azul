//! The `SysDialogType::UpdateVersion` dialog: check → changelog → consent →
//! install, with the package-manager-first policy visible in the UI.
//!
//! Flow: [`open`] spawns the CHECK worker (manifest + changelog fetch) on an
//! azul `Thread` and opens the CPU-rendered dialog window in the `Checking`
//! phase. The worker's writeback flips the shared state to what it found:
//!
//! * self-updatable install + newer version → changelog + **Install now** /
//!   **Remind me later** (7-day suspend) / **Close**
//! * package-managed install → "update via `<hint>`" note, NO install button
//! * up-to-date / error → the respective message + **Close**
//!
//! **Install now** downloads (resumable; a staged artifact is reused via
//! the download cache, after re-verification),
//! atomically swaps the binary, and reports "restart to use x.y.z". Consent
//! is structural: the ONLY path to `apply_update` is that button.

// fn-item -> fn-pointer casts: required for the Into<Callback> generics;
// the annotated-temporary alternative buries the callback wiring.
#![allow(trivial_casts)]
use azul_core::{
    callbacks::Update,
    refany::RefAny,
    task::{ThreadId, ThreadReceiver},
};
use azul_css::AzString;

use super::{cpu_dialog_window, markdown, style};
use crate::callbacks::CallbackInfo;
use crate::thread::{
    Thread, ThreadCallbackType, ThreadReceiveMsg, ThreadSender, ThreadWriteBackMsg,
    WriteBackCallbackType,
};
use crate::updater::{
    apply_update, check_for_updates_blocking, default_state_dir, download_and_verify,
    effective_mode, InstallKind, ReleaseInfo, UpdateCheckResult, UpdateMode, UpdateState,
};
use crate::widgets::button::{Button, ButtonOnClickCallbackType};
use azul_core::callbacks::{LayoutCallbackInfo, LayoutCallbackType};
use azul_core::dom::Dom;

/// Where the dialog currently is.
// Internal (not repr(C)) state machine; Available legitimately carries the
// whole release + changelog while the terminal states carry a line of text.
#[allow(variant_size_differences, clippy::large_enum_variant)]
#[derive(Debug, Clone)]
pub enum UpdatePhase {
    /// The check worker is running.
    Checking,
    /// Nothing newer than the running version.
    UpToDate { current: String },
    /// The check (or the install) failed.
    Failed { error: String },
    /// A newer release exists; waiting for the user's decision.
    Available {
        release: ReleaseInfo,
        /// Fetched changelog Markdown ("" = none available).
        changelog: String,
        /// Requested mode clamped by the install kind — decides whether an
        /// Install button exists at all.
        effective: UpdateMode,
        /// "your package manager" wording for notify-only installs.
        install_hint: String,
    },
    /// Install consented; download/swap in progress.
    Installing { version: String },
    /// Swap done — the NEXT start runs the new version.
    Installed { version: String },
}

/// Shared dialog state (window ctx + worker writebacks).
#[derive(Debug, Clone)]
pub struct UpdateDialogState {
    pub phase: UpdatePhase,
}

/// Opens the dialog and starts the check. Called by
/// `CallbackInfo::invoke_system_dialog(SysDialogType::UpdateVersion)`.
pub fn open(info: &mut CallbackInfo) {
    let state = RefAny::new(UpdateDialogState {
        phase: UpdatePhase::Checking,
    });
    let task = CheckTask {
        env: crate::appenv::app_env(),
    };
    info.add_thread(
        ThreadId::unique(),
        Thread::create(
            RefAny::new(task),
            state.clone(),
            check_worker as ThreadCallbackType,
        ),
    );
    info.create_window(cpu_dialog_window(
        "Software Update",
        (560.0, 520.0),
        dialog_layout as LayoutCallbackType,
        state,
    ));
}

struct CheckTask {
    env: crate::appenv::AppEnv,
}

struct NewPhase(UpdatePhase);

/// Background: manifest check + changelog fetch → one writeback.
extern "C" fn check_worker(mut init: RefAny, mut sender: ThreadSender, _recv: ThreadReceiver) {
    let Some(task) = init.downcast_ref::<CheckTask>() else {
        return;
    };
    let env = task.env.clone();
    drop(task);

    let install = InstallKind::detect();
    let effective =
        crate::updater::apply_shared_update_policy(effective_mode(env.update_mode, &install));
    let phase = match (env.update_manifest.as_deref(), effective) {
        (_, UpdateMode::Disabled) => UpdatePhase::Failed {
            error: "updates are disabled in this build".to_owned(),
        },
        (None, _) => UpdatePhase::Failed {
            error: "no update manifest configured (AppConfig.updates.manifest_url)".to_owned(),
        },
        (Some(url), _) => {
            let dir = default_state_dir(&env.app_name);
            let audience = if effective == UpdateMode::SelfUpdate {
                crate::updater::UpdateAudience::AutoUpdate
            } else {
                crate::updater::UpdateAudience::NotifyOnly
            };
            let mut state = UpdateState::load(&dir);
            let result = check_for_updates_blocking(
                url,
                &env.current_version,
                &env.update_channel,
                &mut state,
                audience,
            );
            state.save(&dir);
            match result {
                UpdateCheckResult::UpToDate => UpdatePhase::UpToDate {
                    current: env.current_version.clone(),
                },
                UpdateCheckResult::Error(e) => UpdatePhase::Failed {
                    error: e.as_str().to_owned(),
                },
                UpdateCheckResult::Available(release) => {
                    let changelog = fetch_changelog(&release, env.changelog_md.as_deref());
                    UpdatePhase::Available {
                        release,
                        changelog,
                        effective,
                        install_hint: install.package_manager_hint().to_owned(),
                    }
                }
            }
        }
    };

    let _ = sender.send(ThreadReceiveMsg::WriteBack(ThreadWriteBackMsg::new(
        apply_phase as WriteBackCallbackType,
        RefAny::new(NewPhase(phase)),
    )));
}

/// The release's own changelog link wins; `AppConfig.changelog_md` is the
/// fallback. Capped at 256 KiB — this renders into a dialog, not a pager.
fn fetch_changelog(release: &ReleaseInfo, fallback: Option<&str>) -> String {
    // A source that carried the changelog inline (a GitHub release body)
    // has already given us the Markdown — no request, and it works even if
    // the release page is unreachable.
    if !release.changelog_md_inline.as_str().is_empty() {
        return release.changelog_md_inline.as_str().to_owned();
    }
    let url = if release.changelog_md_url.as_str().is_empty() {
        fallback.unwrap_or("")
    } else {
        release.changelog_md_url.as_str()
    };
    if url.is_empty() {
        return String::new();
    }
    match crate::http::http_get_with_config(url, &crate::http::HttpRequestConfig::new()) {
        Ok(r) if (200..300).contains(&r.status_code) => {
            let body = r.body.as_ref();
            let cap = body.len().min(256 * 1024);
            String::from_utf8_lossy(&body[..cap]).into_owned()
        }
        _ => String::new(),
    }
}

/// Main thread: move the shared state to the worker's phase.
extern "C" fn apply_phase(mut state: RefAny, mut msg: RefAny, _info: CallbackInfo) -> Update {
    let Some(new_phase) = msg.downcast_ref::<NewPhase>().map(|p| p.0.clone()) else {
        return Update::DoNothing;
    };
    if let Some(mut s) = state.downcast_mut::<UpdateDialogState>() {
        s.phase = new_phase;
    }
    // The dialog window shares this RefAny via layout_callback.ctx; the
    // thread lives on the window that OPENED the dialog — refresh all.
    Update::RefreshDomAllWindows
}

// --- consent buttons ------------------------------------------------------

struct InstallTask {
    release: ReleaseInfo,
    app_name: String,
    root_public_key: String,
}

extern "C" fn on_install_now(mut state: RefAny, mut info: CallbackInfo) -> Update {
    let task = {
        let Some(mut s) = state.downcast_mut::<UpdateDialogState>() else {
            return Update::DoNothing;
        };
        let UpdatePhase::Available { release, .. } = s.phase.clone() else {
            return Update::DoNothing;
        };
        let version = release.version.as_str().to_owned();
        s.phase = UpdatePhase::Installing { version };
        let env = crate::appenv::app_env();
        InstallTask {
            release,
            app_name: env.app_name,
            root_public_key: env.update_root_public_key.unwrap_or_default(),
        }
    };
    info.add_thread(
        ThreadId::unique(),
        Thread::create(
            RefAny::new(task),
            state.clone(),
            install_worker as ThreadCallbackType,
        ),
    );
    Update::RefreshDomAllWindows
}

/// Background: (reuse staged | download) → verify-by-construction → atomic
/// swap of the RUNNING executable. Only ever reached through the consent
/// button, and only on installs where `effective_mode` allowed `SelfUpdate`.
extern "C" fn install_worker(mut init: RefAny, mut sender: ThreadSender, _recv: ThreadReceiver) {
    let Some(task) = init.downcast_ref::<InstallTask>() else {
        return;
    };
    let release = task.release.clone();
    let app_name = task.app_name.clone();
    let root_public_key = task.root_public_key.clone();
    drop(task);

    let version = release.version.as_str().to_owned();
    let state_dir = default_state_dir(&app_name);
    let staging_dir = state_dir.join("staging");
    // Even a previously staged artifact goes through download_and_verify:
    // its cached exit re-checks the digest AND the signature chain on THIS
    // call, so a file tampered on disk between staging and consent can
    // never reach apply_update. (The path shortcut it replaces trusted the
    // filename verbatim.)
    let staged_path = {
        let mut state = UpdateState::load(&state_dir);
        let r = download_and_verify(&release, &staging_dir, &root_public_key, &mut state)
            .map(|o| o.path);
        state.save(&state_dir);
        r
    };
    let outcome = staged_path.and_then(|artifact| {
        let target = std::env::current_exe().map_err(|e| e.to_string())?;
        apply_update(&artifact, &target)
    });

    #[cfg(feature = "telemetry")]
    crate::telemetry::record_update_apply(if outcome.is_ok() { "ok" } else { "error" });

    let phase = match outcome {
        Ok(()) => UpdatePhase::Installed { version },
        Err(error) => UpdatePhase::Failed { error },
    };
    let _ = sender.send(ThreadReceiveMsg::WriteBack(ThreadWriteBackMsg::new(
        apply_phase as WriteBackCallbackType,
        RefAny::new(NewPhase(phase)),
    )));
}

extern "C" fn on_remind_later(mut _state: RefAny, mut info: CallbackInfo) -> Update {
    // "Remind me later" = a 7-day suspension in the updater's state file;
    // checks report UpToDate until it passes.
    let dir = default_state_dir(&crate::appenv::app_env().app_name);
    let mut state = UpdateState::load(&dir);
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs());
    state.suspended_until_unix = now + 7 * 86_400;
    state.save(&dir);
    info.close_window();
    Update::DoNothing
}

extern "C" fn on_close(mut _state: RefAny, mut info: CallbackInfo) -> Update {
    info.close_window();
    Update::DoNothing
}

// --- the dialog DOM -------------------------------------------------------

extern "C" fn dialog_layout(_data: RefAny, info: LayoutCallbackInfo) -> Dom {
    let Some(mut ctx) = info.get_ctx().into_option() else {
        return Dom::create_body();
    };
    let phase = match ctx.downcast_ref::<UpdateDialogState>() {
        Some(s) => s.phase.clone(),
        None => return Dom::create_body(),
    };
    drop(ctx);

    use azul_css::props::{
        basic::pixel::PixelValue,
        layout::{LayoutPaddingBottom, LayoutPaddingLeft, LayoutPaddingRight, LayoutPaddingTop},
    };
    let pad = style(vec![
        azul_css::props::property::CssProperty::padding_left(LayoutPaddingLeft {
            inner: PixelValue::px(16.0),
        }),
        azul_css::props::property::CssProperty::padding_right(LayoutPaddingRight {
            inner: PixelValue::px(16.0),
        }),
        azul_css::props::property::CssProperty::padding_top(LayoutPaddingTop {
            inner: PixelValue::px(16.0),
        }),
        azul_css::props::property::CssProperty::padding_bottom(LayoutPaddingBottom {
            inner: PixelValue::px(16.0),
        }),
    ]);

    let state = info
        .get_ctx()
        .into_option()
        .unwrap_or_else(|| RefAny::new(()));
    let mut children: Vec<Dom> = Vec::new();
    match phase {
        UpdatePhase::Checking => {
            children.push(Dom::create_h2_with_text("Checking for updates…"));
            children.push(Dom::create_p_with_text(
                "Contacting the update server. This only takes a moment.",
            ));
        }
        UpdatePhase::UpToDate { current } => {
            children.push(Dom::create_h2_with_text("You are up to date"));
            children.push(Dom::create_p_with_text(format!(
                "Version {current} is the newest release."
            )));
            children.push(button_row(vec![("Close", on_close, state)]));
        }
        UpdatePhase::Failed { error } => {
            children.push(Dom::create_h2_with_text("Update check failed"));
            children.push(Dom::create_p_with_text(error));
            children.push(button_row(vec![("Close", on_close, state)]));
        }
        UpdatePhase::Available {
            release,
            changelog,
            effective,
            install_hint,
            ..
        } => {
            children.push(Dom::create_h2_with_text(format!(
                "Version {} is available",
                release.version.as_str()
            )));
            if changelog.is_empty() {
                children.push(Dom::create_p_with_text("No changelog was provided."));
            } else {
                children.push(markdown::render_markdown(&changelog));
            }
            if effective == UpdateMode::SelfUpdate {
                children.push(Dom::create_p_with_text(
                    "Installing replaces the current version; the update is used after a restart.",
                ));
                children.push(button_row(vec![
                    ("Install now", on_install_now, state.clone()),
                    ("Remind me later", on_remind_later, state.clone()),
                    ("Close", on_close, state),
                ]));
            } else {
                // Package-managed install: azul NEVER touches the binary.
                children.push(Dom::create_p_with_text(format!(
                    "This installation is managed — update it via {install_hint}."
                )));
                children.push(button_row(vec![
                    ("Remind me later", on_remind_later, state.clone()),
                    ("Close", on_close, state),
                ]));
            }
        }
        UpdatePhase::Installing { version } => {
            children.push(Dom::create_h2_with_text(format!("Installing {version}…")));
            children.push(Dom::create_p_with_text(
                "Downloading (resumes automatically if interrupted) and swapping the binary.",
            ));
        }
        UpdatePhase::Installed { version } => {
            children.push(Dom::create_h2_with_text("Update installed"));
            children.push(Dom::create_p_with_text(format!(
                "Version {version} is installed. Restart the application to start using it."
            )));
            children.push(button_row(vec![("Close", on_close, state)]));
        }
    }

    Dom::create_body().with_child(
        Dom::create_div()
            .with_css_props(pad)
            .with_children(children.into()),
    )
}

/// A row of buttons sharing the dialog state.
fn button_row(buttons: Vec<(&str, ButtonOnClickCallbackType, RefAny)>) -> Dom {
    let children: Vec<Dom> = buttons
        .into_iter()
        .map(|(label, cb, state)| {
            Button::create(AzString::from(label))
                .with_on_click(state, cb)
                .dom()
        })
        .collect();
    Dom::create_div().with_children(children.into())
}
