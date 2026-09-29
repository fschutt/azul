use azul::prelude::*;

extern "C" fn layout(_: RefAny, _: LayoutCallbackInfo) -> Dom {
    Dom::create_body()
}

/// A scripted run (`AZ_E2E`, `AZ_E2E_TEST`) has no debug server to open.
fn scripted() -> bool {
    std::env::var_os("AZ_E2E").is_some() || std::env::var_os("AZ_E2E_TEST").is_some()
}

/// The debug server's port: the caller's `AZ_DEBUG`, else 8080.
fn debug_port() -> String {
    std::env::var("AZ_DEBUG")
        .ok()
        .filter(|p| !p.trim().is_empty())
        .unwrap_or_else(|| "8080".to_string())
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
