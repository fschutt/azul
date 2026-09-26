//! The desktop's palette, in the shapes a widget's DARK twins take.
//!
//! Every constant here is a `system:` colour keyword packaged as a widget
//! style needs it - a background layer, a text colour, a border edge. It is
//! resolved when the node is painted, against the palette of the theme the
//! cascade evaluated (`DynamicSelectorContext::system_colors`), so a widget
//! that takes its dark surfaces, ink and rules from here matches the
//! desktop's own dark controls AND whatever the application paints around
//! it with the same keywords - instead of every widget carrying a second,
//! hand-picked dark palette that agrees with nothing.
//!
//! How widgets use it:
//!
//! * Light values never move: a widget keeps its established light colours
//!   and uses these only for the dark twin
//!   (`CssPropertyWithConditions::dark_theme`).
//! * A twin goes RIGHT AFTER its light value and before any `:hover` /
//!   `:active` / `:focus` rule for the same property. Inline declarations
//!   resolve last-match-wins, and a `dark_theme` declaration matches in every
//!   pseudo-state, so a resting twin pushed after a state rule would shadow
//!   it (see `widgets::theme_pairs`, which rejects the other half of the
//!   mistake).
//! * Which slot for which surface: fields and lists sit on
//!   [`CONTROL_BACKGROUND`], panels / cards / popups on
//!   [`WINDOW_BACKGROUND`], neutral buttons on [`BUTTON_FACE`], selected
//!   items on [`SELECTION_BACKGROUND`] with [`SELECTION_TEXT`]; text is
//!   [`TEXT`], captions and dim labels [`SECONDARY_TEXT`], rules and outlines
//!   [`SEPARATOR`], links [`LINK`], checked / active indicators [`ACCENT`]
//!   with [`ACCENT_TEXT`] on them.

use azul_css::{
    dynamic_selector::CssPropertyWithConditions,
    props::{
        basic::color::{ColorU, SystemColorRef},
        property::CssProperty,
        style::{
            StyleBackgroundContent, StyleBackgroundContentVec, StyleBorderBottomColor,
            StyleBorderLeftColor, StyleBorderRightColor, StyleBorderTopColor, StyleTextColor,
        },
    },
};

// ---------------------------------------------------------------------------
// Colour values, for `color` and `border-*-color`
// ---------------------------------------------------------------------------

/// `system:text` - primary label text.
pub const TEXT: ColorU = SystemColorRef::Text.to_color_token();
/// `system:secondary-text` - captions, field labels, dim text.
pub const SECONDARY_TEXT: ColorU = SystemColorRef::SecondaryText.to_color_token();
/// `system:tertiary-text` - the least prominent text.
pub const TERTIARY_TEXT: ColorU = SystemColorRef::TertiaryText.to_color_token();
/// `system:disabled-text` - a disabled control's label.
pub const DISABLED_TEXT: ColorU = SystemColorRef::DisabledText.to_color_token();
/// `system:placeholder-text` - the prompt in an empty field.
pub const PLACEHOLDER_TEXT: ColorU = SystemColorRef::PlaceholderText.to_color_token();
/// `system:button-text` - a push button's label.
pub const BUTTON_TEXT: ColorU = SystemColorRef::ButtonText.to_color_token();
/// `system:accent` - the user's accent, as a text or border colour.
pub const ACCENT: ColorU = SystemColorRef::Accent.to_color_token();
/// `system:accent-text` - text on an accent-coloured surface.
pub const ACCENT_TEXT: ColorU = SystemColorRef::AccentText.to_color_token();
/// `system:selection-text` - text on a selected item.
pub const SELECTION_TEXT: ColorU = SystemColorRef::SelectionText.to_color_token();
/// `system:link` - hyperlinks and link-like navigation.
pub const LINK: ColorU = SystemColorRef::Link.to_color_token();
/// `system:separator` - rules, dividers and control outlines.
pub const SEPARATOR: ColorU = SystemColorRef::Separator.to_color_token();
/// `system:grid` - table and grid lines.
pub const GRID: ColorU = SystemColorRef::Grid.to_color_token();

// ---------------------------------------------------------------------------
// Background layers
// ---------------------------------------------------------------------------

const WINDOW_BACKGROUND_LAYER: &[StyleBackgroundContent] = &[StyleBackgroundContent::SystemColor(
    SystemColorRef::WindowBackground,
)];
/// `system:window-background` - panels, cards, dialogs, popups.
pub const WINDOW_BACKGROUND: StyleBackgroundContentVec =
    StyleBackgroundContentVec::from_const_slice(WINDOW_BACKGROUND_LAYER);

const CONTROL_BACKGROUND_LAYER: &[StyleBackgroundContent] = &[StyleBackgroundContent::SystemColor(
    SystemColorRef::ControlBackground,
)];
/// `system:control-background` - text fields, lists, drop-downs.
pub const CONTROL_BACKGROUND: StyleBackgroundContentVec =
    StyleBackgroundContentVec::from_const_slice(CONTROL_BACKGROUND_LAYER);

const BUTTON_FACE_LAYER: &[StyleBackgroundContent] = &[StyleBackgroundContent::SystemColor(
    SystemColorRef::ButtonFace,
)];
/// `system:button-face` - a neutral push button.
pub const BUTTON_FACE: StyleBackgroundContentVec =
    StyleBackgroundContentVec::from_const_slice(BUTTON_FACE_LAYER);

const ACCENT_LAYER: &[StyleBackgroundContent] =
    &[StyleBackgroundContent::SystemColor(SystemColorRef::Accent)];
/// `system:accent` as a surface - a checked box, an active step, a thumb.
pub const ACCENT_BACKGROUND: StyleBackgroundContentVec =
    StyleBackgroundContentVec::from_const_slice(ACCENT_LAYER);

const SELECTION_BACKGROUND_LAYER: &[StyleBackgroundContent] = &[StyleBackgroundContent::SystemColor(
    SystemColorRef::SelectionBackground,
)];
/// `system:selection-background` - a selected item or row.
pub const SELECTION_BACKGROUND: StyleBackgroundContentVec =
    StyleBackgroundContentVec::from_const_slice(SELECTION_BACKGROUND_LAYER);

const SELECTION_BACKGROUND_INACTIVE_LAYER: &[StyleBackgroundContent] =
    &[StyleBackgroundContent::SystemColor(
        SystemColorRef::SelectionBackgroundInactive,
    )];
/// `system:selection-background-inactive` - a quiet neutral highlight: an
/// unfocused selection, a chip, a track.
pub const SELECTION_BACKGROUND_INACTIVE: StyleBackgroundContentVec =
    StyleBackgroundContentVec::from_const_slice(SELECTION_BACKGROUND_INACTIVE_LAYER);

const UNDER_PAGE_BACKGROUND_LAYER: &[StyleBackgroundContent] = &[StyleBackgroundContent::SystemColor(
    SystemColorRef::UnderPageBackground,
)];
/// `system:under-page-background` - the recessed canvas behind content.
pub const UNDER_PAGE_BACKGROUND: StyleBackgroundContentVec =
    StyleBackgroundContentVec::from_const_slice(UNDER_PAGE_BACKGROUND_LAYER);

// ---------------------------------------------------------------------------
// Ready-made dark twins (resting state, no pseudo-state condition)
// ---------------------------------------------------------------------------

/// Dark twin: the surface is `system:window-background`.
pub const DARK_WINDOW_BACKGROUND: CssPropertyWithConditions =
    CssPropertyWithConditions::dark_theme(CssProperty::const_background_content(
        WINDOW_BACKGROUND,
    ));
/// Dark twin: the surface is `system:control-background`.
pub const DARK_CONTROL_BACKGROUND: CssPropertyWithConditions =
    CssPropertyWithConditions::dark_theme(CssProperty::const_background_content(
        CONTROL_BACKGROUND,
    ));
/// Dark twin: the surface is `system:button-face`.
pub const DARK_BUTTON_FACE: CssPropertyWithConditions =
    CssPropertyWithConditions::dark_theme(CssProperty::const_background_content(BUTTON_FACE));
/// Dark twin: the surface is `system:accent`.
pub const DARK_ACCENT_BACKGROUND: CssPropertyWithConditions =
    CssPropertyWithConditions::dark_theme(CssProperty::const_background_content(
        ACCENT_BACKGROUND,
    ));
/// Dark twin: the surface is `system:selection-background`.
pub const DARK_SELECTION_BACKGROUND: CssPropertyWithConditions =
    CssPropertyWithConditions::dark_theme(CssProperty::const_background_content(
        SELECTION_BACKGROUND,
    ));
/// Dark twin: the surface is `system:selection-background-inactive`.
pub const DARK_SELECTION_BACKGROUND_INACTIVE: CssPropertyWithConditions =
    CssPropertyWithConditions::dark_theme(CssProperty::const_background_content(
        SELECTION_BACKGROUND_INACTIVE,
    ));

/// Dark twin: the text is `system:text`.
pub const DARK_TEXT: CssPropertyWithConditions = dark_text(TEXT);
/// Dark twin: the text is `system:secondary-text`.
pub const DARK_SECONDARY_TEXT: CssPropertyWithConditions = dark_text(SECONDARY_TEXT);
/// Dark twin: the text is `system:disabled-text`.
pub const DARK_DISABLED_TEXT: CssPropertyWithConditions = dark_text(DISABLED_TEXT);
/// Dark twin: the text is `system:button-text`.
pub const DARK_BUTTON_TEXT: CssPropertyWithConditions = dark_text(BUTTON_TEXT);
/// Dark twin: the text is `system:accent-text`.
pub const DARK_ACCENT_TEXT: CssPropertyWithConditions = dark_text(ACCENT_TEXT);
/// Dark twin: the text is `system:selection-text`.
pub const DARK_SELECTION_TEXT: CssPropertyWithConditions = dark_text(SELECTION_TEXT);
/// Dark twin: the text is `system:link`.
pub const DARK_LINK: CssPropertyWithConditions = dark_text(LINK);

/// Dark twin: the top border edge is `system:separator`.
pub const DARK_SEPARATOR_BORDER_TOP: CssPropertyWithConditions = dark_border_top(SEPARATOR);
/// Dark twin: the right border edge is `system:separator`.
pub const DARK_SEPARATOR_BORDER_RIGHT: CssPropertyWithConditions = dark_border_right(SEPARATOR);
/// Dark twin: the bottom border edge is `system:separator`.
pub const DARK_SEPARATOR_BORDER_BOTTOM: CssPropertyWithConditions =
    dark_border_bottom(SEPARATOR);
/// Dark twin: the left border edge is `system:separator`.
pub const DARK_SEPARATOR_BORDER_LEFT: CssPropertyWithConditions = dark_border_left(SEPARATOR);

/// Dark twin for `color`.
#[must_use]
pub const fn dark_text(color: ColorU) -> CssPropertyWithConditions {
    CssPropertyWithConditions::dark_theme(CssProperty::const_text_color(StyleTextColor {
        inner: color,
    }))
}

/// Dark twin for `border-top-color`.
#[must_use]
pub const fn dark_border_top(color: ColorU) -> CssPropertyWithConditions {
    CssPropertyWithConditions::dark_theme(CssProperty::const_border_top_color(
        StyleBorderTopColor { inner: color },
    ))
}

/// Dark twin for `border-right-color`.
#[must_use]
pub const fn dark_border_right(color: ColorU) -> CssPropertyWithConditions {
    CssPropertyWithConditions::dark_theme(CssProperty::const_border_right_color(
        StyleBorderRightColor { inner: color },
    ))
}

/// Dark twin for `border-bottom-color`.
#[must_use]
pub const fn dark_border_bottom(color: ColorU) -> CssPropertyWithConditions {
    CssPropertyWithConditions::dark_theme(CssProperty::const_border_bottom_color(
        StyleBorderBottomColor { inner: color },
    ))
}

/// Dark twin for `border-left-color`.
#[must_use]
pub const fn dark_border_left(color: ColorU) -> CssPropertyWithConditions {
    CssPropertyWithConditions::dark_theme(CssProperty::const_border_left_color(
        StyleBorderLeftColor { inner: color },
    ))
}

/// Dark twins for all four border edges, top / right / bottom / left - the
/// order a widget declares its light edges in.
#[must_use]
pub const fn dark_border(color: ColorU) -> [CssPropertyWithConditions; 4] {
    [
        dark_border_top(color),
        dark_border_right(color),
        dark_border_bottom(color),
        dark_border_left(color),
    ]
}

/// Dark twin for `background`, a `system:` surface picked at run time.
#[must_use]
pub fn dark_background(slot: SystemColorRef) -> CssPropertyWithConditions {
    CssPropertyWithConditions::dark_theme(CssProperty::const_background_content(
        StyleBackgroundContentVec::from_vec(alloc::vec![StyleBackgroundContent::SystemColor(
            slot,
        )]),
    ))
}

/// Dark twin for `background`, a concrete colour - for the surfaces with no
/// system slot (the semantic info / success / warning / danger tints).
#[must_use]
pub fn dark_background_color(color: ColorU) -> CssPropertyWithConditions {
    CssPropertyWithConditions::dark_theme(CssProperty::const_background_content(
        StyleBackgroundContentVec::from_vec(alloc::vec![StyleBackgroundContent::Color(color)]),
    ))
}
