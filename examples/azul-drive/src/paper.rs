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
};

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
    PRINT_ROOT
        .get()
        .cloned()
        .flatten()
        .unwrap_or_else(std::env::temp_dir)
        .join("kit-print")
}

/// Deletes Print's copies (when the sheet closes, at the start).
pub(crate) fn forget_print_copies() {
    let _ = std::fs::remove_dir_all(print_dir());
}

/// `bytes` into `path`, readable by this user only where the file system knows owners.
pub(crate) fn write_private(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
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
pub(crate) fn print_copy(file_name: &str, bytes: &[u8]) -> Result<PathBuf, String> {
    // A folder of its own per copy (its name need not be secret: the folder is this user's).
    let folder = format!("{:016x}", azul_storage::ids::random_seed());
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
