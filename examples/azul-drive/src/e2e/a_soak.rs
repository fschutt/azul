//! A soak (`drive_soak`): many random steps on several devices of one remote - edits, deletes,
//! renames, passes, passes that crash at a point, restarts, a device cut off and back - from a
//! seed, so a run repeats; the questions a pass asks are answered as a user would (a held
//! conflict: keep both; a mass delete: keep them). At the end every device is reconnected and
//! synced until they agree. `drive_check` `kept_soak` then holds when every version a pass saw
//! on a device is still somewhere: in a folder, a trash or the drive's stored objects.
//!
//! The steps run across the command's calls (a pass is waited for with `pending`), so give the
//! step a `timeout_ms` that fits its `ops`.

use std::collections::{BTreeSet, VecDeque};

use azul::prelude::*;

use super::{
    a_state::with_a, answer_question, pending, resolve, restart, sync, with_harness, Answer, Args,
};
use crate::DriveState;

/// The crash points a soak's crashing pass picks from.
const POINTS: &[&str] = &["after-scan", "before-upload", "after-blob", "after-commit", "before-rename", "before-base-save"];

/// One step of a soak.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum Step {
    Write { device: String, file: String, text: String },
    Delete { device: String, file: String },
    Rename { device: String, from: String, to: String },
    Sync { device: String, crash_at: Option<String> },
    Restart { device: String },
    Partition { device: String, on: bool },
    /// A held conflict answered "keep both", a held mass delete "keep them".
    Resolve { device: String, key: String },
    KeepAll { device: String },
}

/// A soak running.
pub(super) struct Soak {
    pub steps: VecDeque<Step>,
    /// The step at the front was started and waits.
    pub started: bool,
    /// Every text the soak wrote, and the ones a finished pass found in a device's folder.
    pub written: BTreeSet<String>,
    pub synced: BTreeSet<String>,
    /// The steps run so far.
    pub ran: usize,
}

/// A repeatable stream of numbers for `seed` (splitmix64).
struct Draw(u64);

impl Draw {
    fn next(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }

    fn below(&mut self, n: usize) -> usize {
        usize::try_from(self.next() % (n.max(1) as u64)).unwrap_or(0)
    }
}

/// The soak's steps for `devices`: `ops` random ones over `files` names, then each device
/// reconnected and two rounds of passes.
pub(super) fn plan(devices: &[String], ops: usize, seed: u64, files: usize) -> VecDeque<Step> {
    let mut draw = Draw(seed);
    let mut steps = VecDeque::new();
    let name = |k: usize| format!("soak/f{k:02}.txt");
    for n in 0..ops {
        let device = devices[draw.below(devices.len())].clone();
        let file = name(draw.below(files));
        let step = match draw.below(100) {
            0..=34 => Step::Write { device, file, text: format!("soak {seed} {n}") },
            35..=64 => Step::Sync { device, crash_at: None },
            65..=74 => Step::Delete { device, file },
            75..=79 => Step::Rename { device, from: file, to: name(draw.below(files)) },
            80..=87 => Step::Restart { device },
            88..=93 => Step::Sync {
                device,
                crash_at: Some(POINTS[draw.below(POINTS.len())].to_string()),
            },
            _ => Step::Partition { device, on: draw.below(2) == 0 },
        };
        steps.push_back(step);
    }
    for device in devices {
        steps.push_back(Step::Partition { device: device.clone(), on: false });
    }
    for _ in 0..2 {
        for device in devices {
            steps.push_back(Step::Sync { device: device.clone(), crash_at: None });
        }
    }
    steps
}

fn one(device: &str) -> Args {
    Args::from_iter([(String::from("device"), serde_json::Value::String(device.to_string()))])
}

/// `drive_soak`: `devices` (default every device), `ops`, `seed`, `files`. Answers `ran` and
/// `written` (texts) once the devices agree.
pub(super) fn soak(info: &mut CallbackInfo, app: &RefAny, s: &mut DriveState, args: &Args) -> Answer {
    if with_a(|a| a.soak.is_none())? {
        let mut devices: Vec<String> = args
            .get("devices")
            .and_then(serde_json::Value::as_array)
            .map(|a| a.iter().filter_map(|v| v.as_str().map(str::to_string)).collect())
            .unwrap_or_default();
        if devices.is_empty() {
            devices = with_harness(|h| Ok(h.devices.iter().map(|d| d.name.clone()).collect()))?;
        }
        if devices.is_empty() {
            return Err(String::from("no device to soak (drive_open some first)"));
        }
        let num = |k: &str, d: u64| args.get(k).and_then(serde_json::Value::as_u64).unwrap_or(d);
        let steps = plan(
            &devices,
            usize::try_from(num("ops", 100)).unwrap_or(100),
            num("seed", 1),
            usize::try_from(num("files", 10)).unwrap_or(10).max(1),
        );
        with_a(|a| {
            a.soak = Some(Soak { steps, started: false, written: BTreeSet::new(), synced: BTreeSet::new(), ran: 0 });
        })?;
    }
    // Steps that need no waiting run at once, a few dozen per call.
    for _ in 0..64 {
        let next = with_a(|a| a.soak.as_ref().map(|k| (k.steps.front().cloned(), k.started)))?;
        let Some((step, started)) = next else {
            return Err(String::from("the soak is gone"));
        };
        let Some(step) = step else {
            let done = with_a(|a| a.soak.take())?.ok_or("the soak is gone")?;
            with_harness(|h| {
                h.texts.extend(done.written.iter().cloned());
                Ok(())
            })?;
            with_a(|a| a.soaked = Some(done.synced.clone()))?;
            return Ok(serde_json::json!({ "ran": done.ran, "written": done.written.len() }));
        };
        match run_step(info, app, s, &step, started)? {
            Progress::Waits => {
                with_a(|a| {
                    if let Some(k) = a.soak.as_mut() {
                        k.started = true;
                    }
                })?;
                return Ok(pending());
            }
            Progress::Done(follow) => {
                with_a(|a| {
                    if let Some(k) = a.soak.as_mut() {
                        k.steps.pop_front();
                        k.started = false;
                        k.ran += 1;
                        if let Step::Write { text, .. } = &step {
                            k.written.insert(text.clone());
                        }
                        // The questions the pass left are answered before the next step.
                        for f in follow.into_iter().rev() {
                            k.steps.push_front(f);
                        }
                    }
                })?;
                if let Step::Sync { device, .. } = &step {
                    note_synced(device)?;
                }
            }
        }
    }
    Ok(pending())
}

enum Progress {
    Waits,
    /// Done, and the steps that must come next (the pass's questions answered).
    Done(Vec<Step>),
}

/// The texts in `device`'s folder now: what its pass just saw there.
fn note_synced(device: &str) -> Result<(), String> {
    let root = with_harness(|h| Ok(h.device(&one(device))?.root.clone()))?;
    let texts: Vec<String> = super::files::user_files(&root)
        .into_iter()
        .filter_map(|(_, p)| std::fs::read_to_string(p).ok())
        .filter(|t| t.starts_with("soak "))
        .collect();
    with_a(|a| {
        if let Some(k) = a.soak.as_mut() {
            k.synced.extend(texts);
        }
    })
}

fn run_step(info: &mut CallbackInfo, app: &RefAny, s: &mut DriveState, step: &Step, started: bool) -> Result<Progress, String> {
    let root_of = |device: &str| with_harness(|h| Ok(h.device(&one(device))?.root.clone()));
    match step {
        Step::Write { device, file, text } => {
            let path = root_of(device)?.join(file);
            if let Some(parent) = path.parent() {
                std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
            }
            std::fs::write(&path, text).map_err(|e| format!("{}: {e}", path.display()))?;
            Ok(Progress::Done(Vec::new()))
        }
        Step::Delete { device, file } => {
            let _ = std::fs::remove_file(root_of(device)?.join(file));
            Ok(Progress::Done(Vec::new()))
        }
        Step::Rename { device, from, to } => {
            let root = root_of(device)?;
            if from != to && !root.join(to).exists() {
                let _ = std::fs::rename(root.join(from), root.join(to));
            }
            Ok(Progress::Done(Vec::new()))
        }
        Step::Partition { device, on } => {
            with_harness(|h| {
                let remote = h.remote_of(&one(device))?;
                remote.set_faults(device, false, &Args::from_iter([(String::from("partition"), serde_json::Value::Bool(*on))]));
                Ok(())
            })?;
            Ok(Progress::Done(Vec::new()))
        }
        Step::Restart { device } => match restart(s, &one(device)) {
            Ok(_) => Ok(Progress::Done(Vec::new())),
            // A pass of an answer still runs: the restart comes after it.
            Err(_) => Ok(Progress::Waits),
        },
        Step::Sync { device, crash_at } => {
            let mut args = one(device);
            args.insert(String::from("expect"), "any".into());
            if let Some(point) = crash_at {
                args.insert(String::from("crash_at"), point.clone().into());
            }
            if started {
                args.insert(String::from("_attempt"), 1.into());
            }
            let answer = sync(info, app, s, &args, true)?;
            if answer.get("pending").is_some() {
                return Ok(Progress::Waits);
            }
            Ok(Progress::Done(questions(s, device)?))
        }
        Step::Resolve { device, key } => {
            let mut args = one(device);
            args.insert(String::from("key"), key.clone().into());
            args.insert(String::from("choice"), "both".into());
            match resolve(info, app, s, &args) {
                Ok(a) if a.get("pending").is_some() => Ok(Progress::Waits),
                // Answered, or no longer held (a later pass took it): on.
                _ => Ok(Progress::Done(Vec::new())),
            }
        }
        Step::KeepAll { device } => {
            let mut args = one(device);
            args.insert(String::from("question"), "mass_delete".into());
            args.insert(String::from("delete_too"), false.into());
            match answer_question(info, app, s, &args) {
                Ok(a) if a.get("pending").is_some() => Ok(Progress::Waits),
                _ => Ok(Progress::Done(Vec::new())),
            }
        }
    }
}

/// What the device's last pass left waiting for the user, as steps.
fn questions(s: &DriveState, device: &str) -> Result<Vec<Step>, String> {
    let drive_id = with_harness(|h| Ok(h.device(&one(device))?.drive_id.clone()))?;
    let states = s.sync_view.store.states(&drive_id);
    let mut out: Vec<Step> = states
        .conflicts()
        .iter()
        .map(|c| Step::Resolve { device: device.to_string(), key: c.key.clone() })
        .collect();
    if states.mass_delete.is_some() {
        out.push(Step::KeepAll { device: device.to_string() });
    }
    Ok(out)
}

/// `drive_check` `kept_soak`: every version a soak's pass saw in a device's folder is still in
/// some folder, trash or stored object.
pub(super) fn kept_soak(s: &mut DriveState, args: &Args) -> Answer {
    let seen = with_a(|a| a.soaked.clone())?.ok_or("no soak ran (drive_soak first)")?;
    let mut check = args.clone();
    check.insert(String::from("check"), "kept".into());
    check.insert(
        String::from("texts"),
        serde_json::Value::Array(seen.into_iter().map(serde_json::Value::String).collect()),
    );
    super::checks::check(s, &check)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_soak_is_the_same_for_a_seed_and_ends_with_every_device_back_and_synced() {
        let devices = vec![String::from("a"), String::from("b")];
        let one = plan(&devices, 200, 7, 10);
        assert_eq!(one, plan(&devices, 200, 7, 10));
        assert_ne!(one, plan(&devices, 200, 8, 10));
        assert_eq!(one.len(), 200 + 2 + 4);
        let tail: Vec<&Step> = one.iter().skip(200).collect();
        assert!(matches!(tail[0], Step::Partition { on: false, .. }));
        assert!(tail[2..].iter().all(|s| matches!(s, Step::Sync { crash_at: None, .. })));
        assert!(one.iter().any(|s| matches!(s, Step::Sync { crash_at: Some(_), .. })));
        assert!(one.iter().any(|s| matches!(s, Step::Write { .. })));
    }
}
