//! OpenGL texture cache for external image support.
//!
//! This module manages OpenGL textures that are registered for use with WebRender's
//! external image API. Textures are indexed by a stable key (DomId, NodeId) to ensure
//! the same DOM node always maps to the same ExternalImageId across frames.
//!
//! ## Architecture
//!
//! - Textures are stored by stable node identity: DocumentId -> (DomId, NodeId) -> TextureEntry
//! - ExternalImageId is generated deterministically from (DomId, NodeId) 
//! - This ensures WebRender's cached display lists always find the correct texture
//! - Old textures are cleaned up when their epoch is outdated
//!
//! ## Why Stable IDs Matter
//!
//! WebRender caches display lists across frames. When a display list references an
//! ExternalImageId, that ID must remain valid and point to the current texture.
//! If we generate new IDs each frame, cached display lists will reference stale IDs.
//!
//! By using (DomId, NodeId) as the stable key, the same DOM node always gets the same
//! ExternalImageId, so WebRender's cached display lists always work.
//!
//! ## Safety
//!
//! The static mut TEXTURE_CACHE is safe because:
//! 1. Texture itself is not thread-safe (requires OpenGL context)
//! 2. All texture operations happen on the main/render thread
//! 3. The cache is only accessed during rendering, which is single-threaded

use azul_core::{
    dom::{DomId, NodeId},
    gl::Texture,
    hit_test::DocumentId,
    resources::{Epoch, ExternalImageId},
    FastHashMap,
};

/// A stable key for identifying textures across frames.
/// Using (DomId, NodeId) ensures the same DOM node always maps to the same texture slot.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct TextureSlotKey {
    pub dom_id: DomId,
    pub node_id: NodeId,
}

impl TextureSlotKey {
    pub fn new(dom_id: DomId, node_id: NodeId) -> Self {
        Self { dom_id, node_id }
    }
    
    /// Generate a deterministic ExternalImageId from this key.
    /// This ensures the same DOM node always gets the same ExternalImageId.
    pub fn to_external_image_id(&self) -> ExternalImageId {
        let dom = self.dom_id.inner as u64;
        let node = self.node_id.index() as u64;
        debug_assert!(dom <= u32::MAX as u64, "DomId exceeds 32-bit range");
        debug_assert!(node <= u32::MAX as u64, "NodeId exceeds 32-bit range");
        // High 32 bits: dom_id, Low 32 bits: node_id
        let combined = (dom << 32) | (node & 0xFFFFFFFF);
        ExternalImageId { inner: combined }
    }

    /// Decode a TextureSlotKey from an ExternalImageId.
    /// Reverse of `to_external_image_id()`.
    pub fn from_external_image_id(id: &ExternalImageId) -> Self {
        Self {
            dom_id: DomId { inner: (id.inner >> 32) as usize },
            node_id: NodeId::new((id.inner & 0xFFFFFFFF) as usize),
        }
    }
}

/// Entry for a texture in the cache, tracking the texture and when it was last updated.
struct TextureEntry {
    texture: Texture,
    epoch: Epoch,
}

/// Storage for OpenGL textures, organized by stable node identity.
/// Structure: DocumentId -> TextureSlotKey -> TextureEntry
type GlTextureStorage = FastHashMap<TextureSlotKey, TextureEntry>;

/// Global texture cache. Not thread-safe, but textures are inherently single-threaded.
static mut TEXTURE_CACHE: Option<FastHashMap<DocumentId, GlTextureStorage>> = None;

/// Insert or update a texture in the cache for a specific DOM node.
///
/// Returns a stable ExternalImageId that is deterministic based on (dom_id, node_id),
/// so it remains constant across frames for the same DOM node.
pub fn insert_texture_for_node(
    document_id: DocumentId,
    dom_id: DomId,
    node_id: NodeId,
    epoch: Epoch,
    texture: Texture,
) -> ExternalImageId {
    let key = TextureSlotKey::new(dom_id, node_id);
    let external_image_id = key.to_external_image_id();

    unsafe {
        if TEXTURE_CACHE.is_none() {
            TEXTURE_CACHE = Some(FastHashMap::new());
        }

        let cache = TEXTURE_CACHE.as_mut().unwrap();
        let document_storage = cache.entry(document_id).or_default();

        // Insert or update the texture entry
        document_storage.insert(key, TextureEntry { texture, epoch });
    }

    external_image_id
}

/// Legacy function for compatibility - generates a new ExternalImageId each time.
/// Prefer `insert_texture_for_node` for stable IDs.
pub fn insert_texture(document_id: DocumentId, epoch: Epoch, texture: Texture) -> ExternalImageId {
    let external_image_id = ExternalImageId::new();

    unsafe {
        if TEXTURE_CACHE.is_none() {
            TEXTURE_CACHE = Some(FastHashMap::new());
        }

        let cache = TEXTURE_CACHE.as_mut().unwrap();
        let document_storage = cache.entry(document_id).or_default();
        
        // Use a pseudo-key based on the external_image_id (not stable, but backwards compatible)
        let pseudo_key = TextureSlotKey::from_external_image_id(&external_image_id);
        document_storage.insert(pseudo_key, TextureEntry { texture, epoch });
    }

    external_image_id
}

/// Remove all textures with epochs older than the threshold.
///
/// This is called after rendering to clean up textures from previous frames.
/// We keep textures from the current and previous epoch for double-buffering safety.
pub fn remove_old_epochs(document_id: &DocumentId, current_epoch: Epoch) {
    unsafe {
        let cache: &mut FastHashMap<DocumentId, GlTextureStorage> = match TEXTURE_CACHE.as_mut() {
            Some(c) => c,
            None => return,
        };

        let document_storage: &mut GlTextureStorage = match cache.get_mut(document_id) {
            Some(s) => s,
            None => return,
        };

        // Keep at least the previous epoch for double-buffering safety
        let current = current_epoch.into_u32();
        let min_epoch_to_keep = if current >= 2 {
            Epoch::from(current - 1)
        } else {
            Epoch::new()
        };

        // Collect keys to remove (can't modify while iterating)
        let keys_to_remove: Vec<TextureSlotKey> = document_storage
            .iter()
            .filter(|(_, entry)| entry.epoch < min_epoch_to_keep)
            .map(|(key, _)| *key)
            .collect();

        // Remove old textures
        for key in keys_to_remove {
            document_storage.remove(&key);
        }
    }
}

/// Remove a specific texture from the cache by its slot key.
pub fn remove_texture_for_node(
    document_id: &DocumentId,
    dom_id: DomId,
    node_id: NodeId,
) -> Option<()> {
    let key = TextureSlotKey::new(dom_id, node_id);
    unsafe {
        let cache = TEXTURE_CACHE.as_mut()?;
        let document_storage = cache.get_mut(document_id)?;
        document_storage.remove(&key);
        Some(())
    }
}

/// Remove all textures for a document.
pub fn remove_document(document_id: &DocumentId) {
    unsafe {
        if let Some(cache) = TEXTURE_CACHE.as_mut() {
            let _: Option<GlTextureStorage> = cache.remove(document_id);
        }
    }
}

/// Look up a texture by its ExternalImageId.
///
/// Since ExternalImageId is deterministically generated from (DomId, NodeId),
/// we decode the key from the ID and look it up directly.
pub fn get_texture(external_image_id: &ExternalImageId) -> Option<(u32, (f32, f32))> {
    let key = TextureSlotKey::from_external_image_id(external_image_id);
    
    unsafe {
        let cache = TEXTURE_CACHE.as_ref()?;
        
        // Search all documents for this key
        for (_doc_id, doc_storage) in cache.iter() {
            if let Some(entry) = doc_storage.get(&key) {
                return Some((
                    entry.texture.texture_id,
                    (entry.texture.size.width as f32, entry.texture.size.height as f32),
                ));
            }
        }
        
        None
    }
}

/// Clear the entire texture cache.
pub fn clear_all() {
    unsafe {
        TEXTURE_CACHE = None;
    }
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
        // Same (DomId, NodeId) should always produce the same ExternalImageId
        let key1 = TextureSlotKey::new(DomId { inner: 0 }, NodeId::new(1));
        let key2 = TextureSlotKey::new(DomId { inner: 0 }, NodeId::new(1));
        let key3 = TextureSlotKey::new(DomId { inner: 0 }, NodeId::new(2));
        
        assert_eq!(key1.to_external_image_id(), key2.to_external_image_id());
        assert_ne!(key1.to_external_image_id(), key3.to_external_image_id());
    }

    #[test]
    fn test_external_image_id_reversible() {
        // ExternalImageId should decode back to the original key
        let dom_id = DomId { inner: 42 };
        let node_id = NodeId::new(123);
        let key = TextureSlotKey::new(dom_id, node_id);
        let ext_id = key.to_external_image_id();
        
        // Decode
        let decoded = TextureSlotKey::from_external_image_id(&ext_id);

        assert_eq!(decoded.dom_id, dom_id);
        assert_eq!(decoded.node_id, node_id);
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
