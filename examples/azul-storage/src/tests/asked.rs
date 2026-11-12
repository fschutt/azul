//! The production pair of a key change - asked, then done - for the tests that run one: what the
//! apps do, without the account's lockdown between the two.

use std::sync::Arc;

use crate::{
    crypto::{device, keys::RecoveryKdf, DriveKey},
    encrypted::{EncryptedDrive, IndexProvider},
    keyring::{KeyringStore, MemoryKeyring},
    rotation::{self, ReencryptState, Rotated},
    Drive, DriveError,
};

/// [`rotation::ask`], then [`rotation::rotate`] with the permit it gave.
pub(crate) fn rotate(
    bucket: Arc<dyn Drive>,
    keyring: &dyn KeyringStore,
    drive: &str,
    provider: &dyn IndexProvider,
    kdf: RecoveryKdf,
) -> Result<Rotated, DriveError> {
    let rotation = rotation::ask(bucket.as_ref(), keyring, drive)?;
    rotation::rotate(rotation.permit(), bucket, keyring, drive, provider, kdf)
}

/// Whether the account's lockdown is to run before a key change: the answer of [`rotation::ask`],
/// which refuses where the key change could not finish.
pub(crate) fn lockdown_needed(
    bucket: &dyn Drive,
    keyring: &dyn KeyringStore,
    drive: &str,
) -> Result<bool, DriveError> {
    Ok(rotation::ask(bucket, keyring, drive)?.lockdown().is_some())
}

/// [`rotation::ask_reencrypt`], then [`rotation::reencrypt_pass`] with the permit it gave.
pub(crate) fn reencrypt_pass<D: Drive>(
    keyring: &dyn KeyringStore,
    drive_id: &str,
    drive: &EncryptedDrive<D>,
    state: &mut ReencryptState,
    save: &mut dyn FnMut(&ReencryptState) -> Result<(), DriveError>,
    stop: &dyn Fn() -> bool,
) -> Result<bool, DriveError> {
    let permit = rotation::ask_reencrypt(keyring, drive_id, drive)?;
    rotation::reencrypt_pass(&permit, drive, state, save, stop)
}

/// The same for a drive made from parts - a key and an index, no keyring of its own: the key goes into
/// a keyring made for it and the permit is asked there, as the apps' is on theirs.
pub(crate) fn reencrypt_pass_of_parts<D: Drive>(
    key: &DriveKey,
    drive: &EncryptedDrive<D>,
    state: &mut ReencryptState,
    save: &mut dyn FnMut(&ReencryptState) -> Result<(), DriveError>,
    stop: &dyn Fn() -> bool,
) -> Result<bool, DriveError> {
    let keyring = MemoryKeyring::new();
    device::store_drive_key(&keyring, "d_parts", key)?;
    reencrypt_pass(&keyring, "d_parts", drive, state, save, stop)
}
