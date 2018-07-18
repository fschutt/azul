use std::{
    rc::Rc,
    sync::atomic::{Ordering, AtomicUsize},
};
use {
    FastHashMap,
    css_parser::{FontId, FontSize},
    text_layout::Words,
};

static TEXT_ID_COUNTER: AtomicUsize = AtomicUsize::new(0);

fn new_text_id() -> TextId {
    let unique_id = TEXT_ID_COUNTER.fetch_add(1, Ordering::SeqCst);
    TextId {
        inner: unique_id
    }
}

/// A unique ID by which a large block of text can be uniquely identified
#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash, Ord, PartialOrd)]
pub struct TextId {
    inner: usize,
}

/// Cache for accessing large amounts of text
#[derive(Debug, Default, Clone)]
pub struct TextCache {
    /// Caches the layout of the strings / words.
    ///
    /// TextId -> FontId (to look up by font)
    /// FontId -> FontSize (to categorize by size within a font)
    /// FontSize -> layouted words (to cache the glyph widths on a per-font-size basis)
    pub cached_strings: FastHashMap<TextId, FastHashMap<FontId, FastHashMap<FontSize, Words>>>,
    /// Mapping from the TextID to the actual, UTF-8 String
    ///
    /// This is stored outside of the actual glyph calculation, because usually you don't
    /// need the string, except for rebuilding a cached string (for example, when the font is changed)
    pub string_cache: FastHashMap<TextId, String>,
}

impl TextCache {
    /// Add a new, large text to the resources
    pub fn add_text<S: Into<String>>(&mut self, text: S) -> TextId {
        let id = new_text_id();
        self.string_cache.insert(id, text.into());
        id
    }

    pub fn delete_text(&mut self, id: TextId) {
        self.string_cache.remove(&id);
        self.cached_strings.remove(&id);
    }

    pub fn clear_all_texts(&mut self) {
        self.string_cache.clear();
        self.cached_strings.clear();
    }
}

// Empty test, for some reason codecov doesn't detect any files (and therefore
// doesn't report codecov % correctly) except if they have at least one test in
// the file. This is an empty test, which should be updated later on
#[test]
fn __codecov_test_text_cache_file() {

}