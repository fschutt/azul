use core::ffi::c_void;
use azul_core::dom::Dom;
use azul_core::callbacks::LayoutInfo;
use azul_css::Css;
use azul_core::window::WindowCreateOptions;
#[cfg(not(target_arch = "wasm32"))]
use azul_desktop::app::{App, AppConfig};
#[cfg(target_arch = "wasm32")]
use azul_web::app::{App, AppConfig};

pub type AzLayoutCallback = fn(&AzDataModel, LayoutInfo) -> Dom<AzDataModel>;

#[repr(C)]
#[no_mangle]
pub struct AzApp {
    _rust_object: App<AzDataModel>,
}

#[no_mangle]
pub extern "C" fn az_app_new(data: AzDataModel, config: AzAppConfig, callback: AzLayoutCallback) -> AzApp {
    AzApp { _rust_object: App::new_from_callback(data, config._rust_object, callback) }
}

#[no_mangle]
pub extern "C" fn az_app_delete(_: AzApp) { }

#[repr(C)]
#[no_mangle]
pub struct AzAppConfig {
    _rust_object: AppConfig
}

#[no_mangle]
pub extern "C" fn az_app_config_new() -> AzAppConfig { AzAppConfig { _rust_object: AppConfig::default() } }

#[no_mangle]
pub extern "C" fn az_app_config_delete(_: AzAppConfig) { }

#[repr(C)]
#[no_mangle]
pub struct AzDom {
    _rust_object: Dom<AzDataModel>,
}

#[no_mangle]
pub extern "C" fn az_dom_div() -> AzDom { AzDom { _rust_object: Dom::div() } }

#[no_mangle]
pub extern "C" fn az_dom_delete(_: AzDom) { }

#[no_mangle]
pub extern "C" fn az_dom_get_inner(dom: AzDom) -> Dom<AzDataModel> { dom._rust_object }

#[repr(C)]
#[no_mangle]
pub struct AzWindowCreateOptions {
    _rust_object: WindowCreateOptions<AzDataModel>,
}

#[no_mangle]
pub extern "C" fn az_window_create_options_new(css: AzCss) -> AzWindowCreateOptions {
    AzWindowCreateOptions {
        _rust_object: WindowCreateOptions::new(css._rust_object)
    }
}

#[no_mangle]
pub extern "C" fn az_window_create_options_delete(_: AzWindowCreateOptions) { }

#[repr(C)]
#[no_mangle]
pub struct AzCss {
    _rust_object: Css,
}

#[no_mangle]
pub extern "C" fn az_css_new_native() -> AzCss { AzCss { _rust_object: azul_native_style::native() } }

#[no_mangle]
pub extern "C" fn az_css_delete(_: AzCss) { }

#[repr(C)]
#[no_mangle]
pub struct AzDataModel {
    pub data: *mut c_void,
}

#[no_mangle]
pub extern "C" fn az_data_model_new(data: *mut c_void) -> AzDataModel {
    AzDataModel { data }
}

#[no_mangle]
pub extern "C" fn az_data_model_delete(_: AzDataModel) { }

// --- different implementations of app.run()

#[cfg(target_arch = "wasm32")]
#[no_mangle]
pub extern "C" fn az_app_run(app: AzApp, window: AzWindowCreateOptions) -> ! {
    app._rust_object.run(window._rust_object)
}

#[cfg(not(target_arch = "wasm32"))]
#[no_mangle]
pub extern "C" fn az_app_run(app: AzApp, window: AzWindowCreateOptions) -> ! {
    app._rust_object.run(window._rust_object)
}
