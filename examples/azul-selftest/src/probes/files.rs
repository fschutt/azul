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

use azul::{
    dialog::{ExportResult, FileDialog, FolderPickResult, OpenAction, OpenRequest, OpenedKind, ShareItem, ShareRequest, ShareStatus},
    error::DocumentError,
    file::PlaceKind,
    option::OptionString,
    prelude::*,
};

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
        id: "files.documents",
        group: "files",
        title: "Documents: the app's own storage as a place; create, write, read, rename, delete by locator; the failures that say what to do",
        human: None,
        note: "",
        widget: None,
        step: documents_round_trip,
    },
    ProbeDef {
        id: "files.folder_grant",
        group: "files",
        title: "A folder the person grants: the system's folder picker, the grant kept, a file made inside it and read back",
        human: Some("choose a folder in the system's folder picker (AzProof in Documents on the emulator) and allow this app to use it"),
        note: "",
        widget: None,
        step: folder_grant,
    },
    ProbeDef {
        id: "files.folder_use",
        group: "files",
        title: "A folder granted earlier is still a place after the app was closed and opened again",
        human: Some("run files.folder_grant first; then close the app, open it again and run this row"),
        note: "",
        widget: None,
        step: folder_use,
    },
    ProbeDef {
        id: "files.export",
        group: "files",
        title: "Export bytes through the system's save dialog: the answer is a name only after the file was written",
        human: Some("choose where to save the file in the system's save dialog"),
        note: "",
        widget: None,
        step: export,
    },
    ProbeDef {
        id: "files.write_back",
        group: "files",
        title: "A file another app opened this one with (and may be written) is saved back where it came from",
        human: Some("let another app open a writable file with this app (ShareLab's Edit); the row appends a line and saves it back"),
        note: "",
        widget: None,
        step: write_back,
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

/// The file the open handler appends its lines to, next to the files of the rows: a request that STARTED the app (the system opened
/// it, nobody captured its console) is read from here.
pub const OPEN_LOG: &str = "open.log";

/// Append one line to [`OPEN_LOG`] (a failure is ignored: the console line is the first record).
pub fn append_open_log(line: &str) {
    use std::io::Write as _;
    let dir = folder();
    if std::fs::create_dir_all(&dir).is_err() {
        return;
    }
    if let Ok(mut file) = std::fs::OpenOptions::new().create(true).append(true).open(dir.join(OPEN_LOG)) {
        drop(writeln!(file, "{line}"));
    }
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

// ---- Documents ----

/// A line of the report for a failure of `Documents`: what happened and what the app does next.
fn said(what: &str, error: &DocumentError) -> String {
    let kind = match error {
        DocumentError::NotFound(_) => "NotFound (forget the locator)",
        DocumentError::Denied(_) => "Denied (ask for the folder again)",
        DocumentError::TimedOut(_) => "TimedOut (retry or tell the person)",
        DocumentError::Unsupported(_) => "Unsupported",
        DocumentError::Exists(_) => "Exists",
        DocumentError::NotADirectory(_) => "NotADirectory",
        DocumentError::Cancelled => "Cancelled",
        DocumentError::Failed(_) => "Failed",
    };
    format!("{what}: {kind} - {}", error.describe().as_str())
}

/// Every call on the app's own storage, the way an app uses them, ending with the failures an app has to tell apart.
fn documents_round_trip(ctx: &mut Ctx<'_>) -> Step {
    let documents = ctx.info.documents();
    let places = documents.places();
    let Some(storage) = places.as_ref().iter().find(|p| p.kind == PlaceKind::AppStorage) else {
        return Step::Done(Verdict::Fail(format!("places() lists no app storage ({} places)", places.as_ref().len())));
    };
    let root = storage.uri.as_str().to_string();
    let folder = format!("{root}/azul-selftest-documents");
    // a folder of an earlier run: gone first (a missing one is fine)
    drop(documents.delete(folder.as_str()).into_result());
    let made = match documents.create_dir(root.as_str(), "azul-selftest-documents").into_result() {
        Ok(entry) => entry,
        Err(e) => return Step::Done(Verdict::Fail(said("create_dir", &e))),
    };
    if !made.is_dir || made.name.as_str() != "azul-selftest-documents" {
        return Step::Done(Verdict::Fail(format!("create_dir answered {:?} {:?}", made.name.as_str(), made.is_dir)));
    }
    let created = match documents.create_file(made.uri.clone(), "note.txt", "text/plain").into_result() {
        Ok(entry) => entry,
        Err(e) => return Step::Done(Verdict::Fail(said("create_file", &e))),
    };
    let bytes = share_one_bytes();
    let written = match documents.write(created.uri.clone(), bytes.clone().into()).into_result() {
        Ok(entry) => entry,
        Err(e) => return Step::Done(Verdict::Fail(said("write", &e))),
    };
    if written.size.into_option() != Some(bytes.len() as u64) {
        return Step::Done(Verdict::Fail(format!("write: the entry says {:?} bytes, {} were written", written.size.into_option(), bytes.len())));
    }
    // a range of the middle, and the whole file
    let middle = match documents.read(created.uri.clone(), 100, 50).into_result() {
        Ok(read) => read,
        Err(e) => return Step::Done(Verdict::Fail(said("read", &e))),
    };
    if middle.as_ref() != &bytes[100..150] {
        return Step::Done(Verdict::Fail("read of a range gave other bytes than were written".to_string()));
    }
    match documents.read(created.uri.clone(), 0, 1_000_000).into_result() {
        Ok(all) if all.as_ref() == bytes.as_slice() => {}
        Ok(all) => return Step::Done(Verdict::Fail(format!("read of the whole file gave {} bytes, {} were written", all.as_ref().len(), bytes.len()))),
        Err(e) => return Step::Done(Verdict::Fail(said("read", &e))),
    }
    let listed = match documents.children(made.uri.clone()).into_result() {
        Ok(entries) => entries,
        Err(e) => return Step::Done(Verdict::Fail(said("children", &e))),
    };
    if listed.as_ref().len() != 1 || listed.as_ref()[0].name.as_str() != "note.txt" {
        return Step::Done(Verdict::Fail(format!("children lists {} entries", listed.as_ref().len())));
    }
    let renamed = match documents.rename(created.uri.clone(), "renamed.txt").into_result() {
        Ok(entry) => entry,
        Err(e) => return Step::Done(Verdict::Fail(said("rename", &e))),
    };
    // the failures an app tells apart
    let exists = documents.create_file(made.uri.clone(), "renamed.txt", "text/plain").into_result();
    let not_found = documents.stat(created.uri.clone()).into_result();
    let not_a_folder = documents.children(renamed.uri.clone()).into_result();
    let hostile = documents.create_file(made.uri.clone(), "../escape", "text/plain").into_result();
    match (&exists, &not_found, &not_a_folder, &hostile) {
        (Err(DocumentError::Exists(_)), Err(DocumentError::NotFound(_)), Err(DocumentError::NotADirectory(_) | DocumentError::Failed(_)), Err(DocumentError::Failed(_))) => {}
        other => return Step::Done(Verdict::Fail(format!("the failures were not told apart: Exists {:?}, NotFound {:?}, NotADirectory {:?}, a hostile name {:?}", other.0.is_err(), other.1.is_err(), other.2.is_err(), other.3.is_err()))),
    }
    if let Err(e) = documents.delete(made.uri.clone()).into_result() {
        return Step::Done(Verdict::Fail(said("delete", &e)));
    }
    if !matches!(documents.stat(made.uri.clone()).into_result(), Err(DocumentError::NotFound(_))) {
        return Step::Done(Verdict::Fail("a deleted folder is still there".to_string()));
    }
    Step::Done(Verdict::Pass(format!(
        "{} places; a folder, a file, {} bytes written and read back (a range too), renamed, deleted; Exists, NotFound, NotADirectory and a hostile name told apart",
        places.as_ref().len(),
        bytes.len()
    )))
}

/// The answer of the folder picker: it goes to the log (a harness reads it) and to the running row.
extern "C" fn on_folder(mut data: RefAny, _info: CallbackInfo, result: RefAny) -> Update {
    let text = match FolderPickResult::downcast(result).into_option() {
        Some(picked) => match (picked.locator.into_option(), picked.error.into_option()) {
            (Some(locator), _) => format!("granted {}", locator.as_str()),
            (None, Some(why)) => format!("error {}", why.as_str()),
            (None, None) => "cancelled".to_string(),
        },
        None => "no answer".to_string(),
    };
    println!("AZ_SELFTEST_FOLDER {text}");
    if let Some(mut state) = data.downcast_mut::<crate::SelfTest>() {
        state.note_text(text);
    }
    Update::DoNothing
}

/// Read, write, rename and delete inside the granted folder `locator`: the same calls as on the app's own storage, through the system's
/// storage framework. Pass(numbers) or Fail(what and why).
fn use_granted_folder(ctx: &mut Ctx<'_>, locator: &str) -> Verdict {
    let documents = ctx.info.documents();
    let listed = match documents.children(locator).into_result() {
        Ok(entries) => entries,
        Err(e) => return Verdict::Fail(said("children of the granted folder", &e)),
    };
    let before = listed.as_ref().len();
    let name = format!("azul-selftest-{}.txt", ctx.elapsed_ms % 100_000);
    let created = match documents.create_file(locator, name.as_str(), "text/plain").into_result() {
        Ok(entry) => entry,
        Err(e) => return Verdict::Fail(said("create_file in the granted folder", &e)),
    };
    let bytes = share_one_bytes();
    if let Err(e) = documents.write(created.uri.clone(), bytes.clone().into()).into_result() {
        drop(documents.delete(created.uri.clone()).into_result());
        return Verdict::Fail(said("write into the granted folder", &e));
    }
    let back = documents.read(created.uri.clone(), 0, 1_000_000).into_result();
    let same = matches!(&back, Ok(read) if read.as_ref() == bytes.as_slice());
    // the same name again is told as taken, not made a second time under another name
    let again = documents.create_file(locator, name.as_str(), "text/plain").into_result();
    let exists_told = matches!(again, Err(DocumentError::Exists(_)));
    let removed = documents.delete(created.uri.clone()).into_result().is_ok();
    let after = documents.children(locator).into_result().map(|e| e.as_ref().len()).unwrap_or(usize::MAX);
    match (same, exists_told, removed, after == before) {
        (true, true, true, true) => Verdict::Pass(format!(
            "the granted folder listed {before} entries; a file of {} bytes was made, written, read back, refused a second time (Exists) and deleted",
            bytes.len()
        )),
        other => Verdict::Fail(format!("bytes back {}, Exists told {}, deleted {}, the folder as before {}", other.0, other.1, other.2, other.3)),
    }
}

fn folder_grant(ctx: &mut Ctx<'_>) -> Step {
    if ctx.phase == 0 {
        ctx.phase = 1;
        await_feed("files.folder_grant");
        // the picker opens where the proof put a folder (API 26+); a person finds it from there
        let _request = FileDialog::open_folder(
            "AzSelfTest: choose a folder".into(),
            OptionString::Some("/storage/emulated/0/Documents/AzProof".into()),
            ctx.app.clone(),
            on_folder,
        );
    }
    if let Some(answer) = ctx.texts.first().cloned() {
        if let Some(locator) = answer.strip_prefix("granted ") {
            let locator = locator.to_string();
            // the folder is listed by `places()` as a granted one, and works
            let listed = ctx.info.documents().places();
            let kept = listed.as_ref().iter().any(|p| p.kind == PlaceKind::Granted && p.uri.as_str() == locator);
            let used = use_granted_folder(ctx, &locator);
            return Step::Done(match (kept, used) {
                (true, Verdict::Pass(text)) => Verdict::Pass(format!("{text}; places() lists it as a granted folder")),
                (false, Verdict::Pass(text)) => Verdict::Fail(format!("{text}; but places() does not list the folder as granted")),
                (_, other) => other,
            });
        }
        return Step::Done(if answer == "cancelled" {
            Verdict::NeedsHuman("the folder picker was cancelled: choose a folder".into())
        } else {
            Verdict::Fail(format!("the folder picker answered: {answer}"))
        });
    }
    if ctx.elapsed_ms > WAIT_MS {
        return Step::Done(Verdict::NeedsHuman("nobody chose a folder in the picker".into()));
    }
    Step::Wait
}

fn folder_use(ctx: &mut Ctx<'_>) -> Step {
    let places = ctx.info.documents().places();
    let granted: Vec<String> = places
        .as_ref()
        .iter()
        .filter(|p| p.kind == PlaceKind::Granted)
        .map(|p| p.uri.as_str().to_string())
        .collect();
    let Some(locator) = granted.first() else {
        return Step::Done(Verdict::NeedsHuman("no folder is granted: run files.folder_grant first".into()));
    };
    let verdict = use_granted_folder(ctx, locator);
    Step::Done(match verdict {
        Verdict::Pass(text) => Verdict::Pass(format!("{} granted folder(s) are places after the restart; {text}", granted.len())),
        other => other,
    })
}

/// The answer of the export dialog.
extern "C" fn on_export(mut data: RefAny, _info: CallbackInfo, result: RefAny) -> Update {
    let text = match ExportResult::downcast(result).into_option() {
        Some(done) => match (done.saved_as.into_option(), done.error.into_option()) {
            (Some(name), _) => format!("saved {}", name.as_str()),
            (None, Some(why)) => format!("error {}", why.as_str()),
            (None, None) => "cancelled".to_string(),
        },
        None => "no answer".to_string(),
    };
    println!("AZ_SELFTEST_EXPORT {text}");
    if let Some(mut state) = data.downcast_mut::<crate::SelfTest>() {
        state.note_text(text);
    }
    Update::DoNothing
}

fn export(ctx: &mut Ctx<'_>) -> Step {
    if ctx.phase == 0 {
        ctx.phase = 1;
        await_feed("files.export");
        let _request = FileDialog::export_bytes(
            "selftest-export.azt".into(),
            "application/octet-stream".into(),
            share_one_bytes().into(),
            ctx.app.clone(),
            on_export,
        );
    }
    if let Some(answer) = ctx.texts.first() {
        return Step::Done(if let Some(name) = answer.strip_prefix("saved ") {
            Verdict::Pass(format!("the system's save dialog answered after the write: saved as {name} ({} bytes)", share_one_bytes().len()))
        } else if answer == "cancelled" {
            Verdict::NeedsHuman("the save dialog was cancelled: choose a place".into())
        } else {
            Verdict::Fail(format!("the export answered: {answer}"))
        });
    }
    if ctx.elapsed_ms > WAIT_MS {
        return Step::Done(Verdict::NeedsHuman("nobody chose a place in the save dialog".into()));
    }
    Step::Wait
}

fn write_back(ctx: &mut Ctx<'_>) -> Step {
    if ctx.phase == 0 {
        ctx.phase = 1;
        await_feed("files.write_back");
    }
    // the open handler writes "copy <writable> <path>" for the first file of a request
    let Some(line) = ctx.texts.iter().find(|t| t.starts_with("copy ")).cloned() else {
        if ctx.elapsed_ms > WAIT_MS {
            return Step::Done(Verdict::NeedsHuman("no file was opened with this app: let another app open a writable file with it".into()));
        }
        return Step::Wait;
    };
    let mut parts = line.splitn(3, ' ');
    let (_, writable, path) = (parts.next(), parts.next(), parts.next().unwrap_or(""));
    if writable != "writable" {
        return Step::Done(Verdict::NeedsHuman(format!("{path} was opened read-only: open a writable file with this app")));
    }
    let documents = ctx.info.documents();
    let Some(origin) = documents.origin(path).into_option() else {
        return Step::Done(Verdict::Fail(format!("{path} was opened with this app but has no origin to save back to")));
    };
    let Ok(mut text) = std::fs::read_to_string(path) else {
        return Step::Done(Verdict::Fail(format!("{path} cannot be read")));
    };
    text.push_str("\nedited by AzSelfTest: files.write_back\n");
    if let Err(e) = std::fs::write(path, &text) {
        return Step::Done(Verdict::Fail(format!("{path} cannot be edited: {e}")));
    }
    Step::Done(match documents.write_back(path).into_result() {
        Ok(entry) => Verdict::Pass(format!(
            "the working copy was saved back to {} ({} bytes now)",
            origin.uri.as_str(),
            entry.size.into_option().unwrap_or(0)
        )),
        Err(e) => Verdict::Fail(said("write_back", &e)),
    })
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
    /// The first file that has a working copy and whether it may be saved back: `files.write_back` takes it from here.
    pub first_copy: Option<(String, bool)>,
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
    let mut first_copy: Option<(String, bool)> = None;
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
                first_copy = path.clone().map(|p| (p, item.writable));
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
        first_copy,
    }
}

#[cfg(test)]
#[path = "files_test.rs"]
mod files_test;
