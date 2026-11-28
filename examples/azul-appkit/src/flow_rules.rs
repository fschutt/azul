//! The rules of the FLOW editor that need no engine (plain Rust, tested without libazul): the
//! formatting quick-access vocabulary ([`Quick`]: its tags, icons and words), the size steps, the
//! heading cycle, and the canvas the text flows on ([`FlowOptions`] and the CSS of its three
//! boxes). `crate::flow` (feature `azul`) builds the widgets from them.
//!
//! Tags: a quick-access button's tag is `Quick::tag()`, always below [`TAG_LIMIT`], so an app
//! appends buttons of its own with tags from `TAG_LIMIT` upwards to the list this module's
//! `quick_access` returns, without a collision.

/// The tags of [`Quick`] are below this; an app's own quick-access tags start here.
pub const TAG_LIMIT: usize = 1000;

/// The sizes the size buttons step through, in half points (the unit the rich text model keeps:
/// 22 is 11 pt): Word's list of sizes.
pub static SIZES_HALF_POINTS: [u16; 17] = [16, 18, 20, 21, 22, 24, 28, 32, 36, 40, 44, 48, 52, 56, 72, 96, 144];

/// The body size of the editors, half points: what a run without a size of its own has.
pub const BODY_HALF_POINTS: u16 = 22;

/// The size a step takes from `half_points` (0: the body's own): the next larger or smaller size
/// of [`SIZES_HALF_POINTS`] than the current one, held at both ends of the list. A size between
/// two listed ones steps to the neighbour in the direction asked.
#[must_use]
pub fn step_size(half_points: u16, up: bool) -> u16 {
    let current = if half_points == 0 { BODY_HALF_POINTS } else { half_points };
    let first = SIZES_HALF_POINTS[0];
    let last = SIZES_HALF_POINTS[SIZES_HALF_POINTS.len() - 1];
    if up {
        SIZES_HALF_POINTS
            .iter()
            .copied()
            .find(|size| *size > current)
            .unwrap_or(current.max(last))
    } else {
        SIZES_HALF_POINTS
            .iter()
            .rev()
            .copied()
            .find(|size| *size < current)
            .unwrap_or(current.min(first))
    }
}

/// The heading level the Heading button asks for from a block that is a heading of `level` (`None`:
/// not a heading): paragraph -> 1 -> 2 -> 3, and from 3 it asks for 3 again, which the editor's
/// toggle turns back into a paragraph.
#[must_use]
pub const fn heading_target(level: Option<u8>) -> u8 {
    match level {
        None => 1,
        Some(1) => 2,
        Some(_) => 3,
    }
}

/// A formatting button of the collapsed rail.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Quick {
    Undo,
    Redo,
    Bold,
    Italic,
    Underline,
    Strike,
    Code,
    SizeDown,
    SizeUp,
    Bullets,
    Numbers,
    Checklist,
    Heading,
    Quote,
    Outdent,
    Indent,
}

/// Every button, in the order the tags number them.
pub static QUICK_ALL: [Quick; 16] = [
    Quick::Undo,
    Quick::Redo,
    Quick::Bold,
    Quick::Italic,
    Quick::Underline,
    Quick::Strike,
    Quick::Code,
    Quick::SizeDown,
    Quick::SizeUp,
    Quick::Bullets,
    Quick::Numbers,
    Quick::Checklist,
    Quick::Heading,
    Quick::Quote,
    Quick::Outdent,
    Quick::Indent,
];

impl Quick {
    /// The tag the sidebar reports for this button (below [`TAG_LIMIT`]).
    #[must_use]
    pub fn tag(self) -> usize {
        QUICK_ALL.iter().position(|q| *q == self).unwrap_or(0)
    }

    /// The button a reported tag names; `None` for a tag of the app's own.
    #[must_use]
    pub fn of_tag(tag: usize) -> Option<Self> {
        QUICK_ALL.get(tag).copied()
    }

    /// The Material icon of the button.
    #[must_use]
    pub const fn icon(self) -> &'static str {
        match self {
            Self::Undo => "undo",
            Self::Redo => "redo",
            Self::Bold => "format_bold",
            Self::Italic => "format_italic",
            Self::Underline => "format_underlined",
            Self::Strike => "strikethrough_s",
            Self::Code => "code",
            Self::SizeDown => "text_decrease",
            Self::SizeUp => "text_increase",
            Self::Bullets => "format_list_bulleted",
            Self::Numbers => "format_list_numbered",
            Self::Checklist => "checklist",
            Self::Heading => "title",
            Self::Quote => "format_quote",
            Self::Outdent => "format_indent_decrease",
            Self::Indent => "format_indent_increase",
        }
    }

    /// The key of the button's name (its tooltip and its screen reader name) in appkit's resources.
    #[must_use]
    pub const fn key(self) -> &'static str {
        match self {
            Self::Undo => "kit-flow-undo",
            Self::Redo => "kit-flow-redo",
            Self::Bold => "kit-flow-bold",
            Self::Italic => "kit-flow-italic",
            Self::Underline => "kit-flow-underline",
            Self::Strike => "kit-flow-strike",
            Self::Code => "kit-flow-code",
            Self::SizeDown => "kit-flow-size-down",
            Self::SizeUp => "kit-flow-size-up",
            Self::Bullets => "kit-flow-bullets",
            Self::Numbers => "kit-flow-numbers",
            Self::Checklist => "kit-flow-checklist",
            Self::Heading => "kit-flow-heading",
            Self::Quote => "kit-flow-quote",
            Self::Outdent => "kit-flow-outdent",
            Self::Indent => "kit-flow-indent",
        }
    }

    /// Whether the button is a toggle that shows the caret's state (bold, a list, a quote) and
    /// not a plain action (undo, a size step, an indent).
    #[must_use]
    pub const fn is_toggle(self) -> bool {
        matches!(
            self,
            Self::Bold
                | Self::Italic
                | Self::Underline
                | Self::Strike
                | Self::Code
                | Self::Bullets
                | Self::Numbers
                | Self::Checklist
                | Self::Heading
                | Self::Quote
        )
    }
}

/// The default list of the formatting quick access of a writing app's rail: undo at the top,
/// then the size, the emphasis, bold LAST (the nearest to the thumb).
pub static WRITER_QUICK: [Quick; 7] = [
    Quick::Undo,
    Quick::SizeDown,
    Quick::SizeUp,
    Quick::Underline,
    Quick::Italic,
    Quick::Bullets,
    Quick::Bold,
];

// ---- the canvas the text flows on ----

/// The box the page of the text stands in.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum FlowPaper {
    /// The page IS the window: the text on the window's own ground in a column of a comfortable
    /// measure, no edge, no shadow (the default: a mobile Markdown editor, a notes page).
    #[default]
    Plain,
    /// A sheet of paper on a canvas under it: an edge, a shadow, the sheet's own width (the print
    /// layout's A4 sheets, a web sheet).
    Sheet,
}

/// What the canvas needs from the app. Plain data: the app owns every state.
#[derive(Debug, Clone, PartialEq)]
pub struct FlowOptions {
    /// The DOM id of the scroller (an e2e script finds the canvas by it); empty for none.
    pub dom_id: String,
    /// A class of the page's box (the sheet): what a script selects one page by; empty for none.
    pub paper_class: String,
    /// What the page's box is.
    pub paper: FlowPaper,
    /// Declarations that go on the page's box and the placeholder: the paper's font, its ink.
    pub font_css: String,
    /// `Plain`: the widest the column of text is, px (0: the whole width). `Sheet`: the sheet's width.
    pub width: f32,
    /// The least height of the page's box, px (0: as tall as its text).
    pub min_height: f32,
    /// The room inside the page at each side, px (0: the default of the form factor).
    pub margin: f32,
    /// Room to leave under the last line, px: the keyboard's inset, so that the last line can be
    /// scrolled above it.
    pub lift: f32,
    /// Whether this is a phone's layout (narrower margins).
    pub phone: bool,
    /// Whether the document is blank: only then the page says where to write.
    pub blank: bool,
    /// What a blank page says ("Start writing..."), already in the window's language.
    pub placeholder: String,
    /// Whether the text takes the keyboard when it mounts: a desktop's yes, a phone's no (a tap
    /// in the text does, the soft keyboard covers half the screen).
    pub autofocus: bool,
}

impl Default for FlowOptions {
    fn default() -> Self {
        Self {
            dom_id: String::new(),
            paper_class: String::new(),
            paper: FlowPaper::Plain,
            font_css: String::new(),
            width: DEFAULT_MEASURE,
            min_height: 0.0,
            margin: 0.0,
            lift: 0.0,
            phone: false,
            blank: false,
            placeholder: String::new(),
            autofocus: false,
        }
    }
}

/// The widest a plain flow column is by default, px: about 70 characters of body text.
pub const DEFAULT_MEASURE: f32 = 760.0;

/// The room inside a plain page at each side on a phone, and on a desktop, px.
pub const MARGIN_PHONE: f32 = 20.0;
pub const MARGIN_DESKTOP: f32 = 40.0;

/// The most room under the last line the lift may ask for, px: no keyboard is taller than this
/// (a number that is not finite or is negative is none).
pub const LIFT_MAX: f32 = 900.0;

fn clean(px: f32) -> f32 {
    if px.is_finite() && px > 0.0 {
        px
    } else {
        0.0
    }
}

impl FlowOptions {
    /// The room inside the page at each side: the app's, else the form factor's default.
    #[must_use]
    pub fn margin_px(&self) -> f32 {
        let own = clean(self.margin);
        if own > 0.0 {
            own
        } else if self.phone {
            MARGIN_PHONE
        } else {
            MARGIN_DESKTOP
        }
    }

    /// The room under the last line, px: the page's margin and the lift (held to [`LIFT_MAX`]).
    #[must_use]
    pub fn bottom_px(&self) -> f32 {
        self.margin_px() + clean(self.lift).min(LIFT_MAX)
    }

    /// The scroller: a column that takes what its parent leaves and scrolls vertically. A sheet lies
    /// on a canvas of its own ground, a plain page on the window's.
    #[must_use]
    pub fn scroller_css(&self) -> String {
        let ground = match self.paper {
            FlowPaper::Plain => "system:background",
            FlowPaper::Sheet => "system:under-page-background",
        };
        let top = match self.paper {
            FlowPaper::Plain => 0.0,
            FlowPaper::Sheet => 18.0,
        };
        format!(
            "display: flex; flex-direction: column; align-items: center; flex-grow: 1; flex-shrink: 1; \
             min-height: 0px; min-width: 0px; overflow-y: auto; overflow-x: hidden; padding-top: {top}px; \
             background: {ground};"
        )
    }

    /// The page's box: the column of text (plain) or the sheet of paper.
    #[must_use]
    pub fn paper_css(&self) -> String {
        let margin = self.margin_px();
        let bottom = self.bottom_px();
        let font = self.font_css.trim();
        match self.paper {
            FlowPaper::Plain => {
                let max = if clean(self.width) > 0.0 {
                    format!(" max-width: {}px;", clean(self.width))
                } else {
                    String::new()
                };
                format!(
                    "display: flex; flex-direction: column; position: relative; flex-grow: 1; flex-shrink: 0; \
                     box-sizing: border-box; width: 100%;{max} min-height: {}px; padding: {margin}px {margin}px {bottom}px; \
                     color: system:text; {font}",
                    clean(self.min_height)
                )
            }
            FlowPaper::Sheet => format!(
                "display: block; position: relative; flex-shrink: 0; box-sizing: border-box; width: {}px; \
                 min-height: {}px; padding: {margin}px; margin-bottom: 16px; background: system:background; \
                 color: system:text; {font} border: 1px solid system:separator; \
                 box-shadow: 0px 1px 4px rgba(0, 0, 0, 0.25);",
                clean(self.width),
                clean(self.min_height)
            ),
        }
    }

    /// The placeholder: where the first character will stand, under the page's text (the editor
    /// above it is transparent and takes the touch).
    #[must_use]
    pub fn placeholder_css(&self) -> String {
        let margin = self.margin_px();
        let font = self.font_css.trim();
        format!(
            "position: absolute; left: {margin}px; top: {margin}px; display: flex; flex-direction: row; \
             align-items: center; color: system:tertiary-text; {font}"
        )
    }
}

#[cfg(test)]
#[path = "flow_rules_test.rs"]
mod flow_rules_test;
