//! Shared resources for Linux windows
//!
//! This module provides a structure that holds resources shared across
//! all windows in an application: font cache, app data, and system styling.

use std::{cell::RefCell, sync::Arc};

use azul_core::{refany::RefAny, resources::AppConfig};
use azul_css::system::SystemStyle;
use azul_core::icon::IconProviderHandle;
use rust_fontconfig::FcFontCache;
use rust_fontconfig::registry::FcFontRegistry;

use super::super::common::debug_server::LogCategory;
use crate::{log_debug, log_error, log_info, log_trace, log_warn};

/// Shared resources for all windows in a Linux application
///
/// This is Arc<>'d and passed to each window on creation, allowing:
/// - Font cache sharing (expensive to build)
/// - App data sharing (user's global state)
/// - System styling sharing (detected once at startup)
/// - Icon provider sharing (icon packs with fallbacks)
#[derive(Clone)]
pub struct AppResources {
    /// Application configuration
    pub config: AppConfig,

    /// Font configuration cache (shared across all windows)
    pub fc_cache: Arc<FcFontCache>,

    /// Async font registry for background font scanning
    pub font_registry: Option<Arc<FcFontRegistry>>,

    /// Application data (user's global state)
    pub app_data: Arc<RefCell<RefAny>>,

    /// System styling detected at startup (theme, colors, fonts, etc.)
    pub system_style: Arc<SystemStyle>,

    /// Icon provider for resolving icon names to renderable content
    pub icon_provider: IconProviderHandle,
}

impl AppResources {
    /// Create new shared resources
    ///
    /// This should be called once at application startup.
    pub fn new(config: AppConfig, fc_cache: Arc<FcFontCache>, font_registry: Option<Arc<FcFontRegistry>>) -> Self {
        // Create empty app data (user can set this later)
        let app_data = Arc::new(RefCell::new(RefAny::new(())));

        // Use system style from AppConfig (detected once at AppConfig::create())
        let system_style = Arc::new(config.system_style.clone());

        // Clone icon provider from AppConfig (Arc-based, cheap)
        let icon_provider = config.icon_provider.clone();

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
            font_registry,
            app_data,
            system_style,
            icon_provider,
        }
    }

    /// Create default resources for testing
    pub fn default_for_testing() -> Self {
        let config = AppConfig::default();
        let fc_cache = Arc::new(FcFontCache::default());
        Self::new(config, fc_cache, None)
    }
}
