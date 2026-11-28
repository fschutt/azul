//! Files, sharing and being opened with a file (ledger WP-F): what the app hands to another app, and what another app hands to it.
//!
//! * `files.share_status`: the platform says whether it has a share sheet (`can_share`); a platform without one answers the
//!   request with the REASON (`Unsupported`), which is what this row reports as unavailable. No sheet is shown: it runs in every
//!   unattended run.
//! * `files.share_one`, `files.share_many`, `files.open_with`: the app writes known files into its own folder and shows the system's
//!   chooser for them (one file, two files of one name, "open with" of one file). A person (a harness, in the emulator proof:
//!   `scripts/ci/android_files_proof.py`, which picks ShareLab, a second app that reads what it is given the way a mail app does)
//!   chooses the receiver; what the receiver read is checked THERE, byte for byte. Here the verdict is that the sheet was shown.
//! * `files.open_in`: another app shares a file to this one, or opens it with this one; the app's open handler
//!   (`CallbackInfo::set_open_handler`) heard it. The handler logs ONE line `AZ_SELFTEST_OPEN {json}` per request (the harness reads it)
//!   and keeps `[requests, items, files with a copy, bytes of the first file, fnv-1a of its bytes]` in the running row's scratch.
//!
//! The bytes of every file are a function of nothing but this file (`share_one_bytes` ...): the harness computes the same bytes.

use azul::dialog::{OpenAction, OpenRequest, OpenedKind, ShareItem, ShareRequest, ShareStatus};

use super::{await_feed, Ctx, ProbeDef, Step};
use crate::report::{json_string, Verdict};

/// How long `files.open_in` waits for another app to hand this one something.
const WAIT_MS: u64 = 90_000;
/// The folder of the files the rows write, in the app's own storage.
const FOLDER: &str = "azul-selftest-files";
/// The most bytes of a received file the handler reads to describe it.
const READ_LIMIT: u64 = 64 * 1024 * 1024;

pub static PROBES: &[ProbeDef] = &[
    ProbeDef {
        id: "files.share_status",
        group: "files",
        title: "Share: can this platform show a share sheet, and if not, does it say why",
        human: None,
        note: "",
        widget: None,
        step: share_status,
    },
    ProbeDef {
        id: "files.share_one",
        group: "files",
        title: "Share one file to another app through the system's sheet",
        human: Some("choose an app in the share sheet (ShareLab in the emulator proof): it must show the file's name, size and content"),
        note: "",
        widget: None,
        step: share_one,
    },
    ProbeDef {
        id: "files.share_many",
        group: "files",
        title: "Share two files with the same name (the receiver gets a.md and a (2).md)",
        human: Some("choose an app in the share sheet: it must receive both files, each with its own content"),
        note: "",
        widget: None,
        step: share_many,
    },
    ProbeDef {
        id: "files.open_with",
        group: "files",
        title: "Open a file with another app (the chooser of the apps that can open it)",
        human: Some("choose an app in the list: it must open the file"),
        note: "",
        widget: None,
        step: open_with,
    },
    ProbeDef {
        id: "files.open_in",
        group: "files",
        title: "Be opened with a file: another app shares a file to this one and the open handler hears it",
        human: Some("share a file to this app from another app (or tap one in a file manager and choose this app)"),
        note: "",
        widget: None,
        step: open_in,
    },
];

/// What `files.share_one` writes: a line, then every byte value four times (a type-agnostic payload with a recognisable start).
#[must_use]
pub fn share_one_bytes() -> Vec<u8> {
    let mut bytes = b"AzSelfTest share probe\n".to_vec();
    for _ in 0..4 {
        bytes.extend(0..=255_u8);
    }
    bytes
}

/// What `files.share_many` writes into the first of the two folders (`a/a.md`) and into the second (`b/a.md`).
#[must_use]
pub fn share_many_bytes(second: bool) -> Vec<u8> {
    let mut bytes = if second { b"# a again\n".to_vec() } else { b"# a\n".to_vec() };
    bytes.extend(0..64_u8);
    bytes
}

/// FNV-1a over 64 bits: the digest the open handler logs for a received file (std only; the harness computes the same).
#[must_use]
pub fn fnv1a64(bytes: &[u8]) -> u64 {
    bytes
        .iter()
        .fold(0xcbf2_9ce4_8422_2325_u64, |hash, byte| (hash ^ u64::from(*byte)).wrapping_mul(0x0100_0000_01b3))
}

/// The folder the rows write into: the app's own folder on a phone (`HOME` is it there), the temp directory on a desktop.
fn folder() -> std::path::PathBuf {
    let home = if cfg!(any(target_os = "android", target_os = "ios")) {
        std::env::var_os("HOME").map(std::path::PathBuf::from).filter(|p| p.is_dir())
    } else {
        None
    };
    home.unwrap_or_else(std::env::temp_dir).join(FOLDER)
}

/// `bytes` into `<folder>/<sub>/<name>`; the path.
fn write(sub: &str, name: &str, bytes: &[u8]) -> Result<String, String> {
    let dir = folder().join(sub);
    std::fs::create_dir_all(&dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    let path = dir.join(name);
    std::fs::write(&path, bytes).map_err(|e| format!("{}: {e}", path.display()))?;
    Ok(path.to_string_lossy().into_owned())
}

/// The verdict of a share: the sheet was shown, or the platform's reason (a platform with no sheet is unavailable, not a failure;
/// a platform that has one and could not show it, or a request it refused, is).
fn verdict_of(status: &ShareStatus, what: &str) -> Verdict {
    match status {
        ShareStatus::Shown => Verdict::Pass(format!("the system's sheet for {what} was shown (what a person does in it is not reported)")),
        ShareStatus::Unsupported(reason) => Verdict::Unavailable(reason.as_str().to_string()),
        ShareStatus::Failed(reason) => Verdict::Fail(format!("the sheet for {what} could not be shown: {}", reason.as_str())),
        ShareStatus::Refused(reason) => Verdict::Fail(format!("the request for {what} was refused: {}", reason.as_str())),
    }
}

fn share_status(ctx: &mut Ctx<'_>) -> Step {
    if ctx.info.can_share() {
        return Step::Done(Verdict::Pass("can_share is true: this platform has a share sheet".into()));
    }
    // a platform without a sheet answers with the reason, and shows nothing
    let status = ctx.info.share(ShareRequest::text("AzSelfTest: is there a share sheet"));
    Step::Done(match status {
        ShareStatus::Unsupported(reason) => Verdict::Unavailable(reason.as_str().to_string()),
        other => Verdict::Fail(format!("can_share is false and share answered {other:?}, not Unsupported with the reason")),
    })
}

fn share_one(ctx: &mut Ctx<'_>) -> Step {
    let path = match write("one", "selftest-share.azt", &share_one_bytes()) {
        Ok(path) => path,
        Err(e) => return Step::Done(Verdict::Fail(format!("the file could not be written: {e}"))),
    };
    // the type is the app's: the table has no entry for this extension, and the receiver must be told what the app said
    let item = ShareItem::file(path.as_str()).with_mime("application/vnd.azul.selftest");
    let request = ShareRequest::new().with_item(item).with_title("AzSelfTest: one file");
    let status = ctx.info.share(request);
    Step::Done(verdict_of(&status, "one file"))
}

fn share_many(ctx: &mut Ctx<'_>) -> Step {
    let mut request = ShareRequest::new().with_title("AzSelfTest: several files");
    for (sub, second) in [("a", false), ("b", true)] {
        match write(sub, "a.md", &share_many_bytes(second)) {
            Ok(path) => request = request.with_item(ShareItem::file(path.as_str())),
            Err(e) => return Step::Done(Verdict::Fail(format!("the file could not be written: {e}"))),
        }
    }
    let status = ctx.info.share(request);
    Step::Done(verdict_of(&status, "two files of one name"))
}

fn open_with(ctx: &mut Ctx<'_>) -> Step {
    let path = match write("open", "selftest-open.md", &share_one_bytes()) {
        Ok(path) => path,
        Err(e) => return Step::Done(Verdict::Fail(format!("the file could not be written: {e}"))),
    };
    let status = ctx.info.share(ShareRequest::file(path.as_str()).with_open_with(true));
    Step::Done(verdict_of(&status, "open with"))
}

/// What the open handler keeps in the running row's scratch (the layout of the numbers).
pub const SCRATCH_REQUESTS: usize = 0;
pub const SCRATCH_ITEMS: usize = 1;
pub const SCRATCH_COPIES: usize = 2;
pub const SCRATCH_FIRST_LEN: usize = 3;
pub const SCRATCH_FIRST_FNV: usize = 4;
pub const SCRATCH_LEN: usize = 5;

fn open_in(ctx: &mut Ctx<'_>) -> Step {
    if ctx.phase == 0 {
        ctx.phase = 1;
        await_feed("files.open_in");
    }
    let heard = ctx.scratch;
    if heard.len() >= SCRATCH_LEN && heard[SCRATCH_REQUESTS] > 0 {
        let (items, copies) = (heard[SCRATCH_ITEMS], heard[SCRATCH_COPIES]);
        return Step::Done(if copies == items && items > 0 {
            Verdict::Pass(format!(
                "the open handler heard {} request(s): {items} item(s), every one with a working copy (the first: {} bytes, fnv-1a {:#x})",
                heard[SCRATCH_REQUESTS], heard[SCRATCH_FIRST_LEN], heard[SCRATCH_FIRST_FNV]
            ))
        } else {
            Verdict::Fail(format!("the handler heard {items} item(s) and only {copies} had a working copy: see the AZ_SELFTEST_OPEN line"))
        });
    }
    if ctx.elapsed_ms > WAIT_MS {
        return Step::Done(Verdict::NeedsHuman(
            "nothing was shared to this app: share a file to it from another app (or open one with it) while this row waits".into(),
        ));
    }
    Step::Wait
}

/// What the open handler makes of a request: the line the harness reads and the numbers the running row keeps.
pub struct OpenSummary {
    /// The line after `AZ_SELFTEST_OPEN `.
    pub json: String,
    /// The numbers (see the `SCRATCH_*` indexes): items, files with a working copy, the first file's bytes and digest.
    pub items: u64,
    /// How many of the items are files with a readable working copy.
    pub copies: u64,
    /// Bytes of the first working copy.
    pub first_len: u64,
    /// FNV-1a of the first working copy.
    pub first_fnv: u64,
}

/// Describe `request` the way the harness reads it: every item with the working copy READ back (its length and digest), so that the
/// line proves what the app can open and not what the engine claims.
#[must_use]
pub fn summarize(request: &OpenRequest) -> OpenSummary {
    let action = match request.action {
        OpenAction::View => "View",
        OpenAction::Edit => "Edit",
        OpenAction::Send => "Send",
    };
    let mut items_json = Vec::new();
    let mut copies = 0_u64;
    let (mut first_len, mut first_fnv) = (0_u64, 0_u64);
    let items = request.items.as_ref();
    for item in items {
        let kind = match item.kind {
            OpenedKind::File => "File",
            OpenedKind::Url => "Url",
            OpenedKind::Text => "Text",
        };
        let path = item.path.clone().into_option().map(|p| p.as_str().to_string());
        let (mut len, mut fnv, mut readable) = (0_u64, 0_u64, false);
        if let Some(path) = &path {
            let meta = std::fs::metadata(path).ok();
            if meta.as_ref().is_some_and(|m| m.is_file() && m.len() <= READ_LIMIT) {
                if let Ok(bytes) = std::fs::read(path) {
                    (len, fnv, readable) = (bytes.len() as u64, fnv1a64(&bytes), true);
                }
            }
        }
        if readable {
            if copies == 0 {
                (first_len, first_fnv) = (len, fnv);
            }
            copies += 1;
        }
        items_json.push(format!(
            "{{\"kind\":{},\"name\":{},\"mime\":{},\"size\":{},\"path\":{},\"uri\":{},\"text\":{},\"writable\":{},\"error\":{},\"readable\":{readable},\"len\":{len},\"fnv\":\"{fnv:#x}\"}}",
            json_string(kind),
            json_string(item.name.as_str()),
            json_string(item.mime.as_str()),
            item.size.into_option().map_or_else(|| "null".to_string(), |s| s.to_string()),
            path.as_deref().map_or_else(|| "null".to_string(), json_string),
            json_string(item.uri.as_str()),
            json_string(item.text.as_str()),
            item.writable,
            item.error.clone().into_option().map_or_else(|| "null".to_string(), |e| json_string(e.as_str())),
        ));
    }
    OpenSummary {
        json: format!(
            "{{\"action\":{},\"launched_app\":{},\"skipped\":{},\"items\":[{}]}}",
            json_string(action),
            request.launched_app,
            request.skipped,
            items_json.join(",")
        ),
        items: items.len() as u64,
        copies,
        first_len,
        first_fnv,
    }
}

#[cfg(test)]
#[path = "files_test.rs"]
mod files_test;
