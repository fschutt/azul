use std::{
    cmp::Ordering,
    collections::{BTreeSet, HashMap},
    hash::{Hash, Hasher},
    mem::discriminant,
    num::NonZeroUsize,
    sync::{Arc, Mutex},
};

#[cfg(feature = "text_layout_hyphenation")]
use hyphenation::{Hyphenator as _, Language, Load as _, Standard};
use lru::LruCache;
use rust_fontconfig::{
    FcFontCache, FcPattern, FcWeight, FontId, FontMatch, PatternMatch, UnicodeRange,
};
use unicode_bidi::{get_base_direction, BidiInfo};
use unicode_segmentation::UnicodeSegmentation;

use crate::text3::script::Script;

pub mod cache;
pub mod default;
pub mod edit;
pub mod glyphs;
pub mod knuth_plass;
pub mod script;
pub mod selection;

// Test module commented out until it is implemented
// #[cfg(test)]
// pub mod tests;
