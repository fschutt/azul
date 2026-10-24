//! The E2E commands of group A (COMMANDS.md, "Group A"): the cloud-only files of a pairing,
//! its modes (a backup, what this device leaves out), the window's actions around a pass and
//! the drive (cancel, quit, the operations log, the trash), and what the computer around it
//! does (its network, its volumes). This file routes them; each family lives in an `a_*` file
//! of its own.

use azul::prelude::*;

use super::{a_pair, a_place, Answer, Args, OpDoc, DEVICE, PATH};
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
];

/// Whether group A answers `name` with these arguments: its own commands, and the shared ones
/// whose arguments only group A knows (a pairing's mode).
pub(super) fn handles(name: &str, args: &Args) -> bool {
    match name {
        "drive_pair" => a_pair::has_options(args),
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
        "drive_select" => a_pair::select(info, app, s, args),
        "drive_on_demand" => a_pair::on_demand(info, app, s, args),
        "drive_evict" => a_place::evict(info, app, s, args),
        "drive_hydrate" => a_place::hydrate(info, app, s, args),
        "drive_corrupt_placeholder" => a_place::corrupt_placeholder(args),
        "drive_write_through_placeholder" => a_place::write_through(info, app, s, args),
        "drive_scan_read" => a_place::scan_read(args),
        other => Err(format!("'{other}' is not one of group A's commands")),
    }
}
