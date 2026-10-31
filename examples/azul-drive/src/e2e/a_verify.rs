//! The read-back (`drive_verify`, and `drive_sync` with `verify`: AzDrive's read-back job, then
//! the pass) and the failing disk it is for (`drive_bitrot`: bytes changed in place under the
//! same size and time).

use std::{
    fs,
    io::{Seek, SeekFrom, Write},
};

use azcloud_kit::sync::verify::{VerifyReport, VERIFY_FILE};
use azul::prelude::*;

use super::{
    a_place::job, a_state::with_a, files::content_of, need, pending, str_arg, with_harness,
    Answer, Args,
};
use crate::{sync_jobs, DriveState};

/// `drive_verify`: AzDrive's read-back of the device's pairing. Answers `checked`,
/// `mismatches` (the drive's copies that did not read back whole), `healed` (of those, sent
/// again from here), `rotted` (this device's copies changed under the same size and time) and
/// `restored` (of those, put back from the drive).
pub(super) fn verify(info: &mut CallbackInfo, app: &RefAny, s: &mut DriveState, args: &Args) -> Answer {
    let state_dir = with_harness(|h| {
        h.device(args)?
            .state_dir
            .clone()
            .ok_or_else(|| String::from("the device is not paired"))
    })?;
    let report_file = state_dir.join(VERIFY_FILE);
    let waiting = with_harness(|h| Ok(h.device(args)?.change_wanted.is_some()))?;
    if !waiting {
        // A report left by an earlier read-back is never taken for this one's.
        let _ = fs::remove_file(&report_file);
    }
    if job(args, |id| sync_jobs::verify(info, app, s, id))?.is_none() {
        return Ok(pending());
    }
    let bytes = fs::read(&report_file).map_err(|_| {
        String::from("the read-back failed: it left no report (AzDrive's message says why)")
    })?;
    let report: VerifyReport = serde_json::from_slice(&bytes).map_err(|e| e.to_string())?;
    // What it listed counts among the device's problems, as a pass's do.
    let listed: Vec<serde_json::Value> = report
        .problems
        .iter()
        .filter_map(|p| serde_json::to_value(p).ok())
        .collect();
    with_harness(|h| {
        let name = h.device(args)?.name.clone();
        h.seen_problems.entry(name).or_default().extend(listed);
        Ok(())
    })?;
    Ok(serde_json::json!({
        "checked": report.checked,
        "mismatches": report.mismatches,
        "healed": report.healed,
        "rotted": report.rotted,
        "restored": report.restored,
    }))
}

/// `drive_sync` with `verify`: the read-back first, then the pass (answered as `drive_sync`).
pub(super) fn sync_verified(info: &mut CallbackInfo, app: &RefAny, s: &mut DriveState, args: &Args) -> Answer {
    let name = with_harness(|h| Ok(h.device(args)?.name.clone()))?;
    if !with_a(|a| a.verified.contains(&name))? {
        let read = verify(info, app, s, args)?;
        if read.get("pending").is_some() {
            return Ok(read);
        }
        with_a(|a| a.verified.insert(name.clone()))?;
    }
    let mut rest = args.clone();
    rest.remove("verify");
    let answer = super::sync(info, app, s, &rest, true);
    if !matches!(&answer, Ok(a) if a.get("pending").is_some()) {
        with_a(|a| a.verified.remove(&name))?;
    }
    answer
}

/// `drive_bitrot`: bytes of a file changed in place, as a failing disk changes them: `hex` (or
/// `byte`, or content as `drive_write`) written at `offset`, the file's size and modification
/// time kept (its change time moves: only a raw write to the disk keeps that too).
pub(super) fn bitrot(args: &Args) -> Answer {
    let path = with_harness(|h| h.path(args, "path"))?;
    let offset = args
        .get("offset")
        .and_then(serde_json::Value::as_u64)
        .ok_or("'offset' is missing")?;
    let bytes = match str_arg(args, "byte") {
        Some(byte) => {
            let mut one = args.clone();
            one.insert(String::from("hex"), serde_json::Value::String(byte.to_string()));
            content_of(&one)?
        }
        None => content_of(args)?,
    };
    let shown = need(args, "path")?;
    let changed = rot(&path, offset, &bytes).map_err(|e| format!("{shown}: {e}"))?;
    Ok(serde_json::json!({ "changed": changed }))
}

/// Writes `bytes` at `offset` inside the file at `path`, its size and modification time kept.
fn rot(path: &std::path::Path, offset: u64, bytes: &[u8]) -> Result<usize, String> {
    let meta = fs::metadata(path).map_err(|e| e.to_string())?;
    if offset + bytes.len() as u64 > meta.len() {
        return Err(format!("it has {} bytes: a rot stays inside the file", meta.len()));
    }
    let mtime = meta.modified().map_err(|e| e.to_string())?;
    let mut file = fs::File::options()
        .write(true)
        .open(path)
        .map_err(|e| e.to_string())?;
    file.seek(SeekFrom::Start(offset))
        .and_then(|_| file.write_all(bytes))
        .and_then(|()| file.set_modified(mtime))
        .map_err(|e| e.to_string())?;
    Ok(bytes.len())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_rot_keeps_the_files_size_and_time_and_stays_inside_it() {
        let dir = azul_storage::testing::TempDir::new("azdrive-e2e-bitrot");
        let path = dir.path().join("thesis.pdf");
        fs::write(&path, vec![0u8; 100]).unwrap();
        let before = fs::metadata(&path).unwrap();
        assert_eq!(rot(&path, 10, &[0xff]).unwrap(), 1);
        let after = fs::metadata(&path).unwrap();
        assert_eq!(after.len(), before.len());
        assert_eq!(after.modified().unwrap(), before.modified().unwrap());
        assert_eq!(fs::read(&path).unwrap()[10], 0xff);
        assert!(rot(&path, 100, &[1]).is_err(), "past the end is no rot");
    }
}
