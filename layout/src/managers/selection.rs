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

