//! The recorder in a window of its own: the window of `recorder_ui` (the indicator, the buttons, the camera's
//! preview, the list of recordings) over a recorder with the real clock, the real disk and azul's encoders. It
//! is the recorder's own test window until the review window has its menu entry for it: everything it does goes
//! through [`RecorderHost`] and [`crate::recorder_ui`], the very calls the review window makes.
//!
//! `recorder_demo [--record screen,camera,mic] [--data-dir DIR] [--language en|de]`. Recordings go to `review/recordings` of
//! the data folder; `--record` says which of the three the recorder takes (all, when it is not given).
//! stdout, for scripts: `RECDEMO_RECOVERED <n>` (recordings put right at the start), `RECDEMO_READY <folder>`.

use std::path::PathBuf;

use azul::prelude::*;
use azul_appkit::{args::AppArgs, l10n::begin_layout, ui as kit};

use crate::{
    recorder_args::own_arguments,
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

/// Starts the demo window: `recorder_demo [--record screen,camera,mic] [--data-dir DIR] [--language en|de]`.
pub fn run() {
    let (record, rest) = match own_arguments(std::env::args().skip(1).collect()) {
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
    let state = Demo { kit: kit_ref.clone(), recorder: RecorderApp::new(host) };
    let app = App::create(RefAny::new(state), config);
    let mut window = recorder_ui::window(layout);
    window.create_callback = Some(Callback::create(on_window_created)).into();
    app.run(window);
}
