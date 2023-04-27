use alloc::string::String;
use alloc::vec::Vec;

use crate::css::PrintAsCssValue;
use crate::css_properties::*;

impl PrintAsCssValue for StyleFilter {
    fn print_as_css_value(&self) -> String {
        match self {
            StyleFilter::Blend(mode) => format!("blend({})", mode.print_as_css_value()),
            StyleFilter::Flood(c) => format!("flood({})", c),
            StyleFilter::Blur(c) => format!("blur({} {})", c.width, c.height),
            StyleFilter::Opacity(c) => format!("opacity({})", c),
            StyleFilter::ColorMatrix(c) => format!(
                "color-matrix({})",
                c.matrix
                    .iter()
                    .map(|s| format!("{}", s))
                    .collect::<Vec<_>>()
                    .join(", ")
            ),
            StyleFilter::DropShadow(shadow) => {
                format!("drop-shadow({})", shadow.print_as_css_value())
            }
            StyleFilter::ComponentTransfer => format!("component-transfer"),
            StyleFilter::Offset(o) => format!("offset({}, {})", o.x, o.y),
            StyleFilter::Composite(c) => format!("composite({})", c.print_as_css_value()),
        }
    }
}

impl PrintAsCssValue for StyleCompositeFilter {
    fn print_as_css_value(&self) -> String {
        match self {
            StyleCompositeFilter::Over => format!("over"),
            StyleCompositeFilter::In => format!("in"),
            StyleCompositeFilter::Atop => format!("atop"),
            StyleCompositeFilter::Out => format!("out"),
            StyleCompositeFilter::Xor => format!("xor"),
            StyleCompositeFilter::Lighter => format!("lighter"),
            StyleCompositeFilter::Arithmetic(fv) => format!(
                "arithmetic({})",
                fv.iter()
                    .map(|s| format!("{}", s))
                    .collect::<Vec<_>>()
                    .join(", ")
            ),
        }
    }
}

impl PrintAsCssValue for StyleMixBlendMode {
    fn print_as_css_value(&self) -> String {
        format!("{}", self)
    }
}

impl PrintAsCssValue for StyleTextColor {
    fn print_as_css_value(&self) -> String {
        self.inner.to_hash()
    }
}

impl PrintAsCssValue for StyleFontSize {
    fn print_as_css_value(&self) -> String {
        format!("{}", self.inner)
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

impl PrintAsCssValue for StyleTextAlign {
    fn print_as_css_value(&self) -> String {
        String::from(match self {
            StyleTextAlign::Left => "left",
            StyleTextAlign::Center => "center",
            StyleTextAlign::Right => "right",
        })
    }
}

impl PrintAsCssValue for StyleLetterSpacing {
    fn print_as_css_value(&self) -> String {
        format!("{}", self.inner)
    }
}

impl PrintAsCssValue for StyleLineHeight {
    fn print_as_css_value(&self) -> String {
        format!("{}", self.inner)
    }
}

impl PrintAsCssValue for StyleWordSpacing {
    fn print_as_css_value(&self) -> String {
        format!("{}", self.inner)
    }
}

impl PrintAsCssValue for StyleTabWidth {
    fn print_as_css_value(&self) -> String {
        format!("{}", self.inner)
    }
}

impl PrintAsCssValue for StyleCursor {
    fn print_as_css_value(&self) -> String {
        String::from(match self {
            StyleCursor::Alias => "alias",
            StyleCursor::AllScroll => "all-scroll",
            StyleCursor::Cell => "cell",
            StyleCursor::ColResize => "col-resize",
            StyleCursor::ContextMenu => "context-menu",
            StyleCursor::Copy => "copy",
            StyleCursor::Crosshair => "crosshair",
            StyleCursor::Default => "default",
            StyleCursor::EResize => "e-resize",
            StyleCursor::EwResize => "ew-resize",
            StyleCursor::Grab => "grab",
            StyleCursor::Grabbing => "grabbing",
            StyleCursor::Help => "help",
            StyleCursor::Move => "move",
            StyleCursor::NResize => "n-resize",
            StyleCursor::NsResize => "ns-resize",
            StyleCursor::NeswResize => "nesw-resize",
            StyleCursor::NwseResize => "nwse-resize",
            StyleCursor::Pointer => "pointer",
            StyleCursor::Progress => "progress",
            StyleCursor::RowResize => "row-resize",
            StyleCursor::SResize => "s-resize",
            StyleCursor::SeResize => "se-resize",
            StyleCursor::Text => "text",
            StyleCursor::Unset => "unset",
            StyleCursor::VerticalText => "vertical-text",
            StyleCursor::WResize => "w-resize",
            StyleCursor::Wait => "wait",
            StyleCursor::ZoomIn => "zoom-in",
            StyleCursor::ZoomOut => "zoom-out",
        })
    }
}

impl PrintAsCssValue for LayoutDisplay {
    fn print_as_css_value(&self) -> String {
        String::from(match self {
            LayoutDisplay::None => "none",
            LayoutDisplay::Flex => "flex",
            LayoutDisplay::Block => "block",
            LayoutDisplay::InlineBlock => "inline-block",
        })
    }
}

impl PrintAsCssValue for LayoutFloat {
    fn print_as_css_value(&self) -> String {
        String::from(match self {
            LayoutFloat::Left => "left",
            LayoutFloat::Right => "right",
        })
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

impl PrintAsCssValue for LayoutLeft {
    fn print_as_css_value(&self) -> String {
        format!("{}", self.inner)
    }
}

impl PrintAsCssValue for LayoutBottom {
    fn print_as_css_value(&self) -> String {
        format!("{}", self.inner)
    }
}

impl PrintAsCssValue for LayoutFlexWrap {
    fn print_as_css_value(&self) -> String {
        String::from(match self {
            LayoutFlexWrap::Wrap => "wrap",
            LayoutFlexWrap::NoWrap => "nowrap",
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
            LayoutJustifyContent::Start => "start",
            LayoutJustifyContent::End => "end",
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
            LayoutAlignItems::FlexStart => "flex-start",
            LayoutAlignItems::FlexEnd => "flex-end",
        })
    }
}

impl PrintAsCssValue for LayoutAlignContent {
    fn print_as_css_value(&self) -> String {
        String::from(match self {
            LayoutAlignContent::Stretch => "stretch",
            LayoutAlignContent::Center => "center",
            LayoutAlignContent::Start => "start",
            LayoutAlignContent::End => "end",
            LayoutAlignContent::SpaceBetween => "space-between",
            LayoutAlignContent::SpaceAround => "space-around",
        })
    }
}

impl PrintAsCssValue for StyleFilterVec {
    fn print_as_css_value(&self) -> String {
        self.as_ref()
            .iter()
            .map(|f| f.print_as_css_value())
            .collect::<Vec<_>>()
            .join(", ")
    }
}

impl PrintAsCssValue for StyleBackgroundContentVec {
    fn print_as_css_value(&self) -> String {
        self.as_ref()
            .iter()
            .map(|f| f.print_as_css_value())
            .collect::<Vec<_>>()
            .join(", ")
    }
}

impl PrintAsCssValue for StyleBackgroundPositionVec {
    fn print_as_css_value(&self) -> String {
        self.as_ref()
            .iter()
            .map(|f| f.print_as_css_value())
            .collect::<Vec<_>>()
            .join(", ")
    }
}

impl PrintAsCssValue for StyleBackgroundSizeVec {
    fn print_as_css_value(&self) -> String {
        self.as_ref()
            .iter()
            .map(|f| f.print_as_css_value())
            .collect::<Vec<_>>()
            .join(", ")
    }
}

impl PrintAsCssValue for StyleBackgroundRepeatVec {
    fn print_as_css_value(&self) -> String {
        self.as_ref()
            .iter()
            .map(|f| f.print_as_css_value())
            .collect::<Vec<_>>()
            .join(", ")
    }
}

impl PrintAsCssValue for LayoutOverflow {
    fn print_as_css_value(&self) -> String {
        String::from(match self {
            LayoutOverflow::Scroll => "scroll",
            LayoutOverflow::Auto => "auto",
            LayoutOverflow::Hidden => "hidden",
            LayoutOverflow::Visible => "visible",
        })
    }
}

impl PrintAsCssValue for LayoutPaddingTop {
    fn print_as_css_value(&self) -> String {
        format!("{}", self.inner)
    }
}

impl PrintAsCssValue for LayoutPaddingLeft {
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

impl PrintAsCssValue for LayoutMarginTop {
    fn print_as_css_value(&self) -> String {
        format!("{}", self.inner)
    }
}

impl PrintAsCssValue for LayoutMarginLeft {
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

impl PrintAsCssValue for StyleBorderLeftColor {
    fn print_as_css_value(&self) -> String {
        self.inner.to_hash()
    }
}

impl PrintAsCssValue for StyleBorderBottomColor {
    fn print_as_css_value(&self) -> String {
        self.inner.to_hash()
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

impl PrintAsCssValue for StyleBorderLeftStyle {
    fn print_as_css_value(&self) -> String {
        format!("{}", self.inner)
    }
}

impl PrintAsCssValue for StyleBorderBottomStyle {
    fn print_as_css_value(&self) -> String {
        format!("{}", self.inner)
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

impl PrintAsCssValue for LayoutBorderLeftWidth {
    fn print_as_css_value(&self) -> String {
        format!("{}", self.inner)
    }
}

impl PrintAsCssValue for LayoutBorderBottomWidth {
    fn print_as_css_value(&self) -> String {
        format!("{}", self.inner)
    }
}

impl PrintAsCssValue for StyleBoxShadow {
    fn print_as_css_value(&self) -> String {
        format!(
            "{} {} {} {} {} {}",
            self.offset[0],
            self.offset[1],
            self.blur_radius,
            self.spread_radius,
            self.color.to_hash(),
            if self.clip_mode == BoxShadowClipMode::Outset {
                ""
            } else {
                "inset"
            },
        )
    }
}

impl PrintAsCssValue for ScrollbarStyle {
    fn print_as_css_value(&self) -> String {
        format!(
            "horz({}), vert({})",
            self.horizontal.print_as_css_value(),
            self.vertical.print_as_css_value()
        )
    }
}

impl PrintAsCssValue for StyleOpacity {
    fn print_as_css_value(&self) -> String {
        format!("{}", self.inner)
    }
}

impl PrintAsCssValue for StyleTransformVec {
    fn print_as_css_value(&self) -> String {
        self.as_ref()
            .iter()
            .map(|f| f.print_as_css_value())
            .collect::<Vec<_>>()
            .join(", ")
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

// extra ---

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

impl PrintAsCssValue for StyleBackgroundContent {
    fn print_as_css_value(&self) -> String {
        match self {
            StyleBackgroundContent::LinearGradient(lg) => {
                if lg.extend_mode == ExtendMode::Repeat {
                    format!("repeating-linear-gradient({})", lg.print_as_css_value())
                } else {
                    format!("linear-gradient({})", lg.print_as_css_value())
                }
            }
            StyleBackgroundContent::RadialGradient(rg) => {
                if rg.extend_mode == ExtendMode::Repeat {
                    format!("repeating-radial-gradient({})", rg.print_as_css_value())
                } else {
                    format!("radial-gradient({})", rg.print_as_css_value())
                }
            }
            StyleBackgroundContent::ConicGradient(cg) => {
                if cg.extend_mode == ExtendMode::Repeat {
                    format!("repeating-conic-gradient({})", cg.print_as_css_value())
                } else {
                    format!("conic-gradient({})", cg.print_as_css_value())
                }
            }
            StyleBackgroundContent::Image(id) => format!("url(\"{}\")", id.as_str()),
            StyleBackgroundContent::Color(c) => c.to_hash(),
        }
    }
}

impl PrintAsCssValue for LinearGradient {
    fn print_as_css_value(&self) -> String {
        let t = if self.stops.is_empty() { "" } else { ", " };
        format!(
            "{}{}{}",
            match self.direction {
                Direction::Angle(a) => format!("{}", a),
                Direction::FromTo(d) => format!("from {} to {}", d.from, d.to),
            },
            t,
            self.stops
                .iter()
                .map(|s| s.print_as_css_value())
                .collect::<Vec<_>>()
                .join(", ")
        )
    }
}

impl PrintAsCssValue for NormalizedLinearColorStop {
    fn print_as_css_value(&self) -> String {
        format!("{}{}", self.offset, self.color.to_hash())
    }
}

impl PrintAsCssValue for RadialGradient {
    fn print_as_css_value(&self) -> String {
        format!(
            "{} {} at {}, {}",
            match self.shape {
                Shape::Ellipse => "ellipse",
                Shape::Circle => "circle",
            },
            match self.size {
                RadialGradientSize::ClosestSide => "closest-side",
                RadialGradientSize::ClosestCorner => "closest-corner",
                RadialGradientSize::FarthestSide => "farthest-side",
                RadialGradientSize::FarthestCorner => "farthest-corner",
            },
            self.position.print_as_css_value(),
            self.stops
                .iter()
                .map(|s| s.print_as_css_value())
                .collect::<Vec<_>>()
                .join(", ")
        )
    }
}

impl PrintAsCssValue for StyleBackgroundPosition {
    fn print_as_css_value(&self) -> String {
        format!(
            "{} {}",
            match self.horizontal {
                BackgroundPositionHorizontal::Left => format!("left"),
                BackgroundPositionHorizontal::Center => format!("center"),
                BackgroundPositionHorizontal::Right => format!("right"),
                BackgroundPositionHorizontal::Exact(px) => format!("{}", px),
            },
            match self.vertical {
                BackgroundPositionVertical::Top => format!("top"),
                BackgroundPositionVertical::Center => format!("center"),
                BackgroundPositionVertical::Bottom => format!("bottom"),
                BackgroundPositionVertical::Exact(px) => format!("{}", px),
            }
        )
    }
}

impl PrintAsCssValue for StyleBackgroundSize {
    fn print_as_css_value(&self) -> String {
        match self {
            StyleBackgroundSize::ExactSize(p) => format!("{} {}", p[0], p[1]),
            StyleBackgroundSize::Contain => format!("contain"),
            StyleBackgroundSize::Cover => format!("cover"),
        }
    }
}

impl PrintAsCssValue for NormalizedRadialColorStop {
    fn print_as_css_value(&self) -> String {
        format!("{}{}", self.angle, self.color.to_hash())
    }
}

impl PrintAsCssValue for ConicGradient {
    fn print_as_css_value(&self) -> String {
        format!(
            "from {} at {}, {}",
            self.angle,
            self.center.print_as_css_value(),
            self.stops
                .iter()
                .map(|s| s.print_as_css_value())
                .collect::<Vec<_>>()
                .join(", ")
        )
    }
}

impl PrintAsCssValue for StyleBackgroundRepeat {
    fn print_as_css_value(&self) -> String {
        String::from(match self {
            StyleBackgroundRepeat::NoRepeat => "no-repeat",
            StyleBackgroundRepeat::Repeat => "repeat",
            StyleBackgroundRepeat::RepeatX => "repeat-x",
            StyleBackgroundRepeat::RepeatY => "repeat-y",
        })
    }
}

impl PrintAsCssValue for ScrollbarInfo {
    fn print_as_css_value(&self) -> String {
        format!(
            "{} {} {} {} {} {} {} {}",
            self.width,
            self.padding_left,
            self.padding_right,
            self.track.print_as_css_value(),
            self.thumb.print_as_css_value(),
            self.button.print_as_css_value(),
            self.corner.print_as_css_value(),
            self.resizer.print_as_css_value(),
        )
    }
}
