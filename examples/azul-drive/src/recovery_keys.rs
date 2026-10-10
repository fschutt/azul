//! A drive's several recovery keys in AzDrive (feature `encryption`; the token server's
//! recovery keys and the lookup by one: D51, F12, §18.8).
//!
//! A recovery code has two keys at the token server: the DRIVE KEY (derived with the drive id;
//! it signs lockdowns since the first recovery sheet) and the FINDABLE KEY (the code's alone,
//! `azcloud_kit::RecoveryKey::derive_findable`). The setup registers both. The findable one lets
//! a computer that never had the drive find it from the kit alone - the kit leaves the drive id
//! out on purpose:
//!
//! - Options > Drives > "Recover a drive with its emergency kit...": the code typed, the
//!   findable key signs a lookup challenge, the token server names the drive(s), and the code
//!   signs the same recovery-key lockdown as everywhere else: 48 hours for the owner's other
//!   devices to stop it (D42; the pending family gets nothing meanwhile). The recovery waits in
//!   the list "Recoveries under way" until "Finish" - after the 48 hours its first refresh
//!   hands the drive over, and the drive joins the source list; "Unlock with the recovery code"
//!   then opens it.
//! - "...with trusted contacts": two shares give the code back (`recovery_contacts.rs`), the
//!   lookup the drive.
//!
//! The drive's keys as the token server lists them show under its methods in Options > Drives
//! ("Check" lists them). Every change needs one of the drive's codes typed - the key that signs
//! is whichever of the code's two keys the drive has: "Add another recovery code..." (a second
//! kit: a wrap of its own in the bucket, its findable key at the token server, its own recovery
//! sheet), "Make the kit find this drive..." (the findable key of a code from before) and
//! "Remove" (never the last key: the token server answers `last_recovery_key`). The passkey
//! stays "later".
//!
//! On stdout, for scripts: `AZDRIVE_KIT_LOOKUP <drives found>`, `AZDRIVE_KIT_LOCKDOWN <drive>`,
//! `AZDRIVE_RECOVERY_FINISHED <drive>`, `AZDRIVE_RECOVERY_PENDING <drive>`,
//! `AZDRIVE_RECOVERY_KEYS <drive> <count>`, `AZDRIVE_RECOVERY_CODE_ADDED <drive>`,
//! `AZDRIVE_RECOVERY_FINDABLE <drive>`, `AZDRIVE_RECOVERY_KEY_REMOVED <drive> <key>`,
//! `AZDRIVE_RECOVERY_KEY_KEPT <drive> <key>` (the token server kept it: the last key).

use azcloud_kit::{RecoveryKey, TokenError};
use azul_storage::crypto::keys::RecoveryCode;

use crate::recovery_health::{RecoveryState, ServerKey, FINDABLE_LABEL};

/// A code's findable key (no drive in it).
pub(crate) fn findable_key_of(code: &RecoveryCode) -> RecoveryKey {
    RecoveryKey::derive_findable(code.as_bytes())
}

/// The key a typed code signs a change of the drive's recovery keys with.
pub(crate) fn pick_signer(
    _state: Option<&RecoveryState>,
    _typed: &str,
    _drive_id: &str,
) -> Result<RecoveryKey, String> {
    let _ = (ServerKey::default(), FINDABLE_LABEL);
    Err(String::new())
}

/// A token server's refusal as the user reads it.
pub(crate) fn token_text(e: &TokenError) -> String {
    let _ = e;
    String::new()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn code() -> RecoveryCode {
        RecoveryCode::from_bytes([0x5A; 16])
    }

    #[test]
    fn a_typed_code_picks_the_key_the_drive_has_or_says_why_not() {
        let mut state = RecoveryState::new("d_1");
        let drive = crate::encryption::recovery_key_of(&code(), "d_1").public_base64();
        let findable = findable_key_of(&code()).public_base64();
        assert_ne!(drive, findable);
        state.recovery_key = Some(drive.clone());
        let typed = code().to_text().to_lowercase();
        let signer = pick_signer(Some(&state), &typed, "d_1").unwrap();
        assert_eq!(signer.public_base64(), drive);
        state.server_keys = vec![ServerKey {
            key_id: String::from("rk_2"),
            label: String::from(FINDABLE_LABEL),
            recovery_pubkey: findable.clone(),
            created_at: None,
            verified: true,
        }];
        state.keys_checked = Some(1);
        let signer = pick_signer(Some(&state), &typed, "d_1").unwrap();
        assert_eq!(
            signer.public_base64(),
            findable,
            "only the findable key is listed"
        );
        let other = RecoveryCode::from_bytes([0x11; 16]).to_text();
        assert!(pick_signer(Some(&state), &other, "d_1")
            .unwrap_err()
            .contains("not a recovery code of this drive"));
        assert!(pick_signer(Some(&state), "nonsense", "d_1")
            .unwrap_err()
            .contains("not a recovery code"));
        assert!(pick_signer(None, &typed, "d_1").is_err());
    }

    #[test]
    fn the_token_servers_recovery_refusals_read_as_the_table_says() {
        let refused = |code: &str| TokenError::Refused {
            status: 409,
            code: code.to_string(),
            message: String::from("raw"),
        };
        assert!(token_text(&refused("last_recovery_key")).contains("last recovery key"));
        assert!(
            token_text(&refused("frobnicated")).contains("raw"),
            "as it is otherwise"
        );
    }
}
