//! On macOS the generic `sans-serif` is Helvetica, as in Chrome (Blink's
//! default font settings) - user ruling 2026-10-03: Chrome is the reference.
//!
//! rust-fontconfig's per-OS table lists "Helvetica Neue" first for macOS, so
//! azul drew every `sans-serif` text in Helvetica Neue: other advances
//! (Chrome 136.98 / azul 138.36 for "Hello world agenda" at 16px) and other
//! line heights at some sizes (11pt: 16px in Chrome, 17px in azul - the
//! Outlook reply's 18 drifting boxes, scripts/refci/mail_boxes.py, MAILREF8
//! group D). `system-ui` (the apps' UI text, SF) is another generic and does
//! not move.
//!
//! macOS only: elsewhere `sans-serif` is the platform's (fontconfig's
//! `<alias>` on Linux, which Chrome follows too). Chrome 154 (scripts/refci
//! probe, 16px): sans-serif 136.98 = Helvetica 136.98, Helvetica Neue 138.36,
//! system-ui 139.16. Font-independent within macOS: compared face against
//! face. Not compiled by the author (house rule); RED before the fix.

#[cfg(target_os = "macos")]
use crate::table_markup::{body, near, rect};

#[cfg(target_os = "macos")]
fn line(id: &str, family: &str) -> String {
    format!(
        "<span id=\"{id}\" style=\"display: inline-block; font-family: {family}\">Hello world \
         agenda</span><br/>"
    )
}

#[cfg(target_os = "macos")]
#[test]
fn sans_serif_text_is_set_in_helvetica() {
    let lw = body(&format!(
        "{}{}{}",
        line("sans", "sans-serif"),
        line("helvetica", "Helvetica"),
        line("neue", "'Helvetica Neue'")
    ));
    let sans = rect(&lw, "sans");
    let helvetica = rect(&lw, "helvetica");
    let neue = rect(&lw, "neue");
    assert!(
        (helvetica.size.width - neue.size.width).abs() > 0.5,
        "the two faces set this line at different widths, so the comparison below tells them \
         apart: helvetica {helvetica:?}, neue {neue:?}"
    );
    assert!(
        near(sans.size.width, helvetica.size.width, 0.01),
        "sans-serif is Helvetica, as in Chrome (136.98 / 136.98), not Helvetica Neue (138.36): \
         sans {sans:?}, helvetica {helvetica:?}"
    );
}

#[cfg(target_os = "macos")]
#[test]
fn system_ui_is_not_moved_to_helvetica() {
    let lw = body(&format!(
        "{}{}",
        line("ui", "system-ui"),
        line("helvetica", "Helvetica")
    ));
    let ui = rect(&lw, "ui");
    let helvetica = rect(&lw, "helvetica");
    assert!(
        (ui.size.width - helvetica.size.width).abs() > 0.5,
        "system-ui stays the system face (SF), the apps' UI text: ui {ui:?}, helvetica \
         {helvetica:?}"
    );
}
