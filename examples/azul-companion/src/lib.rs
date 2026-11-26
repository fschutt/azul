//! AzCompanion: the AzReview phone companion.
pub mod env;
pub mod l10n;
pub mod theme;
pub mod layout;
pub mod pad;
pub mod sim;
pub mod ui;

use std::sync::{Arc, Mutex};

use azul::prelude::*;

use crate::env::{Lang, DEVICES};
use crate::sim::{mac::SimMac, phone::SimPhone, Extras};
use crate::ui::{glue, EnvSource, Root};

/// What the command line of the simulator says.
const HELP: &str = "companion_sim: the AzReview phone companion in a window\n\
  --device iphone|android-s|android-l   the phone (default iphone)\n\
  --landscape                           hold it sideways\n\
  --lang en|de                          the phone's language\n\
  --no-safe                             no notch and no home indicator\n\
  --shots DIR                           screenshots of the scenes into DIR (phone alone, 2x), with the layout check\n\
  --scenes SPEC                         which scenes: all, a screen letter (a b c d e) or names, comma separated\n\
  --prefix NAME                         the file name's first part (default: device-orientation-language)\n\
  --window-shot FILE                    one screenshot of the whole simulator window, then quit\n\
  --list-scenes                         the scenes and what they show\n";

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
    let phone = SimPhone::new(mac, commands);
    let mut root = Root::new(Box::new(phone), EnvSource::Fixed(extras.env()));
    // the clock starts a minute in: a scene can have a conversation that began long before it
    root.started = std::time::Instant::now().checked_sub(std::time::Duration::from_secs(60)).unwrap_or(root.started);
    let (w, h) = extras.window_size();
    root.sim = Some(extras);
    let mut config = AppConfig::create();
    config.set_mode(azul::option::OptionDarkLightMode::Some(azul::css::DarkLightMode::Dark));
    let app = App::create(RefAny::new(root), config);
    app.run(glue::window_options("AzReview Companion (simulator)", (w, h), 192));
}
