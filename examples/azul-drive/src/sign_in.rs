//! Add drive > Google Drive / Dropbox / OneDrive: "Sign in", as data (no azul types: tested
//! without a window; `add_flow` runs it).
//!
//! 1. The provider's OAuth client comes from the settings ([`SignInSettings`]): the environment
//!    (`AZDRIVE_GOOGLE_CLIENT_ID`, `AZDRIVE_DROPBOX_CLIENT_ID`, `AZDRIVE_ONEDRIVE_CLIENT_ID`, and
//!    `_CLIENT_SECRET`, `_REDIRECT_URI`) over the shared Azlin config's `oauth.<provider>` section
//!    (azul-appkit's `oauth_clients`). No client id is built into AzDrive; without one the button
//!    says which setting is missing ([`missing_client`]). The provider's ENDPOINTS are not
//!    settings: the authorization code, the PKCE verifier and the client secret go to the token
//!    endpoint, so a shipped AzDrive uses azul-storage's provider table and ignores
//!    `_AUTHORIZE_URL`, `_TOKEN_URL` and `_SCOPE` (and a redirect that leaves this computer); a
//!    test build (`--features test-hooks`) takes them from the same places (REVIEW X-AR5b-23).
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

use azul_appkit::{
    l10n::{t_args, Arg},
    oauth_clients::{self, OAuthSettings},
};
use azul_storage::{
    config::{DriveEntry, DriveLocation, SecretOptions},
    keyring::KeyringStore,
    oauth::{
        self, OAuthClient, OAuthProvider, Revoked, Tokens, ACCESS_TOKEN, CLIENT_ID, CLIENT_SECRET,
        REFRESH_TOKEN,
    },
    sigv4::uri_encode,
    Transport,
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
        SignInSettings {
            by_provider: oauth::PROVIDERS
                .iter()
                .map(|p| {
                    (
                        p.id,
                        oauth_clients::resolve(ENV_PREFIX, p.id, var, config_path),
                    )
                })
                .collect(),
        }
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
        self.provider
            .authorize_extras
            .iter()
            .map(|(name, value)| format!("&{}={}", uri_encode(name, true), uri_encode(value, true)))
            .collect()
    }

    /// The token endpoint the drive keeps in its settings: none when it is the provider's own.
    pub(crate) fn token_url_option(&self) -> Option<&str> {
        (self.token_url != self.provider.token_url).then_some(self.token_url.as_str())
    }
}

/// The sign-in of the OpenDAL service `scheme` with `settings`; why not, as a sentence that
/// names the missing setting.
pub(crate) fn plan(scheme: &str, settings: &SignInSettings) -> Result<SignInPlan, String> {
    let provider = oauth::provider(scheme).ok_or_else(|| {
        t_args(
            "azdrive-add-no-sign-in-for",
            &[("scheme", Arg::from(scheme))],
        )
    })?;
    let own = settings.of(provider);
    let client_id = own
        .client_id
        .as_deref()
        .ok_or_else(|| missing_client(provider))?;
    let or = |value: Option<String>, default: &str| value.unwrap_or_else(|| default.to_string());
    Ok(SignInPlan {
        provider,
        client: OAuthClient::public(client_id).with_secret(own.client_secret.as_deref()),
        authorize_endpoint: or(own.authorize_url.clone(), provider.authorize_url),
        token_url: or(own.token_url.clone(), provider.token_url),
        scope: or(own.scope.clone(), provider.scope),
        redirect_uri: or(own.redirect_uri.clone(), DEFAULT_REDIRECT_URI),
    })
}

/// The sentence a provider without a configured client shows: the variable and the config key.
pub(crate) fn missing_client(provider: &OAuthProvider) -> String {
    let setting = format!(
        "\"oauth\": {{\"{}\": {{\"client_id\": \"...\"}}}}",
        provider.id
    );
    t_args(
        "azdrive-add-missing-client",
        &[
            ("provider", Arg::from(provider.name)),
            (
                "variable",
                Arg::from(oauth_clients::env_var(ENV_PREFIX, provider.id, "client_id")),
            ),
            ("setting", Arg::from(setting)),
        ],
    )
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
    let refresh_token = tokens
        .refresh_token
        .as_deref()
        .map(str::trim)
        .filter(|t| !t.is_empty())
        .ok_or_else(|| {
            t_args(
                "azdrive-add-no-refresh-token",
                &[("provider", Arg::from(plan.provider.name))],
            )
        })?;
    Ok(vec![
        (REFRESH_TOKEN, refresh_token.to_string()),
        (CLIENT_ID, plan.client.client_id.clone()),
        (
            CLIENT_SECRET,
            plan.client.client_secret.clone().unwrap_or_default(),
        ),
        (ACCESS_TOKEN, String::new()),
    ])
}

/// The provider the drive `entry` signs in at (Google Drive, Dropbox, OneDrive); `None` for every
/// other drive.
pub(crate) fn provider_of(entry: &DriveEntry) -> Option<&'static OAuthProvider> {
    match &entry.location {
        DriveLocation::Opendal { scheme, .. } => oauth::provider(scheme),
        _ => None,
    }
}

// ==== Ending a sign-in (Remove drive) ====

/// The sign-in of a drive that is being removed: what ending it needs. Made on the UI thread
/// ([`grant_of`]), used on a worker ([`end_grant`]).
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Grant {
    pub provider: &'static OAuthProvider,
    /// The client id the drive signed in with (its plain options).
    pub client_id: String,
    /// The keyring entry that holds its refresh token.
    pub secret_entry: String,
    /// Another drive of this computer is signed in at the same provider: the grant may be theirs
    /// too (a provider can tie every token of an account and client to one grant), so it is kept.
    pub shared: bool,
}

/// What ending a drive's sign-in came to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum GrantEnd {
    /// The provider ended it (or had ended it already).
    Ended,
    /// Another drive of this computer is signed in at the provider: the grant stays.
    Kept { provider: &'static str },
    /// The provider cannot end it from here: the user does, at this page.
    InAccount {
        provider: &'static str,
        page: &'static str,
    },
    /// It could not be ended: why. It stays valid until the user ends it in the provider's
    /// account.
    NotEnded { provider: &'static str, why: String },
    /// The drive holds no refresh token (it never signed in, or the entry is gone): nothing to
    /// end.
    NothingToEnd,
}

/// The sign-in of the drive `entry` to end when it is removed; `others` are the entries of the
/// other drives. `None` for a drive that signs in nowhere.
pub(crate) fn grant_of<'a>(
    entry: &DriveEntry,
    others: impl IntoIterator<Item = &'a DriveEntry>,
) -> Option<Grant> {
    let provider = provider_of(entry)?;
    let client_id = match &entry.location {
        DriveLocation::Opendal { options, .. } => {
            options.get(CLIENT_ID).cloned().unwrap_or_default()
        }
        _ => String::new(),
    };
    let shared = others
        .into_iter()
        .any(|other| other.id != entry.id && provider_of(other) == Some(provider));
    Some(Grant {
        provider,
        client_id,
        secret_entry: azul_storage::config::keyring_key(&entry.id),
        shared,
    })
}

/// Ends the sign-in `grant` at its provider, with the refresh token the keyring holds, over
/// `transport`: the keyring is read before anything of the drive is deleted. A grant that is
/// shared with another drive stays. Never an error: what the provider did, or why not, is the
/// answer, because the removal goes on either way and tells the user.
pub(crate) fn end_grant(
    grant: &Grant,
    keyring: &dyn KeyringStore,
    transport: &dyn Transport,
) -> GrantEnd {
    let provider = grant.provider.name;
    if grant.shared {
        return GrantEnd::Kept { provider };
    }
    let text = match keyring.get(&grant.secret_entry) {
        Ok(Some(text)) => text,
        Ok(None) => return GrantEnd::NothingToEnd,
        Err(e) => {
            return GrantEnd::NotEnded {
                provider,
                why: e.to_string(),
            }
        }
    };
    let secrets = match SecretOptions::from_keyring_secret(&text) {
        Ok(secrets) => secrets,
        Err(e) => {
            return GrantEnd::NotEnded {
                provider,
                why: e.to_string(),
            }
        }
    };
    let Some(refresh_token) = secrets
        .get(REFRESH_TOKEN)
        .map(str::trim)
        .filter(|token| !token.is_empty())
    else {
        return GrantEnd::NothingToEnd;
    };
    let client = OAuthClient::public(&grant.client_id).with_secret(secrets.get(CLIENT_SECRET));
    match oauth::revoke(
        transport,
        grant.provider,
        grant.provider.token_url,
        &client,
        refresh_token,
    ) {
        Ok(Revoked::Done) => GrantEnd::Ended,
        Ok(Revoked::InAccount(page)) => GrantEnd::InAccount { provider, page },
        Err(e) => GrantEnd::NotEnded {
            provider,
            why: e.to_string(),
        },
    }
}

#[cfg(test)]
mod tests {
    use std::{collections::BTreeMap, sync::Mutex};

    use azul_storage::{
        keyring::{KeyringStore, MemoryKeyring},
        HttpCall, HttpReply, Method,
    };

    use super::*;

    fn drive(id: &str, scheme: &str, client_id: Option<&str>) -> DriveEntry {
        DriveEntry {
            id: id.to_string(),
            name: format!("Drive {id}"),
            location: DriveLocation::Opendal {
                scheme: scheme.to_string(),
                options: client_id
                    .map(|c| BTreeMap::from([(CLIENT_ID.to_string(), c.to_string())]))
                    .unwrap_or_default(),
                keyring: true,
            },
        }
    }

    /// A provider that answers what it is told to and remembers what it was asked.
    struct Provider {
        answers: Mutex<Vec<Result<HttpReply, String>>>,
        calls: Mutex<Vec<HttpCall>>,
    }

    impl Provider {
        fn answering(answers: Vec<Result<HttpReply, String>>) -> Provider {
            Provider {
                answers: Mutex::new(answers),
                calls: Mutex::new(Vec::new()),
            }
        }
    }

    impl Transport for Provider {
        fn send(&self, call: &HttpCall) -> Result<HttpReply, String> {
            self.calls.lock().unwrap().push(call.clone());
            let mut answers = self.answers.lock().unwrap();
            if answers.is_empty() {
                Err(String::from("no scripted answer"))
            } else {
                answers.remove(0)
            }
        }
    }

    fn ok() -> Result<HttpReply, String> {
        Ok(HttpReply {
            status: 200,
            headers: Vec::new(),
            body: Vec::new(),
        })
    }

    fn kept(keyring: &MemoryKeyring, id: &str, refresh_token: &str) {
        let mut secrets = SecretOptions::default();
        secrets.insert(REFRESH_TOKEN, refresh_token);
        keyring
            .set(
                &azul_storage::config::keyring_key(id),
                &secrets.to_keyring_secret(),
            )
            .unwrap();
    }

    /// AZ-CN12: Remove drive left the grant live at the provider. Now it is ended with the
    /// refresh token the keyring holds - Google gets one form POST of it - and the answer says so.
    #[test]
    fn az_cn12_removing_a_google_drive_ends_its_grant_with_the_refresh_token_the_keyring_holds() {
        let entry = drive("gd_1", "gdrive", Some("app.example"));
        let grant = grant_of(&entry, [&entry]).expect("a Google Drive signs in");
        assert!(!grant.shared, "the only drive signed in at Google");
        assert_eq!(grant.client_id, "app.example");
        let keyring = MemoryKeyring::new();
        kept(&keyring, "gd_1", "1//the refresh token");
        let provider = Provider::answering(vec![ok()]);

        assert_eq!(end_grant(&grant, &keyring, &provider), GrantEnd::Ended);
        let calls = provider.calls.lock().unwrap();
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].method, Method::Post);
        assert_eq!(calls[0].url, "https://oauth2.googleapis.com/revoke");
        assert_eq!(
            String::from_utf8(calls[0].body.clone()).unwrap(),
            "token=1%2F%2Fthe%20refresh%20token"
        );
    }

    /// AZ-CN12: what could not be ended is said and never an error that stops the removal: a
    /// provider that does not answer, one that has no endpoint for it (OneDrive), a keyring
    /// without the entry, a drive that never signed in.
    #[test]
    fn az_cn12_a_grant_that_cannot_be_ended_is_said_not_thrown() {
        let keyring = MemoryKeyring::new();
        let google = drive("gd_1", "gdrive", Some("app.example"));
        let grant = grant_of(&google, [&google]).unwrap();
        // no answer at all
        kept(&keyring, "gd_1", "r");
        let offline = Provider::answering(vec![Err(String::from("offline"))]);
        match end_grant(&grant, &keyring, &offline) {
            GrantEnd::NotEnded { provider, why } => {
                assert_eq!(provider, "Google Drive");
                assert!(why.contains("offline"), "{why}");
            }
            other => panic!("{other:?}"),
        }
        // OneDrive: nothing is sent, the page where the user ends it is named
        let onedrive = drive("od_1", "onedrive", Some("app.example"));
        let grant = grant_of(&onedrive, [&onedrive]).unwrap();
        kept(&keyring, "od_1", "r");
        let silent = Provider::answering(Vec::new());
        assert_eq!(
            end_grant(&grant, &keyring, &silent),
            GrantEnd::InAccount {
                provider: "OneDrive",
                page: "https://account.live.com/consent/Manage"
            }
        );
        assert!(silent.calls.lock().unwrap().is_empty());
        // the entry is not there: nothing to end, nothing sent
        let grant = grant_of(&drive("gd_2", "gdrive", None), []).unwrap();
        assert_eq!(end_grant(&grant, &keyring, &silent), GrantEnd::NothingToEnd);
        assert!(silent.calls.lock().unwrap().is_empty());
    }

    /// AZ-CN12: two drives signed in at one provider may hold one grant (a provider can tie every
    /// token of an account and client together): removing one keeps it and says so, so the other
    /// is not signed out; a drive that signs in nowhere has none to end.
    #[test]
    fn az_cn12_a_grant_another_drive_may_share_is_kept_and_a_local_drive_has_none() {
        let a = drive("gd_1", "gdrive", Some("app.example"));
        let b = drive("gd_2", "gdrive", Some("app.example"));
        let dropbox = drive("db_1", "dropbox", Some("app"));
        let grant = grant_of(&a, [&a, &b, &dropbox]).unwrap();
        assert!(grant.shared);
        let keyring = MemoryKeyring::new();
        kept(&keyring, "gd_1", "r");
        let silent = Provider::answering(Vec::new());
        assert_eq!(
            end_grant(&grant, &keyring, &silent),
            GrantEnd::Kept {
                provider: "Google Drive"
            }
        );
        assert!(silent.calls.lock().unwrap().is_empty());
        // a Dropbox drive next to two Google drives has a grant of its own
        assert!(!grant_of(&dropbox, [&a, &b, &dropbox]).unwrap().shared);
        let local = DriveEntry {
            id: String::from("l_1"),
            name: String::from("Folder"),
            location: DriveLocation::Local {
                root: String::from("/tmp/x"),
            },
        };
        assert!(provider_of(&local).is_none() && grant_of(&local, [&a]).is_none());
    }
}
