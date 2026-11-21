//! Passes AzDrive starts by itself: after a change its folder watcher heard (`drive_watch`
//! turns the watcher on for an E2E device - off by default, so a scenario's passes stay its
//! own - and `drive_wait_auto` waits for such a pass), and the last passes of a quit
//! (`drive_quit`: the close's flush, without closing the window).

use std::time::{Duration, Instant};

use azul::prelude::*;

use super::{a_state::with_a, harness, pending, with_harness, Answer, Args};
use crate::DriveState;

/// Whether AzDrive watches the folder of the synced drive `drive_id`: every user's drive; an E2E
/// device only after `drive_watch`.
pub(crate) fn watches(drive_id: &str) -> bool {
    let device = harness().as_ref().map(|h| {
        h.devices
            .iter()
            .find(|d| d.drive_id == drive_id)
            .map(|d| d.name.clone())
    });
    match device {
        Some(Some(name)) => {
            super::a_state::peek(|a| a.watched.contains(&name)).unwrap_or(false)
        }
        _ => true,
    }
}

/// `drive_watch`: AzDrive's folder watcher on (`on`, default true) or off for the device.
pub(super) fn watch(args: &Args) -> Answer {
    let name = with_harness(|h| Ok(h.device(args)?.name.clone()))?;
    let on = args.get("on").and_then(serde_json::Value::as_bool).unwrap_or(true);
    with_a(|a| {
        if on {
            a.watched.insert(name.clone());
        } else {
            a.watched.remove(&name);
        }
    })?;
    Ok(serde_json::json!({ "watching": on }))
}

/// `drive_wait_auto`: waits for a pass the device started by itself (no step asked for it) to
/// end, within `within_ms` (default 10000). Answers `passes` (the device's count then).
pub(super) fn wait_auto(s: &DriveState, args: &Args) -> Answer {
    let (name, drive_id, passes) = with_harness(|h| {
        let d = h.device(args)?;
        Ok((d.name.clone(), d.drive_id.clone(), d.passes))
    })?;
    let within = Duration::from_millis(
        args.get("within_ms")
            .and_then(serde_json::Value::as_u64)
            .unwrap_or(10_000),
    );
    let (since, before) = with_a(|a| *a.auto_waits.entry(name.clone()).or_insert((Instant::now(), passes)))?;
    let busy = s
        .sync_view
        .drives
        .get(&drive_id)
        .is_some_and(|d| d.running.is_some() || d.again);
    if passes > before && !busy {
        with_a(|a| a.auto_waits.remove(&name))?;
        return Ok(serde_json::json!({ "passes": passes }));
    }
    if since.elapsed() > within {
        with_a(|a| a.auto_waits.remove(&name))?;
        return Err(format!(
            "{name} started no pass of its own within {} ms (it ran {} since)",
            within.as_millis(),
            passes - before
        ));
    }
    Ok(pending())
}

/// `drive_quit`: what quitting AzDrive does before the window goes (the close's last passes),
/// the window left open: every synced drive gets a last pass; answers `quit` once none runs.
pub(super) fn quit(info: &mut CallbackInfo, app: &RefAny, s: &mut DriveState, args: &Args) -> Answer {
    let first = args.get("_attempt").and_then(serde_json::Value::as_u64).unwrap_or(0) == 0;
    if first {
        crate::closing::last_passes(info, app, s);
    }
    let ids: Vec<String> = with_harness(|h| Ok(h.devices.iter().map(|d| d.drive_id.clone()).collect()))?;
    let busy = ids.iter().any(|id| {
        s.sync_view
            .drives
            .get(id)
            .is_some_and(|d| d.running.is_some() || d.again)
    });
    if busy {
        return Ok(pending());
    }
    Ok(serde_json::json!({ "quit": true }))
}
