//! Clipboard content types for copy/paste operations
//!
//! Contains `ClipboardContent` and `StyledTextRun`, used by clipboard and
//! changeset modules.
//!
//! **Rich-text status:** `StyledTextRun`, `StyledTextRunVec` and the
//! `ClipboardContent.styled_runs` field are FFI-exported (api.json), but the
//! rich path is only half-wired: the live clipboard producers build
//! `styled_runs` empty (`window.rs::get_selected_content_for_clipboard`,
//! paste in `common/event.rs`) and the platform clipboard backends write only
//! `plain_text`. Fully wiring it means (a) extracting per-run style from the
//! styled DOM when copying and (b) adding an HTML/RTF format to each platform's
//! clipboard write (and reading it back on paste). `to_html()` below is the
//! retained consumer for that future format. Until then the FFI surface is
//! kept (it is public API) but `styled_runs` stays empty.

use azul_css::{impl_option, impl_option_inner, AzString, OptionString};

// Clipboard Content Extraction

/// Styled text run for rich clipboard content
#[derive(Debug, Clone, PartialEq)]
#[repr(C)]
pub struct StyledTextRun {
    /// The actual text content
    pub text: AzString,
    /// Font family name
    pub font_family: OptionString,
    /// Font size in pixels
    pub font_size_px: f32,
    /// Text color
    pub color: azul_css::props::basic::ColorU,
    /// Whether text is bold
    pub is_bold: bool,
    /// Whether text is italic
    pub is_italic: bool,
}

azul_css::impl_option!(StyledTextRun, OptionStyledTextRun, copy = false, [Debug, Clone, PartialEq]);
azul_css::impl_vec!(StyledTextRun, StyledTextRunVec, StyledTextRunVecDestructor, StyledTextRunVecDestructorType, StyledTextRunVecSlice, OptionStyledTextRun);
azul_css::impl_vec_debug!(StyledTextRun, StyledTextRunVec);
azul_css::impl_vec_clone!(StyledTextRun, StyledTextRunVec, StyledTextRunVecDestructor);
azul_css::impl_vec_partialeq!(StyledTextRun, StyledTextRunVec);

/// Clipboard content with both plain text and styled (HTML) representation
#[derive(Debug, Clone, PartialEq)]
#[repr(C)]
pub struct ClipboardContent {
    /// Plain text representation (UTF-8)
    pub plain_text: AzString,
    /// Rich text runs with styling information
    pub styled_runs: StyledTextRunVec,
}

impl_option!(
    ClipboardContent,
    OptionClipboardContent,
    copy = false,
    [Debug, Clone, PartialEq]
);

impl ClipboardContent {
    /// Convert styled runs to HTML for rich clipboard formats.
    ///
    /// Retained consumer of the FFI-exported `styled_runs`: returns an empty
    /// `<div></div>` until `styled_runs` is populated and the platform clipboard
    /// backends gain an HTML format (see module docs). Kept as public API.
    #[must_use] pub fn to_html(&self) -> String {
        use core::fmt::Write as _;
        let mut html = String::from("<div>");

        for run in self.styled_runs.as_slice() {
            html.push_str("<span style=\"");

            if let Some(font_family) = run.font_family.as_ref() {
                let _ = write!(html, "font-family: {}; ", font_family.as_str());
            }
            let _ = write!(html, "font-size: {}px; ", run.font_size_px);
            let _ = write!(
                html,
                "color: rgba({}, {}, {}, {}); ",
                run.color.r,
                run.color.g,
                run.color.b,
                f32::from(run.color.a) / 255.0
            );
            if run.is_bold {
                html.push_str("font-weight: bold; ");
            }
            if run.is_italic {
                html.push_str("font-style: italic; ");
            }

            html.push_str("\">");
            // Escape HTML entities
            let escaped = run
                .text
                .as_str()
                .replace('&', "&amp;")
                .replace('<', "&lt;")
                .replace('>', "&gt;");
            html.push_str(&escaped);
            html.push_str("</span>");
        }

        html.push_str("</div>");
        html
    }
}

#[cfg(test)]
mod autotest_generated {
    use azul_css::{props::basic::ColorU, AzString, OptionString};

    use super::*;

    // =========================================================================
    // Fixtures
    //
    // `ClipboardContent::to_html` is a pure string builder, so the adversarial
    // surface is (a) the escaping pass over `text` (ordering, double-escaping,
    // characters it does *not* cover), (b) `f32` Display of `font_size_px`
    // (NaN / inf / -0.0 / MAX), (c) the u8 -> f32 alpha division at the
    // channel boundaries, and (d) structural invariants that must hold for
    // every input (balanced tags, one span per run, determinism).
    // =========================================================================

    const OPAQUE_BLACK: ColorU = ColorU {
        r: 0,
        g: 0,
        b: 0,
        a: 255,
    };

    /// A styled run, parameterized on every field the formatter reads.
    fn run(text: &str, font_size_px: f32, family: Option<&str>) -> StyledTextRun {
        StyledTextRun {
            text: AzString::from(text),
            font_family: family.map_or(OptionString::None, |f| {
                OptionString::Some(AzString::from(f))
            }),
            font_size_px,
            color: OPAQUE_BLACK,
            is_bold: false,
            is_italic: false,
        }
    }

    fn content(runs: Vec<StyledTextRun>) -> ClipboardContent {
        ClipboardContent {
            plain_text: AzString::from(""),
            styled_runs: runs.into(),
        }
    }

    /// Inverse of the escaping pass in `to_html` (entities undone in reverse
    /// order, so `&amp;` is restored last).
    fn unescape(s: &str) -> String {
        s.replace("&lt;", "<")
            .replace("&gt;", ">")
            .replace("&amp;", "&")
    }

    // ---------------------------------------------------------------------
    // basic_access: expected value after a known construction
    // ---------------------------------------------------------------------

    #[test]
    fn to_html_single_run_produces_exact_markup() {
        let c = content(vec![run("hi", 12.0, Some("Arial"))]);
        assert_eq!(
            c.to_html(),
            "<div><span style=\"font-family: Arial; font-size: 12px; color: rgba(0, 0, 0, 1); \
             \">hi</span></div>"
        );
    }

    #[test]
    fn to_html_omits_font_family_when_none() {
        let html = content(vec![run("x", 1.0, None)]).to_html();
        assert!(!html.contains("font-family"), "{html}");
        assert!(html.contains("font-size: 1px; "), "{html}");
    }

    #[test]
    fn to_html_emits_bold_and_italic_only_when_set() {
        let mut r = run("x", 10.0, None);
        assert!(!content(vec![r.clone()]).to_html().contains("font-weight"));
        assert!(!content(vec![r.clone()]).to_html().contains("font-style"));

        r.is_bold = true;
        r.is_italic = true;
        let html = content(vec![r]).to_html();
        assert!(html.contains("font-weight: bold; "), "{html}");
        assert!(html.contains("font-style: italic; "), "{html}");
    }

    #[test]
    fn to_html_concatenates_runs_in_order() {
        let html = content(vec![
            run("a", 1.0, None),
            run("b", 2.0, None),
            run("c", 3.0, None),
        ])
        .to_html();
        let a = html.find(">a<").expect("run a missing");
        let b = html.find(">b<").expect("run b missing");
        let c = html.find(">c<").expect("run c missing");
        assert!(a < b && b < c, "runs reordered: {html}");
    }

    // ---------------------------------------------------------------------
    // edge_access: default / empty / extreme instances must not panic
    // ---------------------------------------------------------------------

    #[test]
    fn to_html_empty_runs_is_empty_div() {
        assert_eq!(content(Vec::new()).to_html(), "<div></div>");
    }

    #[test]
    fn to_html_empty_run_text_yields_empty_span_body() {
        let html = content(vec![run("", 0.0, Some(""))]).to_html();
        assert!(html.ends_with("\"></span></div>"), "{html}");
        assert!(html.contains("font-family: ; "), "{html}");
    }

    #[test]
    fn to_html_ignores_plain_text_field() {
        // `to_html` only reads `styled_runs`; a populated `plain_text` (the
        // only field the live producers fill) must not leak into the markup.
        let c = ClipboardContent {
            plain_text: AzString::from("SHOULD-NOT-APPEAR"),
            styled_runs: Vec::<StyledTextRun>::new().into(),
        };
        assert_eq!(c.to_html(), "<div></div>");
    }

    // ---------------------------------------------------------------------
    // escaping / round-trip: escape(text) must be losslessly reversible
    // ---------------------------------------------------------------------

    #[test]
    fn to_html_escapes_angle_brackets_and_ampersand() {
        let html = content(vec![run("<script>a && b</script>", 10.0, None)]).to_html();
        assert!(
            html.contains("&lt;script&gt;a &amp;&amp; b&lt;/script&gt;"),
            "{html}"
        );
        assert!(!html.contains("<script>"), "raw tag survived: {html}");
    }

    #[test]
    fn to_html_does_not_double_escape_existing_entities() {
        // `&` is replaced first, so an input entity is escaped exactly once.
        let html = content(vec![run("&lt;&amp;", 10.0, None)]).to_html();
        assert!(html.contains(">&amp;lt;&amp;amp;<"), "{html}");
    }

    #[test]
    fn to_html_text_round_trips_through_unescape() {
        for text in [
            "",
            "plain",
            "&",
            "<",
            ">",
            "&amp;",
            "&lt;<>&gt;",
            "a<b>c&d",
            "&&&&<<<<>>>>",
        ] {
            let html = content(vec![run(text, 10.0, None)]).to_html();
            let body = html
                .rsplit_once("</span>")
                .and_then(|(head, _)| head.rsplit_once("\">").map(|(_, b)| b.to_string()))
                .expect("span body not found");
            assert_eq!(unescape(&body), text, "round-trip failed for {text:?}");
        }
    }

    #[test]
    fn to_html_escaped_text_introduces_no_raw_markup_chars() {
        // With no font-family, every `<`/`>` in the output must come from the
        // four structural tags: <div>, <span ...>, </span>, </div>.
        let html = content(vec![run("<<<>>>&&&", 10.0, None)]).to_html();
        assert_eq!(html.matches('<').count(), 4, "{html}");
        assert_eq!(html.matches('>').count(), 4, "{html}");
    }

    #[test]
    fn to_html_font_family_is_interpolated_raw_into_the_style_attribute() {
        // Characterization test (NOT an endorsement): unlike `text`, the font
        // family is written into the `style="..."` attribute with no escaping
        // or quoting, so a quote in the family name terminates the attribute.
        // Live producers never populate `styled_runs`, so this is currently
        // unreachable — but any future producer must sanitize the family name.
        let html = content(vec![run("t", 10.0, Some("\"><img onerror=x>"))]).to_html();
        assert!(
            html.contains("font-family: \"><img onerror=x>; "),
            "escaping behaviour changed, re-check the injection note: {html}"
        );
    }

    // ---------------------------------------------------------------------
    // numeric: font_size_px is a raw f32 Display
    // ---------------------------------------------------------------------

    #[test]
    fn to_html_non_finite_font_size_does_not_panic() {
        for (size, rendered) in [
            (f32::NAN, "font-size: NaNpx; "),
            (f32::INFINITY, "font-size: infpx; "),
            (f32::NEG_INFINITY, "font-size: -infpx; "),
        ] {
            let html = content(vec![run("x", size, None)]).to_html();
            assert!(html.contains(rendered), "{size} -> {html}");
            assert!(html.ends_with("</div>"), "{html}");
        }
    }

    #[test]
    fn to_html_extreme_finite_font_sizes_do_not_panic() {
        for size in [
            0.0,
            -0.0,
            -1.0,
            f32::MIN,
            f32::MAX,
            f32::MIN_POSITIVE,
            f32::EPSILON,
        ] {
            let html = content(vec![run("x", size, None)]).to_html();
            assert!(html.starts_with("<div><span"), "{size} -> {html}");
            assert!(html.ends_with("x</span></div>"), "{size} -> {html}");
            assert!(html.contains("font-size: "), "{size} -> {html}");
        }
    }

    #[test]
    fn to_html_alpha_is_normalized_to_0_1_at_channel_boundaries() {
        let alpha_of = |a: u8| {
            let mut r = run("x", 10.0, None);
            r.color = ColorU { r: 0, g: 0, b: 0, a };
            content(vec![r]).to_html()
        };
        assert!(alpha_of(255).contains("rgba(0, 0, 0, 1); "));
        assert!(alpha_of(0).contains("rgba(0, 0, 0, 0); "));
        // 1/255 keeps full f32 precision — it is not truncated to 0.
        let expected = format!("rgba(0, 0, 0, {}); ", 1.0_f32 / 255.0);
        assert!(alpha_of(1).contains(&expected), "{}", alpha_of(1));
    }

    #[test]
    fn to_html_color_channels_are_verbatim_u8() {
        let mut r = run("x", 10.0, None);
        r.color = ColorU {
            r: 255,
            g: 0,
            b: 128,
            a: 255,
        };
        assert!(content(vec![r])
            .to_html()
            .contains("color: rgba(255, 0, 128, 1); "));
    }

    // ---------------------------------------------------------------------
    // unicode / hostile payloads
    // ---------------------------------------------------------------------

    #[test]
    fn to_html_preserves_unicode_and_control_characters() {
        for text in [
            "😀👨‍👩‍👧‍👦",         // emoji + ZWJ sequence
            "مرحبا بالعالم",  // RTL
            "e\u{0301}\u{0327}", // combining marks
            "a\u{0}b",        // interior NUL
            "line\nbreak\ttab",
            "\u{200B}\u{FEFF}", // zero-width space + BOM
            "\u{202E}reversed", // RTL override
        ] {
            let html = content(vec![run(text, 10.0, None)]).to_html();
            assert!(html.contains(text), "lost {text:?} in {html:?}");
            assert!(html.ends_with("</span></div>"), "{html:?}");
        }
    }

    #[test]
    fn to_html_large_text_does_not_panic() {
        let text = "&".repeat(100_000);
        let html = content(vec![run(&text, 10.0, None)]).to_html();
        // every `&` expands to the 5-byte `&amp;`
        assert_eq!(html.matches("&amp;").count(), 100_000);
        assert!(html.ends_with("</span></div>"));
    }

    #[test]
    fn to_html_many_runs_emits_one_span_each() {
        let runs: Vec<_> = (0..2_000u16)
            .map(|i| run("t", f32::from(i), Some("Arial")))
            .collect();
        let html = content(runs).to_html();
        assert_eq!(html.matches("<span style=\"").count(), 2_000);
        assert_eq!(html.matches("</span>").count(), 2_000);
    }

    // ---------------------------------------------------------------------
    // invariants: structure, purity, clone-equivalence
    // ---------------------------------------------------------------------

    #[test]
    fn to_html_always_wraps_output_in_a_single_div() {
        let cases = vec![
            content(Vec::new()),
            content(vec![run("", f32::NAN, None)]),
            content(vec![run("<>&", -0.0, Some("a\"b"))]),
            content(vec![run("😀", f32::MAX, Some(""))]),
        ];
        for c in cases {
            let html = c.to_html();
            assert!(html.starts_with("<div>"), "{html}");
            assert!(html.ends_with("</div>"), "{html}");
            assert_eq!(html.matches("<div>").count(), 1, "{html}");
            assert_eq!(html.matches("</div>").count(), 1, "{html}");
            assert_eq!(
                html.matches("<span style=\"").count(),
                html.matches("</span>").count(),
                "unbalanced spans: {html}"
            );
        }
    }

    #[test]
    fn to_html_is_pure_and_deterministic() {
        let c = content(vec![run("<a>", 12.5, Some("Arial")), run("&", 0.0, None)]);
        let before = c.clone();
        let first = c.to_html();
        let second = c.to_html();
        assert_eq!(first, second, "to_html is not deterministic");
        assert_eq!(c, before, "to_html mutated the receiver");
        assert_eq!(c.clone().to_html(), first, "clone renders differently");
    }
}

