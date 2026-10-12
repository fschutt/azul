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

use crate::model::{self, AppEntry, Fetching, Installed, Status};

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
}

impl Job {
    /// Whether the job is the quiet poll of the apps' requests (it runs beside the others and
    /// does not hold the buttons).
    pub(crate) fn is_requests(&self) -> bool {
        matches!(self, Job::Requests)
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
    /// The job is done; `requests`: it was the poll of the apps' requests.
    Finished { requests: bool },
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
    let requests = job.is_requests();
    let mut send = |progress: Progress| {
        let _sent = sender.send(ThreadReceiveMsg::WriteBack(ThreadWriteBackMsg::create(
            crate::on_progress,
            RefAny::new(progress),
        )));
    };
    run(&AzulEngine, job, &fetching, &mut send);
    send(Progress::Finished { requests });
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

/// The apps' "Update now": for each request whose app has quit, the staged version it said
/// yes to is checked once more and installed, and the app starts again when it asked. A
/// request of an app still running waits for the next poll; one whose version is no longer
/// staged is answered with why.
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
            None => Err(format!(
                "{} {} is no longer waiting to be installed; check for updates again",
                entry.name, request.version
            )),
        };
        if let (Ok(registration), true) = (&result, request.relaunch) {
            if let Err(why) = model::launch(Path::new(&registration.binary)) {
                eprintln!("[installmgr] {why}");
            }
        }
        send(Progress::Requested { key, result });
    }
}
