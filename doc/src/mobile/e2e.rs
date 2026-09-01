//! Replaying an e2e scenario against a real device.
//!
//! # Why this is a *subset*, and says so loudly
//!
//! The e2e op vocabulary is engine-internal. `mount` installs a DOM,
//! `assert_response` inspects the debug dispatcher's last reply,
//! `snapshot_frame` reaches into the frame cache. None of that is expressible
//! from outside the process — there is no `adb shell mount-a-styled-dom`.
//!
//! What a host driver *can* do is the input half: taps, swipes, keys, text,
//! screenshots, and structural assertions against the platform accessibility
//! tree. That is a genuinely different thing to test — it proves the *platform
//! glue*, the `touchesBegan:` / `InputConnection` / `GestureDetector` path that
//! the in-process harness bypasses entirely.
//!
//! So every op is classified, and the ones this driver cannot honour are
//! **counted and reported**, never skipped silently. A replay that could only
//! run 3 of 40 ops reports itself as INCOMPLETE and exits non-zero, because a
//! green result from a harness that did almost nothing is worse than a red one
//! — it is a false statement about the device.

use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
    thread::sleep,
    time::Duration,
};

use super::{device::Device, Opts};

pub struct HostReplayReport {
    pub name: String,
    pub executed: usize,
    pub failed: Vec<String>,
    /// op name -> how many times it appeared but could not be honoured.
    pub unsupported: BTreeMap<String, usize>,
    pub total: usize,
    pub screenshots: Vec<PathBuf>,
}

impl HostReplayReport {
    /// A replay is only meaningful when nothing failed AND nothing was
    /// silently dropped.
    pub fn complete(&self) -> bool {
        self.failed.is_empty() && self.unsupported.is_empty()
    }

    pub fn print(&self) {
        println!("\n\x1b[1m==> host replay: {}\x1b[0m", self.name);
        println!(
            "  {} of {} ops executed on the device",
            self.executed, self.total
        );
        if !self.failed.is_empty() {
            println!("  \x1b[31mfailed:\x1b[0m");
            for f in &self.failed {
                println!("    - {f}");
            }
        }
        if !self.unsupported.is_empty() {
            let dropped: usize = self.unsupported.values().sum();
            println!(
                "  \x1b[33m{dropped} op(s) a host driver cannot express:\x1b[0m"
            );
            for (op, n) in &self.unsupported {
                println!("    - {op} x{n}");
            }
            println!(
                "    \x1b[90mThese are engine-internal ops (DOM mounting, frame/damage \x1b[0m"
            );
            println!(
                "    \x1b[90minspection, dispatcher replies). Run them with `azul-doc e2e`, \x1b[0m"
            );
            println!(
                "    \x1b[90mwhich drives the same dispatcher in-process.\x1b[0m"
            );
        }
        let verdict = if self.complete() {
            "\x1b[32mCOMPLETE\x1b[0m"
        } else if self.failed.is_empty() {
            "\x1b[33mINCOMPLETE\x1b[0m"
        } else {
            "\x1b[31mFAILED\x1b[0m"
        };
        println!("  verdict: {verdict}");
    }
}

fn num(step: &serde_json::Value, key: &str) -> Option<f32> {
    step.get(key)?.as_f64().map(|v| v as f32)
}

/// Map an engine key name onto an Android keycode. Only the keys that a device
/// driver can meaningfully send; anything else is reported unsupported rather
/// than guessed at, because a wrong keycode produces a passing test of the
/// wrong thing.
fn android_keycode(key: &str) -> Option<&'static str> {
    Some(match key.to_ascii_lowercase().as_str() {
        "escape" | "esc" => "KEYCODE_ESCAPE",
        "tab" => "KEYCODE_TAB",
        "enter" | "return" => "KEYCODE_ENTER",
        "backspace" => "KEYCODE_DEL",
        "delete" => "KEYCODE_FORWARD_DEL",
        "space" => "KEYCODE_SPACE",
        "up" | "arrowup" => "KEYCODE_DPAD_UP",
        "down" | "arrowdown" => "KEYCODE_DPAD_DOWN",
        "left" | "arrowleft" => "KEYCODE_DPAD_LEFT",
        "right" | "arrowright" => "KEYCODE_DPAD_RIGHT",
        "home" => "KEYCODE_MOVE_HOME",
        "end" => "KEYCODE_MOVE_END",
        "pageup" => "KEYCODE_PAGE_UP",
        "pagedown" => "KEYCODE_PAGE_DOWN",
        _ => return None,
    })
}

pub fn replay_scenario(
    device: &Device,
    scenario: &Path,
    out_dir: &Path,
    opts: &Opts,
) -> anyhow::Result<HostReplayReport> {
    let text = std::fs::read_to_string(scenario)
        .map_err(|e| anyhow::anyhow!("cannot read {}: {e}", scenario.display()))?;
    let doc: serde_json::Value = serde_json::from_str(&text)
        .map_err(|e| anyhow::anyhow!("{} is not valid JSON: {e}", scenario.display()))?;

    let name = doc
        .get("name")
        .and_then(|v| v.as_str())
        .unwrap_or_else(|| {
            scenario
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or("scenario")
        })
        .to_string();

    let steps = doc
        .get("steps")
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default();

    // baguette scales coordinates against the logical screen; adb does not
    // need it but asking once keeps both paths on the same numbers.
    let size = device.screen_size().unwrap_or((393.0, 852.0));

    let mut report = HostReplayReport {
        name,
        executed: 0,
        failed: Vec::new(),
        unsupported: BTreeMap::new(),
        total: steps.len(),
        screenshots: Vec::new(),
    };

    // Deferred until the first op that needs it — `uiautomator dump` is slow
    // and most scenarios never assert structurally.
    let mut ui_tree: Option<String> = None;

    for (i, step) in steps.iter().enumerate() {
        let Some(op) = step.get("op").and_then(|v| v.as_str()) else {
            report.failed.push(format!("step {i} has no \"op\""));
            continue;
        };

        let result: anyhow::Result<bool> = (|| {
            Ok(match op {
                "wait" => {
                    let ms = num(step, "ms").unwrap_or(16.0) as u64;
                    sleep(Duration::from_millis(ms));
                    true
                }
                // A host driver has no frame hook. One display refresh is the
                // closest honest approximation, and it is why timing-sensitive
                // scenarios belong in the in-process runner.
                "wait_frame" => {
                    sleep(Duration::from_millis(16));
                    true
                }
                "click" => match (num(step, "x"), num(step, "y")) {
                    (Some(x), Some(y)) => {
                        device.tap(x, y, size)?;
                        true
                    }
                    _ => false,
                },
                // `input motionevent` needs API 30+; a failure here is
                // reported, not swallowed.
                "mouse_down" | "mouse_up" | "mouse_move" | "move" => {
                    match (num(step, "x"), num(step, "y")) {
                        (Some(x), Some(y)) if device.driver.can_inject() => {
                            let action = match op {
                                "mouse_down" => "DOWN",
                                "mouse_up" => "UP",
                                _ => "MOVE",
                            };
                            device.motion(action, x, y, size)?;
                            true
                        }
                        _ => false,
                    }
                }
                // key_up is intentionally a no-op: `input keyevent` sends a
                // complete press, so honouring the up half too would double
                // every keystroke.
                "key_down" => match step.get("key").and_then(|v| v.as_str()) {
                    Some(k) => match android_keycode(k) {
                        Some(code) => {
                            device.key(code)?;
                            true
                        }
                        None => false,
                    },
                    None => false,
                },
                "key_up" => true,
                "set_node_text" | "type_text" => {
                    match step.get("text").and_then(|v| v.as_str()) {
                        Some(t) if op == "type_text" => {
                            device.type_text(t)?;
                            true
                        }
                        _ => false,
                    }
                }
                "scroll_node_by" => {
                    let dx = num(step, "delta_x").unwrap_or(0.0);
                    let dy = num(step, "delta_y").unwrap_or(0.0);
                    // A scroll is a swipe in the OPPOSITE direction: content
                    // moving down by dy means the finger travels up.
                    let cx = size.0 / 2.0;
                    let cy = size.1 / 2.0;
                    device.swipe((cx, cy), (cx - dx, cy - dy), size, 250)?;
                    true
                }
                "take_screenshot" => {
                    let path = out_dir.join(format!("{}-step{i}.png", report.name));
                    device.screenshot(&path)?;
                    report.screenshots.push(path);
                    true
                }
                "assert_exists" | "assert_not_exists" => {
                    let Some(sel) = step.get("selector").and_then(|v| v.as_str()) else {
                        return Ok(false);
                    };
                    if ui_tree.is_none() {
                        ui_tree = device.describe_ui().ok();
                    }
                    let Some(tree) = &ui_tree else {
                        return Ok(false);
                    };
                    // CSS selectors do not survive the trip to a platform a11y
                    // tree; the only honest match is on the identifier text
                    // itself, so `#save-button` looks for "save-button".
                    let needle = sel.trim_start_matches(['#', '.']);
                    let found = tree.contains(needle);
                    let want = op == "assert_exists";
                    if found != want {
                        anyhow::bail!(
                            "step {i}: {op} {sel} — {} in the platform accessibility tree",
                            if found { "present" } else { "absent" }
                        );
                    }
                    true
                }
                _ => false,
            })
        })();

        match result {
            Ok(true) => report.executed += 1,
            Ok(false) => *report.unsupported.entry(op.to_string()).or_insert(0) += 1,
            Err(e) => report.failed.push(format!("{e}")),
        }
    }

    // Printing is the caller's job — it interleaves this with the launch
    // result so the whole run reads as one report.
    let _ = opts;
    Ok(report)
}

// ---------------------------------------------------------------------------
// The device transport: the engine's own runner, on the device
// ---------------------------------------------------------------------------

/// What the in-process runner reported, scraped back out of the device log.
///
/// This is the FULL op vocabulary — it is the same dispatcher `azul-doc e2e`
/// drives, running inside the app on the device. The host replay above exists
/// for the half this cannot see (real UIKit / GestureDetector input); this
/// exists for the half the host replay cannot express.
pub struct DeviceVerdict {
    pub passed: usize,
    pub failed: usize,
    pub xfail: usize,
    pub xpass: usize,
    /// The `test result: …` line, ANSI stripped.
    pub summary: String,
    /// `---- name (FAIL) ----` blocks and the step lines under them.
    pub failures: Vec<String>,
}

impl DeviceVerdict {
    pub fn ok(&self) -> bool {
        self.failed == 0 && self.xpass == 0
    }

    pub fn print(&self) {
        println!("\n\x1b[1m==> on-device e2e (the engine's own runner)\x1b[0m");
        for line in &self.failures {
            println!("  {line}");
        }
        let colour = if self.ok() { "\x1b[32m" } else { "\x1b[31m" };
        println!("  {colour}{}\x1b[0m", self.summary);
    }
}

fn strip_ansi(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut chars = s.chars();
    while let Some(c) = chars.next() {
        if c == '\x1b' {
            // Skip up to and including the final byte of a CSI sequence.
            for c2 in chars.by_ref() {
                if c2.is_ascii_alphabetic() {
                    break;
                }
            }
        } else {
            out.push(c);
        }
    }
    out
}

fn field(line: &str, name: &str) -> usize {
    // "test result: ok. 3 passed; 1 failed; 0 xfailed; 0 xpassed; …"
    line.split(';')
        .find_map(|part| {
            let part = part.trim().trim_start_matches("test result:").trim();
            let (n, rest) = part.split_once(' ')?;
            rest.trim().starts_with(name).then(|| n.parse().ok())?
        })
        .unwrap_or(0)
}

/// Parse the runner's report out of a device log, or `None` if it never ran.
///
/// Absence is the normal case for an APK built without `azul/debug-server`:
/// the property is set, nothing reads it, and the app just starts. That is why
/// this returns an Option rather than failing — the caller falls back to the
/// host replay and says which transport actually drove the app.
pub fn parse_device_verdict(log: &str) -> Option<DeviceVerdict> {
    let clean: Vec<String> = log.lines().map(strip_ansi).collect();
    let summary_idx = clean
        .iter()
        .rposition(|l| l.contains("test result:") && l.contains("passed;"))?;
    let summary = clean[summary_idx]
        .split_once("test result:")
        .map(|(_, tail)| format!("test result:{tail}"))
        .unwrap_or_else(|| clean[summary_idx].clone())
        .trim()
        .to_string();

    let failures: Vec<String> = clean
        .iter()
        .filter(|l| {
            l.contains("... FAIL")
                || l.contains("... XPASS")
                || (l.contains("step ") && l.contains("FAILED:"))
        })
        .map(|l| {
            // logcat prefixes every line with "I/RustStdoutStderr(pid): ".
            l.split_once("): ").map(|(_, t)| t).unwrap_or(l).trim().to_string()
        })
        .collect();

    Some(DeviceVerdict {
        passed: field(&summary, "passed"),
        failed: field(&summary, "failed"),
        xfail: field(&summary, "xfailed"),
        xpass: field(&summary, "xpassed"),
        summary,
        failures,
    })
}
