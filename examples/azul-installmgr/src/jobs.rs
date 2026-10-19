//! The manager's work on an azul `Thread`: installing, checking, updating, reverting, removing,
//! and the apps' "Update now". No callback waits on the network or the disk: each step's outcome
//! comes back to the window as a write-back ([`Progress`], `crate::on_progress`).

use std::path::Path;

use azul::{
    prelude::*,
    task::{Thread, ThreadId, ThreadReceiveMsg, ThreadReceiver, ThreadSender, ThreadWriteBackMsg},
};
use azul_appkit::{
    azlin_config::{AzlinConfig, Channel},
    update::{read_json, unix_now, AzulEngine, Registration, Release, Staged, UpdateEngine},
};

use crate::model::{self, AppEntry, Fetching, Installed, PackageManager, Status};

/// One piece of work.
pub(crate) enum Job {
    /// The first install of `apps` from `channel`, the runtime first. The wizard's choices go
    /// into the config before: the channel and auto-update for every app, the install folder
    /// when it is not the default one.
    Install {
        apps: Vec<&'static AppEntry>,
        channel: Channel,
        autoupdate: bool,
        install_dir: Option<String>,
    },
    /// Read the installed apps again (their registrations and owners).
    Load,
    /// Check (and stage, as each app's policy says) every app of `apps`.
    Check { apps: Vec<Installed> },
    /// Update each app to its release (`None`: look for it first).
    Update {
        apps: Vec<(Installed, Option<Release>)>,
    },
    Revert { entry: &'static AppEntry },
    Uninstall { entry: &'static AppEntry },
    /// The apps' waiting "Update now": install the version each said yes to once its process
    /// is gone, then start it again.
    Requests,
    /// A change of the update policy, into the config.
    Config(ConfigChange),
    /// The first install through the system's package manager: its commands, one after the
    /// other.
    ThroughManager {
        manager: PackageManager,
        runs: Vec<Vec<String>>,
    },
}

/// Which kind of job finished.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum JobKind {
    /// Install, check, update, revert, uninstall: the buttons waited for it.
    Work,
    /// The poll of the apps' requests: it runs beside the others.
    Requests,
    /// Reading the apps, writing a setting: quick, beside the others.
    Background,
}

impl Job {
    fn kind(&self) -> JobKind {
        match self {
            Job::Requests => JobKind::Requests,
            Job::Load | Job::Config(_) => JobKind::Background,
            _ => JobKind::Work,
        }
    }
}

/// A change of the update policy: for every app (`None`) or one app (its key), or the update
/// source every app is fetched from (`None`: no source).
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum ConfigChange {
    Autoupdate(Option<&'static str>, bool),
    Channel(Option<&'static str>, Channel),
    Source(Option<String>),
}

impl ConfigChange {
    /// The change in a line, for scripts.
    pub(crate) fn describe(&self) -> String {
        match self {
            ConfigChange::Autoupdate(key, on) => format!("autoupdate {} {on}", key.unwrap_or("*")),
            ConfigChange::Channel(key, channel) => {
                format!("channel {} {}", key.unwrap_or("*"), channel.name())
            }
            ConfigChange::Source(url) => format!("source {}", url.as_deref().unwrap_or("-")),
        }
    }

    /// Makes the change in `updates`. A change for every app is every app's: the apps whose own
    /// choice differs take it too.
    fn apply(self, updates: &mut azul_appkit::azlin_config::UpdatesSection) {
        match self {
            ConfigChange::Autoupdate(None, on) => {
                updates.set_autoupdate(None, on);
                for a in &model::APPS {
                    if updates.policy_for(a.key).autoupdate != on {
                        updates.set_autoupdate(Some(a.key), on);
                    }
                }
            }
            ConfigChange::Autoupdate(Some(key), on) => updates.set_autoupdate(Some(key), on),
            ConfigChange::Channel(None, channel) => {
                updates.set_channel(None, channel);
                for a in &model::APPS {
                    if updates.policy_for(a.key).channel != channel {
                        updates.set_channel(Some(a.key), channel);
                    }
                }
            }
            ConfigChange::Channel(Some(key), channel) => updates.set_channel(Some(key), channel),
            ConfigChange::Source(url) => updates.manifest_url = url,
        }
    }
}

/// What a step of a job came to.
#[derive(Clone, Debug)]
pub(crate) enum Progress {
    /// Work on the app `key` began.
    Started { key: String },
    Installed {
        key: String,
        result: Result<Registration, String>,
        /// The shortcuts that could not be made (the app is installed either way).
        problems: Vec<String>,
    },
    Checked { key: String, status: Status },
    Loaded { apps: Vec<Installed> },
    /// The policy as the config says it after a change, or why it could not be written.
    Configured {
        result: Result<azul_appkit::azlin_config::UpdatesSection, String>,
    },
    Updated {
        key: String,
        result: Result<Registration, String>,
    },
    /// The version reverted from and the registration of the version gone back to.
    Reverted {
        key: String,
        result: Result<(String, Registration), String>,
    },
    Uninstalled {
        key: String,
        result: Result<(), String>,
    },
    /// An app's "Update now" was carried out (and the app started again when it asked).
    Requested {
        key: String,
        result: Result<Registration, String>,
    },
    /// A command of the package manager ran: its output's lines, or why it failed.
    RanManager {
        command: String,
        result: Result<Vec<String>, String>,
    },
    /// The job is done.
    Finished { kind: JobKind },
}

/// What the worker thread gets.
struct JobInit {
    /// Taken by the worker (a `RefAny` hands out references only).
    job: Option<Job>,
    fetching: Fetching,
}

/// Runs `job` on a `Thread`; every step comes back to `app` through `crate::on_progress`.
pub(crate) fn spawn(info: &mut CallbackInfo, app: &RefAny, job: Job, fetching: Fetching) {
    info.add_thread(
        ThreadId::unique(),
        Thread::create(
            RefAny::new(JobInit {
                job: Some(job),
                fetching,
            }),
            app.clone(),
            worker,
        ),
    );
}

extern "C" fn worker(mut init: RefAny, mut sender: ThreadSender, _receiver: ThreadReceiver) {
    let Some((job, fetching)) = init
        .downcast_mut::<JobInit>()
        .and_then(|mut i| Some((i.job.take()?, i.fetching.clone())))
    else {
        return;
    };
    let kind = job.kind();
    let mut send = |progress: Progress| {
        let _sent = sender.send(ThreadReceiveMsg::WriteBack(ThreadWriteBackMsg::create(
            crate::on_progress,
            RefAny::new(progress),
        )));
    };
    run(&AzulEngine, job, &fetching, &mut send);
    send(Progress::Finished { kind });
}

/// Runs `job` with `engine`, telling `send` every step. Blocking.
pub(crate) fn run(
    engine: &dyn UpdateEngine,
    job: Job,
    fetching: &Fetching,
    send: &mut dyn FnMut(Progress),
) {
    match job {
        Job::Install {
            apps,
            channel,
            autoupdate,
            install_dir,
        } => {
            let kept = AzlinConfig::edit(&fetching.places.config, |c| {
                c.updates.set_channel(None, channel);
                c.updates.set_autoupdate(None, autoupdate);
                if let Some(dir) = &install_dir {
                    c.updates.install_dir = Some(dir.clone());
                }
            });
            if let Err(e) = kept {
                eprintln!("[installmgr] {}: {e}", fetching.places.config.display());
            }
            install(engine, fetching, &apps, channel, send);
        }
        Job::Load => send(Progress::Loaded {
            apps: model::installed(engine, &fetching.places, &fetching.updates),
        }),
        Job::Check { apps } => {
            for app in apps {
                let key = app.entry.key.to_string();
                send(Progress::Started { key: key.clone() });
                let status = model::check_installed(engine, fetching, &app, unix_now());
                send(Progress::Checked { key, status });
            }
        }
        Job::Update { apps } => {
            for (app, release) in apps {
                let key = app.entry.key.to_string();
                send(Progress::Started { key: key.clone() });
                let release = match release {
                    Some(release) => Ok(release),
                    None => model::release_now(engine, fetching, &app, unix_now()),
                };
                let result = release.and_then(|release| {
                    model::update_to(engine, fetching, app.entry, &release, app.channel, unix_now())
                });
                send(Progress::Updated { key, result });
            }
        }
        Job::Revert { entry } => {
            let key = entry.key.to_string();
            send(Progress::Started { key: key.clone() });
            let result = model::revert(&fetching.places, entry, unix_now());
            send(Progress::Reverted { key, result });
        }
        Job::Uninstall { entry } => {
            let key = entry.key.to_string();
            send(Progress::Started { key: key.clone() });
            let result = model::uninstall(&fetching.places, entry);
            send(Progress::Uninstalled { key, result });
        }
        Job::Requests => requests(engine, fetching, send),
        Job::Config(change) => {
            let result = AzlinConfig::edit(&fetching.places.config, |c| change.apply(&mut c.updates))
                .map(|c| c.updates)
                .map_err(|e| format!("{}: {e}", fetching.places.config.display()));
            send(Progress::Configured { result });
        }
        Job::ThroughManager { manager, runs } => {
            for run in runs {
                let command = model::shell_words(&run);
                eprintln!("[installmgr] {}: {command}", manager.name());
                let result = model::run_command(&run);
                send(Progress::RanManager { command, result });
            }
        }
    }
}

/// The first install: the runtime first (without it no app starts, so a failed runtime ends
/// the install), then every app.
fn install(
    engine: &dyn UpdateEngine,
    fetching: &Fetching,
    apps: &[&'static AppEntry],
    channel: Channel,
    send: &mut dyn FnMut(Progress),
) {
    let mut runtime_failed = None;
    for entry in apps {
        let key = entry.key.to_string();
        send(Progress::Started { key: key.clone() });
        let outcome = match &runtime_failed {
            Some(why) => Err(format!("the Azlin runtime could not be installed: {why}")),
            None => model::install_new(engine, fetching, entry, channel, unix_now()),
        };
        let (result, problems) = match outcome {
            Ok((registration, problems)) => (Ok(registration), problems),
            Err(why) => {
                if entry.is_runtime() {
                    runtime_failed = Some(why.clone());
                }
                (Err(why), Vec::new())
            }
        };
        send(Progress::Installed {
            key,
            result,
            problems,
        });
    }
}

/// The apps' "Update now": for each request whose app has quit, the version it said yes to is
/// installed - the staged file checked once more, or (auto-update off: nothing was staged) the
/// release fetched now - and the app starts again when it asked. A request of an app still
/// running waits for the next poll.
fn requests(engine: &dyn UpdateEngine, fetching: &Fetching, send: &mut dyn FnMut(Progress)) {
    for (key, request) in model::pending_requests(&fetching.places.azlin_dir) {
        if model::process_alive(request.pid) {
            continue;
        }
        let Some(entry) = model::entry(&key) else {
            continue;
        };
        let paths = fetching.places.app(&key);
        let _ = std::fs::remove_file(paths.request());
        send(Progress::Started { key: key.clone() });
        let channel = fetching.updates.policy_for(&key).channel;
        let staged = read_json::<Staged>(&paths.staged())
            .filter(|s| s.release.version == request.version);
        let result = match staged {
            Some(staged) => {
                model::update_to(engine, fetching, entry, &staged.release, channel, unix_now())
            }
            None => model::installed(engine, &fetching.places, &fetching.updates)
                .into_iter()
                .find(|a| a.entry.key == entry.key && a.is_managed())
                .ok_or_else(|| format!("{} was not installed by AzInstallManager", entry.name))
                .and_then(|a| model::release_now(engine, fetching, &a, unix_now()))
                .and_then(|release| {
                    model::update_to(engine, fetching, entry, &release, channel, unix_now())
                }),
        };
        if let (Ok(registration), true) = (&result, request.relaunch) {
            if let Err(why) = model::launch(Path::new(&registration.binary)) {
                eprintln!("[installmgr] {why}");
            }
        }
        send(Progress::Requested { key, result });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_new_update_source_is_kept_in_the_config_and_an_empty_one_removes_it() {
        let url = "https://example.org/{app}/{channel}.json";
        let mut updates = azul_appkit::azlin_config::UpdatesSection::default();
        let change = ConfigChange::Source(Some(url.to_string()));
        assert_eq!(change.describe(), format!("source {url}"));
        change.apply(&mut updates);
        assert_eq!(updates.manifest_url.as_deref(), Some(url));
        let change = ConfigChange::Source(None);
        assert_eq!(change.describe(), "source -");
        change.apply(&mut updates);
        assert_eq!(updates.manifest_url, None);
    }
}
