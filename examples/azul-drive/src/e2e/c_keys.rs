//! The keys of an encrypted remote's devices: who holds the drive key, and the ways a device
//! gets it - the recovery code, another device's approval - or loses it to a key rotation.
//!
//! A device opened the usual way shares the remote's keyring with the remote's owner: it is
//! "already joined", one user's computers. A device opened with `joined: false` has a keyring
//! of its own that starts empty, as a new computer, a reinstalled one or a forgotten
//! passphrase leaves it; `key_from: "recovery"` is such a device that unlocked with the
//! recovery code at once. Everything below runs AzDrive's real key flows of azul-storage
//! (`device::recover`, `seal_invite` / `adopt_invite`, `rotation::rotate`) through each
//! device's own fault layer, so a partition or a refusal reaches them as it would reach the
//! app.
//!
//! The remote's recovery code is the one its creation made (the recovery sheet), the new one
//! after a rotation. A scenario writes it as `{recovery:<remote>}`, the one before the last
//! rotation as `{recovery_old:<remote>}`.

use std::{
    collections::BTreeMap,
    sync::{Arc, Mutex, MutexGuard, PoisonError},
};

use azul_storage::{keyring::KeyringStore, Drive};

use super::{
    remote::{FaultDrive, MemKeyring, Remote},
    need, str_arg, with_harness, Answer, Args, Harness,
};
use crate::DriveState;

/// What the key commands keep between steps (emptied by `drive_cleanup`).
struct Keys {
    /// Each encrypted remote's recovery code (its text): the current one, and the one before
    /// the last rotation.
    codes: BTreeMap<String, String>,
    old_codes: BTreeMap<String, String>,
    /// The keyrings of the devices that are not joined through the remote's.
    own: BTreeMap<String, Arc<MemKeyring>>,
    /// A join waiting at its approval (`pause_at: "approve"`): the joining device and the
    /// device that approves it.
    joins: BTreeMap<String, String>,
}

static KEYS: Mutex<Keys> = Mutex::new(Keys {
    codes: BTreeMap::new(),
    old_codes: BTreeMap::new(),
    own: BTreeMap::new(),
    joins: BTreeMap::new(),
});

// Lock order: the harness first, then these keys; never the harness while holding them.
fn keys() -> MutexGuard<'static, Keys> {
    KEYS.lock().unwrap_or_else(PoisonError::into_inner)
}

/// Forgets every code, keyring and waiting join (the scenario ended).
pub(crate) fn forget() {
    let mut k = keys();
    k.codes.clear();
    k.old_codes.clear();
    k.own.clear();
    k.joins.clear();
}

/// Keeps `code` as the recovery code of `remote` (its creation, or a rotation that replaced
/// the one before).
pub(crate) fn remember_code(remote: &str, code: String) {
    let mut k = keys();
    if let Some(before) = k.codes.insert(remote.to_string(), code) {
        k.old_codes.insert(remote.to_string(), before);
    }
}

/// `{recovery:<remote>}` and `{recovery_old:<remote>}` in `text`.
pub(crate) fn subst(text: &str) -> String {
    if !text.contains("{recovery") {
        return text.to_string();
    }
    let k = keys();
    let mut out = text.to_string();
    for (remote, code) in &k.codes {
        out = out.replace(&format!("{{recovery:{remote}}}"), code);
    }
    for (remote, code) in &k.old_codes {
        out = out.replace(&format!("{{recovery_old:{remote}}}"), code);
    }
    out
}

/// The keyring a device of `remote` named `device` keeps its keys in: the remote's own (a
/// joined device), or one of its own (`joined: false`; `key_from: "recovery"` unlocks it with
/// the recovery code, `recovery_code` or the remote's, through `bucket`: the device's
/// connection).
pub(crate) fn keyring_for_open(
    remote: &Arc<Remote>,
    device: &str,
    args: &Args,
    bucket: &dyn Drive,
) -> Result<Arc<dyn KeyringStore>, String> {
    let joined = args
        .get("joined")
        .and_then(serde_json::Value::as_bool)
        .unwrap_or(true);
    let key_from = str_arg(args, "key_from");
    if !remote.encrypted {
        if !joined || key_from.is_some() {
            return Err(format!(
                "remote '{}' is plain: `joined` and `key_from` are about an encrypted remote's \
                 drive key",
                remote.name
            ));
        }
        return Ok(remote.keyring.clone());
    }
    match key_from {
        None if joined => Ok(remote.keyring.clone()),
        None => {
            let own = Arc::new(MemKeyring::default());
            keys().own.insert(device.to_string(), own.clone());
            Ok(own)
        }
        Some("recovery") => {
            let own = Arc::new(MemKeyring::default());
            let code = code_for(&remote.name, str_arg(args, "recovery_code"))?;
            recover_into(bucket, own.as_ref(), &remote.drive_name, &code)?;
            keys().own.insert(device.to_string(), own.clone());
            Ok(own)
        }
        Some(other) => Err(format!("no key_from '{other}' (recovery)")),
    }
}

/// The keyring `device` (a device of the harness) keeps its keys in.
pub(crate) fn keyring_of(h: &Harness, device: &str) -> Result<Arc<dyn KeyringStore>, String> {
    let d = h
        .devices
        .iter()
        .find(|d| d.name == device)
        .ok_or_else(|| format!("no device '{device}'"))?;
    if let Some(own) = keys().own.get(device) {
        return Ok(own.clone() as Arc<dyn KeyringStore>);
    }
    let remote = h
        .remotes
        .get(&d.remote)
        .ok_or_else(|| format!("no remote '{}'", d.remote))?;
    Ok(remote.keyring.clone() as Arc<dyn KeyringStore>)
}

/// The recovery code to use: `given`, else the one `remote`'s creation (or last rotation)
/// made.
fn code_for(remote: &str, given: Option<&str>) -> Result<String, String> {
    match given {
        Some(code) => Ok(code.to_string()),
        None => keys().codes.get(remote).cloned().ok_or_else(|| {
            format!("remote '{remote}' has no recovery code (it is plain, or not made here)")
        }),
    }
}

/// One device of the harness as the key flows need it: its name, its connection to its
/// remote, its keyring, the drive's id, and its drive id in AzDrive.
struct KeyDevice {
    name: String,
    drive_id: String,
    remote: Arc<Remote>,
    bucket: FaultDrive,
    keyring: Arc<dyn KeyringStore>,
}

fn key_device(h: &Harness, args: &Args, field: &str) -> Result<KeyDevice, String> {
    let mut one = Args::new();
    if let Some(name) = args.get(field) {
        one.insert(String::from("device"), name.clone());
    }
    let d = h.device(&one)?;
    let remote = h
        .remotes
        .get(&d.remote)
        .cloned()
        .ok_or_else(|| format!("no remote '{}'", d.remote))?;
    if !remote.encrypted {
        return Err(format!(
            "device '{}' is on the plain remote '{}': it has no drive key",
            d.name, remote.name
        ));
    }
    Ok(KeyDevice {
        name: d.name.clone(),
        drive_id: d.drive_id.clone(),
        bucket: FaultDrive {
            remote: remote.clone(),
            device: d.name.clone(),
        },
        keyring: keyring_of(h, &d.name)?,
        remote,
    })
}

/// The device's drive decides again on its next call whether, and with which key, it is
/// encrypted: after it got the key, or after a rotation changed it.
#[cfg(feature = "encryption")]
fn reopen(s: &DriveState, drive_id: &str) {
    if let Some(i) = s.slot_index(drive_id) {
        if let Some(auto) = &s.slots[i].auto {
            auto.reopen();
        }
    }
}

#[cfg(not(feature = "encryption"))]
fn reopen(_: &DriveState, _: &str) {}

/// Whether a pass of any device of `remote` runs.
fn remote_busy(s: &DriveState, h: &Harness, remote: &str) -> Option<String> {
    h.devices
        .iter()
        .filter(|d| d.remote == remote)
        .find(|d| {
            s.sync_view
                .drives
                .get(&d.drive_id)
                .is_some_and(|sync| sync.running.is_some() || sync.again)
        })
        .map(|d| d.name.clone())
}

// ==== The recovery code ====

#[cfg(feature = "encryption")]
fn recover_into(
    bucket: &dyn Drive,
    keyring: &dyn KeyringStore,
    drive: &str,
    code: &str,
) -> Result<(), String> {
    use azul_storage::crypto::{device, keys::RecoveryCode};
    let code = RecoveryCode::parse(code)
        .ok_or_else(|| String::from("that is no recovery code (its groups do not read)"))?;
    device::recover(bucket, keyring, drive, &code)
        .map(drop)
        .map_err(|e| format!("the recovery: {e}"))
}

#[cfg(not(feature = "encryption"))]
fn recover_into(_: &dyn Drive, _: &dyn KeyringStore, _: &str, _: &str) -> Result<(), String> {
    Err(String::from("this AzDrive was built without encryption"))
}

/// `drive_recover`: the device unlocks the drive with a recovery code (`recovery_code`, else
/// the remote's), as AzDrive's "Unlock with the recovery code" does: the code opens the
/// recovery wrap, the device enrols itself (a member wrap of its own) and keeps the drive key.
/// `expect: "refused"` wants the code turned down (an old code after a rotation, a wrong one).
pub(crate) fn recover(s: &mut DriveState, args: &Args) -> Answer {
    let d = with_harness(|h| key_device(h, args, "device"))?;
    let code = code_for(&d.remote.name, str_arg(args, "recovery_code"))?;
    let refused_wanted = match str_arg(args, "expect").unwrap_or("ok") {
        "ok" => false,
        "refused" => true,
        other => return Err(format!("expect '{other}' is neither ok nor refused")),
    };
    let result = recover_into(&d.bucket, d.keyring.as_ref(), &d.remote.drive_name, &code);
    if result.is_ok() {
        reopen(s, &d.drive_id);
    }
    match (result, refused_wanted) {
        (Ok(()), false) => Ok(serde_json::json!({ "recovered": true, "device": d.name })),
        (Err(why), true) => Ok(serde_json::json!({ "recovered": false, "why": why })),
        (Ok(()), true) => Err(format!("the recovery code opened the drive on {}", d.name)),
        (Err(why), false) => Err(why),
    }
}

// ==== Joining with another device's approval ====

/// `drive_join`: the device joins with a join code from `approver` (a device that holds the
/// key), AzDrive's "Add a device": the approver seals the drive key to a one-time invite key in
/// the bucket, the joining device opens it, enrols itself and deletes the invite. With
/// `pause_at: "approve"` the approval waits until `drive_cont` (a step can cut the approver
/// off meanwhile).
pub(crate) fn join(s: &mut DriveState, args: &Args) -> Answer {
    let approver = need(args, "approver")?.to_string();
    let device = with_harness(|h| {
        key_device(h, args, "approver")?;
        Ok(key_device(h, args, "device")?.name)
    })?;
    match str_arg(args, "pause_at") {
        None => {
            let joined = finish(s, &device, &approver);
            joined.and_then(|answer| {
                if answer["joined"] == true {
                    Ok(answer)
                } else {
                    Err(answer["error"].as_str().unwrap_or("the join failed").to_string())
                }
            })
        }
        Some("approve") => {
            keys().joins.insert(device.clone(), approver.clone());
            Ok(serde_json::json!({ "paused": "approve", "device": device, "approver": approver }))
        }
        Some(other) => Err(format!("a join pauses only at approve, not at '{other}'")),
    }
}

/// Whether the device `args` names has a join waiting at its approval (`drive_cont` lets it
/// go on).
pub(crate) fn join_waits(args: &Args) -> bool {
    let Ok(device) = with_harness(|h| Ok(h.device(args)?.name.clone())) else {
        return false;
    };
    keys().joins.contains_key(&device)
}

/// `drive_cont` of a device whose join waits: the approval goes on. Its answer says whether
/// the device joined (`joined`, and the `error` when not: the approver could not seal the key,
/// as when it went offline); `success` either way, since the step only lets the join go on.
pub(crate) fn cont_join(s: &mut DriveState, args: &Args) -> Answer {
    let (device, folder) = with_harness(|h| {
        let d = h.device(args)?;
        Ok((d.name.clone(), d.pair_folder.clone()))
    })?;
    let approver = keys()
        .joins
        .remove(&device)
        .ok_or_else(|| format!("no join of '{device}' waits"))?;
    // A pass paused at a point of its own goes on too, as drive_cont always does.
    azcloud_kit::sync::faults::release(&folder);
    finish(s, &device, &approver)
}

#[cfg(feature = "encryption")]
fn finish(s: &mut DriveState, device: &str, approver: &str) -> Answer {
    use azul_storage::crypto::device as keys_of;
    let (joining, approving) = with_harness(|h| {
        let one = |name: &str| {
            Args::from_iter([(String::from("device"), serde_json::Value::String(name.into()))])
        };
        Ok((key_device(h, &one(device), "device")?, key_device(h, &one(approver), "device")?))
    })?;
    if joining.remote.name != approving.remote.name {
        return Err(format!("{approver} is on another remote than {device}"));
    }
    let drive = &joining.remote.drive_name;
    let refused = |why: String| Ok(serde_json::json!({ "joined": false, "error": why }));
    // The approver's side: it must hold the key, and reach the bucket to leave the invite.
    let key = match keys_of::load_drive_key(approving.keyring.as_ref(), drive) {
        Ok(Some(key)) => key,
        Ok(None) => return refused(format!("{approver} holds no key for the drive")),
        Err(e) => return refused(format!("{approver}'s keyring: {e}")),
    };
    let invite = match keys_of::seal_invite(&approving.bucket, drive, &key) {
        Ok(invite) => invite,
        Err(e) => return refused(format!("{approver} could not approve: {e}")),
    };
    // The joining side opens the invite with the code's secret.
    match keys_of::adopt_invite(&joining.bucket, joining.keyring.as_ref(), drive, &invite) {
        Ok(_) => {
            reopen(s, &joining.drive_id);
            Ok(serde_json::json!({ "joined": true, "device": device }))
        }
        Err(e) => {
            // An invite nobody opened does not stay behind as a key to the drive.
            let _ = keys_of::withdraw_invite(&approving.bucket, &invite);
            refused(format!("{device} could not open the approval: {e}"))
        }
    }
}

#[cfg(not(feature = "encryption"))]
fn finish(_: &mut DriveState, _: &str, _: &str) -> Answer {
    Err(String::from("this AzDrive was built without encryption"))
}

// ==== Rotating the drive key ====

/// `drive_rotate_key`: the remote's owner rotates the drive key, AzDrive's "I was hacked":
/// a new key seals the index, every file key is re-wrapped, every other member and every
/// share link goes, a new recovery code replaces the old one. The devices joined through the
/// owner's keyring hold the new key at once; a device with a keyring of its own is removed
/// and comes back with the new recovery code (or a new approval).
#[cfg(feature = "encryption")]
pub(crate) fn rotate_key(s: &mut DriveState, args: &Args) -> Answer {
    use azul_storage::{crypto::keys::RecoveryKdf, rotation};
    let (remote, cache, joined) = with_harness(|h| {
        let remote = h.remote_of(args)?;
        if !remote.encrypted {
            return Err(format!("remote '{}' is plain: it has no drive key", remote.name));
        }
        if let Some(busy) = remote_busy(s, h, &remote.name) {
            return Err(format!("a pass of {busy} runs: wait for it first (drive_wait_idle)"));
        }
        let own = keys();
        let joined: Vec<String> = h
            .devices
            .iter()
            .filter(|d| d.remote == remote.name && !own.own.contains_key(&d.name))
            .map(|d| d.drive_id.clone())
            .collect();
        drop(own);
        let cache = h.run.join("owner").join(&remote.name).join("index-cache");
        Ok((remote, cache, joined))
    })?;
    let bucket: Arc<dyn Drive> = Arc::new(FaultDrive {
        remote: remote.clone(),
        device: String::from("(owner)"),
    });
    let provider = crate::encryption::meta_provider(Some(cache));
    let kdf = RecoveryKdf::fresh().map_err(|e| e.to_string())?;
    let rotated = rotation::rotate(
        bucket,
        remote.keyring.as_ref(),
        &remote.drive_name,
        &provider,
        kdf,
    )
    .map_err(|e| format!("the key rotation: {e}"))?;
    remember_code(&remote.name, rotated.recovery_code.to_text().to_string());
    for drive_id in &joined {
        reopen(s, drive_id);
    }
    Ok(serde_json::json!({
        "rewrapped": rotated.rewrapped,
        "members_removed": rotated.members_removed,
        "shares_revoked": rotated.shares_revoked,
        "drive_key": rotated.drive_key.to_hex(),
    }))
}

#[cfg(not(feature = "encryption"))]
pub(crate) fn rotate_key(_: &mut DriveState, _: &Args) -> Answer {
    Err(String::from("this AzDrive was built without encryption"))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The tests share the process's keys: one at a time.
    static ONE_AT_A_TIME: Mutex<()> = Mutex::new(());

    fn one_at_a_time() -> MutexGuard<'static, ()> {
        ONE_AT_A_TIME.lock().unwrap_or_else(PoisonError::into_inner)
    }

    fn args(json: serde_json::Value) -> Args {
        match json {
            serde_json::Value::Object(map) => map,
            _ => Args::new(),
        }
    }

    /// A scenario names the recovery sheet's code as `{recovery:<remote>}`; after a rotation
    /// that is the new code, and the one before is `{recovery_old:<remote>}`.
    #[test]
    fn the_recovery_placeholders_name_the_current_and_the_previous_code() {
        let _one = one_at_a_time();
        forget();
        remember_code("r", String::from("AAAA-1111"));
        assert_eq!(subst("{recovery:r}"), "AAAA-1111");
        assert_eq!(subst("{recovery_old:r}"), "{recovery_old:r}", "no rotation yet");
        remember_code("r", String::from("BBBB-2222"));
        assert_eq!(subst("x {recovery:r} y"), "x BBBB-2222 y");
        assert_eq!(subst("{recovery_old:r}"), "AAAA-1111");
        assert_eq!(subst("{recovery:other}"), "{recovery:other}", "another remote's stays");
        forget();
        assert_eq!(subst("{recovery:r}"), "{recovery:r}", "drive_cleanup forgets the codes");
    }

    /// `joined: false` gives the device a keyring of its own that holds nothing: the drive's
    /// key never reaches it from the owner's. A plain remote has no key to be joined to.
    #[test]
    fn a_device_that_is_not_joined_has_an_empty_keyring_of_its_own() {
        let _one = one_at_a_time();
        forget();
        let dir = azul_storage::testing::TempDir::new("azdrive-e2e-keys");
        let plain = Arc::new(
            Remote::create("p", dir.path().join("p"), false, "").expect("a plain remote"),
        );
        let bucket = FaultDrive {
            remote: plain.clone(),
            device: String::from("c"),
        };
        let refused = keyring_for_open(&plain, "c", &args(serde_json::json!({"joined": false})), &bucket);
        assert!(refused.is_err(), "a plain remote has no drive key to be joined to");

        let encrypted = Arc::new(
            Remote::create("e", dir.path().join("e"), true, "").expect("an encrypted remote"),
        );
        encrypted
            .keyring
            .set("azul-storage/drive-key/e2e-remote-e", "the owner's key")
            .expect("the owner's keyring");
        let bucket = FaultDrive {
            remote: encrypted.clone(),
            device: String::from("c"),
        };
        let own = keyring_for_open(&encrypted, "c", &args(serde_json::json!({"joined": false})), &bucket)
            .expect("a keyring of its own");
        assert_eq!(own.get("azul-storage/drive-key/e2e-remote-e").expect("readable"), None);
        let shared = keyring_for_open(&encrypted, "a", &Args::new(), &bucket).expect("the owner's");
        assert_eq!(
            shared.get("azul-storage/drive-key/e2e-remote-e").expect("readable").as_deref(),
            Some("the owner's key"),
            "a joined device shares the owner's keyring"
        );
        {
            let k = keys();
            assert!(k.own.contains_key("c") && !k.own.contains_key("a"));
        }
        let unknown = keyring_for_open(
            &encrypted,
            "d",
            &args(serde_json::json!({"key_from": "passkey"})),
            &bucket,
        );
        assert!(unknown.is_err(), "only the recovery code is a key source");
        forget();
    }
}
