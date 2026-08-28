//! Shared resources for Linux windows
//!
//! This module provides a structure that holds resources shared across
//! all windows in an application: font cache, app data, and system styling.

use std::{cell::RefCell, sync::Arc};

use azul_core::icon::SharedIconProvider;
use azul_core::{refany::RefAny, resources::AppConfig};
use azul_css::system::SystemStyle;
use rust_fontconfig::registry::FcFontRegistry;
use rust_fontconfig::FcFontCache;

use super::super::common::debug_server::LogCategory;
use super::super::common::event::SharedUndoManager;
use crate::log_debug;

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

    /// THE app-level font manager, shared by every window on this connection.
    ///
    /// Without it each window built its own via `LayoutWindow::new(fc_cache)`,
    /// giving every window a private `embedded_fonts` pool - so a face
    /// registered while laying out one window was invisible to the next. See
    /// `layout_window_sharing_fonts`.
    pub font_manager: Option<
        std::sync::Arc<azul_layout::font_traits::FontManager<azul_css::props::basic::FontRef>>,
    >,

    /// Application data (user's global state)
    pub app_data: Arc<RefCell<RefAny>>,

    /// App-global undo/redo manager (shared across all windows). Set by
    /// `LinuxWindow::new_with_resources` from the App's owned manager; the
    /// placeholder created in `new()` is overwritten there.
    pub undo_manager: SharedUndoManager,

    /// System styling detected at startup (theme, colors, fonts, etc.)
    pub system_style: Arc<SystemStyle>,

    /// Icon provider for resolving icon names to renderable content
    pub icon_provider: SharedIconProvider,
}

impl AppResources {
    /// Create new shared resources
    ///
    /// This should be called once at application startup.
    pub fn new(
        config: AppConfig,
        fc_cache: Arc<FcFontCache>,
        font_registry: Option<Arc<FcFontRegistry>>,
    ) -> Self {
        Self::new_with_font_manager(config, fc_cache, font_registry, None)
    }

    /// As `new`, but adopting the app-level font manager so every window on this
    /// connection shares one set of font pools.
    pub fn new_with_font_manager(
        config: AppConfig,
        fc_cache: Arc<FcFontCache>,
        font_registry: Option<Arc<FcFontRegistry>>,
        font_manager: Option<
            std::sync::Arc<azul_layout::font_traits::FontManager<azul_css::props::basic::FontRef>>,
        >,
    ) -> Self {
        // Create empty app data (user can set this later)
        let app_data = Arc::new(RefCell::new(RefAny::new(())));

        // Use system style from AppConfig (detected once at AppConfig::create())
        let system_style = Arc::new(config.system_style.clone());

        // Convert icon provider from handle to shared (Arc-based, cheap)
        let icon_provider = SharedIconProvider::from_handle(config.icon_provider.clone());

        log_debug!(
            LogCategory::Resources,
            "[AppResources] System style detected: platform={:?}, theme={:?}, ui_font={:?}, accent={:?}",
            system_style.platform,
            system_style.theme,
            system_style.fonts.ui_font,
            system_style.colors.accent
        );

        Self {
            config,
            font_manager,
            fc_cache,
            font_registry,
            app_data,
            // Placeholder; overwritten by LinuxWindow::new_with_resources with the
            // App's owned manager so all Linux windows share one history.
            undo_manager: SharedUndoManager::new(),
            system_style,
            icon_provider,
        }
    }
}
