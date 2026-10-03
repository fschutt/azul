//! `system-ui` is the font the OS draws its own UI in (user, SYSUI8: "system-ui
//! should rather match the font of the normal system UI ... the fonts in the
//! System Settings panel"). Chrome 154 is the reference.
//!
//! On macOS that is San Francisco: ONE variable font file,
//! `/System/Library/Fonts/SFNS.ttf` ("System Font"), with an optical-size axis
//! (`opsz` 17-96, default 28) and an AAT `trak` table. CoreText's UI font - and
//! Chrome's `system-ui`, which measures the same to 1/64 px - draws it at
//! `opsz` = the font size (clamped to the axis, so every size up to 17px is
//! "SF Pro Text") and adds the `trak` tracking for that size to every glyph.
//! azul drew the file's DEFAULT instance (opsz 28, the "SF Pro Display"
//! spacing) with no tracking: "Hello world agenda" at 16px was 128.29px wide,
//! Chrome's is 139.16 (MAILREF8's probe; scripts/SYSUI8.PROGRESS.md).
//!
//! Chrome 154, headless, "Hello world agenda" in a `display: inline-block`
//! span (the probe: target/sysui8/chrome_probe.py in the SYSUI8 worktree):
//!
//! | family     | size | weight | Chrome   | CoreText | azul before |
//! |------------|------|--------|----------|----------|-------------|
//! | system-ui  | 13px | 400    | 116.2656 | 116.257  | 104.24      |
//! | system-ui  | 16px | 400    | 139.1562 | 139.148  | 128.29      |
//! | system-ui  | 20px | 400    | 168.0156 | 168.011  | 160.36      |
//! | system-ui  | 28px | 400    | 231.4062 | 231.396  | 224.51      |
//! | system-ui  | 40px | 400    | 327.4062 | 327.402  | 320.72      |
//! | system-ui  | 16px | 700    | 148.6250 | 148.602  |             |
//! | BlinkMacSystemFont | all |  | = system-ui                          |
//!
//! The tolerance is half a pixel: SF kerns "wo" by -40 font units at
//! `opsz` 17 through a GPOS variation delta that a baked static instance
//! does not carry (CoreText 139.148 = advances + tracking - 0.3125), so azul
//! lands 0.2-0.35px wider than Chrome below 28px and on it from 28px up.
//!
//! macOS only (the face is Apple's). Not compiled by the author (house rule);
//! RED before the fix.

#[cfg(target_os = "macos")]
use crate::table_markup::{body, near, rect};

#[cfg(target_os = "macos")]
fn has_sf() -> bool {
    let present = std::path::Path::new("/System/Library/Fonts/SFNS.ttf").exists();
    if !present {
        eprintln!("SKIP: no /System/Library/Fonts/SFNS.ttf on this host");
    }
    present
}

#[cfg(target_os = "macos")]
fn line(id: &str, family: &str, size: u32, weight: u32) -> String {
    format!(
        "<span id=\"{id}\" style=\"display: inline-block; font-family: {family}; font-size: \
         {size}px; font-weight: {weight}\">Hello world agenda</span><br/>"
    )
}

#[cfg(target_os = "macos")]
#[test]
fn system_ui_text_is_as_wide_as_in_chrome() {
    if !has_sf() {
        return;
    }
    // (id, size, weight, Chrome 154's width)
    let cases: [(&str, u32, u32, f32); 6] = [
        ("s13", 13, 400, 116.2656),
        ("s16", 16, 400, 139.1562),
        ("s20", 20, 400, 168.0156),
        ("s28", 28, 400, 231.4062),
        ("s40", 40, 400, 327.4062),
        ("b16", 16, 700, 148.6250),
    ];
    let markup: String = cases
        .iter()
        .map(|(id, size, weight, _)| line(id, "system-ui", *size, *weight))
        .collect();
    let lw = body(&markup);
    let mut wrong = Vec::new();
    for (id, size, weight, chrome) in cases {
        let got = rect(&lw, id).size.width;
        if !near(got, chrome, 0.5) {
            wrong.push(format!(
                "{size}px weight {weight}: azul {got:.3}, Chrome {chrome:.3}"
            ));
        }
    }
    assert!(
        wrong.is_empty(),
        "system-ui is San Francisco at the optical size of its font size, tracked by its \
         `trak` table, as CoreText and Chrome draw it:\n{}",
        wrong.join("\n")
    );
}

/// `BlinkMacSystemFont` (Chrome's spelling) and `-apple-system` (Safari's) are
/// the system UI font on Apple platforms, the head of every "system font
/// stack" in mail and web CSS. Chrome 154 measures `BlinkMacSystemFont`
/// exactly as `system-ui`; it no longer knows `-apple-system` (a lone one
/// falls to its default font), Safari draws it as the system font - azul
/// follows Safari there (SYSUI8 decision: the stacks always list both, and
/// the user asked for both).
#[cfg(target_os = "macos")]
#[test]
fn blink_mac_system_font_and_apple_system_are_system_ui() {
    if !has_sf() {
        return;
    }
    let lw = body(&format!(
        "{}{}{}",
        line("ui", "system-ui", 16, 400),
        line("blink", "BlinkMacSystemFont", 16, 400),
        line("apple", "-apple-system", 16, 400)
    ));
    let ui = rect(&lw, "ui").size.width;
    let blink = rect(&lw, "blink").size.width;
    let apple = rect(&lw, "apple").size.width;
    assert!(
        near(blink, ui, 0.01) && near(apple, ui, 0.01),
        "BlinkMacSystemFont {blink:.3} and -apple-system {apple:.3} draw as system-ui {ui:.3}"
    );
}

/// A document whose text only changes SIZE (a zoom, a heading that grows)
/// draws it at the new size. The window's "the font stacks did not change"
/// check hashed the families alone, so the second layout reused the first
/// one's chains: the 20px run's key (optical size 20) was never resolved and
/// its face never loaded, and the run shaped to nothing. Chrome: 116.27 at
/// 13px, 168.02 at 20px.
#[cfg(target_os = "macos")]
#[test]
fn text_that_only_changes_size_is_drawn_at_the_new_size() {
    use crate::table_markup::lay_out_in;

    if !has_sf() {
        return;
    }
    let mut lw = body(&line("t", "system-ui", 13, 400));
    let first = rect(&lw, "t").size.width;
    lay_out_in(
        &mut lw,
        &format!(
            "<html><head></head><body style=\"margin: 0\">{}</body></html>",
            line("t", "system-ui", 20, 400)
        ),
        800.0,
        600.0,
    );
    let second = rect(&lw, "t").size.width;
    assert!(
        near(first, 116.2656, 0.5),
        "13px system-ui: azul {first:.3}, Chrome 116.266"
    );
    assert!(
        near(second, 168.0156, 0.5),
        "the same text at 20px in the same window: azul {second:.3}, Chrome 168.016"
    );
}

/// The widgets' `system:ui` (azul's system font role) and CSS `system-ui` are
/// ONE face at one size: every app's UI text and a mail's `system-ui` text
/// look alike. (A pin: it holds before the fix too, both drew SFNS.ttf.)
#[cfg(target_os = "macos")]
#[test]
fn the_widgets_system_font_is_css_system_ui() {
    if !has_sf() {
        return;
    }
    let lw = body(&format!(
        "{}{}",
        line("css", "system-ui", 13, 400),
        line("widget", "system:ui", 13, 400)
    ));
    let css = rect(&lw, "css").size.width;
    let widget = rect(&lw, "widget").size.width;
    assert!(
        near(css, widget, 0.01),
        "system:ui {widget:.3} and system-ui {css:.3} are the same face"
    );
}
