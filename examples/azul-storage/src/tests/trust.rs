//! The authorities the Azlin apps trust for the iroh relays: Let's Encrypt's four roots, which
//! n0's relays and the Azlin nodes' relays are served under, and nothing of the public set.

use crate::trust::AZLIN_RELAY_ROOTS;

#[test]
fn the_relays_are_trusted_under_let_s_encrypt_s_four_roots_only() {
    assert_eq!(AZLIN_RELAY_ROOTS.len(), 4);
    for (name, pem) in AZLIN_RELAY_ROOTS {
        assert!(name.starts_with("ISRG Root "), "{name}");
        assert_eq!(pem.matches("-----BEGIN CERTIFICATE-----").count(), 1, "{name}");
        assert!(pem.trim_end().ends_with("-----END CERTIFICATE-----"), "{name}");
    }
    let names: Vec<&str> = AZLIN_RELAY_ROOTS.iter().map(|(name, _)| *name).collect();
    assert_eq!(names, ["ISRG Root X1", "ISRG Root X2", "ISRG Root YE", "ISRG Root YR"]);
}

/// X-AR5b-4, X-AR5b-3, X-AR5b-2: the encrypted-or-not fact is ONE value, and what each thing that
/// depends on it does is a policy that names it. A table of every state against every policy: a
/// new state or a new policy has to be placed here, and "could not tell" is never "plain".
#[cfg(feature = "encryption")]
mod enc_state {
    use crate::{
        crypto::DriveKey,
        trust::{EncState, IndexHome, Op, RestoreKind, SyncKind, Via, Why},
    };

    fn states() -> Vec<EncState> {
        let key_id = Some(DriveKey::generate().unwrap().id());
        vec![
            EncState::Undecided,
            EncState::Encrypted {
                key_id,
                via: Via::KeyInKeyring,
            },
            EncState::Encrypted {
                key_id,
                via: Via::Record,
            },
            EncState::Encrypted {
                key_id: None,
                via: Via::KeyFilesListed,
            },
            EncState::SetupPending,
            EncState::Unencrypted(Why::New),
            EncState::Unencrypted(Why::HiddenOrAbsent),
        ]
    }

    #[test]
    fn x_ar5b_4_a_state_that_is_not_decided_is_no_state_a_drive_can_be_used_in() {
        for state in states() {
            let decided = !matches!(state, EncState::Undecided);
            assert_eq!(state.is_decided(), decided, "{state:?}");
            for plain_reads in [true, false] {
                for op in [Op::Read, Op::Write, Op::Delete] {
                    let admitted = state.admits(op, plain_reads);
                    match state {
                        EncState::Encrypted { .. } => assert!(admitted, "{state:?} {op:?}"),
                        EncState::Undecided => {
                            assert!(!admitted, "nothing goes through an undecided drive: {op:?}")
                        }
                        // X-DELTA-AZDRIVE-KEYS-19 (ARCH28 Q15): files and no key file, and
                        // neither a key nor a record here - not even a read goes through.
                        EncState::Unencrypted(Why::HiddenOrAbsent) => assert!(
                            !admitted,
                            "{state:?} {op:?} plain_reads={plain_reads}: a device with neither a \
                             key nor a record may not list the bucket"
                        ),
                        EncState::SetupPending | EncState::Unencrypted(Why::New) => assert_eq!(
                            admitted,
                            op == Op::Read && plain_reads,
                            "{state:?} {op:?} plain_reads={plain_reads}: a drive that is not \
                             encrypted takes no write and no delete, and reads only where the \
                             app reads one"
                        ),
                    }
                }
            }
        }
    }

    /// X-DELTA-AZDRIVE-KEYS-19: the app finishes a drive's setup by itself only for a setup of
    /// THIS device that stopped halfway (its key is in the keyring); a drive that merely lists
    /// empty, or holds files and shows no key file, waits for the user's word. And "Encrypt this
    /// drive" is warned about for the drive this device cannot tell from an encrypted one.
    #[test]
    fn x_delta_azdrive_keys_19_only_an_own_stopped_setup_resumes_and_only_a_drive_with_files_is_warned_about(
    ) {
        for state in states() {
            assert_eq!(
                state.resumes_setup(),
                matches!(state, EncState::SetupPending),
                "{state:?}"
            );
            assert_eq!(
                state.encrypt_needs_warning(),
                matches!(state, EncState::Unencrypted(Why::HiddenOrAbsent)),
                "{state:?}"
            );
            // A state that cannot be encrypted at all needs no warning, and one that resumes is
            // offered the encryption (the menu line is how a failed resume is retried).
            if state.encrypt_needs_warning() || state.resumes_setup() {
                assert!(state.offers_encrypt(), "{state:?}");
            }
        }
    }

    /// AZ-K9: "Encrypt this drive" was offered for every Azlin drive, also for one that is
    /// encrypted already (its setup then failed with "encrypted already") and for one not decided
    /// yet. It is offered for a drive known not to be encrypted, and for no other.
    #[test]
    fn az_k9_encrypt_this_drive_is_offered_only_for_a_drive_known_to_be_plain() {
        for state in states() {
            let plain = matches!(state, EncState::SetupPending | EncState::Unencrypted(_));
            assert_eq!(state.offers_encrypt(), plain, "{state:?}");
        }
    }

    #[test]
    fn x_ar5b_2_only_an_encrypted_drive_is_restored_from_its_files_and_never_by_its_bucket() {
        for state in states() {
            let kind = state.restore_kind();
            match state {
                EncState::Encrypted { .. } => assert_eq!(kind, RestoreKind::Files, "{state:?}"),
                EncState::Undecided => assert_eq!(
                    kind,
                    RestoreKind::OpenFirst,
                    "could not tell is an error for the user, not a plain drive"
                ),
                EncState::SetupPending | EncState::Unencrypted(_) => {
                    assert_eq!(kind, RestoreKind::EncryptFirst, "{state:?}")
                }
            }
        }
    }

    #[test]
    fn x_ar5b_3_every_reader_of_the_state_gets_the_answer_of_its_own_policy() {
        for state in states() {
            let encrypted = matches!(state, EncState::Encrypted { .. });
            let undecided = matches!(state, EncState::Undecided);
            assert_eq!(state.has_drive_index(), encrypted, "{state:?}");
            assert_eq!(
                state.sync_kind(),
                if encrypted {
                    SyncKind::Named
                } else if undecided {
                    SyncKind::NotYet
                } else {
                    SyncKind::Refused
                },
                "{state:?}"
            );
            assert_eq!(
                state.index_home(),
                if encrypted {
                    IndexHome::DriveIndex
                } else if undecided {
                    IndexHome::NotYet
                } else {
                    IndexHome::SharedCache
                },
                "{state:?}"
            );
            // A listing is kept in the clear only where the bucket's own listing is all there is
            // to the names: never for an encrypted drive, and never before it is known.
            assert_eq!(
                state.may_cache_listing(),
                !encrypted && !undecided,
                "{state:?}"
            );
            // The changes of a pass are index commits unless the drive is known not to be encrypted.
            assert_eq!(
                state.sends_index_commits(),
                encrypted || undecided,
                "{state:?}"
            );
            assert_eq!(
                state.needs_encryption(),
                matches!(
                    state,
                    EncState::SetupPending | EncState::Unencrypted(Why::New)
                ),
                "{state:?}"
            );
        }
    }
}
