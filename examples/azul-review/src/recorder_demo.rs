//! The recorder in a window of its own: the window of `recorder_ui` (the indicator, the buttons, the camera's
//! preview, the list of recordings) over a recorder with the real clock, the real disk and azul's encoders. It
//! is the recorder's own test window until the review window has its menu entry for it: everything it does goes
//! through [`RecorderHost`] and [`crate::recorder_ui`], the very calls the review window makes.
//!
//! `recorder_demo [--data-dir DIR] [--language en|de]`. Recordings go to `review/recordings` of the data folder.
//! stdout, for scripts: `RECDEMO_RECOVERED <n>` (recordings put right at the start), `RECDEMO_READY <folder>`.

use std::path::PathBuf;

use azul::prelude::*;
use azul_appkit::{args::AppArgs, l10n::begin_layout, ui as kit};

use crate::{
    recorder::Deps,
    recorder_azul::AzulFactory,
    recorder_clock::{Clock, SessionClock},
    recorder_host::{Facts, RecorderHost},
    recorder_manifest::{AppInfo, HostInfo, Workspace},
    recorder_settings::RecorderSettings,
    recorder_ui::{self, Host, RecorderApp},
};

struct Demo {
    kit: RefAny,
    recorder: RecorderApp,
}

fn with_recorder(app: &mut RefAny, f: &mut dyn FnMut(&mut RecorderApp)) -> bool {
    match app.downcast_mut::<Demo>() {
        Some(mut d) => {
            f(&mut d.recorder);
            true
        }
        None => false,
    }
}

extern "C" fn layout(data: RefAny, info: LayoutCallbackInfo) -> Dom {
    begin_layout(&info);
    recorder_ui::hud(&Host { app: data, with: with_recorder }, &info)
}

extern "C" fn on_window_created(mut data: RefAny, mut info: CallbackInfo) -> Update {
    let kit_ref = data.downcast_ref::<Demo>().map(|d| d.kit.clone());
    if let Some(kit_ref) = kit_ref {
        kit::on_window_created(&kit_ref, &mut info);
    }
    Update::DoNothing
}

/// The real machine under the recorder: the wall clock, a session clock that starts at the session's own
/// start, the free space of the disk, azul's encoders.
fn real_deps() -> Deps {
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

/// `+0100` / `-0530` as minutes; `None` for anything else.
#[must_use]
pub fn parse_utc_offset(text: &str) -> Option<i32> {
    let (sign, digits) = match text.as_bytes().first()? {
        b'+' => (1, &text[1..]),
        b'-' => (-1, &text[1..]),
        _ => return None,
    };
    if digits.len() != 4 || !digits.bytes().all(|c| c.is_ascii_digit()) {
        return None;
    }
    let (hours, minutes): (i32, i32) = (digits[..2].parse().ok()?, digits[2..].parse().ok()?);
    (hours <= 14 && minutes < 60).then_some(sign * (hours * 60 + minutes))
}

/// Starts the demo window.
pub fn run() {
    let args = match AppArgs::parse(&crate::SPEC, std::env::args().skip(1).collect::<Vec<String>>()) {
        Ok(a) => a,
        Err(why) => {
            eprintln!("{why}");
            std::process::exit(2);
        }
    };
    let kit_ref = kit::create_kit(crate::SPEC, crate::ABOUT, &crate::SHORTCUTS, &[], args);
    let mut config = kit::app_config(&kit_ref);
    crate::l10n::register(&mut config);
    let data_root: PathBuf = {
        let mut k = kit_ref.clone();
        k.downcast_ref::<kit::Kit>().map(|k| k.data_root.clone()).unwrap_or_default()
    };
    // The environment is read once, here, and handed down.
    let term = std::env::var("TERM_PROGRAM").ok();
    let exe = std::env::current_exe().map(|p| p.to_string_lossy().into_owned()).unwrap_or_default();
    let cwd = std::env::current_dir().unwrap_or_default();
    let facts = Facts {
        app: AppInfo { name: String::from(crate::ABOUT.name), version: String::from(crate::ABOUT.version) },
        workspace: Workspace { root: cwd.to_string_lossy().into_owned(), name: cwd.file_name().map_or_else(String::new, |n| n.to_string_lossy().into_owned()) },
        host: HostInfo { os: String::from(std::env::consts::OS), arch: String::from(std::env::consts::ARCH) },
        utc_offset_minutes: local_utc_offset_minutes(),
        recordings_dir: data_root.join("review").join("recordings"),
        who: crate::recorder_panel::attribution(&exe, term.as_deref()),
        decimal: '.',
        hotkey: String::from(recorder_ui::pause_hotkey_label()),
    };
    let dir = facts.recordings_dir.clone();
    let mut host = RecorderHost::new(RecorderSettings::default(), facts, real_deps());
    let recovered = host.recover();
    println!("RECDEMO_RECOVERED {recovered}");
    host.refresh();
    println!("RECDEMO_READY {}", dir.display());
    let state = Demo { kit: kit_ref.clone(), recorder: RecorderApp::new(host) };
    let app = App::create(RefAny::new(state), config);
    let mut window = recorder_ui::window(layout);
    window.create_callback = Some(Callback::create(on_window_created)).into();
    app.run(window);
}

#[cfg(test)]
#[path = "recorder_demo_test.rs"]
mod recorder_demo_test;
