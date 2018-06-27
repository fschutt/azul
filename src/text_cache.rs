use css_parser::FontSize;
use text_layout::SemanticWordItem;
use FastHashMap;
use std::sync::atomic::{Ordering, AtomicUsize};
use css_parser::Font;
use std::rc::Rc;

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

#[derive(Debug, Clone)]
pub(crate) enum LargeString {
    Raw(String),
    /// The `Vec<SemanticWordItem>` stores the individual word, so we don't need
    /// to store it again. The `words` is stored in an Rc, so that we don't need to
    /// duplicate it for every font size.
    Cached { font: Font, size: FontSize, words: Rc<Vec<SemanticWordItem>> },
}

/// Cache for accessing large amounts of text
#[derive(Debug, Default, Clone)]
pub(crate) struct TextCache {
    /// Gives you the mapping from the TextID to the actual, UTF-8 String
    pub(crate) cached_strings: FastHashMap<TextId, LargeString>,
}

impl TextCache {
    pub(crate) fn add_text(&mut self, text: LargeString) -> TextId {
        let id = new_text_id();
        self.cached_strings.insert(id, text);
        id
    }

    pub(crate) fn delete_text(&mut self, id: TextId) {
        self.cached_strings.remove(&id);
    }

    pub(crate) fn clear_all_texts(&mut self) {
        self.cached_strings.clear();
    }
}