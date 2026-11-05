//! Where an Azlin app's trust comes from. This is the one file that says which keys a build
//! believes: they are CONSTANTS compiled into it, and a shipped build reads nothing else.
//!
//! The update chain used to end in a string from the shared config (`~/.azlin/config.json`, or
//! whatever `$AZLIN_CONFIG` named) - the lowest layer of a stack any process of the user can
//! write - and an empty string meant "no check": whoever could write that file, or answer at the
//! update address, installed what they liked (REVIEW Y1, Y2). Now:
//!
//! - [`UPDATE_ROOT_KEYS`] is the minisign ROOT public key (the current one and, during a
//!   rotation, the next). It is EMPTY until the owner's key ceremony - nothing is generated or
//!   guessed here - and an empty list is "updates are not available in this build"
//!   ([`NoRootKey`]), never "unsigned updates are fine".
//! - [`update_policy`] is the one function that turns an app's configuration into an
//!   [`UpdatePolicy`]. A build with the cargo feature `test-hooks` also reads the config's
//!   `updates.root_public_key` and `updates.allow_unsigned` (an E2E stack signs its fixtures with
//!   a throwaway key, or says it wants none); a shipped build never does.
//!
//! The same holds for the places a CREDENTIAL travels to (REVIEW K20, P7, X-AR5b-23): the token
//! server, the S3 endpoint and the OAuth endpoints of the providers. A drive token, a refresh
//! token or an authorization code goes to the address a run resolves; if that address came from a
//! file, a variable or a switch, whoever can write one of them chooses where the secret goes.
//! A shipped build therefore resolves them from constants ([`TOKEN_SERVER`], the providers'
//! table in azul-storage) and starts from the LIVE profile ([`DEFAULT_PROFILE`]), not from the
//! local stack; a `test-hooks` build keeps the layers (the profile's local stack, the file, the
//! environment, the switches) as before.
//!
//! No azul types here: tested without a window.

use std::fmt;

use crate::azlin_config::{Profile, UpdatesSection};

/// The live token server: the one address a shipped build sends a drive token to. The profile
/// table ([`Profile::Production`]) reads it from here.
pub const TOKEN_SERVER: &str = "https://token.azlin.io";

/// The profile whose addresses are the bottom layer when nothing names one: the LIVE service in a
/// shipped build, the local stack in a test build (`test-hooks`). A release that started from the
/// local profile talked to `http://127.0.0.1:8081` (REVIEW P7); a test that forgets its config
/// must be built with `test-hooks` anyway, and then it gets the local stack, never the live one.
pub const DEFAULT_PROFILE: Profile = if layers_honoured() {
    Profile::Local
} else {
    Profile::Production
};

/// Whether a run may take an address a credential goes to from its layers - the shared config's
/// `endpoints`, the environment, the switches - instead of from the build's constants. Only a
/// build with the cargo feature `test-hooks` may.
#[must_use]
pub const fn layers_honoured() -> bool {
    cfg!(feature = "test-hooks")
}

/// The token server an Azlin drive's refreshes go to. A shipped build: [`TOKEN_SERVER`] (what the
/// run resolved), whatever the drive's entry in `drives.json` names - a file a same-user process
/// can write must not redirect where a rotated drive token is sent. A test build: the one the
/// entry names, else the run's.
#[must_use]
pub fn token_server_for<'a>(named_by_entry: &'a str, resolved: &'a str) -> &'a str {
    token_server_choice(layers_honoured(), named_by_entry, resolved)
}

/// [`token_server_for`] with the build's choice as an argument (the tests set both).
fn token_server_choice<'a>(
    honour_entry: bool,
    named_by_entry: &'a str,
    resolved: &'a str,
) -> &'a str {
    let named = named_by_entry.trim();
    if honour_entry && !named.is_empty() {
        named
    } else {
        resolved.trim()
    }
}

/// The minisign ROOT public key(s) of every Azlin app, base64 as in a `.pub` file's second line:
/// the current one and, while a rotation is under way, the next.
///
/// PLACEHOLDER: this list is EMPTY until the owner's key ceremony (REVIEW Y1, ARCH28 question Q1).
/// Nothing generates a key pair or guesses a key; an empty list makes the whole update chain
/// refuse - the manager and the apps say "updates are not available in this build". To ship
/// updates, replace the empty list with the public key(s) the ceremony produced; the test
/// `every_compiled_root_key_parses` then pins that each one is a key the updater will read.
pub const UPDATE_ROOT_KEYS: &[&str] = &[];

/// The sentence every UI starts with when a build cannot prove an update.
pub const NOT_AVAILABLE: &str = "updates are not available in this build";

/// The root key(s) an update's signature chain ends in: one or more base64 minisign public keys,
/// each checked to be a key. There is no empty value and no `Default`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RootKeys(Vec<String>);

impl RootKeys {
    /// Reads one or more base64 keys separated by commas, semicolons or white space.
    ///
    /// # Errors
    ///
    /// Why the text is no list of keys: an empty text, or an entry that is no minisign public
    /// key (a typo in the second key does not leave the first one standing alone).
    pub fn parse(text: &str) -> Result<RootKeys, String> {
        let keys = text
            .split([',', ';', ' ', '\t', '\r', '\n'])
            .map(str::trim)
            .filter(|key| !key.is_empty())
            .map(|key| {
                minisign_verify::PublicKey::from_base64(key)
                    .map(|_| key.to_string())
                    .map_err(|e| format!("{key:?} is no minisign public key ({e})"))
            })
            .collect::<Result<Vec<_>, _>>()?;
        if keys.is_empty() {
            return Err("there is no key in it".to_string());
        }
        Ok(RootKeys(keys))
    }

    /// The keys as the text the update engine takes (comma-separated).
    #[must_use]
    pub fn as_text(&self) -> String {
        self.0.join(",")
    }
}

/// What an app's updates have to prove.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum UpdatePolicy {
    /// The whole chain: a root-signed signing-key statement and a signature of the artifact by
    /// the key it names. What a shipped build uses.
    Signed(RootKeys),
    /// The manifest's sha256 pin only: it protects against corruption and a swapped file, not
    /// against whoever can write the manifest. Only a test build can have it (a config that
    /// spells `updates.allow_unsigned`), and every UI says "unsigned".
    DigestOnly,
}

impl UpdatePolicy {
    /// Whether this policy checks a signature chain.
    #[must_use]
    pub fn is_signed(&self) -> bool {
        matches!(self, UpdatePolicy::Signed(_))
    }
}

/// Why a build has no [`UpdatePolicy`]: it cannot prove an update, so it installs none.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum NoRootKey {
    /// The build carries no root key ([`UPDATE_ROOT_KEYS`] is empty) and a test config spells
    /// none.
    Missing,
    /// A key is there but it is no key: why. An unreadable anchor refuses, it never means "no
    /// check".
    Unreadable(String),
}

impl fmt::Display for NoRootKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            NoRootKey::Missing => write!(f, "{NOT_AVAILABLE}: it carries no update root key"),
            NoRootKey::Unreadable(why) => {
                write!(
                    f,
                    "{NOT_AVAILABLE}: its update root key is unreadable ({why})"
                )
            }
        }
    }
}

impl std::error::Error for NoRootKey {}

/// The policy of the keys `keys` (a build's [`UPDATE_ROOT_KEYS`]).
fn policy_of(keys: &[&str]) -> Result<UpdatePolicy, NoRootKey> {
    if keys.is_empty() {
        return Err(NoRootKey::Missing);
    }
    RootKeys::parse(&keys.join(","))
        .map(UpdatePolicy::Signed)
        .map_err(NoRootKey::Unreadable)
}

/// What the config of a TEST build says (`updates.root_public_key`, `updates.allow_unsigned`);
/// `None` when it says nothing, so the compiled keys decide. A key beats `allow_unsigned`, and a
/// key that is no key is an error even beside it.
#[cfg(feature = "test-hooks")]
fn test_policy(updates: &UpdatesSection) -> Result<Option<UpdatePolicy>, NoRootKey> {
    let key = updates
        .raw("root_public_key")
        .and_then(serde_json::Value::as_str)
        .map(str::trim)
        .filter(|key| !key.is_empty());
    if let Some(text) = key {
        return RootKeys::parse(text)
            .map(|keys| Some(UpdatePolicy::Signed(keys)))
            .map_err(NoRootKey::Unreadable);
    }
    // collapse-ok: a missing or non-boolean choice is "not allowed", the refusing answer.
    let allow = updates
        .raw("allow_unsigned")
        .and_then(serde_json::Value::as_bool)
        .unwrap_or(false);
    Ok(allow.then_some(UpdatePolicy::DigestOnly))
}

/// The policy an app's configuration gives its updates: the compiled [`UPDATE_ROOT_KEYS`], and in
/// a test build (feature `test-hooks`) what `updates` of the shared config spells besides.
///
/// # Errors
///
/// [`NoRootKey`]: the build carries no root key, or its key is unreadable. Updates are then not
/// available: nothing is staged and nothing is installed.
pub fn update_policy(updates: &UpdatesSection) -> Result<UpdatePolicy, NoRootKey> {
    #[cfg(feature = "test-hooks")]
    if let Some(policy) = test_policy(updates)? {
        return Ok(policy);
    }
    // A shipped build does not look at the config for an anchor.
    #[cfg(not(feature = "test-hooks"))]
    let _ = updates;
    policy_of(UPDATE_ROOT_KEYS)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::azlin_config::AzlinConfig;

    /// minisign's own documentation key: a valid public key that signs nothing of ours.
    const KEY: &str = "RWQf6LRCGA9i53mlYecO4IzT51TGPpvWucNSCh1CBM0QTaLn73Y7GFO3";
    /// A second well-formed key (the documentation key with one character of its key bytes
    /// changed: a key as far as its text goes, a key of nobody).
    const OTHER: &str = "RWQf6LRCGA9i53mlYecO4IzT51TGPpUWucNSCh1CBM0QTaLn73Y7GFO3";

    fn updates(json: &str) -> UpdatesSection {
        AzlinConfig::parse(json).0.updates
    }

    /// REVIEW K20: where a rotated drive token goes is not a thing a file can say in a shipped build.
    #[test]
    fn k20_a_drive_entry_names_the_token_server_in_a_test_build_and_never_in_a_shipped_one() {
        let live = "https://token.azlin.io";
        assert_eq!(
            token_server_choice(false, "https://evil.example", live),
            live
        );
        assert_eq!(token_server_choice(false, "", live), live);
        assert_eq!(
            token_server_choice(true, "http://127.0.0.1:18081", live),
            "http://127.0.0.1:18081"
        );
        assert_eq!(
            token_server_choice(true, "  ", live),
            live,
            "an entry that names none"
        );
        assert_eq!(
            token_server_choice(true, "", " http://127.0.0.1:8081 "),
            "http://127.0.0.1:8081"
        );
        // And what this build does.
        let named = "http://127.0.0.1:18081";
        if layers_honoured() {
            assert_eq!(token_server_for(named, live), named);
        } else {
            assert_eq!(token_server_for(named, live), live);
        }
    }

    #[test]
    fn p7_the_default_profile_is_the_live_service_except_in_a_test_build() {
        if layers_honoured() {
            assert_eq!(DEFAULT_PROFILE, Profile::Local);
        } else {
            assert_eq!(DEFAULT_PROFILE, Profile::Production);
        }
        assert_eq!(
            Profile::Production.default_for(crate::azlin_config::Endpoint::Token),
            Some(TOKEN_SERVER)
        );
        assert!(TOKEN_SERVER.starts_with("https://"));
    }

    #[test]
    fn every_compiled_root_key_parses() {
        for key in UPDATE_ROOT_KEYS {
            assert!(RootKeys::parse(key).is_ok(), "{key}");
        }
        // And what the constant makes of it: no keys, no updates; keys, a signed policy.
        let keys = UPDATE_ROOT_KEYS.to_vec();
        let policy = policy_of(&keys);
        assert_eq!(policy.is_err(), keys.is_empty());
        if let Err(why) = policy {
            assert_eq!(
                why,
                NoRootKey::Missing,
                "a constant that does not parse is not a placeholder"
            );
        }
    }

    #[test]
    fn y1_no_key_is_no_updates_and_a_key_that_is_none_is_an_error_not_a_pass() {
        assert_eq!(policy_of(&[]), Err(NoRootKey::Missing));
        let signed = policy_of(&[KEY]).expect("a key");
        assert!(signed.is_signed());
        let rotating = policy_of(&[KEY, OTHER]).expect("the current and the next");
        assert!(
            matches!(&rotating, UpdatePolicy::Signed(k) if k.as_text() == format!("{KEY},{OTHER}"))
        );
        for bad in [&["junk"][..], &[KEY, "junk"][..], &[""][..], &["  "][..]] {
            assert!(
                matches!(policy_of(bad), Err(NoRootKey::Unreadable(_))),
                "{bad:?} is no key"
            );
        }
        // The sentence the UIs show.
        assert!(NoRootKey::Missing.to_string().starts_with(NOT_AVAILABLE));
        assert!(NoRootKey::Unreadable("x".into())
            .to_string()
            .contains("(x)"));
    }

    #[test]
    fn root_keys_are_read_from_a_list_and_a_typo_spoils_the_list() {
        assert_eq!(
            RootKeys::parse(&format!(" {KEY} ;\n{OTHER}"))
                .unwrap()
                .as_text(),
            format!("{KEY},{OTHER}")
        );
        assert!(RootKeys::parse("").is_err());
        assert!(RootKeys::parse(" , ; ").is_err());
        assert!(RootKeys::parse(&format!("{KEY} oops")).is_err());
    }

    /// REVIEW Y2: the config file is the lowest layer of the stack, writable by any process of
    /// the user. A shipped build must not take its update anchor from it.
    #[cfg(not(feature = "test-hooks"))]
    #[test]
    fn y2_a_shipped_build_ignores_the_root_key_and_the_unsigned_choice_in_the_config_file() {
        let planted = updates(&format!(
            r#"{{"updates": {{"root_public_key": "{KEY}", "allow_unsigned": true}}}}"#
        ));
        assert_eq!(update_policy(&planted), Err(NoRootKey::Missing));
        assert_eq!(
            update_policy(&UpdatesSection::default()),
            Err(NoRootKey::Missing)
        );
    }

    #[cfg(feature = "test-hooks")]
    #[test]
    fn a_test_build_takes_the_root_key_and_the_unsigned_choice_from_the_config() {
        let signed = updates(&format!(r#"{{"updates": {{"root_public_key": "{KEY}"}}}}"#));
        assert!(update_policy(&signed).expect("a key").is_signed());
        // A key wins over allow_unsigned: the choice never lowers a chain that is configured.
        let both = updates(&format!(
            r#"{{"updates": {{"root_public_key": "{KEY}", "allow_unsigned": true}}}}"#
        ));
        assert!(update_policy(&both).expect("a key").is_signed());
        // The one way to a policy without a key is to spell it out.
        let spelled = updates(r#"{"updates": {"allow_unsigned": true}}"#);
        assert_eq!(update_policy(&spelled), Ok(UpdatePolicy::DigestOnly));
        // A key that is no key refuses, allow_unsigned or not.
        let typo =
            updates(r#"{"updates": {"root_public_key": "RWQ-typo", "allow_unsigned": true}}"#);
        assert!(matches!(
            update_policy(&typo),
            Err(NoRootKey::Unreadable(_))
        ));
        // Nothing spelled, nothing compiled: no updates.
        assert_eq!(update_policy(&updates("{}")), policy_of(UPDATE_ROOT_KEYS));
        let falsey = updates(r#"{"updates": {"allow_unsigned": "yes"}}"#);
        assert_eq!(
            update_policy(&falsey),
            policy_of(UPDATE_ROOT_KEYS),
            "not a boolean: not allowed"
        );
    }
}
