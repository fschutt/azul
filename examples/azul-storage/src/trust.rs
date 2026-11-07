//! What this device trusts, as two facts and nothing else.
//!
//! - The certificate authorities the Azlin apps pin: the iroh relays (n0's and the Azlin nodes')
//!   are served under Let's Encrypt, so a relay's TLS trusts Let's Encrypt's four roots and
//!   nothing else of the ~150 public ones. A forged relay certificate from any other authority is
//!   refused in the handshake. The token server's and the nodes' HTTPS keep the public roots for
//!   now: their edge certificates rotate among several authorities.
//! - Whether a drive is encrypted ([`EncState`], feature `encryption`): one value with one
//!   owner ([`crate::AutoEncrypted`]), read by every app through the policy that names what it
//!   decides. "Could not tell" is a state of its own and never reads as "plain": the bucket's
//!   listing is the least trusted word there is, so it can add to what this device knows (key
//!   files listed make a drive encrypted) and never take from it.

/// Let's Encrypt's roots, by name: X1 (RSA), X2 (ECDSA) and the 2025 generation's YE and YR
/// (n0's relays chain to Root YE, cross-signed by X2). PEM text from letsencrypt.org.
pub const AZLIN_RELAY_ROOTS: [(&str, &str); 4] = [
    ("ISRG Root X1", include_str!("../roots/ISRGRootX1.pem")),
    ("ISRG Root X2", include_str!("../roots/ISRGRootX2.pem")),
    ("ISRG Root YE", include_str!("../roots/ISRGRootYE.pem")),
    ("ISRG Root YR", include_str!("../roots/ISRGRootYR.pem")),
];

#[cfg(feature = "encryption")]
pub use enc_state::{EncState, IndexHome, Op, RestoreKind, SyncKind, Via, Why};

#[cfg(feature = "encryption")]
mod enc_state {
    use crate::crypto::KeyId;

    /// How this device came to take a drive for an encrypted one.
    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    pub enum Via {
        /// The drive key is in this device's keyring. Sticky: a bucket that hides its key files
        /// cannot turn this device back to plaintext.
        KeyInKeyring,
        /// The drives file records that this device found the drive encrypted. The key may be
        /// gone from the keyring since (a reinstall, a wiped keychain): the drive is locked then,
        /// never plain.
        Record,
        /// Only the bucket's listing of `.azlin/keys/` says so - the least trusted word. It makes
        /// a drive encrypted and locked for a device without a key; it never makes one plain.
        KeyFilesListed,
    }

    /// Why a drive that is not encrypted is not.
    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    pub enum Why {
        /// A bucket without a file of the drive: a new drive waiting for its encryption.
        New,
        /// A bucket with files and no key anywhere this device can see: a drive made before the
        /// encryption, or a hostile bucket that hides its key files. The two cannot be told
        /// apart from here (no authenticated source says "plain"), so it takes no file.
        HiddenOrAbsent,
    }

    /// What a call asks of a drive.
    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    pub enum Op {
        Read,
        Write,
        Delete,
    }

    /// Whether the drive is encrypted, as this device knows it. The value of "could not tell"
    /// is [`EncState::Undecided`]: it is never read as "plain", and the policies below say what
    /// each thing that depends on the answer does meanwhile.
    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    pub enum EncState {
        /// Nothing was asked yet, or the last ask failed (a keyring that could not be read, no
        /// answer from the bucket, a drive locked for want of its key).
        Undecided,
        /// The drive is encrypted. `key_id` is the id of the key it opened with.
        Encrypted { key_id: Option<KeyId>, via: Via },
        /// A setup of this device that stopped halfway (the key is kept, the device is not
        /// enrolled yet): the same setup call finishes it. Not a locked drive.
        SetupPending,
        /// The drive is not encrypted. It reads as its bucket lists and takes no write, ever.
        Unencrypted(Why),
    }

    /// What a sync of a drive does in a state.
    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    pub enum SyncKind {
        /// Through the drive's names (its index).
        Named,
        /// None may start: the drive takes no file (the drive's own refusal says why).
        Refused,
        /// The drive is not opened yet: open it, then ask again.
        NotYet,
    }

    /// What "Restore as of" does in a state.
    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    pub enum RestoreKind {
        /// From the drive's metadata repository: the files put back, as one new commit.
        Files,
        /// The drive could not be opened (nothing is decided): say so. NEVER the bucket's
        /// restore - it rolls every object of the bucket back, an encrypted drive's included.
        OpenFirst,
        /// The drive is not encrypted (yet): it has no history to restore from.
        EncryptFirst,
    }

    /// Where a drive's full-text index lives in a state.
    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    pub enum IndexHome {
        /// In the drive's own cache folder beside its drive index (the plain text of an
        /// encrypted drive's files never lies in the shared cache).
        DriveIndex,
        /// In the cache every drive shares.
        SharedCache,
        /// Nowhere yet: the drive is not known to be either.
        NotYet,
    }

    impl EncState {
        /// Whether a call decided it (anything but [`EncState::Undecided`]).
        #[must_use]
        pub const fn is_decided(self) -> bool {
            match self {
                EncState::Undecided => false,
                EncState::Encrypted { .. } | EncState::SetupPending | EncState::Unencrypted(_) => {
                    true
                }
            }
        }

        /// Whether the drive waits for its encryption to be set up by the app (a new drive, or
        /// a stopped setup): it takes no file until then.
        #[must_use]
        pub const fn needs_encryption(self) -> bool {
            match self {
                EncState::SetupPending | EncState::Unencrypted(Why::New) => true,
                EncState::Undecided
                | EncState::Encrypted { .. }
                | EncState::Unencrypted(Why::HiddenOrAbsent) => false,
            }
        }

        /// Whether "Encrypt this drive" is offered: the drive is known not to be encrypted (a new
        /// one, a stopped setup, a plain one made before the encryption). Not for an encrypted
        /// drive - its setup refuses with "encrypted already" - and not before the drive is
        /// decided: it may be an encrypted one whose keys this device cannot see (AZ-K9).
        #[must_use]
        pub const fn offers_encrypt(self) -> bool {
            match self {
                EncState::SetupPending | EncState::Unencrypted(_) => true,
                EncState::Undecided | EncState::Encrypted { .. } => false,
            }
        }

        /// Whether the drive goes through `op`. `plain_reads` is whether this app reads a drive
        /// that is not encrypted as its bucket lists (AzDrive does, to offer "Encrypt this
        /// drive"; the bridge and AzMail do not). Nothing goes through an undecided drive, and
        /// a drive that is not encrypted takes no write and no delete.
        #[must_use]
        pub const fn admits(self, op: Op, plain_reads: bool) -> bool {
            match self {
                EncState::Encrypted { .. } => true,
                EncState::Undecided => false,
                EncState::SetupPending | EncState::Unencrypted(_) => match op {
                    Op::Read => plain_reads,
                    Op::Write | Op::Delete => false,
                },
            }
        }

        /// Whether the drive has a drive index on this computer, and so its names come from it
        /// and its upkeep (recompression, repacking, totals) applies: an encrypted drive.
        #[must_use]
        pub const fn has_drive_index(self) -> bool {
            match self {
                EncState::Encrypted { .. } => true,
                EncState::Undecided | EncState::SetupPending | EncState::Unencrypted(_) => false,
            }
        }

        /// Whether a listing of the drive may be kept on this computer: its names would lie in
        /// the cache in the clear unless the bucket's listing is all there is to them.
        #[must_use]
        pub const fn may_cache_listing(self) -> bool {
            match self {
                EncState::SetupPending | EncState::Unencrypted(_) => true,
                EncState::Undecided | EncState::Encrypted { .. } => false,
            }
        }

        /// Whether the changes of a pass are index commits: an Azlin drive's are, unless it is
        /// known not to be encrypted (what the drive's state is asked to permit).
        #[must_use]
        pub const fn sends_index_commits(self) -> bool {
            match self {
                EncState::Undecided | EncState::Encrypted { .. } => true,
                EncState::SetupPending | EncState::Unencrypted(_) => false,
            }
        }

        /// What a sync of the drive does.
        #[must_use]
        pub const fn sync_kind(self) -> SyncKind {
            match self {
                EncState::Encrypted { .. } => SyncKind::Named,
                EncState::SetupPending | EncState::Unencrypted(_) => SyncKind::Refused,
                EncState::Undecided => SyncKind::NotYet,
            }
        }

        /// What "Restore as of" does. The bucket's restore is not one of the answers: it is
        /// never right for a drive that may be encrypted, and no authenticated source says a
        /// drive is not.
        #[must_use]
        pub const fn restore_kind(self) -> RestoreKind {
            match self {
                EncState::Encrypted { .. } => RestoreKind::Files,
                EncState::Undecided => RestoreKind::OpenFirst,
                EncState::SetupPending | EncState::Unencrypted(_) => RestoreKind::EncryptFirst,
            }
        }

        /// Where the drive's full-text index lives.
        #[must_use]
        pub const fn index_home(self) -> IndexHome {
            match self {
                EncState::Encrypted { .. } => IndexHome::DriveIndex,
                EncState::SetupPending | EncState::Unencrypted(_) => IndexHome::SharedCache,
                EncState::Undecided => IndexHome::NotYet,
            }
        }
    }
}
