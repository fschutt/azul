//! Add drive > Google Drive / Dropbox / OneDrive: "Sign in", as data (no azul types: tested
//! without a window; `add_flow` runs it).
//!
//! 1. The provider's OAuth client comes from the settings ([`SignInSettings`]): the environment
//!    (`AZDRIVE_GOOGLE_CLIENT_ID`, `AZDRIVE_DROPBOX_CLIENT_ID`, `AZDRIVE_ONEDRIVE_CLIENT_ID`, and
//!    `_CLIENT_SECRET`, `_REDIRECT_URI`, `_AUTHORIZE_URL`, `_TOKEN_URL`, `_SCOPE`) over the
//!    shared Azlin config's `oauth.<provider>` section (azul-appkit's `oauth_clients`). No client
//!    id is built into AzDrive; without one the button says which setting is missing
//!    ([`missing_client`]).
//! 2. A [`SignInPlan`]: the authorize endpoint, the client, the scope, the redirect URI (a
//!    loopback one with a free port unless the settings name another: the provider's app
//!    registration has to allow it), the token endpoint.
//! 3. azul's PKCE pair and state (`AuthPkce::create`), azul's `AuthSession` (the system browser
//!    and a loopback listener on the desktops, the platform's sign-in sheet for a custom-scheme
//!    redirect), the redirect read with `AuthPkce::read_redirect` (the state first).
//! 4. The code exchanged for tokens on a worker thread (azul-storage's `oauth::exchange_code`
//!    through `AzulTransport`: azul's HTTP client, to the provider's token endpoint only).
//! 5. [`form_settings`]: the refresh token into the form's secrets (the drive's keyring entry on
//!    Add drive, `azul-storage/s3/<drive id>` like every source's secrets), the client id into
//!    its settings; the drive refreshes its access token itself from then on (azul-storage's
//!    `RefreshingTransport`: before the first request and on a 401).

use std::{fmt, path::Path};

use azul_appkit::oauth_clients::{self, OAuthSettings};
use azul_storage::{
    oauth::{
        self, OAuthClient, OAuthProvider, Tokens, ACCESS_TOKEN, CLIENT_ID, CLIENT_SECRET,
        REFRESH_TOKEN,
    },
    sigv4::uri_encode,
};

/// The prefix of AzDrive's variables: `AZDRIVE_GOOGLE_CLIENT_ID`.
pub(crate) const ENV_PREFIX: &str = "AZDRIVE";

/// Where a sign-in comes back to unless the settings name another: this computer, a free port.
pub(crate) const DEFAULT_REDIRECT_URI: &str = "http://127.0.0.1/";

/// The OAuth settings of every provider, as this run resolved them (at the start).
#[derive(Clone, Debug, Default)]
pub(crate) struct SignInSettings {
    by_provider: Vec<(&'static str, OAuthSettings)>,
}

impl SignInSettings {
    /// The environment `var` reads over the shared config at `config_path` (none: no file).
    pub(crate) fn resolve(
        var: &dyn Fn(&str) -> Option<String>,
        config_path: Option<&Path>,
    ) -> SignInSettings {
        let _ = (var, config_path);
        SignInSettings::default()
    }

    /// The settings of `provider` (none: no opinion on anything).
    pub(crate) fn of(&self, provider: &OAuthProvider) -> OAuthSettings {
        self.by_provider
            .iter()
            .find(|(id, _)| *id == provider.id)
            .map(|(_, settings)| settings.clone())
            .unwrap_or_default()
    }
}

/// What a sign-in at a provider uses. `Debug` hides the client secret.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct SignInPlan {
    pub provider: &'static OAuthProvider,
    pub client: OAuthClient,
    /// The authorization endpoint (the provider's unless the settings name another).
    pub authorize_endpoint: String,
    /// The token endpoint (the provider's unless the settings name another).
    pub token_url: String,
    pub scope: String,
    pub redirect_uri: String,
}

impl SignInPlan {
    /// The parameters the provider needs besides the standard authorization request
    /// (`&access_type=offline&prompt=consent`), escaped, each with its `&`.
    pub(crate) fn extras_query(&self) -> String {
        String::new()
    }

    /// The token endpoint the drive keeps in its settings: none when it is the provider's own.
    pub(crate) fn token_url_option(&self) -> Option<&str> {
        None
    }
}

/// The sign-in of the OpenDAL service `scheme` with `settings`; why not, as a sentence that
/// names the missing setting.
pub(crate) fn plan(scheme: &str, settings: &SignInSettings) -> Result<SignInPlan, String> {
    let _ = (scheme, settings);
    Err(String::new())
}

/// The sentence a provider without a configured client shows: the variable and the config key.
pub(crate) fn missing_client(provider: &OAuthProvider) -> String {
    let _ = provider;
    String::new()
}

/// What the form shows of its sign-in.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum SignInStep {
    Idle,
    /// The browser (or the sign-in sheet) is open; the dialog waits for the redirect.
    Waiting,
    /// The code is being exchanged for tokens.
    Exchanging,
    /// The form holds the refresh token.
    SignedIn,
    /// Why the last sign-in ended without one.
    Failed(String),
}

/// A sign-in under way: what its redirect is read with and its code exchanged with. `Debug`
/// hides the verifier.
#[derive(Clone, PartialEq, Eq)]
pub(crate) struct PendingSignIn {
    pub plan: SignInPlan,
    pub code_verifier: String,
    pub code_challenge: String,
    pub state: String,
}

impl fmt::Debug for PendingSignIn {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("PendingSignIn")
            .field("plan", &self.plan)
            .field("code_verifier", &"<hidden>")
            .field("code_challenge", &self.code_challenge)
            .field("state", &self.state)
            .finish()
    }
}

/// The token endpoint's answer as the form's settings: the refresh token, the client (its
/// secret where it has one), no fixed access token. An answer without a refresh token can keep
/// no drive signed in: why, as a sentence.
pub(crate) fn form_settings(
    plan: &SignInPlan,
    tokens: &Tokens,
) -> Result<Vec<(&'static str, String)>, String> {
    let _ = (plan, tokens);
    Err(String::new())
}
