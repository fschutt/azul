//! OpenGL texture cache for external image support.
//!
//! This module manages OpenGL textures that are registered for use with WebRender's
//! external image API. Textures are indexed by `ExternalImageId`, which is the single
//! key type used by WebRender to reference external images.
//!
//! ## Architecture
//!
//! - Textures are stored as `DocumentId -> ExternalImageId -> TextureEntry`.
//! - Callers compute the `ExternalImageId` from whatever stable identity they have
//!   (a (DomId, NodeId) pair via `TextureSlotKey::to_external_image_id`, or an
//!   `ImageRefHash` via `ExternalImageId { inner: hash.inner as u64 }`).
//! - Old textures are cleaned up when their epoch is outdated.
//!
//! ## Why Stable IDs Matter
//!
//! WebRender caches display lists across frames. When a display list references an
//! `ExternalImageId`, that ID must remain valid and point to the current texture.
//! If we generate new IDs each frame, cached display lists will reference stale IDs.
//!
//! By computing `ExternalImageId` deterministically at the call site, the same DOM
//! node (or the same `ImageRef`) always gets the same `ExternalImageId`, so
//! WebRender's cached display lists always work.
//!
//! ## Safety
//!
//! The TEXTURE_CACHE uses `thread_local!` to enforce single-thread access at
//! the type level. Textures require an OpenGL context, which is inherently
//! single-threaded.

use std::cell::RefCell;

use azul_core::{
    dom::{DomId, NodeId},
    gl::Texture,
    hit_test::DocumentId,
    resources::{Epoch, ExternalImageId},
    OrderedMap,
};

/// A stable key for identifying textures bound to a specific DOM node.
///
/// This is a small encoder helper: it packs `(DomId, NodeId)` into the single
/// `ExternalImageId` keyspace used by the cache.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub(crate) struct TextureSlotKey {
    pub dom_id: DomId,
    pub node_id: NodeId,
}

impl TextureSlotKey {
    pub fn new(dom_id: DomId, node_id: NodeId) -> Self {
        Self { dom_id, node_id }
    }

    /// Generate a deterministic `ExternalImageId` from this key.
    /// The same DOM node always gets the same `ExternalImageId`.
    pub fn to_external_image_id(&self) -> ExternalImageId {
        let dom = self.dom_id.inner as u64;
        let node = self.node_id.index() as u64;
        debug_assert!(dom <= u32::MAX as u64, "DomId exceeds 32-bit range");
        debug_assert!(node <= u32::MAX as u64, "NodeId exceeds 32-bit range");
        // High 32 bits: dom_id, Low 32 bits: node_id
        let combined = (dom << 32) | (node & 0xFFFFFFFF);
        ExternalImageId { inner: combined }
    }
}

/// Entry for a texture in the cache, tracking the texture and when it was last updated.
struct TextureEntry {
    texture: Texture,
    epoch: Epoch,
}

/// Storage for OpenGL textures, keyed directly by the `ExternalImageId` they expose
/// to WebRender.
type GlTextureStorage = OrderedMap<ExternalImageId, TextureEntry>;

thread_local! {
    static TEXTURE_CACHE: RefCell<Option<OrderedMap<DocumentId, GlTextureStorage>>> = RefCell::new(None);
}

/// Insert or update a texture in the cache for a specific DOM node.
///
/// Returns the deterministic `ExternalImageId` derived from `(dom_id, node_id)`,
/// so the same DOM node always references the same external image.
pub fn insert_texture_for_node(
    document_id: DocumentId,
    dom_id: DomId,
    node_id: NodeId,
    epoch: Epoch,
    texture: Texture,
) -> ExternalImageId {
    let external_image_id = TextureSlotKey::new(dom_id, node_id).to_external_image_id();
    insert_texture_by_id(document_id, external_image_id, epoch, texture);
    external_image_id
}

/// Insert or update a texture in the cache under a caller-supplied `ExternalImageId`.
///
/// Used when the caller already has a stable `ExternalImageId` (e.g. derived from an
/// `ImageRefHash`). All texture insertions ultimately funnel through this function so
/// the cache has a single keyspace.
pub fn insert_texture_by_id(
    document_id: DocumentId,
    external_image_id: ExternalImageId,
    epoch: Epoch,
    texture: Texture,
) {
    TEXTURE_CACHE.with(|cell| {
        let mut cache_opt = cell.borrow_mut();
        let cache = cache_opt.get_or_insert_with(OrderedMap::new);
        let document_storage = cache.entry(document_id).or_default();
        document_storage.insert(external_image_id, TextureEntry { texture, epoch });
    });
}

/// Remove all textures with epochs older than the threshold.
///
/// This is called after rendering to clean up textures from previous frames.
/// We keep textures from the current and previous epoch for double-buffering safety.
pub fn remove_old_epochs(document_id: &DocumentId, current_epoch: Epoch) {
    TEXTURE_CACHE.with(|cell| {
        let mut cache_opt = cell.borrow_mut();
        let cache = match cache_opt.as_mut() {
            Some(c) => c,
            None => return,
        };

        let document_storage = match cache.get_mut(document_id) {
            Some(s) => s,
            None => return,
        };

        let current = current_epoch.into_u32();
        let min_epoch_to_keep = if current >= 2 {
            Epoch::from(current - 1)
        } else {
            Epoch::new()
        };

        let keys_to_remove: Vec<ExternalImageId> = document_storage
            .iter()
            .filter(|(_, entry)| entry.epoch < min_epoch_to_keep)
            .map(|(key, _)| *key)
            .collect();

        for key in keys_to_remove {
            document_storage.remove(&key);
        }
    });
}

/// Remove a specific texture from the cache by its (DomId, NodeId) slot.
pub fn remove_texture_for_node(
    document_id: &DocumentId,
    dom_id: DomId,
    node_id: NodeId,
) -> Option<()> {
    let external_image_id = TextureSlotKey::new(dom_id, node_id).to_external_image_id();
    remove_texture_by_id(document_id, &external_image_id)
}

/// Remove a specific texture from the cache by its `ExternalImageId`.
pub fn remove_texture_by_id(
    document_id: &DocumentId,
    external_image_id: &ExternalImageId,
) -> Option<()> {
    TEXTURE_CACHE.with(|cell| {
        let mut cache_opt = cell.borrow_mut();
        let cache = cache_opt.as_mut()?;
        let document_storage = cache.get_mut(document_id)?;
        document_storage.remove(external_image_id);
        Some(())
    })
}

/// Remove all textures for a document.
pub fn remove_document(document_id: &DocumentId) {
    TEXTURE_CACHE.with(|cell| {
        let mut cache_opt = cell.borrow_mut();
        if let Some(cache) = cache_opt.as_mut() {
            let _: Option<GlTextureStorage> = cache.remove(document_id);
        }
    });
}

/// Look up a texture by its `ExternalImageId`.
pub fn get_texture(external_image_id: &ExternalImageId) -> Option<(u32, (f32, f32))> {
    TEXTURE_CACHE.with(|cell| {
        let cache_opt = cell.borrow();
        let cache = cache_opt.as_ref()?;

        for (_doc_id, doc_storage) in cache.iter() {
            if let Some(entry) = doc_storage.get(external_image_id) {
                return Some((
                    entry.texture.texture_id,
                    (entry.texture.size.width as f32, entry.texture.size.height as f32),
                ));
            }
        }

        None
    })
}

/// Clear the entire texture cache.
pub fn clear_all() {
    TEXTURE_CACHE.with(|cell| {
        *cell.borrow_mut() = None;
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use azul_core::resources::IdNamespace;

    fn create_test_document_id(id: u32) -> DocumentId {
        DocumentId {
            namespace_id: IdNamespace(0),
            id,
        }
    }

    #[test]
    fn test_stable_external_image_id() {
        let key1 = TextureSlotKey::new(DomId { inner: 0 }, NodeId::new(1));
        let key2 = TextureSlotKey::new(DomId { inner: 0 }, NodeId::new(1));
        let key3 = TextureSlotKey::new(DomId { inner: 0 }, NodeId::new(2));

        assert_eq!(key1.to_external_image_id(), key2.to_external_image_id());
        assert_ne!(key1.to_external_image_id(), key3.to_external_image_id());
    }

    #[test]
    fn test_cache_operations_without_gl() {
        clear_all();

        let doc_id = create_test_document_id(1);
        let epoch = Epoch::new();

        remove_old_epochs(&doc_id, epoch);
        remove_document(&doc_id);

        let fake_ext_id = ExternalImageId { inner: 999 };
        assert!(get_texture(&fake_ext_id).is_none());
    }
}
