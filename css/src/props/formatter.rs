//! Trait and implementations for formatting CSS properties back into strings.

use alloc::{
    string::{String, ToString},
    vec::Vec,
};

use crate::props::{
    basic::{
        angle::AngleValue,
        color::ColorU,
        direction::{Direction, DirectionCorner},
        value::{FloatValue, PercentageValue, PixelValue},
    },
    layout::{dimensions::*, display::*, flex::*, overflow::*, position::*, spacing::*},
    style::{
        background::*,
        border::*,
        border_radius::*,
        box_shadow::{BoxShadowClipMode, StyleBoxShadow},
        effects::*,
        filter::*,
        font::*,
        scrollbar::*,
        text::*,
        transform::*,
    },
};

/// A trait for any type that can be formatted as a valid CSS value string.
pub trait PrintAsCssValue {
    /// Formats the type as a CSS value string.
    fn print_as_css_value(&self) -> String;
}

// --- Basic Properties ---

impl PrintAsCssValue for AngleValue {
    fn print_as_css_value(&self) -> String {
        format!("{}", self)
    }
}

impl PrintAsCssValue for ColorU {
    fn print_as_css_value(&self) -> String {
        self.to_hash()
    }
}

impl PrintAsCssValue for Direction {
    fn print_as_css_value(&self) -> String {
        match self {
            Direction::Angle(a) => format!("{}", a),
            Direction::FromTo(d) => format!("to {}", d.to),
        }
    }
}

impl PrintAsCssValue for DirectionCorner {
    fn print_as_css_value(&self) -> String {
        format!("{}", self)
    }
}

impl PrintAsCssValue for PixelValue {
    fn print_as_css_value(&self) -> String {
        format!("{}", self)
    }
}

impl PrintAsCssValue for PercentageValue {
    fn print_as_css_value(&self) -> String {
        format!("{}", self)
    }
}

impl PrintAsCssValue for FloatValue {
    fn print_as_css_value(&self) -> String {
        format!("{}", self)
    }
}

// --- Layout Properties ---

impl PrintAsCssValue for LayoutWidth {
    fn print_as_css_value(&self) -> String {
        format!("{}", self.inner)
    }
}
impl PrintAsCssValue for LayoutHeight {
    fn print_as_css_value(&self) -> String {
        format!("{}", self.inner)
    }
}
impl PrintAsCssValue for LayoutMinWidth {
    fn print_as_css_value(&self) -> String {
        format!("{}", self.inner)
    }
}
impl PrintAsCssValue for LayoutMinHeight {
    fn print_as_css_value(&self) -> String {
        format!("{}", self.inner)
    }
}
impl PrintAsCssValue for LayoutMaxWidth {
    fn print_as_css_value(&self) -> String {
        format!("{}", self.inner)
    }
}
impl PrintAsCssValue for LayoutMaxHeight {
    fn print_as_css_value(&self) -> String {
        format!("{}", self.inner)
    }
}

impl PrintAsCssValue for LayoutBoxSizing {
    fn print_as_css_value(&self) -> String {
        String::from(match self {
            LayoutBoxSizing::ContentBox => "content-box",
            LayoutBoxSizing::BorderBox => "border-box",
        })
    }
}

impl PrintAsCssValue for LayoutDisplay {
    fn print_as_css_value(&self) -> String {
        String::from(match self {
            LayoutDisplay::None => "none",
            LayoutDisplay::Block => "block",
            LayoutDisplay::Inline => "inline",
            LayoutDisplay::InlineBlock => "inline-block",
            LayoutDisplay::Flex => "flex",
            LayoutDisplay::InlineFlex => "inline-flex",
            LayoutDisplay::Table => "table",
            LayoutDisplay::InlineTable => "inline-table",
            LayoutDisplay::TableRowGroup => "table-row-group",
            LayoutDisplay::TableHeaderGroup => "table-header-group",
            LayoutDisplay::TableFooterGroup => "table-footer-group",
            LayoutDisplay::TableRow => "table-row",
            LayoutDisplay::TableColumnGroup => "table-column-group",
            LayoutDisplay::TableColumn => "table-column",
            LayoutDisplay::TableCell => "table-cell",
            LayoutDisplay::TableCaption => "table-caption",
            LayoutDisplay::ListItem => "list-item",
            LayoutDisplay::RunIn => "run-in",
            LayoutDisplay::Marker => "marker",
            LayoutDisplay::FlowRoot => "flow-root",
            LayoutDisplay::Grid => "grid",
            LayoutDisplay::InlineGrid => "inline-grid",
            LayoutDisplay::Initial => "initial",
            LayoutDisplay::Inherit => "inherit",
        })
    }
}

impl PrintAsCssValue for LayoutFloat {
    fn print_as_css_value(&self) -> String {
        String::from(match self {
            LayoutFloat::Left => "left",
            LayoutFloat::Right => "right",
            LayoutFloat::None => "none",
        })
    }
}

impl PrintAsCssValue for LayoutFlexDirection {
    fn print_as_css_value(&self) -> String {
        String::from(match self {
            LayoutFlexDirection::Row => "row",
            LayoutFlexDirection::RowReverse => "row-reverse",
            LayoutFlexDirection::Column => "column",
            LayoutFlexDirection::ColumnReverse => "column-reverse",
        })
    }
}

impl PrintAsCssValue for LayoutFlexWrap {
    fn print_as_css_value(&self) -> String {
        String::from(match self {
            LayoutFlexWrap::Wrap => "wrap",
            LayoutFlexWrap::NoWrap => "nowrap",
            LayoutFlexWrap::WrapReverse => "wrap-reverse",
        })
    }
}

impl PrintAsCssValue for LayoutFlexGrow {
    fn print_as_css_value(&self) -> String {
        format!("{}", self.inner)
    }
}
impl PrintAsCssValue for LayoutFlexShrink {
    fn print_as_css_value(&self) -> String {
        format!("{}", self.inner)
    }
}

impl PrintAsCssValue for LayoutJustifyContent {
    fn print_as_css_value(&self) -> String {
        String::from(match self {
            LayoutJustifyContent::Start => "flex-start",
            LayoutJustifyContent::End => "flex-end",
            LayoutJustifyContent::Center => "center",
            LayoutJustifyContent::SpaceBetween => "space-between",
            LayoutJustifyContent::SpaceAround => "space-around",
            LayoutJustifyContent::SpaceEvenly => "space-evenly",
        })
    }
}

impl PrintAsCssValue for LayoutAlignItems {
    fn print_as_css_value(&self) -> String {
        String::from(match self {
            LayoutAlignItems::Stretch => "stretch",
            LayoutAlignItems::Center => "center",
            LayoutAlignItems::Start => "flex-start",
            LayoutAlignItems::End => "flex-end",
            LayoutAlignItems::Baseline => "baseline",
        })
    }
}

impl PrintAsCssValue for LayoutAlignContent {
    fn print_as_css_value(&self) -> String {
        String::from(match self {
            LayoutAlignContent::Stretch => "stretch",
            LayoutAlignContent::Center => "center",
            LayoutAlignContent::Start => "flex-start",
            LayoutAlignContent::End => "flex-end",
            LayoutAlignContent::SpaceBetween => "space-between",
            LayoutAlignContent::SpaceAround => "space-around",
        })
    }
}

impl PrintAsCssValue for LayoutOverflow {
    fn print_as_css_value(&self) -> String {
        String::from(match self {
            LayoutOverflow::Scroll => "scroll",
            LayoutOverflow::Auto => "auto",
            LayoutOverflow::Hidden => "hidden",
            LayoutOverflow::Visible => "visible",
            LayoutOverflow::Clip => "clip",
        })
    }
}

impl PrintAsCssValue for LayoutPosition {
    fn print_as_css_value(&self) -> String {
        String::from(match self {
            LayoutPosition::Static => "static",
            LayoutPosition::Relative => "relative",
            LayoutPosition::Absolute => "absolute",
            LayoutPosition::Fixed => "fixed",
        })
    }
}

impl PrintAsCssValue for LayoutTop {
    fn print_as_css_value(&self) -> String {
        format!("{}", self.inner)
    }
}
impl PrintAsCssValue for LayoutRight {
    fn print_as_css_value(&self) -> String {
        format!("{}", self.inner)
    }
}
impl PrintAsCssValue for LayoutBottom {
    fn print_as_css_value(&self) -> String {
        format!("{}", self.inner)
    }
}
impl PrintAsCssValue for LayoutLeft {
    fn print_as_css_value(&self) -> String {
        format!("{}", self.inner)
    }
}

impl PrintAsCssValue for LayoutPaddingTop {
    fn print_as_css_value(&self) -> String {
        format!("{}", self.inner)
    }
}
impl PrintAsCssValue for LayoutPaddingRight {
    fn print_as_css_value(&self) -> String {
        format!("{}", self.inner)
    }
}
impl PrintAsCssValue for LayoutPaddingBottom {
    fn print_as_css_value(&self) -> String {
        format!("{}", self.inner)
    }
}
impl PrintAsCssValue for LayoutPaddingLeft {
    fn print_as_css_value(&self) -> String {
        format!("{}", self.inner)
    }
}

impl PrintAsCssValue for LayoutMarginTop {
    fn print_as_css_value(&self) -> String {
        format!("{}", self.inner)
    }
}
impl PrintAsCssValue for LayoutMarginRight {
    fn print_as_css_value(&self) -> String {
        format!("{}", self.inner)
    }
}
impl PrintAsCssValue for LayoutMarginBottom {
    fn print_as_css_value(&self) -> String {
        format!("{}", self.inner)
    }
}
impl PrintAsCssValue for LayoutMarginLeft {
    fn print_as_css_value(&self) -> String {
        format!("{}", self.inner)
    }
}

// --- Style Properties ---

impl PrintAsCssValue for StyleBackgroundContentVec {
    fn print_as_css_value(&self) -> String {
        self.iter()
            .map(|f| f.print_as_css_value())
            .collect::<Vec<_>>()
            .join(", ")
    }
}

impl PrintAsCssValue for StyleBackgroundPositionVec {
    fn print_as_css_value(&self) -> String {
        self.iter()
            .map(|f| f.print_as_css_value())
            .collect::<Vec<_>>()
            .join(", ")
    }
}

impl PrintAsCssValue for StyleBackgroundSizeVec {
    fn print_as_css_value(&self) -> String {
        self.iter()
            .map(|f| f.print_as_css_value())
            .collect::<Vec<_>>()
            .join(", ")
    }
}

impl PrintAsCssValue for StyleBackgroundRepeatVec {
    fn print_as_css_value(&self) -> String {
        self.iter()
            .map(|f| f.print_as_css_value())
            .collect::<Vec<_>>()
            .join(", ")
    }
}

impl PrintAsCssValue for StyleBorderTopLeftRadius {
    fn print_as_css_value(&self) -> String {
        format!("{}", self.inner)
    }
}
impl PrintAsCssValue for StyleBorderTopRightRadius {
    fn print_as_css_value(&self) -> String {
        format!("{}", self.inner)
    }
}
impl PrintAsCssValue for StyleBorderBottomLeftRadius {
    fn print_as_css_value(&self) -> String {
        format!("{}", self.inner)
    }
}
impl PrintAsCssValue for StyleBorderBottomRightRadius {
    fn print_as_css_value(&self) -> String {
        format!("{}", self.inner)
    }
}

impl PrintAsCssValue for BorderStyle {
    fn print_as_css_value(&self) -> String {
        format!("{}", self)
    }
}

impl PrintAsCssValue for StyleBorderTopStyle {
    fn print_as_css_value(&self) -> String {
        format!("{}", self.inner)
    }
}
impl PrintAsCssValue for StyleBorderRightStyle {
    fn print_as_css_value(&self) -> String {
        format!("{}", self.inner)
    }
}
impl PrintAsCssValue for StyleBorderBottomStyle {
    fn print_as_css_value(&self) -> String {
        format!("{}", self.inner)
    }
}
impl PrintAsCssValue for StyleBorderLeftStyle {
    fn print_as_css_value(&self) -> String {
        format!("{}", self.inner)
    }
}

impl PrintAsCssValue for StyleBorderTopColor {
    fn print_as_css_value(&self) -> String {
        self.inner.to_hash()
    }
}
impl PrintAsCssValue for StyleBorderRightColor {
    fn print_as_css_value(&self) -> String {
        self.inner.to_hash()
    }
}
impl PrintAsCssValue for StyleBorderBottomColor {
    fn print_as_css_value(&self) -> String {
        self.inner.to_hash()
    }
}
impl PrintAsCssValue for StyleBorderLeftColor {
    fn print_as_css_value(&self) -> String {
        self.inner.to_hash()
    }
}

impl PrintAsCssValue for LayoutBorderTopWidth {
    fn print_as_css_value(&self) -> String {
        format!("{}", self.inner)
    }
}
impl PrintAsCssValue for LayoutBorderRightWidth {
    fn print_as_css_value(&self) -> String {
        format!("{}", self.inner)
    }
}
impl PrintAsCssValue for LayoutBorderBottomWidth {
    fn print_as_css_value(&self) -> String {
        format!("{}", self.inner)
    }
}
impl PrintAsCssValue for LayoutBorderLeftWidth {
    fn print_as_css_value(&self) -> String {
        format!("{}", self.inner)
    }
}

impl PrintAsCssValue for StyleBoxShadow {
    fn print_as_css_value(&self) -> String {
        let clip_str = if self.clip_mode == BoxShadowClipMode::Inset {
            " inset"
        } else {
            ""
        };
        let formatted = format!(
            "{} {} {} {} {}{}",
            self.offset[0].print_as_css_value(),
            self.offset[1].print_as_css_value(),
            self.blur_radius.print_as_css_value(),
            self.spread_radius.print_as_css_value(),
            self.color.to_hash(),
            clip_str
        );
        formatted.split_whitespace().collect::<Vec<_>>().join(" ")
    }
}

impl PrintAsCssValue for StyleOpacity {
    fn print_as_css_value(&self) -> String {
        format!("{}", self.inner.normalized())
    }
}
impl PrintAsCssValue for StyleMixBlendMode {
    fn print_as_css_value(&self) -> String {
        format!("{}", self)
    }
}
impl PrintAsCssValue for StyleCursor {
    fn print_as_css_value(&self) -> String {
        format!("{:?}", self).to_lowercase().replace("_", "-")
    }
}

impl PrintAsCssValue for StyleFilterVec {
    fn print_as_css_value(&self) -> String {
        self.iter()
            .map(|f| f.print_as_css_value())
            .collect::<Vec<_>>()
            .join(" ")
    }
}

impl PrintAsCssValue for StyleFontFamilyVec {
    fn print_as_css_value(&self) -> String {
        self.iter()
            .map(|f| f.as_string())
            .collect::<Vec<_>>()
            .join(", ")
    }
}

impl PrintAsCssValue for StyleFontSize {
    fn print_as_css_value(&self) -> String {
        format!("{}", self.inner)
    }
}

impl PrintAsCssValue for StyleTextAlign {
    fn print_as_css_value(&self) -> String {
        String::from(match self {
            StyleTextAlign::Left => "left",
            StyleTextAlign::Center => "center",
            StyleTextAlign::Right => "right",
            StyleTextAlign::Justify => "justify",
            StyleTextAlign::Start => "start",
            StyleTextAlign::End => "end",
        })
    }
}

impl PrintAsCssValue for StyleLetterSpacing {
    fn print_as_css_value(&self) -> String {
        format!("{}", self.inner)
    }
}
impl PrintAsCssValue for StyleWordSpacing {
    fn print_as_css_value(&self) -> String {
        format!("{}", self.inner)
    }
}
impl PrintAsCssValue for StyleLineHeight {
    fn print_as_css_value(&self) -> String {
        format!("{}", self.inner)
    }
}
impl PrintAsCssValue for StyleTabWidth {
    fn print_as_css_value(&self) -> String {
        format!("{}", self.inner)
    }
}
impl PrintAsCssValue for StyleWhiteSpace {
    fn print_as_css_value(&self) -> String {
        format!("{}", self)
    }
}
impl PrintAsCssValue for StyleHyphens {
    fn print_as_css_value(&self) -> String {
        format!("{}", self)
    }
}
impl PrintAsCssValue for StyleDirection {
    fn print_as_css_value(&self) -> String {
        format!("{}", self)
    }
}

impl PrintAsCssValue for StyleTransformVec {
    fn print_as_css_value(&self) -> String {
        self.iter()
            .map(|f| f.print_as_css_value())
            .collect::<Vec<_>>()
            .join(" ")
    }
}

impl PrintAsCssValue for StyleTransformOrigin {
    fn print_as_css_value(&self) -> String {
        format!("{} {}", self.x, self.y)
    }
}

impl PrintAsCssValue for StylePerspectiveOrigin {
    fn print_as_css_value(&self) -> String {
        format!("{} {}", self.x, self.y)
    }
}

impl PrintAsCssValue for StyleBackfaceVisibility {
    fn print_as_css_value(&self) -> String {
        String::from(match self {
            StyleBackfaceVisibility::Hidden => "hidden",
            StyleBackfaceVisibility::Visible => "visible",
        })
    }
}

impl PrintAsCssValue for ScrollbarStyle {
    fn print_as_css_value(&self) -> String {
        // NOTE: Non-standard property, formatting is for debug purposes.
        format!(
            "horz({}), vert({})",
            self.horizontal.print_as_css_value(),
            self.vertical.print_as_css_value()
        )
    }
}

// -- Helper impls --

impl PrintAsCssValue for ScrollbarInfo {
    fn print_as_css_value(&self) -> String {
        format!(
            "{} {} {} {} {} {} {} {}",
            self.width.print_as_css_value(),
            self.padding_left.print_as_css_value(),
            self.padding_right.print_as_css_value(),
            self.track.print_as_css_value(),
            self.thumb.print_as_css_value(),
            self.button.print_as_css_value(),
            self.corner.print_as_css_value(),
            self.resizer.print_as_css_value(),
        )
    }
}

impl PrintAsCssValue for StyleFilter {
    fn print_as_css_value(&self) -> String {
        match self {
            StyleFilter::Blend(mode) => format!("blend({})", mode.print_as_css_value()),
            StyleFilter::Flood(c) => format!("flood({})", c.print_as_css_value()),
            StyleFilter::Blur(c) => format!(
                "blur({} {})",
                c.width.print_as_css_value(),
                c.height.print_as_css_value()
            ),
            StyleFilter::Opacity(c) => format!("opacity({})", c.print_as_css_value()),
            StyleFilter::ColorMatrix(c) => format!(
                "color-matrix({})",
                c.matrix
                    .iter()
                    .map(|s| s.print_as_css_value())
                    .collect::<Vec<_>>()
                    .join(", ")
            ),
            StyleFilter::DropShadow(shadow) => {
                format!("drop-shadow({})", shadow.print_as_css_value())
            }
            StyleFilter::ComponentTransfer => "component-transfer".to_string(),
            StyleFilter::Offset(o) => format!(
                "offset({}, {})",
                o.x.print_as_css_value(),
                o.y.print_as_css_value()
            ),
            StyleFilter::Composite(c) => format!("composite({})", c.print_as_css_value()),
        }
    }
}

impl PrintAsCssValue for StyleCompositeFilter {
    fn print_as_css_value(&self) -> String {
        match self {
            StyleCompositeFilter::Over => "over".to_string(),
            StyleCompositeFilter::In => "in".to_string(),
            StyleCompositeFilter::Atop => "atop".to_string(),
            StyleCompositeFilter::Out => "out".to_string(),
            StyleCompositeFilter::Xor => "xor".to_string(),
            StyleCompositeFilter::Lighter => "lighter".to_string(),
            StyleCompositeFilter::Arithmetic(fv) => format!(
                "arithmetic({})",
                fv.iter()
                    .map(|s| s.print_as_css_value())
                    .collect::<Vec<_>>()
                    .join(", ")
            ),
        }
    }
}

impl PrintAsCssValue for StyleTransform {
    fn print_as_css_value(&self) -> String {
        match self {
            StyleTransform::Matrix(m) => format!(
                "matrix({}, {}, {}, {}, {}, {})",
                m.a, m.b, m.c, m.d, m.tx, m.ty
            ),
            StyleTransform::Matrix3D(m) => format!(
                "matrix3d({}, {}, {}, {}, {}, {}, {}, {}, {}, {}, {}, {}, {}, {}, {}, {})",
                m.m11,
                m.m12,
                m.m13,
                m.m14,
                m.m21,
                m.m22,
                m.m23,
                m.m24,
                m.m31,
                m.m32,
                m.m33,
                m.m34,
                m.m41,
                m.m42,
                m.m43,
                m.m44
            ),
            StyleTransform::Translate(t) => format!("translate({}, {})", t.x, t.y),
            StyleTransform::Translate3D(t) => format!("translate3d({}, {}, {})", t.x, t.y, t.z),
            StyleTransform::TranslateX(x) => format!("translateX({})", x),
            StyleTransform::TranslateY(y) => format!("translateY({})", y),
            StyleTransform::TranslateZ(z) => format!("translateZ({})", z),
            StyleTransform::Rotate(r) => format!("rotate({})", r),
            StyleTransform::Rotate3D(r) => {
                format!("rotate3d({}, {}, {}, {})", r.x, r.y, r.z, r.angle)
            }
            StyleTransform::RotateX(x) => format!("rotateX({})", x),
            StyleTransform::RotateY(y) => format!("rotateY({})", y),
            StyleTransform::RotateZ(z) => format!("rotateZ({})", z),
            StyleTransform::Scale(s) => format!("scale({}, {})", s.x, s.y),
            StyleTransform::Scale3D(s) => format!("scale3d({}, {}, {})", s.x, s.y, s.z),
            StyleTransform::ScaleX(x) => format!("scaleX({})", x),
            StyleTransform::ScaleY(y) => format!("scaleY({})", y),
            StyleTransform::ScaleZ(z) => format!("scaleZ({})", z),
            StyleTransform::Skew(sk) => format!("skew({}, {})", sk.x, sk.y),
            StyleTransform::SkewX(x) => format!("skewX({})", x),
            StyleTransform::SkewY(y) => format!("skewY({})", y),
            StyleTransform::Perspective(dist) => format!("perspective({})", dist),
        }
    }
}
