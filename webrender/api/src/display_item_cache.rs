/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

use crate::{display_item::*, display_list::*};

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct CachedDisplayItem {
    item: DisplayItem,
    data: Vec<u8>,
}

impl CachedDisplayItem {
    pub fn display_item(&self) -> &DisplayItem {
        &self.item
    }

    pub fn data_as_item_range<T>(&self) -> ItemRange<T> {
        ItemRange::new(&self.data)
    }
}

impl From<DisplayItemRef<'_, '_>> for CachedDisplayItem {
    fn from(item_ref: DisplayItemRef) -> Self {
        let item = item_ref.item();

        match item {
            DisplayItem::Text(..) => CachedDisplayItem {
                item: *item,
                // Store glyphs as bytes for caching (copy each byte)
                data: item_ref
                    .glyphs()
                    .iter()
                    .flat_map(|g| {
                        let bytes: [u8; std::mem::size_of::<crate::font::GlyphInstance>()] =
                            unsafe { std::mem::transmute_copy(g) };
                        bytes
                    })
                    .collect(),
            },
            _ => CachedDisplayItem {
                item: *item,
                data: Vec::new(),
            },
        }
    }
}

#[derive(Clone, Deserialize, Serialize)]
struct CacheEntry {
    items: Vec<CachedDisplayItem>,
    occupied: bool,
}

#[derive(Clone, Deserialize, Serialize)]
pub struct DisplayItemCache {
    entries: Vec<CacheEntry>,
}

impl DisplayItemCache {
    fn add_item(&mut self, key: ItemKey, item: CachedDisplayItem) {
        let entry = &mut self.entries[key as usize];
        entry.items.push(item);
        entry.occupied = true;
    }

    fn clear_entry(&mut self, key: ItemKey) {
        let entry = &mut self.entries[key as usize];
        entry.items.clear();
        entry.occupied = false;
    }

    fn grow_if_needed(&mut self, capacity: usize) {
        if capacity > self.entries.len() {
            self.entries.resize_with(capacity, || CacheEntry {
                items: Vec::new(),
                occupied: false,
            });
        }
    }

    pub fn get_items(&self, key: ItemKey) -> &[CachedDisplayItem] {
        let entry = &self.entries[key as usize];
        debug_assert!(entry.occupied);
        entry.items.as_slice()
    }

    pub fn new() -> Self {
        Self {
            entries: Vec::new(),
        }
    }

    pub fn update(&mut self, display_list: &BuiltDisplayList) {
        eprintln!(
            "[DisplayItemCache::update] START cache_size={}",
            display_list.cache_size()
        );
        self.grow_if_needed(display_list.cache_size());

        let mut iter = display_list.cache_data_iter();
        let mut current_key: Option<ItemKey> = None;
        let mut item_count = 0;
        loop {
            eprintln!("[DisplayItemCache::update] Iterating item #{}", item_count);
            let item = match iter.next() {
                Some(item) => item,
                None => {
                    eprintln!("[DisplayItemCache::update] No more items, breaking");
                    break;
                }
            };
            item_count += 1;
            eprintln!(
                "[DisplayItemCache::update] Got item #{}: {:?}",
                item_count,
                item.item()
            );

            if let DisplayItem::RetainedItems(key) = item.item() {
                current_key = Some(*key);
                self.clear_entry(*key);
                continue;
            }

            // Skip items that don't need caching if no RetainedItems marker was seen
            if current_key.is_none() {
                continue;
            }

            let key = current_key.unwrap();
            let cached_item = CachedDisplayItem::from(item);
            self.add_item(key, cached_item);
        }
        eprintln!(
            "[DisplayItemCache::update] END, processed {} items",
            item_count
        );
    }
}
