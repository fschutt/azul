    impl Default for AppConfig {
        fn default() -> Self {
            Self {
                // note: this field should never be changed, apps that
                // want to use a newer layout model need to explicitly set
                // it or use a header shim for ABI compat
                layout_model: LayoutSolverVersion::March2021,
                log_level: AppLogLevel::Error,
                enable_visual_panic_hook: true,
                enable_logging_on_panic: true,
                enable_tab_navigation: true,
                system_callbacks: ExternalSystemCallbacks::rust_internal(),
            }
        }
    }