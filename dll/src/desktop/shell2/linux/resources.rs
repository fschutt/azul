//! Shared resources for Linux windows
//!
//! This module provides a structure that holds resources shared across
//! all windows in an application: font cache, app data, and system styling.

use std::{cell::RefCell, sync::Arc};

use azul_core::{refany::RefAny, resources::AppConfig};
use azul_css::system::SystemStyle;
use rust_fontconfig::FcFontCache;

use super::super::common::debug_server::LogCategory;
use crate::{log_debug, log_error, log_info, log_trace, log_warn};

/// Shared resources for all windows in a Linux application
///
/// This is Arc<>'d and passed to each window on creation, allowing:
/// - Font cache sharing (expensive to build)
/// - App data sharing (user's global state)
/// - System styling sharing (detected once at startup)
#[derive(Clone)]
pub struct AppResources {
    /// Application configuration
    pub config: AppConfig,

    /// Font configuration cache (shared across all windows)
    pub fc_cache: Arc<FcFontCache>,

    /// Application data (user's global state)
    pub app_data: Arc<RefCell<RefAny>>,

    /// System styling detected at startup (theme, colors, fonts, etc.)
    pub system_style: Arc<SystemStyle>,
}

impl AppResources {
    /// Create new shared resources
    ///
    /// This should be called once at application startup.
    pub fn new(config: AppConfig, fc_cache: Arc<FcFontCache>) -> Self {
        // Create empty app data (user can set this later)
        let app_data = Arc::new(RefCell::new(RefAny::new(())));

        // Detect system style once at startup
        let system_style = Arc::new(SystemStyle::new());

        log_debug!(
            LogCategory::Resources,
            "[AppResources] System style detected:"
        );
        log_debug!(
            LogCategory::Resources,
            "  Platform: {:?}",
            system_style.platform
        );
        log_debug!(LogCategory::Resources, "  Theme: {:?}", system_style.theme);
        log_debug!(
            LogCategory::Resources,
            "  UI Font: {:?}",
            system_style.fonts.ui_font
        );
        log_debug!(
            LogCategory::Resources,
            "  Accent Color: {:?}",
            system_style.colors.accent
        );

        Self {
            config,
            fc_cache,
            app_data,
            system_style,
        }
    }

    /// Create default resources for testing
    pub fn default_for_testing() -> Self {
        let config = AppConfig::default();
        let fc_cache = Arc::new(FcFontCache::default());
        Self::new(config, fc_cache)
    }
}
