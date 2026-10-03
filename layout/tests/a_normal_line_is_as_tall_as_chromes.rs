//! A `line-height: normal` line is as tall as Chrome's (MAILENG6 items 3 + 7).
//!
//! Chrome rounds a face's ascent, descent and line gap to whole pixels at
//! the font size before it adds them up (Blink `FontMetrics::
//! AscentDescentWithHacks` + `SimpleFontData::PlatformInit`:
//! `round(A) + round(D) + round(G)`): a 16px Arial line is 14 + 3 + 1 = 18px.
//! Azul added the unrounded values (18.4px), so every `normal` line of a
//! mail drifted 0.4px against Chrome - 4px (the corpus tool's tolerance)
//! every ten lines, the largest drift left in the Gmail / Apple Mail /
//! Postmark mails.
//!
//! On macOS Blink then adds `floor((A + D) * 0.15 + 0.5)` to the ROUNDED
//! ascent of exactly the families Times, Helvetica and Courier (the Apple
//! faces whose hhea metrics are tighter than the Microsoft fonts the web was
//! made with). Done in font units at parse time (MAILHTML), the boost
//! rounded differently: 16px Helvetica came out 18.4px, Chrome 18.
//!
//! Every expected number below was measured in headless Chrome 154 on macOS
//! (one-line `<div>`s, `font: <size>px <family>`, 11 families x 13 sizes;
//! the rounding model above reproduced all 143 of them). The first test is
//! font-independent (the faces' hhea values are written out); the second
//! parses Apple's Times / Helvetica / Courier where the machine has them.
//!
//! Not compiled by the author (house rule); RED before the fix.

use azul_layout::{
    font::parsed::ParsedFont,
    text3::cache::{LayoutFontMetrics, LineHeight},
};

/// hhea `ascender`, `descender` (negative), `lineGap` at `upem`.
fn hhea(ascent: f32, descent: f32, line_gap: f32, upem: u16) -> LayoutFontMetrics {
    LayoutFontMetrics {
        ascent,
        descent,
        line_gap,
        units_per_em: upem,
        x_height: None,
        cap_height: None,
        browser_ascent_boost: false,
    }
}

/// `(font size px, Chrome's line height px)`.
const SIZES: [f32; 13] = [
    10.0, 11.0, 12.0, 13.0, 14.0, 15.0, 16.0, 18.0, 20.0, 22.0, 24.0, 28.0, 32.0,
];

fn check(family: &str, m: &LayoutFontMetrics, chrome: [f32; 13]) {
    for (size, want) in SIZES.iter().zip(chrome) {
        let got = LineHeight::Normal.resolve_with_metrics(*size, m);
        assert!(
            (got - want).abs() < 0.01,
            "{family} {size}px: Chrome's `line-height: normal` is {want}px, azul {got}px"
        );
    }
}

#[test]
fn a_normal_line_is_the_sum_of_the_rounded_ascent_descent_and_line_gap() {
    check(
        "Arial",
        &hhea(1854.0, -434.0, 67.0, 2048),
        [
            11., 12., 14., 15., 16., 17., 18., 21., 23., 26., 28., 32., 37.,
        ],
    );
    check(
        "Times New Roman",
        &hhea(1825.0, -443.0, 87.0, 2048),
        [
            11., 12., 15., 16., 16., 17., 18., 21., 23., 26., 27., 32., 37.,
        ],
    );
    check(
        "Georgia",
        &hhea(1878.0, -449.0, 0.0, 2048),
        [
            11., 12., 14., 15., 16., 17., 19., 21., 22., 25., 27., 32., 36.,
        ],
    );
    check(
        "Verdana",
        &hhea(2059.0, -430.0, 0.0, 2048),
        [
            12., 13., 15., 16., 17., 18., 19., 22., 24., 27., 29., 34., 39.,
        ],
    );
    check(
        "Courier New",
        &hhea(1705.0, -615.0, 0.0, 2048),
        [
            11., 12., 14., 15., 16., 17., 18., 20., 23., 25., 27., 31., 37.,
        ],
    );
}

/// The metrics a system face lays text out with, where the machine has it.
fn system_face(path: &str) -> Option<LayoutFontMetrics> {
    let bytes = std::fs::read(path).ok()?;
    let mut warnings = Vec::new();
    Some(ParsedFont::from_bytes(&bytes, 0, &mut warnings)?.font_metrics)
}

#[test]
fn apples_times_helvetica_and_courier_lines_are_as_tall_as_chromes() {
    // Elsewhere there is no such face to parse, and nothing to check.
    if !cfg!(target_os = "macos") {
        return;
    }
    for (family, path, chrome) in [
        (
            "Helvetica",
            "/System/Library/Fonts/Helvetica.ttc",
            [
                12., 13., 14., 15., 16., 17., 18., 21., 23., 25., 28., 32., 37.,
            ],
        ),
        (
            "Times",
            "/System/Library/Fonts/Times.ttc",
            [
                13., 13., 14., 15., 17., 17., 18., 22., 23., 26., 28., 32., 37.,
            ],
        ),
        (
            "Courier",
            "/System/Library/Fonts/Courier.ttc",
            [
                12., 13., 14., 15., 16., 17., 18., 21., 23., 25., 28., 32., 37.,
            ],
        ),
    ] {
        let Some(m) = system_face(path) else {
            continue;
        };
        check(family, &m, chrome);
    }
    // Helvetica Neue is not one of the three: its own metrics, rounded.
    if let Some(m) = system_face("/System/Library/Fonts/HelveticaNeue.ttc") {
        let got = LineHeight::Normal.resolve_with_metrics(16.0, &m);
        // round(15.23) + round(3.41) + round(0.45) = 15 + 3 + 0
        assert!(
            (got - 18.0).abs() < 0.01,
            "Helvetica Neue 16px: Chrome 18px, azul {got}px"
        );
    }
}
