//! The update kit inside an Azlin app (feature `azul`).
//!
//! An app calls [`start`] where it builds its `AppConfig`, and puts [`banner`] at the top of its
//! window; the kit does the rest:
//!
//! - at the start it reads the policy from `~/.azlin/config.json`, registers the app
//!   (`current.json`: its name, version and program) and arms the engine's own update settings
//!   (`AppConfig.updates`, notify-only: an app never replaces its own program);
//! - from the window's creation on, a timer checks the app's manifest every six hours under the
//!   machine's rollout cohort, and stages a newer release when auto-update is on and the program
//!   is the manager's install ([`super::background_round`], on a `Thread`);
//! - a staged or available release shows a thin bar: "A new version of AzWriter is available
//!   (2.1.0). [Update now] [Later] [Skip this version]";
//! - "Update now" writes the request for AzInstallManager, starts it with `--apply <key>` and
//!   closes the app's window (the app's own close guard still asks to save); the manager installs
//!   once the app's process is gone and starts the app again. A copy the manager did not install
//!   opens azul's update dialog instead (`SysDialogType::UpdateVersion`), notify-only.

use std::path::{Path, PathBuf};

use azul::{
    callbacks::{TimerCallbackInfo, TimerCallbackReturn, UpdateMode},
    dialog::SysDialogType,
    option::OptionString,
    prelude::*,
    str::String as AzString,
    task::{
        Thread, ThreadId, ThreadReceiveMsg, ThreadReceiver, ThreadSender, ThreadWriteBackMsg,
        Timer, TimerId,
    },
    time::{Duration, SystemTimeDiff},
    widgets::{Button, ButtonType},
};

use super::{
    audience_for, background_round, install_dir, machine_cohort, manifest_url_for, read_json,
    register, request_apply, same_program, unix_now, AppPaths, ApplyRequest, AzulEngine, InstallKind,
    Registration, Release, Round, RoundOutcome, Staged, Starting, UpdateEngine, MANAGER_KEY,
};
use crate::{
    azlin_config::AzlinConfig,
    l10n::{t, t_args, Arg},
    pieces,
    ui::Kit,
};

/// How often the timer looks whether a check is due.
const TICK_MS: u64 = 60_000;
/// How long after a check the next one is due.
pub const CHECK_EVERY_SECS: u64 = 6 * 3600;
/// How long "Later" keeps the banner away.
const LATER_SECS: u64 = 24 * 3600;

/// A newer release the banner offers.
#[derive(Clone, Debug, PartialEq, Eq)]
struct Offer {
    release: Release,
    /// Fetched and verified already: "Update now" installs at once.
    staged: bool,
}

/// One app's update kit, kept in its [`Kit`] from [`start`] on.
pub struct AppUpdates {
    /// The app's key (`azwriter`): its registry folder, its manifest's `{app}`.
    key: &'static str,
    /// The app's name and the version that runs (the kit's About facts).
    name: &'static str,
    version: &'static str,
    /// The running program.
    binary: PathBuf,
    paths: AppPaths,
    /// The shared config: the policy is read from it before every check.
    config: PathBuf,
    /// The running program is the manager's install: "Update now" goes through the manager,
    /// and only then is anything staged for it.
    managed: bool,
    /// What the banner offers (`None`: no banner).
    offer: Option<Offer>,
    /// "Later": the banner stays away until then.
    later_until: u64,
    /// A check runs on its thread.
    checking: bool,
    /// When the last check began (0: none yet, the first tick checks).
    last_check: u64,
    /// The banner's line after "Update now" when the manager could not be started.
    note: String,
}

impl AppUpdates {
    /// The background round of now, under the policy the config says now (`None`: no update
    /// source is configured).
    fn round(&self) -> Option<Round> {
        let (shared, _) = AzlinConfig::load(&self.config);
        let updates = shared.updates;
        let policy = updates.policy_for(self.key);
        let manifest_url = manifest_url_for(&updates, self.key, policy.channel)?;
        let (cohort, problem) = machine_cohort(&AzulEngine, &self.config, &updates);
        if let Some(problem) = problem {
            eprintln!("[{}] {problem}", self.key);
        }
        let kind = if self.managed {
            InstallKind::UserWritable
        } else {
            AzulEngine.install_kind(&self.binary)
        };
        Some(Round {
            paths: self.paths.clone(),
            current_version: self.version.to_string(),
            manifest_url,
            channel: policy.channel,
            cohort,
            audience: audience_for(kind, policy.channel),
            root_public_key: updates.root_public_key.clone().unwrap_or_default(),
            autoupdate: policy.autoupdate && self.managed,
            skip_version: policy.skip_version,
            maintenance_window: updates.maintenance_window.clone().unwrap_or_default(),
        })
    }
}

/// Starts the update kit of the app whose kit is `kit_ref` and whose key is `key` (`azwriter`):
/// registers it, arms `config.updates` and shows a release staged before at once. Called where
/// the app builds its `AppConfig`, before the window. Nothing without a shared config
/// (`AZLIN_CONFIG=off`, a `--shot` run).
pub fn start(kit_ref: &RefAny, config: &mut AppConfig, key: &'static str) {
    let mut kit = kit_ref.clone();
    let Some(mut k) = kit.downcast_mut::<Kit>() else {
        return;
    };
    let Some(config_path) = k.config_path.clone() else {
        return;
    };
    let (shared, _) = AzlinConfig::load(&config_path);
    let updates = shared.updates;
    let azlin_dir = config_path
        .parent()
        .map_or_else(|| PathBuf::from("."), Path::to_path_buf);
    let paths = AppPaths::new(&azlin_dir, &install_dir(&azlin_dir, &updates), key);
    let binary = std::env::current_exe().unwrap_or_default();
    let (name, version) = (k.about.name, k.about.version);
    let started = register(
        &paths,
        &Starting {
            name,
            key,
            version,
            binary: &binary,
            now_unix: unix_now(),
        },
    );
    if let Err(e) = started {
        eprintln!("[{key}] {}: {e}", paths.registration().display());
    }
    let managed = read_json::<Registration>(&paths.registration())
        .is_some_and(|r| r.managed && same_program(Path::new(&r.binary), &binary));
    // An "Update now" of an earlier run is void: the user started the app again.
    let _ = std::fs::remove_file(paths.request());
    let policy = updates.policy_for(key);
    let offer = read_json::<Staged>(&paths.staged())
        .filter(|s| {
            managed
                && s.release.version != version
                && Path::new(&s.file).exists()
                && policy.skip_version.as_deref() != Some(s.release.version.as_str())
        })
        .map(|s| Offer {
            release: s.release,
            staged: true,
        });
    if let Some(offer) = &offer {
        println!("AZLIN_UPDATE_OFFER {key} {} staged", offer.release.version);
    }

    // The engine's own update paths (its update dialog, `check_for_updates`) know the app too,
    // notify-only: an app never replaces its own program.
    config.updates.mode = UpdateMode::NotifyOnly;
    config.updates.app_name = AzString::from(key);
    config.updates.current_version = AzString::from(version);
    config.updates.channel = AzString::from(policy.channel.name());
    if let Some(url) = manifest_url_for(&updates, key, policy.channel) {
        config.updates.manifest_url = OptionString::Some(AzString::from(url));
    }
    config.updates.root_public_key =
        AzString::from(updates.root_public_key.clone().unwrap_or_default());

    k.updates = Some(AppUpdates {
        key,
        name,
        version,
        binary,
        paths,
        config: config_path,
        managed,
        offer,
        later_until: 0,
        checking: false,
        last_check: 0,
        note: String::new(),
    });
}

/// Arms the periodic check (the kit's window-created callback calls it when [`start`] ran).
pub(crate) fn arm(kit_ref: &RefAny, info: &mut CallbackInfo) {
    let timer = Timer::create(kit_ref.clone(), on_tick, info.get_system_time_fn())
        .with_interval(Duration::System(SystemTimeDiff::from_millis(TICK_MS)));
    info.add_timer(TimerId::unique(), timer);
}

/// What the round's thread gets.
struct RoundInit {
    /// Taken by the worker.
    round: Option<Round>,
    now: u64,
}

extern "C" fn on_tick(mut data: RefAny, mut info: TimerCallbackInfo) -> TimerCallbackReturn {
    let kit_ref = data.clone();
    let Some(mut k) = data.downcast_mut::<Kit>() else {
        return TimerCallbackReturn::terminate_unchanged();
    };
    let Some(u) = k.updates.as_mut() else {
        return TimerCallbackReturn::terminate_unchanged();
    };
    let now = unix_now();
    if u.checking || now.saturating_sub(u.last_check) < CHECK_EVERY_SECS {
        return TimerCallbackReturn::continue_unchanged();
    }
    u.last_check = now;
    let Some(round) = u.round() else {
        return TimerCallbackReturn::continue_unchanged();
    };
    u.checking = true;
    drop(k);
    info.callback_info.add_thread(
        ThreadId::unique(),
        Thread::create(
            RefAny::new(RoundInit {
                round: Some(round),
                now,
            }),
            kit_ref,
            round_worker,
        ),
    );
    TimerCallbackReturn::continue_unchanged()
}

extern "C" fn round_worker(mut init: RefAny, mut sender: ThreadSender, _receiver: ThreadReceiver) {
    let Some((round, now)) = init
        .downcast_mut::<RoundInit>()
        .and_then(|mut i| Some((i.round.take()?, i.now)))
    else {
        return;
    };
    let outcome = background_round(&AzulEngine, &round, now);
    let _sent = sender.send(ThreadReceiveMsg::WriteBack(ThreadWriteBackMsg::create(
        on_round,
        RefAny::new(outcome),
    )));
}

/// The round's outcome: what the banner offers.
extern "C" fn on_round(mut data: RefAny, mut msg: RefAny, _info: CallbackInfo) -> Update {
    let Some(outcome) = msg
        .downcast_ref::<RoundOutcome>()
        .map(|o| RoundOutcome::clone(&o))
    else {
        return Update::DoNothing;
    };
    let Some(mut k) = data.downcast_mut::<Kit>() else {
        return Update::DoNothing;
    };
    let Some(u) = k.updates.as_mut() else {
        return Update::DoNothing;
    };
    u.checking = false;
    let before = u.offer.clone();
    match outcome {
        RoundOutcome::Staged(staged) => {
            u.offer = Some(Offer {
                release: staged.release,
                staged: true,
            });
        }
        RoundOutcome::Available(release) => {
            u.offer = Some(Offer {
                release,
                staged: false,
            });
        }
        RoundOutcome::UpToDate | RoundOutcome::Skipped(_) => u.offer = None,
        RoundOutcome::Failed(why) => eprintln!("[{}] update check: {why}", u.key),
    }
    if let Some(offer) = &u.offer {
        let how = if offer.staged { "staged" } else { "available" };
        println!("AZLIN_UPDATE_OFFER {} {} {how}", u.key, offer.release.version);
    }
    if u.offer == before {
        Update::DoNothing
    } else {
        Update::RefreshDom
    }
}

/// The banner over the app's window while a newer release is offered (`None`: none, or the
/// user said Later).
#[must_use]
pub fn banner(kit_ref: &RefAny) -> Option<Dom> {
    let mut kit = kit_ref.clone();
    let k = kit.downcast_ref::<Kit>()?;
    let u = k.updates.as_ref()?;
    let offer = u.offer.as_ref()?;
    if unix_now() < u.later_until {
        return None;
    }
    let line = if u.note.is_empty() {
        t_args(
            "kit-update-banner",
            &[
                ("app", Arg::from(u.name)),
                ("version", Arg::from(offer.release.version.as_str())),
            ],
        )
    } else {
        u.note.clone()
    };
    let button = |key: &str, id: &str, primary: bool, cb: azul::callbacks::ButtonOnClickCallbackType| {
        let b = Button::create(AzString::from(t(key))).with_on_click(kit_ref.clone(), cb);
        let b = if primary {
            b.with_button_type(ButtonType::Primary)
        } else {
            b
        };
        b.dom().with_id(AzString::from(id))
    };
    Some(
        pieces::flex_row(
            "gap: 8px; padding: 4px 10px; flex-shrink: 0; \
             background: rgba(47, 74, 133, 0.12); border-bottom: 1px solid rgba(47, 74, 133, 0.35);",
            vec![
                Dom::create_icon(AzString::from("system_update_alt")),
                pieces::block("flex-grow: 1;", pieces::text(line)),
                button("kit-update-now", "kit-update-now", true, on_update_now),
                button("kit-update-later", "kit-update-later", false, on_later),
                button("kit-update-skip", "kit-update-skip", false, on_skip),
            ],
        )
        .with_id(AzString::from("kit-update-banner")),
    )
}

/// The manager's program, from its registration.
fn manager_program(u: &AppUpdates) -> Option<PathBuf> {
    let azlin_dir = u.config.parent()?;
    let r: Registration = read_json(&azlin_dir.join(super::APPS_DIR).join(MANAGER_KEY).join(super::REGISTRATION_FILE))?;
    let program = PathBuf::from(r.binary);
    program.exists().then_some(program)
}

/// "Update now": the manager's install asks the manager (the request, the manager started with
/// `--apply <key>`, the window closed); any other copy opens azul's update dialog.
extern "C" fn on_update_now(mut data: RefAny, mut info: CallbackInfo) -> Update {
    let Some(mut k) = data.downcast_mut::<Kit>() else {
        return Update::DoNothing;
    };
    let Some(u) = k.updates.as_mut() else {
        return Update::DoNothing;
    };
    let Some(offer) = u.offer.clone() else {
        return Update::DoNothing;
    };
    if !u.managed {
        drop(k);
        info.invoke_system_dialog(SysDialogType::UpdateVersion);
        return Update::DoNothing;
    }
    let request = ApplyRequest {
        version: offer.release.version.clone(),
        pid: std::process::id(),
        relaunch: true,
        asked_unix: unix_now(),
    };
    if let Err(e) = request_apply(&u.paths, &request) {
        u.note = t_args("kit-update-failed", &[("reason", Arg::from(e.to_string()))]);
        return Update::RefreshDom;
    }
    println!("AZLIN_UPDATE_REQUESTED {} {}", u.key, request.version);
    let started = manager_program(u).map(|program| {
        std::process::Command::new(program)
            .arg("--apply")
            .arg(u.key)
            .stdin(std::process::Stdio::null())
            .spawn()
    });
    match started {
        Some(Ok(_child)) => {
            drop(k);
            // The app's own close guard still asks to save; the manager waits for the process.
            info.close_window();
            Update::DoNothing
        }
        Some(Err(e)) => {
            u.note = t_args("kit-update-failed", &[("reason", Arg::from(e.to_string()))]);
            Update::RefreshDom
        }
        None => {
            u.note = t("kit-update-no-manager");
            Update::RefreshDom
        }
    }
}

extern "C" fn on_later(mut data: RefAny, _info: CallbackInfo) -> Update {
    let Some(mut k) = data.downcast_mut::<Kit>() else {
        return Update::DoNothing;
    };
    let Some(u) = k.updates.as_mut() else {
        return Update::DoNothing;
    };
    u.later_until = unix_now() + LATER_SECS;
    println!("AZLIN_UPDATE_LATER {}", u.key);
    Update::RefreshDom
}

/// "Skip this version": the policy skips it (the manager and the next check leave it), the
/// staged file is thrown away; a newer release is offered again.
extern "C" fn on_skip(mut data: RefAny, _info: CallbackInfo) -> Update {
    let Some(mut k) = data.downcast_mut::<Kit>() else {
        return Update::DoNothing;
    };
    let Some(u) = k.updates.as_mut() else {
        return Update::DoNothing;
    };
    let Some(offer) = u.offer.take() else {
        return Update::DoNothing;
    };
    let version = offer.release.version;
    let key = u.key;
    if let Err(e) = AzlinConfig::edit(&u.config, |c| {
        c.updates.set_skip_version(key, Some(version.as_str()));
    }) {
        eprintln!("[{key}] {}: {e}", u.config.display());
    }
    if let Some(staged) = read_json::<Staged>(&u.paths.staged()) {
        if staged.release.version == version {
            let _ = std::fs::remove_file(&staged.file);
            let _ = std::fs::remove_file(u.paths.staged());
        }
    }
    println!("AZLIN_UPDATE_SKIPPED {key} {version}");
    Update::RefreshDom
}
