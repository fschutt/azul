//! AzBuilder: the window is the CANVAS of azul's builder, whose UI is the
//! debug server's page (opened in the browser at start).
//!
//! The empty `<body>` is deliberate. The builder's first document is "what
//! the window shows right now" (`layout/src/e2e/builder.rs`,
//! `BuilderDocument::from_styled_dom`), and every edit rebuilds this window
//! from the document: a shell, a theme scope or body styles here would be
//! imported into every new design (a margin-less, full-height flex body the
//! user never asked for). The white canvas in dark mode is the design's own
//! page colour, not the app's chrome. Do not "fix" it with
//! `ShellThemeScope::body()` like the other apps (wave-6 LOOK, 2026-10-03).

use azul::prelude::*;

/// The canvas: an empty body (see the module documentation).
extern "C" fn layout(_: RefAny, _: LayoutCallbackInfo) -> Dom {
    Dom::create_body()
}

/// A scripted run (`AZ_E2E`, `AZ_E2E_TEST`) of a TEST build has no debug server to open.
fn scripted() -> bool {
    scripted_in(cfg!(feature = "test-hooks"), &|name| {
        std::env::var_os(name).is_some()
    })
}

/// [`scripted`] for a build that has the test hooks (`hooks`) or not, where `set` says whether a
/// variable is set. A shipped AzBuilder (RF38) takes no switch from its environment: it is the
/// builder, whatever a shell profile exports.
fn scripted_in(hooks: bool, set: &dyn Fn(&str) -> bool) -> bool {
    hooks && (set("AZ_E2E") || set("AZ_E2E_TEST"))
}

/// A headless run of a TEST build has nobody to show the builder page to. `AZ_BACKEND=headless`
/// stays a windowing choice in a shipped build, which still opens the page (the opener finds
/// no browser and does nothing).
fn headless_in(hooks: bool, backend: Option<&str>) -> bool {
    hooks && backend == Some("headless")
}

/// The debug server's port: the caller's `AZ_DEBUG`, else [`DEFAULT_PORT`].
fn debug_port() -> String {
    port_from(std::env::var("AZ_DEBUG").ok().as_deref())
}

/// The debug server's port without `AZ_DEBUG`: one the local stack leaves free (its sqld is on
/// 8080, the token server on 8081, S3 on 9000, the meeting server on 8790).
const DEFAULT_PORT: &str = "8765";

/// [`debug_port`] from `AZ_DEBUG`'s value.
fn port_from(var: Option<&str>) -> String {
    var.filter(|p| !p.trim().is_empty())
        .map(String::from)
        .unwrap_or_else(|| DEFAULT_PORT.to_string())
}

extern "C" fn on_start(_data: RefAny, _info: CallbackInfo) -> Update {
    // A headless run has nobody to show the builder page to.
    let headless = headless_in(
        cfg!(feature = "test-hooks"),
        std::env::var("AZ_BACKEND").ok().as_deref(),
    );
    if !scripted() && !headless {
        azul::dll::AzUrl::parse(format!("http://localhost:{}", debug_port()).as_str())
            .into_result()
            .unwrap()
            .open();
    }
    Update::DoNothing
}

pub fn run_app() {
    if !scripted() {
        std::env::set_var("AZ_DEBUG", debug_port());
    }
    let config = AppConfig::create();
    let app = App::create(RefAny::new(()), config);
    let mut options = WindowCreateOptions::create(layout);
    options.window_state.flags.is_always_on_top = true;
    options.create_callback =
        azul::dll::AzOptionCallback::Some(azul::dll::AzCallback::create(on_start));
    app.run(options);
}

// Android entry point
#[cfg(target_os = "android")]
#[no_mangle]
pub extern "C" fn android_main(app: azul::dll::AndroidApp) {
    azul::dll::android_main_glue(app);
    run_app();
}

#[cfg(test)]
mod tests {
    use super::{headless_in, port_from, scripted_in};

    #[test]
    fn the_debug_port_is_az_debug_else_one_the_local_stack_leaves_free() {
        assert_eq!(port_from(Some("9123")), "9123");
        assert_eq!(port_from(Some("  ")), port_from(None), "blank is unset");
        assert_ne!(
            port_from(None),
            "8080",
            "8080 is the local stack's sqld (iso/docs/GETTING-STARTED.md)"
        );
    }

    /// RF38: a script variable makes a run "scripted" (no debug page, no `AZ_DEBUG`) in a TEST
    /// build only; a shipped AzBuilder is the builder whatever its environment holds.
    #[test]
    fn only_a_test_build_takes_a_script_for_a_scripted_run() {
        for name in ["AZ_E2E", "AZ_E2E_TEST"] {
            let set = |asked: &str| asked == name;
            assert!(scripted_in(true, &set), "{name}");
            assert!(!scripted_in(false, &set), "{name} scripted a shipped build");
        }
        assert!(!scripted_in(true, &|_| false));
        assert!(!scripted_in(false, &|_| false));
    }

    /// The headless backend is a stand-in switch only in a TEST build, and only for `headless`.
    #[test]
    fn only_a_test_build_takes_the_headless_backend_for_a_stand_in() {
        assert!(headless_in(true, Some("headless")));
        assert!(!headless_in(false, Some("headless")));
        assert!(!headless_in(true, Some("metal")));
        assert!(!headless_in(true, None));
    }

    /// A shipped build of this crate's own tests: the process-level answer follows the feature.
    #[cfg(not(feature = "test-hooks"))]
    #[test]
    fn this_shipped_process_is_never_scripted() {
        assert!(!super::scripted());
    }
}
