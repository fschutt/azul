//! Avatar widget — a circular container showing either an image or short
//! initials text, in one of three size variants. A stateless widget (no
//! callbacks), a styled near-clone of [`crate::widgets::label::Label`] /
//! [`crate::widgets::button::Button`] (image-or-text content) rendered as a
//! `border-radius: 50%` circle.
//!
//! If an [`ImageRef`] is set it is rendered (clipped to the circle); otherwise
//! the `initials` string is shown centred on a neutral background.
//!
//! TODO2: the circular image relies on `overflow: hidden` + `border-radius` on
//! the container clipping the child image; whether the renderer clips a child
//! image to the parent's rounded corners is not GUI-verified here, so the image
//! is *also* given its own matching `border-radius` as a fallback.
//!
//! Key types: [`Avatar`], [`AvatarSize`].

use azul_core::{
    dom::{Dom, IdOrClass, IdOrClass::Class, IdOrClassVec},
    resources::{ImageRef, OptionImageRef},
};
use azul_css::dynamic_selector::{CssPropertyWithConditions, CssPropertyWithConditionsVec};
use azul_css::{
    props::{
        basic::{color::ColorU, StyleFontSize},
        layout::{LayoutDisplay, LayoutFlexDirection, LayoutJustifyContent, LayoutAlignItems, LayoutAlignSelf, LayoutFlexGrow, LayoutWidth, LayoutHeight, LayoutOverflow},
        property::{CssProperty, *},
        style::{StyleBackgroundContent, StyleBackgroundContentVec, StyleBorderTopLeftRadius, StyleBorderTopRightRadius, StyleBorderBottomLeftRadius, StyleBorderBottomRightRadius, StyleTextAlign, StyleTextColor},
    },
    AzString,
};

static AVATAR_CLASS: &[IdOrClass] = &[Class(AzString::from_const_str("__azul-native-avatar"))];
static AVATAR_IMAGE_CLASS: &[IdOrClass] =
    &[Class(AzString::from_const_str("__azul-native-avatar-image"))];
static AVATAR_INITIALS_CLASS: &[IdOrClass] = &[Class(AzString::from_const_str(
    "__azul-native-avatar-initials",
))];

/// Neutral background (#6c757d, grey) shown behind the initials.
const AVATAR_BG_COLOR: ColorU = ColorU { r: 108, g: 117, b: 125, a: 255 };
/// Initials text colour (white).
const AVATAR_TEXT_COLOR: ColorU = ColorU { r: 255, g: 255, b: 255, a: 255 };

const AVATAR_BG_ITEMS: &[StyleBackgroundContent] =
    &[StyleBackgroundContent::Color(AVATAR_BG_COLOR)];
const AVATAR_BG: StyleBackgroundContentVec =
    StyleBackgroundContentVec::from_const_slice(AVATAR_BG_ITEMS);

/// Diameter (and font) size variant of an [`Avatar`].
#[derive(Debug, Copy, Clone, PartialEq, Eq, Default)]
#[repr(C)]
pub enum AvatarSize {
    /// 24px diameter.
    Small,
    /// 40px diameter — the default.
    #[default]
    Medium,
    /// 64px diameter.
    Large,
}

impl AvatarSize {
    /// Diameter of the circle in logical pixels.
    #[allow(clippy::trivially_copy_pass_by_ref)] // <=8B Copy param kept by-ref intentionally (hot pixel/coord path or to avoid churning call sites for a perf-neutral change)
    const fn diameter(&self) -> isize {
        match self {
            Self::Small => 24,
            Self::Medium => 40,
            Self::Large => 64,
        }
    }

    /// Corner radius for a full circle = diameter / 2.
    #[allow(clippy::trivially_copy_pass_by_ref)] // <=8B Copy param kept by-ref intentionally (hot pixel/coord path or to avoid churning call sites for a perf-neutral change)
    const fn radius(&self) -> isize {
        self.diameter() / 2
    }

    /// Initials font size in logical pixels.
    #[allow(clippy::trivially_copy_pass_by_ref)] // <=8B Copy param kept by-ref intentionally (hot pixel/coord path or to avoid churning call sites for a perf-neutral change)
    const fn font_size(&self) -> isize {
        match self {
            Self::Small => 11,
            Self::Medium => 16,
            Self::Large => 24,
        }
    }
}

/// A circular avatar showing an image or initials. Stateless.
#[derive(Debug, Clone, PartialEq, Eq)]
#[repr(C)]
pub struct Avatar {
    /// Optional image; when present it is shown instead of the initials.
    pub image: OptionImageRef,
    /// Fallback initials shown when no image is set.
    pub initials: AzString,
    /// The size variant.
    pub size: AvatarSize,
    /// The computed inline style for the circular container.
    pub avatar_style: CssPropertyWithConditionsVec,
}

/// Builds the circular container style for a given size. Diameter, corner radius
/// and font size are size-dependent, so the style is built at runtime per the
/// recipe's "runtime vec when param-dependent" path (see `badge::build_badge_style`).
fn build_avatar_style(size: AvatarSize) -> CssPropertyWithConditionsVec {
    let d = size.diameter();
    let r = size.radius();
    CssPropertyWithConditionsVec::from_vec(alloc::vec![
        CssPropertyWithConditions::simple(CssProperty::const_display(LayoutDisplay::Flex)),
        CssPropertyWithConditions::simple(CssProperty::const_flex_direction(
            LayoutFlexDirection::Row,
        )),
        CssPropertyWithConditions::simple(CssProperty::const_justify_content(
            LayoutJustifyContent::Center,
        )),
        CssPropertyWithConditions::simple(CssProperty::const_align_items(LayoutAlignItems::Center)),
        // Hug content rather than stretch across a flex parent's cross axis.
        CssPropertyWithConditions::simple(CssProperty::align_self(LayoutAlignSelf::Start)),
        CssPropertyWithConditions::simple(CssProperty::const_flex_grow(LayoutFlexGrow::const_new(
            0,
        ))),
        CssPropertyWithConditions::simple(CssProperty::const_width(LayoutWidth::const_px(d))),
        CssPropertyWithConditions::simple(CssProperty::const_height(LayoutHeight::const_px(d))),
        // circle
        CssPropertyWithConditions::simple(CssProperty::const_border_top_left_radius(
            StyleBorderTopLeftRadius::const_px(r),
        )),
        CssPropertyWithConditions::simple(CssProperty::const_border_top_right_radius(
            StyleBorderTopRightRadius::const_px(r),
        )),
        CssPropertyWithConditions::simple(CssProperty::const_border_bottom_left_radius(
            StyleBorderBottomLeftRadius::const_px(r),
        )),
        CssPropertyWithConditions::simple(CssProperty::const_border_bottom_right_radius(
            StyleBorderBottomRightRadius::const_px(r),
        )),
        // clip the image (or overflowing initials) to the circle
        CssPropertyWithConditions::simple(CssProperty::const_overflow_x(LayoutOverflow::Hidden)),
        CssPropertyWithConditions::simple(CssProperty::const_overflow_y(LayoutOverflow::Hidden)),
        CssPropertyWithConditions::simple(CssProperty::const_font_size(StyleFontSize::const_px(
            size.font_size(),
        ))),
        CssPropertyWithConditions::simple(CssProperty::const_text_align(StyleTextAlign::Center)),
        CssPropertyWithConditions::simple(CssProperty::const_text_color(StyleTextColor {
            inner: AVATAR_TEXT_COLOR,
        })),
        CssPropertyWithConditions::simple(CssProperty::const_background_content(AVATAR_BG)),
    ])
}

/// Builds the inner image style: fills the circle and is itself rounded so the
/// image reads as a circle even if `overflow: hidden` clipping is unavailable.
fn build_image_style(size: AvatarSize) -> CssPropertyWithConditionsVec {
    let d = size.diameter();
    let r = size.radius();
    CssPropertyWithConditionsVec::from_vec(alloc::vec![
        CssPropertyWithConditions::simple(CssProperty::const_flex_grow(LayoutFlexGrow::const_new(
            0,
        ))),
        CssPropertyWithConditions::simple(CssProperty::const_width(LayoutWidth::const_px(d))),
        CssPropertyWithConditions::simple(CssProperty::const_height(LayoutHeight::const_px(d))),
        CssPropertyWithConditions::simple(CssProperty::const_border_top_left_radius(
            StyleBorderTopLeftRadius::const_px(r),
        )),
        CssPropertyWithConditions::simple(CssProperty::const_border_top_right_radius(
            StyleBorderTopRightRadius::const_px(r),
        )),
        CssPropertyWithConditions::simple(CssProperty::const_border_bottom_left_radius(
            StyleBorderBottomLeftRadius::const_px(r),
        )),
        CssPropertyWithConditions::simple(CssProperty::const_border_bottom_right_radius(
            StyleBorderBottomRightRadius::const_px(r),
        )),
    ])
}

impl Avatar {
    /// Creates a medium initials avatar with the given text.
    #[inline]
    #[must_use] pub fn create(initials: AzString) -> Self {
        Self {
            image: None.into(),
            initials,
            size: AvatarSize::Medium,
            avatar_style: build_avatar_style(AvatarSize::Medium),
        }
    }

    /// Creates a medium image avatar (with empty fallback initials).
    #[inline]
    #[must_use] pub fn create_with_image(image: ImageRef) -> Self {
        Self {
            image: Some(image).into(),
            initials: AzString::from_const_str(""),
            size: AvatarSize::Medium,
            avatar_style: build_avatar_style(AvatarSize::Medium),
        }
    }

    /// Sets the avatar image (shown instead of the initials).
    #[inline]
    pub fn set_image(&mut self, image: ImageRef) {
        self.image = Some(image).into();
    }

    /// Builder-style setter for the avatar image.
    #[inline]
    #[must_use] pub fn with_image(mut self, image: ImageRef) -> Self {
        self.set_image(image);
        self
    }

    /// Sets the size variant, recomputing the style.
    #[inline]
    pub fn set_size(&mut self, size: AvatarSize) {
        self.size = size;
        self.avatar_style = build_avatar_style(size);
    }

    /// Builder-style setter for the size variant.
    #[inline]
    #[must_use] pub fn with_size(mut self, size: AvatarSize) -> Self {
        self.set_size(size);
        self
    }

    /// Replaces `self` with a default (empty medium) avatar and returns the original.
    #[inline]
    #[must_use] pub fn swap_with_default(&mut self) -> Self {
        let mut s = Self::create(AzString::from_const_str(""));
        core::mem::swap(&mut s, self);
        s
    }

    /// Converts this avatar into a DOM subtree with the `__azul-native-avatar` class.
    #[inline]
    #[must_use] pub fn dom(self) -> Dom {
        let size = self.size;
        let child = match self.image.into_option() {
            Some(image) => Dom::create_image(image)
                .with_ids_and_classes(IdOrClassVec::from_const_slice(AVATAR_IMAGE_CLASS))
                .with_css_props(build_image_style(size)),
            None => Dom::create_text(self.initials)
                .with_ids_and_classes(IdOrClassVec::from_const_slice(AVATAR_INITIALS_CLASS)),
        };

        Dom::create_div()
            .with_ids_and_classes(IdOrClassVec::from_const_slice(AVATAR_CLASS))
            .with_css_props(self.avatar_style)
            .with_children(alloc::vec![child].into())
    }
}

impl Default for Avatar {
    fn default() -> Self {
        Self::create(AzString::from_const_str(""))
    }
}

impl From<Avatar> for Dom {
    fn from(a: Avatar) -> Self {
        a.dom()
    }
}
