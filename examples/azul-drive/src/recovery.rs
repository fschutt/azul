//! An encrypted drive's recovery methods in AzDrive (feature `encryption`; D51, milestone C14).
//!
//! The EMERGENCY KIT: the recovery code on paper - a short explanation, the drive's name (never
//! its id or its bucket), the code in groups and as a QR code (azul-appkit's `qr`) - made as a
//! PDF by azul's PDF writer from a DOM laid out for A4 paper. The recovery sheet offers it
//! three ways: Print (the PDF opens in the system's viewer from a private copy in the run's
//! cache folder, deleted when the sheet closes and at the next start), Save as PDF (the
//! system's save dialog) and Save to a USB stick (a folder picked, the PDF written into it).
//! The sheet shows the QR code too, for a photo with a phone.
//!
//! DRILLS (the schedule is `recovery_health`'s): once a minute the recompression's timer looks
//! whether a drive's drill is due and opens "Do you still have your recovery kit?" - the code
//! typed from the kit, checked offline against the public recovery key kept for the drive (a
//! drive set up before asks its bucket's recovery wrap). "Later" moves it a week; closing it
//! waits for the next start.
//!
//! On stdout, for scripts (never a secret): `AZDRIVE_KIT_PRINT <bytes>` (the kit opened for
//! printing), `AZDRIVE_KIT_SAVED <bytes>` (the save dialog took it), `AZDRIVE_KIT_WRITTEN
//! <bytes>` (written into the folder picked), `AZDRIVE_RECOVERY_VERIFIED <drive id>` (the
//! setup's groups typed back), `AZDRIVE_DRILL_DUE|PASSED|FAILED|LATER <drive id>`.

use std::{
    path::{Path, PathBuf},
    sync::Mutex,
};

use azul::{
    callbacks::{ButtonOnClickCallbackType, TextInputOnTextInputCallbackType},
    dialog::{FileDialog, FileOpenResult},
    pdf::Pdf,
    prelude::*,
    str::String as AzString,
    widgets::ButtonType,
};
use azul_appkit::qr::QrCode;
use azul_storage::{
    config::DriveLocation,
    crypto::{keys::RecoveryCode, random_bytes, Zeroizing},
};

use crate::{
    encryption::{Dialog, EncryptionJob},
    ids,
    jobs::Job,
    recovery_health::{
        health_line, methods_list, methods_warning, state_mut, state_of, Method, MethodAction,
        RecoveryState,
    },
    save_settings, spawn,
    ui_dialogs::line,
    with_state, DriveState, Popup,
};

// ==== The emergency kit ====

/// The kit's title.
pub(crate) const KIT_TITLE: &str = "Azlin Emergency Kit";
/// What a kit calls a drive whose name would show its id or its bucket.
pub(crate) const GENERIC_NAME: &str = "your Azlin drive";
/// The kit's explanation, in its order.
pub(crate) const KIT_TEXT: [&str; 4] = [
    "This is the only key to your files. Azlin can't reset it: without this code and without \
     your devices nobody can open them, Azlin neither.",
    "Keep it apart from your computers and phones: in a drawer at home, in a safe, or on a USB \
     stick kept somewhere safe. Never type it into a website and never send it to anyone.",
    "To use it, in AzDrive: the drive's menu, then \"Unlock with the recovery code\" on a \
     computer the drive is on, or \"Lock down with the recovery code\" when your devices are \
     lost or in someone else's hands.",
    "Whoever has this page can ask for your drive. Your devices are told at once, and a \
     recovery with it waits 48 hours so that they can stop it.",
];
/// The label over the code, and the words beside its QR code.
pub(crate) const KIT_CODE_LABEL: &str = "Your recovery code";
pub(crate) const KIT_QR_LABEL: &str = "The same code as a QR code: a phone's camera reads it, \
     and AzDrive takes the text it shows as it is.";
/// A4 at 96 dpi in CSS px (the page AzMail prints on).
const A4: (f32, f32) = (794.0, 1123.0);
/// A module of the kit's QR code, and of the one on the sheet, in px.
const KIT_MODULE_PX: usize = 6;
const SHEET_MODULE_PX: usize = 4;
/// The longest drive name in a file name.
const FILE_NAME_MAX: usize = 60;

/// What the kit shows. The drive's name only: a name that is (or holds) the drive's id or its
/// bucket shows as "your Azlin drive".
pub(crate) struct Kit {
    pub drive_name: String,
    /// Whether `drive_name` is the drive's own (else [`GENERIC_NAME`]).
    pub named: bool,
    /// The recovery code as the sheet shows it. A secret: never printed.
    pub code: Zeroizing<String>,
    /// The day it was made (`2026-10-10`).
    pub made: String,
}

impl Kit {
    /// The kit of `code` for the drive called `drive_name`; `hidden` are the drive's id, its
    /// Azlin id and its bucket, which no kit shows.
    pub(crate) fn new(
        drive_name: &str,
        hidden: &[&str],
        code: Zeroizing<String>,
        made: String,
    ) -> Kit {
        let name = drive_name.trim();
        let lower = name.to_lowercase();
        let shows_hidden = hidden
            .iter()
            .map(|h| h.trim().to_lowercase())
            .any(|h| !h.is_empty() && lower.contains(&h));
        let named = !name.is_empty() && !shows_hidden;
        Kit {
            drive_name: if named {
                name.to_string()
            } else {
                String::from(GENERIC_NAME)
            },
            named,
            code,
            made,
        }
    }

    /// The kit's file name: `Azlin Emergency Kit - <name>.pdf`, the name's letters and digits
    /// with `-` between words (`Azlin Emergency Kit.pdf` when nothing of it is left).
    pub(crate) fn file_name(&self) -> String {
        let mut name = String::new();
        if self.named {
            for c in self.drive_name.chars() {
                if c.is_ascii_alphanumeric() || c == '_' {
                    name.push(c);
                } else if !name.is_empty() && !name.ends_with('-') {
                    name.push('-');
                }
            }
        }
        let name: String = name
            .trim_end_matches('-')
            .chars()
            .take(FILE_NAME_MAX)
            .collect();
        let name = name.trim_end_matches('-');
        if name.is_empty() {
            format!("{KIT_TITLE}.pdf")
        } else {
            format!("{KIT_TITLE} - {name}.pdf")
        }
    }

    /// Every line of the kit's text but the code: the title, the drive and the day, the
    /// explanation, the code's label, the QR code's words.
    pub(crate) fn lines(&self) -> Vec<String> {
        let drive = if self.named {
            format!(
                "For the drive \"{}\", made on {}.",
                self.drive_name, self.made
            )
        } else {
            format!("For {}, made on {}.", self.drive_name, self.made)
        };
        let mut lines = vec![String::from(KIT_TITLE), drive];
        lines.extend(KIT_TEXT.iter().map(|text| (*text).to_string()));
        lines.push(String::from(KIT_CODE_LABEL));
        lines.push(String::from(KIT_QR_LABEL));
        lines
    }

    /// The text its QR code holds: the code as the sheet shows it, which "Unlock with the
    /// recovery code" takes as it is.
    pub(crate) fn qr_text(&self) -> Zeroizing<String> {
        Zeroizing::new(self.code.as_str().to_string())
    }
}

/// The kit of `code` for the drive `drive_id` (AzDrive's id of it): its name from the drives
/// list, its ids and bucket kept off the paper.
pub(crate) fn kit_of(s: &DriveState, drive_id: &str, code: &str) -> Kit {
    let mut hidden: Vec<String> = vec![drive_id.to_string()];
    let mut name = String::new();
    if let Some(index) = s.slot_index(drive_id) {
        let entry = &s.slots[index].entry;
        name = entry.name.clone();
        if let Some((azlin_id, _)) = entry.azlin() {
            hidden.push(azlin_id.to_string());
        }
        if let DriveLocation::S3 { bucket, .. } = &entry.location {
            hidden.push(bucket.clone());
        }
    }
    let hidden: Vec<&str> = hidden.iter().map(String::as_str).collect();
    let made = chrono::Local::now().format("%Y-%m-%d").to_string();
    Kit::new(&name, &hidden, Zeroizing::new(code.to_string()), made)
}

/// `text` in a block of `css`.
fn block(text: &str, css: &str) -> Dom {
    Dom::create_div()
        .with_css(css.to_string())
        .with_child(Dom::create_span_with_text(AzString::from(text)))
}

/// A QR symbol as boxes, `module` px a module, with its quiet zone (four light modules) on
/// white: a row of dark runs and the light gaps between them for each row of modules.
pub(crate) fn qr_dom(symbol: &QrCode, module: usize) -> Dom {
    let side = symbol.size() * module;
    let quiet = 4 * module;
    let mut rows = Dom::create_div().with_css(format!(
        "display: flex; flex-direction: column; flex-shrink: 0; width: {side}px; padding: \
         {quiet}px; background: #ffffff;"
    ));
    for y in 0..symbol.size() {
        let mut row = Dom::create_div().with_css(format!(
            "display: flex; flex-direction: row; flex-shrink: 0; width: {side}px; height: \
             {module}px;"
        ));
        let mut at = 0;
        for (start, len) in symbol.dark_runs(y) {
            if start > at {
                row.add_child(Dom::create_div().with_css(format!(
                    "flex-shrink: 0; width: {}px; height: {module}px;",
                    (start - at) * module
                )));
            }
            row.add_child(Dom::create_div().with_css(format!(
                "flex-shrink: 0; width: {}px; height: {module}px; background: #000000;",
                len * module
            )));
            at = start + len;
        }
        rows.add_child(row);
    }
    rows
}

/// A page of paper with a secret on it: the recovery code's kit, or a trusted contact's share.
pub(crate) struct Paper {
    pub title: String,
    pub subtitle: String,
    pub text: Vec<String>,
    /// The label over the secret, the secret as the page shows it (and its QR code holds), the
    /// words beside the QR code.
    pub label: &'static str,
    pub secret: Zeroizing<String>,
    pub qr_label: &'static str,
    pub file_name: String,
}

impl Kit {
    /// The kit as a page.
    pub(crate) fn paper(&self) -> Paper {
        let lines = self.lines();
        Paper {
            title: lines[0].clone(),
            subtitle: lines[1].clone(),
            text: lines[2..2 + KIT_TEXT.len()].to_vec(),
            label: KIT_CODE_LABEL,
            secret: self.qr_text(),
            qr_label: KIT_QR_LABEL,
            file_name: self.file_name(),
        }
    }
}

/// A page on A4 paper (black on white whatever mode the window is in).
pub(crate) fn paper_dom(paper: &Paper, symbol: &QrCode) -> Dom {
    let mut body = Dom::create_body().with_css(
        "margin: 0px; padding: 56px; background: #ffffff; color: #000000; font-family: \
         sans-serif; font-size: 13px; display: flex; flex-direction: column;",
    );
    body.add_child(block(&paper.title, "font-size: 26px; font-weight: bold;"));
    body.add_child(block(
        &paper.subtitle,
        "margin-top: 4px; padding-bottom: 10px; border-bottom: 2px solid #000000;",
    ));
    for text in &paper.text {
        body.add_child(block(text, "margin-top: 10px;"));
    }
    body.add_child(block(paper.label, "margin-top: 24px; font-weight: bold;"));
    body.add_child(block(
        &paper.secret,
        "margin-top: 6px; padding: 12px; border: 1px solid #000000; font-family: monospace; \
         font-size: 22px; letter-spacing: 1px;",
    ));
    body.add_child(
        Dom::create_div()
            .with_css("display: flex; flex-direction: row; align-items: center; margin-top: 24px;")
            .with_child(qr_dom(symbol, KIT_MODULE_PX))
            .with_child(block(paper.qr_label, "margin-left: 18px; flex-grow: 1;")),
    );
    body
}

/// The page's PDF (azul's writer lays the DOM out in this callback, with the window's fonts).
fn paper_pdf(info: &mut CallbackInfo, paper: &Paper) -> Result<Zeroizing<Vec<u8>>, String> {
    let symbol = QrCode::encode(paper.secret.as_bytes()).map_err(|e| e.to_string())?;
    let bytes = Zeroizing::new(
        Pdf::create()
            .from_dom_in_callback(*info, paper_dom(paper, &symbol), A4.0, A4.1)
            .as_ref()
            .to_vec(),
    );
    if bytes.is_empty() {
        Err(String::from(
            "azul's PDF writer made no file (a build without its `pdf` feature?).",
        ))
    } else {
        Ok(bytes)
    }
}

/// Which page a paper button makes.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum Which {
    /// The code the dialog shows: the recovery sheet's, or the one trusted contacts gave back.
    Shown,
    /// A trusted contact's printed share on the "shares made" page (its row).
    Share(usize),
}

/// What a paper button carries.
struct PaperRef {
    app: RefAny,
    which: Which,
}

fn paper_ref(data: &mut RefAny) -> Option<(RefAny, Which)> {
    data.downcast_ref::<PaperRef>()
        .map(|paper| (paper.app.clone(), paper.which))
}

/// The page `which` of the dialog showing, if it shows one.
fn shown_paper(s: &DriveState, which: Which) -> Option<Paper> {
    match (s.popup.as_ref()?, which) {
        (Popup::Encryption(Dialog::Sheet(sheet)), Which::Shown) => {
            Some(kit_of(s, &sheet.drive_id, &sheet.code).paper())
        }
        (Popup::Encryption(Dialog::Contacts(page)), which) => {
            crate::recovery_contacts::paper_of(s, page, which)
        }
        _ => None,
    }
}

/// A line under the paper buttons of the dialog showing them.
fn set_kit_note(s: &mut DriveState, note: String) {
    match s.popup.as_mut() {
        Some(Popup::Encryption(Dialog::Sheet(sheet))) => sheet.kit_note = note,
        Some(Popup::Encryption(Dialog::Contacts(page))) => {
            crate::recovery_contacts::set_note(page, note);
        }
        _ => {}
    }
}

/// The three paper buttons (Print, Save as PDF, Save to a USB stick) of page `which`: the
/// kit's ids, or a printed share's by its row.
pub(crate) fn paper_buttons(app: &RefAny, which: Which) -> Dom {
    let id = |kit: AzString, what: &str| match which {
        Which::Shown => kit,
        Which::Share(row) => ids::share_paper(row, what),
    };
    let button = |text: &str, id: AzString, callback: ButtonOnClickCallbackType| {
        Button::create(AzString::from(text))
            .with_on_click(
                RefAny::new(PaperRef {
                    app: app.clone(),
                    which,
                }),
                callback,
            )
            .dom()
            .with_id(id)
            .with_css("margin-right: 6px;")
    };
    Dom::create_div()
        .with_css("display: flex; flex-direction: row; margin-top: 8px;")
        .with_child(button(
            "Print\u{2026}",
            id(ids::KIT_PRINT, "print"),
            on_kit_print,
        ))
        .with_child(button(
            "Save as PDF\u{2026}",
            id(ids::KIT_SAVE, "save"),
            on_kit_save,
        ))
        .with_child(button(
            "Save to a USB stick\u{2026}",
            id(ids::KIT_USB, "usb"),
            on_kit_usb,
        ))
}

/// The kit's three buttons and its QR code, for a dialog that shows a code.
pub(crate) fn kit_pieces(app: &RefAny, code: &str, note: &str) -> Vec<Dom> {
    let mut pieces = vec![paper_buttons(app, Which::Shown)];
    if !note.is_empty() {
        pieces.push(line(note).with_css("font-size: 12px; opacity: 0.75;"));
    }
    if let Ok(symbol) = QrCode::encode(code.as_bytes()) {
        pieces.push(
            Dom::create_div()
                .with_css(
                    "display: flex; flex-direction: row; align-items: center; margin-top: 8px;",
                )
                .with_child(qr_dom(&symbol, SHEET_MODULE_PX))
                .with_child(
                    line("Or take a photo of this QR code with your phone and keep it offline.")
                        .with_css("margin-left: 12px; font-size: 12px;"),
                ),
        );
    }
    pieces
}

// ==== Print, Save as PDF, Save to a USB stick ====

/// Where Print's private copies of a page wait for the PDF viewer: `kit-print/` in the run's
/// cache folder.
fn print_dir() -> PathBuf {
    crate::encryption::run_cache_dir()
        .unwrap_or_else(std::env::temp_dir)
        .join("kit-print")
}

/// Deletes Print's copies (when the sheet closes, at the start).
pub(crate) fn forget_print_copies() {
    let _ = std::fs::remove_dir_all(print_dir());
}

/// `bytes` into `path`, readable by this user only where the file system knows owners.
fn write_private(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    let mut options = std::fs::OpenOptions::new();
    options.write(true).create(true).truncate(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = options.open(path)?;
    std::io::Write::write_all(&mut file, bytes)?;
    file.sync_all()
}

/// Print's copy of a page: `kit-print/<random>/<file name>` (a folder of this user's only).
fn print_copy(file_name: &str, bytes: &[u8]) -> Result<PathBuf, String> {
    let mut random = [0u8; 8];
    random_bytes(&mut random).map_err(|e| e.to_string())?;
    let folder: String = random.iter().map(|b| format!("{b:02x}")).collect();
    let dir = print_dir().join(folder);
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let private = std::fs::Permissions::from_mode(0o700);
        let _ = std::fs::set_permissions(print_dir(), private.clone());
        let _ = std::fs::set_permissions(&dir, private);
    }
    let path = dir.join(file_name);
    write_private(&path, bytes).map_err(|e| e.to_string())?;
    Ok(path)
}

/// Print: the page opens in the system's PDF viewer, which prints it.
extern "C" fn on_kit_print(mut data: RefAny, mut info: CallbackInfo) -> Update {
    let Some((mut app, which)) = paper_ref(&mut data) else {
        return Update::DoNothing;
    };
    with_state(&mut app, &mut info, |info, _app, s| {
        let Some(paper) = shown_paper(s, which) else {
            return;
        };
        let note = match paper_pdf(info, &paper).and_then(|bytes| {
            let path = print_copy(&paper.file_name, &bytes)?;
            println!("AZDRIVE_KIT_PRINT {}", bytes.len());
            azul_appkit::files::open_external(&path.to_string_lossy())
        }) {
            Ok(()) => String::from(
                "It is open in your PDF viewer: print it from there. AzDrive deletes this copy \
                 when the dialog closes.",
            ),
            Err(why) => format!("It could not be opened for printing: {why}"),
        };
        if which != Which::Shown {
            crate::recovery_contacts::handed(s, which);
        }
        set_kit_note(s, note);
    })
}

/// Save as PDF: the system's save dialog (the app's state let go before it).
extern "C" fn on_kit_save(mut data: RefAny, mut info: CallbackInfo) -> Update {
    let Some((mut app, which)) = paper_ref(&mut data) else {
        return Update::DoNothing;
    };
    let made = {
        let Some(s) = app.downcast_ref::<DriveState>() else {
            return Update::DoNothing;
        };
        shown_paper(&s, which).map(|paper| (paper.file_name.clone(), paper_pdf(&mut info, &paper)))
    };
    let Some((name, bytes)) = made else {
        return Update::DoNothing;
    };
    let (note, saved) = match bytes {
        Ok(bytes) => {
            let len = bytes.len();
            if FileDialog::save_bytes(name.as_str(), "application/pdf", bytes.to_vec()) {
                println!("AZDRIVE_KIT_SAVED {len}");
                (format!("Saved {name}."), true)
            } else {
                (String::from("It was not saved."), false)
            }
        }
        Err(why) => (why, false),
    };
    with_state(&mut app, &mut info, |_info, _app, s| {
        if saved && which != Which::Shown {
            crate::recovery_contacts::handed(s, which);
        }
        set_kit_note(s, note);
    })
}

/// Save to a USB stick: the system's folder picker.
extern "C" fn on_kit_usb(mut data: RefAny, _info: CallbackInfo) -> Update {
    let Some((app, which)) = paper_ref(&mut data) else {
        return Update::DoNothing;
    };
    let _request = FileDialog::open_directory(
        AzString::from("Save it to a USB stick"),
        OptionString::None,
        RefAny::new(PaperRef { app, which }),
        on_kit_folder,
    );
    Update::DoNothing
}

/// The folder picked: the page is written into it on a worker thread.
extern "C" fn on_kit_folder(mut data: RefAny, mut info: CallbackInfo, result: RefAny) -> Update {
    let Some((mut app, which)) = paper_ref(&mut data) else {
        return Update::DoNothing;
    };
    let Some(picked) = FileOpenResult::downcast(result).into_option() else {
        return Update::DoNothing;
    };
    let Some(path) = picked.path.into_option() else {
        return Update::DoNothing; // cancelled
    };
    let folder = PathBuf::from(path.inner.as_str());
    with_state(&mut app, &mut info, |info, app, s| {
        let Some(paper) = shown_paper(s, which) else {
            return;
        };
        match paper_pdf(info, &paper) {
            Ok(bytes) => {
                set_kit_note(s, format!("Writing it to {}\u{2026}", folder.display()));
                if which != Which::Shown {
                    crate::recovery_contacts::handed(s, which);
                }
                let job = EncryptionJob::SaveKit {
                    path: folder.join(&paper.file_name),
                    bytes,
                };
                spawn(info, app, s, Job::Encryption(job));
            }
            Err(why) => set_kit_note(s, why),
        }
    })
}

/// The worker thread's part of Save to a USB stick.
pub(crate) fn save_kit(path: &Path, bytes: &[u8]) -> Result<(), String> {
    write_private(path, bytes).map_err(|e| format!("{}: {e}", path.display()))
}

/// The page is written (or why not).
pub(crate) fn kit_saved(s: &mut DriveState, path: &Path, len: usize, result: Result<(), String>) {
    let note = match result {
        Ok(()) => {
            println!("AZDRIVE_KIT_WRITTEN {len}");
            format!("It is on the stick: {}", path.display())
        }
        Err(why) => format!("It was not written: {why}"),
    };
    set_kit_note(s, note);
}

// ==== Drills ====

/// What a drill makes of the code typed.
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum DrillAnswer {
    /// The drive's code.
    Passed,
    /// A recovery code, but not the drive's.
    NotTheCode,
    /// No recovery code at all (26 letters and digits in five groups).
    NotACode,
    /// This computer knows no recovery key of the drive (a setup from before the drills): the
    /// bucket's recovery wrap is asked.
    AskTheBucket(RecoveryCode),
}

/// The drill's answer to `typed` for the drive `drive_id` whose code's public recovery key is
/// `recovery_key`: offline, without the bucket and without Argon2id's second.
pub(crate) fn drill_answer(recovery_key: Option<&str>, drive_id: &str, typed: &str) -> DrillAnswer {
    let Some(code) = RecoveryCode::parse(typed) else {
        return DrillAnswer::NotACode;
    };
    let Some(known) = recovery_key else {
        return DrillAnswer::AskTheBucket(code);
    };
    if crate::encryption::recovery_key_of(&code, drive_id).public_base64() == known.trim() {
        DrillAnswer::Passed
    } else {
        DrillAnswer::NotTheCode
    }
}

/// The drives a drill was shown for in this run (closing one counts as "Later" until the next
/// start).
static ASKED: Mutex<Vec<String>> = Mutex::new(Vec::new());

fn now() -> u64 {
    azul_storage::time::now_unix()
}

/// A new recovery code of `drive_id` (the setup, a rotation): kept with its public recovery
/// key; the drills count from it and its shares are made again.
pub(crate) fn code_made(s: &mut DriveState, drive_id: &str, code: &str) {
    let key = RecoveryCode::parse(code)
        .map(|code| crate::encryption::recovery_key_of(&code, drive_id).public_base64());
    state_mut(&mut s.settings.recovery.drives, drive_id).code_made(now(), key);
}

/// The setup's groups were typed back right.
pub(crate) fn setup_verified(s: &mut DriveState, drive_id: &str) {
    state_mut(&mut s.settings.recovery.drives, drive_id).setup_verified(now());
    println!("AZDRIVE_RECOVERY_VERIFIED {drive_id}");
}

/// Opens the drill of `drive_id` ("Test" in the methods list, or a due one).
pub(crate) fn open_drill(s: &mut DriveState, drive_id: &str) {
    if s.popup.is_none() {
        s.popups_opened += 1;
        s.popup = Some(Popup::Encryption(Dialog::Drill {
            drive_id: drive_id.to_string(),
            typed: Zeroizing::new(String::new()),
            error: String::new(),
        }));
    }
}

/// A drill due `now` opens, when no dialog shows and none was shown for that drive in this
/// run (the timer's minute); whether one opened.
pub(crate) fn drill_if_due(s: &mut DriveState, now: u64) -> bool {
    if s.popup.is_some() {
        return false;
    }
    let Ok(mut asked) = ASKED.lock() else {
        return false;
    };
    let due = s
        .settings
        .recovery
        .drives
        .iter()
        .find(|state| {
            state.drill_due(now)
                && s.slot_index(&state.drive_id).is_some()
                && !asked.contains(&state.drive_id)
        })
        .map(|state| state.drive_id.clone());
    let Some(drive_id) = due else {
        return false;
    };
    asked.push(drive_id.clone());
    drop(asked);
    println!("AZDRIVE_DRILL_DUE {drive_id}");
    open_drill(s, &drive_id);
    true
}

/// The drill's page.
pub(crate) fn drill_parts(dialog: &Dialog, s: &DriveState, app: &RefAny) -> (String, Dom) {
    let Dialog::Drill {
        drive_id, error, ..
    } = dialog
    else {
        return (String::new(), Dom::create_div());
    };
    let name = s.drive_name(&crate::browse::Place::folder(drive_id, ""));
    let mut body = Dom::create_div()
        .with_css("display: flex; flex-direction: column; min-width: 420px; max-width: 520px;")
        .with_child(line(&format!(
            "A short check that the recovery code of \"{name}\" still works: type it from your \
             emergency kit (any case, with or without dashes). It is checked on this computer \
             and kept nowhere."
        )))
        .with_child(crate::ui_dialogs::label("The recovery code"))
        .with_child(
            TextInput::create()
                .with_placeholder(AzString::from("XXXXX-XXXXX-XXXXX-XXXXX-XXXXXX"))
                .with_on_text_input(
                    app.clone(),
                    crate::encryption::on_typed as TextInputOnTextInputCallbackType,
                )
                .dom()
                .with_id(ids::DRILL_CODE),
        );
    if !error.is_empty() {
        body.add_child(line(error).with_css("color: #C42B1C;"));
    }
    let may_stop =
        state_of(&s.settings.recovery.drives, drive_id).is_some_and(RecoveryState::may_stop_drills);
    let mut row =
        vec![crate::ui_dialogs::button("Later", app, on_drill_later).with_id(ids::DRILL_LATER)];
    if may_stop {
        row.push(crate::ui_dialogs::button(
            "Stop the checks",
            app,
            on_drill_stop,
        ));
    }
    row.push(
        crate::ui_dialogs::typed_button("Check", ButtonType::Primary, app, on_drill_check)
            .with_id(ids::DRILL_CHECK),
    );
    body.add_child(crate::ui_dialogs::buttons(row));
    (String::from("Do you still have your recovery kit?"), body)
}

/// The drive of the drill showing.
fn drill_drive(s: &DriveState) -> Option<String> {
    match s.popup.as_ref()? {
        Popup::Encryption(Dialog::Drill { drive_id, .. }) => Some(drive_id.clone()),
        _ => None,
    }
}

/// The drill passed: the next one, and a word of it.
pub(crate) fn drill_passed(
    info: &mut CallbackInfo,
    app: &RefAny,
    s: &mut DriveState,
    drive_id: &str,
    recovery_key: Option<String>,
) {
    let state = state_mut(&mut s.settings.recovery.drives, drive_id);
    state.drill_passed(now());
    if state.recovery_key.is_none() {
        state.recovery_key = recovery_key;
    }
    let next = state.next_drill().map(|at| {
        let day = azul_storage::time::iso8601(at);
        day.get(..10).unwrap_or(&day).to_string()
    });
    println!("AZDRIVE_DRILL_PASSED {drive_id}");
    let name = s.drive_name(&crate::browse::Place::folder(drive_id, ""));
    s.popup = Some(Popup::Encryption(Dialog::Message {
        title: String::from("Your recovery kit works"),
        text: match next {
            Some(day) => {
                format!("That is the recovery code of \"{name}\". AzDrive asks again on {day}.")
            }
            None => format!("That is the recovery code of \"{name}\"."),
        },
    }));
    save_settings(info, app, s);
}

extern "C" fn on_drill_check(mut data: RefAny, mut info: CallbackInfo) -> Update {
    with_state(&mut data, &mut info, |info, app, s| {
        let Some(Popup::Encryption(Dialog::Drill {
            drive_id, typed, ..
        })) = s.popup.as_ref()
        else {
            return;
        };
        let drive_id = drive_id.clone();
        let known = state_of(&s.settings.recovery.drives, &drive_id)
            .and_then(|state| state.recovery_key.clone());
        let answer = drill_answer(known.as_deref(), &drive_id, typed);
        let error = match answer {
            DrillAnswer::Passed => {
                drill_passed(info, app, s, &drive_id, None);
                return;
            }
            DrillAnswer::AskTheBucket(code) => {
                crate::encryption::check_code_in_bucket(info, app, s, &drive_id, code);
                return;
            }
            DrillAnswer::NotTheCode => String::from(
                "That is not this drive's recovery code. If your kit is lost, make a new code \
                 (the drive's menu: I was hacked: new keys) and print its kit.",
            ),
            DrillAnswer::NotACode => {
                String::from("That is not a recovery code: 26 letters and digits, in five groups.")
            }
        };
        println!("AZDRIVE_DRILL_FAILED {drive_id}");
        if let Some(Popup::Encryption(Dialog::Drill { error: shown, .. })) = s.popup.as_mut() {
            *shown = error;
        }
    })
}

/// The bucket's recovery wrap answered a drill (`Ok(true)`: the code opens it).
pub(crate) fn bucket_answered(
    info: &mut CallbackInfo,
    app: &RefAny,
    s: &mut DriveState,
    drive_id: &str,
    recovery_key: String,
    result: Result<bool, String>,
) {
    match result {
        Ok(true) => drill_passed(info, app, s, drive_id, Some(recovery_key)),
        Ok(false) => {
            println!("AZDRIVE_DRILL_FAILED {drive_id}");
            s.popup = None;
            open_drill(s, drive_id);
            if let Some(Popup::Encryption(Dialog::Drill { error, .. })) = s.popup.as_mut() {
                *error = String::from("That is not this drive's recovery code.");
            }
        }
        Err(why) => {
            s.popup = Some(Popup::Encryption(Dialog::Message {
                title: String::from("The code could not be checked"),
                text: why,
            }));
        }
    }
}

extern "C" fn on_drill_later(mut data: RefAny, mut info: CallbackInfo) -> Update {
    with_state(&mut data, &mut info, |info, app, s| {
        let Some(drive_id) = drill_drive(s) else {
            return;
        };
        state_mut(&mut s.settings.recovery.drives, &drive_id).postpone(now());
        println!("AZDRIVE_DRILL_LATER {drive_id}");
        s.popup = None;
        save_settings(info, app, s);
    })
}

extern "C" fn on_drill_stop(mut data: RefAny, mut info: CallbackInfo) -> Update {
    with_state(&mut data, &mut info, |info, app, s| {
        let Some(drive_id) = drill_drive(s) else {
            return;
        };
        let stopped = state_mut(&mut s.settings.recovery.drives, &drive_id).stop_drills();
        s.popup = None;
        if stopped {
            s.info("No more checks of the recovery code: two printed shares are a second way in.");
        }
        save_settings(info, app, s);
    })
}

// ==== Options > Drives: the methods list ====

/// What a method's button carries.
struct MethodRef {
    app: RefAny,
    drive_id: String,
    method: Method,
    action: MethodAction,
}

fn method_button(app: &RefAny, drive_id: &str, method: Method, action: MethodAction) -> Dom {
    let text = match action {
        MethodAction::Test => "Test",
        MethodAction::Add => "Add\u{2026}",
        MethodAction::Remove => "Remove",
        MethodAction::CountAgain => "Count again",
    };
    Button::create(AzString::from(text))
        .with_on_click(
            RefAny::new(MethodRef {
                app: app.clone(),
                drive_id: drive_id.to_string(),
                method,
                action,
            }),
            on_method as ButtonOnClickCallbackType,
        )
        .dom()
        .with_id(ids::method_button(drive_id, method, action))
        .with_css("margin-left: 6px;")
}

/// Options > Drives' recovery sections: each encrypted drive's methods (with its Recovery
/// health and the warning below two), and the shares this computer holds for others.
pub(crate) fn options_sections(s: &DriveState, app: &RefAny) -> Vec<(String, Dom)> {
    let now = now();
    let mut drives: Vec<Dom> = Vec::new();
    for state in &s.settings.recovery.drives {
        if s.slot_index(&state.drive_id).is_none() {
            continue;
        }
        let name = s.drive_name(&crate::browse::Place::folder(&state.drive_id, ""));
        let health =
            health_line(&s.settings.recovery.drives, &state.drive_id, now).unwrap_or_default();
        let mut block = Dom::create_div()
            .with_css("display: flex; flex-direction: column; padding: 6px 0px;")
            .with_child(line(&format!("{name} - {health}")).with_css("font-weight: bold;"));
        if let Some(warning) = methods_warning(state) {
            block.add_child(
                line(warning)
                    .with_css("color: #C42B1C;")
                    .with_id(ids::method_warning(&state.drive_id)),
            );
        }
        for row in methods_list(state, now) {
            let mut item = Dom::create_div()
                .with_css(
                    "display: flex; flex-direction: row; align-items: center; margin-top: 4px;",
                )
                .with_child(
                    Dom::create_div()
                        .with_css("display: flex; flex-direction: column; flex-grow: 1;")
                        .with_child(Dom::create_span_with_text(AzString::from(
                            row.method.name(),
                        )))
                        .with_child(
                            Dom::create_span_with_text(AzString::from(row.status.as_str()))
                                .with_css("font-size: 12px; opacity: 0.75;"),
                        ),
                );
            for action in &row.actions {
                item.add_child(method_button(app, &state.drive_id, row.method, *action));
            }
            block.add_child(item);
        }
        drives.push(block);
    }
    if drives.is_empty() {
        drives.push(
            line(
                "No encrypted drive yet: an Azlin drive's menu in the source list offers \
                 \"Encrypt this drive\".",
            )
            .with_css("font-size: 12px; opacity: 0.75;"),
        );
    }
    vec![
        (
            String::from("Recovery"),
            Dom::create_div()
                .with_css("display: flex; flex-direction: column;")
                .with_children(DomVec::from(drives))
                .with_id(ids::RECOVERY_METHODS),
        ),
        (
            String::from("Shares you hold for others"),
            crate::recovery_contacts::held_section(s, app),
        ),
    ]
}

extern "C" fn on_method(mut data: RefAny, mut info: CallbackInfo) -> Update {
    let Some((mut app, drive_id, method, action)) = data
        .downcast_ref::<MethodRef>()
        .map(|m| (m.app.clone(), m.drive_id.clone(), m.method, m.action))
    else {
        return Update::DoNothing;
    };
    with_state(&mut app, &mut info, |info, app, s| match (method, action) {
        (Method::Code, MethodAction::Test) => open_drill(s, &drive_id),
        (Method::Contacts, MethodAction::Add) => crate::recovery_contacts::ask_add(s, &drive_id),
        (Method::Contacts, MethodAction::Test) => {
            crate::recovery_contacts::ask_recover(info, app, s, &drive_id, true);
        }
        (Method::Contacts, MethodAction::Remove) => {
            crate::recovery_contacts::forget(info, app, s, &drive_id);
        }
        (Method::OtherDevice, MethodAction::CountAgain) => {
            crate::encryption::count_devices(info, app, s, &drive_id);
        }
        (Method::OtherDevice, MethodAction::Add) => {
            if s.popup.is_none() {
                s.popups_opened += 1;
                s.popup = Some(Popup::Encryption(Dialog::Message {
                    title: String::from("Another device"),
                    text: String::from(
                        "A phone or a second computer that has the drive's key is a way back \
                         in when this one is lost: join it with a join code from this computer \
                         (azcloud invite), pass the code by a file or a QR code, then Count \
                         again here. It counts best as a device of another kind - a phone \
                         beside a computer - since both can be lost together.",
                    ),
                }));
            }
        }
        _ => {}
    })
}

/// The other devices were counted.
pub(crate) fn devices_counted(
    info: &mut CallbackInfo,
    app: &RefAny,
    s: &mut DriveState,
    drive_id: &str,
    result: Result<u32, String>,
) {
    match result {
        Ok(count) => {
            println!("AZDRIVE_DEVICES_COUNTED {drive_id} {count}");
            state_mut(&mut s.settings.recovery.drives, drive_id).other_devices = count;
            save_settings(info, app, s);
        }
        Err(why) => s.error(format!("The other devices were not counted: {why}")),
    }
}

#[cfg(test)]
mod tests {
    use azul_appkit::qr::QrCode;
    use azul_storage::crypto::keys::RecoveryCode;

    use super::*;

    fn code() -> Zeroizing<String> {
        RecoveryCode::from_bytes([0x5A; 16]).to_text()
    }

    #[test]
    fn the_kit_explains_names_the_drive_and_holds_the_code_as_text_and_qr() {
        let kit = Kit::new(
            "Photos",
            &["d_7k2m", "d-7k2m"],
            code(),
            String::from("2026-10-10"),
        );
        let text = kit.lines().join("\n");
        assert!(
            text.contains("This is the only key to your files"),
            "{text}"
        );
        assert!(text.contains("Azlin can't reset it"), "{text}");
        assert!(text.contains("\"Photos\""), "the drive's name: {text}");
        assert!(text.contains("2026-10-10"), "the day it was made: {text}");
        assert!(text.contains("48 hours"), "what a recovery costs: {text}");
        assert!(
            !text.contains(code().as_str()),
            "the code is not one of the lines"
        );
        assert_eq!(kit.qr_text().as_str(), code().as_str());
        assert!(
            RecoveryCode::parse(&kit.qr_text()).is_some(),
            "the QR's text unlocks"
        );
        assert_eq!(
            QrCode::encode(kit.qr_text().as_bytes()).unwrap().version(),
            3
        );
        assert!(KIT_TEXT.len() >= 3);
    }

    #[test]
    fn the_kit_never_shows_the_drives_id_or_its_bucket() {
        for name in ["d_7k2m", "Photos (d-7k2m)", "  d_7k2m  ", "D_7K2M"] {
            let kit = Kit::new(
                name,
                &["d_7k2m", "d-7k2m"],
                code(),
                String::from("2026-10-10"),
            );
            let text = format!("{}\n{}", kit.lines().join("\n"), kit.file_name());
            assert!(!text.to_lowercase().contains("7k2m"), "{name}: {text}");
            assert!(text.contains("your Azlin drive"), "{name}: {text}");
        }
    }

    #[test]
    fn a_drill_checks_the_code_typed_against_the_drives_recovery_key_offline() {
        let code = RecoveryCode::from_bytes([0x5A; 16]);
        let key = crate::encryption::recovery_key_of(&code, "d_1").public_base64();
        let typed = code.to_text().replace('-', " ").to_lowercase();
        assert_eq!(drill_answer(Some(&key), "d_1", &typed), DrillAnswer::Passed);
        let other = RecoveryCode::from_bytes([0x11; 16]).to_text();
        assert_eq!(
            drill_answer(Some(&key), "d_1", &other),
            DrillAnswer::NotTheCode
        );
        assert_eq!(
            drill_answer(Some(&key), "d_2", &code.to_text()),
            DrillAnswer::NotTheCode,
            "another drive's key"
        );
        assert_eq!(
            drill_answer(Some(&key), "d_1", "hello"),
            DrillAnswer::NotACode
        );
        assert!(matches!(
            drill_answer(None, "d_1", &code.to_text()),
            DrillAnswer::AskTheBucket(_)
        ));
    }

    #[test]
    fn the_kits_file_name_is_a_safe_name_of_the_drive() {
        let kit = Kit::new(
            "Photos/2026: ok?",
            &["d_1"],
            code(),
            String::from("2026-10-10"),
        );
        assert_eq!(kit.file_name(), "Azlin Emergency Kit - Photos-2026-ok.pdf");
        let kit = Kit::new("???", &["d_1"], code(), String::from("2026-10-10"));
        assert_eq!(kit.file_name(), "Azlin Emergency Kit.pdf");
    }
}
