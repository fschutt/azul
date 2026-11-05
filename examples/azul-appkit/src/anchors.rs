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
//! No azul types here: tested without a window.

use std::fmt;

use crate::azlin_config::UpdatesSection;

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

    #[test]
    fn every_compiled_root_key_parses() {
        for key in UPDATE_ROOT_KEYS {
            assert!(RootKeys::parse(key).is_ok(), "{key}");
        }
        // And what the constant makes of it: no keys, no updates; keys, a signed policy.
        let policy = policy_of(UPDATE_ROOT_KEYS);
        assert_eq!(policy.is_err(), UPDATE_ROOT_KEYS.first().is_none());
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
