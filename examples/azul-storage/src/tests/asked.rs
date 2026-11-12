//! The production pair of a key change - asked, then done - for the tests that run one: what the
//! apps do, without the account's lockdown between the two.

use std::sync::Arc;

use crate::{
    crypto::keys::RecoveryKdf,
    encrypted::IndexProvider,
    keyring::KeyringStore,
    rotation::{self, Rotated},
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
