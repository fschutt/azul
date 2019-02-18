use std::sync::atomic::{Ordering, AtomicUsize};
use azul_css::StyleLetterSpacing;
use webrender::api::{FontKey, FontInstanceKey, RenderApi};
use {
    FastHashMap,
    text_layout::Words,
    app_resources::AppResources,
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
    /// Caches the layout of the strings / words.
    ///
    /// TextId -> FontId (to look up by font)
    /// FontId -> PixelValue (to categorize by size within a font)
    /// PixelValue -> layouted words (to cache the glyph widths on a per-font-size basis)
    pub(crate) layouted_strings_cache: FastHashMap<TextId, FastHashMap<FontKey, FastHashMap<FontInstanceKey, ScaledWords>>>,
}

impl TextCache {

    /// Add a new, large text to the resources
    pub fn add_text(&mut self, text: &str) -> TextId {
        use text_layout::split_text_into_words;
        let id = new_text_id();
        self.string_cache.insert(id, split_text_into_words(text));
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

    /// Gets or inserts words into a cache
    pub(crate) fn get_words_cached<'a>(
        &'a mut self,
        text_id: &TextId,
        render_api: &RenderApi,
        font_key: FontKey,
        font_instance_key: FontInstanceKey,
        letter_spacing: Option<StyleLetterSpacing>,
    ) -> &'a Words {

        use std::collections::hash_map::Entry::*;
        use FastHashMap;
        use text_layout::split_text_into_words;

        let mut should_words_be_scaled = false;

        match self.layouted_strings_cache.entry(*text_id) {
            Occupied(mut font_hash_map) => {

                let font_size_map = font_hash_map.get_mut().entry(font_key.clone()).or_insert_with(|| FastHashMap::default());
                let is_new_font = font_size_map.is_empty();

                match font_size_map.entry(font_instance_key) {
                    Occupied(existing_font_size_words) => { }
                    Vacant(v) => {
                        if is_new_font {
                            v.insert(split_text_into_words(&self.string_cache[text_id], render_api, font_key, font_instance_key, letter_spacing));
                        } else {
                            // If we can get the words from any other size, we can just scale them here
                            // ex. if an existing font size gets scaled.
                           should_words_be_scaled = true;
                        }
                    }
                }
            },
            Vacant(_) => { },
        }

        // We have an entry in the font size -> words cache already, but it's not the right font size
        // instead of recalculating the words, we simply scale them up.
        if should_words_be_scaled {
            let words_cloned = {
                let font_size_map = &self.layouted_strings_cache[&text_id][&font_id];
                let (old_font_size, next_words_for_font) = font_size_map.iter().next().unwrap();
                let mut words_cloned: Words = next_words_for_font.clone();
                let scale_factor = font_size.to_pixels() / old_font_size.to_pixels();

                scale_words(&mut words_cloned, scale_factor);
                words_cloned
            };

            self.layouted_strings_cache.get_mut(&text_id).unwrap().get_mut(&font_key).unwrap().insert(*font_instance_key, words_cloned);
        }

        self.layouted_strings_cache.get(&text_id).unwrap().get(&font_key).unwrap().get(&font_instance_key).unwrap()
    }
}

fn scale_words(words: &mut Words, scale_factor: f32) {
    // Scale the horizontal width of the words to match the new font size
    // Since each word has a local origin (i.e. the first character of each word
    // is at (0, 0)), we can simply scale the X position of each glyph by a
    // certain factor.
    //
    // So if we previously had a 12pt font and now a 13pt font,
    // we simply scale each glyph position by 13 / 12. This is faster than
    // re-calculating the font metrics (from Rusttype) each time we scale a
    // large amount of text.
    for word in words.items.iter_mut() {
        if let SemanticWordItem::Word(ref mut w) = word {
            w.glyphs.iter_mut().for_each(|g| g.point.x *= scale_factor);
            w.total_width *= scale_factor;
        }
    }
}

/// This is used for caching large strings (in the `push_text` function)
/// In the cached version, you can lookup the text as well as the dimensions of
/// the words in the `AppResources`. For the `Uncached` version, you'll have to re-
/// calculate it on every frame.
///
/// TODO: It should be possible to switch this over to a `&'a str`, but currently
/// this leads to unsolvable borrowing issues.
#[derive(Debug)]
pub(crate) enum TextInfo {
    Cached(TextId),
    Uncached(String),
}

impl TextInfo {

    /// Returns if the inner text is empty.
    ///
    /// Returns true if the TextInfo::Cached TextId does not exist
    /// (since in that case, it is "empty", so to speak)
    pub(crate) fn is_empty_text(&self, app_resources: &AppResources)
    -> bool
    {
        use self::TextInfo::*;

        match self {
            Cached(text_id) => {
                match app_resources.text_cache.string_cache.get(text_id) {
                    Some(s) => s.is_empty(),
                    None => true,
                }
            }
            Uncached(s) => s.is_empty(),
        }
    }
}
