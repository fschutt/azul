//! The E2E commands of group A (COMMANDS.md, "Group A"): the cloud-only files of a pairing,
//! its modes (a backup, what this device leaves out), the window's actions around a pass and
//! the drive (cancel, quit, the operations log, the trash), and what the computer around it
//! does (its network, its volumes). This file routes them; each family lives in an `a_*` file
//! of its own.

use azul::prelude::*;

use super::{a_lock, a_ops, a_pair, a_place, a_verify, a_versions, Answer, Args, OpDoc, DEVICE, PATH};
use crate::DriveState;

/// Group A's commands, for `list_app_ops` (COMMANDS.md has the long form).
pub(super) const OPS: &[OpDoc] = &[
    OpDoc { name: "drive_evict", summary: "Free up space: the files become cloud-only", args: &[DEVICE, ("path", "string", false), ("glob", "string", false)] },
    OpDoc { name: "drive_hydrate", summary: "bring cloud-only files down", args: &[DEVICE, ("path", "string", false), ("glob", "string", false)] },
    OpDoc { name: "drive_on_demand", summary: "the pairing's on-demand files on or off", args: &[DEVICE, ("on", "bool", true)] },
    OpDoc { name: "drive_corrupt_placeholder", summary: "damage what the base keeps of a cloud-only file", args: &[DEVICE, PATH] },
    OpDoc { name: "drive_write_through_placeholder", summary: "an app writing a file of an on-demand pairing", args: &[DEVICE, PATH, ("text", "string", false), ("size", "number", false), ("seed", "number", false)] },
    OpDoc { name: "drive_scan_read", summary: "read every matching file end to end (a virus scanner)", args: &[DEVICE, ("glob", "string", false)] },
    OpDoc { name: "drive_select", summary: "what this device leaves out (selective sync)", args: &[DEVICE, ("exclude", "array", true)] },
    OpDoc { name: "drive_setting", summary: "one setting of the device's pairing", args: &[DEVICE, ("name", "string", true), ("value", "any", true)] },
    OpDoc { name: "drive_settings", summary: "the device's pairing's settings (after `set`)", args: &[DEVICE, ("set", "object", false)] },
    OpDoc { name: "drive_cancel", summary: "Cancel the pass running", args: &[DEVICE] },
    OpDoc { name: "drive_ops", summary: "the device's operations log as data", args: &[DEVICE] },
    OpDoc { name: "drive_untrash", summary: "put back what the passes trashed", args: &[DEVICE, ("path", "string", false)] },
    OpDoc { name: "drive_network", summary: "the network as AzDrive reads it", args: &[("metered", "bool", false), ("constrained", "bool", false), ("hotspot", "bool", false), ("connected", "bool", false)] },
    OpDoc { name: "drive_wake", summary: "the system woke: every drive gets a pass", args: &[] },
    OpDoc { name: "drive_lock", summary: "a file locked by another app, or the drive key off the device", args: &[DEVICE, ("path", "string", false)] },
    OpDoc { name: "drive_unlock", summary: "the lock let go, or the drive key back", args: &[DEVICE, ("path", "string", false)] },
    OpDoc { name: "drive_versions", summary: "a file's stored versions, oldest first", args: &[DEVICE, ("remote", "string", false), ("key", "string", true)] },
    OpDoc { name: "drive_restore_version", summary: "a stored version back as the drive's", args: &[DEVICE, ("key", "string", true), ("version", "number", false), ("hash", "string", false)] },
    OpDoc { name: "drive_restore_deleted", summary: "a deleted file back as its last version was", args: &[DEVICE, ("key", "string", true)] },
    OpDoc { name: "drive_verify", summary: "the read-back of both sides", args: &[DEVICE] },
    OpDoc { name: "drive_bitrot", summary: "bytes changed in place under the same size and time", args: &[DEVICE, PATH, ("offset", "number", true), ("hex", "string", false), ("byte", "string", false)] },
];

/// Whether group A answers `name` with these arguments: its own commands, and the shared ones
/// whose arguments only group A knows (a pairing's mode).
pub(super) fn handles(name: &str, args: &Args) -> bool {
    match name {
        "drive_pair" => a_pair::has_options(args),
        "drive_sync" => args.contains_key("verify"),
        _ => OPS.iter().any(|op| op.name == name),
    }
}

/// Runs group A's command `name`.
pub(super) fn run(
    info: &mut CallbackInfo,
    app: &RefAny,
    s: &mut DriveState,
    name: &str,
    args: &Args,
) -> Answer {
    match name {
        "drive_pair" => a_pair::pair(info, app, s, args),
        "drive_sync" => a_verify::sync_verified(info, app, s, args),
        "drive_verify" => a_verify::verify(info, app, s, args),
        "drive_versions" => a_versions::versions(s, args),
        "drive_restore_version" => a_versions::restore_version(s, args),
        "drive_restore_deleted" => a_versions::restore_deleted(s, args),
        "drive_bitrot" => a_verify::bitrot(args),
        "drive_select" => a_pair::select(info, app, s, args),
        "drive_on_demand" => a_pair::on_demand(info, app, s, args),
        "drive_evict" => a_place::evict(info, app, s, args),
        "drive_hydrate" => a_place::hydrate(info, app, s, args),
        "drive_corrupt_placeholder" => a_place::corrupt_placeholder(args),
        "drive_write_through_placeholder" => a_place::write_through(info, app, s, args),
        "drive_scan_read" => a_place::scan_read(args),
        "drive_setting" => a_ops::setting(info, app, s, args),
        "drive_settings" => a_ops::settings(info, app, s, args),
        "drive_cancel" => a_ops::cancel(s, args),
        "drive_ops" => a_ops::ops(args),
        "drive_untrash" => a_ops::untrash(args),
        "drive_network" => a_ops::network(s, args),
        "drive_wake" => a_ops::wake(info, app, s),
        "drive_lock" => a_lock::lock(s, args),
        "drive_unlock" => a_lock::unlock(s, args),
        other => Err(format!("'{other}' is not one of group A's commands")),
    }
}

/// Before the sandbox goes (`drive_cleanup`): the files a scenario held locked are let go.
pub(super) fn cleanup() {
    super::a_state::peek(|a| a.locks.clear());
}
