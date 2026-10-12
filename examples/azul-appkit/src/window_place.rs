//! Where an app's main window opens: maximized on the first start, then where and how big the
//! user left it.
//!
//! The place is kept per computer, in the data root's own bookkeeping folder
//! (`<data root>/.azlin/windows/<app folder>.json`): the data root's files travel to the
//! user's S3 bucket and to computers with other screens, its `.azlin` folder does not (a drive
//! never lists it). A scripted run (`--size`, `--shot`) opens at its size and leaves the file
//! alone.
//!
//! [`startup`] decides the first frame, [`observe`] what the window's state means for the file
//! while the app runs (a maximized or minimized window keeps the size un-maximizing returns to).

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

/// The window's place as the file keeps it.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct WindowPlace {
    /// The window's width when it is not maximized (logical px): what un-maximizing returns to.
    pub width: f32,
    /// The window's height when it is not maximized (logical px).
    pub height: f32,
    /// The window's top-left on the screen (physical px); `None`: where the OS puts it.
    #[serde(default)]
    pub x: Option<i32>,
    /// See [`WindowPlace::x`].
    #[serde(default)]
    pub y: Option<i32>,
    /// The window was maximized.
    #[serde(default)]
    pub maximized: bool,
}

/// How the main window opens.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Startup {
    /// Its size (logical px), the restored size of a maximized window.
    pub size: (f32, f32),
    /// Its top-left on the screen (physical px); `None`: where the OS puts it.
    pub position: Option<(i32, i32)>,
    /// It opens maximized.
    pub maximized: bool,
}

/// What the window looked like when the app last asked.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Seen {
    /// Neither maximized nor minimized: its size (logical px) and, where the OS tells it, its
    /// top-left (physical px).
    Normal {
        size: (f32, f32),
        position: Option<(i32, i32)>,
    },
    /// Maximized: its size is the screen's, not the user's.
    Maximized,
    /// Minimized or full screen: nothing to keep.
    Other,
}

/// The file of `app_folder`'s window place under `data_root`.
#[must_use]
pub fn place_path(data_root: &Path, app_folder: &str) -> PathBuf {
    data_root
        .join(".azlin")
        .join("windows")
        .join(format!("{app_folder}.json"))
}

impl WindowPlace {
    /// Reads a place; `None` for a file that is not one or holds no usable size.
    #[must_use]
    pub fn parse(json: &str) -> Option<WindowPlace> {
        let place: WindowPlace = serde_json::from_str(json).ok()?;
        let usable = |v: f32| v.is_finite() && v >= 1.0;
        (usable(place.width) && usable(place.height)).then_some(place)
    }

    /// The file's text.
    #[must_use]
    pub fn to_json(&self) -> String {
        serde_json::to_string_pretty(self).unwrap_or_default()
    }
}

/// How the main window opens: `scripted` (`--size`, `--shot`) at `size` (`--size`, else the
/// app's default); else where the user left it (`saved`); else, on the first start, maximized,
/// un-maximizing to `size`.
#[must_use]
pub fn startup(saved: Option<WindowPlace>, scripted: bool, size: (f32, f32)) -> Startup {
    match saved {
        _ if scripted => Startup {
            size,
            position: None,
            maximized: false,
        },
        Some(place) => Startup {
            size: (place.width, place.height),
            position: place.x.zip(place.y),
            maximized: place.maximized,
        },
        None => Startup {
            size,
            position: None,
            maximized: true,
        },
    }
}

/// The place to keep after the window was `seen`, from the one kept before (`previous`);
/// `fallback` is the size un-maximizing returns to when no normal size was ever seen.
#[must_use]
pub fn observe(previous: Option<WindowPlace>, seen: Seen, fallback: (f32, f32)) -> Option<WindowPlace> {
    match seen {
        Seen::Normal { size, position } => {
            let usable = |v: f32| v.is_finite() && v >= 1.0;
            if !(usable(size.0) && usable(size.1)) {
                return previous;
            }
            Some(WindowPlace {
                width: size.0,
                height: size.1,
                x: position.map(|p| p.0),
                y: position.map(|p| p.1),
                maximized: false,
            })
        }
        Seen::Maximized => Some(WindowPlace {
            maximized: true,
            ..previous.unwrap_or(WindowPlace {
                width: fallback.0,
                height: fallback.1,
                x: None,
                y: None,
                maximized: true,
            })
        }),
        Seen::Other => previous,
    }
}

/// Reads the place kept at `path`, if any.
#[must_use]
pub fn load(path: &Path) -> Option<WindowPlace> {
    WindowPlace::parse(&std::fs::read_to_string(path).ok()?)
}

/// Keeps `place` at `path`: written beside it, then moved over it, so a crash never leaves
/// half a file.
pub fn save(path: &Path, place: &WindowPlace) -> std::io::Result<()> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let partial = path.with_extension("json.partial");
    std::fs::write(&partial, place.to_json())?;
    std::fs::rename(&partial, path)
}

#[cfg(test)]
mod tests {
    use super::*;

    const DEFAULT: (f32, f32) = (1100.0, 720.0);

    fn normal(width: f32, height: f32, x: i32, y: i32) -> WindowPlace {
        WindowPlace {
            width,
            height,
            x: Some(x),
            y: Some(y),
            maximized: false,
        }
    }

    #[test]
    fn an_app_opens_maximized_on_its_first_start() {
        let first = startup(None, false, DEFAULT);
        assert!(first.maximized);
        assert_eq!(first.size, DEFAULT, "un-maximizing returns to the default size");
        assert_eq!(first.position, None, "the OS places the restored window");
    }

    #[test]
    fn a_scripted_run_opens_at_its_size_and_never_maximized() {
        let saved = Some(WindowPlace {
            maximized: true,
            ..normal(900.0, 600.0, 40, 50)
        });
        let run = startup(saved, true, (1280.0, 800.0));
        assert_eq!(
            run,
            Startup {
                size: (1280.0, 800.0),
                position: None,
                maximized: false
            }
        );
    }

    #[test]
    fn an_app_opens_where_and_as_big_as_the_user_left_it() {
        let run = startup(Some(normal(900.0, 600.0, 40, 50)), false, DEFAULT);
        assert_eq!(
            run,
            Startup {
                size: (900.0, 600.0),
                position: Some((40, 50)),
                maximized: false
            }
        );
        let maximized = startup(
            Some(WindowPlace {
                maximized: true,
                ..normal(900.0, 600.0, 40, 50)
            }),
            false,
            DEFAULT,
        );
        assert!(maximized.maximized);
        assert_eq!(maximized.size, (900.0, 600.0), "and un-maximizes to the size it had");
    }

    #[test]
    fn a_normal_window_is_kept_as_it_is_seen() {
        let seen = Seen::Normal {
            size: (800.0, 500.0),
            position: Some((10, 20)),
        };
        assert_eq!(observe(None, seen, DEFAULT), Some(normal(800.0, 500.0, 10, 20)));
        let moved = observe(Some(normal(800.0, 500.0, 10, 20)), Seen::Normal {
            size: (800.0, 500.0),
            position: Some((30, 40)),
        }, DEFAULT);
        assert_eq!(moved, Some(normal(800.0, 500.0, 30, 40)));
    }

    #[test]
    fn a_maximized_window_keeps_the_size_it_returns_to() {
        let kept = observe(Some(normal(800.0, 500.0, 10, 20)), Seen::Maximized, DEFAULT);
        assert_eq!(
            kept,
            Some(WindowPlace {
                maximized: true,
                ..normal(800.0, 500.0, 10, 20)
            })
        );
        let first = observe(None, Seen::Maximized, DEFAULT).expect("a maximized first start is kept");
        assert!(first.maximized);
        assert_eq!((first.width, first.height), DEFAULT);
        assert_eq!((first.x, first.y), (None, None));
    }

    #[test]
    fn a_minimized_window_changes_nothing() {
        let before = Some(normal(800.0, 500.0, 10, 20));
        assert_eq!(observe(before, Seen::Other, DEFAULT), before);
        assert_eq!(observe(None, Seen::Other, DEFAULT), None);
    }

    #[test]
    fn a_place_round_trips_and_a_broken_file_is_no_place() {
        let place = WindowPlace {
            maximized: true,
            ..normal(800.0, 500.0, -1200, 20)
        };
        assert_eq!(WindowPlace::parse(&place.to_json()), Some(place));
        assert_eq!(
            WindowPlace::parse(r#"{"width": 640, "height": 480}"#),
            Some(WindowPlace {
                width: 640.0,
                height: 480.0,
                x: None,
                y: None,
                maximized: false
            })
        );
        assert_eq!(WindowPlace::parse("not json"), None);
        assert_eq!(WindowPlace::parse(r#"{"width": 0, "height": 480}"#), None);
    }

    #[test]
    fn the_place_lives_in_the_data_roots_own_folder() {
        assert_eq!(
            place_path(Path::new("/data"), "drive"),
            Path::new("/data/.azlin/windows/drive.json")
        );
    }
}
