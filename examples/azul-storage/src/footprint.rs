//! What one drive leaves on a device, named in one place (ARCH28 RF53 / AR5b-4).
//!
//! A drive leaves the entries of [`KeyEntry`] in the keyring and, in each app, files and folders:
//! its entry in the drives file, the sync state, the copy of its index and the search index (the
//! PLAIN TEXT of its files), local copies of its objects, the period vault, open calls, the states
//! of its encryption jobs. Every flow listed "its" stores by hand - `forget_keys` three, `key_entries`
//! six, the real number is at least eight - so a drive taken off the list left the rest behind,
//! readable by every program of the user. This module has:
//!
//! - [`KeyEntry`]: the kinds of keyring entry a drive has, with the ONLY code that formats an
//!   entry's name (`azul-storage/<kind>/<drive>`; `footprint_gate` reads for the prefix anywhere
//!   else). A name is never renamed: users' keys are in them (a test pins them).
//! - [`Protection`] and [`Allow`]: what a keyring keeps its entries safe with, and whether a plain
//!   file may hold key material at all (it may not, unless the user said so).
//! - [`RemovalProof`]: the evidence a drive's keys need before they leave the device with it.
//! - [`Footprint`], [`forget_drive`] and [`ForgetReport`]: one store's part of a removal, and the
//!   removal that tries every store and takes the drives-file entry LAST - only when every other
//!   store is clean, so a delete that was refused leaves the drive on the list for a retry and
//!   not entries nobody can find by drive id.
//! - [`KeyringFootprint`], [`Paths`] and [`DrivesListing`]: the stores that need no knowledge of
//!   an app.
//!
//! Nothing here logs a secret or puts one in `Debug` output; the sentences name an entry or a
//! path, never its content.

use std::{
    fmt,
    path::{Path, PathBuf},
};

use crate::keyring::KeyringStore;

/// The folder name every entry of this crate's keyring starts with.
const PREFIX: &str = "azul-storage";

/// The kinds of keyring entry one drive has on a device. [`KeyEntry::name`] is the entry's name in
/// the keyring; NEVER rename one: the keys of users' drives are in them.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum KeyEntry {
    /// The drive's session: an S3 drive's credentials, an Azlin drive's session (its drive token
    /// and the 12-hour credentials). A credential, not a key.
    Session,
    /// The drive token of a device whose account keeps its session in the state folder's secrets
    /// file (the command line, the bridge), under the OS keyring's name.
    AzlinToken,
    /// The drive's key.
    DriveKey,
    /// This device's member key for the drive, with the pin of the drive key it holds.
    MemberKey,
    /// A drive key made before its drive existed (a checkout's), kept until the device is
    /// enrolled.
    PlannedKey,
    /// The drive key a rotation replaces, kept until the rotation is done.
    RotationPrevious,
    /// The drive key a rotation brings, kept until the rotation is done.
    RotationNext,
    /// The one-time key of a recovery request, kept while trusted contacts answer.
    RequestKey,
    /// The random key that seals the drive's period vault on this computer (the issue keys and
    /// period tokens of the months a customer paid for, AZ-K13). A credential: it opens a file of
    /// this device that is removed with the drive.
    VaultKey,
}

impl KeyEntry {
    /// Every kind, the entries that grant access to the drive's server side first, then the keys
    /// (a removal that stops in the middle leaves the keys, which can be found by the drive's id
    /// and tried again).
    pub const ALL: [KeyEntry; 9] = [
        KeyEntry::Session,
        KeyEntry::AzlinToken,
        KeyEntry::DriveKey,
        KeyEntry::RotationPrevious,
        KeyEntry::RotationNext,
        KeyEntry::PlannedKey,
        KeyEntry::MemberKey,
        KeyEntry::RequestKey,
        // Last: a removal that stops before it leaves the vault readable.
        KeyEntry::VaultKey,
    ];

    /// The entry's name in the keyring for the drive `drive`.
    #[must_use]
    pub fn name(self, drive: &str) -> String {
        format!("{PREFIX}/{}/{drive}", self.kind())
    }

    /// The word of the kind in the name.
    const fn kind(self) -> &'static str {
        match self {
            KeyEntry::Session => "s3",
            KeyEntry::AzlinToken => "azlin",
            KeyEntry::DriveKey => "drive-key",
            KeyEntry::MemberKey => "member-key",
            KeyEntry::PlannedKey => "drive-key-planned",
            KeyEntry::RotationPrevious => "drive-key-previous",
            KeyEntry::RotationNext => "drive-key-next",
            KeyEntry::RequestKey => "recovery-request",
            KeyEntry::VaultKey => "vault-key",
        }
    }

    /// Whether the entry holds key material - a key that opens the drive's files, or a secret
    /// that unwraps one - and not a credential that only a session needs. Key material leaves
    /// the device only with a [`RemovalProof`], and stays out of a plain file unless the user
    /// allowed it ([`Allow::PlainSecrets`]).
    #[must_use]
    pub const fn is_key_material(self) -> bool {
        !matches!(
            self,
            KeyEntry::Session | KeyEntry::AzlinToken | KeyEntry::VaultKey
        )
    }

    /// Whether the entry leaves the device only with a [`RemovalProof`]: key material, and the key
    /// of the period vault. The vault's FILES go with a proof (the months paid for the drive may
    /// be only there), so the key that opens them goes with it and never alone: a vault that
    /// stays without its key is ciphertext nobody can read (X-DELTA-AZDRIVE-KEYS-8).
    #[must_use]
    pub const fn goes_with_proof(self) -> bool {
        self.is_key_material() || matches!(self, KeyEntry::VaultKey)
    }

    /// The entry of a drive of `drive` whose name is `name`, if it is one.
    #[must_use]
    pub fn of_name(name: &str, drive: &str) -> Option<KeyEntry> {
        KeyEntry::ALL
            .into_iter()
            .find(|entry| entry.name(drive) == name)
    }
}

/// The keyring entry of a contact key - the key this device made to hold one owner's share of a
/// recovery code - by its public key's id. A contact key belongs to the owner it holds a share
/// for, not to a drive of this device: a removed drive leaves it alone.
#[must_use]
pub fn contact_key_entry(id: &str) -> String {
    format!("{PREFIX}/contact-key/{id}")
}

/// Whether a plain file may hold key material.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Allow {
    /// No: a store that is a plain file refuses to keep a drive's keys.
    #[default]
    Nothing,
    /// The user said so (a flag at the command, a setting of the bridge): this machine has no
    /// keyring, and the drive's keys may be kept in a file only this user can read.
    PlainSecrets,
}

/// What a keyring keeps its entries safe with: who else on this device can read them.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Protection {
    /// The operating system's keyring (Keychain, Credential Manager, the Secret Service): the
    /// system decides which program reads an entry, and may ask the user.
    Os,
    /// A file in a folder of this user: every program of this user, and any copy of the folder,
    /// reads it. Key material goes in only when the user allowed it.
    PlainFile {
        /// Whether the user did.
        allow: Allow,
    },
    /// The memory of this process: nothing is kept for the next run (a test's, or a stand-in).
    Memory,
}

impl Protection {
    /// Whether a store with this protection takes a drive's key material.
    #[must_use]
    pub fn keeps_key_material(self) -> bool {
        match self {
            Protection::Os | Protection::Memory => true,
            Protection::PlainFile { allow } => allow == Allow::PlainSecrets,
        }
    }

    /// The sentence a refusal to keep key material says.
    #[must_use]
    pub fn refusal(self) -> &'static str {
        match self {
            Protection::PlainFile { .. } => {
                "this device keeps its secrets in a plain file, which a drive's key is not kept \
                 in unless you allow it (--allow-plain-secrets at the command line, the bridge's \
                 \"plain_secrets\" setting): any program of this user could read it"
            }
            Protection::Os | Protection::Memory => "this keyring takes key material",
        }
    }
}

/// Evidence that a drive's keys may leave this device with the drive: its recovery code was typed
/// back right, so the keys here are not the only way into the drive. Without one the keys stay and
/// that is said. One constructor, no `Clone`, no `Default`, no `Deserialize`: a proof is made
/// from what the drive's recovery state knows (AzDrive's `RecoveryState::removal_proof`), and
/// `footprint_gate` counts the places that call it.
#[derive(Debug)]
#[must_use]
pub struct RemovalProof {
    drive: String,
}

impl RemovalProof {
    /// The proof for `drive`. Only the recovery state of a drive whose code was checked calls it.
    pub fn from_checked_code(drive: &str) -> RemovalProof {
        RemovalProof {
            drive: drive.to_string(),
        }
    }

    /// The drive it is for.
    #[must_use]
    pub fn drive(&self) -> &str {
        &self.drive
    }
}

/// A store could not do its part of a removal. The sentence names the entry or the path, never
/// its content.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FootprintError(String);

impl FootprintError {
    /// An error with a sentence.
    pub fn new(text: impl Into<String>) -> FootprintError {
        FootprintError(text.into())
    }
}

impl fmt::Display for FootprintError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for FootprintError {}

/// Why something of a drive is still on the device after a removal.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KeptBecause {
    /// Key material, and no [`RemovalProof`]: the keys here may be the only way into the drive.
    NoProof,
}

/// A trace of the drive left on purpose.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Kept {
    /// The entry or path.
    pub what: String,
    pub because: KeptBecause,
}

/// What one store did: what is gone, and what stays.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct Forgotten {
    /// The entries and paths that are gone now (whether or not they were there).
    pub removed: Vec<String>,
    /// What stays, on purpose.
    pub kept: Vec<Kept>,
}

/// One store of what a drive leaves on a device.
pub trait Footprint {
    /// The store's name, for a report.
    fn store(&self) -> &'static str;

    /// Whether this store is the list the drive is found by (the drives file): it goes LAST, and
    /// only when every other store is clean.
    fn is_the_listing(&self) -> bool {
        false
    }

    /// What of the drive `drive` is still in the store, by name; empty: nothing.
    ///
    /// # Errors
    ///
    /// The store cannot be read.
    fn residue(&self, drive: &str) -> Result<Vec<String>, FootprintError>;

    /// Takes everything of `drive` out of the store (what is not there is no error). Key material
    /// is taken only with a proof ([`RemovalProof`]) of this very drive.
    ///
    /// # Errors
    ///
    /// What could not be removed, all of it named: the store has tried everything.
    fn forget(
        &self,
        drive: &str,
        proof: Option<&RemovalProof>,
    ) -> Result<Forgotten, FootprintError>;
}

/// What a store did in a removal.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StoreOutcome {
    Done(Forgotten),
    Failed(FootprintError),
    /// The listing, left alone because a store before it failed.
    Skipped,
}

/// One line of a [`ForgetReport`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StoreReport {
    pub store: &'static str,
    pub outcome: StoreOutcome,
}

/// What a removal did, store by store, in the order they ran.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ForgetReport {
    pub stores: Vec<StoreReport>,
}

impl ForgetReport {
    /// Whether every store did its part, the listing included.
    #[must_use]
    pub fn complete(&self) -> bool {
        self.stores
            .iter()
            .all(|s| matches!(s.outcome, StoreOutcome::Done(_)))
    }

    /// The stores that failed, and why.
    #[must_use]
    pub fn failures(&self) -> Vec<(&'static str, &FootprintError)> {
        self.stores
            .iter()
            .filter_map(|s| match &s.outcome {
                StoreOutcome::Failed(e) => Some((s.store, e)),
                _ => None,
            })
            .collect()
    }

    /// Whether the listing was taken off (the drive is gone from the list).
    #[must_use]
    pub fn listing_removed(&self, listing: &'static str) -> bool {
        self.stores
            .iter()
            .any(|s| s.store == listing && matches!(s.outcome, StoreOutcome::Done(_)))
    }

    /// What stays on purpose.
    #[must_use]
    pub fn kept(&self) -> Vec<&Kept> {
        self.stores
            .iter()
            .filter_map(|s| match &s.outcome {
                StoreOutcome::Done(done) => Some(done.kept.iter()),
                _ => None,
            })
            .flatten()
            .collect()
    }
}

/// Takes the drive `drive` off this device: every store in `stores` is tried, whatever the
/// others did, and then the listing - only when no store failed, so that a delete the system
/// refused leaves the drive on the list, with its entries findable by its id, for another try.
/// A proof for another drive is no proof.
pub fn forget_drive(
    stores: &[&dyn Footprint],
    drive: &str,
    proof: Option<&RemovalProof>,
) -> ForgetReport {
    let proof = proof.filter(|p| p.drive() == drive);
    let mut report = ForgetReport::default();
    let mut failed = false;
    for store in stores.iter().filter(|s| !s.is_the_listing()) {
        let outcome = match store.forget(drive, proof) {
            Ok(done) => StoreOutcome::Done(done),
            Err(e) => {
                failed = true;
                StoreOutcome::Failed(e)
            }
        };
        report.stores.push(StoreReport {
            store: store.store(),
            outcome,
        });
    }
    for store in stores.iter().filter(|s| s.is_the_listing()) {
        let outcome = if failed {
            StoreOutcome::Skipped
        } else {
            match store.forget(drive, proof) {
                Ok(done) => StoreOutcome::Done(done),
                Err(e) => StoreOutcome::Failed(e),
            }
        };
        report.stores.push(StoreReport {
            store: store.store(),
            outcome,
        });
    }
    report
}

/// What a drive has in the keyring: the entries of [`KeyEntry::ALL`].
pub struct KeyringFootprint<'k> {
    keyring: &'k dyn KeyringStore,
}

impl<'k> KeyringFootprint<'k> {
    /// The entries of `keyring`.
    #[must_use]
    pub fn new(keyring: &'k dyn KeyringStore) -> KeyringFootprint<'k> {
        KeyringFootprint { keyring }
    }
}

impl Footprint for KeyringFootprint<'_> {
    fn store(&self) -> &'static str {
        "keyring"
    }

    fn residue(&self, drive: &str) -> Result<Vec<String>, FootprintError> {
        let mut left = Vec::new();
        for entry in KeyEntry::ALL {
            let name = entry.name(drive);
            match self.keyring.get(&name) {
                Ok(None) => {}
                Ok(Some(_)) => left.push(name),
                Err(e) => return Err(FootprintError::new(format!("{name}: {e}"))),
            }
        }
        Ok(left)
    }

    fn forget(
        &self,
        drive: &str,
        proof: Option<&RemovalProof>,
    ) -> Result<Forgotten, FootprintError> {
        let mut done = Forgotten::default();
        let mut failures = Vec::new();
        for entry in KeyEntry::ALL {
            let name = entry.name(drive);
            if entry.goes_with_proof() && proof.is_none() {
                // Key material stays without the evidence - when there is some. An entry that is
                // not there is nothing to keep and is not said to be (a drive of a bucket has a
                // session and no key); one that cannot be looked at is neither kept nor gone: the
                // store fails and the drive stays listed.
                match self.keyring.get(&name) {
                    Ok(None) => {}
                    Ok(Some(_)) => done.kept.push(Kept {
                        what: name,
                        because: KeptBecause::NoProof,
                    }),
                    Err(e) => failures.push(format!("{name}: {e}")),
                }
                continue;
            }
            match self.keyring.delete(&name) {
                Ok(()) => done.removed.push(name),
                Err(e) => failures.push(format!("{name}: {e}")),
            }
        }
        if failures.is_empty() {
            Ok(done)
        } else {
            Err(FootprintError::new(failures.join("; ")))
        }
    }
}

/// The places a drive has in a store, from its id.
type PlacesOf = Box<dyn Fn(&str) -> Vec<PathBuf> + Send + Sync>;

/// Files and folders of a drive, named by the code that writes them: `places_of(drive)` gives
/// the paths the drive has in this store (a file, or a folder with everything in it).
pub struct Paths {
    store: &'static str,
    places_of: PlacesOf,
    /// Whether what is here may be the only way back to the drive or to what was paid for it,
    /// so that it goes only with a [`RemovalProof`].
    needs_proof: bool,
}

impl Paths {
    /// A store `store` whose places for a drive are `places_of(drive)`.
    #[must_use]
    pub fn new(
        store: &'static str,
        places_of: impl Fn(&str) -> Vec<PathBuf> + Send + Sync + 'static,
    ) -> Paths {
        Paths {
            store,
            places_of: Box::new(places_of),
            needs_proof: false,
        }
    }

    /// This store goes only with a [`RemovalProof`], like the keys: a vault of paid months, the
    /// calls that guard them. Without one everything in it stays and the report says so.
    #[must_use]
    pub fn only_with_proof(mut self) -> Paths {
        self.needs_proof = true;
        self
    }
}

/// Removes `path` (a file, or a folder and everything in it); one that is not there is no error.
fn remove_place(path: &Path) -> Result<(), String> {
    let meta = match std::fs::symlink_metadata(path) {
        Ok(meta) => meta,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(e) => return Err(format!("{}: {e}", path.display())),
    };
    let removed = if meta.is_dir() {
        std::fs::remove_dir_all(path)
    } else {
        std::fs::remove_file(path)
    };
    match removed {
        Err(e) if e.kind() != std::io::ErrorKind::NotFound => {
            Err(format!("{}: {e}", path.display()))
        }
        _ => Ok(()),
    }
}

impl Footprint for Paths {
    fn store(&self) -> &'static str {
        self.store
    }

    fn residue(&self, drive: &str) -> Result<Vec<String>, FootprintError> {
        Ok((self.places_of)(drive)
            .into_iter()
            .filter(|p| std::fs::symlink_metadata(p).is_ok())
            .map(|p| p.display().to_string())
            .collect())
    }

    fn forget(
        &self,
        drive: &str,
        proof: Option<&RemovalProof>,
    ) -> Result<Forgotten, FootprintError> {
        let mut done = Forgotten::default();
        let mut failures = Vec::new();
        for place in (self.places_of)(drive) {
            if self.needs_proof && proof.is_none() {
                if std::fs::symlink_metadata(&place).is_ok() {
                    done.kept.push(Kept {
                        what: place.display().to_string(),
                        because: KeptBecause::NoProof,
                    });
                }
                continue;
            }
            match remove_place(&place) {
                Ok(()) => done.removed.push(place.display().to_string()),
                Err(e) => failures.push(e),
            }
        }
        if failures.is_empty() {
            Ok(done)
        } else {
            Err(FootprintError::new(failures.join("; ")))
        }
    }
}

/// The drives file as a store: the drive's entry in it. The listing of a removal - last.
pub struct DrivesListing {
    file: PathBuf,
}

impl DrivesListing {
    /// The drives file at `file`.
    #[must_use]
    pub fn new(file: impl Into<PathBuf>) -> DrivesListing {
        DrivesListing { file: file.into() }
    }

    /// The name of this store in a report.
    pub const STORE: &'static str = "drives file";
}

impl Footprint for DrivesListing {
    fn store(&self) -> &'static str {
        Self::STORE
    }

    fn is_the_listing(&self) -> bool {
        true
    }

    fn residue(&self, drive: &str) -> Result<Vec<String>, FootprintError> {
        let file = crate::config::DrivesFile::load(&self.file)
            .map_err(|e| FootprintError::new(e.to_string()))?;
        Ok(file
            .get(drive)
            .map(|_| drive.to_string())
            .into_iter()
            .collect())
    }

    fn forget(
        &self,
        drive: &str,
        _proof: Option<&RemovalProof>,
    ) -> Result<Forgotten, FootprintError> {
        let had =
            crate::config::DrivesFile::update(&self.file, |file| file.remove(drive).is_some())
                .map_err(|e| FootprintError::new(e.to_string()))?;
        Ok(Forgotten {
            removed: if had {
                vec![drive.to_string()]
            } else {
                Vec::new()
            },
            kept: Vec::new(),
        })
    }
}
