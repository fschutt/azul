use azul::prelude::*;

extern "C" fn layout(_: RefAny, _: LayoutCallbackInfo) -> Dom {
    Dom::create_body()
}

extern "C" fn on_start(_data: RefAny, _info: CallbackInfo) -> Update {
    azul::dll::AzUrl::parse("http://localhost:8080").into_result().unwrap().open();
    Update::DoNothing
}

pub fn run_app() {
    std::env::set_var("AZ_DEBUG", "8080");
    std::env::set_var("AZUL_DEBUG", "8080");
    let config = AppConfig::create();
    let app = App::create(RefAny::new(()), config);
    let mut options = WindowCreateOptions::create(layout);
    options.create_callback = azul::dll::AzOptionCallback::Some(azul::dll::AzCallback::create(on_start));
    app.run(options);
}

// Android entry point
#[cfg(target_os = "android")]
#[no_mangle]
pub extern "C" fn android_main(app: azul::dll::AndroidApp) {
    azul::dll::android_main_glue(app);
    run_app();
}
