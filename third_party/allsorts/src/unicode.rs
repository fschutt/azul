//! Unicode utilities and character properties.

use std::convert::TryFrom;

pub mod codepoint;
mod emoji_data;
pub mod mcc;

/// A Unicode variation selector.
///
/// VS04-VS14 are omitted as they aren't currently used.
#[derive(Debug, Copy, Clone, Eq, PartialEq)]
pub enum VariationSelector {
    /// VARIATION SELECTOR-1
    VS01 = 1,
    /// VARIATION SELECTOR-2
    VS02 = 2,
    /// VARIATION SELECTOR-3
    VS03 = 3,
    /// Text presentation
    VS15 = 15,
    /// Emoji presentation
    VS16 = 16,
}

impl TryFrom<char> for VariationSelector {
    type Error = ();

    fn try_from(ch: char) -> Result<Self, Self::Error> {
        match ch {
            '\u{FE00}' => Ok(VariationSelector::VS01),
            '\u{FE01}' => Ok(VariationSelector::VS02),
            '\u{FE02}' => Ok(VariationSelector::VS03),
            '\u{FE0E}' => Ok(VariationSelector::VS15),
            '\u{FE0F}' => Ok(VariationSelector::VS16),
            _ => Err(()),
        }
    }
}

/// Returns the `Emoji_Presentation` Unicode property for a character.
///
/// ```
/// use allsorts::unicode::bool_prop_emoji_presentation;
///
/// assert_eq!(bool_prop_emoji_presentation('🦀'), true);
/// assert_eq!(bool_prop_emoji_presentation('&'), false);
pub fn bool_prop_emoji_presentation(ch: char) -> bool {
    emoji_data::EMOJI_PRESENTATION.contains_u32(ch as u32)
}
