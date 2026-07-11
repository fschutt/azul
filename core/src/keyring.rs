//! POD types for the system-keyring surface
//! (SUPER_PLAN_2 §4 P4.2 + research/02 §0 "hardware-bound" storage).
//!
//! A biometry-bindable secret key/value store backed by the OS keyring:
//! iOS/macOS Keychain (`SecItem*`, optionally `kSecAttrAccessControl =
//! biometryCurrentSet`), Android `KeyStore` (`setUserAuthenticationRequired`),
//! Linux libsecret, Windows `CredentialLocker`. Defined here in `azul-core`
//! so the request/result types cross the FFI without `azul-layout` being a
//! dependency; the stateful side lives in
//! `azul_layout::managers::keyring::KeyringManager`.
//!
//! Request-driven and channel-delivered, mirroring biometric ([`crate::
//! biometric`]): a `Get` of a biometry-bound item shows the OS prompt and
//! resolves asynchronously, so *every* op resolves through the result
//! channel for a uniform, engine-agnostic surface. One op is in flight at
//! a time (the demo reveals one entry at a time); request↔result
//! correlation by id is a future refinement.

use azul_css::AzString;

/// A keyring operation queued by a callback
/// (`CallbackInfo::keyring_store` / `keyring_get` / `keyring_delete`) and
/// dispatched to the platform backend by the layout pass.
///
/// `secret` is an [`AzString`] — the common case is a password / token;
/// binary blobs are base64-encoded by the caller. `key` is the lookup
/// name, scoped to the app's keyring service.
#[repr(C, u8)]
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum KeyringRequest {
    /// Write `secret` under `key`, overwriting any existing value. When
    /// `require_biometry` is set the item is stored access-controlled so a
    /// later `Get` triggers the OS biometric prompt (Keychain
    /// `biometryCurrentSet` / `KeyStore` `setUserAuthenticationRequired`).
    Store {
        key: AzString,
        secret: AzString,
        require_biometry: bool,
    },
    /// Read the secret stored under `key`. For a biometry-bound item the
    /// OS shows its auth prompt first; the result arrives asynchronously.
    Get { key: AzString },
    /// Remove the item stored under `key` (no-op if absent).
    Delete { key: AzString },
}
#[allow(variant_size_differences)] // repr(C,u8) FFI enum: boxing the large variant would change the C ABI (api.json bindings); size disparity accepted
/// The outcome of a [`KeyringRequest`], delivered to the result channel
/// and read by callbacks via `CallbackInfo::get_keyring_result()`.
#[repr(C, u8)]
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum KeyringResult {
    /// A `Store` succeeded.
    Stored,
    /// A `Get` returned the secret.
    Retrieved(AzString),
    /// A `Delete` succeeded (the key is now absent).
    Deleted,
    /// The requested key was not present in the keyring.
    NotFound,
    /// A biometry-bound read was refused — the user failed or cancelled
    /// the OS auth prompt.
    Denied,
    /// No keyring backend is available on this platform / it isn't
    /// configured (e.g. Linux without a running secret service).
    Unavailable,
    /// A platform error occurred (locked keychain, I/O, unmapped code).
    Error,
}

impl KeyringResult {
    /// The retrieved secret, if this is a successful `Get`.
    #[must_use] pub const fn secret(&self) -> Option<&AzString> {
        match self {
            Self::Retrieved(s) => Some(s),
            _ => None,
        }
    }

    /// `true` for the success outcomes (`Stored` / `Retrieved` / `Deleted`).
    #[must_use] pub const fn is_ok(&self) -> bool {
        matches!(
            self,
            Self::Stored | Self::Retrieved(_) | Self::Deleted
        )
    }
}

// FFI Option wrapper for `CallbackInfo::get_keyring_result() ->
// Option<KeyringResult>` — `None` until the first op completes. Not Copy
// (carries an `AzString` in `Retrieved`), so `copy = false` (mirrors
// `OptionNodeType`).
impl_option!(
    KeyringResult,
    OptionKeyringResult,
    copy = false,
    [Debug, Clone, PartialEq, Eq]
);

#[cfg(test)]
mod autotest_generated {
    use super::*;
    use alloc::string::String;

    // Payloads chosen to break naive C-string / byte-length assumptions in the
    // FFI layer: interior NUL, C0/C1 controls, CRLF.
    const NASTY_BYTES: &str = "pw\0with\u{1}nul\u{7f}\r\n\t";
    // Combining marks, RTL overrides, replacement char and the maximum scalar.
    const NASTY_UNICODE: &str = "🔑🗝 ключ 鍵 مفتاح e\u{301}\u{202e}terces\u{202c}\u{fffd}\u{10ffff}";

    /// Every `KeyringResult` variant, in declaration order.
    fn all_variants() -> [KeyringResult; 7] {
        [
            KeyringResult::Stored,
            KeyringResult::Retrieved(AzString::from("s3cr3t")),
            KeyringResult::Deleted,
            KeyringResult::NotFound,
            KeyringResult::Denied,
            KeyringResult::Unavailable,
            KeyringResult::Error,
        ]
    }

    /// The documented contract, restated independently of the impl:
    /// `(is_ok, has_secret)`. The exhaustive match is deliberate — adding a
    /// variant to `KeyringResult` breaks this and forces the truth tables
    /// below to be revisited rather than silently under-testing the new one.
    fn expected(r: &KeyringResult) -> (bool, bool) {
        match r {
            KeyringResult::Stored => (true, false),
            KeyringResult::Retrieved(_) => (true, true),
            KeyringResult::Deleted => (true, false),
            KeyringResult::NotFound => (false, false),
            KeyringResult::Denied => (false, false),
            KeyringResult::Unavailable => (false, false),
            KeyringResult::Error => (false, false),
        }
    }

    // ---- getter: KeyringResult::secret ------------------------------------

    #[test]
    fn secret_returns_the_exact_payload_of_retrieved() {
        let r = KeyringResult::Retrieved(AzString::from("hunter2"));
        assert_eq!(r.secret().expect("Retrieved must yield a secret").as_str(), "hunter2");
    }

    #[test]
    fn secret_is_none_for_every_non_retrieved_variant() {
        for r in &all_variants() {
            let has_secret = expected(r).1;
            assert_eq!(
                r.secret().is_some(),
                has_secret,
                "secret() disagrees with the documented contract for {r:?}"
            );
        }
    }

    #[test]
    fn secret_of_an_empty_payload_is_some_empty_not_none() {
        // An empty secret is a *present* secret. Collapsing it to `None` would
        // make a stored-empty-string indistinguishable from `NotFound`.
        let r = KeyringResult::Retrieved(AzString::from(""));
        assert_eq!(r.secret().expect("empty payload is still a payload").as_str(), "");
        assert!(r.is_ok());

        let d = KeyringResult::Retrieved(AzString::default());
        assert_eq!(d.secret().expect("default payload is still a payload").as_str(), "");
        assert!(d.is_ok());
    }

    #[test]
    fn secret_preserves_interior_nul_and_control_bytes() {
        let r = KeyringResult::Retrieved(AzString::from(NASTY_BYTES));
        let s = r.secret().expect("Retrieved must yield a secret");
        assert_eq!(s.as_str(), NASTY_BYTES);
        // Byte-exact: nothing truncated at the NUL, nothing re-encoded.
        assert_eq!(s.as_str().as_bytes(), NASTY_BYTES.as_bytes());
        assert_eq!(s.as_str().len(), NASTY_BYTES.len());
    }

    #[test]
    fn secret_preserves_multibyte_and_max_scalar_boundaries() {
        let r = KeyringResult::Retrieved(AzString::from(NASTY_UNICODE));
        let s = r.secret().expect("Retrieved must yield a secret");
        // `AzString::as_str` is `from_utf8_unchecked`, so a byte-level mangling
        // here would be UB rather than a clean error — assert byte equality.
        assert_eq!(s.as_str(), NASTY_UNICODE);
        assert_eq!(s.as_str().as_bytes(), NASTY_UNICODE.as_bytes());
        assert_eq!(s.as_str().chars().count(), NASTY_UNICODE.chars().count());
        assert!(s.as_str().ends_with('\u{10ffff}'));
    }

    #[test]
    fn secret_handles_a_megabyte_payload() {
        let big = "k".repeat(1 << 20);
        let r = KeyringResult::Retrieved(AzString::from(big.clone()));
        let s = r.secret().expect("Retrieved must yield a secret");
        assert_eq!(s.as_str().len(), 1 << 20);
        assert_eq!(s.as_str(), big.as_str());
        assert!(r.is_ok());
    }

    #[test]
    fn secret_borrows_the_payload_in_place_and_is_repeatable() {
        let r = KeyringResult::Retrieved(AzString::from("borrow-me"));
        let inner: &AzString = match &r {
            KeyringResult::Retrieved(s) => s,
            other => unreachable!("constructed Retrieved, got {other:?}"),
        };

        let first = r.secret().expect("Retrieved must yield a secret");
        assert!(
            core::ptr::eq(first, inner),
            "secret() must borrow the payload, not hand back a copy"
        );

        // Repeated calls are stable: same AzString, same backing buffer.
        let second = r.secret().expect("secret() must be repeatable");
        assert!(core::ptr::eq(first, second));
        assert_eq!(first.as_str().as_ptr(), second.as_str().as_ptr());
    }

    // ---- predicate: KeyringResult::is_ok ----------------------------------

    #[test]
    fn is_ok_truth_table_is_exhaustive() {
        for r in &all_variants() {
            let want = expected(r).0;
            assert_eq!(r.is_ok(), want, "is_ok() wrong for {r:?}");
        }

        // Spelled out as well, so a mis-edited `expected()` can't hide a bug.
        assert!(KeyringResult::Stored.is_ok());
        assert!(KeyringResult::Retrieved(AzString::from("x")).is_ok());
        assert!(KeyringResult::Deleted.is_ok());
        assert!(!KeyringResult::NotFound.is_ok());
        assert!(!KeyringResult::Denied.is_ok());
        assert!(!KeyringResult::Unavailable.is_ok());
        assert!(!KeyringResult::Error.is_ok());
    }

    #[test]
    fn is_ok_is_independent_of_the_retrieved_payload() {
        let payloads = [
            String::new(),
            String::from(NASTY_BYTES),
            String::from(NASTY_UNICODE),
            "a".repeat(100_000),
        ];
        for p in payloads {
            assert!(
                KeyringResult::Retrieved(AzString::from(p)).is_ok(),
                "Retrieved is a success outcome whatever it carries"
            );
        }
    }

    #[test]
    fn is_ok_and_secret_never_contradict_each_other() {
        for r in &all_variants() {
            // A secret can only come from a success; a failure never carries one.
            if r.secret().is_some() {
                assert!(r.is_ok(), "{r:?} yielded a secret but reports failure");
            }
            if !r.is_ok() {
                assert!(r.secret().is_none(), "failure {r:?} yielded a secret");
            }
        }
    }

    #[test]
    fn accessors_are_pure() {
        for r in &all_variants() {
            let before = r.clone();
            for _ in 0..8 {
                assert_eq!(r.is_ok(), before.is_ok());
                assert_eq!(r.secret().is_some(), before.secret().is_some());
            }
            assert_eq!(*r, before, "accessors must not mutate the receiver");
        }
    }

    // ---- clone/drop: AzString carries an FFI destructor ------------------

    #[test]
    fn dropping_a_clone_leaves_the_original_payload_readable() {
        let payload = "🔐 secret-that-must-survive";
        let original = KeyringResult::Retrieved(AzString::from(payload));

        for _ in 0..1000 {
            let c = original.clone();
            assert_eq!(c.secret().expect("clone keeps the payload").as_str(), payload);
            // A shallow clone would free the original's buffer right here.
            drop(c);
        }

        assert_eq!(
            original.secret().expect("original survives 1000 clone/drop cycles").as_str(),
            payload
        );
        assert!(original.is_ok());
    }

    #[test]
    fn dropping_the_original_leaves_the_clone_payload_readable() {
        let payload = NASTY_UNICODE;
        let original = KeyringResult::Retrieved(AzString::from(payload));
        let clone = original.clone();
        drop(original);
        assert_eq!(clone.secret().expect("clone owns its payload").as_str(), payload);
    }

    #[test]
    fn eq_is_sensitive_to_both_variant_and_payload() {
        assert_eq!(
            KeyringResult::Retrieved(AzString::from("a")),
            KeyringResult::Retrieved(AzString::from("a"))
        );
        assert_ne!(
            KeyringResult::Retrieved(AzString::from("a")),
            KeyringResult::Retrieved(AzString::from("b"))
        );
        // Empty payload must not compare equal to a payload-less success.
        assert_ne!(KeyringResult::Retrieved(AzString::from("")), KeyringResult::Stored);
        assert_ne!(KeyringResult::NotFound, KeyringResult::Denied);
        assert_ne!(KeyringResult::Unavailable, KeyringResult::Error);
        assert_eq!(KeyringResult::Deleted, KeyringResult::Deleted);
    }

    // ---- round-trip: OptionKeyringResult FFI wrapper ----------------------

    #[test]
    fn option_wrapper_round_trips_none_and_every_variant() {
        let none: OptionKeyringResult = Option::<KeyringResult>::None.into();
        assert!(none.is_none());
        assert_eq!(Option::<KeyringResult>::from(none), None);
        assert!(OptionKeyringResult::default().is_none());

        for r in all_variants() {
            let wrapped: OptionKeyringResult = Some(r.clone()).into();
            let back: Option<KeyringResult> = wrapped.into();
            assert_eq!(back, Some(r), "encode/decode through OptionKeyringResult lost data");
        }
    }

    #[test]
    fn option_wrapper_accessors_agree_with_each_other() {
        let payload = "round-trip-me";
        let some = OptionKeyringResult::Some(KeyringResult::Retrieved(AzString::from(payload)));
        assert!(some.is_some());
        assert!(!some.is_none());
        assert_eq!(
            some.as_option().and_then(KeyringResult::secret).map(AzString::as_str),
            Some(payload)
        );
        assert_eq!(some.as_ref(), some.as_option());

        let none = OptionKeyringResult::None;
        assert!(none.is_none());
        assert!(!none.is_some());
        assert!(none.as_option().is_none());
    }

    #[test]
    fn option_wrapper_into_option_clones_and_leaves_the_wrapper_intact() {
        // `into_option(&self)` takes a reference and clones — calling it twice
        // must not double-free or hollow out the wrapper.
        let payload = NASTY_BYTES;
        let wrapped = OptionKeyringResult::Some(KeyringResult::Retrieved(AzString::from(payload)));

        let first = wrapped.into_option().expect("Some stays Some");
        let second = wrapped.into_option().expect("into_option must not consume the wrapper");
        assert_eq!(first, second);
        assert_eq!(first.secret().expect("payload survives").as_str(), payload);
        drop(first);
        assert_eq!(second.secret().expect("payload survives its sibling").as_str(), payload);
        drop(second);
        assert_eq!(
            wrapped.as_option().and_then(KeyringResult::secret).map(AzString::as_str),
            Some(payload),
            "wrapper must still own its payload after both clones died"
        );
    }

    #[test]
    fn option_wrapper_replace_returns_the_previous_value() {
        let mut o = OptionKeyringResult::None;
        assert_eq!(o.replace(KeyringResult::Stored), OptionKeyringResult::None);

        let prev = o.replace(KeyringResult::Retrieved(AzString::from("new")));
        assert_eq!(prev, OptionKeyringResult::Some(KeyringResult::Stored));
        assert_eq!(
            o.as_option().and_then(KeyringResult::secret).map(AzString::as_str),
            Some("new")
        );
    }

    #[test]
    fn option_wrapper_as_mut_can_overwrite_a_retrieved_payload() {
        let mut o = OptionKeyringResult::Some(KeyringResult::Retrieved(AzString::from("old")));
        if let Some(slot) = o.as_mut() {
            *slot = KeyringResult::Deleted; // drops the old AzString in place
        }
        assert_eq!(o.as_option(), Some(&KeyringResult::Deleted));
        assert!(o.as_option().expect("still Some").is_ok());
        assert!(o.as_option().expect("still Some").secret().is_none());
    }

    // ---- KeyringRequest: payload-carrying FFI enum ------------------------

    #[test]
    fn request_equality_is_sensitive_to_key_secret_and_biometry_flag() {
        let store = KeyringRequest::Store {
            key: AzString::from("k"),
            secret: AzString::from("s"),
            require_biometry: false,
        };
        assert_eq!(store, store.clone());

        assert_ne!(
            store,
            KeyringRequest::Store {
                key: AzString::from("k"),
                secret: AzString::from("s"),
                require_biometry: true,
            },
            "require_biometry changes the stored access control — it must not be ignored"
        );
        assert_ne!(
            store,
            KeyringRequest::Store {
                key: AzString::from("k"),
                secret: AzString::from("other"),
                require_biometry: false,
            }
        );
        assert_ne!(
            KeyringRequest::Get { key: AzString::from("k") },
            KeyringRequest::Delete { key: AzString::from("k") },
            "a read and a delete of the same key are different operations"
        );
        assert_ne!(
            KeyringRequest::Get { key: AzString::from("a") },
            KeyringRequest::Get { key: AzString::from("b") }
        );
    }

    #[test]
    fn request_payloads_survive_clone_and_drop() {
        let key = "🔑".repeat(10_000);
        let req = KeyringRequest::Store {
            key: AzString::from(key.clone()),
            secret: AzString::from(NASTY_BYTES),
            require_biometry: true,
        };

        let clone = req.clone();
        drop(req);

        match &clone {
            KeyringRequest::Store { key: k, secret: s, require_biometry } => {
                assert_eq!(k.as_str(), key.as_str());
                assert_eq!(s.as_str(), NASTY_BYTES);
                assert!(*require_biometry);
            }
            other => unreachable!("constructed Store, got {other:?}"),
        }
    }

    #[test]
    fn request_accepts_empty_keys_without_panicking() {
        // An empty key is nonsense to a backend, but the POD type must still
        // construct, compare and drop cleanly rather than blow up in the callback.
        let empty = KeyringRequest::Get { key: AzString::from("") };
        assert_eq!(empty, KeyringRequest::Get { key: AzString::default() });
        assert_ne!(empty, KeyringRequest::Get { key: AzString::from("k") });
    }
}
