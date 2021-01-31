//! Module for printing the CSS to Rust code

use azul_css::*;

pub fn css_to_rust_code(css: &Css) -> String {

    let mut output = String::new();

    output.push_str("const CSS: Css = Css {\r\n");
    output.push_str("\tstylesheets: [\r\n");

    for stylesheet in css.stylesheets.iter() {

        output.push_str("\t\tStylesheet {\r\n");
        output.push_str("\t\t\trules: [\r\n");

        for block in stylesheet.rules.iter() {

            output.push_str("\t\t\t\tCssRuleBlock: {\r\n");
            output.push_str(&format!("\t\t\t\t\tpath: {},\r\n", print_block_path(&block.path, 5)));

            output.push_str("\t\t\t\t\tdeclarations: [\r\n");

            for declaration in block.declarations.iter() {
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

fn print_block_path(path: &CssPath, tabs: usize) -> String {
    let t = String::from("    ").repeat(tabs);
    let t1 = String::from("    ").repeat(tabs + 1);

    format!("CssPath {{\r\n{}selectors: {}\r\n{}}}", t1, format_selectors(path.selectors.as_ref(), tabs + 1), t)
}

fn format_selectors(selectors: &[CssPathSelector], tabs: usize) -> String {
    let t = String::from("    ").repeat(tabs);
    let t1 = String::from("    ").repeat(tabs + 1);

    let selectors_formatted = selectors.iter()
    .map(|s| format!("{}{},", t1, format_single_selector(s, tabs + 1)))
    .collect::<Vec<String>>()
    .join("\r\n");

    format!("vec![\r\n{}\r\n{}].into()", selectors_formatted, t)
}

fn format_single_selector(p: &CssPathSelector, _tabs: usize) -> String {
    match p {
        CssPathSelector::Global => format!("CssPathSelector::Global"),
        CssPathSelector::Type(ntp) => format!("CssPathSelector::Type({})", format_node_type(ntp)),
        CssPathSelector::Class(class) => format!("CssPathSelector::Class(String::from({:?}))", class),
        CssPathSelector::Id(id) => format!("CssPathSelector::Id(String::from({:?}))", id),
        CssPathSelector::PseudoSelector(cps) => format!("CssPathSelector::PseudoSelector({})", format_pseudo_selector_type(cps)),
        CssPathSelector::DirectChildren => format!("CssPathSelector::DirectChildren"),
        CssPathSelector::Children => format!("CssPathSelector::Children"),
    }
}

fn format_node_type(n: &NodeTypePath) -> &'static str {
    match n {
        NodeTypePath::Body => "NodeTypePath::Body",
        NodeTypePath::Div => "NodeTypePath::Div",
        NodeTypePath::Br => "NodeTypePath::Br",
        NodeTypePath::P => "NodeTypePath::P",
        NodeTypePath::Img => "NodeTypePath::Img",
        NodeTypePath::Texture => "NodeTypePath::Texture",
        NodeTypePath::IFrame => "NodeTypePath::IFrame",
    }
}

fn format_pseudo_selector_type(p: &CssPathPseudoSelector) -> String {
    match p {
        CssPathPseudoSelector::First => format!("CssPathPseudoSelector::First"),
        CssPathPseudoSelector::Last => format!("CssPathPseudoSelector::Last"),
        CssPathPseudoSelector::NthChild(n) => format!("CssPathPseudoSelector::NthChild({})", format_nth_child_selector(n)),
        CssPathPseudoSelector::Hover => format!("CssPathPseudoSelector::Hover"),
        CssPathPseudoSelector::Active => format!("CssPathPseudoSelector::Active"),
        CssPathPseudoSelector::Focus => format!("CssPathPseudoSelector::Focus"),
    }
}

fn format_nth_child_selector(n: &CssNthChildSelector) -> String {
    match n {
        CssNthChildSelector::Number(num) => format!("CssNthChildSelector::Number({})", num),
        CssNthChildSelector::Even => format!("CssNthChildSelector::Even"),
        CssNthChildSelector::Odd => format!("CssNthChildSelector::Odd"),
        CssNthChildSelector::Pattern(CssNthChildPattern { repeat, offset }) =>
            format!("CssNthChildSelector::Pattern(CssNthChildPattern {{ repeat: {}, offset: {} }})", repeat, offset),
    }
}

fn print_declaraction(decl: &CssDeclaration, tabs: usize) -> String {
    match decl {
        CssDeclaration::Static(s) => format!("CssDeclaration::Static({})", format_static_css_prop(s, tabs)),
        CssDeclaration::Dynamic(d) => format!("CssDeclaration::Dynamic({})", format_dynamic_css_prop(d, tabs)),
    }
}

trait FormatAsRustCode {
    fn format_as_rust_code(&self, _tabs: usize) -> String;
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
        CssProperty::ScrollbarStyle(p) => format!("CssProperty::ScrollbarStyle({})", print_css_property_value(p, tabs)),
        CssProperty::Opacity(p) => format!("CssProperty::Opacity({})", print_css_property_value(p, tabs)),
        CssProperty::Transform(p) => format!("CssProperty::Transform({})", print_css_property_value(p, tabs)),
        CssProperty::TransformOrigin(p) => format!("CssProperty::TransformOrigin({})", print_css_property_value(p, tabs)),
        CssProperty::PerspectiveOrigin(p) => format!("CssProperty::PerspectiveOrigin({})", print_css_property_value(p, tabs)),
        CssProperty::BackfaceVisibility(p) => format!("CssProperty::BackfaceVisibility({})", print_css_property_value(p, tabs)),
    }
}

fn format_dynamic_css_prop(decl: &DynamicCssProperty, tabs: usize) -> String {
    let t = String::from("    ").repeat(tabs);
    format!("DynamicCssProperty {{\r\n{}    dynamic_id: {:?},\r\n{}    default_value: {},\r\n{}}}",
        t, decl.dynamic_id, t, format_static_css_prop(&decl.default_value, tabs + 1), t)
}

fn format_pixel_value(p: &PixelValue) -> String {
    match p.metric {
        SizeMetric::Px => format!("PixelValue::px({:?})", p.number.get()),
        SizeMetric::Pt => format!("PixelValue::pt({:?})", p.number.get()),
        SizeMetric::Em => format!("PixelValue::em({:?})", p.number.get()),
        SizeMetric::Percent => format!("PixelValue::percent({:?})", p.number.get()),
    }
}

fn format_pixel_value_no_percent(p: &PixelValueNoPercent) -> String {
    format!("PixelValueNoPercent {{ inner: {} }}", format_pixel_value(&p.inner))
}

fn format_float_value(f: &FloatValue) -> String {
    format!("FloatValue::from({:?})", f.get())
}

fn format_percentage_value(f: &PercentageValue) -> String {
    format!("PercentageValue::from({:?})", f.get())
}

fn format_angle_value(f: &AngleValue) -> String {
    format!("AngleValue::degrees({:?})", f.to_degrees())
}

fn format_color_value(c: &ColorU) -> String {
    format!("ColorU {{ r: {}, g: {}, b: {}, a: {} }}", c.r, c.g, c.b, c.a)
}

macro_rules! impl_float_value_fmt {($struct_name:ident) => (
    impl FormatAsRustCode for $struct_name {
        fn format_as_rust_code(&self, _tabs: usize) -> String {
            format!("{}({})", stringify!($struct_name), format_float_value(&self.inner))
        }
    }
)}

impl_float_value_fmt!(LayoutFlexGrow);
impl_float_value_fmt!(LayoutFlexShrink);
impl_float_value_fmt!(StyleOpacity);

macro_rules! impl_percentage_value_fmt {($struct_name:ident) => (
    impl FormatAsRustCode for $struct_name {
        fn format_as_rust_code(&self, _tabs: usize) -> String {
            format!("{}({})", stringify!($struct_name), format_percentage_value(&self.inner))
        }
    }
)}

impl_percentage_value_fmt!(StyleTabWidth);
impl_percentage_value_fmt!(StyleLineHeight);

macro_rules! impl_pixel_value_fmt {($struct_name:ident) => (
    impl FormatAsRustCode for $struct_name {
        fn format_as_rust_code(&self, _tabs: usize) -> String {
            format!("{}({})", stringify!($struct_name), format_pixel_value(&self.inner))
        }
    }
)}

impl_pixel_value_fmt!(StyleBorderTopLeftRadius);
impl_pixel_value_fmt!(StyleBorderBottomLeftRadius);
impl_pixel_value_fmt!(StyleBorderTopRightRadius);
impl_pixel_value_fmt!(StyleBorderBottomRightRadius);

impl_pixel_value_fmt!(LayoutBorderTopWidth);
impl_pixel_value_fmt!(LayoutBorderLeftWidth);
impl_pixel_value_fmt!(LayoutBorderRightWidth);
impl_pixel_value_fmt!(LayoutBorderBottomWidth);
impl_pixel_value_fmt!(StyleLetterSpacing);
impl_pixel_value_fmt!(StyleWordSpacing);
impl_pixel_value_fmt!(StyleFontSize);

impl_pixel_value_fmt!(LayoutMarginTop);
impl_pixel_value_fmt!(LayoutMarginBottom);
impl_pixel_value_fmt!(LayoutMarginRight);
impl_pixel_value_fmt!(LayoutMarginLeft);

impl_pixel_value_fmt!(LayoutPaddingTop);
impl_pixel_value_fmt!(LayoutPaddingBottom);
impl_pixel_value_fmt!(LayoutPaddingRight);
impl_pixel_value_fmt!(LayoutPaddingLeft);

impl_pixel_value_fmt!(LayoutWidth);
impl_pixel_value_fmt!(LayoutHeight);
impl_pixel_value_fmt!(LayoutMinHeight);
impl_pixel_value_fmt!(LayoutMinWidth);
impl_pixel_value_fmt!(LayoutMaxWidth);
impl_pixel_value_fmt!(LayoutMaxHeight);
impl_pixel_value_fmt!(LayoutTop);
impl_pixel_value_fmt!(LayoutBottom);
impl_pixel_value_fmt!(LayoutRight);
impl_pixel_value_fmt!(LayoutLeft);

macro_rules! impl_color_value_fmt {($struct_name:ty) => (
    impl FormatAsRustCode for $struct_name {
        fn format_as_rust_code(&self, _tabs: usize) -> String {
            format!("{}({})", stringify!($struct_name), format_color_value(&self.inner))
        }
    }
)}

impl_color_value_fmt!(StyleTextColor);
impl_color_value_fmt!(StyleBorderTopColor);
impl_color_value_fmt!(StyleBorderLeftColor);
impl_color_value_fmt!(StyleBorderRightColor);
impl_color_value_fmt!(StyleBorderBottomColor);

macro_rules! impl_enum_fmt {($enum_name:ident, $($enum_type:ident),+) => (
    impl FormatAsRustCode for $enum_name {
        fn format_as_rust_code(&self, _tabs: usize) -> String {
            match self {
                $(
                    $enum_name::$enum_type => String::from(concat!(stringify!($enum_name), "::", stringify!($enum_type))),
                )+
            }
        }
    }
)}

impl_enum_fmt!(StyleCursor,
    Alias,
    AllScroll,
    Cell,
    ColResize,
    ContextMenu,
    Copy,
    Crosshair,
    Default,
    EResize,
    EwResize,
    Grab,
    Grabbing,
    Help,
    Move,
    NResize,
    NsResize,
    NeswResize,
    NwseResize,
    Pointer,
    Progress,
    RowResize,
    SResize,
    SeResize,
    Text,
    Unset,
    VerticalText,
    WResize,
    Wait,
    ZoomIn,
    ZoomOut
);

impl_enum_fmt!(BorderStyle,
    None,
    Solid,
    Double,
    Dotted,
    Dashed,
    Hidden,
    Groove,
    Ridge,
    Inset,
    Outset
);

impl FormatAsRustCode for StyleBackgroundSizeVec {
    fn format_as_rust_code(&self, tabs: usize) -> String {
        let t = String::from("    ").repeat(tabs);
        self.iter()
            .map(|bgs| format_style_background_size(bgs))
            .collect::<Vec<_>>()
            .join(&format!(",\r\n{}", t))
    }
}

fn format_style_background_size(c: &StyleBackgroundSize) -> String {
    match c {
        StyleBackgroundSize::Contain => String::from("StyleBackgroundSize::Contain"),
        StyleBackgroundSize::Cover => String::from("StyleBackgroundSize::Cover"),
        StyleBackgroundSize::ExactSize([w, h]) => format!("StyleBackgroundSize::ExactSize([{}, {}])", format_pixel_value(w), format_pixel_value(h)),
    }
}

impl FormatAsRustCode for ScrollbarStyle {
    fn format_as_rust_code(&self, tabs: usize) -> String {
        let t = String::from("    ").repeat(tabs);
        let t1 = String::from("    ").repeat(tabs + 1);
        format!("ScrollbarStyle {{\r\n{}horizontal: {},\r\n{}vertical: {},\r\n{}}}",
            t1,
            format_scrollbar_info(&self.horizontal, tabs + 1), t1,
            format_scrollbar_info(&self.vertical, tabs + 1),
            t,
        )
    }
}

fn format_scrollbar_info(s: &ScrollbarInfo, tabs: usize) -> String {
    let t = String::from("    ").repeat(tabs);
    let t1 = String::from("    ").repeat(tabs + 1);
    format!("ScrollbarInfo {{\r\n{}width: {},\r\n{}padding_left: {},\r\n{}padding_right: {},\r\n{}track: {},\r\n{}thumb: {},\r\n{}button: {},\r\n{}button: {},\r\n{}resizer: {},\r\n{}}}",
        t1,
        s.width.format_as_rust_code(tabs + 1), t1,
        s.padding_left.format_as_rust_code(tabs + 1), t1,
        s.padding_right.format_as_rust_code(tabs + 1), t1,
        format_style_background_content(&s.track, tabs + 1), t1,
        format_style_background_content(&s.thumb, tabs + 1), t1,
        format_style_background_content(&s.button, tabs + 1), t1,
        format_style_background_content(&s.corner, tabs + 1), t1,
        format_style_background_content(&s.resizer, tabs + 1),
        t
    )
}

impl_enum_fmt!(StyleBackgroundRepeat,
    NoRepeat,
    Repeat,
    RepeatX,
    RepeatY
);

impl FormatAsRustCode for StyleBackgroundRepeatVec {
    fn format_as_rust_code(&self, tabs: usize) -> String {
        let t = String::from("    ").repeat(tabs);
        self.iter()
            .map(|bgr| bgr.format_as_rust_code(tabs + 1))
            .collect::<Vec<_>>()
            .join(&format!(",\r\n{}", t))
    }
}

impl_enum_fmt!(LayoutDisplay,
    Flex,
    Block,
    InlineBlock
);

impl_enum_fmt!(LayoutFloat,
    Left,
    Right
);

impl_enum_fmt!(LayoutBoxSizing,
    ContentBox,
    BorderBox
);

impl_enum_fmt!(LayoutFlexDirection,
    Row,
    RowReverse,
    Column,
    ColumnReverse
);

impl_enum_fmt!(LayoutFlexWrap,
    Wrap,
    NoWrap
);

impl_enum_fmt!(LayoutJustifyContent,
    Start,
    End,
    Center,
    SpaceBetween,
    SpaceAround,
    SpaceEvenly
);

impl_enum_fmt!(LayoutAlignItems,
    FlexStart,
    FlexEnd,
    Stretch,
    Center
);

impl_enum_fmt!(LayoutAlignContent,
    Start,
    End,
    Stretch,
    Center,
    SpaceBetween,
    SpaceAround
);

impl_enum_fmt!(Shape,
    Circle,
    Ellipse
);

impl_enum_fmt!(LayoutPosition,
    Static,
    Fixed,
    Absolute,
    Relative
);

impl_enum_fmt!(LayoutOverflow,
    Auto,
    Scroll,
    Visible,
    Hidden
);

impl_enum_fmt!(StyleTextAlignmentHorz,
    Center,
    Left,
    Right
);

impl_enum_fmt!(DirectionCorner,
    Right,
    Left,
    Top,
    Bottom,
    TopRight,
    TopLeft,
    BottomRight,
    BottomLeft
);

impl_enum_fmt!(ExtendMode,
    Clamp,
    Repeat
);

impl_enum_fmt!(StyleBackfaceVisibility,
    Visible,
    Hidden
);

impl FormatAsRustCode for StyleBackgroundContentVec {
    fn format_as_rust_code(&self, tabs: usize) -> String {
        let t = String::from("    ").repeat(tabs);
        self.iter()
            .map(|bgc| format_style_background_content(bgc, tabs + 1))
            .collect::<Vec<_>>()
            .join(&format!(",\r\n{}", t))
    }
}

fn format_style_background_content(content: &StyleBackgroundContent, tabs: usize) -> String {
    match content {
        StyleBackgroundContent::LinearGradient(l) => format!("StyleBackgroundContent::LinearGradient({})", format_linear_gradient(l, tabs)),
        StyleBackgroundContent::RadialGradient(r) => format!("StyleBackgroundContent::RadialGradient({})", format_radial_gradient(r, tabs)),
        StyleBackgroundContent::ConicGradient(r) => format!("StyleBackgroundContent::ConicGradient({})", format_conic_gradient(r, tabs)),
        StyleBackgroundContent::Image(id) => format!("StyleBackgroundContent::Image({:?})", id),
        StyleBackgroundContent::Color(c) => format!("StyleBackgroundContent::Color({})", format_color_value(c)),
    }
}

fn format_direction(d: &Direction, tabs: usize) -> String {
    match d {
        Direction::Angle(fv) => format!("Direction::Angle({})", format_angle_value(fv)),
        Direction::FromTo(DirectionCorners { from, to }) => format!("Direction::FromTo(DirectionCorners {{ from: {}, to: {} }})",
            from.format_as_rust_code(tabs + 1), to.format_as_rust_code(tabs + 1)
        ),
    }
}

fn format_linear_gradient(l: &LinearGradient, tabs: usize) -> String {
    let t = String::from("    ").repeat(tabs);
    let t1 = String::from("    ").repeat(tabs + 1);
    format!("LinearGradient {{\r\n{}direction: {},\r\n{}extend_mode: {},\r\n{}stops: vec![\r\n{}{}\r\n{}].into(),\r\n{}}}",
        t1, format_direction(&l.direction, tabs + 1), t1,
        l.extend_mode.format_as_rust_code(tabs + 1), t1,
        t1, format_linear_color_stops(l.stops.as_ref(), tabs), t1, t,
    )
}

fn format_conic_gradient(r: &ConicGradient, tabs: usize) -> String {
    let t = String::from("    ").repeat(tabs);
    let t1 = String::from("    ").repeat(tabs + 1);

    format!("ConicGradient {{\r\n{}extend_mode: {},\r\n{}center: {},\r\n{}angle: {},\r\n{}stops: vec![\r\n{}{}\r\n{}].into(),\r\n{}}}",
        t1,
        r.extend_mode.format_as_rust_code(tabs + 1), t1,
        format_style_background_position(&r.center, tabs + 1), t1,
        format_angle_value(&r.angle), t1,
        t1, format_radial_color_stops(r.stops.as_ref(), tabs + 1), t1,
        t,
    )
}

fn format_radial_gradient(r: &RadialGradient, tabs: usize) -> String {
    let t = String::from("    ").repeat(tabs);
    let t1 = String::from("    ").repeat(tabs + 1);
    format!("RadialGradient {{\r\n{}shape: {},\r\n{}extend_mode: {},\r\n{}stops: vec![\r\n{}{}\r\n{}].into(),\r\n{}}}",
        t1, r.shape.format_as_rust_code(tabs + 1), t1,
        r.extend_mode.format_as_rust_code(tabs + 1), t1,
        t1, format_linear_color_stops(r.stops.as_ref(), tabs + 1), t1, t,
    )
}

fn format_linear_color_stops(stops: &[LinearColorStop], tabs: usize) -> String {
    let t = String::from("    ").repeat(tabs);
    stops.iter()
        .map(|s| format_linear_color_stop(s))
        .collect::<Vec<_>>()
        .join(&format!(",\r\n{}", t))
}

fn format_linear_color_stop(g: &LinearColorStop) -> String {
    format!("LinearColorStop {{ offset: {}, color: {} }}",
        {
            let offset: Option<PercentageValue> = g.offset.into();
            offset.as_ref().map(|s| format_percentage_value(s)).unwrap_or(format!("None"))
        },
        format_color_value(&g.color),
    )
}

fn format_radial_color_stops(stops: &[RadialColorStop], tabs: usize) -> String {
    let t = String::from("    ").repeat(tabs);
    stops.iter()
        .map(|s| format_radial_color_stop(s))
        .collect::<Vec<_>>()
        .join(&format!(",\r\n{}", t))
}

fn format_radial_color_stop(g: &RadialColorStop) -> String {
    format!("RadialColorStop {{ offset: {}, color: {} }}",
        {
            let offset: Option<AngleValue> = g.offset.into();
            offset.as_ref().map(|s| format_angle_value(s)).unwrap_or(format!("None"))
        },
        format_color_value(&g.color),
    )
}

impl FormatAsRustCode for StyleTransformVec {
    fn format_as_rust_code(&self, tabs: usize) -> String {
        let t1 = String::from("    ").repeat(tabs + 1);
        format!("vec![\r\n{}{}\r\n{}].into()",
            t1, format_style_transforms(self.as_ref(), tabs + 1), t1,
        )
    }
}

fn format_style_transforms(stops: &[StyleTransform], tabs: usize) -> String {
    let t = String::from("    ").repeat(tabs);
    stops.iter()
        .map(|s| format_style_transform(s, tabs))
        .collect::<Vec<_>>()
        .join(&format!(",\r\n{}", t))
}

fn format_style_transform(st: &StyleTransform, tabs: usize) -> String {
    let tabs_minus_one = String::from("    ").repeat(tabs);
    let tabs = String::from("    ").repeat(tabs + 1);
    match st {
        StyleTransform::Matrix(m) => format!("StyleTransform::Matrix(StyleTransformMatrix2D {{ a: {}, b: {}, c: {}, d: {}, tx: {}, ty: {} }})", format_pixel_value(&m.a), format_pixel_value(&m.b), format_pixel_value(&m.c), format_pixel_value(&m.d), format_pixel_value(&m.tx), format_pixel_value(&m.ty)),
        StyleTransform::Matrix3D(m) => format!("StyleTransform::Matrix3D(StyleTransformMatrix3D {{\r\n{tabs}m11: {},\r\n{tabs}m12: {},\r\n{tabs}m13: {},\r\n{tabs}m14: {},\r\n{tabs}m21: {},\r\n{tabs}m22: {},\r\n{tabs}m23: {},\r\n{tabs}m24: {},\r\n{tabs}m31: {},\r\n{tabs}m32: {},\r\n{tabs}m33: {},\r\n{tabs}m34: {},\r\n{tabs}m41: {},\r\n{tabs}m42: {},\r\n{tabs}m43: {},\r\n{tabs}m44: {}\r\n{tabs_minus_one}}})",
           format_pixel_value(&m.m11),
           format_pixel_value(&m.m12),
           format_pixel_value(&m.m13),
           format_pixel_value(&m.m14),
           format_pixel_value(&m.m21),
           format_pixel_value(&m.m22),
           format_pixel_value(&m.m23),
           format_pixel_value(&m.m24),
           format_pixel_value(&m.m31),
           format_pixel_value(&m.m32),
           format_pixel_value(&m.m33),
           format_pixel_value(&m.m34),
           format_pixel_value(&m.m41),
           format_pixel_value(&m.m42),
           format_pixel_value(&m.m43),
           format_pixel_value(&m.m44),
           tabs = tabs,
           tabs_minus_one = tabs_minus_one,
       ),
        StyleTransform::Translate(t) => format!("StyleTransform::Translate(StyleTransformTranslate2D {{ x: {}, y: {} }})", format_pixel_value(&t.x), format_pixel_value(&t.y)),
        StyleTransform::Translate3D(t) => format!("StyleTransform::Translate3D(StyleTransformTranslate3D {{ x: {}, y: {}, z: {})", format_pixel_value(&t.x), format_pixel_value(&t.y), format_pixel_value(&t.z)),
        StyleTransform::TranslateX(x) => format!("StyleTransform::TranslateX({})", format_pixel_value(&x)),
        StyleTransform::TranslateY(y) => format!("StyleTransform::TranslateY({})", format_pixel_value(&y)),
        StyleTransform::TranslateZ(z) => format!("StyleTransform::TranslateZ({})", format_pixel_value(&z)),
        StyleTransform::Rotate(r) => format!("StyleTransform::Rotate({})", format_angle_value(&r)),
        StyleTransform::Rotate3D(r) => format!("StyleTransform::Rotate3D(StyleTransformRotate3D {{ {}, {}, {}, {} }})", format_percentage_value(&r.x), format_percentage_value(&r.y), format_percentage_value(&r.z), format_angle_value(&r.angle)),
        StyleTransform::RotateX(x) => format!("StyleTransform::RotateX({})", format_angle_value(&x)),
        StyleTransform::RotateY(y) => format!("StyleTransform::RotateY({})", format_angle_value(&y)),
        StyleTransform::RotateZ(z) => format!("StyleTransform::RotateZ({})", format_angle_value(&z)),
        StyleTransform::Scale(s) => format!("StyleTransform::Scale(StyleTransformScale2D {{ x: {}, y: {} }})", format_percentage_value(&s.x), format_percentage_value(&s.y)),
        StyleTransform::Scale3D(s) => format!("StyleTransform::Scale3D(StyleTransformScale3D {{ x; {}, y: {}, z: {} }})", format_percentage_value(&s.x), format_percentage_value(&s.y), format_percentage_value(&s.z)),
        StyleTransform::ScaleX(x) => format!("StyleTransform::ScaleX({})", format_percentage_value(&x)),
        StyleTransform::ScaleY(y) => format!("StyleTransform::ScaleY({})", format_percentage_value(&y)),
        StyleTransform::ScaleZ(z) => format!("StyleTransform::ScaleZ({})", format_percentage_value(&z)),
        StyleTransform::Skew(sk) => format!("StyleTransform::Skew(StyleTransformSkew2D {{ x: {}, y: {} }})", format_percentage_value(&sk.x), format_percentage_value(&sk.y)),
        StyleTransform::SkewX(x) => format!("StyleTransform::SkewX({})", format_percentage_value(&x)),
        StyleTransform::SkewY(y) => format!("StyleTransform::SkewY({})", format_percentage_value(&y)),
        StyleTransform::Perspective(dist) => format!("StyleTransform::Perspective({})", format_pixel_value(&dist)),
    }
}

fn format_font_ids(stops: &[AzString], tabs: usize) -> String {
    let t = String::from("    ").repeat(tabs);
    stops.iter()
        .map(|s| format!("{:?}", s))
        .collect::<Vec<_>>()
        .join(&format!(",\r\n{}", t))
}

impl FormatAsRustCode for StyleFontFamily {
    fn format_as_rust_code(&self, tabs: usize) -> String {
        let t = String::from("    ").repeat(tabs);
        let t1 = String::from("    ").repeat(tabs + 1);
        format!("StyleFontFamily {{ fonts: vec![\r\n{}{}\r\n{}].into()\r\n{}}}",
            t1, format_font_ids(self.fonts.as_ref(), tabs + 1), t1, t
        )
    }
}

impl FormatAsRustCode for StyleBackgroundPositionVec {
    fn format_as_rust_code(&self, tabs: usize) -> String {
        let t = String::from("    ").repeat(tabs);
        self.iter()
            .map(|bgp| format_style_background_position(bgp, tabs))
            .collect::<Vec<_>>()
            .join(&format!(",\r\n{}", t))
    }
}

fn format_style_background_position(b: &StyleBackgroundPosition, tabs: usize) -> String {
    let t = String::from("    ").repeat(tabs);
    let t1 = String::from("    ").repeat(tabs + 1);
    format!("StyleBackgroundPosition {{\r\n{}horizontal: {},\r\n{}vertical: {},\r\n{}}}",
        t1,
        format_background_position_horizontal(&b.horizontal), t1,
        format_background_position_vertical(&b.vertical), t
    )
}

fn format_background_position_horizontal(b: &BackgroundPositionHorizontal) -> String {
    match b {
        BackgroundPositionHorizontal::Left => format!("BackgroundPositionHorizontal::Left"),
        BackgroundPositionHorizontal::Center => format!("BackgroundPositionHorizontal::Center"),
        BackgroundPositionHorizontal::Right => format!("BackgroundPositionHorizontal::Right"),
        BackgroundPositionHorizontal::Exact(p) => format!("BackgroundPositionHorizontal::Exact({})", format_pixel_value(p)),
    }
}

fn format_background_position_vertical(b: &BackgroundPositionVertical) -> String {
    match b {
        BackgroundPositionVertical::Top => format!("BackgroundPositionVertical::Top"),
        BackgroundPositionVertical::Center => format!("BackgroundPositionVertical::Center"),
        BackgroundPositionVertical::Bottom => format!("BackgroundPositionVertical::Bottom"),
        BackgroundPositionVertical::Exact(p) => format!("BackgroundPositionVertical::Exact({})", format_pixel_value(p)),
    }
}

impl FormatAsRustCode for StyleBorderTopStyle {
    fn format_as_rust_code(&self, tabs: usize) -> String {
        format!("StyleBorderTopStyle({})", &self.inner.format_as_rust_code(tabs))
    }
}

impl FormatAsRustCode for StyleBorderRightStyle {
    fn format_as_rust_code(&self, tabs: usize) -> String {
        format!("StyleBorderRightStyle({})", &self.inner.format_as_rust_code(tabs))
    }
}

impl FormatAsRustCode for StyleBorderLeftStyle {
    fn format_as_rust_code(&self, tabs: usize) -> String {
        format!("StyleBorderLeftStyle({})", &self.inner.format_as_rust_code(tabs))
    }
}

impl FormatAsRustCode for StyleBorderBottomStyle {
    fn format_as_rust_code(&self, tabs: usize) -> String {
        format!("StyleBorderBottomStyle({})", &self.inner.format_as_rust_code(tabs))
    }
}

impl FormatAsRustCode for StyleBoxShadow {
    fn format_as_rust_code(&self, tabs: usize) -> String {
        let t = String::from("    ").repeat(tabs);
        format!("StyleBoxShadow {{\r\n{}    offset: [{}, {}],\r\n{}    color: {},\r\n{}    blur_radius: {},\r\n{}    spread_radius: {},\r\n{}    clip_mode: ClipMode::{:?},\r\n{}}}",
            t, format_pixel_value_no_percent(&self.offset[0]), format_pixel_value_no_percent(&self.offset[1]),
            t, format_color_value(&self.color),
            t, format_pixel_value_no_percent(&self.blur_radius),
            t, format_pixel_value_no_percent(&self.spread_radius),
            t, self.clip_mode,
            t
        )
    }
}

impl FormatAsRustCode for StyleTransformOrigin {
    fn format_as_rust_code(&self, _tabs: usize) -> String {
        format!("StyleTransformOrigin {{ x: {}, y: {} }}", format_pixel_value(&self.x), format_pixel_value(&self.y))
    }
}

impl FormatAsRustCode for StylePerspectiveOrigin {
    fn format_as_rust_code(&self, _tabs: usize) -> String {
        format!("StylePerspectiveOrigin {{ x: {}, y: {} }}", format_pixel_value(&self.x), format_pixel_value(&self.y))
    }
}
