use crate::props::property::CssParsingError;

#[derive(Debug, Clone, PartialEq)]
pub enum CssParsingErrorOwned {
    CssBorderParseError(CssBorderParseErrorOwned),
    CssShadowParseError(CssShadowParseErrorOwned),
    InvalidValueErr(InvalidValueErrOwned),
    PixelParseError(CssPixelValueParseErrorOwned),
    PercentageParseError(PercentageParseError),
    CssImageParseError(CssImageParseErrorOwned),
    CssStyleFontFamilyParseError(CssStyleFontFamilyParseErrorOwned),
    CssBackgroundParseError(CssBackgroundParseErrorOwned),
    CssColorParseError(CssColorParseErrorOwned),
    CssStyleBorderRadiusParseError(CssStyleBorderRadiusParseErrorOwned),
    PaddingParseError(LayoutPaddingParseErrorOwned),
    MarginParseError(LayoutMarginParseErrorOwned),
    FlexShrinkParseError(FlexShrinkParseErrorOwned),
    FlexGrowParseError(FlexGrowParseErrorOwned),
    BackgroundPositionParseError(CssBackgroundPositionParseErrorOwned),
    TransformParseError(CssStyleTransformParseErrorOwned),
    TransformOriginParseError(CssStyleTransformOriginParseErrorOwned),
    PerspectiveOriginParseError(CssStylePerspectiveOriginParseErrorOwned),
    Opacity(OpacityParseErrorOwned),
    Scrollbar(CssScrollbarStyleParseErrorOwned),
    Filter(CssStyleFilterParseErrorOwned),
}

// Implement `to_contained` and `to_shared` for CssParsingError
impl<'a> CssParsingError<'a> {
    pub fn to_contained(&self) -> CssParsingErrorOwned {
        match self {
            CssParsingError::CssBorderParseError(e) => {
                CssParsingErrorOwned::CssBorderParseError(e.to_contained())
            }
            CssParsingError::CssShadowParseError(e) => {
                CssParsingErrorOwned::CssShadowParseError(e.to_contained())
            }
            CssParsingError::InvalidValueErr(e) => {
                CssParsingErrorOwned::InvalidValueErr(e.to_contained())
            }
            CssParsingError::PixelParseError(e) => {
                CssParsingErrorOwned::PixelParseError(e.to_contained())
            }
            CssParsingError::PercentageParseError(e) => {
                CssParsingErrorOwned::PercentageParseError(e.clone())
            }
            CssParsingError::CssImageParseError(e) => {
                CssParsingErrorOwned::CssImageParseError(e.to_contained())
            }
            CssParsingError::CssStyleFontFamilyParseError(e) => {
                CssParsingErrorOwned::CssStyleFontFamilyParseError(e.to_contained())
            }
            CssParsingError::CssBackgroundParseError(e) => {
                CssParsingErrorOwned::CssBackgroundParseError(e.to_contained())
            }
            CssParsingError::CssColorParseError(e) => {
                CssParsingErrorOwned::CssColorParseError(e.to_contained())
            }
            CssParsingError::CssStyleBorderRadiusParseError(e) => {
                CssParsingErrorOwned::CssStyleBorderRadiusParseError(e.to_contained())
            }
            CssParsingError::PaddingParseError(e) => {
                CssParsingErrorOwned::PaddingParseError(e.to_contained())
            }
            CssParsingError::MarginParseError(e) => {
                CssParsingErrorOwned::MarginParseError(e.to_contained())
            }
            CssParsingError::FlexShrinkParseError(e) => {
                CssParsingErrorOwned::FlexShrinkParseError(e.to_contained())
            }
            CssParsingError::FlexGrowParseError(e) => {
                CssParsingErrorOwned::FlexGrowParseError(e.to_contained())
            }
            CssParsingError::BackgroundPositionParseError(e) => {
                CssParsingErrorOwned::BackgroundPositionParseError(e.to_contained())
            }
            CssParsingError::TransformParseError(e) => {
                CssParsingErrorOwned::TransformParseError(e.to_contained())
            }
            CssParsingError::TransformOriginParseError(e) => {
                CssParsingErrorOwned::TransformOriginParseError(e.to_contained())
            }
            CssParsingError::PerspectiveOriginParseError(e) => {
                CssParsingErrorOwned::PerspectiveOriginParseError(e.to_contained())
            }
            CssParsingError::Opacity(e) => CssParsingErrorOwned::Opacity(e.to_contained()),
            CssParsingError::Scrollbar(e) => CssParsingErrorOwned::Scrollbar(e.to_contained()),
            CssParsingError::Filter(e) => CssParsingErrorOwned::Filter(e.to_contained()),
        }
    }
}

impl CssParsingErrorOwned {
    pub fn to_shared<'a>(&'a self) -> CssParsingError<'a> {
        match self {
            CssParsingErrorOwned::CssBorderParseError(e) => {
                CssParsingError::CssBorderParseError(e.to_shared())
            }
            CssParsingErrorOwned::CssShadowParseError(e) => {
                CssParsingError::CssShadowParseError(e.to_shared())
            }
            CssParsingErrorOwned::InvalidValueErr(e) => {
                CssParsingError::InvalidValueErr(e.to_shared())
            }
            CssParsingErrorOwned::PixelParseError(e) => {
                CssParsingError::PixelParseError(e.to_shared())
            }
            CssParsingErrorOwned::PercentageParseError(e) => {
                CssParsingError::PercentageParseError(e.clone())
            }
            CssParsingErrorOwned::CssImageParseError(e) => {
                CssParsingError::CssImageParseError(e.to_shared())
            }
            CssParsingErrorOwned::CssStyleFontFamilyParseError(e) => {
                CssParsingError::CssStyleFontFamilyParseError(e.to_shared())
            }
            CssParsingErrorOwned::CssBackgroundParseError(e) => {
                CssParsingError::CssBackgroundParseError(e.to_shared())
            }
            CssParsingErrorOwned::CssColorParseError(e) => {
                CssParsingError::CssColorParseError(e.to_shared())
            }
            CssParsingErrorOwned::CssStyleBorderRadiusParseError(e) => {
                CssParsingError::CssStyleBorderRadiusParseError(e.to_shared())
            }
            CssParsingErrorOwned::PaddingParseError(e) => {
                CssParsingError::PaddingParseError(e.to_shared())
            }
            CssParsingErrorOwned::MarginParseError(e) => {
                CssParsingError::MarginParseError(e.to_shared())
            }
            CssParsingErrorOwned::FlexShrinkParseError(e) => {
                CssParsingError::FlexShrinkParseError(e.to_shared())
            }
            CssParsingErrorOwned::FlexGrowParseError(e) => {
                CssParsingError::FlexGrowParseError(e.to_shared())
            }
            CssParsingErrorOwned::BackgroundPositionParseError(e) => {
                CssParsingError::BackgroundPositionParseError(e.to_shared())
            }
            CssParsingErrorOwned::TransformParseError(e) => {
                CssParsingError::TransformParseError(e.to_shared())
            }
            CssParsingErrorOwned::TransformOriginParseError(e) => {
                CssParsingError::TransformOriginParseError(e.to_shared())
            }
            CssParsingErrorOwned::PerspectiveOriginParseError(e) => {
                CssParsingError::PerspectiveOriginParseError(e.to_shared())
            }
            CssParsingErrorOwned::Opacity(e) => CssParsingError::Opacity(e.to_shared()),
            CssParsingErrorOwned::Scrollbar(e) => CssParsingError::Scrollbar(e.to_shared()),
            CssParsingErrorOwned::Filter(e) => CssParsingError::Filter(e.to_shared()),
        }
    }
}

/// Simple "invalid value" error, used for
#[derive(Debug, Copy, Clone, Eq, PartialEq)]
pub struct InvalidValueErr<'a>(pub &'a str);

/// Owned version of InvalidValueErr with String.
#[derive(Debug, Clone, PartialEq)]
pub struct InvalidValueErrOwned(pub String);

impl<'a> InvalidValueErr<'a> {
    pub fn to_contained(&self) -> InvalidValueErrOwned {
        InvalidValueErrOwned(self.0.to_string())
    }
}

impl InvalidValueErrOwned {
    pub fn to_shared<'a>(&'a self) -> InvalidValueErr<'a> {
        InvalidValueErr(&self.0)
    }
}
