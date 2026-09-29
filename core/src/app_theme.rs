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
//! The environment outranks the app: `AZ_THEME=<theme>` (a user's choice for
//! every app they run, or a screenshot run's) is what [`app_theme`] answers
//! whatever the app chose - `AZ_THEME` > the app's choice > the default
//! ([`resolve_app_theme`]). The name is the HEAD of the theme chain; the
//! cascade expands it (`xyz:pink` -> `[xyz:pink, xyz, flat]`,
//! `azul_css::theme_chain`).
//!
//! Without `std` there is no thread-local and no lock: everything answers the
//! environment's theme or the default, and a scope is a no-op.

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
///
/// `AZ_THEME` outranks the choice ([`app_theme`]). The first call logs, once,
/// what the environment asked for that is not taken as written (the
/// deprecated `AZ_THEME=light|dark` mode pin, an unknown `AZ_MODE`).
pub fn set_app_theme(name: &str) {
    report_theme_env_once();
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

/// The theme the app runs in: `AZ_THEME` if the environment names one, else
/// the last [`set_app_theme`], else [`DEFAULT_APP_THEME`]
/// ([`resolve_app_theme`] of the app's choice).
#[must_use]
pub fn app_theme() -> AzString {
    #[cfg(feature = "std")]
    {
        let slot = state::APP_THEME
            .read()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if let Some(name) = slot.as_deref() {
            return resolve_app_theme(Some(name));
        }
    }
    resolve_app_theme(None)
}

/// THE app-theme decision with the environment applied:
/// `AZ_THEME` > `choice` > [`DEFAULT_APP_THEME`]
/// (`azul_css::theme_chain::resolve_theme_head`). [`app_theme`] asks it about
/// the app's published choice; a host that keeps its own choice per window
/// (the E2E runner) asks it directly, so the environment outranks it too.
#[must_use]
pub fn resolve_app_theme(choice: Option<&str>) -> AzString {
    let env = azul_css::theme_chain::theme_env().theme.as_deref();
    AzString::from(azul_css::theme_chain::resolve_theme_head(env, choice))
}

/// Log, once per process, what the environment asked for that is not taken
/// as written (`azul_css::theme_chain::ThemeEnv::warnings`): the deprecated
/// `AZ_THEME=light|dark` mode pin, an unknown `AZ_MODE`. Through the
/// framework diagnostics, so an app's own sink sees it.
fn report_theme_env_once() {
    #[cfg(feature = "std")]
    {
        static REPORTED: std::sync::Once = std::sync::Once::new();
        REPORTED.call_once(|| {
            for warning in &azul_css::theme_chain::theme_env().warnings {
                crate::diagnostics::emit(alloc::format!("[azul][warn] {warning}"));
            }
        });
    }
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
