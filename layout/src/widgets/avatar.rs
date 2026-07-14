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

#[cfg(test)]
mod autotest_generated {
    use std::collections::HashSet;

    use azul_core::{dom::NodeType, resources::RawImageFormat};
    use azul_css::props::basic::{length::SizeMetric, pixel::PixelValue};

    use super::*;

    // ------------------------------------------------------------------
    // Helpers
    // ------------------------------------------------------------------

    /// Every variant of `AvatarSize` — the full input domain of the getters and
    /// of `build_avatar_style` / `build_image_style`.
    const ALL_SIZES: [AvatarSize; 3] = [AvatarSize::Small, AvatarSize::Medium, AvatarSize::Large];

    /// A 2x2 placeholder image: `null_image` needs neither a decoder nor a GPU.
    fn test_image() -> ImageRef {
        ImageRef::null_image(2, 2, RawImageFormat::RGBA8, Vec::new())
    }

    /// The declared properties of a style vec, in declaration order.
    fn properties(v: &CssPropertyWithConditionsVec) -> Vec<CssProperty> {
        v.as_ref().iter().map(|p| p.property.clone()).collect()
    }

    /// The `f32` of a `PixelValue`, asserting it is an absolute `px` length —
    /// an `em`/`%` slipping in here would make the "circle" resolve against the
    /// parent instead of the intended diameter.
    fn px(pv: &PixelValue) -> f32 {
        assert_eq!(
            pv.metric,
            SizeMetric::Px,
            "avatar geometry must be absolute px, got {:?}",
            pv.metric
        );
        pv.number.get()
    }

    fn width_px(v: &CssPropertyWithConditionsVec) -> Option<f32> {
        v.as_ref().iter().find_map(|p| match &p.property {
            CssProperty::Width(w) => match w.get_property()? {
                LayoutWidth::Px(pv) => Some(px(pv)),
                _ => None,
            },
            _ => None,
        })
    }

    fn height_px(v: &CssPropertyWithConditionsVec) -> Option<f32> {
        v.as_ref().iter().find_map(|p| match &p.property {
            CssProperty::Height(h) => match h.get_property()? {
                LayoutHeight::Px(pv) => Some(px(pv)),
                _ => None,
            },
            _ => None,
        })
    }

    fn font_size_px(v: &CssPropertyWithConditionsVec) -> Option<f32> {
        v.as_ref().iter().find_map(|p| match &p.property {
            CssProperty::FontSize(f) => f.get_property().map(|f| px(&f.inner)),
            _ => None,
        })
    }

    /// The four corner radii in declaration order (top-left, top-right,
    /// bottom-left, bottom-right).
    fn radii_px(v: &CssPropertyWithConditionsVec) -> Vec<f32> {
        v.as_ref()
            .iter()
            .filter_map(|p| match &p.property {
                CssProperty::BorderTopLeftRadius(r) => r.get_property().map(|r| px(&r.inner)),
                CssProperty::BorderTopRightRadius(r) => r.get_property().map(|r| px(&r.inner)),
                CssProperty::BorderBottomLeftRadius(r) => r.get_property().map(|r| px(&r.inner)),
                CssProperty::BorderBottomRightRadius(r) => r.get_property().map(|r| px(&r.inner)),
                _ => None,
            })
            .collect()
    }

    /// Every `PixelValue` a style vec mentions (sizes, radii, font size).
    fn all_pixel_values(v: &CssPropertyWithConditionsVec) -> Vec<PixelValue> {
        v.as_ref()
            .iter()
            .filter_map(|p| match &p.property {
                CssProperty::Width(w) => match w.get_property()? {
                    LayoutWidth::Px(pv) => Some(*pv),
                    _ => None,
                },
                CssProperty::Height(h) => match h.get_property()? {
                    LayoutHeight::Px(pv) => Some(*pv),
                    _ => None,
                },
                CssProperty::BorderTopLeftRadius(r) => r.get_property().map(|r| r.inner),
                CssProperty::BorderTopRightRadius(r) => r.get_property().map(|r| r.inner),
                CssProperty::BorderBottomLeftRadius(r) => r.get_property().map(|r| r.inner),
                CssProperty::BorderBottomRightRadius(r) => r.get_property().map(|r| r.inner),
                CssProperty::FontSize(f) => f.get_property().map(|f| f.inner),
                _ => None,
            })
            .collect()
    }

    /// True if `node` carries the CSS class `name`.
    fn has_class(node: &Dom, name: &str) -> bool {
        node.root
            .get_ids_and_classes()
            .as_ref()
            .iter()
            .any(|c| matches!(c, IdOrClass::Class(s) if s.as_str() == name))
    }

    /// The properties of a rendered node's *inline* style, in declaration order.
    fn inline_properties(node: &Dom) -> Vec<CssProperty> {
        node.root
            .style
            .iter_inline_properties()
            .map(|(p, _)| p.clone())
            .collect()
    }

    /// The single child of a rendered avatar DOM (the widget is always
    /// `container -> [image | text]`).
    fn only_child(dom: &Dom) -> &Dom {
        let children = dom.children.as_ref();
        assert_eq!(children.len(), 1, "an avatar renders exactly one child");
        &children[0]
    }

    // ------------------------------------------------------------------
    // AvatarSize::diameter / radius / font_size  (getters)
    // ------------------------------------------------------------------

    #[test]
    fn avatar_size_getters_return_documented_values() {
        assert_eq!(AvatarSize::Small.diameter(), 24);
        assert_eq!(AvatarSize::Medium.diameter(), 40);
        assert_eq!(AvatarSize::Large.diameter(), 64);

        assert_eq!(AvatarSize::Small.radius(), 12);
        assert_eq!(AvatarSize::Medium.radius(), 20);
        assert_eq!(AvatarSize::Large.radius(), 32);

        assert_eq!(AvatarSize::Small.font_size(), 11);
        assert_eq!(AvatarSize::Medium.font_size(), 16);
        assert_eq!(AvatarSize::Large.font_size(), 24);
    }

    #[test]
    fn avatar_size_radius_is_exactly_half_the_diameter() {
        // `radius()` is an integer division: an odd diameter would truncate and
        // the "circle" would render as a rounded square. Every variant must be even.
        for size in ALL_SIZES {
            let d = size.diameter();
            assert_eq!(
                d % 2,
                0,
                "{size:?}: diameter {d} is odd, so radius() truncates and the avatar is not a circle"
            );
            assert_eq!(size.radius() * 2, d, "{size:?}: radius must be exactly d/2");
        }
    }

    #[test]
    fn avatar_size_getters_are_positive_and_font_fits_the_circle() {
        for size in ALL_SIZES {
            assert!(size.diameter() > 0, "{size:?}: non-positive diameter");
            assert!(size.radius() > 0, "{size:?}: non-positive radius");
            assert!(size.font_size() > 0, "{size:?}: non-positive font size");
            assert!(
                size.font_size() < size.diameter(),
                "{size:?}: font size {} does not fit in a {}px circle",
                size.font_size(),
                size.diameter()
            );
        }
    }

    #[test]
    fn avatar_size_getters_are_monotonic_in_the_size_variant() {
        // Small < Medium < Large must hold for both the box and the text, or a
        // "larger" avatar could render smaller than a "smaller" one.
        let d: Vec<isize> = ALL_SIZES.iter().map(AvatarSize::diameter).collect();
        let f: Vec<isize> = ALL_SIZES.iter().map(AvatarSize::font_size).collect();
        assert!(d[0] < d[1] && d[1] < d[2], "diameters not increasing: {d:?}");
        assert!(f[0] < f[1] && f[1] < f[2], "font sizes not increasing: {f:?}");
    }

    #[test]
    fn avatar_size_getters_are_pure_and_default_is_medium() {
        assert_eq!(AvatarSize::default(), AvatarSize::Medium);
        assert_eq!(AvatarSize::default().diameter(), 40);

        // The getters take `&self` on a `Copy` enum: repeated calls (and calls
        // through a copy) must be side-effect free and identical.
        for size in ALL_SIZES {
            let copy = size;
            assert_eq!(size.diameter(), copy.diameter());
            assert_eq!(size.diameter(), size.diameter());
            assert_eq!(size.radius(), size.radius());
            assert_eq!(size.font_size(), size.font_size());
        }
    }

    // ------------------------------------------------------------------
    // build_avatar_style / build_image_style  (numeric)
    // ------------------------------------------------------------------

    #[test]
    fn build_avatar_style_box_matches_the_size_variant() {
        for size in ALL_SIZES {
            let style = build_avatar_style(size);
            #[allow(clippy::cast_precision_loss)]
            let d = size.diameter() as f32;
            #[allow(clippy::cast_precision_loss)]
            let r = size.radius() as f32;
            #[allow(clippy::cast_precision_loss)]
            let f = size.font_size() as f32;

            assert_eq!(width_px(&style), Some(d), "{size:?}: width != diameter");
            assert_eq!(height_px(&style), Some(d), "{size:?}: height != diameter");
            assert_eq!(font_size_px(&style), Some(f), "{size:?}: wrong font size");
            assert_eq!(
                radii_px(&style),
                vec![r, r, r, r],
                "{size:?}: all four corners must carry the same radius"
            );
        }
    }

    #[test]
    fn build_avatar_style_radius_is_half_the_box_so_it_renders_as_a_circle() {
        // The widget's whole premise: r == d/2 in the *emitted* style, not just
        // in the getters.
        for size in ALL_SIZES {
            let style = build_avatar_style(size);
            let d = width_px(&style).expect("width must be declared");
            for r in radii_px(&style) {
                assert!(
                    (r * 2.0 - d).abs() < f32::EPSILON,
                    "{size:?}: radius {r} is not half of the {d}px box"
                );
            }
        }
    }

    #[test]
    fn build_avatar_style_clips_and_centres_its_content() {
        for size in ALL_SIZES {
            let props = properties(&build_avatar_style(size));
            let has = |p: &CssProperty| props.contains(p);

            assert!(has(&CssProperty::const_display(LayoutDisplay::Flex)));
            assert!(has(&CssProperty::const_flex_direction(LayoutFlexDirection::Row)));
            assert!(has(&CssProperty::const_justify_content(LayoutJustifyContent::Center)));
            assert!(has(&CssProperty::const_align_items(LayoutAlignItems::Center)));
            assert!(has(&CssProperty::align_self(LayoutAlignSelf::Start)));
            assert!(has(&CssProperty::const_text_align(StyleTextAlign::Center)));
            // Both axes must clip, or an image/long initials escape the circle.
            assert!(has(&CssProperty::const_overflow_x(LayoutOverflow::Hidden)));
            assert!(has(&CssProperty::const_overflow_y(LayoutOverflow::Hidden)));
            // flex-grow: 0 — the avatar hugs its fixed diameter in a flex parent.
            assert!(has(&CssProperty::const_flex_grow(LayoutFlexGrow::const_new(0))));
        }
    }

    #[test]
    fn build_avatar_style_colors_are_the_documented_constants() {
        let props = properties(&build_avatar_style(AvatarSize::Medium));

        let text = props.iter().find_map(|p| match p {
            CssProperty::TextColor(c) => c.get_property().map(|c| c.inner),
            _ => None,
        });
        assert_eq!(text, Some(ColorU { r: 255, g: 255, b: 255, a: 255 }));

        let bg = props.iter().find_map(|p| match p {
            CssProperty::BackgroundContent(b) => b.get_property(),
            _ => None,
        });
        let bg = bg.expect("a background must be declared behind the initials");
        assert_eq!(bg.as_ref().len(), 1, "exactly one background layer");
        assert_eq!(
            bg.as_ref()[0],
            StyleBackgroundContent::Color(ColorU { r: 108, g: 117, b: 125, a: 255 })
        );
        // Both colours must be fully opaque, or the initials wash out.
        assert_eq!(text.expect("text colour").a, 255);
    }

    #[test]
    fn build_avatar_style_declares_every_property_at_most_once() {
        // A duplicated declaration is a last-one-wins ambiguity: two `width`s
        // would silently make one of them dead.
        for size in ALL_SIZES {
            let props = properties(&build_avatar_style(size));
            let mut seen = HashSet::new();
            for p in &props {
                assert!(
                    seen.insert(core::mem::discriminant(p)),
                    "{size:?}: duplicate declaration of {p:?}"
                );
            }
            assert_eq!(seen.len(), props.len());
        }
    }

    #[test]
    fn build_avatar_style_properties_are_all_unconditional() {
        // Every declaration must apply with no `:hover`/state condition — a
        // conditional one would simply never paint on a stateless widget.
        for size in ALL_SIZES {
            for p in build_avatar_style(size).as_ref() {
                assert!(
                    p.apply_if.as_ref().is_empty(),
                    "{size:?}: {:?} is conditional on a stateless widget",
                    p.property
                );
            }
        }
    }

    #[test]
    fn build_avatar_style_is_deterministic_and_size_dependent() {
        for size in ALL_SIZES {
            assert_eq!(
                properties(&build_avatar_style(size)),
                properties(&build_avatar_style(size)),
                "{size:?}: two builds of the same size disagree"
            );
        }
        // Different variants must not collapse onto the same style.
        assert_ne!(
            properties(&build_avatar_style(AvatarSize::Small)),
            properties(&build_avatar_style(AvatarSize::Large))
        );
        assert_ne!(
            properties(&build_avatar_style(AvatarSize::Small)),
            properties(&build_avatar_style(AvatarSize::Medium))
        );
    }

    #[test]
    fn build_image_style_fills_the_circle_exactly() {
        for size in ALL_SIZES {
            let container = build_avatar_style(size);
            let image = build_image_style(size);

            // The image must be exactly as big and as round as its container,
            // otherwise it either leaves a gap or is clipped square at a corner.
            assert_eq!(width_px(&image), width_px(&container), "{size:?}: image width");
            assert_eq!(height_px(&image), height_px(&container), "{size:?}: image height");
            assert_eq!(radii_px(&image), radii_px(&container), "{size:?}: image radii");
            assert_eq!(radii_px(&image).len(), 4, "{size:?}: all four corners rounded");
            // The image must not grow past the circle in a flex row.
            assert!(properties(&image)
                .contains(&CssProperty::const_flex_grow(LayoutFlexGrow::const_new(0))));
        }
    }

    #[test]
    fn build_image_style_declares_every_property_once_and_unconditionally() {
        for size in ALL_SIZES {
            let style = build_image_style(size);
            let mut seen = HashSet::new();
            for p in style.as_ref() {
                assert!(
                    seen.insert(core::mem::discriminant(&p.property)),
                    "{size:?}: duplicate declaration of {:?}",
                    p.property
                );
                assert!(p.apply_if.as_ref().is_empty(), "{size:?}: conditional image property");
            }
            assert_eq!(
                properties(&build_image_style(size)),
                properties(&build_image_style(size)),
                "{size:?}: build_image_style is not deterministic"
            );
        }
    }

    #[test]
    fn every_emitted_length_is_a_finite_non_negative_px_value() {
        // `isize` -> `PixelValue` is the only numeric conversion in this file:
        // guard against a NaN/inf/negative length ever reaching the solver.
        for size in ALL_SIZES {
            for style in [build_avatar_style(size), build_image_style(size)] {
                let values = all_pixel_values(&style);
                assert!(!values.is_empty(), "{size:?}: no lengths emitted at all");
                for pv in values {
                    let n = px(&pv); // also asserts SizeMetric::Px
                    assert!(n.is_finite(), "{size:?}: non-finite length {n}");
                    assert!(n >= 0.0, "{size:?}: negative length {n}");
                    assert!(n <= 4096.0, "{size:?}: implausibly large length {n}");
                }
            }
        }
    }

    // ------------------------------------------------------------------
    // Avatar::create / create_with_image  (constructors)
    // ------------------------------------------------------------------

    #[test]
    fn create_round_trips_initials_verbatim() {
        // Adversarial strings: empty, combining marks, ZWJ emoji, RTL, embedded
        // NULs (AzString is length-based, so a NUL must NOT truncate), and a
        // string far longer than any real set of initials.
        let long = "x".repeat(100_000);
        let cases = [
            "",
            "AB",
            "e\u{0301}",                  // e + combining acute
            "\u{1F469}\u{200D}\u{1F469}\u{200D}\u{1F467}", // ZWJ family
            "\u{5E9}\u{5DC}",             // RTL Hebrew
            "\0\0",                       // embedded NULs
            "  ",                         // whitespace only
            long.as_str(),
        ];

        for s in cases {
            let a = Avatar::create(AzString::from(s.to_string()));
            assert_eq!(a.initials.as_str(), s, "initials were not preserved verbatim");
            assert_eq!(a.initials.len(), s.len(), "byte length changed (NUL truncation?)");
            assert!(a.image.is_none(), "create() must not set an image");
            assert_eq!(a.size, AvatarSize::Medium);
            assert_eq!(properties(&a.avatar_style), properties(&build_avatar_style(AvatarSize::Medium)));
        }
    }

    #[test]
    fn create_with_image_keeps_the_image_and_empty_initials() {
        let img = test_image();
        let hash = img.get_hash();
        let a = Avatar::create_with_image(img);

        assert!(a.image.is_some());
        match &a.image {
            OptionImageRef::Some(i) => assert_eq!(i.get_hash(), hash, "a different image came back"),
            OptionImageRef::None => panic!("image was dropped by create_with_image"),
        }
        assert_eq!(a.initials.as_str(), "", "image avatars have empty fallback initials");
        assert_eq!(a.size, AvatarSize::Medium);
    }

    #[test]
    fn default_avatar_equals_an_empty_medium_avatar() {
        let d = Avatar::default();
        assert_eq!(d, Avatar::create(AzString::from_const_str("")));
        assert_eq!(d.clone(), d, "Clone must preserve equality");
        assert_ne!(d, Avatar::create(AzString::from_const_str("AB")));
        assert_ne!(
            Avatar::create(AzString::from_const_str("AB")),
            Avatar::create(AzString::from_const_str("AB")).with_size(AvatarSize::Large),
            "avatars of different sizes must not compare equal"
        );
    }

    // ------------------------------------------------------------------
    // set_image / with_image / set_size / with_size  (setters)
    // ------------------------------------------------------------------

    #[test]
    fn with_image_and_set_image_agree_and_keep_the_other_fields() {
        let base = Avatar::create(AzString::from_const_str("AB")).with_size(AvatarSize::Large);

        let mut mutated = base.clone();
        mutated.set_image(test_image());
        let built = base.clone().with_image(test_image());

        assert!(mutated.image.is_some() && built.image.is_some());
        // Setting an image must not disturb the size, the style, or the fallback text.
        for a in [&mutated, &built] {
            assert_eq!(a.size, AvatarSize::Large);
            assert_eq!(a.initials.as_str(), "AB", "set_image must keep the fallback initials");
            assert_eq!(properties(&a.avatar_style), properties(&base.avatar_style));
        }
    }

    #[test]
    fn set_image_replaces_a_previous_image_rather_than_stacking() {
        let first = test_image();
        let second = test_image();
        let (h1, h2) = (first.get_hash(), second.get_hash());
        assert_ne!(h1, h2, "fixture bug: the two images must be distinguishable");

        let mut a = Avatar::create_with_image(first);
        a.set_image(second);
        match &a.image {
            OptionImageRef::Some(i) => assert_eq!(i.get_hash(), h2, "the second image must win"),
            OptionImageRef::None => panic!("image lost"),
        }
    }

    #[test]
    fn set_size_recomputes_the_style_without_growing_it() {
        // A `push`-instead-of-replace bug would make the style vec grow on every
        // call and leave stale (earlier-size) declarations behind.
        let mut a = Avatar::create(AzString::from_const_str("AB"));
        let expected_len = build_avatar_style(AvatarSize::Medium).as_ref().len();

        for round in 0..50 {
            let size = ALL_SIZES[round % ALL_SIZES.len()];
            a.set_size(size);

            assert_eq!(a.size, size, "round {round}: size field not updated");
            assert_eq!(
                a.avatar_style.as_ref().len(),
                expected_len,
                "round {round}: style vec changed length — stale declarations?"
            );
            assert_eq!(
                properties(&a.avatar_style),
                properties(&build_avatar_style(size)),
                "round {round}: style does not match the freshly built one"
            );
            assert_eq!(a.initials.as_str(), "AB", "round {round}: set_size ate the initials");
        }
    }

    #[test]
    fn set_size_keeps_the_image() {
        let mut a = Avatar::create_with_image(test_image());
        a.set_size(AvatarSize::Small);
        assert!(a.image.is_some(), "set_size must not drop the image");
        assert_eq!(width_px(&a.avatar_style), Some(24.0));
    }

    #[test]
    fn with_size_is_last_call_wins_and_matches_set_size() {
        let chained = Avatar::create(AzString::from_const_str("AB"))
            .with_size(AvatarSize::Large)
            .with_size(AvatarSize::Small)
            .with_size(AvatarSize::Medium);

        let mut mutated = Avatar::create(AzString::from_const_str("AB"));
        mutated.set_size(AvatarSize::Large);
        mutated.set_size(AvatarSize::Small);
        mutated.set_size(AvatarSize::Medium);

        assert_eq!(chained, mutated, "builder and mutator must agree");
        assert_eq!(chained.size, AvatarSize::Medium);
        assert_eq!(
            properties(&chained.avatar_style),
            properties(&build_avatar_style(AvatarSize::Medium))
        );
    }

    // ------------------------------------------------------------------
    // swap_with_default
    // ------------------------------------------------------------------

    #[test]
    fn swap_with_default_returns_the_original_and_leaves_a_default_behind() {
        let mut a = Avatar::create(AzString::from_const_str("AB"))
            .with_size(AvatarSize::Large)
            .with_image(test_image());

        let taken = a.swap_with_default();

        // The returned value is the *original*, intact.
        assert_eq!(taken.initials.as_str(), "AB");
        assert_eq!(taken.size, AvatarSize::Large);
        assert!(taken.image.is_some());
        assert_eq!(properties(&taken.avatar_style), properties(&build_avatar_style(AvatarSize::Large)));

        // What is left behind is a *default* avatar — in particular its style
        // must be Medium's, not a stale Large one.
        assert_eq!(a, Avatar::default());
        assert!(a.image.is_none(), "the image must not survive in the emptied avatar");
        assert_eq!(a.initials.as_str(), "");
        assert_eq!(a.size, AvatarSize::Medium);
        assert_eq!(properties(&a.avatar_style), properties(&build_avatar_style(AvatarSize::Medium)));
    }

    #[test]
    fn swap_with_default_is_idempotent_on_an_already_default_avatar() {
        let mut a = Avatar::default();
        let first = a.swap_with_default();
        let second = a.swap_with_default();
        assert_eq!(first, Avatar::default());
        assert_eq!(second, Avatar::default());
        assert_eq!(a, Avatar::default());
    }

    // ------------------------------------------------------------------
    // Avatar::dom
    // ------------------------------------------------------------------

    #[test]
    fn dom_of_an_initials_avatar_is_a_circle_wrapping_the_text() {
        for size in ALL_SIZES {
            let avatar = Avatar::create(AzString::from_const_str("AB")).with_size(size);
            let expected = properties(&avatar.avatar_style);
            let dom = avatar.dom();

            assert!(has_class(&dom, "__azul-native-avatar"), "{size:?}: missing root class");
            assert_eq!(
                inline_properties(&dom),
                expected,
                "{size:?}: the container lost its computed style"
            );

            let child = only_child(&dom);
            assert!(has_class(child, "__azul-native-avatar-initials"));
            match child.root.get_node_type() {
                NodeType::Text(s) => assert_eq!(s.as_ref().as_str(), "AB"),
                other => panic!("{size:?}: expected a text child, got {other:?}"),
            }
        }
    }

    #[test]
    fn dom_of_an_image_avatar_renders_the_image_and_drops_the_initials() {
        for size in ALL_SIZES {
            let img = test_image();
            let hash = img.get_hash();
            // The initials are only a *fallback*: with an image set they must not
            // be rendered as a second child on top of the image.
            let avatar = Avatar::create(AzString::from_const_str("AB"))
                .with_size(size)
                .with_image(img);
            let dom = avatar.dom();

            let child = only_child(&dom);
            assert!(has_class(child, "__azul-native-avatar-image"), "{size:?}: missing image class");
            assert!(
                !has_class(child, "__azul-native-avatar-initials"),
                "{size:?}: initials rendered on top of the image"
            );
            match child.root.get_node_type() {
                NodeType::Image(i) => assert_eq!(i.as_ref().get_hash(), hash, "{size:?}: wrong image"),
                other => panic!("{size:?}: expected an image child, got {other:?}"),
            }
            assert_eq!(
                inline_properties(child),
                properties(&build_image_style(size)),
                "{size:?}: the image child does not carry the matching circular style"
            );
        }
    }

    #[test]
    fn dom_preserves_adversarial_initials_verbatim() {
        let long = "\u{1F600}".repeat(10_000); // 40 000 bytes of emoji
        for s in ["", "\0", "e\u{0301}", "\u{5E9}\u{5DC}", long.as_str()] {
            let dom = Avatar::create(AzString::from(s.to_string())).dom();
            let child = only_child(&dom);
            match child.root.get_node_type() {
                NodeType::Text(t) => {
                    assert_eq!(t.as_ref().as_str(), s, "text node mangled the initials");
                    assert_eq!(t.as_ref().len(), s.len(), "text node changed the byte length");
                }
                other => panic!("expected a text child, got {other:?}"),
            }
        }
    }

    #[test]
    fn dom_and_the_from_impl_agree() {
        let avatar = Avatar::create(AzString::from_const_str("AB")).with_size(AvatarSize::Small);
        assert_eq!(Dom::from(avatar.clone()), avatar.dom());
    }

    #[test]
    fn dom_geometry_is_consistent_between_container_and_image_after_set_size() {
        // Through the supported API (`set_size` / `with_size`) the container and
        // the image must always resolve to the same diameter.
        for size in ALL_SIZES {
            let dom = Avatar::create_with_image(test_image()).with_size(size).dom();
            #[allow(clippy::cast_precision_loss)]
            let d = size.diameter() as f32;

            let container: Vec<CssProperty> = inline_properties(&dom);
            let child: Vec<CssProperty> = inline_properties(only_child(&dom));
            let width_of = |props: &[CssProperty]| {
                props.iter().find_map(|p| match p {
                    CssProperty::Width(w) => match w.get_property()? {
                        LayoutWidth::Px(pv) => Some(px(pv)),
                        _ => None,
                    },
                    _ => None,
                })
            };
            assert_eq!(width_of(&container), Some(d), "{size:?}: container width");
            assert_eq!(width_of(&child), Some(d), "{size:?}: image width");
        }
    }

    #[test]
    fn assigning_the_size_field_directly_desyncs_the_container_from_the_image() {
        // `size` and `avatar_style` are both public, and `dom()` reads the image
        // geometry from `size` while the container keeps the *stored* style. So a
        // direct field write (bypassing `set_size`) silently produces an avatar
        // whose image is a different diameter than its circle. Pinned here as the
        // current behaviour — `set_size` is the only correct path.
        let mut a = Avatar::create_with_image(test_image());
        a.size = AvatarSize::Large; // NOT set_size: the style is not recomputed
        let dom = a.dom();

        assert_eq!(
            width_px(&build_avatar_style(AvatarSize::Medium)),
            Some(40.0),
            "fixture assumption: the stored style is still Medium's"
        );
        let container = inline_properties(&dom);
        let container_width = container.iter().find_map(|p| match p {
            CssProperty::Width(w) => match w.get_property()? {
                LayoutWidth::Px(pv) => Some(px(pv)),
                _ => None,
            },
            _ => None,
        });
        assert_eq!(container_width, Some(40.0), "container still uses the stored Medium style");
        assert_eq!(
            inline_properties(only_child(&dom)),
            properties(&build_image_style(AvatarSize::Large)),
            "the image, however, follows the freshly assigned `size` field"
        );
    }
}
