//! AzCompanion: the AzReview phone companion: the phone as the touch surface of a desk with a pen tablet.
//!
//! WHAT IT DOES: the pad shows the Mac's surface (the pages of the code review as a column of sheets with their breaks, or the
//! architecture whiteboard) scrolling under a frame that stands for what the Mac shows; a drag moves the content with the
//! finger (the Mac's setting "reverse the pan direction" flips it; the sign is `viewport::finger_to_pan`'s, said once), a pinch
//! zooms around the pen, two quick taps fit the width; four quick buttons send the user's recurring instructions; the files
//! of the workspace; "Continue reviewing" goes back to where the review was before the user jumped to an agent's conversation.
//!
//! WHAT IT CANNOT DO, and does not pretend to: it is a remote control that needs the connection, and a phone app that is
//! suspended cannot hold a LAN connection. Haptics (a success buzz when an agent is done, a warning when it has a suggestion
//! that waits) and the line about it work while the app is open. A sideloaded app with a free Apple account has no push
//! notifications (no APNs), so nothing reaches a closed app; an Android phone's own "touch feedback" setting still decides
//! whether a buzz is felt.
pub mod env;
pub mod l10n;
pub mod theme;
pub mod layout;
pub mod net;
pub mod offline;
pub mod pad;
pub mod padscene;
pub mod sim;
pub mod ui;

use std::sync::{Arc, Mutex};

use azul::prelude::*;

use crate::env::{Lang, DEVICES};
use crate::sim::{mac::SimMac, phone::SimPhone, Extras, Zoom};
use crate::ui::{glue, EnvSource, Root};

/// What the command line of the simulator says.
const HELP: &str = "companion_sim: the AzReview phone companion in a window\n\
  --device iphone|android-s|android-l   the phone (default iphone)\n\
  --landscape                           hold it sideways\n\
  --lang en|de                          the phone's language\n\
  --no-safe                             no notch and no home indicator\n\
  --zoom fit|PERCENT                    how big the phone is drawn: fit (default) is as big as the screen has room for\n\
  --shots DIR                           screenshots of the scenes into DIR (phone alone, 2x), with the layout check\n\
  --scenes SPEC                         which scenes: all, a screen letter (a b c d e) or names, comma separated\n\
  --prefix NAME                         the file name's first part (default: device-orientation-language)\n\
  --window-shot FILE                    one screenshot of the whole simulator window, then quit\n\
  --trace                               print every line of the panel's log too (for a script that drives the window)\n\
  --list-scenes                         the scenes and what they show\n";

/// The phone's app: the screens in the shape the phone is held, on the connection to AzReview over the local network (what the phone
/// keeps, and the client over azul's iroh endpoint: [`net`]). "Try it without a Mac" shows the rest of the screens. Runs until the app
/// is closed.
pub fn start() {
    // the screens start on a stand-in; the real connection is made when the window exists (glue::on_window_ready): an Android app has
    // no activity yet when this runs from the library's constructor
    let mut root = Root::new(Box::new(offline::OfflinePhone::new()), EnvSource::Window);
    root.connect_later = true;
    let mut config = AppConfig::create();
    config.set_mode(azul::option::OptionDarkLightMode::Some(azul::css::DarkLightMode::Dark));
    let app = App::create(RefAny::new(root), config);
    app.run(glue::phone_window_options("AzReview Companion"));
}

/// Android has no `main`: the system loads the library and calls into libazul, which reads the window options this puts
/// aside (it must run before that, which is what a load-time constructor is for).
#[cfg(target_os = "android")]
#[ctor::ctor]
fn azul_android_init() {
    start();
}

/// Runs the simulator with its command line.
pub fn run_sim(args: Vec<String>) {
    let mac = Arc::new(Mutex::new(SimMac::new()));
    let commands: sim::phone::Commands = Arc::new(Mutex::new(Vec::new()));
    let mut extras = Extras::new(mac.clone(), commands.clone());
    let (mut shots_dir, mut scene_spec, mut prefix, mut window_shot) = (None, String::from("all"), None, None);
    let mut it = args.into_iter();
    while let Some(arg) = it.next() {
        match arg.as_str() {
            "--landscape" => extras.landscape = true,
            "--no-safe" => extras.safe = false,
            "--trace" => mac.lock().map(|mut m| m.trace = true).unwrap_or(()),
            "--zoom" => match Zoom::parse(&it.next().unwrap_or_default()) {
                Ok(zoom) => extras.zoom = zoom,
                Err(said) => {
                    eprintln!("{said}");
                    std::process::exit(2);
                }
            },
            "--lang" => extras.lang = Lang::from_tag(&it.next().unwrap_or_default()),
            "--device" => {
                let id = it.next().unwrap_or_default();
                match DEVICES.iter().position(|d| d.id == id) {
                    Some(i) => extras.device = i,
                    None => {
                        eprintln!("no phone called {id:?}: {}", DEVICES.iter().map(|d| d.id).collect::<Vec<_>>().join(", "));
                        std::process::exit(2);
                    }
                }
            }
            "--shots" => shots_dir = it.next(),
            "--scenes" => scene_spec = it.next().unwrap_or_default(),
            "--prefix" => prefix = it.next(),
            "--window-shot" => window_shot = it.next(),
            "--list-scenes" => {
                for (name, what) in sim::scenes::ALL {
                    println!("{name}\t{what}");
                }
                return;
            }
            "--help" | "-h" => {
                print!("{HELP}");
                return;
            }
            other => {
                eprintln!("unknown option {other:?}\n{HELP}");
                std::process::exit(2);
            }
        }
    }
    if let Some(dir) = shots_dir {
        let names = sim::scenes::pick(&scene_spec);
        if names.is_empty() {
            eprintln!("no scene matches {scene_spec:?}");
            std::process::exit(2);
        }
        if let Err(e) = std::fs::create_dir_all(&dir) {
            eprintln!("cannot make {dir}: {e}");
            std::process::exit(2);
        }
        extras.chrome = false;
        let name = prefix.unwrap_or_else(|| {
            format!("{}-{}-{}", DEVICES[extras.device].id, if extras.landscape { "landscape" } else { "portrait" }, if extras.lang == Lang::De { "de" } else { "en" })
        });
        extras.player = Some(sim::shots::Player::new(dir.into(), name, names));
    } else if let Some(file) = window_shot {
        extras.player = Some(sim::shots::Player::window(file.into()));
    }
    let phone = SimPhone::new(mac.clone(), commands);
    let mut root = Root::new(Box::new(phone), EnvSource::Fixed(extras.env()));
    // the clock starts a minute in: a scene can have a conversation that began long before it
    root.started = std::time::Instant::now().checked_sub(std::time::Duration::from_secs(60)).unwrap_or(root.started);
    root.sim = Some(extras);
    let mut data = RefAny::new(root);
    let mut config = AppConfig::create();
    config.set_mode(azul::option::OptionDarkLightMode::Some(azul::css::DarkLightMode::Dark));
    let app = App::create(data.clone(), config);
    // the screen the window opens on: the phone is drawn as big as it has room for
    if let Some(mut root) = data.downcast_mut::<Root>() {
        if let Some(extras) = root.sim.as_mut().filter(|e| e.chrome && e.player.is_none()) {
            if let Some((width, height)) = primary_work_area(&app) {
                if let Some(said) = extras.set_work_area(width, height) {
                    eprintln!("companion_sim: {said}");
                }
            }
        }
    }
    let size = data.downcast_ref::<Root>().and_then(|root| root.sim.as_ref().map(Extras::window_size)).unwrap_or((400.0, 800.0));
    if mac.lock().is_ok_and(|m| m.trace) {
        if let Some(root) = data.downcast_ref::<Root>() {
            if let Some(extras) = root.sim.as_ref() {
                println!("{}", sim::window_line(size, extras.scale(), extras.work_area));
            }
        }
    }
    app.run(glue::window_options("AzReview Companion (simulator)", size, 192));
}

/// The usable part of the primary screen in logical pixels (no menu bar, no Dock), when the system tells it.
fn primary_work_area(app: &App) -> Option<(f32, f32)> {
    let monitors = app.get_monitors();
    let list = monitors.as_slice();
    let monitor = list.iter().find(|m| m.is_primary_monitor).or_else(|| list.first())?;
    Some((monitor.work_area.size.width as f32, monitor.work_area.size.height as f32))
}
