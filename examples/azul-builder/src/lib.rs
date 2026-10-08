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

/// A scripted run (`AZ_E2E`, `AZ_E2E_TEST`) has no debug server to open.
fn scripted() -> bool {
    std::env::var_os("AZ_E2E").is_some() || std::env::var_os("AZ_E2E_TEST").is_some()
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
    let headless = std::env::var("AZ_BACKEND").is_ok_and(|b| b == "headless");
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
    use super::port_from;

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
}
