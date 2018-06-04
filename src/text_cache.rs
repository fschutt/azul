use FastHashMap;
use std::sync::atomic::{Ordering, AtomicUsize};

static TEXT_ID_COUNTER: AtomicUsize = AtomicUsize::new(0);

fn new_text_id() -> TextId {
    let unique_id = TEXT_ID_COUNTER.fetch_add(1, Ordering::SeqCst);
    TextId {
        inner: unique_id
    }
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash, Ord, PartialOrd)]
pub struct TextId {
    inner: usize,
}

#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct TextRegistry {
    inner: FastHashMap<TextId, String>
}

impl TextRegistry {

    pub(crate) fn add_text<S: Into<String>>(&mut self, text: S) -> TextId {
        let id = new_text_id();
        self.inner.insert(id, text.into());
        id
    }

    pub(crate) fn delete_text(&mut self, id: TextId) {
        self.inner.remove(&id);
    }

    pub(crate) fn clear_all_texts(&mut self) {
        self.inner.clear();
    }
}