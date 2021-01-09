    #![allow(dead_code, unused_imports)]
    //! `App` construction and configuration
    use crate::dll::*;
    use std::ffi::c_void;
    use crate::callbacks::RefAny;
    use crate::window::WindowCreateOptions;


    /// `AppLogLevel` struct
    #[doc(inline)] pub use crate::dll::AzAppLogLevel as AppLogLevel;

    impl Clone for AppLogLevel { fn clone(&self) -> Self { *self } }
    impl Copy for AppLogLevel { }


    /// Configuration for optional features, such as whether to enable logging or panic hooks
    #[doc(inline)] pub use crate::dll::AzAppConfig as AppConfig;

    impl AppConfig {
        /// Creates a new AppConfig with default values
        pub fn default() -> Self { (crate::dll::get_azul_dll().az_app_config_default)() }
    }

    impl Clone for AppConfig { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_app_config_deep_copy)(self) } }
    impl Drop for AppConfig { fn drop(&mut self) { (crate::dll::get_azul_dll().az_app_config_delete)(self); } }


    /// `App` struct
    #[doc(inline)] pub use crate::dll::AzAppPtr as App;

    impl App {
        /// Creates a new App instance from the given `AppConfig`
        pub fn new(data: RefAny, config: AppConfig) -> Self { (crate::dll::get_azul_dll().az_app_ptr_new)(data, config) }
        /// Spawn a new window on the screen when the app is run.
        pub fn add_window(&mut self, window: WindowCreateOptions)  { (crate::dll::get_azul_dll().az_app_ptr_add_window)(self, window) }
        /// Runs the application. Due to platform restrictions (specifically `WinMain` on Windows), this function never returns.
        pub fn run(self, window: WindowCreateOptions)  { (crate::dll::get_azul_dll().az_app_ptr_run)(self, window) }
    }

    impl Drop for App { fn drop(&mut self) { (crate::dll::get_azul_dll().az_app_ptr_delete)(self); } }
