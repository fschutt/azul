//! Clipboard content types for copy/paste operations
//!
//! Contains `ClipboardContent` and `StyledTextRun`, used by clipboard and
//! changeset modules.
//!
//! **Rich-text status: wired end to end.** A paste populates `styled_runs`
//! whenever the source offered a styled flavor — the platform transports in
//! `dll/src/desktop/shell2/*/clipboard.rs` hand every flavor the source
//! published to `rich-clipboard`, whose decode policy prefers RTF, then HTML,
//! then plain text, and `shell2/common/clipboard.rs` converts the result into
//! this type. A copy goes the other way: [`ClipboardExtract`] pulls per-run
//! formatting off each source `StyledRun` as the selection is walked, and the
//! transports fan that out as RTF *and* HTML *and* plain text at once — so a
//! copy out of azul pastes into Word or LibreOffice formatted.
//!
//! Two platforms are less capable, and both say so in their own docs: X11's
//! selection owner can serve only one target (`x11/clipboard.rs`), so it
//! publishes plain text; and a Wayland session with no compositor selection
//! falls back through XWayland to that same single-target path.
//!
//! `to_html()` below predates all of this and is not on the clipboard path:
//! `rich-clipboard`'s `RichText::to_html_fragment` is what actually gets
//! published, because it also emits the matching RTF and the `CF_HTML`
//! wrapper Windows needs. This stays as public FFI API.

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

/// Builds a [`ClipboardContent`] from the styled runs a selection walks over.
///
/// Not FFI — this is the copy path's scratch buffer, and it exists so the three
/// selection shapes (single-run, multi-run, cross-block) all produce their
/// plain text and their styling from one place instead of the plain text
/// twice.
///
/// The two invariants it maintains are the ones the OS transports depend on:
/// `plain_text` is exactly the concatenation of the runs' text (a receiver must
/// not get different characters depending on which flavor it picks), and
/// adjacent runs with identical formatting are merged.
///
/// Merging is not cosmetic. A style run is cut at every DOM text node and every
/// styling change, so a paragraph typed in one font can arrive as dozens of
/// runs; emitting each one as its own `<span>` or `\b0\b` pair would produce
/// RTF and HTML several times the size of the text for no visible difference.
#[derive(Debug, Default)]
pub struct ClipboardExtract {
    /// Grown in place. Runs borrow nothing from it — they keep their own text —
    /// but the two are appended to together, which is what keeps them equal.
    plain: String,
    runs: Vec<PendingRun>,
}

/// A run still being accumulated.
///
/// Held as a plain `String` rather than the FFI `AzString` so that merging is
/// an append. Building an `AzString` per merge would re-copy everything
/// accumulated so far, which on a uniformly-styled select-all — where *every*
/// run merges into one — is quadratic in the document.
#[derive(Debug)]
struct PendingRun {
    text: String,
    style: RunStyle,
}

/// The formatting fields of a [`StyledTextRun`], without the text.
///
/// Comparing these is what decides a merge. `f32` equality is the derived one:
/// a NaN size simply never merges, which costs a few extra runs in a case that
/// cannot arise from the cascade anyway.
#[derive(Debug, Clone, PartialEq)]
struct RunStyle {
    font_family: Option<String>,
    font_size_px: f32,
    color: azul_css::props::basic::ColorU,
    is_bold: bool,
    is_italic: bool,
}

impl ClipboardExtract {
    /// Append `text`, formatted by `style`.
    pub fn push(&mut self, text: &str, style: &crate::text3::cache::StyleProperties) {
        if text.is_empty() {
            return;
        }
        self.plain.push_str(text);

        let selector = style.font_stack.first_selector();
        let run_style = RunStyle {
            // A direct `FontRef` (an embedded icon font) has no family name to
            // give a receiving application, and `FontStack::first_family`'s
            // `"<embedded-font>"` placeholder is a debugging string, not a
            // font anyone can resolve. `None` means "inherit", which is the
            // honest answer.
            font_family: selector.map(|s| s.family.clone()),
            font_size_px: style.font_size_px,
            color: style.color,
            // CSS `font-weight: bold` is 700; everything at or above it reads
            // as bold to a format that only has a boolean. `FcWeight` is
            // ordered by its CSS numeric value, so this is that comparison.
            is_bold: selector.is_some_and(|s| s.weight >= rust_fontconfig::FcWeight::Bold),
            // Oblique is a slanted rendering of an upright face; every
            // clipboard format this feeds collapses it into italic.
            is_italic: selector.is_some_and(|s| {
                matches!(
                    s.style,
                    crate::text3::cache::FontStyle::Italic
                        | crate::text3::cache::FontStyle::Oblique
                )
            }),
        };

        match self.runs.last_mut() {
            Some(last) if last.style == run_style => last.text.push_str(text),
            _ => self.runs.push(PendingRun {
                text: String::from(text),
                style: run_style,
            }),
        }
    }

    /// Append text under whatever formatting is already in effect.
    ///
    /// For the paragraph joiner between blocks of a cross-block selection: a
    /// `\n` of its own would be an unformatted run wedged between two formatted
    /// ones, which RTF and HTML both have to spell out.
    pub fn push_inheriting(&mut self, text: &str) {
        if text.is_empty() {
            return;
        }
        self.plain.push_str(text);
        match self.runs.last_mut() {
            Some(last) => last.text.push_str(text),
            // Nothing to inherit from: the selection started with the joiner.
            None => self.runs.push(PendingRun {
                text: String::from(text),
                style: RunStyle {
                    font_family: None,
                    font_size_px: 0.0,
                    color: azul_css::props::basic::ColorU {
                        r: 0,
                        g: 0,
                        b: 0,
                        a: 255,
                    },
                    is_bold: false,
                    is_italic: false,
                },
            }),
        }
    }

    /// The collected content, or `None` if the selection held no text.
    ///
    /// This is where the FFI strings are built — once per run, not once per
    /// merge.
    #[must_use]
    pub fn finish(self) -> Option<ClipboardContent> {
        if self.plain.is_empty() {
            return None;
        }
        let runs: Vec<StyledTextRun> = self
            .runs
            .into_iter()
            .map(|r| StyledTextRun {
                text: r.text.into(),
                font_family: r.style.font_family.map(AzString::from).into(),
                font_size_px: r.style.font_size_px,
                color: r.style.color,
                is_bold: r.style.is_bold,
                is_italic: r.style.is_italic,
            })
            .collect();
        Some(ClipboardContent {
            plain_text: self.plain.into(),
            styled_runs: runs.into(),
        })
    }
}

impl ClipboardContent {
    /// Convert styled runs to HTML for rich clipboard formats.
    ///
    /// Public FFI API, and **not** what the clipboard publishes — that is
    /// `rich-clipboard`'s `RichText::to_html_fragment`, which also emits the
    /// matching RTF and the `CF_HTML` wrapper Windows needs (see module docs).
    /// Kept for callers that want a quick HTML rendering of a
    /// `ClipboardContent` without the rest of that stack.
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

#[cfg(test)]
mod extract_tests {
    use std::sync::Arc;

    use azul_css::props::basic::ColorU;
    use rust_fontconfig::FcWeight;

    use super::*;
    use crate::text3::cache::{FontSelector, FontStack, FontStyle, StyleProperties};

    fn style(family: &str, weight: FcWeight, italic: bool, size_px: f32) -> StyleProperties {
        StyleProperties {
            font_stack: FontStack::Stack(vec![FontSelector {
                family: family.to_string(),
                weight,
                style: if italic {
                    FontStyle::Italic
                } else {
                    FontStyle::Normal
                },
                unicode_ranges: Vec::new(),
            }]),
            font_size_px: size_px,
            ..StyleProperties::default()
        }
    }

    fn plain_style() -> StyleProperties {
        style("Helvetica", FcWeight::Normal, false, 16.0)
    }

    /// The whole point of the copy path: formatting from the source runs has to
    /// reach `styled_runs`, because that is what the OS transports turn into
    /// RTF and HTML. It used to be hardcoded empty.
    ///
    /// NEGATIVE CONTROL: make `push` always write `StyledTextRun::default()`-ish
    /// values — the bold/size/family assertions below go red.
    #[test]
    fn formatting_reaches_the_styled_runs() {
        let mut acc = ClipboardExtract::default();
        acc.push("normal ", &plain_style());
        acc.push("bold", &style("Georgia", FcWeight::Bold, false, 24.0));

        let content = acc.finish().expect("a non-empty selection yields content");
        assert_eq!(content.plain_text.as_str(), "normal bold");

        let runs = content.styled_runs.as_slice();
        assert_eq!(runs.len(), 2);
        assert_eq!(runs[0].text.as_str(), "normal ");
        assert!(!runs[0].is_bold);
        assert_eq!(runs[0].font_family.as_ref().map(|f| f.as_str()), Some("Helvetica"));
        assert_eq!(runs[1].text.as_str(), "bold");
        assert!(runs[1].is_bold);
        assert_eq!(runs[1].font_size_px, 24.0);
        assert_eq!(runs[1].font_family.as_ref().map(|f| f.as_str()), Some("Georgia"));
    }

    /// `plain_text` must be exactly the concatenation of the runs. A receiver
    /// picking the plain flavor and one picking RTF must not end up with
    /// different characters — the seam's encoder drops the rich flavor outright
    /// when these disagree, so a mismatch silently costs all the formatting.
    ///
    /// NEGATIVE CONTROL: push to `self.plain` without pushing a run (or the
    /// reverse) in either `push` or `push_inheriting`.
    #[test]
    fn plain_text_is_exactly_the_concatenated_runs() {
        let mut acc = ClipboardExtract::default();
        acc.push("one", &plain_style());
        acc.push_inheriting("\n");
        acc.push("two", &style("Georgia", FcWeight::Bold, false, 16.0));
        acc.push_inheriting("\n");
        acc.push("three", &plain_style());

        let content = acc.finish().expect("content");
        let joined: String = content
            .styled_runs
            .as_slice()
            .iter()
            .map(|r| r.text.as_str())
            .collect();
        assert_eq!(content.plain_text.as_str(), joined);
        assert_eq!(content.plain_text.as_str(), "one\ntwo\nthree");
    }

    /// A style run is cut at every DOM text node and every styling change, so
    /// identically-formatted neighbours must merge — otherwise a paragraph in
    /// one font emits dozens of `<span>`s and `\b0\b` pairs for no visible
    /// difference.
    ///
    /// NEGATIVE CONTROL: drop the `same_formatting` arm from `push`.
    #[test]
    fn identically_formatted_neighbours_merge() {
        let mut acc = ClipboardExtract::default();
        for word in ["a", "b", "c", "d"] {
            acc.push(word, &plain_style());
        }
        let content = acc.finish().expect("content");
        assert_eq!(
            content.styled_runs.as_slice().len(),
            1,
            "four identically-styled pushes must collapse to one run"
        );
        assert_eq!(content.styled_runs.as_slice()[0].text.as_str(), "abcd");
        assert_eq!(content.plain_text.as_str(), "abcd");
    }

    /// The paragraph joiner in a cross-block copy inherits the formatting it
    /// follows, so it does not split a styled run in three.
    ///
    /// NEGATIVE CONTROL: make `push_inheriting` push a default-styled run.
    #[test]
    fn the_paragraph_joiner_does_not_split_a_run() {
        let mut acc = ClipboardExtract::default();
        let bold = style("Georgia", FcWeight::Bold, false, 16.0);
        acc.push("first", &bold);
        acc.push_inheriting("\n");
        acc.push("second", &bold);

        let content = acc.finish().expect("content");
        let runs = content.styled_runs.as_slice();
        assert_eq!(runs.len(), 1, "one style spans the whole thing");
        assert_eq!(runs[0].text.as_str(), "first\nsecond");
        assert!(runs[0].is_bold);
    }

    /// Weight is a scale and the clipboard formats have a boolean. CSS's
    /// `bold` is 700, so that is the cut.
    #[test]
    fn weight_maps_to_bold_at_seven_hundred() {
        for (weight, expect_bold) in [
            (FcWeight::Light, false),
            (FcWeight::Normal, false),
            (FcWeight::Medium, false),
            (FcWeight::SemiBold, false),
            (FcWeight::Bold, true),
            (FcWeight::Black, true),
        ] {
            let mut acc = ClipboardExtract::default();
            acc.push("x", &style("Helvetica", weight, false, 16.0));
            let content = acc.finish().expect("content");
            assert_eq!(
                content.styled_runs.as_slice()[0].is_bold,
                expect_bold,
                "{weight:?} mapped to the wrong boldness"
            );
        }
    }

    /// Oblique is a slanted upright face; every format this feeds has only
    /// "italic", so it must not be silently dropped.
    #[test]
    fn oblique_is_carried_as_italic() {
        let mut acc = ClipboardExtract::default();
        let mut oblique = plain_style();
        oblique.font_stack = FontStack::Stack(vec![FontSelector {
            family: "Helvetica".to_string(),
            weight: FcWeight::Normal,
            style: FontStyle::Oblique,
            unicode_ranges: Vec::new(),
        }]);
        acc.push("slanted", &oblique);
        let content = acc.finish().expect("content");
        assert!(content.styled_runs.as_slice()[0].is_italic);
    }

    /// An embedded `FontRef` has no family name a receiving application could
    /// resolve, so it must inherit rather than be handed the internal
    /// `"<embedded-font>"` debugging placeholder as if it were a font.
    ///
    /// NEGATIVE CONTROL: use `FontStack::first_family()` in `push`.
    #[test]
    fn an_embedded_font_has_no_family_to_publish() {
        let mut acc = ClipboardExtract::default();
        let mut embedded = plain_style();
        // `FontStack::Ref` needs a real FontRef; `first_selector()` returning
        // None is the property under test, and `Stack(vec![])` reaches the
        // same branch without constructing one.
        embedded.font_stack = FontStack::Stack(Vec::new());
        acc.push("icon", &embedded);

        let content = acc.finish().expect("content");
        let run = &content.styled_runs.as_slice()[0];
        assert!(run.font_family.as_ref().is_none());
        assert!(!run.is_bold && !run.is_italic);
    }

    /// A uniformly-styled select-all merges *every* run into one, so the merge
    /// has to be an append. Rebuilding the run's string each time — which is
    /// what constructing the FFI `AzString` per merge would do — is quadratic
    /// in the document, and Ctrl+A Ctrl+C is exactly when it bites.
    ///
    /// This asserts the result rather than the timing; the guard against a
    /// regression is that `PendingRun::text` is a `String` and `finish` is the
    /// only place `AzString`s are built.
    #[test]
    fn a_uniformly_styled_document_collapses_to_one_run() {
        let mut acc = ClipboardExtract::default();
        let s = plain_style();
        for i in 0..2_000 {
            acc.push("paragraph text ", &s);
            if i + 1 < 2_000 {
                acc.push_inheriting("\n");
            }
        }
        let content = acc.finish().expect("content");
        assert_eq!(content.styled_runs.as_slice().len(), 1);
        assert_eq!(
            content.styled_runs.as_slice()[0].text.as_str().len(),
            content.plain_text.as_str().len()
        );
    }

    /// An empty selection is not a copy. Pushing nothing must not produce a
    /// `ClipboardContent` that would clear the user's clipboard.
    #[test]
    fn an_empty_selection_yields_nothing() {
        assert!(ClipboardExtract::default().finish().is_none());
        let mut acc = ClipboardExtract::default();
        acc.push("", &plain_style());
        acc.push_inheriting("");
        assert!(acc.finish().is_none());
    }
}

