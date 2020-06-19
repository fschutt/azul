    #![allow(dead_code, unused_imports)]
    //! `App` construction and configuration
    use crate::dll::*;
    use std::ffi::c_void;
    use crate::callbacks::{LayoutCallbackType, RefAny};
    use crate::window::WindowCreateOptions;


    /// `AppConfig` struct
    pub use crate::dll::AzAppConfigPtr as AppConfig;

    impl AppConfig {
        /// Creates a new AppConfig with default values
        pub fn default() -> Self { (crate::dll::get_azul_dll().az_app_config_ptr_default)() }
    }

    impl std::fmt::Debug for AppConfig { fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result { write!(f, "{}", (crate::dll::get_azul_dll().az_app_config_ptr_fmt_debug)(self)) } }
    impl Drop for AppConfig { fn drop(&mut self) { (crate::dll::get_azul_dll().az_app_config_ptr_delete)(self); } }


    /// `App` struct
    pub use crate::dll::AzAppPtr as App;

    impl App {
        /// Creates a new App instance from the given `AppConfig`
        pub fn new(data: RefAny, config: AppConfig, callback: LayoutCallbackType) -> Self { (crate::dll::get_azul_dll().az_app_ptr_new)(data, config, callback) }
        /// Runs the application. Due to platform restrictions (specifically `WinMain` on Windows), this function never returns.
        pub fn run(self, window: WindowCreateOptions)  { (crate::dll::get_azul_dll().az_app_ptr_run)(self, window) }
    }

    impl std::fmt::Debug for App { fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result { write!(f, "{}", (crate::dll::get_azul_dll().az_app_ptr_fmt_debug)(self)) } }
    impl Drop for App { fn drop(&mut self) { (crate::dll::get_azul_dll().az_app_ptr_delete)(self); } }
