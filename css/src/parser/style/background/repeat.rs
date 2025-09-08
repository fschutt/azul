use crate::{css_properties::*, parser::*, impl_vec, impl_vec_debug, impl_vec_partialord, impl_vec_ord, impl_vec_clone, impl_vec_partialeq, impl_vec_eq, impl_vec_hash};

/// Represents a `background-repeat` attribute
#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
pub enum StyleBackgroundRepeat {
    NoRepeat,
    Repeat,
    RepeatX,
    RepeatY,
}

impl_vec!(
    StyleBackgroundRepeat,
    StyleBackgroundRepeatVec,
    StyleBackgroundRepeatVecDestructor
);
impl_vec_debug!(StyleBackgroundRepeat, StyleBackgroundRepeatVec);
impl_vec_partialord!(StyleBackgroundRepeat, StyleBackgroundRepeatVec);
impl_vec_ord!(StyleBackgroundRepeat, StyleBackgroundRepeatVec);
impl_vec_clone!(
    StyleBackgroundRepeat,
    StyleBackgroundRepeatVec,
    StyleBackgroundRepeatVecDestructor
);
impl_vec_partialeq!(StyleBackgroundRepeat, StyleBackgroundRepeatVec);
impl_vec_eq!(StyleBackgroundRepeat, StyleBackgroundRepeatVec);
impl_vec_hash!(StyleBackgroundRepeat, StyleBackgroundRepeatVec);

impl Default for StyleBackgroundRepeat {
    fn default() -> Self {
        StyleBackgroundRepeat::Repeat
    }
}

// parses multiple background-repeat
pub fn parse_style_background_repeat_multiple<'a>(
    input: &'a str,
) -> Result<StyleBackgroundRepeatVec, InvalidValueErr<'a>> {
    Ok(split_string_respect_comma(input)
        .iter()
        .map(|i| parse_style_background_repeat(i))
        .collect::<Result<Vec<_>, _>>()?
        .into())
}

multi_type_parser!(
    parse_style_background_repeat,
    StyleBackgroundRepeat,
    ["no-repeat", NoRepeat],
    ["repeat", Repeat],
    ["repeat-x", RepeatX],
    ["repeat-y", RepeatY]
);
