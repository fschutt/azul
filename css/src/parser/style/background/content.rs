use crate::{css_properties::*, parser::*, impl_vec, impl_vec_debug, impl_vec_partialord, impl_vec_ord, impl_vec_clone, impl_vec_partialeq, impl_vec_eq, impl_vec_hash};

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C, u8)]
pub enum StyleBackgroundContent {
    LinearGradient(LinearGradient),
    RadialGradient(RadialGradient),
    ConicGradient(ConicGradient),
    Image(AzString),
    Color(ColorU),
}

impl_vec!(
    StyleBackgroundContent,
    StyleBackgroundContentVec,
    StyleBackgroundContentVecDestructor
);
impl_vec_debug!(StyleBackgroundContent, StyleBackgroundContentVec);
impl_vec_partialord!(StyleBackgroundContent, StyleBackgroundContentVec);
impl_vec_ord!(StyleBackgroundContent, StyleBackgroundContentVec);
impl_vec_clone!(
    StyleBackgroundContent,
    StyleBackgroundContentVec,
    StyleBackgroundContentVecDestructor
);
impl_vec_partialeq!(StyleBackgroundContent, StyleBackgroundContentVec);
impl_vec_eq!(StyleBackgroundContent, StyleBackgroundContentVec);
impl_vec_hash!(StyleBackgroundContent, StyleBackgroundContentVec);

impl Default for StyleBackgroundContent {
    fn default() -> StyleBackgroundContent {
        StyleBackgroundContent::Color(ColorU::TRANSPARENT)
    }
}

impl<'a> From<AzString> for StyleBackgroundContent {
    fn from(id: AzString) -> Self {
        StyleBackgroundContent::Image(id)
    }
}

// parses multiple backgrounds, such as "linear-gradient(red, green), radial-gradient(blue, yellow)"
pub fn parse_style_background_content_multiple<'a>(
    input: &'a str,
) -> Result<StyleBackgroundContentVec, CssBackgroundParseError<'a>> {
    Ok(split_string_respect_comma(input)
        .iter()
        .map(|i| parse_style_background_content(i))
        .collect::<Result<Vec<_>, _>>()?
        .into())
}

// parses a background, such as "linear-gradient(red, green)"
pub fn parse_style_background_content<'a>(
    input: &'a str,
) -> Result<StyleBackgroundContent, CssBackgroundParseError<'a>> {
    match parse_parentheses(
        input,
        &[
            "linear-gradient",
            "repeating-linear-gradient",
            "radial-gradient",
            "repeating-radial-gradient",
            "conic-gradient",
            "repeating-conic-gradient",
            "image",
        ],
    ) {
        Ok((background_type, brace_contents)) => {
            let gradient_type = match background_type {
                "linear-gradient" => GradientType::LinearGradient,
                "repeating-linear-gradient" => GradientType::RepeatingLinearGradient,
                "radial-gradient" => GradientType::RadialGradient,
                "repeating-radial-gradient" => GradientType::RepeatingRadialGradient,
                "conic-gradient" => GradientType::ConicGradient,
                "repeating-conic-gradient" => GradientType::RepeatingConicGradient,
                "image" => {
                    return Ok(StyleBackgroundContent::Image(parse_image(brace_contents)?));
                }
                other => {
                    return Err(CssBackgroundParseError::Error(other)); /* unreachable */
                }
            };

            parse_gradient(brace_contents, gradient_type)
        }
        Err(_) => Ok(StyleBackgroundContent::Color(parse_css_color(input)?)),
    }
}
