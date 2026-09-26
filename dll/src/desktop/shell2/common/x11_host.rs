//! The X11 backend on a host whose own toolkit is something else.
//!
//! `shell2/linux/x11` is the Linux X11 backend. With the opt-in `x11-macos`
//! feature it is ALSO compiled into the macOS build, and `AZ_BACKEND=x11` (or
//! `AZ_WINDOW=x11`) then runs that same backend - the same `X11Window`, the
//! same window loop in `run.rs` - against XQuartz instead of opening AppKit
//! windows. The point is reach: an X11 bug reproduces on a Mac, and because
//! an XQuartz window is a real macOS window the Mac's own tools
//! (`scripts/cgevent_input.py`, `screencapture`) drive and capture it. The
//! how-to is the "X11 on macOS" section of `doc/guide/en/debugging.md`.
//!
//! Whether the backend is compiled in at all is the build-script cfg
//! `az_x11` (Linux always; macOS with `x11-macos`). What differs by HOST lives
//! here, as data and small pure functions, so it compiles - and its tests run -
//! in every build, with or without the backend. A file gated to the backend is
//! a file whose tests never run on the machine that has no backend:
//!
//! - [`library_candidates`]: where each library of the X11 stack is found.
//! - [`host_windowing`]: whether the environment asks a macOS build for X11.
//! - [`active`]: whether X11 drives THIS process's windows on a non-Linux host - what the few
//!   window-system policies above the backend consult instead of `target_os`.

use core::sync::atomic::{AtomicBool, Ordering};

// ============================================================================
// Library names
// ============================================================================

/// One library of the X11 stack, as the X11 backend and its helpers load it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) enum X11Lib {
    /// libX11: the protocol client everything else is built on.
    X11,
    /// libXi: XInput2 (touch, pen, smooth scroll, per-device keyboards).
    Xi,
    /// libXext: XShape, for windows shaped by their alpha.
    Xext,
    /// libXrender: ARGB visual detection.
    Xrender,
    /// libXrandr: monitor enumeration and screen-change events.
    Xrandr,
    /// libX11-xcb: the XCB connection under an Xlib display.
    X11Xcb,
    /// libxkbcommon: compose sequences and per-seat keymaps.
    XkbCommon,
    /// libxkbcommon-x11: keymaps built from an X input device.
    XkbCommonX11,
    /// libEGL: the GPU path's context.
    Egl,
    /// libGL: core GL entry points `eglGetProcAddress` does not hand out.
    Gl,
    /// libgtk-3: the IME cursor-rectangle bridge.
    Gtk3,
}

/// Whose file naming the loader speaks.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum LibHost {
    /// ELF sonames, resolved by `ld.so` through the distribution's paths.
    Linux,
    /// Mach-O dylibs, as XQuartz installs them under `/opt/X11/lib`.
    MacOs,
}

impl LibHost {
    /// The host this binary runs on.
    #[must_use]
    pub(crate) const fn current() -> Self {
        if cfg!(target_os = "macos") {
            Self::MacOs
        } else {
            Self::Linux
        }
    }
}

/// The names to hand `dlopen`, in order, for `lib` on `host`.
///
/// **Linux** keeps exactly the sonames the loaders always used.
///
/// **macOS** asks for XQuartz's copy by full path first, then by leaf name.
/// The path is not a shortcut around `DYLD_LIBRARY_PATH`: dyld consults that
/// variable for the LEAF name even when it is handed a path, so an override
/// still wins, and the leaf-name entry covers `DYLD_FALLBACK_LIBRARY_PATH`.
///
/// Every library that is handed a `Display*` comes from XQuartz only, never
/// from Homebrew. Each of them links its own libX11 by install name; loading
/// Homebrew's libXi next to XQuartz's libX11 puts two copies of Xlib in the
/// process, and the second one reads the first one's `Display` struct. Only
/// libxkbcommon, which never talks to an X server, may also come from
/// Homebrew (`brew install libxkbcommon`); XQuartz ships none.
///
/// Empty means "not on this host": GTK on macOS is the Quartz build, and its
/// input-method context knows nothing about X.
#[must_use]
pub(crate) fn library_candidates(lib: X11Lib, host: LibHost) -> &'static [&'static str] {
    match host {
        LibHost::Linux => match lib {
            X11Lib::X11 => &["libX11.so.6", "libX11.so"],
            X11Lib::Xi => &["libXi.so.6", "libXi.so"],
            X11Lib::Xext => &["libXext.so.6", "libXext.so"],
            X11Lib::Xrender => &["libXrender.so.1", "libXrender.so"],
            X11Lib::Xrandr => &["libXrandr.so.2", "libXrandr.so"],
            X11Lib::X11Xcb => &["libX11-xcb.so.1"],
            X11Lib::XkbCommon => &["libxkbcommon.so.0"],
            X11Lib::XkbCommonX11 => &["libxkbcommon-x11.so.0"],
            X11Lib::Egl => &["libEGL.so.1", "libEGL.so"],
            X11Lib::Gl => &["libGL.so.1"],
            X11Lib::Gtk3 => &["libgtk-3.so.0", "libgtk-3.so"],
        },
        LibHost::MacOs => match lib {
            X11Lib::X11 => &["/opt/X11/lib/libX11.6.dylib", "libX11.6.dylib"],
            X11Lib::Xi => &["/opt/X11/lib/libXi.6.dylib", "libXi.6.dylib"],
            X11Lib::Xext => &["/opt/X11/lib/libXext.6.dylib", "libXext.6.dylib"],
            X11Lib::Xrender => &["/opt/X11/lib/libXrender.1.dylib", "libXrender.1.dylib"],
            X11Lib::Xrandr => &["/opt/X11/lib/libXrandr.2.dylib", "libXrandr.2.dylib"],
            X11Lib::X11Xcb => &["/opt/X11/lib/libX11-xcb.1.dylib", "libX11-xcb.1.dylib"],
            X11Lib::XkbCommon => &[
                "/opt/X11/lib/libxkbcommon.0.dylib",
                "libxkbcommon.0.dylib",
                "/opt/homebrew/lib/libxkbcommon.0.dylib",
                "/usr/local/lib/libxkbcommon.0.dylib",
            ],
            X11Lib::XkbCommonX11 => &[
                "/opt/X11/lib/libxkbcommon-x11.0.dylib",
                "libxkbcommon-x11.0.dylib",
            ],
            X11Lib::Egl => &["/opt/X11/lib/libEGL.1.dylib", "libEGL.1.dylib"],
            X11Lib::Gl => &["/opt/X11/lib/libGL.1.dylib", "libGL.1.dylib"],
            X11Lib::Gtk3 => &[],
        },
    }
}

/// [`library_candidates`] for the host this binary runs on.
#[must_use]
pub(crate) fn candidates(lib: X11Lib) -> &'static [&'static str] {
    library_candidates(lib, LibHost::current())
}

// ============================================================================
// Which windowing system a macOS build was asked for
// ============================================================================

/// What the environment asks a macOS build to open its windows with.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum HostWindowing {
    /// AppKit: the default, and the answer to `auto` or to nothing at all.
    Native,
    /// The X11 backend, against XQuartz. Needs the `x11-macos` feature.
    X11,
    /// A windowing value this host cannot honour (`wayland`, or an `AZ_WINDOW`
    /// that names no windowing system). Carried for the warning; the window
    /// still opens natively.
    Unsupported(String),
}

/// Read the windowing request the way `LinuxWindow::select_backend` does on
/// Linux: `AZ_WINDOW` wins, then the legacy `AZ_BACKEND=x11|wayland`, whose
/// RENDER values (`cpu`, `gpu`, `auto`, `headless`, `web://...`) are a
/// different axis and are skipped. Matching ignores case and nothing else -
/// no trimming, an empty `AZ_WINDOW` is still an `AZ_WINDOW` - so whenever
/// this answers `X11`, `select_backend` in the X11 loop it hands over to
/// answers `X11` too.
///
/// What is deliberately missing is Linux's auto-detection. Linux picks X11
/// when `DISPLAY` is set; on a Mac with XQuartz installed its launchd agent
/// exports `DISPLAY` to every process in the login session, so it says nothing
/// about what this app wants. Only an explicit `x11` leaves AppKit.
#[must_use]
pub(crate) fn host_windowing(az_window: Option<&str>, az_backend: Option<&str>) -> HostWindowing {
    let legacy =
        az_backend.filter(|v| v.eq_ignore_ascii_case("x11") || v.eq_ignore_ascii_case("wayland"));
    match az_window.or(legacy) {
        None => HostWindowing::Native,
        Some(v) if v.eq_ignore_ascii_case("auto") => HostWindowing::Native,
        Some(v) if v.eq_ignore_ascii_case("x11") => HostWindowing::X11,
        Some(v) => HostWindowing::Unsupported(v.to_owned()),
    }
}

/// [`host_windowing`] over this process's environment.
pub(crate) fn host_windowing_from_env() -> HostWindowing {
    let az_window = std::env::var("AZ_WINDOW").ok();
    let az_backend = std::env::var("AZ_BACKEND").ok();
    host_windowing(az_window.as_deref(), az_backend.as_deref())
}

// ============================================================================
// Is X11 driving this process?
// ============================================================================

/// Set once, by the macOS `run()`, when it hands the process to the X11 loop.
static ACTIVE: AtomicBool = AtomicBool::new(false);

/// Hand this process's windows to the X11 backend on a non-Linux host.
///
/// Called before the first window exists: the monitor list, the frame rules
/// and the menu bar of that very first window already read [`active`]. The
/// keyboard shortcuts switch with them - Ctrl, not Cmd, is the primary
/// modifier of an X11 window (`azul_core::window::mac_shortcut_conventions`).
pub(crate) fn activate() {
    ACTIVE.store(true, Ordering::Release);
    azul_core::window::use_linux_shortcuts_on_macos();
}

/// Is X11 drawing this process's windows on a host whose native toolkit is
/// something else?
///
/// A handful of choices above the backend are made per `target_os` but are
/// really about the WINDOW SYSTEM: which monitor list sizes a window
/// (`display.rs`), whether a frame can show its controls without a title
/// (`csd.rs`), whether the window carries a software menu bar (`layout.rs`).
/// Those ask this instead, so an X11 window on a Mac gets the X11 answer.
///
/// Constant `false` on Linux, where X11 is native and the Linux answers are
/// compiled in, and in every build without the backend.
pub(crate) fn active() -> bool {
    cfg!(all(az_x11, not(target_os = "linux"))) && ACTIVE.load(Ordering::Acquire)
}

/// Do this process's windows follow the Linux desktop's window rules - X11's
/// and Wayland's frames, the software menu bar? Always on Linux; elsewhere
/// exactly while [`active`].
pub(crate) fn linux_window_rules() -> bool {
    cfg!(target_os = "linux") || active()
}

#[cfg(test)]
mod tests {
    use super::{host_windowing, library_candidates, HostWindowing, LibHost, X11Lib};

    const ALL: [X11Lib; 11] = [
        X11Lib::X11,
        X11Lib::Xi,
        X11Lib::Xext,
        X11Lib::Xrender,
        X11Lib::Xrandr,
        X11Lib::X11Xcb,
        X11Lib::XkbCommon,
        X11Lib::XkbCommonX11,
        X11Lib::Egl,
        X11Lib::Gl,
        X11Lib::Gtk3,
    ];

    /// The Linux lists are the sonames the loaders hard-coded before the
    /// table existed. Moving them here must not change a single lookup on
    /// the platform the backend was written for.
    #[test]
    fn linux_keeps_the_sonames_it_always_loaded() {
        let linux = |lib| library_candidates(lib, LibHost::Linux);
        assert_eq!(linux(X11Lib::X11), ["libX11.so.6", "libX11.so"]);
        assert_eq!(linux(X11Lib::Xi), ["libXi.so.6", "libXi.so"]);
        assert_eq!(linux(X11Lib::Xext), ["libXext.so.6", "libXext.so"]);
        assert_eq!(linux(X11Lib::Xrender), ["libXrender.so.1", "libXrender.so"]);
        assert_eq!(linux(X11Lib::Xrandr), ["libXrandr.so.2", "libXrandr.so"]);
        assert_eq!(linux(X11Lib::X11Xcb), ["libX11-xcb.so.1"]);
        assert_eq!(linux(X11Lib::XkbCommon), ["libxkbcommon.so.0"]);
        assert_eq!(linux(X11Lib::XkbCommonX11), ["libxkbcommon-x11.so.0"]);
        assert_eq!(linux(X11Lib::Egl), ["libEGL.so.1", "libEGL.so"]);
        assert_eq!(linux(X11Lib::Gl), ["libGL.so.1"]);
        assert_eq!(linux(X11Lib::Gtk3), ["libgtk-3.so.0", "libgtk-3.so"]);
    }

    /// XQuartz first, by full path, then the leaf name for a `DYLD_*` path.
    /// A Linux soname can never load on macOS, so none may leak into the list.
    #[test]
    fn macos_asks_for_xquartz_first_then_the_leaf_name() {
        let x11 = library_candidates(X11Lib::X11, LibHost::MacOs);
        assert_eq!(x11, ["/opt/X11/lib/libX11.6.dylib", "libX11.6.dylib"]);

        for lib in ALL {
            let names = library_candidates(lib, LibHost::MacOs);
            assert!(
                names.iter().all(|n| n.ends_with(".dylib")),
                "{lib:?}: {names:?} carries a non-dylib name"
            );
            if let Some(first) = names.first() {
                assert!(
                    first.starts_with("/opt/X11/lib/"),
                    "{lib:?}: XQuartz's copy must be tried first, got {first}"
                );
                let leaf = first.rsplit('/').next().unwrap_or_default();
                assert_eq!(
                    names.get(1).copied(),
                    Some(leaf),
                    "{lib:?}: the leaf name must follow the path, for DYLD_LIBRARY_PATH"
                );
            }
        }
    }

    /// Two copies of Xlib in one process read each other's `Display` structs.
    /// Anything that is handed a `Display*` therefore comes from XQuartz or
    /// from a `DYLD_*` path the user chose - never from a guessed Homebrew
    /// prefix. xkbcommon talks to no X server and is the one exception.
    #[test]
    fn nothing_that_takes_a_display_is_taken_from_homebrew() {
        for lib in ALL {
            if lib == X11Lib::XkbCommon {
                continue;
            }
            let names = library_candidates(lib, LibHost::MacOs);
            assert!(
                names
                    .iter()
                    .all(|n| !n.starts_with('/') || n.starts_with("/opt/X11/lib/")),
                "{lib:?}: {names:?}"
            );
        }
        let xkb = library_candidates(X11Lib::XkbCommon, LibHost::MacOs);
        assert!(xkb.contains(&"/opt/homebrew/lib/libxkbcommon.0.dylib"));
    }

    /// GTK on macOS is the Quartz build: an IM context from it cannot see X
    /// key events. Nothing is loaded rather than something wrong.
    #[test]
    fn gtk_is_not_loaded_on_macos() {
        assert!(library_candidates(X11Lib::Gtk3, LibHost::MacOs).is_empty());
        assert!(!library_candidates(X11Lib::Gtk3, LibHost::Linux).is_empty());
    }

    #[test]
    fn nothing_set_opens_appkit() {
        assert_eq!(host_windowing(None, None), HostWindowing::Native);
        assert_eq!(host_windowing(None, Some("cpu")), HostWindowing::Native);
        assert_eq!(host_windowing(Some("auto"), None), HostWindowing::Native);
    }

    /// The vocabulary is Linux's: `AZ_BACKEND=x11` is the legacy spelling,
    /// `AZ_WINDOW=x11` the current one, and case does not matter.
    #[test]
    fn x11_is_asked_for_by_either_variable() {
        assert_eq!(host_windowing(None, Some("x11")), HostWindowing::X11);
        assert_eq!(host_windowing(None, Some("X11")), HostWindowing::X11);
        assert_eq!(host_windowing(Some("x11"), None), HostWindowing::X11);
    }

    /// `AZ_WINDOW` wins, as on Linux: an explicit `auto` keeps AppKit even
    /// when the legacy variable says `x11`, and the two axes combine - X11
    /// windows with a GPU render request.
    #[test]
    fn az_window_wins_over_the_legacy_variable() {
        assert_eq!(
            host_windowing(Some("auto"), Some("x11")),
            HostWindowing::Native
        );
        assert_eq!(host_windowing(Some("x11"), Some("gpu")), HostWindowing::X11);
    }

    /// `AZ_BACKEND`'s render values say nothing about windowing; they are not
    /// a request for anything this function decides.
    #[test]
    fn a_render_value_is_not_a_windowing_request() {
        for render in ["cpu", "gpu", "auto", "headless", "web://127.0.0.1:8080"] {
            assert_eq!(
                host_windowing(None, Some(render)),
                HostWindowing::Native,
                "{render}"
            );
        }
    }

    /// Wayland is the Linux desktop's protocol: asked for on a Mac, it is
    /// named in the warning and the window opens natively.
    #[test]
    fn wayland_and_unknown_values_are_reported_not_honoured() {
        assert_eq!(
            host_windowing(None, Some("wayland")),
            HostWindowing::Unsupported("wayland".into())
        );
        assert_eq!(
            host_windowing(Some("cocoa"), None),
            HostWindowing::Unsupported("cocoa".into())
        );
        // Parsed exactly as `select_backend` parses it, where an empty or
        // padded `AZ_WINDOW` is an invalid request too - never an X11 one.
        assert_eq!(
            host_windowing(Some(""), Some("x11")),
            HostWindowing::Unsupported(String::new())
        );
        assert_eq!(
            host_windowing(Some(" x11 "), None),
            HostWindowing::Unsupported(" x11 ".into())
        );
    }

    /// Nothing activates the flag in a test process, and on Linux it is
    /// compiled to `false`: the Linux rules come from `target_os` there.
    #[test]
    fn the_flag_is_off_until_the_run_loop_raises_it() {
        assert!(!super::active());
        assert_eq!(super::linux_window_rules(), cfg!(target_os = "linux"));
    }
}
