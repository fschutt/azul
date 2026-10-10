//! Unified `AuthSession` namespace (sign-in sessions, OAuth 2.0 for native apps). See
//! [`crate::unified`] and `crate::desktop::extra::auth_session` (the platforms).
//!
//! The request and result types are azul-layout's (`azul_layout::auth_session`: plain
//! `#[repr(C)]` data, the same on every target); only the namespace that starts a session is
//! here. On wasm there is no session yet: the stub has the identical `#[repr(C)]` layout and
//! answers every sign-in `Unsupported` (through the request queue, like every resumable call);
//! its PKCE pair is empty, so nothing could sign in with it by mistake.

#[cfg(all(feature = "cabi_internal", not(target_arch = "wasm32")))]
pub use crate::desktop::extra::auth_session::{create_pkce, AuthSession};

#[cfg(target_arch = "wasm32")]
use azul_core::{refany::RefAny, task::RequestId};
#[cfg(target_arch = "wasm32")]
use azul_css::AzString;
#[cfg(target_arch = "wasm32")]
use azul_layout::{
    auth_session::{AuthPkce, AuthRequest, AuthSessionResult, AuthSessionStatus},
    callbacks::{CallbackInfo, ResumeCallback},
};

/// wasm stub of the desktop `AuthSession` namespace; `#[repr(C)]` layout MUST match.
#[cfg(target_arch = "wasm32")]
#[repr(C)]
#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash, Default)]
pub struct AuthSession {
    pub _reserved: u8,
}

#[cfg(target_arch = "wasm32")]
impl AuthSession {
    pub const fn new() -> Self {
        Self { _reserved: 0 }
    }

    /// No sign-in session on wasm yet: resumes `on_result` with `Unsupported`.
    pub fn start(
        _info: &mut CallbackInfo,
        request: AuthRequest,
        data: RefAny,
        on_result: ResumeCallback,
    ) -> RequestId {
        azul_layout::request::complete(
            data,
            on_result,
            AuthSessionResult::ended(
                AuthSessionStatus::Unsupported,
                request.redirect_uri.as_str(),
                "a sign-in session needs a desktop or a phone; the web build has none yet",
            ),
        )
    }
}

/// No random source here: an empty pair (a sign-in with it fails).
#[cfg(target_arch = "wasm32")]
#[must_use]
pub fn create_pkce() -> AuthPkce {
    AuthPkce {
        code_verifier: AzString::default(),
        code_challenge: AzString::default(),
        state: AzString::default(),
    }
}
