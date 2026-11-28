#[allow(unused_imports)]
pub use super::*;
#[cfg(test)]
mod autotest_generated {
    use alloc::string::String;

    use super::*;

    // Payloads chosen to break naive C-string / byte-length assumptions in the
    // FFI layer: interior NUL, C0/C1 controls, CRLF.
    const NASTY_BYTES: &str = "pw\0with\u{1}nul\u{7f}\r\n\t";
    // Combining marks, RTL overrides, replacement char and the maximum scalar.
    const NASTY_UNICODE: &str =
        "🔑🗝 ключ 鍵 مفتاح e\u{301}\u{202e}terces\u{202c}\u{fffd}\u{10ffff}";

    /// Every `KeyringResult` variant, in declaration order.
    fn all_variants() -> [KeyringResult; 13] {
        [
            KeyringResult::Stored,
            KeyringResult::Retrieved(AzString::from("s3cr3t")),
            KeyringResult::Deleted,
            KeyringResult::NotFound,
            KeyringResult::Denied,
            KeyringResult::Unavailable,
            KeyringResult::Error,
            KeyringResult::Cancelled,
            KeyringResult::Failed,
            KeyringResult::LockedOut,
            KeyringResult::NotEnrolled,
            KeyringResult::Invalidated,
            KeyringResult::WouldPrompt,
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
            KeyringResult::Cancelled
            | KeyringResult::Failed
            | KeyringResult::LockedOut
            | KeyringResult::NotEnrolled
            | KeyringResult::Invalidated
            | KeyringResult::WouldPrompt => (false, false),
        }
    }

    // ---- getter: KeyringResult::secret ------------------------------------

    #[test]
    fn secret_returns_the_exact_payload_of_retrieved() {
        let r = KeyringResult::Retrieved(AzString::from("hunter2"));
        assert_eq!(
            r.secret().expect("Retrieved must yield a secret").as_str(),
            "hunter2"
        );
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
        assert_eq!(
            r.secret()
                .expect("empty payload is still a payload")
                .as_str(),
            ""
        );
        assert!(r.is_ok());

        let d = KeyringResult::Retrieved(AzString::default());
        assert_eq!(
            d.secret()
                .expect("default payload is still a payload")
                .as_str(),
            ""
        );
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
        for refusal in [
            KeyringResult::Cancelled,
            KeyringResult::Failed,
            KeyringResult::LockedOut,
            KeyringResult::NotEnrolled,
            KeyringResult::Invalidated,
            KeyringResult::WouldPrompt,
        ] {
            assert!(!refusal.is_ok(), "{refusal:?} is not a success");
        }
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
            assert_eq!(
                c.secret().expect("clone keeps the payload").as_str(),
                payload
            );
            // A shallow clone would free the original's buffer right here.
            drop(c);
        }

        assert_eq!(
            original
                .secret()
                .expect("original survives 1000 clone/drop cycles")
                .as_str(),
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
        assert_eq!(
            clone.secret().expect("clone owns its payload").as_str(),
            payload
        );
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
        assert_ne!(
            KeyringResult::Retrieved(AzString::from("")),
            KeyringResult::Stored
        );
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
            assert_eq!(
                back,
                Some(r),
                "encode/decode through OptionKeyringResult lost data"
            );
        }
    }

    #[test]
    fn option_wrapper_accessors_agree_with_each_other() {
        let payload = "round-trip-me";
        let some = OptionKeyringResult::Some(KeyringResult::Retrieved(AzString::from(payload)));
        assert!(some.is_some());
        assert!(!some.is_none());
        assert_eq!(
            some.as_option()
                .and_then(KeyringResult::secret)
                .map(AzString::as_str),
            Some(payload)
        );
        assert_eq!(some.as_ref(), some.as_option());

        let none = OptionKeyringResult::None;
        assert!(none.is_none());
        assert!(!none.is_some());
        assert!(none.as_option().is_none());
    }

    #[test]
    fn option_wrapper_to_option_clones_and_leaves_the_wrapper_intact() {
        // `to_option(&self)` takes a reference and clones — calling it twice
        // must not double-free or hollow out the wrapper.
        let payload = NASTY_BYTES;
        let wrapped = OptionKeyringResult::Some(KeyringResult::Retrieved(AzString::from(payload)));

        let first = wrapped.to_option().expect("Some stays Some");
        let second = wrapped
            .to_option()
            .expect("to_option must not consume the wrapper");
        assert_eq!(first, second);
        assert_eq!(first.secret().expect("payload survives").as_str(), payload);
        drop(first);
        assert_eq!(
            second
                .secret()
                .expect("payload survives its sibling")
                .as_str(),
            payload
        );
        drop(second);
        assert_eq!(
            wrapped
                .as_option()
                .and_then(KeyringResult::secret)
                .map(AzString::as_str),
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
            o.as_option()
                .and_then(KeyringResult::secret)
                .map(AzString::as_str),
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

    fn store(key: &str, secret: &str, protection: KeyringProtection) -> KeyringRequest {
        KeyringRequest::Store {
            key: AzString::from(key),
            secret: AzString::from(secret),
            protection,
            prompt: KeyringPrompt::default(),
        }
    }

    #[test]
    fn request_equality_is_sensitive_to_key_secret_protection_and_prompt() {
        let store = store("k", "s", KeyringProtection::DeviceUnlocked);
        assert_eq!(store, store.clone());

        assert_ne!(
            store,
            self::store("k", "s", KeyringProtection::Biometric),
            "the protection level changes the stored access control: it must not be ignored"
        );
        assert_ne!(
            self::store("k", "s", KeyringProtection::UserPresence),
            self::store("k", "s", KeyringProtection::Biometric),
            "a passcode fallback and no fallback are different items"
        );
        assert_ne!(store, self::store("k", "other", KeyringProtection::DeviceUnlocked));
        assert_ne!(
            KeyringRequest::Unlock {
                key: AzString::from("k"),
                prompt: KeyringPrompt::create(AzString::from("Unlock your vault")),
            },
            KeyringRequest::Unlock {
                key: AzString::from("k"),
                prompt: KeyringPrompt::default(),
            },
            "the sheet's text is part of the op"
        );
        assert_ne!(
            KeyringRequest::Get {
                key: AzString::from("k")
            },
            KeyringRequest::Delete {
                key: AzString::from("k")
            },
            "a read and a delete of the same key are different operations"
        );
        assert_ne!(
            KeyringRequest::Get {
                key: AzString::from("k")
            },
            KeyringRequest::Unlock {
                key: AzString::from("k"),
                prompt: KeyringPrompt::default(),
            },
            "a get never shows the sheet, an unlock may: they are different operations"
        );
        assert_ne!(
            KeyringRequest::Get {
                key: AzString::from("a")
            },
            KeyringRequest::Get {
                key: AzString::from("b")
            }
        );
    }

    #[test]
    fn request_payloads_survive_clone_and_drop() {
        let key = "🔑".repeat(10_000);
        let req = KeyringRequest::Store {
            key: AzString::from(key.clone()),
            secret: AzString::from(NASTY_BYTES),
            protection: KeyringProtection::Biometric,
            prompt: KeyringPrompt::create(AzString::from(NASTY_UNICODE))
                .with_cancel_label(AzString::from("Use password")),
        };

        let clone = req.clone();
        drop(req);

        match &clone {
            KeyringRequest::Store {
                key: k,
                secret: s,
                protection,
                prompt,
            } => {
                assert_eq!(k.as_str(), key.as_str());
                assert_eq!(s.as_str(), NASTY_BYTES);
                assert_eq!(*protection, KeyringProtection::Biometric);
                assert_eq!(prompt.reason.as_str(), NASTY_UNICODE);
                assert_eq!(prompt.cancel_label.as_str(), "Use password");
            }
            other => unreachable!("constructed Store, got {other:?}"),
        }
    }

    #[test]
    fn only_an_unlock_and_a_protected_store_may_show_the_sheet() {
        let key = || AzString::from("k");
        assert!(!KeyringRequest::Get { key: key() }.may_show_sheet());
        assert!(!KeyringRequest::Delete { key: key() }.may_show_sheet());
        assert!(!store("k", "s", KeyringProtection::DeviceUnlocked).may_show_sheet());
        assert!(store("k", "s", KeyringProtection::UserPresence).may_show_sheet());
        assert!(store("k", "s", KeyringProtection::Biometric).may_show_sheet());
        assert!(KeyringRequest::Unlock {
            key: key(),
            prompt: KeyringPrompt::default()
        }
        .may_show_sheet());
    }

    #[test]
    fn every_request_names_its_key() {
        for req in [
            store("a", "s", KeyringProtection::UserPresence),
            KeyringRequest::Get { key: AzString::from("a") },
            KeyringRequest::Unlock {
                key: AzString::from("a"),
                prompt: KeyringPrompt::default(),
            },
            KeyringRequest::Delete { key: AzString::from("a") },
        ] {
            assert_eq!(req.key().as_str(), "a");
        }
    }

    // ---- the protection level, the prompt, the probes ---------------------

    #[test]
    fn the_default_protection_is_the_one_that_never_shows_a_sheet() {
        assert_eq!(KeyringProtection::default(), KeyringProtection::DeviceUnlocked);
        assert!(!KeyringProtection::DeviceUnlocked.needs_user());
        assert!(KeyringProtection::UserPresence.needs_user());
        assert!(KeyringProtection::Biometric.needs_user());
    }

    #[test]
    fn a_protection_level_round_trips_through_the_number_the_backends_keep() {
        for level in [
            KeyringProtection::DeviceUnlocked,
            KeyringProtection::UserPresence,
            KeyringProtection::Biometric,
        ] {
            assert_eq!(KeyringProtection::from_code(level.code()), Some(level));
        }
        assert_eq!(KeyringProtection::DeviceUnlocked.code(), 0);
        assert_eq!(KeyringProtection::UserPresence.code(), 1);
        assert_eq!(KeyringProtection::Biometric.code(), 2);
        assert_eq!(KeyringProtection::from_code(3), None);
        assert_eq!(KeyringProtection::from_code(255), None);
    }

    #[test]
    fn the_levels_order_by_strength() {
        assert!(KeyringProtection::DeviceUnlocked < KeyringProtection::UserPresence);
        assert!(KeyringProtection::UserPresence < KeyringProtection::Biometric);
    }

    #[test]
    fn a_prompt_keeps_its_reason_and_cancel_label() {
        let plain = KeyringPrompt::create(AzString::from("Unlock your vault"));
        assert_eq!(plain.reason.as_str(), "Unlock your vault");
        assert_eq!(plain.cancel_label.as_str(), "", "no label: the platform's own Cancel");
        let labelled = plain.clone().with_cancel_label(AzString::from("Use password"));
        assert_eq!(labelled.cancel_label.as_str(), "Use password");
        assert_eq!(labelled.reason, plain.reason);
        assert_eq!(KeyringPrompt::default().reason.as_str(), "");
    }

    #[test]
    fn the_refusals_of_the_user_are_told_from_the_ones_of_the_system() {
        for r in all_variants() {
            let user = matches!(
                r,
                KeyringResult::Cancelled | KeyringResult::Failed | KeyringResult::LockedOut
            );
            assert_eq!(r.is_user_refusal(), user, "is_user_refusal wrong for {r:?}");
            assert_eq!(
                r.needs_user(),
                matches!(r, KeyringResult::WouldPrompt),
                "needs_user wrong for {r:?}"
            );
        }
        assert!(!KeyringResult::Denied.is_user_refusal(), "Denied is the system's");
        assert!(!KeyringResult::Denied.needs_user());
    }

    #[test]
    fn the_first_seven_results_keep_their_places_and_the_new_ones_come_after() {
        // The C bindings number the variants in declaration order; an app built before the
        // protection levels must read `Error` as `Error`.
        let order = all_variants();
        assert_eq!(order[0], KeyringResult::Stored);
        assert_eq!(order[2], KeyringResult::Deleted);
        assert_eq!(order[3], KeyringResult::NotFound);
        assert_eq!(order[4], KeyringResult::Denied);
        assert_eq!(order[5], KeyringResult::Unavailable);
        assert_eq!(order[6], KeyringResult::Error);
        assert_eq!(order[7], KeyringResult::Cancelled);
        assert_eq!(order[12], KeyringResult::WouldPrompt);
    }

    #[test]
    fn the_probe_answers_default_to_the_cautious_one() {
        assert_eq!(KeyringNeed::default(), KeyringNeed::NotFound);
        assert_eq!(KeyringSupport::default(), KeyringSupport::Unavailable);
    }

    #[test]
    fn request_accepts_empty_keys_without_panicking() {
        // An empty key is nonsense to a backend, but the POD type must still
        // construct, compare and drop cleanly rather than blow up in the callback.
        let empty = KeyringRequest::Get {
            key: AzString::from(""),
        };
        assert_eq!(
            empty,
            KeyringRequest::Get {
                key: AzString::default()
            }
        );
        assert_ne!(
            empty,
            KeyringRequest::Get {
                key: AzString::from("k")
            }
        );
    }
}
