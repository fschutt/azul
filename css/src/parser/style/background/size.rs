use crate::{css_properties::*, parser::*, impl_vec, impl_vec_debug, impl_vec_partialord, impl_vec_ord, impl_vec_clone, impl_vec_partialeq, impl_vec_eq, impl_vec_hash};

/// Represents a `background-size` attribute
#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C, u8)]
pub enum StyleBackgroundSize {
    ExactSize([PixelValue; 2]),
    Contain,
    Cover,
}

impl StyleBackgroundSize {
    pub fn scale_for_dpi(&mut self, scale_factor: f32) {
        match self {
            StyleBackgroundSize::ExactSize(a) => {
                for q in a.iter_mut() {
                    q.scale_for_dpi(scale_factor);
                }
            }
            _ => {}
        }
    }
}

impl Default for StyleBackgroundSize {
    fn default() -> Self {
        StyleBackgroundSize::Contain
    }
}

impl_vec!(
    StyleBackgroundSize,
    StyleBackgroundSizeVec,
    StyleBackgroundSizeVecDestructor
);
impl_vec_debug!(StyleBackgroundSize, StyleBackgroundSizeVec);
impl_vec_partialord!(StyleBackgroundSize, StyleBackgroundSizeVec);
impl_vec_ord!(StyleBackgroundSize, StyleBackgroundSizeVec);
impl_vec_clone!(
    StyleBackgroundSize,
    StyleBackgroundSizeVec,
    StyleBackgroundSizeVecDestructor
);
impl_vec_partialeq!(StyleBackgroundSize, StyleBackgroundSizeVec);
impl_vec_eq!(StyleBackgroundSize, StyleBackgroundSizeVec);
impl_vec_hash!(StyleBackgroundSize, StyleBackgroundSizeVec);

// parses multiple background-size
pub fn parse_style_background_size_multiple<'a>(
    input: &'a str,
) -> Result<StyleBackgroundSizeVec, InvalidValueErr<'a>> {
    Ok(split_string_respect_comma(input)
        .iter()
        .map(|i| parse_style_background_size(i))
        .collect::<Result<Vec<_>, _>>()?
        .into())
}

pub fn parse_style_background_size<'a>(
    input: &'a str,
) -> Result<StyleBackgroundSize, InvalidValueErr<'a>> {
    let input = input.trim();
    match input {
        "contain" => Ok(StyleBackgroundSize::Contain),
        "cover" => Ok(StyleBackgroundSize::Cover),
        other => {
            let other = other.trim();
            let mut iter = other.split_whitespace();
            let x_pos = iter.next().ok_or(InvalidValueErr(input))?;
            let x_pos = parse_pixel_value(x_pos).map_err(|_| InvalidValueErr(input))?;
            let y_pos = iter.next().ok_or(InvalidValueErr(input))?;
            let y_pos = parse_pixel_value(y_pos).map_err(|_| InvalidValueErr(input))?;
            Ok(StyleBackgroundSize::ExactSize([x_pos, y_pos]))
        }
    }
}
