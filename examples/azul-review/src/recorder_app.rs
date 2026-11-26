//! The recorder as the app builds it: a [`RecorderHost`] over the real machine (the wall clock, a session clock, the
//! free space of the disk, azul's encoders) and the facts of the machine it runs on. Built once, at the start,
//! where the environment is read; nothing here opens a device or starts a thread (that is a recording's start).

use std::path::Path;

use crate::{
    recorder::Deps,
    recorder_args::parse_utc_offset,
    recorder_azul::AzulFactory,
    recorder_clock::{Clock, SessionClock},
    recorder_host::{Facts, RecorderHost},
    recorder_manifest::{AppInfo, HostInfo, Workspace},
    recorder_settings::RecorderSettings,
};

/// The real machine under the recorder: the wall clock, a session clock that starts at the session's own start,
/// the free space of the disk, azul's encoders.
#[must_use]
pub fn real_deps() -> Deps {
    Deps {
        unix_ms: Box::new(|| std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| u64::try_from(d.as_millis()).unwrap_or(u64::MAX))),
        make_clock: Box::new(|origin_unix_ms| std::sync::Arc::new(SessionClock::start(origin_unix_ms)) as std::sync::Arc<dyn Clock>),
        free_bytes: Box::new(|path| {
            // (The folder may not exist yet: the volume it would be made on.)
            let probe = azul::file::FilePath::create(azul::str::String::from(path.to_string_lossy().as_ref()));
            probe.disk_space_for_new().into_option().map(|space| space.free)
        }),
        factory: Box::new(AzulFactory),
    }
}

/// The local time's offset from UTC in minutes: what `date +%z` says (`+0100`), once, at the start; 0 where there
/// is no `date` (the sessions are then named by UTC).
#[must_use]
pub fn local_utc_offset_minutes() -> i32 {
    std::process::Command::new("date")
        .arg("+%z")
        .output()
        .ok()
        .and_then(|out| parse_utc_offset(String::from_utf8_lossy(&out.stdout).trim()))
        .unwrap_or(0)
}

/// A recorder host for `settings`: the recordings go to `recordings` unless the settings name a folder, the
/// review is of `workspace`, and `decimal` is the language's decimal mark. The environment is read here, once: the
/// terminal the program was started from and the program's own path say whom the system holds responsible for
/// the permissions.
#[must_use]
pub fn host(settings: RecorderSettings, recordings: &Path, workspace: &Path, decimal: char) -> RecorderHost {
    let term = std::env::var("TERM_PROGRAM").ok();
    let exe = std::env::current_exe().map(|p| p.to_string_lossy().into_owned()).unwrap_or_default();
    let facts = Facts {
        app: AppInfo { name: String::from(crate::ABOUT.name), version: String::from(crate::ABOUT.version) },
        workspace: Workspace { root: workspace.to_string_lossy().into_owned(), name: workspace.file_name().map_or_else(String::new, |n| n.to_string_lossy().into_owned()) },
        host: HostInfo { os: String::from(std::env::consts::OS), arch: String::from(std::env::consts::ARCH) },
        utc_offset_minutes: local_utc_offset_minutes(),
        recordings_dir: recordings.to_path_buf(),
        who: crate::recorder_panel::attribution(&exe, term.as_deref()),
        decimal,
        hotkey: String::from(crate::recorder_ui::pause_hotkey_label()),
    };
    RecorderHost::new(settings, facts, real_deps())
}
