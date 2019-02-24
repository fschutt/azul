use std::sync::atomic::{Ordering, AtomicUsize};
use {
    FastHashMap,
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
    pub(crate) string_cache: FastHashMap<TextId, Words>,

    // -- for now, don't cache ScaledWords, it's too complicated...

    // /// Caches the layout of the strings / words.
    // ///
    // /// TextId -> FontId (to look up by font)
    // /// FontId -> PixelValue (to categorize by size within a font)
    // /// PixelValue -> layouted words (to cache the glyph widths on a per-font-size basis)
    // pub(crate) layouted_strings_cache: FastHashMap<TextId, FastHashMap<FontInstanceKey, ScaledWords>>,
}

impl TextCache {

    /// Add a new, large text to the resources
    pub fn add_text(&mut self, text: &str) -> TextId {
        use text_layout::split_text_into_words;
        let id = new_text_id();
        self.string_cache.insert(id, split_text_into_words(text));
        id
    }

    pub fn get_text(&self, text_id: &TextId) -> Option<&Words> {
        self.string_cache.get(text_id)
    }

    /// Removes a string from the string cache, but not the layouted text cache
    pub fn delete_text(&mut self, id: TextId) {
        self.string_cache.remove(&id);
    }

    pub fn clear_all_texts(&mut self) {
        self.string_cache.clear();
    }
}