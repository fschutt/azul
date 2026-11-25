//! Bookmarks and clickable links.
//!
//! A PDF can navigate in two ways. The *outline* (the bookmarks panel of a viewer) is a
//! document-level list of titles that each jump to a page. A *link annotation* is a
//! clickable rectangle on one page that jumps to another place in the document or opens a
//! URI. Both are written by [`PdfDocument::save`](crate::PdfDocument::save) and read back by
//! [`PdfDocument::parse`](crate::PdfDocument::parse).
//!
//! * A bookmark is a [`PageAnnotation`]. Register it with
//!   [`PdfDocument::add_bookmark`](crate::PdfDocument::add_bookmark). Bookmarks form a flat list
//!   (no nesting), ordered by page.
//! * A link is a [`LinkAnnotation`]. It belongs to a page: push an
//!   [`Op::LinkAnnotation`](crate::Op::LinkAnnotation) into the `ops` of that page.
//!
//! Page numbers in this module start at 1: page 1 is the first entry of
//! [`PdfDocument::pages`](crate::PdfDocument::pages). A bookmark that points at a page that does
//! not exist is left out of the outline, and a `GoTo` destination to such a page is written
//! without a page reference.
//!
//! # Examples
//!
//! ```
//! use printpdf::*;
//!
//! let mut doc = PdfDocument::new("Links");
//! let link = LinkAnnotation::new(
//!     // 100 x 20 pt clickable area, 50 pt from the left and bottom edge of the page
//!     Rect::from_xywh(Pt(50.0), Pt(50.0), Pt(100.0), Pt(20.0)),
//!     Actions::uri("https://example.com".to_string()),
//!     None,
//!     None,
//!     None,
//! );
//! doc.pages.push(PdfPage::new(Mm(210.0), Mm(297.0), vec![Op::LinkAnnotation { link }]));
//! doc.add_bookmark("Start", 1);
//! let bytes = doc.save(&PdfSaveOptions::default(), &mut Vec::new());
//! assert!(bytes.starts_with(b"%PDF"));
//! ```

use serde_derive::{Deserialize, Serialize};

use crate::graphics::Rect;

/// A bookmark: an entry of the document outline that jumps to the top of a page.
///
/// Serialized to JSON with camelCase field names.
#[derive(Debug, PartialEq, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PageAnnotation {
    /// Title shown in the viewer's outline, for example `"Chapter 5"`.
    pub name: String,
    /// Page to jump to. Starts at 1, so `10` is the tenth page. An entry whose page does not
    /// exist is not written.
    pub page: usize,
}

/// A clickable rectangle on a page (a PDF `Link` annotation).
///
/// Add it to a page with [`Op::LinkAnnotation`](crate::Op::LinkAnnotation). Serialized to JSON
/// with camelCase field names.
#[derive(Debug, PartialEq, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LinkAnnotation {
    /// Clickable area, in points, measured from the bottom left corner of the page. Only
    /// `x`, `y`, `width` and `height` are used.
    pub rect: Rect,
    /// What happens when the area is clicked.
    pub actions: Actions,
    /// Border drawn around the area. Defaults to [`BorderArray::default`].
    #[serde(default)]
    pub border: BorderArray,
    /// Color of the border. Defaults to [`ColorArray::default`].
    #[serde(default)]
    pub color: ColorArray,
    /// How the area looks while the mouse button is held down. Defaults to
    /// [`HighlightingMode::Invert`].
    #[serde(default)]
    pub highlighting: HighlightingMode,
}

impl LinkAnnotation {
    /// Creates a link. `None` for `border`, `color` or `highlighting` selects the default of
    /// that field.
    pub fn new(
        rect: Rect,
        actions: Actions,
        border: Option<BorderArray>,
        color: Option<ColorArray>,
        highlighting: Option<HighlightingMode>,
    ) -> Self {
        Self {
            rect,
            border: border.unwrap_or_default(),
            color: color.unwrap_or_default(),
            actions,
            highlighting: highlighting.unwrap_or_default(),
        }
    }
}

/// Border of a link, the `/Border` entry of the annotation (ISO 32000-1, 12.5.2).
///
/// Serialized to JSON as `{"type": "solid", "data": ...}`.
#[derive(Debug, PartialEq, Clone, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case", tag = "type", content = "data")]
pub enum BorderArray {
    /// Solid border: horizontal corner radius, vertical corner radius and line width, in
    /// points. A width of `0.0` draws no border.
    Solid([f32; 3]),
    /// Like `Solid`, with a dash pattern.
    Dashed([f32; 3], DashPhase),
}

impl BorderArray {
    /// Flattens the border to the numbers written to the PDF: the three values and, for
    /// `Dashed`, the dash phase as a fourth number. The dash lengths are not included.
    pub fn to_array(&self) -> Vec<f32> {
        match self {
            BorderArray::Solid(s) => s.to_vec(),
            BorderArray::Dashed(s, dash_phase) => {
                let mut s = s.to_vec();
                s.push(dash_phase.phase);
                s
            }
        }
    }
}

impl Default for BorderArray {
    /// A solid border with square corners and a width of 1 pt.
    fn default() -> Self {
        BorderArray::Solid([0.0, 0.0, 1.0])
    }
}

/// Dash pattern of a [`BorderArray::Dashed`] border.
#[derive(Debug, PartialEq, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DashPhase {
    /// Lengths of the alternating dashes and gaps, in points.
    pub dash_array: Vec<f32>,
    /// Offset into the pattern at which the border starts, in points.
    pub phase: f32,
}

/// Color of a link border, the `/C` entry of the annotation. Components are in `0.0..=1.0`.
///
/// Serialized to JSON as `{"type": "rgb", "data": [r, g, b]}`.
#[derive(Debug, PartialEq, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case", tag = "type", content = "data")]
pub enum ColorArray {
    /// No color: the border is not drawn (written as an empty array).
    Transparent,
    /// Gray level, 0 is black.
    Gray([f32; 1]),
    /// Red, green and blue.
    Rgb([f32; 3]),
    /// Cyan, magenta, yellow and black.
    Cmyk([f32; 4]),
}

impl Default for ColorArray {
    /// Cyan, `Rgb([0.0, 1.0, 1.0])`.
    fn default() -> Self {
        ColorArray::Rgb([0.0, 1.0, 1.0])
    }
}

/// A place in the document that a link jumps to (ISO 32000-1, 12.3.2).
///
/// Serialized to JSON as `{"type": "xyz", "data": {...}}`.
#[derive(Debug, PartialEq, Clone, Serialize, Deserialize)]
#[non_exhaustive]
#[serde(rename_all = "kebab-case", tag = "type", content = "data")]
pub enum Destination {
    /// Shows `page` with the point (`left`, `top`) at the upper left corner of the window,
    /// magnified by `zoom` (`/XYZ`).
    ///
    /// `None` keeps the viewer's current value for that parameter. The coordinates are in
    /// points, measured from the bottom left corner of the page. A `zoom` of `0.0` means the
    /// same as `None`.
    Xyz {
        /// Page to show, starting at 1. A page that does not exist is written as `null`.
        page: usize,
        /// Horizontal position in points.
        left: Option<f32>,
        /// Vertical position in points.
        top: Option<f32>,
        /// Magnification factor, `1.0` is 100%.
        zoom: Option<f32>,
    },
}

/// What a link does when clicked (ISO 32000-1, 12.6). Only these two action types are
/// supported; a link with another action is dropped when a PDF is parsed.
///
/// Serialized to JSON as `{"type": "goto", "data": ...}`.
#[derive(Debug, PartialEq, Clone, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case", tag = "type", content = "data")]
pub enum Actions {
    /// Jumps to a place in this document (`/GoTo`).
    Goto(Destination),
    /// Opens a URI, for example a web address (`/URI`).
    Uri(String),
}

impl Actions {
    /// Name of the action type written to the `/S` entry: `"GoTo"` or `"URI"`.
    pub fn get_action_type_id(&self) -> &'static str {
        match self {
            Actions::Goto(_) => "GoTo",
            Actions::Uri(_) => "URI",
        }
    }

    /// Creates an [`Actions::Goto`] action.
    pub fn go_to(destination: Destination) -> Self {
        Self::Goto(destination)
    }

    /// Creates an [`Actions::Uri`] action.
    pub fn uri(uri: String) -> Self {
        Self::Uri(uri)
    }
}

/// Visual feedback while the mouse button is held down on a link, the `/H` entry
/// (ISO 32000-1, 12.5.6.5).
#[derive(Debug, PartialEq, Clone, Copy, Default, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum HighlightingMode {
    /// No feedback (`/N`).
    None,
    /// Inverts the colors of the area (`/I`). The default.
    #[default]
    Invert,
    /// Inverts the border (`/O`).
    Outline,
    /// Shows the area as pushed in (`/P`).
    Push,
}

impl HighlightingMode {
    /// The name written to the `/H` entry: `"N"`, `"I"`, `"O"` or `"P"`.
    pub fn get_id(&self) -> &'static str {
        use self::HighlightingMode::*;
        match self {
            None => "N",
            Invert => "I",
            Outline => "O",
            Push => "P",
        }
    }
}
