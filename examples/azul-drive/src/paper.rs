//! A page of paper with a secret on it, as AzDrive prints it: a title, a line under it, the
//! text, the secret as text (monospace, in a box) and as a QR code (azul-appkit's `qr`) - laid
//! out for A4 and made into a PDF by azul's PDF writer, black on white whatever mode the window
//! is in. An encrypted drive's emergency kit and a trusted contact's share are pages
//! (`recovery`, feature `encryption`), and so are cash by post's two (`cash`): the buyer's copy
//! with the claim code, the slip posted with the cash.
//!
//! Print opens a private copy of the PDF in the system's viewer: `kit-print/<random>/<name>` in
//! the run's cache folder (readable by this user only), deleted when the dialog closes and at
//! the next start ([`forget_print_copies`]).

use std::{
    path::{Path, PathBuf},
    sync::OnceLock,
};

use azcloud_kit::Zeroizing;
use azul::{pdf::Pdf, prelude::*, str::String as AzString};
use azul_appkit::{
    l10n::{t, t_label},
    qr::QrCode,
    ui::qr_dom,
};
use azul_storage::scratch::Scratch;

/// A4 at 96 dpi in CSS px (the page AzMail prints on).
pub(crate) const A4: (f32, f32) = (794.0, 1123.0);
/// A module of a page's QR code, in px.
pub(crate) const MODULE_PX: usize = 6;

/// `text` in a block of `css`.
pub(crate) fn block(text: &str, css: &str) -> Dom {
    Dom::create_div()
        .with_css(css.to_string())
        .with_child(Dom::create_span_with_text(AzString::from(text)))
}

/// A page of paper with a secret on it: the recovery code's kit, a trusted contact's share, cash
/// by post's buyer's copy (the claim code) or its slip (the activation code).
pub(crate) struct Paper {
    pub title: String,
    pub subtitle: String,
    /// The lines of a postal address, set close together under the subtitle ("Send it to:");
    /// empty for none.
    pub address: Vec<String>,
    pub text: Vec<String>,
    /// The label over the secret, the secret as the page shows it (and its QR code holds), the
    /// words beside the QR code (keys of the resources, or plain words).
    pub label: &'static str,
    pub secret: Zeroizing<String>,
    pub qr_label: &'static str,
    pub file_name: String,
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
    if !paper.address.is_empty() {
        body.add_child(block(&t("azdrive-paper-send-to"), "margin-top: 14px; font-weight: bold;"));
        for line in &paper.address {
            body.add_child(block(line, "margin-top: 2px; font-size: 15px;"));
        }
    }
    for text in &paper.text {
        body.add_child(block(text, "margin-top: 10px;"));
    }
    body.add_child(block(&t_label(paper.label), "margin-top: 24px; font-weight: bold;"));
    body.add_child(block(
        &paper.secret,
        "margin-top: 6px; padding: 12px; border: 1px solid #000000; font-family: monospace; \
         font-size: 22px; letter-spacing: 1px;",
    ));
    body.add_child(
        Dom::create_div()
            .with_css("display: flex; flex-direction: row; align-items: center; margin-top: 24px;")
            .with_child(qr_dom(symbol, MODULE_PX))
            .with_child(block(&t_label(paper.qr_label), "margin-left: 18px; flex-grow: 1;")),
    );
    body
}

/// The page's PDF (azul's writer lays the DOM out in this callback, with the window's fonts).
pub(crate) fn paper_pdf(
    info: &mut CallbackInfo,
    paper: &Paper,
) -> Result<Zeroizing<Vec<u8>>, String> {
    let symbol = QrCode::encode(paper.secret.as_bytes()).map_err(|e| e.to_string())?;
    let bytes = Zeroizing::new(
        Pdf::create()
            .from_dom_in_callback(*info, paper_dom(paper, &symbol), A4.0, A4.1)
            .as_ref()
            .to_vec(),
    );
    if bytes.is_empty() {
        Err(t("azdrive-kit-no-pdf"))
    } else {
        Ok(bytes)
    }
}

/// The run's cache folder (`--cache-dir`, else `<cache>/AzDrive`), set once at the start: Print's
/// copies live in it.
static PRINT_ROOT: OnceLock<Option<PathBuf>> = OnceLock::new();

/// Sets the run's cache folder (the start, once) and deletes the Print copies a run before left
/// behind.
pub(crate) fn set_print_root(cache_dir: Option<PathBuf>) {
    let _ = PRINT_ROOT.set(cache_dir);
    forget_print_copies();
}

/// Where Print's private copies of a page wait for the PDF viewer: `kit-print/` in the run's
/// cache folder.
fn print_dir() -> PathBuf {
    match PRINT_ROOT.get().cloned().flatten() {
        Some(root) => root.join("kit-print"),
        // No cache folder (a `--shot` run): the copies wait in scratch, a folder of this user's
        // only - a page may carry a recovery code, and the shared temp folder is no place for it.
        None => Scratch::new("kit-print").dir().to_path_buf(),
    }
}

/// Deletes Print's copies (when the sheet closes, at the start).
pub(crate) fn forget_print_copies() {
    let _ = std::fs::remove_dir_all(print_dir());
}

/// `bytes` into `path`, readable by this user only where the file system knows owners: written
/// whole beside the name, synced and renamed over it (`durable::replace`), so a cut leaves the old
/// file or the new one and never half a recovery sheet.
pub(crate) fn write_private(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    azul_storage::durable::replace(path, bytes, azul_storage::durable::Mode::Private)
}

/// Print's copy of a page: `kit-print/<random>/<file name>` (a folder of this user's only).
pub(crate) fn print_copy(file_name: &str, bytes: &[u8]) -> Result<PathBuf, String> {
    // A folder of its own per copy (its name need not be secret: the folder is this user's).
    let folder = format!("{:016x}", azul_storage::ids::random_seed());
    if PRINT_ROOT.get().cloned().flatten().is_none() {
        // Scratch's folders are made private and checked before anything is put in them.
        Scratch::new("kit-print")
            .ensure()
            .map_err(|e| e.to_string())?;
    }
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

#[cfg(test)]
mod tests {
    use super::*;

    /// X-AR5a-23: with no cache folder (a `--shot` run) Print's copy of a page - it may carry a
    /// recovery code - waited in `<system temp>/kit-print`, a name every user can guess. It waits
    /// in a private folder of scratch now: the folders only this user can enter, the file 0600.
    #[cfg(unix)]
    #[test]
    fn x_ar5a_23_a_printed_copy_without_a_cache_folder_waits_in_private_scratch() {
        use std::os::unix::fs::PermissionsExt;
        let mode = |path: &Path| std::fs::metadata(path).unwrap().permissions().mode() & 0o777;
        let scratch = Scratch::new("kit-print");
        let path = print_copy("sheet.pdf", b"%PDF-1.4 a recovery code").expect("a copy");
        assert!(path.starts_with(scratch.dir()), "{path:?}");
        assert!(!path.starts_with(std::env::temp_dir().join("kit-print")));
        assert_eq!(mode(&path), 0o600);
        assert_eq!(mode(path.parent().unwrap()), 0o700, "the copy's own folder");
        assert_eq!(mode(scratch.dir()), 0o700, "the purpose's folder");
        assert_eq!(
            mode(scratch.dir().parent().unwrap()),
            0o700,
            "the root of scratch"
        );
        forget_print_copies();
        assert!(!path.exists(), "the copies go when the sheet closes");
    }
}
