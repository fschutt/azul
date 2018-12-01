//! Contains utilities to convert strings (CSS strings) to servo types

pub use {
    app_units::Au,
    euclid::{TypedSize2D, SideOffsets2D, TypedPoint2D},
    webrender::api::{
        BorderDetails, NormalBorder,
        NinePatchBorder, LayoutPixel, BoxShadowClipMode, ColorU,
        ColorF, LayoutVector2D, Gradient, RadialGradient, LayoutPoint,
        LayoutSize, ExtendMode, LayoutSideOffsets, BorderStyle,
        BorderRadius, BorderSide, LayoutRect,
    },
};

pub(crate) const EM_HEIGHT: f32 = 16.0;
/// WebRender measures in points, not in pixels!
pub(crate) const PT_TO_PX: f32 = 96.0 / 72.0;

// In case no font size is specified for a node,
// this will be substituted as the default font size
pub(crate) const DEFAULT_FONT_SIZE: StyleFontSize = StyleFontSize(PixelValue {
    metric: CssMetric::Px,
    number: FloatValue { number: (100.0 * SCALE_FACTOR) as isize },
});

/// Creates `pt`, `px` and `em` constructors for any struct that has a
/// `PixelValue` as it's self.0 field.
macro_rules! impl_pixel_value {($struct:ident) => (
    impl $struct {
        #[inline]
        pub fn px(value: f32) -> Self {
            $struct(PixelValue::px(value))
        }

        #[inline]
        pub fn em(value: f32) -> Self {
            $struct(PixelValue::em(value))
        }

        #[inline]
        pub fn pt(value: f32) -> Self {
            $struct(PixelValue::pt(value))
        }
    }
)}

/// A property that can be used to style DOM nodes
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum StyleProperty {
    BorderRadius(StyleBorderRadius),
    BackgroundColor(StyleBackgroundColor),
    TextColor(StyleTextColor),
    Border(StyleBorder),
    Background(StyleBackground),
    FontSize(StyleFontSize),
    FontFamily(StyleFontFamily),
    TextAlign(StyleTextAlignmentHorz),
    LetterSpacing(StyleLetterSpacing),
    BoxShadow(StyleBoxShadow),
    LineHeight(StyleLineHeight),

    Width(LayoutWidth),
    Height(LayoutHeight),
    MinWidth(LayoutMinWidth),
    MinHeight(LayoutMinHeight),
    MaxWidth(LayoutMaxWidth),
    MaxHeight(LayoutMaxHeight),
    Position(LayoutPosition),
    Top(LayoutTop),
    Right(LayoutRight),
    Left(LayoutLeft),
    Bottom(LayoutBottom),
    Padding(LayoutPadding),
    Margin(LayoutMargin),
    FlexWrap(LayoutWrap),
    FlexDirection(LayoutDirection),
    FlexGrow(LayoutFlexGrow),
    FlexShrink(LayoutFlexShrink),
    JustifyContent(LayoutJustifyContent),
    AlignItems(LayoutAlignItems),
    AlignContent(LayoutAlignContent),
    Overflow(LayoutOverflow),
}

impl StyleProperty {
    /// Returns whether this property will be inherited during cascading
    pub fn is_inheritable(&self) -> bool {
        use self::StyleProperty::*;
        match self {
            | TextColor(_)
            | FontFamily(_)
            | FontSize(_)
            | LineHeight(_)
            | TextAlign(_) => true,
            _ => false,
        }
    }
}

impl_from!(StyleBorderRadius, StyleProperty::BorderRadius);
impl_from!(StyleBackground, StyleProperty::Background);
impl_from!(StyleBoxShadow, StyleProperty::BoxShadow);
impl_from!(StyleBorder, StyleProperty::Border);
impl_from!(StyleFontSize, StyleProperty::FontSize);
impl_from!(StyleFontFamily, StyleProperty::FontFamily);
impl_from!(StyleTextAlignmentHorz, StyleProperty::TextAlign);
impl_from!(StyleLineHeight, StyleProperty::LineHeight);
impl_from!(StyleLetterSpacing, StyleProperty::LetterSpacing);
impl_from!(StyleBackgroundColor, StyleProperty::BackgroundColor);
impl_from!(StyleTextColor, StyleProperty::TextColor);

impl_from!(LayoutOverflow, StyleProperty::Overflow);
impl_from!(LayoutWidth, StyleProperty::Width);
impl_from!(LayoutHeight, StyleProperty::Height);
impl_from!(LayoutMinWidth, StyleProperty::MinWidth);
impl_from!(LayoutMinHeight, StyleProperty::MinHeight);
impl_from!(LayoutMaxWidth, StyleProperty::MaxWidth);
impl_from!(LayoutMaxHeight, StyleProperty::MaxHeight);

impl_from!(LayoutPosition, StyleProperty::Position);
impl_from!(LayoutTop, StyleProperty::Top);
impl_from!(LayoutBottom, StyleProperty::Bottom);
impl_from!(LayoutRight, StyleProperty::Right);
impl_from!(LayoutLeft, StyleProperty::Left);

impl_from!(LayoutPadding, StyleProperty::Padding);
impl_from!(LayoutMargin, StyleProperty::Margin);

impl_from!(LayoutWrap, StyleProperty::FlexWrap);
impl_from!(LayoutDirection, StyleProperty::FlexDirection);
impl_from!(LayoutFlexGrow, StyleProperty::FlexGrow);
impl_from!(LayoutFlexShrink, StyleProperty::FlexShrink);
impl_from!(LayoutJustifyContent, StyleProperty::JustifyContent);
impl_from!(LayoutAlignItems, StyleProperty::AlignItems);
impl_from!(LayoutAlignContent, StyleProperty::AlignContent);

const SCALE_FACTOR: f32 = 10000.0;

#[derive(Debug, Copy, Clone, Hash, PartialEq, Eq)]
pub struct PixelValue {
    pub metric: CssMetric,
    pub number: FloatValue,
}

impl PixelValue {
    #[inline]
    pub fn px(value: f32) -> Self {
        Self::from_metric(CssMetric::Px, value)
    }

    #[inline]
    pub fn em(value: f32) -> Self {
        Self::from_metric(CssMetric::Em, value)
    }

    #[inline]
    pub fn pt(value: f32) -> Self {
        Self::from_metric(CssMetric::Pt, value)
    }

    #[inline]
    pub fn from_metric(metric: CssMetric, value: f32) -> Self {
        Self {
            metric: metric,
            number: value.into(),
        }
    }

    #[inline]
    pub fn to_pixels(&self) -> f32 {
        match self.metric {
            CssMetric::Px => { self.number.get() },
            CssMetric::Pt => { (self.number.get()) * PT_TO_PX },
            CssMetric::Em => { (self.number.get()) * EM_HEIGHT },
        }
    }
}

/// "100%" or "1.0" value - usize based, so it can be
/// safely hashed, accurate to 4 decimal places
#[derive(Debug, PartialEq, Copy, Clone, Hash, Eq)]
pub struct PercentageValue {
    /// Normalized value, 100% = 1.0
    number: isize,
}

impl PercentageValue {
    pub fn new(value: f32) -> Self {
        Self { number: (value * SCALE_FACTOR) as isize }
    }

    pub fn get(&self) -> f32 {
        self.number as f32 / SCALE_FACTOR
    }
}

#[derive(Debug, PartialEq, Copy, Clone, Hash, Eq)]
pub struct FloatValue {
    number: isize,
}

impl FloatValue {
    pub fn new(value: f32) -> Self {
        Self { number: (value * SCALE_FACTOR) as isize }
    }

    pub fn get(&self) -> f32 {
        self.number as f32 / SCALE_FACTOR
    }
}

impl From<f32> for FloatValue {
    fn from(val: f32) -> Self {
        Self::new(val)
    }
}

#[derive(Debug, PartialEq, Clone, Copy, Hash, Eq)]
pub enum CssMetric {
    Px,
    Pt,
    Em,
}

#[derive(Debug, Copy, Clone, PartialEq)]
pub enum CssColorComponent {
    Red,
    Green,
    Blue,
    Hue,
    Saturation,
    Lightness,
    Alpha,
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash)]
pub struct StyleBorderRadius {
    pub top_left: [PixelValue;2],
    pub top_right: [PixelValue;2],
    pub bottom_left: [PixelValue;2],
    pub bottom_right: [PixelValue;2],
}

impl StyleBorderRadius {

    pub fn zero() -> Self {
        const ZERO_PX: PixelValue = PixelValue { number: FloatValue { number: (0.0 * SCALE_FACTOR) as isize }, metric: CssMetric::Px };
        Self::uniform(ZERO_PX)
    }

    pub fn uniform(value: PixelValue) -> Self {
        Self {
            top_left: [value, value],
            top_right: [value, value],
            bottom_left: [value, value],
            bottom_right: [value, value],
        }
    }
}

impl From<StyleBorderRadius> for BorderRadius {
    fn from(radius: StyleBorderRadius) -> BorderRadius {
        Self {
            top_left: LayoutSize::new(radius.top_left[0].to_pixels(), radius.top_left[1].to_pixels()),
            top_right: LayoutSize::new(radius.top_right[0].to_pixels(), radius.top_right[1].to_pixels()),
            bottom_left: LayoutSize::new(radius.bottom_left[0].to_pixels(), radius.bottom_left[1].to_pixels()),
            bottom_right: LayoutSize::new(radius.bottom_right[0].to_pixels(), radius.bottom_right[1].to_pixels()),
        }
    }
}

/// Represents a parsed CSS `background-color` attribute
#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash)]
pub struct StyleBackgroundColor(pub ColorU);

impl Default for StyleBackgroundColor {
    fn default() -> Self {
        // Transparent color
        StyleBackgroundColor(ColorU::new(0, 0, 0, 0))
    }
}

/// Represents a parsed CSS `color` attribute
#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash)]
pub struct StyleTextColor(pub ColorU);

/// Represents a parsed CSS `padding` attribute
#[derive(Debug, Default, Copy, Clone, PartialEq, Eq, Hash)]
pub struct LayoutPadding {
    pub top: Option<PixelValue>,
    pub bottom: Option<PixelValue>,
    pub left: Option<PixelValue>,
    pub right: Option<PixelValue>,
}

// $struct_name has to have top, left, right, bottom properties
macro_rules! merge_struct {($struct_name:ident) => (
impl $struct_name {
    pub fn merge(a: &mut Option<$struct_name>, b: &$struct_name) {
       if let Some(ref mut existing) = a {
           if b.top.is_some() { existing.top = b.top; }
           if b.bottom.is_some() { existing.bottom = b.bottom; }
           if b.left.is_some() { existing.left = b.left; }
           if b.right.is_some() { existing.right = b.right; }
       } else {
           *a = Some(*b);
       }
    }
})}

macro_rules! struct_all {($struct_name:ident, $field_type:ty) => (
impl $struct_name {
    /// Sets all of the fields (top, left, right, bottom) to `Some(field)`
    pub fn all(field: $field_type) -> Self {
        Self {
            top: Some(field),
            right: Some(field),
            left: Some(field),
            bottom: Some(field),
        }
    }
})}

merge_struct!(LayoutPadding);
merge_struct!(LayoutMargin);
struct_all!(LayoutPadding, PixelValue);
struct_all!(LayoutMargin, PixelValue);

/// Represents a parsed CSS `padding` attribute
#[derive(Debug, Default, Copy, Clone, PartialEq, Eq, Hash)]
pub struct LayoutMargin {
    pub top: Option<PixelValue>,
    pub bottom: Option<PixelValue>,
    pub left: Option<PixelValue>,
    pub right: Option<PixelValue>,
}

/// Wrapper for the `overflow-{x,y}` + `overflow` property
#[derive(Debug, Default, Copy, Clone, PartialEq, Eq, Hash)]
pub struct LayoutOverflow {
    pub horizontal: TextOverflowBehaviour,
    pub vertical: TextOverflowBehaviour,
}

impl LayoutOverflow {

    // "merges" two LayoutOverflow properties
    pub fn merge(a: &mut Option<Self>, b: &Self) {

        fn merge_property(p: &mut TextOverflowBehaviour, other: &TextOverflowBehaviour) {
            if *other == TextOverflowBehaviour::NotModified {
                return;
            }
            *p = *other;
        }

        if let Some(ref mut existing_overflow) = a {
            merge_property(&mut existing_overflow.horizontal, &b.horizontal);
            merge_property(&mut existing_overflow.vertical, &b.vertical);
        } else {
            *a = Some(*b)
        }
    }

    pub fn allows_horizontal_overflow(&self) -> bool {
        self.horizontal.can_overflow()
    }

    pub fn allows_vertical_overflow(&self) -> bool {
        self.vertical.can_overflow()
    }

    // If this overflow setting should show the horizontal scrollbar
    pub fn allows_horizontal_scrollbar(&self) -> bool {
        self.allows_horizontal_overflow()
    }

    pub fn allows_vertical_scrollbar(&self) -> bool {
        self.allows_vertical_overflow()
    }
}

#[derive(Debug, Default, Copy, Clone, PartialEq, Eq, Hash)]
pub struct StyleBorder {
    pub top: Option<StyleBorderSide>,
    pub left: Option<StyleBorderSide>,
    pub bottom: Option<StyleBorderSide>,
    pub right: Option<StyleBorderSide>,
}

merge_struct!(StyleBorder);
struct_all!(StyleBorder, StyleBorderSide);

impl StyleBorder {

    /// Returns the merged offsets and details for the top, left,
    /// right and bottom styles - necessary, so we can combine `border-top`,
    /// `border-left`, etc. into one border
    pub fn get_webrender_border(&self, border_radius: Option<StyleBorderRadius>) -> Option<(LayoutSideOffsets, BorderDetails)> {
        match (self.top, self.left, self.bottom, self.right) {
            (None, None, None, None) => None,
            (top, left, bottom, right) => {

                // Widths
                let border_width_top = top.and_then(|top|  Some(top.border_width.to_pixels())).unwrap_or(0.0);
                let border_width_bottom = bottom.and_then(|bottom|  Some(bottom.border_width.to_pixels())).unwrap_or(0.0);
                let border_width_left = left.and_then(|left|  Some(left.border_width.to_pixels())).unwrap_or(0.0);
                let border_width_right = right.and_then(|right|  Some(right.border_width.to_pixels())).unwrap_or(0.0);

                // Color
                let border_color_top = top.and_then(|top| Some(top.border_color.into())).unwrap_or(DEFAULT_BORDER_COLOR);
                let border_color_bottom = bottom.and_then(|bottom| Some(bottom.border_color.into())).unwrap_or(DEFAULT_BORDER_COLOR);
                let border_color_left = left.and_then(|left| Some(left.border_color.into())).unwrap_or(DEFAULT_BORDER_COLOR);
                let border_color_right = right.and_then(|right| Some(right.border_color.into())).unwrap_or(DEFAULT_BORDER_COLOR);

                // Styles
                let border_style_top = top.and_then(|top| Some(top.border_style)).unwrap_or(DEFAULT_BORDER_STYLE);
                let border_style_bottom = bottom.and_then(|bottom| Some(bottom.border_style)).unwrap_or(DEFAULT_BORDER_STYLE);
                let border_style_left = left.and_then(|left| Some(left.border_style)).unwrap_or(DEFAULT_BORDER_STYLE);
                let border_style_right = right.and_then(|right| Some(right.border_style)).unwrap_or(DEFAULT_BORDER_STYLE);

                // Webrender crashes if AA is disabled and the border isn't pure-solid
                let is_not_solid = [border_style_top, border_style_bottom, border_style_left, border_style_right].iter().any(|style| {
                    *style != BorderStyle::Solid
                });

                let border_widths = LayoutSideOffsets::new(border_width_top, border_width_right, border_width_bottom, border_width_left);
                let border_details = BorderDetails::Normal(NormalBorder {
                    top: BorderSide { color:  border_color_top.into(), style: border_style_top },
                    left: BorderSide { color:  border_color_left.into(), style: border_style_left },
                    right: BorderSide { color:  border_color_right.into(),  style: border_style_right },
                    bottom: BorderSide { color:  border_color_bottom.into(), style: border_style_bottom },
                    radius: border_radius.and_then(|b| Some(b.into())).unwrap_or(BorderRadius::zero()),
                    do_aa: border_radius.is_some() || is_not_solid,
                });

                Some((border_widths, border_details))
            }
        }
    }
}

const DEFAULT_BORDER_STYLE: BorderStyle = BorderStyle::Solid;
const DEFAULT_BORDER_COLOR: ColorU = ColorU { r: 0, g: 0, b: 0, a: 255 };

#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash)]
pub struct StyleBorderSide {
    pub border_width: PixelValue,
    pub border_style: BorderStyle,
    pub border_color: ColorU,
}

#[derive(Debug, Default, Copy, Clone, PartialEq, Eq, Hash)]
pub struct StyleBoxShadow {
    pub top: Option<Option<BoxShadowPreDisplayItem>>,
    pub left: Option<Option<BoxShadowPreDisplayItem>>,
    pub bottom: Option<Option<BoxShadowPreDisplayItem>>,
    pub right: Option<Option<BoxShadowPreDisplayItem>>,
}

merge_struct!(StyleBoxShadow);
struct_all!(StyleBoxShadow, Option<BoxShadowPreDisplayItem>);

// missing StyleBorderRadius & LayoutRect
#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash)]
pub struct BoxShadowPreDisplayItem {
    pub offset: [PixelValue;2],
    pub color: ColorU,
    pub blur_radius: PixelValue,
    pub spread_radius: PixelValue,
    pub clip_mode: BoxShadowClipMode,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum StyleBackground {
    LinearGradient(LinearGradientPreInfo),
    RadialGradient(RadialGradientPreInfo),
    Image(CssImageId),
    NoBackground,
}

impl<'a> From<CssImageId> for StyleBackground {
    fn from(id: CssImageId) -> Self {
        StyleBackground::Image(id)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct LinearGradientPreInfo {
    pub direction: Direction,
    pub extend_mode: ExtendMode,
    pub stops: Vec<GradientStopPre>,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct RadialGradientPreInfo {
    pub shape: Shape,
    pub extend_mode: ExtendMode,
    pub stops: Vec<GradientStopPre>,
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash)]
pub enum Direction {
    Angle(FloatValue),
    FromTo(DirectionCorner, DirectionCorner),
}

impl Direction {
    /// Calculates the points of the gradient stops for angled linear gradients
    pub fn to_points(&self, rect: &LayoutRect)
    -> (LayoutPoint, LayoutPoint)
    {
        match self {
            Direction::Angle(deg) => {
                // note: assumes that the LayoutRect has positive sides

                // see: https://hugogiraudel.com/2013/02/04/css-gradients/

                let deg = deg.get(); // FloatValue -> f32

                let deg = -deg; // negate winding direction

                let width_half = rect.size.width as usize / 2;
                let height_half = rect.size.height as usize / 2;

                // hypotenuse_len is the length of the center of the rect to the corners
                let hypotenuse_len = (((width_half * width_half) + (height_half * height_half)) as f64).sqrt();

                // The corner also serves to determine what quadrant we're in
                // Get the quadrant (corner) the angle is in and get the degree associated
                // with that corner.

                let angle_to_top_left = (height_half as f64 / width_half as f64).atan().to_degrees();

                // We need to calculate the angle from the center to the corner!
                let ending_point_degrees = if deg < 90.0 {
                    // top left corner
                    90.0 - angle_to_top_left
                } else if deg < 180.0 {
                    // bottom left corner
                    90.0 + angle_to_top_left
                } else if deg < 270.0 {
                    // bottom right corner
                    270.0 - angle_to_top_left
                } else /* deg > 270.0 && deg < 360.0 */ {
                    // top right corner
                    270.0 + angle_to_top_left
                };

                // assuming deg = 36deg, then degree_diff_to_corner = 9deg
                let degree_diff_to_corner = ending_point_degrees - deg as f64;

                // Searched_len is the distance between the center of the rect and the
                // ending point of the gradient
                let searched_len = (hypotenuse_len * degree_diff_to_corner.to_radians().cos()).abs();

                // TODO: This searched_len is incorrect...

                // Once we have the length, we can simply rotate the length by the angle,
                // then translate it to the center of the rect
                let dx = deg.to_radians().sin() * searched_len as f32;
                let dy = deg.to_radians().cos() * searched_len as f32;

                let start_point_location = LayoutPoint::new(width_half as f32 + dx, height_half as f32 + dy);
                let end_point_location = LayoutPoint::new(width_half as f32 - dx, height_half as f32 - dy);

                (start_point_location, end_point_location)
            },
            Direction::FromTo(from, to) => {
                (from.to_point(rect), to.to_point(rect))
            }
        }
    }
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash)]
pub enum Shape {
    Ellipse,
    Circle,
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash)]
pub enum DirectionCorner {
    Right,
    Left,
    Top,
    Bottom,
    TopRight,
    TopLeft,
    BottomRight,
    BottomLeft,
}

impl DirectionCorner {

    pub fn opposite(&self) -> Self {
        use self::DirectionCorner::*;
        match *self {
            Right => Left,
            Left => Right,
            Top => Bottom,
            Bottom => Top,
            TopRight => BottomLeft,
            BottomLeft => TopRight,
            TopLeft => BottomRight,
            BottomRight => TopLeft,
        }
    }

    pub fn combine(&self, other: &Self) -> Option<Self> {
        use self::DirectionCorner::*;
        match (*self, *other) {
            (Right, Top) | (Top, Right) => Some(TopRight),
            (Left, Top) | (Top, Left) => Some(TopLeft),
            (Right, Bottom) | (Bottom, Right) => Some(BottomRight),
            (Left, Bottom) | (Bottom, Left) => Some(BottomLeft),
            _ => { None }
        }
    }

    pub fn to_point(&self, rect: &LayoutRect) -> TypedPoint2D<f32, LayoutPixel>
    {
        use self::DirectionCorner::*;
        match *self {
            Right => TypedPoint2D::new(rect.size.width, rect.size.height / 2.0),
            Left => TypedPoint2D::new(0.0, rect.size.height / 2.0),
            Top => TypedPoint2D::new(rect.size.width / 2.0, 0.0),
            Bottom => TypedPoint2D::new(rect.size.width / 2.0, rect.size.height),
            TopRight =>  TypedPoint2D::new(rect.size.width, 0.0),
            TopLeft => TypedPoint2D::new(0.0, 0.0),
            BottomRight => TypedPoint2D::new(rect.size.width, rect.size.height),
            BottomLeft => TypedPoint2D::new(0.0, rect.size.height),
        }
    }
}

#[derive(Debug, Copy, Clone, Eq, PartialEq, Hash)]
pub enum BackgroundType {
    LinearGradient,
    RepeatingLinearGradient,
    RadialGradient,
    RepeatingRadialGradient,
    Image,
}

/// Note: In theory, we could take a String here,
/// but this leads to horrible lifetime issues. Also
/// since we only parse the CSS once (at startup),
/// the performance is absolutely negligible.
///
/// However, this allows the `Css` struct to be independent
/// of the original source text, i.e. the original CSS string
/// can be deallocated after successfully parsing it.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct CssImageId(pub String);

#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash)]
pub struct GradientStopPre {
    pub offset: Option<PercentageValue>, // this is set to None if there was no offset that could be parsed
    pub color: ColorU,
}

/// Represents a parsed CSS `width` attribute
#[derive(Debug, PartialEq, Eq, Copy, Clone, Hash)]
pub struct LayoutWidth(pub PixelValue);
/// Represents a parsed CSS `min-width` attribute
#[derive(Debug, PartialEq, Eq, Copy, Clone, Hash)]
pub struct LayoutMinWidth(pub PixelValue);
/// Represents a parsed CSS `max-width` attribute
#[derive(Debug, PartialEq, Eq, Copy, Clone, Hash)]
pub struct LayoutMaxWidth(pub PixelValue);
/// Represents a parsed CSS `height` attribute
#[derive(Debug, PartialEq, Eq, Copy, Clone, Hash)]
pub struct LayoutHeight(pub PixelValue);
/// Represents a parsed CSS `min-height` attribute
#[derive(Debug, PartialEq, Eq, Copy, Clone, Hash)]
pub struct LayoutMinHeight(pub PixelValue);
/// Represents a parsed CSS `max-height` attribute
#[derive(Debug, PartialEq, Eq, Copy, Clone, Hash)]
pub struct LayoutMaxHeight(pub PixelValue);

/// Represents a parsed CSS `top` attribute
#[derive(Debug, PartialEq, Eq, Copy, Clone, Hash)]
pub struct LayoutTop(pub PixelValue);
/// Represents a parsed CSS `left` attribute
#[derive(Debug, PartialEq, Eq, Copy, Clone, Hash)]
pub struct LayoutLeft(pub PixelValue);
/// Represents a parsed CSS `right` attribute
#[derive(Debug, PartialEq, Eq, Copy, Clone, Hash)]
pub struct LayoutRight(pub PixelValue);
/// Represents a parsed CSS `bottom` attribute
#[derive(Debug, PartialEq, Eq, Copy, Clone, Hash)]
pub struct LayoutBottom(pub PixelValue);

/// Represents a parsed CSS `flex-grow` attribute
#[derive(Debug, PartialEq, Eq, Copy, Clone, Hash)]
pub struct LayoutFlexGrow(pub FloatValue);
/// Represents a parsed CSS `flex-shrink` attribute
#[derive(Debug, PartialEq, Eq, Copy, Clone, Hash)]
pub struct LayoutFlexShrink(pub FloatValue);

/// Represents a parsed CSS `flex-direction` attribute - default: `Column`
#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash)]
pub enum LayoutDirection {
    Row,
    RowReverse,
    Column,
    ColumnReverse,
}

/// Represents a parsed CSS `line-height` attribute
#[derive(Debug, PartialEq, Eq, Copy, Clone, Hash)]
pub struct StyleLineHeight(pub PercentageValue);
/// Represents a parsed CSS `letter-spacing` attribute
#[derive(Debug, PartialEq, Eq, Copy, Clone, Hash)]
pub struct StyleLetterSpacing(pub PixelValue);

/// Same as the `LayoutDirection`, but without the `-reverse` properties, used in the layout solver,
/// makes decisions based on horizontal / vertical direction easier to write.
/// Use `LayoutDirection::get_axis()` to get the axis for a given `LayoutDirection`.
#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash)]
pub enum LayoutAxis {
    Horizontal,
    Vertical,
}

impl LayoutDirection {
    pub fn get_axis(&self) -> LayoutAxis {
        use self::{LayoutAxis::*, LayoutDirection::*};
        match self {
            Row | RowReverse => Horizontal,
            Column | ColumnReverse => Vertical,
        }
    }

    /// Returns true, if this direction is a `column-reverse` or `row-reverse` direction
    pub fn is_reverse(&self) -> bool {
        *self == LayoutDirection::RowReverse || *self == LayoutDirection::ColumnReverse
    }
}

/// Represents a parsed CSS `position` attribute - default: `Static`
///
/// NOTE: No inline positioning is supported.
#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash)]
pub enum LayoutPosition {
    Static,
    Relative,
    Absolute,
}

impl Default for LayoutPosition {
    fn default() -> Self {
        LayoutPosition::Static
    }
}

impl Default for LayoutDirection {
    fn default() -> Self {
        LayoutDirection::Column
    }
}

/// Represents a parsed CSS `flex-wrap` attribute - default: `Wrap`
#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash)]
pub enum LayoutWrap {
    Wrap,
    NoWrap,
}

impl Default for LayoutWrap {
    fn default() -> Self {
        LayoutWrap::Wrap
    }
}

/// Represents a parsed CSS `justify-content` attribute
#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash)]
pub enum LayoutJustifyContent {
    /// Default value. Items are positioned at the beginning of the container
    Start,
    /// Items are positioned at the end of the container
    End,
    /// Items are positioned at the center of the container
    Center,
    /// Items are positioned with space between the lines
    SpaceBetween,
    /// Items are positioned with space before, between, and after the lines
    SpaceAround,
}

impl Default for LayoutJustifyContent {
    fn default() -> Self {
        LayoutJustifyContent::Start
    }
}

/// Represents a parsed CSS `align-items` attribute
#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash)]
pub enum LayoutAlignItems {
    /// Items are stretched to fit the container
    Stretch,
    /// Items are positioned at the center of the container
    Center,
    /// Items are positioned at the beginning of the container
    Start,
    /// Items are positioned at the end of the container
    End,
}

impl Default for LayoutAlignItems {
    fn default() -> Self {
        LayoutAlignItems::Stretch
    }
}

/// Represents a parsed CSS `align-content` attribute
#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash)]
pub enum LayoutAlignContent {
    /// Default value. Lines stretch to take up the remaining space
    Stretch,
    /// Lines are packed toward the center of the flex container
    Center,
    /// Lines are packed toward the start of the flex container
    Start,
    /// Lines are packed toward the end of the flex container
    End,
    /// Lines are evenly distributed in the flex container
    SpaceBetween,
    /// Lines are evenly distributed in the flex container, with half-size spaces on either end
    SpaceAround,
}

/// Represents a parsed CSS `overflow` attribute
///
/// NOTE: This is split into `NotModified` and `Modified`
/// in order to be able to "merge" `overflow-x` and `overflow-y`
/// into one property.
#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash)]
pub enum TextOverflowBehaviour {
    NotModified,
    Modified(TextOverflowBehaviourInner),
}

impl TextOverflowBehaviour {
    pub fn can_overflow(&self) -> bool {
        use self::TextOverflowBehaviour::*;
        use self::TextOverflowBehaviourInner::*;
        match self {
            Modified(m) => match m {
                Scroll | Auto => true,
                Hidden | Visible => false,
            },
            // default: allow horizontal overflow
            NotModified => false,
        }
    }

    pub fn clips_children(&self) -> bool {
        use self::TextOverflowBehaviour::*;
        use self::TextOverflowBehaviourInner::*;
        match self {
            Modified(m) => match m {
                Scroll | Auto | Hidden => true,
                Visible => false,
            },
            // default: allow horizontal overflow
            NotModified => false,
        }
    }
}

impl Default for TextOverflowBehaviour {
    fn default() -> Self {
        TextOverflowBehaviour::NotModified
    }
}

/// Represents a parsed CSS `overflow-x` or `overflow-y` property, see
/// [`TextOverflowBehaviour`](./struct.TextOverflowBehaviour.html) - default: `Auto`
#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash)]
pub enum TextOverflowBehaviourInner {
    /// Always shows a scroll bar, overflows on scroll
    Scroll,
    /// Does not show a scroll bar by default, only when text is overflowing
    Auto,
    /// Never shows a scroll bar, simply clips text
    Hidden,
    /// Doesn't show a scroll bar, simply overflows the text
    Visible,
}

impl Default for TextOverflowBehaviourInner {
    fn default() -> Self {
        TextOverflowBehaviourInner::Auto
    }
}

/// Horizontal text alignment enum (left, center, right) - default: `Center`
#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash)]
pub enum StyleTextAlignmentHorz {
    Left,
    Center,
    Right,
}

impl Default for StyleTextAlignmentHorz {
    fn default() -> Self {
        StyleTextAlignmentHorz::Center
    }
}

/// Vertical text alignment enum (top, center, bottom) - default: `Center`
#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash)]
pub enum StyleTextAlignmentVert {
    Top,
    Center,
    Bottom,
}

impl Default for StyleTextAlignmentVert {
    fn default() -> Self {
        StyleTextAlignmentVert::Center
    }
}

/// Stylistic options of the rectangle that don't influence the layout
/// (todo: border-box?)
#[derive(Default, Debug, Clone, PartialEq, Hash)]
pub(crate) struct RectStyle {
    /// Background color of this rectangle
    pub(crate) background_color: Option<StyleBackgroundColor>,
    /// Shadow color
    pub(crate) box_shadow: Option<StyleBoxShadow>,
    /// Gradient (location) + stops
    pub(crate) background: Option<StyleBackground>,
    /// Border
    pub(crate) border: Option<StyleBorder>,
    /// Border radius
    pub(crate) border_radius: Option<StyleBorderRadius>,
    /// Font size
    pub(crate) font_size: Option<StyleFontSize>,
    /// Font name / family
    pub(crate) font_family: Option<StyleFontFamily>,
    /// Text color
    pub(crate) font_color: Option<StyleTextColor>,
    /// Text alignment
    pub(crate) text_align: Option<StyleTextAlignmentHorz,>,
    /// Text overflow behaviour
    pub(crate) overflow: Option<LayoutOverflow>,
    /// `line-height` property
    pub(crate) line_height: Option<StyleLineHeight>,
    /// `letter-spacing` property (modifies the width and height)
    pub(crate) letter_spacing: Option<StyleLetterSpacing>,
}

impl_pixel_value!(StyleLetterSpacing);

// Layout constraints for a given rectangle, such as "width", "min-width", "height", etc.
#[derive(Default, Debug, Copy, Clone, PartialEq, Hash)]
pub struct RectLayout {

    pub width: Option<LayoutWidth>,
    pub height: Option<LayoutHeight>,
    pub min_width: Option<LayoutMinWidth>,
    pub min_height: Option<LayoutMinHeight>,
    pub max_width: Option<LayoutMaxWidth>,
    pub max_height: Option<LayoutMaxHeight>,

    pub position: Option<LayoutPosition>,
    pub top: Option<LayoutTop>,
    pub bottom: Option<LayoutBottom>,
    pub right: Option<LayoutRight>,
    pub left: Option<LayoutLeft>,

    pub padding: Option<LayoutPadding>,
    pub margin: Option<LayoutMargin>,

    pub direction: Option<LayoutDirection>,
    pub wrap: Option<LayoutWrap>,
    pub flex_grow: Option<LayoutFlexGrow>,
    pub flex_shrink: Option<LayoutFlexShrink>,
    pub justify_content: Option<LayoutJustifyContent>,
    pub align_items: Option<LayoutAlignItems>,
    pub align_content: Option<LayoutAlignContent>,
}

impl_pixel_value!(LayoutWidth);

impl_pixel_value!(LayoutHeight);
impl_pixel_value!(LayoutMinHeight);
impl_pixel_value!(LayoutMinWidth);
impl_pixel_value!(LayoutMaxWidth);
impl_pixel_value!(LayoutMaxHeight);

impl_pixel_value!(LayoutTop);
impl_pixel_value!(LayoutBottom);
impl_pixel_value!(LayoutRight);
impl_pixel_value!(LayoutLeft);

#[derive(Debug, PartialEq, Eq, Copy, Clone, Hash)]
pub struct StyleFontSize(pub PixelValue);

impl_pixel_value!(StyleFontSize);

impl StyleFontSize {
    pub fn to_pixels(&self) -> f32 {
        self.0.to_pixels()
    }
}

#[derive(Debug, PartialEq, Clone, Eq, Hash)]
pub struct StyleFontFamily {
    // fonts in order of precedence, i.e. "Webly Sleeky UI", "monospace", etc.
    pub fonts: Vec<FontId>
}

#[derive(Debug, PartialEq, Eq, Clone, Hash)]
pub enum FontId {
    BuiltinFont(String),
    ExternalFont(String),
}

/// CssColor is simply a wrapper around the internal CSS color parsing methods.
///
/// Sometimes you'd want to load and parse a CSS color, but you don't want to
/// write your own parser for that. Since Azul already has a parser for CSS colors,
/// this API exposes
#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash)]
pub struct CssColor {
    internal: ColorU,
}

impl CssColor {
    /// Returns the internal parsed color, but in a `0.0 - 1.0` range instead of `0 - 255`
    pub fn to_color_f(&self) -> ColorF {
        self.internal.into()
    }

    /// Returns the internal parsed color
    pub fn to_color_u(&self) -> ColorU {
        self.internal
    }

    /// If `prefix_hash` is set to false, you only get the string, without a hash, in lowercase
    ///
    /// If `self.alpha` is `FF`, it will be omitted from the final result (since `FF` is the default for CSS colors)
    pub fn to_string(&self, prefix_hash: bool) -> String {
        let prefix = if prefix_hash { "#" } else { "" };
        let alpha = if self.internal.a == 255 { String::new() } else { format!("{:02x}", self.internal.a) };
        format!("{}{:02x}{:02x}{:02x}{}", prefix, self.internal.r, self.internal.g, self.internal.b, alpha)
    }
}

impl From<ColorU> for CssColor {
    fn from(color: ColorU) -> Self {
        CssColor { internal: color }
    }
}

impl From<ColorF> for CssColor {
    fn from(color: ColorF) -> Self {
        CssColor { internal: color.into() }
    }
}

impl Into<ColorF> for CssColor {
    fn into(self) -> ColorF {
        self.to_color_f()
    }
}

impl Into<ColorU> for CssColor {
    fn into(self) -> ColorU {
        self.to_color_u()
    }
}

impl Into<String> for CssColor {
    fn into(self) -> String {
        self.to_string(false)
    }
}

#[cfg(feature = "serde_serialization")]
use serde::{de, Serialize, Deserialize, Serializer, Deserializer};

#[cfg(feature = "serde_serialization")]
impl Serialize for CssColor {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where S: Serializer,
    {
        let prefix_css_color_with_hash = true;
        serializer.serialize_str(&self.to_string(prefix_css_color_with_hash))
    }
}

#[cfg(feature = "serde_serialization")]
impl<'de> Deserialize<'de> for CssColor {
    fn deserialize<D>(deserializer: D) -> Result<CssColor, D::Error>
    where D: Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        CssColor::from_str(&s).map_err(de::Error::custom)
    }
}
