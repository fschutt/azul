//! A file's stored versions (`drive_versions`) and the window's restore of one
//! (`drive_restore_version`) or of a file deleted since (`drive_restore_deleted`). An encrypted
//! drive keeps every version in its metadata repository; a plain drive's index names each
//! file's current version only, so these fail there.

#[cfg(feature = "encryption")]
use super::{need, str_arg, with_harness};
use super::{Answer, Args};
use crate::DriveState;

/// The drive of the device named (else the remote's first device), its key in `keyring`, and
/// the file's key in the drive (the remote's prefix in front).
#[cfg(feature = "encryption")]
struct Versioned {
    auto: std::sync::Arc<azul_storage::encrypted::AutoEncrypted>,
    keyring: std::sync::Arc<super::remote::MemKeyring>,
    key: String,
}

#[cfg(feature = "encryption")]
fn versioned(s: &mut DriveState, args: &Args) -> Result<Versioned, String> {
    let (drive_id, remote) = with_harness(|h| {
        let remote = h.remote_of(args)?;
        let drive_id = match str_arg(args, "device") {
            Some(_) => h.device(args)?.drive_id.clone(),
            None => h
                .devices
                .iter()
                .find(|d| d.remote == remote.name)
                .map(|d| d.drive_id.clone())
                .ok_or_else(|| format!("remote '{}' has no device to read it", remote.name))?,
        };
        Ok((drive_id, remote))
    })?;
    if !remote.encrypted {
        return Err(String::from(
            "a plain drive keeps no versions of its files: its index names each file's current \
             version only (drive_create with \"encrypted\": true)",
        ));
    }
    let index = s
        .slot_index(&drive_id)
        .ok_or_else(|| format!("AzDrive has no drive {drive_id}"))?;
    crate::open_slot(s, index).ok_or_else(|| format!("the drive of {drive_id} cannot be opened"))?;
    let auto = s.slots[index]
        .auto
        .clone()
        .ok_or_else(|| format!("{drive_id} is no encrypted drive"))?;
    Ok(Versioned {
        auto,
        keyring: remote.keyring.clone(),
        key: format!("{}{}", remote.prefix, need(args, "key")?),
    })
}

/// `drive_versions`: every stored version of `key`, oldest first: `versions`
/// `[{hash, at, device}]` (`hash` names the version's content object).
#[cfg(feature = "encryption")]
pub(super) fn versions(s: &mut DriveState, args: &Args) -> Answer {
    let v = versioned(s, args)?;
    let versions = crate::encryption::file_versions(&v.auto, v.keyring.as_ref(), &v.key)?;
    let listed: Vec<serde_json::Value> = versions
        .iter()
        .map(|version| {
            serde_json::json!({
                "hash": version.id.to_hex(), "at": version.at, "device": version.device,
            })
        })
        .collect();
    Ok(serde_json::json!({ "versions": listed }))
}

/// `drive_restore_version`: the version `version` of `key` (1 the oldest, as `drive_versions`
/// lists them; or its `hash`) back as the drive's, as the window's "Restore this version"
/// does; the devices bring it down with their next pass.
#[cfg(feature = "encryption")]
pub(super) fn restore_version(s: &mut DriveState, args: &Args) -> Answer {
    let v = versioned(s, args)?;
    let versions = crate::encryption::file_versions(&v.auto, v.keyring.as_ref(), &v.key)?;
    let chosen = match (args.get("version").and_then(serde_json::Value::as_u64), str_arg(args, "hash")) {
        (Some(n), _) => usize::try_from(n)
            .ok()
            .and_then(|n| n.checked_sub(1))
            .and_then(|i| versions.get(i)),
        (None, Some(hash)) => versions.iter().find(|version| version.id.to_hex() == hash),
        (None, None) => return Err(String::from("'version' (1 the oldest) or 'hash' is missing")),
    }
    .ok_or_else(|| format!("{} has {} versions: none such", v.key, versions.len()))?;
    let hash = chosen.id.to_hex();
    crate::encryption::restore_file_version(&v.auto, v.keyring.as_ref(), &v.key, chosen.id)?;
    Ok(serde_json::json!({ "restored": hash }))
}

/// `drive_restore_deleted`: a file deleted from the drive back as its last version was, as
/// the window's "Restore" of a deleted file does.
#[cfg(feature = "encryption")]
pub(super) fn restore_deleted(s: &mut DriveState, args: &Args) -> Answer {
    let v = versioned(s, args)?;
    let versions = crate::encryption::file_versions(&v.auto, v.keyring.as_ref(), &v.key)?;
    let last = versions
        .last()
        .ok_or_else(|| format!("the drive never had {}", v.key))?;
    let hash = last.id.to_hex();
    crate::encryption::restore_file_version(&v.auto, v.keyring.as_ref(), &v.key, last.id)?;
    Ok(serde_json::json!({ "restored": hash }))
}

#[cfg(not(feature = "encryption"))]
pub(super) fn versions(_s: &mut DriveState, _args: &Args) -> Answer {
    Err(String::from("this AzDrive was built without encrypted drives: no versions are kept"))
}

#[cfg(not(feature = "encryption"))]
pub(super) fn restore_version(s: &mut DriveState, args: &Args) -> Answer {
    versions(s, args)
}

#[cfg(not(feature = "encryption"))]
pub(super) fn restore_deleted(s: &mut DriveState, args: &Args) -> Answer {
    versions(s, args)
}
