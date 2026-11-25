//! Shared resources for Linux windows
//!
//! This module provides a structure that holds resources shared across
//! all windows in an application: font cache, app data, and system styling.

use std::{cell::RefCell, sync::Arc};

use azul_core::{icon::SharedIconProvider, refany::RefAny, resources::AppConfig};
use azul_css::system::SystemStyle;
use rust_fontconfig::{registry::FcFontRegistry, FcFontCache};

use super::super::common::{
    app_shared::AppShared, debug_server::LogCategory, event::SharedUndoManager,
};
use super::linux_settings::LinuxSettings;
use crate::{desktop::platform_settings::PlatformEnv, log_debug};

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

    /// What the app chose and configured, shared by every window of it: its mode and language
    /// (a window opened after a switch starts in them), the natural-scrolling preference the
    /// compositor reports, the two config flags. The App's own, like `undo_manager`.
    pub app_shared: Arc<AppShared>,

    /// System styling detected at startup (theme, colors, fonts, etc.)
    pub system_style: Arc<SystemStyle>,

    /// Icon provider for resolving icon names to renderable content
    pub icon_provider: SharedIconProvider,
}

impl AppResources {
    /// The Linux shell's switches, read once at the start of the app (`AppSettings::linux`).
    #[must_use]
    pub fn linux(&self) -> &LinuxSettings {
        &self.app_shared.settings().linux
    }

    /// The platform lookups the app read at its start (`AppSettings::platform`): the session,
    /// the desktop, the user's directories.
    #[must_use]
    pub fn platform(&self) -> &PlatformEnv {
        &self.app_shared.settings().platform
    }

    /// Create new shared resources
    ///
    /// This should be called once at application startup.
    pub fn new(
        config: AppConfig,
        fc_cache: Arc<FcFontCache>,
        font_registry: Option<Arc<FcFontRegistry>>,
    ) -> Self {
        let app_shared = Arc::new(AppShared::new(&config));
        Self::new_with_font_manager(config, fc_cache, font_registry, None, app_shared)
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
        // The App's own choices, shared by every window on this connection.
        app_shared: Arc<AppShared>,
    ) -> Self {
        // Create empty app data (user can set this later)
        let app_data = Arc::new(RefCell::new(RefAny::new(())));

        // Use system style from AppConfig (detected once at AppConfig::create())
        let system_style = Arc::new(config.system_style.clone());

        // Convert icon provider from handle to shared (Arc-based, cheap)
        let icon_provider = SharedIconProvider::from_handle(config.icon_provider.clone());

        log_debug!(
            LogCategory::Resources,
            "[AppResources] System style detected: platform={:?}, theme={:?}, ui_font={:?}, \
             accent={:?}",
            system_style.platform,
            system_style.mode,
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
            app_shared,
            system_style,
            icon_provider,
        }
    }
}
