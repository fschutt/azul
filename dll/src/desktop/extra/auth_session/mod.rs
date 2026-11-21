//! SIGN-IN SESSIONS: OAuth 2.0 for native apps (RFC 8252), the platform half.
//!
//! A cloud drive's sign-in (Google Drive, Dropbox, OneDrive) runs where the user's browser
//! runs, never in a web view of the app: Google refuses sign-in inside embedded web views, and
//! the address bar a user types a password under should be the browser's. [`AuthSession::start`]
//! opens the provider's authorize page the way the platform wants native apps to, waits for the
//! provider's redirect and resumes the app's callback with an [`AuthSessionResult`] - the URL
//! the browser came back to, or why there is none (cancelled, timed out, unsupported, failed).
//!
//! | redirect | macOS | iOS | Windows, Linux | Android |
//! |---|---|---|---|---|
//! | custom scheme (`com.example.app:/cb`) | `ASWebAuthenticationSession` | `ASWebAuthenticationSession` | Unsupported | Custom Tab + the app's redirect intent |
//! | loopback (`http://127.0.0.1/cb`) | system browser + loopback listener | Unsupported | system browser + loopback listener | Unsupported |
//!
//! - **Apple** ([`apple`]): `AuthenticationServices.framework` is dlopen'd at the first sign-in
//!   and its classes looked up at runtime, the way the `<webview>` loads WebKit; the
//!   presentation anchor is the calling window. A timeout cancels the session.
//! - **The loopback redirect** (`azul_layout::auth_session::loopback`): a one-shot HTTP listener
//!   on 127.0.0.1 (a random port unless the redirect names one) on a thread of its own, then the
//!   system browser through `Url::open`. One request, answered with "You can close this tab".
//! - **Android** ([`android`]): `com.azul.auth.AzulAuthSession` (scripts/android) opens a
//!   Custom Tab; the app's redirect intent comes back through `AzulActivity.onNewIntent`, a
//!   return to the app without it is a cancel. The manifest needs the redirect scheme
//!   (`AZ_ANDROID_AUTH_SCHEME` in build-android.sh).
//! - **A headless or E2E TEST run** (`AZ_BACKEND=headless`, `AZ_E2E_TEST`; a build with the cargo
//!   feature `test-hooks` only, `azul_core::run_mode`) opens nothing: it answers from
//!   [`FAKE_REDIRECT_VAR`] ([`fake_answer`]). A shipped build has no such hook: whatever its
//!   environment holds, a sign-in opens the provider's page and no variable answers for the user.
//!
//! Every platform hands its URL to `azul_layout::auth_session::finish`: the redirect must be the
//! request's redirect URI exactly and carry the request's `state`, or the session FAILS rather
//! than hand the app a code someone else started. The PKCE pair and the state come from
//! [`create_pkce`] (the OS's random source); reading the code and exchanging it for tokens is the
//! app's (`AuthPkce::read_redirect`, then its own HTTP client to the provider's token endpoint).
//! Nothing here sends anything anywhere.

use std::{
    sync::mpsc::{channel, TryRecvError},
    time::{Duration, Instant},
};

use azul_core::{refany::RefAny, task::RequestId, window::RawWindowHandle};
use azul_css::AzString;
use azul_layout::{
    auth_session::{
        self as auth, AuthPkce, AuthRequest, AuthSessionResult, AuthSessionStatus, Redirect,
        PKCE_ENTROPY_LEN,
    },
    callbacks::{CallbackInfo, ResumeCallback},
    request::{self, PollFn},
};

#[cfg(target_os = "android")]
pub mod android;
#[cfg(any(target_os = "macos", target_os = "ios"))]
mod apple;

/// The variable a headless or E2E TEST run answers every sign-in from.
///
/// The URL the "browser" comes back to, with `{redirect_uri}`, `{state}` and `{code_challenge}`
/// replaced by the request's (`{redirect_uri}?code=e2e-{code_challenge}&state={state}`); `cancel`
/// or `timeout` for those answers. Unset: the sign-in is Unsupported. Read by
/// `azul_core::run_mode` in a build with the cargo feature `test-hooks` only.
pub const FAKE_REDIRECT_VAR: &str = azul_core::run_mode::FAKE_REDIRECT_VAR;

/// What a session answers: now, or through a poll the request pump asks every frame.
enum Begun {
    Now(AuthSessionResult),
    Later(PollFn),
}

/// Static-method namespace of the sign-in session (see the module documentation).
#[repr(C)]
#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash)]
#[allow(clippy::pub_underscore_fields)] // _reserved: FFI/api.json static-namespace placeholder
                                        // field
pub struct AuthSession {
    pub _reserved: u8,
}

impl Default for AuthSession {
    fn default() -> Self {
        Self::new()
    }
}

impl AuthSession {
    /// Returns a zero-initialised namespace handle. Static-only - the struct is just a hook for
    /// the FFI layer.
    #[must_use]
    pub const fn new() -> Self {
        Self { _reserved: 0 }
    }

    /// Starts a sign-in (see the module documentation) from a callback of the window that shows
    /// it, and resumes `on_result` with an [`AuthSessionResult`] once it ends - never inside
    /// this call. `data` is handed back untouched. The request's `authorize_url` has every
    /// parameter but `redirect_uri`, which the session adds.
    pub fn start(
        info: &mut CallbackInfo,
        request: AuthRequest,
        data: RefAny,
        on_result: ResumeCallback,
    ) -> RequestId {
        let window = info.get_current_window_handle();
        match begin(&request, window) {
            Begun::Now(result) => request::complete(data, on_result, result),
            Begun::Later(poll) => request::defer(data, on_result, poll),
        }
    }
}

/// A fresh PKCE pair and state for one sign-in, from the OS's random source.
#[must_use]
pub fn create_pkce() -> AuthPkce {
    let mut entropy = [0_u8; PKCE_ENTROPY_LEN];
    if getrandom::getrandom(&mut entropy).is_err() {
        fallback_entropy(&mut entropy);
    }
    let pkce = AuthPkce::from_entropy(&entropy).unwrap_or_else(|| AuthPkce {
        code_verifier: AzString::default(),
        code_challenge: AzString::default(),
        state: AzString::default(),
    });
    entropy.fill(0);
    pkce
}

/// Where the OS's random source failed (it does not on any platform azul runs on): std's
/// `RandomState`, whose keys the OS seeded, hashing a counter and the clock. Unpredictable to
/// anyone without the process's keys.
fn fallback_entropy(out: &mut [u8]) {
    use std::{
        collections::hash_map::RandomState,
        hash::{BuildHasher, Hasher},
    };
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_nanos());
    for (i, chunk) in out.chunks_mut(8).enumerate() {
        let mut hasher = RandomState::new().build_hasher();
        hasher.write_usize(i);
        hasher.write_u128(nanos);
        let bytes = hasher.finish().to_le_bytes();
        let n = chunk.len().min(bytes.len());
        chunk[..n].copy_from_slice(&bytes[..n]);
    }
}

/// What the stand-in of a test run answers for a sign-in at `authorize_url`: [`fake_answer`] of
/// the hooks' redirect when they have a fake sign-in (a headless or E2E TEST run; no browser
/// opens), `None` when they have not - every shipped build, whatever its environment holds - and
/// the real session starts.
fn answered_by_hooks(
    hooks: &azul_core::run_mode::TestHooks,
    authorize_url: &str,
    redirect_uri: &str,
) -> Option<AuthSessionResult> {
    hooks
        .fake_sign_in
        .then(|| fake_answer(hooks.fake_redirect.as_deref(), authorize_url, redirect_uri))
}

/// The session for `request`, started from the window `window`.
fn begin(request: &AuthRequest, window: RawWindowHandle) -> Begun {
    begin_with(azul_core::run_mode::test_hooks(), request, window)
}

/// [`begin`] under the hooks `hooks`: a request a real session would refuse is refused first, and
/// then the stand-in of a test run answers ([`answered_by_hooks`]) or the real session starts.
fn begin_with(
    hooks: &azul_core::run_mode::TestHooks,
    request: &AuthRequest,
    window: RawWindowHandle,
) -> Begun {
    let redirect_uri = request.redirect_uri.as_str().trim();
    let authorize_url = request.authorize_url.as_str().trim();
    let failed = |message: String| {
        Begun::Now(AuthSessionResult::ended(
            AuthSessionStatus::Failed,
            redirect_uri,
            &message,
        ))
    };
    if !(authorize_url.starts_with("https://") || authorize_url.starts_with("http://")) {
        return failed(String::from(
            "the authorize URL is no web address: a sign-in starts at the provider's https page",
        ));
    }
    let redirect = match auth::parse_redirect(redirect_uri) {
        Ok(redirect) => redirect,
        Err(why) => return failed(why),
    };
    if let Some(answer) = answered_by_hooks(hooks, authorize_url, redirect_uri) {
        crate::plog_info!(
            "[auth_session] headless: a sign-in at {} answered {:?}",
            host_of(authorize_url),
            answer.status
        );
        return Begun::Now(answer);
    }
    let timeout = Duration::from_secs(u64::from(request.effective_timeout_secs()));
    match redirect {
        Redirect::Loopback { .. } => start_loopback(&redirect, authorize_url, timeout),
        Redirect::CustomScheme { scheme } => {
            let url = match auth::with_redirect_uri(authorize_url, redirect_uri) {
                Ok(url) => url,
                Err(why) => return failed(why),
            };
            start_custom_scheme(
                window,
                &url,
                &scheme,
                redirect_uri,
                request.prefers_ephemeral,
                timeout,
            )
        }
    }
}

/// The host of `url` (what a log line may name; never the query).
fn host_of(url: &str) -> &str {
    let rest = url.split_once("://").map_or(url, |(_, rest)| rest);
    rest.split(['/', '?', '#']).next().unwrap_or_default()
}

/// What a headless test run answers.
///
/// `fake` (the value of [`FAKE_REDIRECT_VAR`]) with the request's redirect URI, state and code
/// challenge filled in, checked like a real redirect; `cancel` and `timeout` as those; nothing
/// set: Unsupported.
#[must_use]
pub fn fake_answer(
    fake: Option<&str>,
    authorize_url: &str,
    redirect_uri: &str,
) -> AuthSessionResult {
    let fake = fake.map(str::trim).unwrap_or_default();
    match fake {
        "" => AuthSessionResult::ended(
            AuthSessionStatus::Unsupported,
            redirect_uri,
            &format!(
                "a headless run opens no browser; it answers a sign-in from {FAKE_REDIRECT_VAR}, \
                 which is not set"
            ),
        ),
        "cancel" => AuthSessionResult::ended(
            AuthSessionStatus::Cancelled,
            redirect_uri,
            "the sign-in was cancelled",
        ),
        "timeout" => AuthSessionResult::ended(
            AuthSessionStatus::TimedOut,
            redirect_uri,
            "the sign-in did not come back in time",
        ),
        template => {
            let param = |name: &str| {
                azul_core::webview::url_query_param(authorize_url, name).unwrap_or_default()
            };
            let url = template
                .replace("{redirect_uri}", redirect_uri)
                .replace("{state}", &auth::percent_encode(&param("state")))
                .replace(
                    "{code_challenge}",
                    &auth::percent_encode(&param("code_challenge")),
                );
            auth::finish(&url, redirect_uri, authorize_url)
        }
    }
}

/// A loopback redirect: the listener first (its port goes into the authorize URL), then the
/// system browser; the listener's thread answers.
fn start_loopback(redirect: &Redirect, authorize_url: &str, timeout: Duration) -> Begun {
    let fallback_uri = redirect.uri_with_port(0).unwrap_or_default();
    let failed = |uri: &str, message: &str| {
        Begun::Now(AuthSessionResult::ended(
            AuthSessionStatus::Failed,
            uri,
            message,
        ))
    };
    if cfg!(any(target_os = "ios", target_os = "android")) {
        return Begun::Now(AuthSessionResult::ended(
            AuthSessionStatus::Unsupported,
            &fallback_uri,
            "a phone hands a sign-in back through a custom-scheme redirect \
             (com.example.app:/oauth2redirect), not a loopback one",
        ));
    }
    let listener = match auth::loopback::Loopback::bind(redirect) {
        Ok(listener) => listener,
        Err(why) => return failed(&fallback_uri, &why),
    };
    let redirect_uri = listener.redirect_uri().to_string();
    let url = match auth::with_redirect_uri(authorize_url, &redirect_uri) {
        Ok(url) => url,
        Err(why) => return failed(&redirect_uri, &why),
    };
    let deadline = Instant::now() + timeout;
    let (answer_tx, answer_rx) = channel();
    let state_source = authorize_url.to_string();
    let spawned = std::thread::Builder::new()
        .name("azul-auth-session".into())
        .spawn(move || {
            let result = listener.wait(deadline, &state_source);
            // Fails only when the request was dropped unanswered.
            let _ = answer_tx.send(result);
            crate::desktop::loop_waker::wake();
        });
    if spawned.is_err() {
        return failed(&redirect_uri, "the sign-in's listener could not start");
    }
    let opened = azul_core::url::Url {
        href: AzString::from(url),
        ..Default::default()
    }
    .open();
    if !opened {
        // The listener's thread ends at its deadline; nobody waits for it.
        return failed(
            &redirect_uri,
            "no web browser could be started for the sign-in",
        );
    }
    crate::plog_info!(
        "[auth_session] a sign-in at {} in the system browser, back to {}",
        host_of(authorize_url),
        host_of(&redirect_uri)
    );
    let lost_uri = redirect_uri;
    Begun::Later(Box::new(move || match answer_rx.try_recv() {
        Ok(result) => Some(RefAny::new(result)),
        Err(TryRecvError::Empty) => None,
        Err(TryRecvError::Disconnected) => Some(RefAny::new(AuthSessionResult::ended(
            AuthSessionStatus::Failed,
            &lost_uri,
            "the sign-in's listener ended without an answer",
        ))),
    }))
}

/// A custom-scheme redirect: the platform's session where there is one.
#[allow(unused_variables)] // every cfg arm consumes a different subset
fn start_custom_scheme(
    window: RawWindowHandle,
    url: &str,
    scheme: &str,
    redirect_uri: &str,
    prefers_ephemeral: bool,
    timeout: Duration,
) -> Begun {
    #[cfg(any(target_os = "macos", target_os = "ios"))]
    {
        match apple::start(
            window,
            url,
            scheme,
            redirect_uri,
            prefers_ephemeral,
            timeout,
        ) {
            Ok(poll) => Begun::Later(poll),
            Err(why) => Begun::Now(AuthSessionResult::ended(
                AuthSessionStatus::Failed,
                redirect_uri,
                &why,
            )),
        }
    }
    #[cfg(target_os = "android")]
    {
        match android::start(url, scheme, redirect_uri, prefers_ephemeral, timeout) {
            Ok(poll) => Begun::Later(poll),
            Err(why) => Begun::Now(AuthSessionResult::ended(
                AuthSessionStatus::Failed,
                redirect_uri,
                &why,
            )),
        }
    }
    #[cfg(not(any(target_os = "macos", target_os = "ios", target_os = "android")))]
    {
        Begun::Now(AuthSessionResult::ended(
            AuthSessionStatus::Unsupported,
            redirect_uri,
            "a custom-scheme redirect reaches no app on this platform: sign in with a loopback \
             redirect (http://127.0.0.1/...)",
        ))
    }
}

/// A poll that answers what `slot` holds once something wrote it, and `TimedOut` once
/// `deadline` passed without; `on_end(timed_out)` runs once, when it answers. The shape of the
/// platform sessions whose answer arrives in a callback (Apple's completion handler, Android's
/// JNI call).
#[allow(dead_code)] // not every platform has such a session
fn poll_slot(
    slot: std::sync::Arc<std::sync::Mutex<Option<AuthSessionResult>>>,
    deadline: Instant,
    redirect_uri: String,
    mut on_end: impl FnMut(bool) + Send + 'static,
) -> PollFn {
    Box::new(move || {
        let taken = slot
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .take();
        if let Some(result) = taken {
            on_end(false);
            return Some(RefAny::new(result));
        }
        if Instant::now() < deadline {
            return None;
        }
        on_end(true);
        Some(RefAny::new(AuthSessionResult::ended(
            AuthSessionStatus::TimedOut,
            &redirect_uri,
            "the sign-in did not come back in time",
        )))
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const AUTH: &str =
        "https://accounts.example/auth?client_id=c&code_challenge=Ch-al_l&state=St-4te";
    const REDIRECT: &str = "http://127.0.0.1/cb";

    /// The hooks of a headless test run whose browser comes back to `fake`.
    fn headless_hooks(fake: Option<&str>) -> azul_core::run_mode::TestHooks {
        azul_core::run_mode::TestHooks {
            fake_redirect: fake.map(String::from),
            ..azul_core::run_mode::TestHooks::HEADLESS
        }
    }

    /// The answer a sign-in gave at once.
    fn at_once(begun: Begun) -> AuthSessionResult {
        match begun {
            Begun::Now(result) => result,
            Begun::Later(_) => panic!("a session was started: it should have answered at once"),
        }
    }

    /// AZ-CN18: the fake answers ONLY for the hooks of a test run. A redirect variable alone is
    /// no switch - without `fake_sign_in` (every shipped build) the hooks answer nothing and the
    /// real session starts; a forged answer needs the test build.
    #[test]
    fn only_a_test_runs_hooks_answer_a_sign_in() {
        use azul_core::run_mode::TestHooks;

        let answer = answered_by_hooks(&headless_hooks(Some("cancel")), AUTH, REDIRECT)
            .expect("a headless test run answers");
        assert_eq!(answer.status, AuthSessionStatus::Cancelled);

        assert!(answered_by_hooks(&TestHooks::NONE, AUTH, REDIRECT).is_none());
        let only_the_variable = TestHooks {
            fake_redirect: Some(String::from("{redirect_uri}?code=forged&state={state}")),
            ..TestHooks::NONE
        };
        assert!(
            answered_by_hooks(&only_the_variable, AUTH, REDIRECT).is_none(),
            "the redirect variable without the stand-in switch forged a sign-in"
        );
    }

    /// The entry point under a test run's hooks: a request a real session would accept is
    /// answered from the redirect (the state and the code challenge are the request's); one it
    /// would refuse is refused before the fake looks at it.
    #[test]
    fn a_test_runs_sign_in_is_answered_from_the_redirect_after_the_request_is_checked() {
        let hooks = headless_hooks(Some(
            "{redirect_uri}?code=e2e-{code_challenge}&state={state}",
        ));
        let request = AuthRequest::create(AzString::from(AUTH), AzString::from(REDIRECT));
        let answer = at_once(begin_with(&hooks, &request, RawWindowHandle::Unsupported));
        assert_eq!(answer.status, AuthSessionStatus::Redirected);
        assert_eq!(
            answer.redirect_url.as_str(),
            "http://127.0.0.1/cb?code=e2e-Ch-al_l&state=St-4te"
        );

        let not_a_web_address = AuthRequest::create(
            AzString::from("file:///etc/passwd?state=St-4te"),
            AzString::from(REDIRECT),
        );
        let refused = at_once(begin_with(
            &hooks,
            &not_a_web_address,
            RawWindowHandle::Unsupported,
        ));
        assert_eq!(refused.status, AuthSessionStatus::Failed);

        let no_redirect = AuthRequest::create(AzString::from(AUTH), AzString::from("cb"));
        let refused = at_once(begin_with(
            &hooks,
            &no_redirect,
            RawWindowHandle::Unsupported,
        ));
        assert_eq!(refused.status, AuthSessionStatus::Failed);
    }

    #[test]
    fn a_headless_run_without_a_fake_redirect_answers_unsupported() {
        let answer = fake_answer(None, AUTH, REDIRECT);
        assert_eq!(answer.status, AuthSessionStatus::Unsupported);
        assert!(answer.message.as_str().contains(FAKE_REDIRECT_VAR));
        assert_eq!(
            fake_answer(Some("  "), AUTH, REDIRECT).status,
            AuthSessionStatus::Unsupported
        );
    }

    #[test]
    fn the_fake_redirect_fills_in_the_requests_redirect_uri_state_and_challenge() {
        let answer = fake_answer(
            Some("{redirect_uri}?code=e2e-{code_challenge}&state={state}"),
            AUTH,
            REDIRECT,
        );
        assert_eq!(answer.status, AuthSessionStatus::Redirected);
        assert_eq!(
            answer.redirect_url.as_str(),
            "http://127.0.0.1/cb?code=e2e-Ch-al_l&state=St-4te"
        );
        assert_eq!(answer.redirect_uri.as_str(), REDIRECT);
    }

    #[test]
    fn a_fake_redirect_with_another_state_or_address_fails_like_a_real_one() {
        let forged = fake_answer(Some("{redirect_uri}?code=x&state=forged"), AUTH, REDIRECT);
        assert_eq!(forged.status, AuthSessionStatus::Failed);
        let elsewhere = fake_answer(
            Some("http://127.0.0.1/other?code=x&state={state}"),
            AUTH,
            REDIRECT,
        );
        assert_eq!(elsewhere.status, AuthSessionStatus::Failed);
    }

    #[test]
    fn the_fake_answers_cancel_and_timeout() {
        assert_eq!(
            fake_answer(Some("cancel"), AUTH, REDIRECT).status,
            AuthSessionStatus::Cancelled
        );
        assert_eq!(
            fake_answer(Some("timeout"), AUTH, REDIRECT).status,
            AuthSessionStatus::TimedOut
        );
    }

    #[test]
    fn two_pkces_from_the_os_differ_and_are_complete() {
        let a = create_pkce();
        let b = create_pkce();
        assert_eq!(a.code_verifier.as_str().len(), 43);
        assert_eq!(a.state.as_str().len(), 22);
        assert_ne!(a.code_verifier, b.code_verifier);
        assert_ne!(a.state, b.state);
    }

    #[test]
    fn a_log_line_names_the_host_only() {
        assert_eq!(host_of(AUTH), "accounts.example");
        assert_eq!(host_of("http://127.0.0.1:5000/cb"), "127.0.0.1:5000");
    }
}
