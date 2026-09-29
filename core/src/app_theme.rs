//! The APP THEME: the one name - `flat`, `flora`, later `native` and user
//! themes - that `@theme(<name>)` blocks select by, separate from the light /
//! dark colour scheme.
//!
//! Two pieces of state:
//!
//! - the app's CHOICE ([`set_app_theme`] / [`app_theme`]), process-global like
//!   the colour scheme's (`azul_layout::window::set_app_color_scheme`):
//!   `App::create` publishes `AppConfig::theme`, `CallbackInfo::set_theme`
//!   switches it. Global because a window opened after a switch has to start
//!   in it, and window creation has no path back to the window whose callback
//!   switched.
//! - the theme a DOM is being BUILT for ([`ThemeScope`] / [`current_theme`]),
//!   per thread. A widget's `dom()` has no `LayoutCallbackInfo`, yet its
//!   STRUCTURE may depend on the theme (flora wraps nodes flat does not). The
//!   engine enters a scope with the window's theme around every DOM build of
//!   that window (the `layout()` call, the form-control widgets it styles);
//!   outside one, the app's choice applies - which is what every window shows
//!   anyway once its pending rebuild ran. The seam shape of the
//!   style-dependency recorder (`LayoutCallbackInfo::depends_on_system_style`):
//!   the build runs synchronously on the calling thread.
//!
//! Without `std` there is no thread-local and no lock: everything answers the
//! default theme and a scope is a no-op.

use azul_css::{dynamic_selector::DEFAULT_APP_THEME, AzString};

#[cfg(feature = "std")]
mod state {
    use alloc::string::String;

    use azul_css::AzString;

    /// The app's choice; `None` until `App::create` publishes one.
    pub(super) static APP_THEME: std::sync::RwLock<Option<String>> = std::sync::RwLock::new(None);

    std::thread_local! {
        /// The theme the DOM being built on this thread is for, while a
        /// [`super::ThemeScope`] is entered.
        pub(super) static BUILD_THEME: core::cell::RefCell<Option<AzString>> =
            const { core::cell::RefCell::new(None) };
    }
}

/// Publish the app's theme choice: every window built from now on starts in
/// it, and every window's next DOM rebuild adopts it. `App::create` calls this
/// with `AppConfig::theme`; `CallbackInfo::set_theme` with the new name.
pub fn set_app_theme(name: &str) {
    #[cfg(feature = "std")]
    {
        use alloc::string::ToString;
        let mut slot = state::APP_THEME
            .write()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        *slot = Some(name.to_string());
    }
    #[cfg(not(feature = "std"))]
    let _ = name;
}

/// The app's theme choice: the last [`set_app_theme`], else
/// [`DEFAULT_APP_THEME`].
#[must_use]
pub fn app_theme() -> AzString {
    #[cfg(feature = "std")]
    {
        use alloc::string::ToString;
        let slot = state::APP_THEME
            .read()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if let Some(name) = slot.as_deref() {
            return AzString::from(name.to_string());
        }
    }
    AzString::from_const_str(DEFAULT_APP_THEME)
}

/// The theme the DOM being built on this thread is for: the innermost
/// entered [`ThemeScope`], else the app's choice ([`app_theme`]).
///
/// What a widget's `dom()` reads to choose its STRUCTURE (its CSS carries
/// every theme's block and needs no answer); `layout()` reads the same value
/// through `LayoutCallbackInfo::get_theme_name`.
#[must_use]
pub fn current_theme() -> AzString {
    #[cfg(feature = "std")]
    {
        if let Some(name) = state::BUILD_THEME
            .try_with(|scoped| scoped.borrow().clone())
            .ok()
            .flatten()
        {
            return name;
        }
    }
    app_theme()
}

/// While alive, the DOM built on this thread is built for `name`
/// ([`current_theme`]). Dropping it restores the scope around it, so scopes
/// nest. Bound to the thread that entered it.
#[must_use = "the scope ends when the guard is dropped"]
pub struct ThemeScope {
    #[cfg(feature = "std")]
    previous: Option<AzString>,
    /// `!Send`: the scope belongs to the thread whose build it frames.
    _thread_bound: core::marker::PhantomData<*const ()>,
}

impl ThemeScope {
    /// Build for `name` until the returned guard is dropped.
    pub fn enter(name: AzString) -> Self {
        #[cfg(feature = "std")]
        let previous = state::BUILD_THEME
            .try_with(|scoped| scoped.replace(Some(name)))
            .ok()
            .flatten();
        #[cfg(not(feature = "std"))]
        let _ = name;
        Self {
            #[cfg(feature = "std")]
            previous,
            _thread_bound: core::marker::PhantomData,
        }
    }
}

impl Drop for ThemeScope {
    fn drop(&mut self) {
        #[cfg(feature = "std")]
        {
            let previous = self.previous.take();
            let _ = state::BUILD_THEME.try_with(|scoped| *scoped.borrow_mut() = previous);
        }
    }
}

impl core::fmt::Debug for ThemeScope {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("ThemeScope").finish_non_exhaustive()
    }
}
