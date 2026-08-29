//! UI font override.
//!
//! WORKAROUND(engine, see ENGINE-ISSUES.md #1): `font-family: system:ui`
//! resolves through the Linux fallback chain Cantarell -> Ubuntu -> ... .
//! On this machine Cantarell is absent and Ubuntu ships as a VARIABLE font
//! (`Ubuntu[wdth,wght].ttf`); once azul's async font registry finishes
//! scanning, every `system:ui` text re-resolves onto that face and renders
//! as .notdef boxes. Until the engine bakes disk-loaded variable fonts the
//! way `FontManager::register_named_font` already does for memory fonts,
//! the app pins an installed STATIC family everywhere.
//!
//! "Liberation Sans" is metric-compatible with Arial — the closest
//! stand-in for the Office-2013-era Segoe UI that ships on stock Ubuntu.

use azul::css::{
    ColorU, CssProperty, CssPropertyWithConditions, LayoutMarginBottom, LayoutMarginTop,
    PixelValue, StyleFontFamily, StyleFontSize, StyleTextColor,
};
use azul::dom::Dom;
use azul::vec::{CssPropertyWithConditionsVec, StyleFontFamilyVec};

/// The pinned UI family, as a CSS string fragment for `with_css` blocks.
pub const UI_FONT_CSS: &str = "font-family: \"Liberation Sans\";";

/// One `font-family: "Liberation Sans"` declaration.
fn ui_font_cond() -> CssPropertyWithConditions {
    CssPropertyWithConditions::simple(CssProperty::const_font_family(
        StyleFontFamilyVec::from(vec![StyleFontFamily::System("Liberation Sans".into())]),
    ))
}

/// Appends the pinned family to a widget part style. Inline properties
/// resolve last-match-wins, so the append overrides the widget's
/// `system:ui` without rebuilding the style bundle.
pub fn push_ui_font(style: &mut CssPropertyWithConditionsVec) {
    let mut v: Vec<CssPropertyWithConditions> = style.as_ref().to_vec();
    v.push(ui_font_cond());
    *style = CssPropertyWithConditionsVec::from(v);
}

// -- the Office-2013-era look palette shared by the app-side compositions --

/// Office 2013 accent blue (#2B579A).
pub const OFFICE_BLUE: ColorU = ColorU {
    r: 43,
    g: 87,
    b: 154,
    a: 255,
};
/// Regular chrome text (#444444).
pub const TEXT: ColorU = ColorU {
    r: 68,
    g: 68,
    b: 68,
    a: 255,
};
/// Secondary gray (#808080).
pub const TEXT_GRAY: ColorU = ColorU {
    r: 128,
    g: 128,
    b: 128,
    a: 255,
};
/// Faint gray (#949494).
pub const TEXT_FAINT: ColorU = ColorU {
    r: 148,
    g: 148,
    b: 148,
    a: 255,
};
/// Backstage pane titles (#565656).
pub const TITLE_GRAY: ColorU = ColorU {
    r: 86,
    g: 86,
    b: 86,
    a: 255,
};
pub const WHITE: ColorU = ColorU {
    r: 255,
    g: 255,
    b: 255,
    a: 255,
};

/// A `<p>` label with programmatic font-size + color (+ the pinned family).
///
/// The label is a real block box, not a bare text node: azul does not wrap a
/// raw text run in an anonymous block the way browsers do, so a text node has
/// no box and every property put on it is inert. (That is the whole of the
/// old "inline `with_css` on TEXT nodes drops declarations unpredictably"
/// note — the declarations were never dropped, they had nowhere to apply.)
/// Every app-side label goes through this helper; wrapping DIVs still own
/// margins and layout.
pub fn text(contents: &str, size_px: isize, color: ColorU) -> Dom {
    Dom::create_p_with_text(contents).with_css_props(CssPropertyWithConditionsVec::from(vec![
        CssPropertyWithConditions::simple(CssProperty::const_font_size(StyleFontSize::px(
            size_px as f32,
        ))),
        CssPropertyWithConditions::simple(CssProperty::const_text_color(StyleTextColor {
            inner: color,
        })),
        ui_font_cond(),
        // `<p>` carries the UA 1em block margins; the bare text node this
        // helper used to return carried none, and every margin in this app
        // belongs to the wrapping DIVs.
        CssPropertyWithConditions::simple(CssProperty::const_margin_top(LayoutMarginTop {
            inner: PixelValue::zero(),
        })),
        CssPropertyWithConditions::simple(CssProperty::const_margin_bottom(LayoutMarginBottom {
            inner: PixelValue::zero(),
        })),
    ]))
}
