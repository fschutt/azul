use std::sync::atomic::{Ordering, AtomicUsize};
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
    /// Mapping from the TextID to the actual, UTF-8 String
    ///
    /// This is stored outside of the actual glyph calculation, because usually you don't
    /// need the string, except for rebuilding a cached string (for example, when the font is changed)
    pub string_cache: FastHashMap<TextId, String>,
    /// Caches the layout of the strings / words.
    ///
    /// TextId -> FontId (to look up by font)
    /// FontId -> FontSize (to categorize by size within a font)
    /// FontSize -> layouted words (to cache the glyph widths on a per-font-size basis)
    pub layouted_strings_cache: FastHashMap<TextId, FastHashMap<FontId, FastHashMap<FontSize, Words>>>,
}

impl TextCache {

    /// Add a new, large text to the resources
    pub fn add_text<S: Into<String>>(&mut self, text: S) -> TextId {
        let id = new_text_id();
        self.string_cache.insert(id, text.into());
        id
    }

    /// Removes a string from the string cache, but not the layouted text cache
    pub fn delete_string(&mut self, id: TextId) {
        self.string_cache.remove(&id);
    }

    /// Removes a string from the layouted text cache, but not the string cache
    pub fn delete_layouted_text(&mut self, id: TextId) {
        self.layouted_strings_cache.remove(&id);
    }

    /// Delete a text from both the string cache and the layouted text cache
    pub fn delete_text(&mut self, id: TextId) {
        self.delete_string(id);
        self.delete_layouted_text(id);
    }

    pub fn clear_all_texts(&mut self) {
        self.string_cache.clear();
        self.layouted_strings_cache.clear();
    }
}

// Empty test, for some reason codecov doesn't detect any files (and therefore
// doesn't report codecov % correctly) except if they have at least one test in
// the file. This is an empty test, which should be updated later on
#[test]
fn __codecov_test_text_cache_file() {

}