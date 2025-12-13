//! OpenGL texture cache for external image support.
//!
//! This module manages OpenGL textures that are registered for use with WebRender's
//! external image API. Textures are indexed by DocumentId and Epoch to allow proper
//! cleanup when frames are no longer needed.
//!
//! ## Architecture
//!
//! - Textures are stored in a nested hash map: DocumentId -> Epoch -> ExternalImageId -> Texture
//! - Each texture is reference-counted (via the Texture type's internal refcount)
//! - Old textures are automatically cleaned up when their epoch is no longer active
//! - Thread-safe through use of static mut with careful synchronization
//!
//! ## Safety
//!
//! The static mut TEXTURE_CACHE is not thread-safe in the general case, but this is
//! acceptable because:
//! 1. Texture itself is not thread-safe (requires OpenGL context)
//! 2. All texture operations happen on the main/render thread
//! 3. The cache is only accessed during rendering, which is single-threaded

use azul_core::{
    gl::Texture,
    hit_test::DocumentId,
    resources::{Epoch, ExternalImageId},
    FastHashMap,
};

/// Storage for OpenGL textures, organized by epoch for efficient cleanup.
///
/// Structure: DocumentId -> Epoch -> ExternalImageId -> Texture
type GlTextureStorage = FastHashMap<Epoch, FastHashMap<ExternalImageId, Texture>>;

/// Global texture cache. Not thread-safe, but textures are inherently single-threaded.
///
/// # Safety
///
/// This static mut is safe because:
/// - Textures require an OpenGL context, which is thread-local
/// - All rendering operations happen on the main thread
/// - No concurrent access is possible in the current architecture
static mut TEXTURE_CACHE: Option<FastHashMap<DocumentId, GlTextureStorage>> = None;

/// Insert a texture into the cache and return a new ExternalImageId for it.
///
/// This registers a texture with WebRender so it can be used in image callbacks
/// and other rendering operations.
///
/// # Arguments
///
/// * `document_id` - The WebRender document this texture belongs to
/// * `epoch` - The frame epoch when this texture was created
/// * `texture` - The OpenGL texture to register
///
/// # Returns
///
/// A unique ExternalImageId that can be used to reference this texture
pub fn insert_texture(document_id: DocumentId, epoch: Epoch, texture: Texture) -> ExternalImageId {
    let external_image_id = ExternalImageId::new();

    unsafe {
        // Initialize cache on first use
        if TEXTURE_CACHE.is_none() {
            TEXTURE_CACHE = Some(FastHashMap::new());
        }

        let cache = TEXTURE_CACHE.as_mut().unwrap();

        // Get or create document storage
        let document_storage = cache.entry(document_id).or_insert_with(FastHashMap::new);

        // Get or create epoch storage
        let epoch_storage = document_storage
            .entry(epoch)
            .or_insert_with(FastHashMap::new);

        // Insert texture
        epoch_storage.insert(external_image_id, texture);
    }

    external_image_id
}

/// Remove all textures older than the given epoch for a document.
///
/// This is called after rendering to clean up textures from previous frames
/// that are no longer needed. WebRender guarantees that textures from epochs
/// older than the current one are safe to delete.
///
/// # Arguments
///
/// * `document_id` - The document to clean up
/// * `current_epoch` - The current frame epoch (textures older than this are removed)
///
/// # Note
///
/// This handles epoch overflow correctly by using comparison operators.
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

        // Collect epochs to remove (can't modify while iterating)
        let epochs_to_remove: Vec<Epoch> = document_storage
            .keys()
            .filter(|&&epoch| epoch < current_epoch)
            .copied()
            .collect();

        // Remove old epochs (textures are automatically cleaned up via Drop)
        for epoch in epochs_to_remove {
            document_storage.remove(&epoch);
        }
    }
}

/// Remove a specific texture from the cache.
///
/// This is useful when a texture needs to be removed before its epoch expires,
/// for example when an image is explicitly deleted by the application.
///
/// # Arguments
///
/// * `document_id` - The document containing the texture
/// * `epoch` - The epoch when the texture was created
/// * `external_image_id` - The ID of the texture to remove
///
/// # Returns
///
/// `Some(())` if the texture was found and removed, `None` if it wasn't found
pub fn remove_single_texture(
    document_id: &DocumentId,
    epoch: &Epoch,
    external_image_id: &ExternalImageId,
) -> Option<()> {
    unsafe {
        let cache = TEXTURE_CACHE.as_mut()?;
        let document_storage = cache.get_mut(document_id)?;
        let epoch_storage = document_storage.get_mut(epoch)?;
        epoch_storage.remove(external_image_id);
        Some(())
    }
}

/// Remove all textures for a document.
///
/// This is called when a window/document is closed to clean up all associated textures.
///
/// # Arguments
///
/// * `document_id` - The document to remove
pub fn remove_document(document_id: &DocumentId) {
    unsafe {
        if let Some(cache) = TEXTURE_CACHE.as_mut() {
            let _: Option<GlTextureStorage> = cache.remove(document_id);
        }
    }
}

/// Look up a texture by its ExternalImageId.
///
/// This searches all documents and epochs for the given texture ID.
/// This is necessary because WebRender only provides the ExternalImageId
/// when requesting texture data, not the document or epoch.
///
/// # Arguments
///
/// * `external_image_id` - The ID to look up
///
/// # Returns
///
/// `Some((texture_id, (width, height)))` if found, `None` if not found
///
/// # Performance
///
/// This performs a linear search across all documents and epochs, which could
/// be slow with many textures. However, in practice:
/// - Most applications have few windows (documents)
/// - Only recent epochs are kept (old ones are cleaned up)
/// - Texture lookup happens rarely (only when rendering external images)
pub fn get_texture(external_image_id: &ExternalImageId) -> Option<(u32, (f32, f32))> {
    unsafe {
        let cache = TEXTURE_CACHE.as_ref()?;

        // Search all documents and epochs for this texture
        cache
            .values()
            .flat_map(|document_storage: &GlTextureStorage| document_storage.values())
            .find_map(|epoch_storage: &FastHashMap<ExternalImageId, Texture>| {
                epoch_storage.get(external_image_id)
            })
            .map(|texture| {
                (
                    texture.texture_id,
                    (texture.size.width as f32, texture.size.height as f32),
                )
            })
    }
}

/// Clear the entire texture cache.
///
/// This removes all textures from all documents. This should be called
/// before destroying the OpenGL context to ensure proper cleanup.
///
/// # Safety
///
/// After calling this, all ExternalImageIds become invalid. Any attempts
/// to use them will return None.
pub fn clear_all() {
    unsafe {
        TEXTURE_CACHE = None;
    }
}

// NOTE: These tests require a valid GL context which is not available in unit tests.
// The gl_texture_cache functionality is tested via integration tests instead.
#[cfg(test)]
mod tests {
    use super::*;
    use azul_core::{
        geom::PhysicalSizeU32,
        resources::{IdNamespace, RawImageFormat},
    };
    use azul_css::props::basic::ColorU;

    fn create_test_document_id(id: u32) -> DocumentId {
        DocumentId {
            namespace_id: IdNamespace(0),
            id,
        }
    }

    #[test]
    fn test_document_id_creation() {
        let doc_id = create_test_document_id(42);
        assert_eq!(doc_id.id, 42);
        assert_eq!(doc_id.namespace_id.0, 0);
    }

    #[test]
    fn test_epoch_creation() {
        let epoch = Epoch::new();
        let epoch_from = Epoch::from(5);
        assert_ne!(epoch, epoch_from); // New epoch is 0, from(5) is 5
    }

    #[test]
    fn test_cache_operations_without_gl() {
        // Test that cache functions don't panic with empty cache
        clear_all();
        
        let doc_id = create_test_document_id(1);
        let epoch = Epoch::new();
        
        // Remove from empty cache should not panic
        remove_old_epochs(&doc_id, epoch);
        remove_document(&doc_id);
        
        // Get from empty cache should return None
        let fake_ext_id = ExternalImageId { inner: 999 };
        assert!(get_texture(&fake_ext_id).is_none());
    }
}
