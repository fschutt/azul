//! Integration layer between gl_texture_cache and the rendering system
//!
//! This module provides the glue code to integrate the low-level gl_texture_cache
//! with the high-level rendering and resource management system.
use azul_core::{
    gl::Texture,
    hit_test::DocumentId,
    resources::{Epoch, ExternalImageId},
};

/// Wrapper around `gl_texture_cache::insert_texture_by_id`. Used as the
/// `GlStoreImageFn` callback wired through `core::resources` so the cache stays
/// keyed by a single `ExternalImageId` namespace.
pub fn insert_into_active_gl_textures(
    document_id: DocumentId,
    epoch: Epoch,
    texture: Texture,
    external_image_id: ExternalImageId,
) {
    crate::desktop::gl_texture_cache::insert_texture_by_id(
        document_id,
        external_image_id,
        epoch,
        texture,
    );
}

/// Wrapper around `gl_texture_cache::remove_old_epochs`
pub fn remove_old_gl_textures(document_id: &DocumentId, current_epoch: Epoch) {
    crate::desktop::gl_texture_cache::remove_old_epochs(document_id, current_epoch);
}

/// Wrapper around `gl_texture_cache::remove_document`. Called from each shell's
/// window-close path so closing a window evicts its GL textures from the cache.
pub fn remove_document_textures(document_id: &DocumentId) {
    crate::desktop::gl_texture_cache::remove_document(document_id);
}
