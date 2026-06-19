//! Clipboard content types for copy/paste operations
//!
//! Contains `ClipboardContent` and `StyledTextRun`, used by clipboard and
//! changeset modules.

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
    /// Convert styled runs to HTML for rich clipboard formats
    pub fn to_html(&self) -> String {
        let mut html = String::from("<div>");

        for run in self.styled_runs.as_slice() {
            html.push_str("<span style=\"");

            if let Some(font_family) = run.font_family.as_ref() {
                html.push_str(&format!("font-family: {}; ", font_family.as_str()));
            }
            html.push_str(&format!("font-size: {}px; ", run.font_size_px));
            html.push_str(&format!(
                "color: rgba({}, {}, {}, {}); ",
                run.color.r,
                run.color.g,
                run.color.b,
                run.color.a as f32 / 255.0
            ));
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

