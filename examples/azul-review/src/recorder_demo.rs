//! The recorder in a window of its own: the window of `recorder_ui` (the indicator, the buttons, the camera's
//! preview, the list of recordings) over a recorder with the real clock, the real disk and azul's encoders. It
//! is the recorder's own test window until the review window has its menu entry for it: everything it does goes
//! through [`RecorderHost`] and [`crate::recorder_ui`], the very calls the review window makes.
//!
//! `recorder_demo [--record screen,camera,mic] [--auto SECONDS] [--data-dir DIR] [--language en|de]`. Recordings go to
//! `review/recordings` of the data folder; `--record` says which of the three the recorder takes (all, when it is not
//! given). `--auto` makes it record on its own, for a run with no one to press the button: it starts when the window
//! is up, stops after SECONDS (2 to 600) and closes the window.
//! stdout, for scripts: `RECDEMO_RECOVERED <n>` (recordings put right at the start), `RECDEMO_READY <folder>`, and
//! with `--auto` `RECDEMO_STARTED <folder>`, then `RECDEMO_STOPPED <folder>` (or `RECDEMO_FAILED <why>` when it did not start).

use std::{
    path::PathBuf,
    time::{Duration as StdDuration, Instant},
};

use azul::{prelude::*, task::TerminateTimer, time::SystemTimeDiff};
use azul_appkit::{
    args::AppArgs,
    l10n::{begin_layout, t_text},
    ui as kit,
};

use crate::{
    recorder_args::{own_arguments, take_auto},
    recorder_settings::RecorderSettings,
    recorder_ui::{self, Host, RecorderApp},
};

struct Demo {
    kit: RefAny,
    recorder: RecorderApp,
    /// A recording on its own (`--auto`): how long it lasts.
    auto: Option<u64>,
    /// When it began.
    since: Option<Instant>,
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
    let (kit_ref, auto) = data.downcast_ref::<Demo>().map_or((None, None), |d| (Some(d.kit.clone()), d.auto));
    if let Some(kit_ref) = kit_ref {
        kit::on_window_created(&kit_ref, &mut info);
    }
    if auto.is_none() {
        return Update::DoNothing;
    }
    // A recording on its own: started with the call the button makes, stopped by a timer when its time is up.
    let host = Host { app: data.clone(), with: with_recorder };
    if recorder_ui::start_now(&host, &mut info) {
        if let Some(mut d) = data.downcast_mut::<Demo>() {
            d.since = Some(Instant::now());
            let folder = d.recorder.host.current().and_then(|name| d.recorder.host.path_of(&name));
            println!("RECDEMO_STARTED {}", folder.map_or_else(String::new, |p| p.display().to_string()));
        }
        let timer = Timer::create(data.clone(), on_auto_tick, info.get_system_time_fn()).with_interval(Duration::System(SystemTimeDiff::from_millis(250)));
        info.add_timer(TimerId::unique(), timer);
    } else {
        let why = data.downcast_ref::<Demo>().and_then(|d| d.recorder.host.error().map(t_text)).unwrap_or_default();
        println!("RECDEMO_FAILED {why}");
        info.close_window();
    }
    Update::RefreshDom
}

/// The time of a recording on its own is up: it is stopped (its files are finished) and the window closes.
extern "C" fn on_auto_tick(mut data: RefAny, mut info: TimerCallbackInfo) -> TimerCallbackReturn {
    let up = data
        .downcast_ref::<Demo>()
        .is_none_or(|d| matches!((d.auto, d.since), (Some(seconds), Some(since)) if since.elapsed() >= StdDuration::from_secs(seconds)));
    if !up {
        return TimerCallbackReturn { should_update: Update::DoNothing, should_terminate: TerminateTimer::Continue };
    }
    if let Some(mut d) = data.downcast_mut::<Demo>() {
        let folder = d.recorder.host.current().and_then(|name| d.recorder.host.path_of(&name));
        d.recorder.host.stop();
        println!("RECDEMO_STOPPED {}", folder.map_or_else(String::new, |p| p.display().to_string()));
    }
    info.callback_info.close_window();
    TimerCallbackReturn { should_update: Update::DoNothing, should_terminate: TerminateTimer::Terminate }
}

/// Starts the demo window: `recorder_demo [--record screen,camera,mic] [--auto SECONDS] [--data-dir DIR] [--language en|de]`.
pub fn run() {
    let (auto, args) = match take_auto(std::env::args().skip(1).collect()) {
        Ok(parsed) => parsed,
        Err(why) => {
            eprintln!("{why}");
            std::process::exit(2);
        }
    };
    if let Some(note) = auto.as_ref().and_then(|a| a.note.as_deref()) {
        eprintln!("{note}");
    }
    let (record, rest) = match own_arguments(args) {
        Ok(parsed) => parsed,
        Err(why) => {
            eprintln!("{why}");
            std::process::exit(2);
        }
    };
    let args = match AppArgs::parse(&crate::SPEC, rest) {
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
    let cwd = std::env::current_dir().unwrap_or_default();
    let mut settings = RecorderSettings::default();
    if let Some((screen, camera, microphone)) = record {
        (settings.screen, settings.camera, settings.microphone) = (screen, camera, microphone);
    }
    let dir = data_root.join("review").join("recordings");
    let mut host = crate::recorder_app::host(settings, &dir, &cwd, '.');
    let recovered = host.recover();
    println!("RECDEMO_RECOVERED {recovered}");
    host.refresh();
    println!("RECDEMO_READY {}", dir.display());
    let state = Demo { kit: kit_ref.clone(), recorder: RecorderApp::new(host), auto: auto.map(|a| a.seconds), since: None };
    let app = App::create(RefAny::new(state), config);
    let mut window = recorder_ui::window(layout);
    window.create_callback = Some(Callback::create(on_window_created)).into();
    app.run(window);
}
