//! Module for printing the CSS to Rust code

use azul_css::*;

pub fn css_to_rust_code(css: &Css) -> String {

    let mut output = String::new();
    let mut tabs = 0;

    output.push_str("const CSS: Css = Css {\r\n");
    output.push_str("\tstylesheets: [\r\n");

    for stylesheet in &css.stylesheets {
        
        output.push_str("\t\tStylesheet {\r\n");
        output.push_str("\t\t\trules: [\r\n");

        for block in &stylesheet.rules {
            output.push_str("\t\t\t\tCssRuleBlock: {\r\n");
            output.push_str(&format!("\t\t\t\t\tpath: {:?},\r\n", block.path));

            output.push_str("\t\t\t\t\tdeclarations: [\r\n");
            
            for declaration in &block.declarations {
                output.push_str(&format!("\t\t\t\t\t\t{},\r\n", print_declaraction(declaration, 6)));
            }

            output.push_str("\t\t\t\t\t]\r\n");

            output.push_str("\t\t\t\t},\r\n");
        }

        output.push_str("\t\t\t]\r\n");
        output.push_str("\t\t},\r\n");
    }

    output.push_str("\t]\r\n");
    output.push_str("};");

    let output = output.replace("\t", "    ");

    output
}

fn print_declaraction(decl: &CssDeclaration, tabs: usize) -> String {
    match decl {
        CssDeclaration::Static(s) => format!("CssDeclaration::Static({})", format_static_css_prop(s, tabs)),
        CssDeclaration::Dynamic(d) => format!("CssDeclaration::Dynamic({})", format_dynamic_css_prop(d, tabs)),
    }
}

trait FormatAsRustCode {
    fn format_as_rust_code(&self, tabs: usize) -> String;
}

fn print_css_property_value<T: FormatAsRustCode>(prop_val: &CssPropertyValue<T>, tabs: usize) -> String {
    match prop_val {
        CssPropertyValue::Auto => format!("CssPropertyValue::Auto"),
        CssPropertyValue::None => format!("CssPropertyValue::None"),
        CssPropertyValue::Initial => format!("CssPropertyValue::Initial"),
        CssPropertyValue::Inherit => format!("CssPropertyValue::Inherit"),
        CssPropertyValue::Exact(t) => format!("CssPropertyValue::Exact({})", t.format_as_rust_code(tabs)),
    }
}

fn format_static_css_prop(prop: &CssProperty, tabs: usize) -> String {
    match prop {
        CssProperty::TextColor(p) => format!("CssProperty::TextColor({})", print_css_property_value(p, tabs)),
        CssProperty::FontSize(p) => format!("CssProperty::FontSize({})", print_css_property_value(p, tabs)),
        CssProperty::FontFamily(p) => format!("CssProperty::FontFamily({})", print_css_property_value(p, tabs)),
        CssProperty::TextAlign(p) => format!("CssProperty::TextAlign({})", print_css_property_value(p, tabs)),
        CssProperty::LetterSpacing(p) => format!("CssProperty::LetterSpacing({})", print_css_property_value(p, tabs)),
        CssProperty::LineHeight(p) => format!("CssProperty::LineHeight({})", print_css_property_value(p, tabs)),
        CssProperty::WordSpacing(p) => format!("CssProperty::WordSpacing({})", print_css_property_value(p, tabs)),
        CssProperty::TabWidth(p) => format!("CssProperty::TabWidth({})", print_css_property_value(p, tabs)),
        CssProperty::Cursor(p) => format!("CssProperty::Cursor({})", print_css_property_value(p, tabs)),
        CssProperty::Display(p) => format!("CssProperty::Display({})", print_css_property_value(p, tabs)),
        CssProperty::Float(p) => format!("CssProperty::Float({})", print_css_property_value(p, tabs)),
        CssProperty::BoxSizing(p) => format!("CssProperty::BoxSizing({})", print_css_property_value(p, tabs)),
        CssProperty::Width(p) => format!("CssProperty::Width({})", print_css_property_value(p, tabs)),
        CssProperty::Height(p) => format!("CssProperty::Height({})", print_css_property_value(p, tabs)),
        CssProperty::MinWidth(p) => format!("CssProperty::MinWidth({})", print_css_property_value(p, tabs)),
        CssProperty::MinHeight(p) => format!("CssProperty::MinHeight({})", print_css_property_value(p, tabs)),
        CssProperty::MaxWidth(p) => format!("CssProperty::MaxWidth({})", print_css_property_value(p, tabs)),
        CssProperty::MaxHeight(p) => format!("CssProperty::MaxHeight({})", print_css_property_value(p, tabs)),
        CssProperty::Position(p) => format!("CssProperty::Position({})", print_css_property_value(p, tabs)),
        CssProperty::Top(p) => format!("CssProperty::Top({})", print_css_property_value(p, tabs)),
        CssProperty::Right(p) => format!("CssProperty::Right({})", print_css_property_value(p, tabs)),
        CssProperty::Left(p) => format!("CssProperty::Left({})", print_css_property_value(p, tabs)),
        CssProperty::Bottom(p) => format!("CssProperty::Bottom({})", print_css_property_value(p, tabs)),
        CssProperty::FlexWrap(p) => format!("CssProperty::FlexWrap({})", print_css_property_value(p, tabs)),
        CssProperty::FlexDirection(p) => format!("CssProperty::FlexDirection({})", print_css_property_value(p, tabs)),
        CssProperty::FlexGrow(p) => format!("CssProperty::FlexGrow({})", print_css_property_value(p, tabs)),
        CssProperty::FlexShrink(p) => format!("CssProperty::FlexShrink({})", print_css_property_value(p, tabs)),
        CssProperty::JustifyContent(p) => format!("CssProperty::JustifyContent({})", print_css_property_value(p, tabs)),
        CssProperty::AlignItems(p) => format!("CssProperty::AlignItems({})", print_css_property_value(p, tabs)),
        CssProperty::AlignContent(p) => format!("CssProperty::AlignContent({})", print_css_property_value(p, tabs)),
        CssProperty::BackgroundContent(p) => format!("CssProperty::BackgroundContent({})", print_css_property_value(p, tabs)),
        CssProperty::BackgroundPosition(p) => format!("CssProperty::BackgroundPosition({})", print_css_property_value(p, tabs)),
        CssProperty::BackgroundSize(p) => format!("CssProperty::BackgroundSize({})", print_css_property_value(p, tabs)),
        CssProperty::BackgroundRepeat(p) => format!("CssProperty::BackgroundRepeat({})", print_css_property_value(p, tabs)),
        CssProperty::OverflowX(p) => format!("CssProperty::OverflowX({})", print_css_property_value(p, tabs)),
        CssProperty::OverflowY(p) => format!("CssProperty::OverflowY({})", print_css_property_value(p, tabs)),
        CssProperty::PaddingTop(p) => format!("CssProperty::PaddingTop({})", print_css_property_value(p, tabs)),
        CssProperty::PaddingLeft(p) => format!("CssProperty::PaddingLeft({})", print_css_property_value(p, tabs)),
        CssProperty::PaddingRight(p) => format!("CssProperty::PaddingRight({})", print_css_property_value(p, tabs)),
        CssProperty::PaddingBottom(p) => format!("CssProperty::PaddingBottom({})", print_css_property_value(p, tabs)),
        CssProperty::MarginTop(p) => format!("CssProperty::MarginTop({})", print_css_property_value(p, tabs)),
        CssProperty::MarginLeft(p) => format!("CssProperty::MarginLeft({})", print_css_property_value(p, tabs)),
        CssProperty::MarginRight(p) => format!("CssProperty::MarginRight({})", print_css_property_value(p, tabs)),
        CssProperty::MarginBottom(p) => format!("CssProperty::MarginBottom({})", print_css_property_value(p, tabs)),
        CssProperty::BorderTopLeftRadius(p) => format!("CssProperty::BorderTopLeftRadius({})", print_css_property_value(p, tabs)),
        CssProperty::BorderTopRightRadius(p) => format!("CssProperty::BorderTopRightRadius({})", print_css_property_value(p, tabs)),
        CssProperty::BorderBottomLeftRadius(p) => format!("CssProperty::BorderBottomLeftRadius({})", print_css_property_value(p, tabs)),
        CssProperty::BorderBottomRightRadius(p) => format!("CssProperty::BorderBottomRightRadius({})", print_css_property_value(p, tabs)),
        CssProperty::BorderTopColor(p) => format!("CssProperty::BorderTopColor({})", print_css_property_value(p, tabs)),
        CssProperty::BorderRightColor(p) => format!("CssProperty::BorderRightColor({})", print_css_property_value(p, tabs)),
        CssProperty::BorderLeftColor(p) => format!("CssProperty::BorderLeftColor({})", print_css_property_value(p, tabs)),
        CssProperty::BorderBottomColor(p) => format!("CssProperty::BorderBottomColor({})", print_css_property_value(p, tabs)),
        CssProperty::BorderTopStyle(p) => format!("CssProperty::BorderTopStyle({})", print_css_property_value(p, tabs)),
        CssProperty::BorderRightStyle(p) => format!("CssProperty::BorderRightStyle({})", print_css_property_value(p, tabs)),
        CssProperty::BorderLeftStyle(p) => format!("CssProperty::BorderLeftStyle({})", print_css_property_value(p, tabs)),
        CssProperty::BorderBottomStyle(p) => format!("CssProperty::BorderBottomStyle({})", print_css_property_value(p, tabs)),
        CssProperty::BorderTopWidth(p) => format!("CssProperty::BorderTopWidth({})", print_css_property_value(p, tabs)),
        CssProperty::BorderRightWidth(p) => format!("CssProperty::BorderRightWidth({})", print_css_property_value(p, tabs)),
        CssProperty::BorderLeftWidth(p) => format!("CssProperty::BorderLeftWidth({})", print_css_property_value(p, tabs)),
        CssProperty::BorderBottomWidth(p) => format!("CssProperty::BorderBottomWidth({})", print_css_property_value(p, tabs)),
        CssProperty::BoxShadowLeft(p) => format!("CssProperty::BoxShadowLeft({})", print_css_property_value(p, tabs)),
        CssProperty::BoxShadowRight(p) => format!("CssProperty::BoxShadowRight({})", print_css_property_value(p, tabs)),
        CssProperty::BoxShadowTop(p) => format!("CssProperty::BoxShadowTop({})", print_css_property_value(p, tabs)),
        CssProperty::BoxShadowBottom(p) => format!("CssProperty::BoxShadowBottom({})", print_css_property_value(p, tabs)),
    }
}

fn format_dynamic_css_prop(decl: &DynamicCssProperty, tabs: usize) -> String {
    let t = String::from("    ").repeat(tabs);
    format!("DynamicCssProperty {{\r\n{}    dynamic_id: {:?},\r\n{}    default_value: {},\r\n{}}}", 
        t, decl.dynamic_id, t, format_static_css_prop(&decl.default_value, tabs + 1), t)
}


impl FormatAsRustCode for StyleTextColor { 
    fn format_as_rust_code(&self, tabs: usize) -> String {
        format!("{:?}", self)
    }
}

impl FormatAsRustCode for StyleFontSize { 
    fn format_as_rust_code(&self, tabs: usize) -> String {
        format!("{:?}", self)
    }
}

impl FormatAsRustCode for StyleFontFamily { 
    fn format_as_rust_code(&self, tabs: usize) -> String {
        format!("{:?}", self)
    }
}

impl FormatAsRustCode for StyleTextAlignmentHorz { 
    fn format_as_rust_code(&self, tabs: usize) -> String {
        format!("{:?}", self)
    }
}

impl FormatAsRustCode for StyleLetterSpacing { 
    fn format_as_rust_code(&self, tabs: usize) -> String {
        format!("{:?}", self)
    }
}

impl FormatAsRustCode for StyleLineHeight { 
    fn format_as_rust_code(&self, tabs: usize) -> String {
        format!("{:?}", self)
    }
}

impl FormatAsRustCode for StyleWordSpacing { 
    fn format_as_rust_code(&self, tabs: usize) -> String {
        format!("{:?}", self)
    }
}

impl FormatAsRustCode for StyleTabWidth { 
    fn format_as_rust_code(&self, tabs: usize) -> String {
        format!("{:?}", self)
    }
}

impl FormatAsRustCode for StyleCursor { 
    fn format_as_rust_code(&self, tabs: usize) -> String {
        format!("{:?}", self)
    }
}

impl FormatAsRustCode for LayoutDisplay { 
    fn format_as_rust_code(&self, tabs: usize) -> String {
        format!("{:?}", self)
    }
}

impl FormatAsRustCode for LayoutFloat { 
    fn format_as_rust_code(&self, tabs: usize) -> String {
        format!("{:?}", self)
    }
}

impl FormatAsRustCode for LayoutBoxSizing { 
    fn format_as_rust_code(&self, tabs: usize) -> String {
        format!("{:?}", self)
    }
}

impl FormatAsRustCode for LayoutWidth { 
    fn format_as_rust_code(&self, tabs: usize) -> String {
        format!("{:?}", self)
    }
}

impl FormatAsRustCode for LayoutHeight { 
    fn format_as_rust_code(&self, tabs: usize) -> String {
        format!("{:?}", self)
    }
}

impl FormatAsRustCode for LayoutMinWidth { 
    fn format_as_rust_code(&self, tabs: usize) -> String {
        format!("{:?}", self)
    }
}

impl FormatAsRustCode for LayoutMinHeight { 
    fn format_as_rust_code(&self, tabs: usize) -> String {
        format!("{:?}", self)
    }
}

impl FormatAsRustCode for LayoutMaxWidth { 
    fn format_as_rust_code(&self, tabs: usize) -> String {
        format!("{:?}", self)
    }
}

impl FormatAsRustCode for LayoutMaxHeight { 
    fn format_as_rust_code(&self, tabs: usize) -> String {
        format!("{:?}", self)
    }
}

impl FormatAsRustCode for LayoutPosition { 
    fn format_as_rust_code(&self, tabs: usize) -> String {
        format!("{:?}", self)
    }
}

impl FormatAsRustCode for LayoutTop { 
    fn format_as_rust_code(&self, tabs: usize) -> String {
        format!("{:?}", self)
    }
}

impl FormatAsRustCode for LayoutRight { 
    fn format_as_rust_code(&self, tabs: usize) -> String {
        format!("{:?}", self)
    }
}

impl FormatAsRustCode for LayoutLeft { 
    fn format_as_rust_code(&self, tabs: usize) -> String {
        format!("{:?}", self)
    }
}

impl FormatAsRustCode for LayoutBottom { 
    fn format_as_rust_code(&self, tabs: usize) -> String {
        format!("{:?}", self)
    }
}

impl FormatAsRustCode for LayoutWrap { 
    fn format_as_rust_code(&self, tabs: usize) -> String {
        format!("{:?}", self)
    }
}

impl FormatAsRustCode for LayoutDirection { 
    fn format_as_rust_code(&self, tabs: usize) -> String {
        format!("{:?}", self)
    }
}

impl FormatAsRustCode for LayoutFlexGrow { 
    fn format_as_rust_code(&self, tabs: usize) -> String {
        format!("{:?}", self)
    }
}

impl FormatAsRustCode for LayoutFlexShrink { 
    fn format_as_rust_code(&self, tabs: usize) -> String {
        format!("{:?}", self)
    }
}

impl FormatAsRustCode for LayoutJustifyContent { 
    fn format_as_rust_code(&self, tabs: usize) -> String {
        format!("{:?}", self)
    }
}

impl FormatAsRustCode for LayoutAlignItems { 
    fn format_as_rust_code(&self, tabs: usize) -> String {
        format!("{:?}", self)
    }
}

impl FormatAsRustCode for LayoutAlignContent { 
    fn format_as_rust_code(&self, tabs: usize) -> String {
        format!("{:?}", self)
    }
}

impl FormatAsRustCode for StyleBackgroundContent { 
    fn format_as_rust_code(&self, tabs: usize) -> String {
        format!("{:?}", self)
    }
}

impl FormatAsRustCode for StyleBackgroundPosition { 
    fn format_as_rust_code(&self, tabs: usize) -> String {
        format!("{:?}", self)
    }
}

impl FormatAsRustCode for StyleBackgroundSize { 
    fn format_as_rust_code(&self, tabs: usize) -> String {
        format!("{:?}", self)
    }
}

impl FormatAsRustCode for StyleBackgroundRepeat { 
    fn format_as_rust_code(&self, tabs: usize) -> String {
        format!("{:?}", self)
    }
}

impl FormatAsRustCode for Overflow { 
    fn format_as_rust_code(&self, tabs: usize) -> String {
        format!("{:?}", self)
    }
}

impl FormatAsRustCode for LayoutPaddingTop { 
    fn format_as_rust_code(&self, tabs: usize) -> String {
        format!("{:?}", self)
    }
}

impl FormatAsRustCode for LayoutPaddingLeft { 
    fn format_as_rust_code(&self, tabs: usize) -> String {
        format!("{:?}", self)
    }
}

impl FormatAsRustCode for LayoutPaddingRight { 
    fn format_as_rust_code(&self, tabs: usize) -> String {
        format!("{:?}", self)
    }
}

impl FormatAsRustCode for LayoutPaddingBottom { 
    fn format_as_rust_code(&self, tabs: usize) -> String {
        format!("{:?}", self)
    }
}

impl FormatAsRustCode for LayoutMarginTop { 
    fn format_as_rust_code(&self, tabs: usize) -> String {
        format!("{:?}", self)
    }
}

impl FormatAsRustCode for LayoutMarginLeft { 
    fn format_as_rust_code(&self, tabs: usize) -> String {
        format!("{:?}", self)
    }
}

impl FormatAsRustCode for LayoutMarginRight { 
    fn format_as_rust_code(&self, tabs: usize) -> String {
        format!("{:?}", self)
    }
}

impl FormatAsRustCode for LayoutMarginBottom { 
    fn format_as_rust_code(&self, tabs: usize) -> String {
        format!("{:?}", self)
    }
}

impl FormatAsRustCode for StyleBorderTopLeftRadius { 
    fn format_as_rust_code(&self, tabs: usize) -> String {
        format!("{:?}", self)
    }
}

impl FormatAsRustCode for StyleBorderTopRightRadius { 
    fn format_as_rust_code(&self, tabs: usize) -> String {
        format!("{:?}", self)
    }
}

impl FormatAsRustCode for StyleBorderBottomLeftRadius { 
    fn format_as_rust_code(&self, tabs: usize) -> String {
        format!("{:?}", self)
    }
}

impl FormatAsRustCode for StyleBorderBottomRightRadius { 
    fn format_as_rust_code(&self, tabs: usize) -> String {
        format!("{:?}", self)
    }
}

impl FormatAsRustCode for StyleBorderTopColor { 
    fn format_as_rust_code(&self, tabs: usize) -> String {
        format!("{:?}", self)
    }
}

impl FormatAsRustCode for StyleBorderRightColor { 
    fn format_as_rust_code(&self, tabs: usize) -> String {
        format!("{:?}", self)
    }
}

impl FormatAsRustCode for StyleBorderLeftColor { 
    fn format_as_rust_code(&self, tabs: usize) -> String {
        format!("{:?}", self)
    }
}

impl FormatAsRustCode for StyleBorderBottomColor { 
    fn format_as_rust_code(&self, tabs: usize) -> String {
        format!("{:?}", self)
    }
}

impl FormatAsRustCode for StyleBorderTopStyle { 
    fn format_as_rust_code(&self, tabs: usize) -> String {
        format!("{:?}", self)
    }
}

impl FormatAsRustCode for StyleBorderRightStyle { 
    fn format_as_rust_code(&self, tabs: usize) -> String {
        format!("{:?}", self)
    }
}

impl FormatAsRustCode for StyleBorderLeftStyle { 
    fn format_as_rust_code(&self, tabs: usize) -> String {
        format!("{:?}", self)
    }
}

impl FormatAsRustCode for StyleBorderBottomStyle { 
    fn format_as_rust_code(&self, tabs: usize) -> String {
        format!("{:?}", self)
    }
}

impl FormatAsRustCode for StyleBorderTopWidth { 
    fn format_as_rust_code(&self, tabs: usize) -> String {
        format!("{:?}", self)
    }
}

impl FormatAsRustCode for StyleBorderRightWidth { 
    fn format_as_rust_code(&self, tabs: usize) -> String {
        format!("{:?}", self)
    }
}

impl FormatAsRustCode for StyleBorderLeftWidth { 
    fn format_as_rust_code(&self, tabs: usize) -> String {
        format!("{:?}", self)
    }
}

impl FormatAsRustCode for StyleBorderBottomWidth { 
    fn format_as_rust_code(&self, tabs: usize) -> String {
        format!("{:?}", self)
    }
}

impl FormatAsRustCode for BoxShadowPreDisplayItem { 
    fn format_as_rust_code(&self, tabs: usize) -> String {
        let t = String::from("    ").repeat(tabs);
        format!("BoxShadowPreDisplayItem {{\r\n{}    offset: {:?}\r\n{}    color: {:?}\r\n{}    blur_radius: {:?}\r\n{}    spread_radius: {:?}\r\n{}    clip_mode: ClipMode::{:?}\r\n{}}}", 
            t, self.offset, t, self.color, t, self.blur_radius, t, self.spread_radius, t, self.clip_mode, t)
    }
}

