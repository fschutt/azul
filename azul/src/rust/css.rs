    #![allow(dead_code, unused_imports)]
    //! `Css` parsing module
    use crate::dll::*;
    use std::ffi::c_void;
    use crate::str::String;


    /// `CssRuleBlock` struct
    #[doc(inline)] pub use crate::dll::AzCssRuleBlock as CssRuleBlock;

    impl Clone for CssRuleBlock { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_css_rule_block_deep_copy)(self) } }
    impl Drop for CssRuleBlock { fn drop(&mut self) { (crate::dll::get_azul_dll().az_css_rule_block_delete)(self); } }


    /// `CssDeclaration` struct
    #[doc(inline)] pub use crate::dll::AzCssDeclaration as CssDeclaration;

    impl Clone for CssDeclaration { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_css_declaration_deep_copy)(self) } }
    impl Drop for CssDeclaration { fn drop(&mut self) { (crate::dll::get_azul_dll().az_css_declaration_delete)(self); } }


    /// `DynamicCssProperty` struct
    #[doc(inline)] pub use crate::dll::AzDynamicCssProperty as DynamicCssProperty;

    impl Clone for DynamicCssProperty { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_dynamic_css_property_deep_copy)(self) } }
    impl Drop for DynamicCssProperty { fn drop(&mut self) { (crate::dll::get_azul_dll().az_dynamic_css_property_delete)(self); } }


    /// `CssPath` struct
    #[doc(inline)] pub use crate::dll::AzCssPath as CssPath;

    impl Clone for CssPath { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_css_path_deep_copy)(self) } }
    impl Drop for CssPath { fn drop(&mut self) { (crate::dll::get_azul_dll().az_css_path_delete)(self); } }


    /// `CssPathSelector` struct
    #[doc(inline)] pub use crate::dll::AzCssPathSelector as CssPathSelector;

    impl Clone for CssPathSelector { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_css_path_selector_deep_copy)(self) } }
    impl Drop for CssPathSelector { fn drop(&mut self) { (crate::dll::get_azul_dll().az_css_path_selector_delete)(self); } }


    /// `NodeTypePath` struct
    #[doc(inline)] pub use crate::dll::AzNodeTypePath as NodeTypePath;

    impl Clone for NodeTypePath { fn clone(&self) -> Self { *self } }
    impl Copy for NodeTypePath { }


    /// `CssPathPseudoSelector` struct
    #[doc(inline)] pub use crate::dll::AzCssPathPseudoSelector as CssPathPseudoSelector;

    impl Clone for CssPathPseudoSelector { fn clone(&self) -> Self { *self } }
    impl Copy for CssPathPseudoSelector { }


    /// `CssNthChildSelector` struct
    #[doc(inline)] pub use crate::dll::AzCssNthChildSelector as CssNthChildSelector;

    impl Clone for CssNthChildSelector { fn clone(&self) -> Self { *self } }
    impl Copy for CssNthChildSelector { }


    /// `CssNthChildPattern` struct
    #[doc(inline)] pub use crate::dll::AzCssNthChildPattern as CssNthChildPattern;

    impl Clone for CssNthChildPattern { fn clone(&self) -> Self { *self } }
    impl Copy for CssNthChildPattern { }


    /// `Stylesheet` struct
    #[doc(inline)] pub use crate::dll::AzStylesheet as Stylesheet;

    impl Clone for Stylesheet { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_stylesheet_deep_copy)(self) } }
    impl Drop for Stylesheet { fn drop(&mut self) { (crate::dll::get_azul_dll().az_stylesheet_delete)(self); } }


    /// `Css` struct
    #[doc(inline)] pub use crate::dll::AzCss as Css;

    impl Css {
        /// Returns an empty CSS style
        pub fn empty() -> Self { (crate::dll::get_azul_dll().az_css_empty)() }
        /// Returns a CSS style parsed from a `String`
        pub fn from_string(s: String) -> Self { (crate::dll::get_azul_dll().az_css_from_string)(s) }
    }

    impl Clone for Css { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_css_deep_copy)(self) } }
    impl Drop for Css { fn drop(&mut self) { (crate::dll::get_azul_dll().az_css_delete)(self); } }


    /// `ColorU` struct
    #[doc(inline)] pub use crate::dll::AzColorU as ColorU;

    impl Clone for ColorU { fn clone(&self) -> Self { *self } }
    impl Copy for ColorU { }


    /// `SizeMetric` struct
    #[doc(inline)] pub use crate::dll::AzSizeMetric as SizeMetric;

    impl Clone for SizeMetric { fn clone(&self) -> Self { *self } }
    impl Copy for SizeMetric { }


    /// `FloatValue` struct
    #[doc(inline)] pub use crate::dll::AzFloatValue as FloatValue;

    impl Clone for FloatValue { fn clone(&self) -> Self { *self } }
    impl Copy for FloatValue { }


    /// `PixelValue` struct
    #[doc(inline)] pub use crate::dll::AzPixelValue as PixelValue;

    impl Clone for PixelValue { fn clone(&self) -> Self { *self } }
    impl Copy for PixelValue { }


    /// `PixelValueNoPercent` struct
    #[doc(inline)] pub use crate::dll::AzPixelValueNoPercent as PixelValueNoPercent;

    impl Clone for PixelValueNoPercent { fn clone(&self) -> Self { *self } }
    impl Copy for PixelValueNoPercent { }


    /// `BoxShadowClipMode` struct
    #[doc(inline)] pub use crate::dll::AzBoxShadowClipMode as BoxShadowClipMode;

    impl Clone for BoxShadowClipMode { fn clone(&self) -> Self { *self } }
    impl Copy for BoxShadowClipMode { }


    /// `BoxShadowPreDisplayItem` struct
    #[doc(inline)] pub use crate::dll::AzBoxShadowPreDisplayItem as BoxShadowPreDisplayItem;

    impl Clone for BoxShadowPreDisplayItem { fn clone(&self) -> Self { *self } }
    impl Copy for BoxShadowPreDisplayItem { }


    /// `LayoutAlignContent` struct
    #[doc(inline)] pub use crate::dll::AzLayoutAlignContent as LayoutAlignContent;

    impl Clone for LayoutAlignContent { fn clone(&self) -> Self { *self } }
    impl Copy for LayoutAlignContent { }


    /// `LayoutAlignItems` struct
    #[doc(inline)] pub use crate::dll::AzLayoutAlignItems as LayoutAlignItems;

    impl Clone for LayoutAlignItems { fn clone(&self) -> Self { *self } }
    impl Copy for LayoutAlignItems { }


    /// `LayoutBottom` struct
    #[doc(inline)] pub use crate::dll::AzLayoutBottom as LayoutBottom;

    impl Clone for LayoutBottom { fn clone(&self) -> Self { *self } }
    impl Copy for LayoutBottom { }


    /// `LayoutBoxSizing` struct
    #[doc(inline)] pub use crate::dll::AzLayoutBoxSizing as LayoutBoxSizing;

    impl Clone for LayoutBoxSizing { fn clone(&self) -> Self { *self } }
    impl Copy for LayoutBoxSizing { }


    /// `LayoutDirection` struct
    #[doc(inline)] pub use crate::dll::AzLayoutDirection as LayoutDirection;

    impl Clone for LayoutDirection { fn clone(&self) -> Self { *self } }
    impl Copy for LayoutDirection { }


    /// `LayoutDisplay` struct
    #[doc(inline)] pub use crate::dll::AzLayoutDisplay as LayoutDisplay;

    impl Clone for LayoutDisplay { fn clone(&self) -> Self { *self } }
    impl Copy for LayoutDisplay { }


    /// `LayoutFlexGrow` struct
    #[doc(inline)] pub use crate::dll::AzLayoutFlexGrow as LayoutFlexGrow;

    impl Clone for LayoutFlexGrow { fn clone(&self) -> Self { *self } }
    impl Copy for LayoutFlexGrow { }


    /// `LayoutFlexShrink` struct
    #[doc(inline)] pub use crate::dll::AzLayoutFlexShrink as LayoutFlexShrink;

    impl Clone for LayoutFlexShrink { fn clone(&self) -> Self { *self } }
    impl Copy for LayoutFlexShrink { }


    /// `LayoutFloat` struct
    #[doc(inline)] pub use crate::dll::AzLayoutFloat as LayoutFloat;

    impl Clone for LayoutFloat { fn clone(&self) -> Self { *self } }
    impl Copy for LayoutFloat { }


    /// `LayoutHeight` struct
    #[doc(inline)] pub use crate::dll::AzLayoutHeight as LayoutHeight;

    impl Clone for LayoutHeight { fn clone(&self) -> Self { *self } }
    impl Copy for LayoutHeight { }


    /// `LayoutJustifyContent` struct
    #[doc(inline)] pub use crate::dll::AzLayoutJustifyContent as LayoutJustifyContent;

    impl Clone for LayoutJustifyContent { fn clone(&self) -> Self { *self } }
    impl Copy for LayoutJustifyContent { }


    /// `LayoutLeft` struct
    #[doc(inline)] pub use crate::dll::AzLayoutLeft as LayoutLeft;

    impl Clone for LayoutLeft { fn clone(&self) -> Self { *self } }
    impl Copy for LayoutLeft { }


    /// `LayoutMarginBottom` struct
    #[doc(inline)] pub use crate::dll::AzLayoutMarginBottom as LayoutMarginBottom;

    impl Clone for LayoutMarginBottom { fn clone(&self) -> Self { *self } }
    impl Copy for LayoutMarginBottom { }


    /// `LayoutMarginLeft` struct
    #[doc(inline)] pub use crate::dll::AzLayoutMarginLeft as LayoutMarginLeft;

    impl Clone for LayoutMarginLeft { fn clone(&self) -> Self { *self } }
    impl Copy for LayoutMarginLeft { }


    /// `LayoutMarginRight` struct
    #[doc(inline)] pub use crate::dll::AzLayoutMarginRight as LayoutMarginRight;

    impl Clone for LayoutMarginRight { fn clone(&self) -> Self { *self } }
    impl Copy for LayoutMarginRight { }


    /// `LayoutMarginTop` struct
    #[doc(inline)] pub use crate::dll::AzLayoutMarginTop as LayoutMarginTop;

    impl Clone for LayoutMarginTop { fn clone(&self) -> Self { *self } }
    impl Copy for LayoutMarginTop { }


    /// `LayoutMaxHeight` struct
    #[doc(inline)] pub use crate::dll::AzLayoutMaxHeight as LayoutMaxHeight;

    impl Clone for LayoutMaxHeight { fn clone(&self) -> Self { *self } }
    impl Copy for LayoutMaxHeight { }


    /// `LayoutMaxWidth` struct
    #[doc(inline)] pub use crate::dll::AzLayoutMaxWidth as LayoutMaxWidth;

    impl Clone for LayoutMaxWidth { fn clone(&self) -> Self { *self } }
    impl Copy for LayoutMaxWidth { }


    /// `LayoutMinHeight` struct
    #[doc(inline)] pub use crate::dll::AzLayoutMinHeight as LayoutMinHeight;

    impl Clone for LayoutMinHeight { fn clone(&self) -> Self { *self } }
    impl Copy for LayoutMinHeight { }


    /// `LayoutMinWidth` struct
    #[doc(inline)] pub use crate::dll::AzLayoutMinWidth as LayoutMinWidth;

    impl Clone for LayoutMinWidth { fn clone(&self) -> Self { *self } }
    impl Copy for LayoutMinWidth { }


    /// `LayoutPaddingBottom` struct
    #[doc(inline)] pub use crate::dll::AzLayoutPaddingBottom as LayoutPaddingBottom;

    impl Clone for LayoutPaddingBottom { fn clone(&self) -> Self { *self } }
    impl Copy for LayoutPaddingBottom { }


    /// `LayoutPaddingLeft` struct
    #[doc(inline)] pub use crate::dll::AzLayoutPaddingLeft as LayoutPaddingLeft;

    impl Clone for LayoutPaddingLeft { fn clone(&self) -> Self { *self } }
    impl Copy for LayoutPaddingLeft { }


    /// `LayoutPaddingRight` struct
    #[doc(inline)] pub use crate::dll::AzLayoutPaddingRight as LayoutPaddingRight;

    impl Clone for LayoutPaddingRight { fn clone(&self) -> Self { *self } }
    impl Copy for LayoutPaddingRight { }


    /// `LayoutPaddingTop` struct
    #[doc(inline)] pub use crate::dll::AzLayoutPaddingTop as LayoutPaddingTop;

    impl Clone for LayoutPaddingTop { fn clone(&self) -> Self { *self } }
    impl Copy for LayoutPaddingTop { }


    /// `LayoutPosition` struct
    #[doc(inline)] pub use crate::dll::AzLayoutPosition as LayoutPosition;

    impl Clone for LayoutPosition { fn clone(&self) -> Self { *self } }
    impl Copy for LayoutPosition { }


    /// `LayoutRight` struct
    #[doc(inline)] pub use crate::dll::AzLayoutRight as LayoutRight;

    impl Clone for LayoutRight { fn clone(&self) -> Self { *self } }
    impl Copy for LayoutRight { }


    /// `LayoutTop` struct
    #[doc(inline)] pub use crate::dll::AzLayoutTop as LayoutTop;

    impl Clone for LayoutTop { fn clone(&self) -> Self { *self } }
    impl Copy for LayoutTop { }


    /// `LayoutWidth` struct
    #[doc(inline)] pub use crate::dll::AzLayoutWidth as LayoutWidth;

    impl Clone for LayoutWidth { fn clone(&self) -> Self { *self } }
    impl Copy for LayoutWidth { }


    /// `LayoutWrap` struct
    #[doc(inline)] pub use crate::dll::AzLayoutWrap as LayoutWrap;

    impl Clone for LayoutWrap { fn clone(&self) -> Self { *self } }
    impl Copy for LayoutWrap { }


    /// `Overflow` struct
    #[doc(inline)] pub use crate::dll::AzOverflow as Overflow;

    impl Clone for Overflow { fn clone(&self) -> Self { *self } }
    impl Copy for Overflow { }


    /// `PercentageValue` struct
    #[doc(inline)] pub use crate::dll::AzPercentageValue as PercentageValue;

    impl Clone for PercentageValue { fn clone(&self) -> Self { *self } }
    impl Copy for PercentageValue { }


    /// `GradientStopPre` struct
    #[doc(inline)] pub use crate::dll::AzGradientStopPre as GradientStopPre;

    impl Clone for GradientStopPre { fn clone(&self) -> Self { *self } }
    impl Copy for GradientStopPre { }


    /// `DirectionCorner` struct
    #[doc(inline)] pub use crate::dll::AzDirectionCorner as DirectionCorner;

    impl Clone for DirectionCorner { fn clone(&self) -> Self { *self } }
    impl Copy for DirectionCorner { }


    /// `DirectionCorners` struct
    #[doc(inline)] pub use crate::dll::AzDirectionCorners as DirectionCorners;

    impl Clone for DirectionCorners { fn clone(&self) -> Self { *self } }
    impl Copy for DirectionCorners { }


    /// `Direction` struct
    #[doc(inline)] pub use crate::dll::AzDirection as Direction;

    impl Clone for Direction { fn clone(&self) -> Self { *self } }
    impl Copy for Direction { }


    /// `ExtendMode` struct
    #[doc(inline)] pub use crate::dll::AzExtendMode as ExtendMode;

    impl Clone for ExtendMode { fn clone(&self) -> Self { *self } }
    impl Copy for ExtendMode { }


    /// `LinearGradient` struct
    #[doc(inline)] pub use crate::dll::AzLinearGradient as LinearGradient;

    impl Clone for LinearGradient { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_linear_gradient_deep_copy)(self) } }
    impl Drop for LinearGradient { fn drop(&mut self) { (crate::dll::get_azul_dll().az_linear_gradient_delete)(self); } }


    /// `Shape` struct
    #[doc(inline)] pub use crate::dll::AzShape as Shape;

    impl Clone for Shape { fn clone(&self) -> Self { *self } }
    impl Copy for Shape { }


    /// `RadialGradient` struct
    #[doc(inline)] pub use crate::dll::AzRadialGradient as RadialGradient;

    impl Clone for RadialGradient { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_radial_gradient_deep_copy)(self) } }
    impl Drop for RadialGradient { fn drop(&mut self) { (crate::dll::get_azul_dll().az_radial_gradient_delete)(self); } }


    /// `CssImageId` struct
    #[doc(inline)] pub use crate::dll::AzCssImageId as CssImageId;

    impl Clone for CssImageId { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_css_image_id_deep_copy)(self) } }
    impl Drop for CssImageId { fn drop(&mut self) { (crate::dll::get_azul_dll().az_css_image_id_delete)(self); } }


    /// `StyleBackgroundContent` struct
    #[doc(inline)] pub use crate::dll::AzStyleBackgroundContent as StyleBackgroundContent;

    impl Clone for StyleBackgroundContent { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_style_background_content_deep_copy)(self) } }
    impl Drop for StyleBackgroundContent { fn drop(&mut self) { (crate::dll::get_azul_dll().az_style_background_content_delete)(self); } }


    /// `BackgroundPositionHorizontal` struct
    #[doc(inline)] pub use crate::dll::AzBackgroundPositionHorizontal as BackgroundPositionHorizontal;

    impl Clone for BackgroundPositionHorizontal { fn clone(&self) -> Self { *self } }
    impl Copy for BackgroundPositionHorizontal { }


    /// `BackgroundPositionVertical` struct
    #[doc(inline)] pub use crate::dll::AzBackgroundPositionVertical as BackgroundPositionVertical;

    impl Clone for BackgroundPositionVertical { fn clone(&self) -> Self { *self } }
    impl Copy for BackgroundPositionVertical { }


    /// `StyleBackgroundPosition` struct
    #[doc(inline)] pub use crate::dll::AzStyleBackgroundPosition as StyleBackgroundPosition;

    impl Clone for StyleBackgroundPosition { fn clone(&self) -> Self { *self } }
    impl Copy for StyleBackgroundPosition { }


    /// `StyleBackgroundRepeat` struct
    #[doc(inline)] pub use crate::dll::AzStyleBackgroundRepeat as StyleBackgroundRepeat;

    impl Clone for StyleBackgroundRepeat { fn clone(&self) -> Self { *self } }
    impl Copy for StyleBackgroundRepeat { }


    /// `StyleBackgroundSize` struct
    #[doc(inline)] pub use crate::dll::AzStyleBackgroundSize as StyleBackgroundSize;

    impl Clone for StyleBackgroundSize { fn clone(&self) -> Self { *self } }
    impl Copy for StyleBackgroundSize { }


    /// `StyleBorderBottomColor` struct
    #[doc(inline)] pub use crate::dll::AzStyleBorderBottomColor as StyleBorderBottomColor;

    impl Clone for StyleBorderBottomColor { fn clone(&self) -> Self { *self } }
    impl Copy for StyleBorderBottomColor { }


    /// `StyleBorderBottomLeftRadius` struct
    #[doc(inline)] pub use crate::dll::AzStyleBorderBottomLeftRadius as StyleBorderBottomLeftRadius;

    impl Clone for StyleBorderBottomLeftRadius { fn clone(&self) -> Self { *self } }
    impl Copy for StyleBorderBottomLeftRadius { }


    /// `StyleBorderBottomRightRadius` struct
    #[doc(inline)] pub use crate::dll::AzStyleBorderBottomRightRadius as StyleBorderBottomRightRadius;

    impl Clone for StyleBorderBottomRightRadius { fn clone(&self) -> Self { *self } }
    impl Copy for StyleBorderBottomRightRadius { }


    /// `BorderStyle` struct
    #[doc(inline)] pub use crate::dll::AzBorderStyle as BorderStyle;

    impl Clone for BorderStyle { fn clone(&self) -> Self { *self } }
    impl Copy for BorderStyle { }


    /// `StyleBorderBottomStyle` struct
    #[doc(inline)] pub use crate::dll::AzStyleBorderBottomStyle as StyleBorderBottomStyle;

    impl Clone for StyleBorderBottomStyle { fn clone(&self) -> Self { *self } }
    impl Copy for StyleBorderBottomStyle { }


    /// `StyleBorderBottomWidth` struct
    #[doc(inline)] pub use crate::dll::AzStyleBorderBottomWidth as StyleBorderBottomWidth;

    impl Clone for StyleBorderBottomWidth { fn clone(&self) -> Self { *self } }
    impl Copy for StyleBorderBottomWidth { }


    /// `StyleBorderLeftColor` struct
    #[doc(inline)] pub use crate::dll::AzStyleBorderLeftColor as StyleBorderLeftColor;

    impl Clone for StyleBorderLeftColor { fn clone(&self) -> Self { *self } }
    impl Copy for StyleBorderLeftColor { }


    /// `StyleBorderLeftStyle` struct
    #[doc(inline)] pub use crate::dll::AzStyleBorderLeftStyle as StyleBorderLeftStyle;

    impl Clone for StyleBorderLeftStyle { fn clone(&self) -> Self { *self } }
    impl Copy for StyleBorderLeftStyle { }


    /// `StyleBorderLeftWidth` struct
    #[doc(inline)] pub use crate::dll::AzStyleBorderLeftWidth as StyleBorderLeftWidth;

    impl Clone for StyleBorderLeftWidth { fn clone(&self) -> Self { *self } }
    impl Copy for StyleBorderLeftWidth { }


    /// `StyleBorderRightColor` struct
    #[doc(inline)] pub use crate::dll::AzStyleBorderRightColor as StyleBorderRightColor;

    impl Clone for StyleBorderRightColor { fn clone(&self) -> Self { *self } }
    impl Copy for StyleBorderRightColor { }


    /// `StyleBorderRightStyle` struct
    #[doc(inline)] pub use crate::dll::AzStyleBorderRightStyle as StyleBorderRightStyle;

    impl Clone for StyleBorderRightStyle { fn clone(&self) -> Self { *self } }
    impl Copy for StyleBorderRightStyle { }


    /// `StyleBorderRightWidth` struct
    #[doc(inline)] pub use crate::dll::AzStyleBorderRightWidth as StyleBorderRightWidth;

    impl Clone for StyleBorderRightWidth { fn clone(&self) -> Self { *self } }
    impl Copy for StyleBorderRightWidth { }


    /// `StyleBorderTopColor` struct
    #[doc(inline)] pub use crate::dll::AzStyleBorderTopColor as StyleBorderTopColor;

    impl Clone for StyleBorderTopColor { fn clone(&self) -> Self { *self } }
    impl Copy for StyleBorderTopColor { }


    /// `StyleBorderTopLeftRadius` struct
    #[doc(inline)] pub use crate::dll::AzStyleBorderTopLeftRadius as StyleBorderTopLeftRadius;

    impl Clone for StyleBorderTopLeftRadius { fn clone(&self) -> Self { *self } }
    impl Copy for StyleBorderTopLeftRadius { }


    /// `StyleBorderTopRightRadius` struct
    #[doc(inline)] pub use crate::dll::AzStyleBorderTopRightRadius as StyleBorderTopRightRadius;

    impl Clone for StyleBorderTopRightRadius { fn clone(&self) -> Self { *self } }
    impl Copy for StyleBorderTopRightRadius { }


    /// `StyleBorderTopStyle` struct
    #[doc(inline)] pub use crate::dll::AzStyleBorderTopStyle as StyleBorderTopStyle;

    impl Clone for StyleBorderTopStyle { fn clone(&self) -> Self { *self } }
    impl Copy for StyleBorderTopStyle { }


    /// `StyleBorderTopWidth` struct
    #[doc(inline)] pub use crate::dll::AzStyleBorderTopWidth as StyleBorderTopWidth;

    impl Clone for StyleBorderTopWidth { fn clone(&self) -> Self { *self } }
    impl Copy for StyleBorderTopWidth { }


    /// `StyleCursor` struct
    #[doc(inline)] pub use crate::dll::AzStyleCursor as StyleCursor;

    impl Clone for StyleCursor { fn clone(&self) -> Self { *self } }
    impl Copy for StyleCursor { }


    /// `StyleFontFamily` struct
    #[doc(inline)] pub use crate::dll::AzStyleFontFamily as StyleFontFamily;

    impl Clone for StyleFontFamily { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_style_font_family_deep_copy)(self) } }
    impl Drop for StyleFontFamily { fn drop(&mut self) { (crate::dll::get_azul_dll().az_style_font_family_delete)(self); } }


    /// `StyleFontSize` struct
    #[doc(inline)] pub use crate::dll::AzStyleFontSize as StyleFontSize;

    impl Clone for StyleFontSize { fn clone(&self) -> Self { *self } }
    impl Copy for StyleFontSize { }


    /// `StyleLetterSpacing` struct
    #[doc(inline)] pub use crate::dll::AzStyleLetterSpacing as StyleLetterSpacing;

    impl Clone for StyleLetterSpacing { fn clone(&self) -> Self { *self } }
    impl Copy for StyleLetterSpacing { }


    /// `StyleLineHeight` struct
    #[doc(inline)] pub use crate::dll::AzStyleLineHeight as StyleLineHeight;

    impl Clone for StyleLineHeight { fn clone(&self) -> Self { *self } }
    impl Copy for StyleLineHeight { }


    /// `StyleTabWidth` struct
    #[doc(inline)] pub use crate::dll::AzStyleTabWidth as StyleTabWidth;

    impl Clone for StyleTabWidth { fn clone(&self) -> Self { *self } }
    impl Copy for StyleTabWidth { }


    /// `StyleOpacity` struct
    #[doc(inline)] pub use crate::dll::AzStyleOpacity as StyleOpacity;

    impl Clone for StyleOpacity { fn clone(&self) -> Self { *self } }
    impl Copy for StyleOpacity { }


    /// `StyleTransformOrigin` struct
    #[doc(inline)] pub use crate::dll::AzStyleTransformOrigin as StyleTransformOrigin;

    impl Clone for StyleTransformOrigin { fn clone(&self) -> Self { *self } }
    impl Copy for StyleTransformOrigin { }


    /// `StylePerspectiveOrigin` struct
    #[doc(inline)] pub use crate::dll::AzStylePerspectiveOrigin as StylePerspectiveOrigin;

    impl Clone for StylePerspectiveOrigin { fn clone(&self) -> Self { *self } }
    impl Copy for StylePerspectiveOrigin { }


    /// `StyleBackfaceVisibility` struct
    #[doc(inline)] pub use crate::dll::AzStyleBackfaceVisibility as StyleBackfaceVisibility;

    impl Clone for StyleBackfaceVisibility { fn clone(&self) -> Self { *self } }
    impl Copy for StyleBackfaceVisibility { }


    /// `StyleTransform` struct
    #[doc(inline)] pub use crate::dll::AzStyleTransform as StyleTransform;

    impl Clone for StyleTransform { fn clone(&self) -> Self { *self } }
    impl Copy for StyleTransform { }


    /// `StyleTransformMatrix2D` struct
    #[doc(inline)] pub use crate::dll::AzStyleTransformMatrix2D as StyleTransformMatrix2D;

    impl Clone for StyleTransformMatrix2D { fn clone(&self) -> Self { *self } }
    impl Copy for StyleTransformMatrix2D { }


    /// `StyleTransformMatrix3D` struct
    #[doc(inline)] pub use crate::dll::AzStyleTransformMatrix3D as StyleTransformMatrix3D;

    impl Clone for StyleTransformMatrix3D { fn clone(&self) -> Self { *self } }
    impl Copy for StyleTransformMatrix3D { }


    /// `StyleTransformTranslate2D` struct
    #[doc(inline)] pub use crate::dll::AzStyleTransformTranslate2D as StyleTransformTranslate2D;

    impl Clone for StyleTransformTranslate2D { fn clone(&self) -> Self { *self } }
    impl Copy for StyleTransformTranslate2D { }


    /// `StyleTransformTranslate3D` struct
    #[doc(inline)] pub use crate::dll::AzStyleTransformTranslate3D as StyleTransformTranslate3D;

    impl Clone for StyleTransformTranslate3D { fn clone(&self) -> Self { *self } }
    impl Copy for StyleTransformTranslate3D { }


    /// `StyleTransformRotate3D` struct
    #[doc(inline)] pub use crate::dll::AzStyleTransformRotate3D as StyleTransformRotate3D;

    impl Clone for StyleTransformRotate3D { fn clone(&self) -> Self { *self } }
    impl Copy for StyleTransformRotate3D { }


    /// `StyleTransformScale2D` struct
    #[doc(inline)] pub use crate::dll::AzStyleTransformScale2D as StyleTransformScale2D;

    impl Clone for StyleTransformScale2D { fn clone(&self) -> Self { *self } }
    impl Copy for StyleTransformScale2D { }


    /// `StyleTransformScale3D` struct
    #[doc(inline)] pub use crate::dll::AzStyleTransformScale3D as StyleTransformScale3D;

    impl Clone for StyleTransformScale3D { fn clone(&self) -> Self { *self } }
    impl Copy for StyleTransformScale3D { }


    /// `StyleTransformSkew2D` struct
    #[doc(inline)] pub use crate::dll::AzStyleTransformSkew2D as StyleTransformSkew2D;

    impl Clone for StyleTransformSkew2D { fn clone(&self) -> Self { *self } }
    impl Copy for StyleTransformSkew2D { }


    /// `StyleTextAlignmentHorz` struct
    #[doc(inline)] pub use crate::dll::AzStyleTextAlignmentHorz as StyleTextAlignmentHorz;

    impl Clone for StyleTextAlignmentHorz { fn clone(&self) -> Self { *self } }
    impl Copy for StyleTextAlignmentHorz { }


    /// `StyleTextColor` struct
    #[doc(inline)] pub use crate::dll::AzStyleTextColor as StyleTextColor;

    impl Clone for StyleTextColor { fn clone(&self) -> Self { *self } }
    impl Copy for StyleTextColor { }


    /// `StyleWordSpacing` struct
    #[doc(inline)] pub use crate::dll::AzStyleWordSpacing as StyleWordSpacing;

    impl Clone for StyleWordSpacing { fn clone(&self) -> Self { *self } }
    impl Copy for StyleWordSpacing { }


    /// `BoxShadowPreDisplayItemValue` struct
    #[doc(inline)] pub use crate::dll::AzBoxShadowPreDisplayItemValue as BoxShadowPreDisplayItemValue;

    impl Clone for BoxShadowPreDisplayItemValue { fn clone(&self) -> Self { *self } }
    impl Copy for BoxShadowPreDisplayItemValue { }


    /// `LayoutAlignContentValue` struct
    #[doc(inline)] pub use crate::dll::AzLayoutAlignContentValue as LayoutAlignContentValue;

    impl Clone for LayoutAlignContentValue { fn clone(&self) -> Self { *self } }
    impl Copy for LayoutAlignContentValue { }


    /// `LayoutAlignItemsValue` struct
    #[doc(inline)] pub use crate::dll::AzLayoutAlignItemsValue as LayoutAlignItemsValue;

    impl Clone for LayoutAlignItemsValue { fn clone(&self) -> Self { *self } }
    impl Copy for LayoutAlignItemsValue { }


    /// `LayoutBottomValue` struct
    #[doc(inline)] pub use crate::dll::AzLayoutBottomValue as LayoutBottomValue;

    impl Clone for LayoutBottomValue { fn clone(&self) -> Self { *self } }
    impl Copy for LayoutBottomValue { }


    /// `LayoutBoxSizingValue` struct
    #[doc(inline)] pub use crate::dll::AzLayoutBoxSizingValue as LayoutBoxSizingValue;

    impl Clone for LayoutBoxSizingValue { fn clone(&self) -> Self { *self } }
    impl Copy for LayoutBoxSizingValue { }


    /// `LayoutDirectionValue` struct
    #[doc(inline)] pub use crate::dll::AzLayoutDirectionValue as LayoutDirectionValue;

    impl Clone for LayoutDirectionValue { fn clone(&self) -> Self { *self } }
    impl Copy for LayoutDirectionValue { }


    /// `LayoutDisplayValue` struct
    #[doc(inline)] pub use crate::dll::AzLayoutDisplayValue as LayoutDisplayValue;

    impl Clone for LayoutDisplayValue { fn clone(&self) -> Self { *self } }
    impl Copy for LayoutDisplayValue { }


    /// `LayoutFlexGrowValue` struct
    #[doc(inline)] pub use crate::dll::AzLayoutFlexGrowValue as LayoutFlexGrowValue;

    impl Clone for LayoutFlexGrowValue { fn clone(&self) -> Self { *self } }
    impl Copy for LayoutFlexGrowValue { }


    /// `LayoutFlexShrinkValue` struct
    #[doc(inline)] pub use crate::dll::AzLayoutFlexShrinkValue as LayoutFlexShrinkValue;

    impl Clone for LayoutFlexShrinkValue { fn clone(&self) -> Self { *self } }
    impl Copy for LayoutFlexShrinkValue { }


    /// `LayoutFloatValue` struct
    #[doc(inline)] pub use crate::dll::AzLayoutFloatValue as LayoutFloatValue;

    impl Clone for LayoutFloatValue { fn clone(&self) -> Self { *self } }
    impl Copy for LayoutFloatValue { }


    /// `LayoutHeightValue` struct
    #[doc(inline)] pub use crate::dll::AzLayoutHeightValue as LayoutHeightValue;

    impl Clone for LayoutHeightValue { fn clone(&self) -> Self { *self } }
    impl Copy for LayoutHeightValue { }


    /// `LayoutJustifyContentValue` struct
    #[doc(inline)] pub use crate::dll::AzLayoutJustifyContentValue as LayoutJustifyContentValue;

    impl Clone for LayoutJustifyContentValue { fn clone(&self) -> Self { *self } }
    impl Copy for LayoutJustifyContentValue { }


    /// `LayoutLeftValue` struct
    #[doc(inline)] pub use crate::dll::AzLayoutLeftValue as LayoutLeftValue;

    impl Clone for LayoutLeftValue { fn clone(&self) -> Self { *self } }
    impl Copy for LayoutLeftValue { }


    /// `LayoutMarginBottomValue` struct
    #[doc(inline)] pub use crate::dll::AzLayoutMarginBottomValue as LayoutMarginBottomValue;

    impl Clone for LayoutMarginBottomValue { fn clone(&self) -> Self { *self } }
    impl Copy for LayoutMarginBottomValue { }


    /// `LayoutMarginLeftValue` struct
    #[doc(inline)] pub use crate::dll::AzLayoutMarginLeftValue as LayoutMarginLeftValue;

    impl Clone for LayoutMarginLeftValue { fn clone(&self) -> Self { *self } }
    impl Copy for LayoutMarginLeftValue { }


    /// `LayoutMarginRightValue` struct
    #[doc(inline)] pub use crate::dll::AzLayoutMarginRightValue as LayoutMarginRightValue;

    impl Clone for LayoutMarginRightValue { fn clone(&self) -> Self { *self } }
    impl Copy for LayoutMarginRightValue { }


    /// `LayoutMarginTopValue` struct
    #[doc(inline)] pub use crate::dll::AzLayoutMarginTopValue as LayoutMarginTopValue;

    impl Clone for LayoutMarginTopValue { fn clone(&self) -> Self { *self } }
    impl Copy for LayoutMarginTopValue { }


    /// `LayoutMaxHeightValue` struct
    #[doc(inline)] pub use crate::dll::AzLayoutMaxHeightValue as LayoutMaxHeightValue;

    impl Clone for LayoutMaxHeightValue { fn clone(&self) -> Self { *self } }
    impl Copy for LayoutMaxHeightValue { }


    /// `LayoutMaxWidthValue` struct
    #[doc(inline)] pub use crate::dll::AzLayoutMaxWidthValue as LayoutMaxWidthValue;

    impl Clone for LayoutMaxWidthValue { fn clone(&self) -> Self { *self } }
    impl Copy for LayoutMaxWidthValue { }


    /// `LayoutMinHeightValue` struct
    #[doc(inline)] pub use crate::dll::AzLayoutMinHeightValue as LayoutMinHeightValue;

    impl Clone for LayoutMinHeightValue { fn clone(&self) -> Self { *self } }
    impl Copy for LayoutMinHeightValue { }


    /// `LayoutMinWidthValue` struct
    #[doc(inline)] pub use crate::dll::AzLayoutMinWidthValue as LayoutMinWidthValue;

    impl Clone for LayoutMinWidthValue { fn clone(&self) -> Self { *self } }
    impl Copy for LayoutMinWidthValue { }


    /// `LayoutPaddingBottomValue` struct
    #[doc(inline)] pub use crate::dll::AzLayoutPaddingBottomValue as LayoutPaddingBottomValue;

    impl Clone for LayoutPaddingBottomValue { fn clone(&self) -> Self { *self } }
    impl Copy for LayoutPaddingBottomValue { }


    /// `LayoutPaddingLeftValue` struct
    #[doc(inline)] pub use crate::dll::AzLayoutPaddingLeftValue as LayoutPaddingLeftValue;

    impl Clone for LayoutPaddingLeftValue { fn clone(&self) -> Self { *self } }
    impl Copy for LayoutPaddingLeftValue { }


    /// `LayoutPaddingRightValue` struct
    #[doc(inline)] pub use crate::dll::AzLayoutPaddingRightValue as LayoutPaddingRightValue;

    impl Clone for LayoutPaddingRightValue { fn clone(&self) -> Self { *self } }
    impl Copy for LayoutPaddingRightValue { }


    /// `LayoutPaddingTopValue` struct
    #[doc(inline)] pub use crate::dll::AzLayoutPaddingTopValue as LayoutPaddingTopValue;

    impl Clone for LayoutPaddingTopValue { fn clone(&self) -> Self { *self } }
    impl Copy for LayoutPaddingTopValue { }


    /// `LayoutPositionValue` struct
    #[doc(inline)] pub use crate::dll::AzLayoutPositionValue as LayoutPositionValue;

    impl Clone for LayoutPositionValue { fn clone(&self) -> Self { *self } }
    impl Copy for LayoutPositionValue { }


    /// `LayoutRightValue` struct
    #[doc(inline)] pub use crate::dll::AzLayoutRightValue as LayoutRightValue;

    impl Clone for LayoutRightValue { fn clone(&self) -> Self { *self } }
    impl Copy for LayoutRightValue { }


    /// `LayoutTopValue` struct
    #[doc(inline)] pub use crate::dll::AzLayoutTopValue as LayoutTopValue;

    impl Clone for LayoutTopValue { fn clone(&self) -> Self { *self } }
    impl Copy for LayoutTopValue { }


    /// `LayoutWidthValue` struct
    #[doc(inline)] pub use crate::dll::AzLayoutWidthValue as LayoutWidthValue;

    impl Clone for LayoutWidthValue { fn clone(&self) -> Self { *self } }
    impl Copy for LayoutWidthValue { }


    /// `LayoutWrapValue` struct
    #[doc(inline)] pub use crate::dll::AzLayoutWrapValue as LayoutWrapValue;

    impl Clone for LayoutWrapValue { fn clone(&self) -> Self { *self } }
    impl Copy for LayoutWrapValue { }


    /// `OverflowValue` struct
    #[doc(inline)] pub use crate::dll::AzOverflowValue as OverflowValue;

    impl Clone for OverflowValue { fn clone(&self) -> Self { *self } }
    impl Copy for OverflowValue { }


    /// `StyleBackgroundContentValue` struct
    #[doc(inline)] pub use crate::dll::AzStyleBackgroundContentValue as StyleBackgroundContentValue;

    impl Clone for StyleBackgroundContentValue { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_style_background_content_value_deep_copy)(self) } }
    impl Drop for StyleBackgroundContentValue { fn drop(&mut self) { (crate::dll::get_azul_dll().az_style_background_content_value_delete)(self); } }


    /// `StyleBackgroundPositionValue` struct
    #[doc(inline)] pub use crate::dll::AzStyleBackgroundPositionValue as StyleBackgroundPositionValue;

    impl Clone for StyleBackgroundPositionValue { fn clone(&self) -> Self { *self } }
    impl Copy for StyleBackgroundPositionValue { }


    /// `StyleBackgroundRepeatValue` struct
    #[doc(inline)] pub use crate::dll::AzStyleBackgroundRepeatValue as StyleBackgroundRepeatValue;

    impl Clone for StyleBackgroundRepeatValue { fn clone(&self) -> Self { *self } }
    impl Copy for StyleBackgroundRepeatValue { }


    /// `StyleBackgroundSizeValue` struct
    #[doc(inline)] pub use crate::dll::AzStyleBackgroundSizeValue as StyleBackgroundSizeValue;

    impl Clone for StyleBackgroundSizeValue { fn clone(&self) -> Self { *self } }
    impl Copy for StyleBackgroundSizeValue { }


    /// `StyleBorderBottomColorValue` struct
    #[doc(inline)] pub use crate::dll::AzStyleBorderBottomColorValue as StyleBorderBottomColorValue;

    impl Clone for StyleBorderBottomColorValue { fn clone(&self) -> Self { *self } }
    impl Copy for StyleBorderBottomColorValue { }


    /// `StyleBorderBottomLeftRadiusValue` struct
    #[doc(inline)] pub use crate::dll::AzStyleBorderBottomLeftRadiusValue as StyleBorderBottomLeftRadiusValue;

    impl Clone for StyleBorderBottomLeftRadiusValue { fn clone(&self) -> Self { *self } }
    impl Copy for StyleBorderBottomLeftRadiusValue { }


    /// `StyleBorderBottomRightRadiusValue` struct
    #[doc(inline)] pub use crate::dll::AzStyleBorderBottomRightRadiusValue as StyleBorderBottomRightRadiusValue;

    impl Clone for StyleBorderBottomRightRadiusValue { fn clone(&self) -> Self { *self } }
    impl Copy for StyleBorderBottomRightRadiusValue { }


    /// `StyleBorderBottomStyleValue` struct
    #[doc(inline)] pub use crate::dll::AzStyleBorderBottomStyleValue as StyleBorderBottomStyleValue;

    impl Clone for StyleBorderBottomStyleValue { fn clone(&self) -> Self { *self } }
    impl Copy for StyleBorderBottomStyleValue { }


    /// `StyleBorderBottomWidthValue` struct
    #[doc(inline)] pub use crate::dll::AzStyleBorderBottomWidthValue as StyleBorderBottomWidthValue;

    impl Clone for StyleBorderBottomWidthValue { fn clone(&self) -> Self { *self } }
    impl Copy for StyleBorderBottomWidthValue { }


    /// `StyleBorderLeftColorValue` struct
    #[doc(inline)] pub use crate::dll::AzStyleBorderLeftColorValue as StyleBorderLeftColorValue;

    impl Clone for StyleBorderLeftColorValue { fn clone(&self) -> Self { *self } }
    impl Copy for StyleBorderLeftColorValue { }


    /// `StyleBorderLeftStyleValue` struct
    #[doc(inline)] pub use crate::dll::AzStyleBorderLeftStyleValue as StyleBorderLeftStyleValue;

    impl Clone for StyleBorderLeftStyleValue { fn clone(&self) -> Self { *self } }
    impl Copy for StyleBorderLeftStyleValue { }


    /// `StyleBorderLeftWidthValue` struct
    #[doc(inline)] pub use crate::dll::AzStyleBorderLeftWidthValue as StyleBorderLeftWidthValue;

    impl Clone for StyleBorderLeftWidthValue { fn clone(&self) -> Self { *self } }
    impl Copy for StyleBorderLeftWidthValue { }


    /// `StyleBorderRightColorValue` struct
    #[doc(inline)] pub use crate::dll::AzStyleBorderRightColorValue as StyleBorderRightColorValue;

    impl Clone for StyleBorderRightColorValue { fn clone(&self) -> Self { *self } }
    impl Copy for StyleBorderRightColorValue { }


    /// `StyleBorderRightStyleValue` struct
    #[doc(inline)] pub use crate::dll::AzStyleBorderRightStyleValue as StyleBorderRightStyleValue;

    impl Clone for StyleBorderRightStyleValue { fn clone(&self) -> Self { *self } }
    impl Copy for StyleBorderRightStyleValue { }


    /// `StyleBorderRightWidthValue` struct
    #[doc(inline)] pub use crate::dll::AzStyleBorderRightWidthValue as StyleBorderRightWidthValue;

    impl Clone for StyleBorderRightWidthValue { fn clone(&self) -> Self { *self } }
    impl Copy for StyleBorderRightWidthValue { }


    /// `StyleBorderTopColorValue` struct
    #[doc(inline)] pub use crate::dll::AzStyleBorderTopColorValue as StyleBorderTopColorValue;

    impl Clone for StyleBorderTopColorValue { fn clone(&self) -> Self { *self } }
    impl Copy for StyleBorderTopColorValue { }


    /// `StyleBorderTopLeftRadiusValue` struct
    #[doc(inline)] pub use crate::dll::AzStyleBorderTopLeftRadiusValue as StyleBorderTopLeftRadiusValue;

    impl Clone for StyleBorderTopLeftRadiusValue { fn clone(&self) -> Self { *self } }
    impl Copy for StyleBorderTopLeftRadiusValue { }


    /// `StyleBorderTopRightRadiusValue` struct
    #[doc(inline)] pub use crate::dll::AzStyleBorderTopRightRadiusValue as StyleBorderTopRightRadiusValue;

    impl Clone for StyleBorderTopRightRadiusValue { fn clone(&self) -> Self { *self } }
    impl Copy for StyleBorderTopRightRadiusValue { }


    /// `StyleBorderTopStyleValue` struct
    #[doc(inline)] pub use crate::dll::AzStyleBorderTopStyleValue as StyleBorderTopStyleValue;

    impl Clone for StyleBorderTopStyleValue { fn clone(&self) -> Self { *self } }
    impl Copy for StyleBorderTopStyleValue { }


    /// `StyleBorderTopWidthValue` struct
    #[doc(inline)] pub use crate::dll::AzStyleBorderTopWidthValue as StyleBorderTopWidthValue;

    impl Clone for StyleBorderTopWidthValue { fn clone(&self) -> Self { *self } }
    impl Copy for StyleBorderTopWidthValue { }


    /// `StyleCursorValue` struct
    #[doc(inline)] pub use crate::dll::AzStyleCursorValue as StyleCursorValue;

    impl Clone for StyleCursorValue { fn clone(&self) -> Self { *self } }
    impl Copy for StyleCursorValue { }


    /// `StyleFontFamilyValue` struct
    #[doc(inline)] pub use crate::dll::AzStyleFontFamilyValue as StyleFontFamilyValue;

    impl Clone for StyleFontFamilyValue { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_style_font_family_value_deep_copy)(self) } }
    impl Drop for StyleFontFamilyValue { fn drop(&mut self) { (crate::dll::get_azul_dll().az_style_font_family_value_delete)(self); } }


    /// `StyleFontSizeValue` struct
    #[doc(inline)] pub use crate::dll::AzStyleFontSizeValue as StyleFontSizeValue;

    impl Clone for StyleFontSizeValue { fn clone(&self) -> Self { *self } }
    impl Copy for StyleFontSizeValue { }


    /// `StyleLetterSpacingValue` struct
    #[doc(inline)] pub use crate::dll::AzStyleLetterSpacingValue as StyleLetterSpacingValue;

    impl Clone for StyleLetterSpacingValue { fn clone(&self) -> Self { *self } }
    impl Copy for StyleLetterSpacingValue { }


    /// `StyleLineHeightValue` struct
    #[doc(inline)] pub use crate::dll::AzStyleLineHeightValue as StyleLineHeightValue;

    impl Clone for StyleLineHeightValue { fn clone(&self) -> Self { *self } }
    impl Copy for StyleLineHeightValue { }


    /// `StyleTabWidthValue` struct
    #[doc(inline)] pub use crate::dll::AzStyleTabWidthValue as StyleTabWidthValue;

    impl Clone for StyleTabWidthValue { fn clone(&self) -> Self { *self } }
    impl Copy for StyleTabWidthValue { }


    /// `StyleTextAlignmentHorzValue` struct
    #[doc(inline)] pub use crate::dll::AzStyleTextAlignmentHorzValue as StyleTextAlignmentHorzValue;

    impl Clone for StyleTextAlignmentHorzValue { fn clone(&self) -> Self { *self } }
    impl Copy for StyleTextAlignmentHorzValue { }


    /// `StyleTextColorValue` struct
    #[doc(inline)] pub use crate::dll::AzStyleTextColorValue as StyleTextColorValue;

    impl Clone for StyleTextColorValue { fn clone(&self) -> Self { *self } }
    impl Copy for StyleTextColorValue { }


    /// `StyleWordSpacingValue` struct
    #[doc(inline)] pub use crate::dll::AzStyleWordSpacingValue as StyleWordSpacingValue;

    impl Clone for StyleWordSpacingValue { fn clone(&self) -> Self { *self } }
    impl Copy for StyleWordSpacingValue { }


    /// `StyleOpacityValue` struct
    #[doc(inline)] pub use crate::dll::AzStyleOpacityValue as StyleOpacityValue;

    impl Clone for StyleOpacityValue { fn clone(&self) -> Self { *self } }
    impl Copy for StyleOpacityValue { }


    /// `StyleTransformVecValue` struct
    #[doc(inline)] pub use crate::dll::AzStyleTransformVecValue as StyleTransformVecValue;

    impl Clone for StyleTransformVecValue { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_style_transform_vec_value_deep_copy)(self) } }
    impl Drop for StyleTransformVecValue { fn drop(&mut self) { (crate::dll::get_azul_dll().az_style_transform_vec_value_delete)(self); } }


    /// `StyleTransformOriginValue` struct
    #[doc(inline)] pub use crate::dll::AzStyleTransformOriginValue as StyleTransformOriginValue;

    impl Clone for StyleTransformOriginValue { fn clone(&self) -> Self { *self } }
    impl Copy for StyleTransformOriginValue { }


    /// `StylePerspectiveOriginValue` struct
    #[doc(inline)] pub use crate::dll::AzStylePerspectiveOriginValue as StylePerspectiveOriginValue;

    impl Clone for StylePerspectiveOriginValue { fn clone(&self) -> Self { *self } }
    impl Copy for StylePerspectiveOriginValue { }


    /// `StyleBackfaceVisibilityValue` struct
    #[doc(inline)] pub use crate::dll::AzStyleBackfaceVisibilityValue as StyleBackfaceVisibilityValue;

    impl Clone for StyleBackfaceVisibilityValue { fn clone(&self) -> Self { *self } }
    impl Copy for StyleBackfaceVisibilityValue { }


    /// Parsed CSS key-value pair
    #[doc(inline)] pub use crate::dll::AzCssProperty as CssProperty;

    impl Clone for CssProperty { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_css_property_deep_copy)(self) } }
    impl Drop for CssProperty { fn drop(&mut self) { (crate::dll::get_azul_dll().az_css_property_delete)(self); } }
