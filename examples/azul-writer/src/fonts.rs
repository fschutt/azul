use azul::{
    css::{
        ColorU, CssProperty, CssPropertyWithConditions, LayoutMarginBottom, LayoutMarginTop,
        PixelValue, StyleFontFamily, StyleFontSize, StyleTextColor,
    },
    dom::Dom,
    option::OptionCssPropertyWithConditionsVec,
    vec::{CssPropertyWithConditionsVec, StyleFontFamilyVec},
};

pub const UI_FONT_CSS: &str = "font-family: \"Liberation Sans\";";

fn ui_font_cond() -> CssPropertyWithConditions {
    CssPropertyWithConditions::simple(CssProperty::const_font_family(StyleFontFamilyVec::from(
        vec![StyleFontFamily::System("Liberation Sans".into())],
    )))
}

pub fn push_ui_font(
    style: &mut OptionCssPropertyWithConditionsVec,
    default: CssPropertyWithConditionsVec,
) {
    let base = match style {
        OptionCssPropertyWithConditionsVec::Some(explicit) => explicit.clone(),
        OptionCssPropertyWithConditionsVec::None => default,
    };
    let mut v: Vec<CssPropertyWithConditions> = base.as_ref().to_vec();
    v.push(ui_font_cond());
    *style = OptionCssPropertyWithConditionsVec::Some(CssPropertyWithConditionsVec::from(v));
}

pub fn text(contents: &str, size_px: isize, color: ColorU) -> Dom {
    Dom::create_p_with_text(contents).with_css_props(CssPropertyWithConditionsVec::from(vec![
        CssPropertyWithConditions::simple(CssProperty::const_font_size(StyleFontSize::px(
            size_px as f32,
        ))),
        CssPropertyWithConditions::simple(CssProperty::const_text_color(StyleTextColor {
            inner: color,
        })),
        ui_font_cond(),
        CssPropertyWithConditions::simple(CssProperty::const_margin_top(LayoutMarginTop {
            inner: PixelValue::zero(),
        })),
        CssPropertyWithConditions::simple(CssProperty::const_margin_bottom(LayoutMarginBottom {
            inner: PixelValue::zero(),
        })),
    ]))
}
