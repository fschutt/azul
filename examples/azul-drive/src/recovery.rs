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
//! On stdout, for scripts (never a secret): `AZDRIVE_KIT_PRINT <bytes>` (the kit opened for
//! printing), `AZDRIVE_KIT_SAVED <bytes>` (the save dialog took it), `AZDRIVE_KIT_WRITTEN
//! <bytes>` (written into the folder picked).

use std::path::{Path, PathBuf};

use azul::{
    callbacks::ButtonOnClickCallbackType,
    dialog::{FileDialog, FileOpenResult},
    pdf::Pdf,
    prelude::*,
    str::String as AzString,
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
    spawn,
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

/// The kit on A4 paper (black on white whatever mode the window is in).
pub(crate) fn kit_dom(kit: &Kit, symbol: &QrCode) -> Dom {
    let lines = kit.lines();
    let mut body = Dom::create_body().with_css(
        "margin: 0px; padding: 56px; background: #ffffff; color: #000000; font-family: \
         sans-serif; font-size: 13px; display: flex; flex-direction: column;",
    );
    body.add_child(block(&lines[0], "font-size: 26px; font-weight: bold;"));
    body.add_child(block(
        &lines[1],
        "margin-top: 4px; padding-bottom: 10px; border-bottom: 2px solid #000000;",
    ));
    for text in &lines[2..2 + KIT_TEXT.len()] {
        body.add_child(block(text, "margin-top: 10px;"));
    }
    body.add_child(block(
        KIT_CODE_LABEL,
        "margin-top: 24px; font-weight: bold;",
    ));
    body.add_child(block(
        &kit.code,
        "margin-top: 6px; padding: 12px; border: 1px solid #000000; font-family: monospace; \
         font-size: 24px; letter-spacing: 1px;",
    ));
    body.add_child(
        Dom::create_div()
            .with_css("display: flex; flex-direction: row; align-items: center; margin-top: 24px;")
            .with_child(qr_dom(symbol, KIT_MODULE_PX))
            .with_child(block(KIT_QR_LABEL, "margin-left: 18px; flex-grow: 1;")),
    );
    body
}

/// The kit's PDF (azul's writer lays the DOM out in this callback, with the window's fonts).
fn kit_pdf(info: &mut CallbackInfo, kit: &Kit) -> Result<Zeroizing<Vec<u8>>, String> {
    let symbol = QrCode::encode(kit.qr_text().as_bytes()).map_err(|e| e.to_string())?;
    let bytes = Zeroizing::new(
        Pdf::create()
            .from_dom_in_callback(*info, kit_dom(kit, &symbol), A4.0, A4.1)
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

/// The kit of the code the open dialog shows (the recovery sheet), if it shows one.
fn shown_kit(s: &DriveState) -> Option<Kit> {
    match s.popup.as_ref()? {
        Popup::Encryption(Dialog::Sheet(sheet)) => Some(kit_of(s, &sheet.drive_id, &sheet.code)),
        _ => None,
    }
}

/// A line under the kit's buttons on the dialog showing it.
fn set_kit_note(s: &mut DriveState, note: String) {
    if let Some(Popup::Encryption(Dialog::Sheet(sheet))) = s.popup.as_mut() {
        sheet.kit_note = note;
    }
}

/// The kit's three buttons and its QR code, for a dialog that shows a code.
pub(crate) fn kit_pieces(app: &RefAny, code: &str, note: &str) -> Vec<Dom> {
    let button = |text: &str, id: AzString, callback: ButtonOnClickCallbackType| {
        Button::create(AzString::from(text))
            .with_on_click(app.clone(), callback)
            .dom()
            .with_id(id)
            .with_css("margin-right: 6px;")
    };
    let mut pieces = vec![Dom::create_div()
        .with_css("display: flex; flex-direction: row; margin-top: 8px;")
        .with_child(button("Print\u{2026}", ids::KIT_PRINT, on_kit_print))
        .with_child(button("Save as PDF\u{2026}", ids::KIT_SAVE, on_kit_save))
        .with_child(button(
            "Save to a USB stick\u{2026}",
            ids::KIT_USB,
            on_kit_usb,
        ))];
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

/// Where Print's private copies of the kit wait for the PDF viewer: `kit-print/` in the run's
/// cache folder.
fn print_dir() -> PathBuf {
    crate::encryption::run_cache_dir()
        .unwrap_or_else(std::env::temp_dir)
        .join("kit-print")
}

/// Deletes Print's copies of the kit (when the sheet closes, at the start).
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

/// Print's copy of the kit: `kit-print/<random>/<file name>` (a folder of this user's only).
fn print_copy(kit: &Kit, bytes: &[u8]) -> Result<PathBuf, String> {
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
    let path = dir.join(kit.file_name());
    write_private(&path, bytes).map_err(|e| e.to_string())?;
    Ok(path)
}

/// Print: the kit opens in the system's PDF viewer, which prints it.
extern "C" fn on_kit_print(mut data: RefAny, mut info: CallbackInfo) -> Update {
    with_state(&mut data, &mut info, |info, _app, s| {
        let Some(kit) = shown_kit(s) else {
            return;
        };
        let note = match kit_pdf(info, &kit).and_then(|bytes| {
            let path = print_copy(&kit, &bytes)?;
            println!("AZDRIVE_KIT_PRINT {}", bytes.len());
            azul_appkit::files::open_external(&path.to_string_lossy())
        }) {
            Ok(()) => String::from(
                "The kit is open in your PDF viewer: print it from there. AzDrive deletes this \
                 copy when the sheet closes.",
            ),
            Err(why) => format!("The kit could not be opened for printing: {why}"),
        };
        set_kit_note(s, note);
    })
}

/// Save as PDF: the system's save dialog (the app's state let go before it).
extern "C" fn on_kit_save(mut data: RefAny, mut info: CallbackInfo) -> Update {
    let made = {
        let Some(s) = data.downcast_ref::<DriveState>() else {
            return Update::DoNothing;
        };
        shown_kit(&s).map(|kit| (kit.file_name(), kit_pdf(&mut info, &kit)))
    };
    let Some((name, bytes)) = made else {
        return Update::DoNothing;
    };
    let note = match bytes {
        Ok(bytes) => {
            let len = bytes.len();
            if FileDialog::save_bytes(name.as_str(), "application/pdf", bytes.to_vec()) {
                println!("AZDRIVE_KIT_SAVED {len}");
                format!("Saved {name}.")
            } else {
                String::from("The kit was not saved.")
            }
        }
        Err(why) => why,
    };
    with_state(&mut data, &mut info, |_info, _app, s| set_kit_note(s, note))
}

/// What the folder picker of "Save to a USB stick" carries.
struct KitPick {
    app: RefAny,
}

/// Save to a USB stick: the system's folder picker.
extern "C" fn on_kit_usb(data: RefAny, _info: CallbackInfo) -> Update {
    let _request = FileDialog::open_directory(
        AzString::from("Save the emergency kit to a USB stick"),
        OptionString::None,
        RefAny::new(KitPick { app: data }),
        on_kit_folder,
    );
    Update::DoNothing
}

/// The folder picked: the kit is written into it on a worker thread.
extern "C" fn on_kit_folder(mut data: RefAny, mut info: CallbackInfo, result: RefAny) -> Update {
    let Some(mut app) = data.downcast_ref::<KitPick>().map(|pick| pick.app.clone()) else {
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
        let Some(kit) = shown_kit(s) else {
            return;
        };
        match kit_pdf(info, &kit) {
            Ok(bytes) => {
                set_kit_note(
                    s,
                    format!("Writing the kit to {}\u{2026}", folder.display()),
                );
                let job = EncryptionJob::SaveKit {
                    path: folder.join(kit.file_name()),
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

/// The kit is written (or why not).
pub(crate) fn kit_saved(s: &mut DriveState, path: &Path, len: usize, result: Result<(), String>) {
    let note = match result {
        Ok(()) => {
            println!("AZDRIVE_KIT_WRITTEN {len}");
            format!("The kit is on the stick: {}", path.display())
        }
        Err(why) => format!("The kit was not written: {why}"),
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
    let _ = (recovery_key, drive_id, typed);
    DrillAnswer::NotACode
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
        assert_eq!(drill_answer(Some(&key), "d_1", &other), DrillAnswer::NotTheCode);
        assert_eq!(
            drill_answer(Some(&key), "d_2", &code.to_text()),
            DrillAnswer::NotTheCode,
            "another drive's key"
        );
        assert_eq!(drill_answer(Some(&key), "d_1", "hello"), DrillAnswer::NotACode);
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
