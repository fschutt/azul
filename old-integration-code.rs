///! Integration layer between gl_texture_cache and the rendering system
///!
///! This module provides the glue code to integrate the low-level gl_texture_cache
///! with the high-level rendering and resource management system.
use azul_core::{
    gl::Texture,
    hit_test::DocumentId,
    resources::{Epoch, ExternalImageId, GlStoreImageFn},
};

/// Wrapper function that implements GlStoreImageFn using our gl_texture_cache
///
/// This function is passed to various rendering functions that need to store
/// OpenGL textures. It translates the high-level API into calls to our
/// low-level texture cache.
///
/// # Arguments
///
/// * `document_id` - The WebRender document this texture belongs to
/// * `epoch` - The frame epoch when this texture was created
/// * `texture` - The OpenGL texture to register
///
/// # Returns
///
/// An ExternalImageId that can be used to reference this texture in WebRender
pub fn insert_into_active_gl_textures(
    document_id: DocumentId,
    epoch: Epoch,
    texture: Texture,
) -> ExternalImageId {
    crate::desktop::gl_texture_cache::insert_texture(document_id, epoch, texture)
}

/// Returns a function pointer to insert_into_active_gl_textures
///
/// This is used when code expects a `GlStoreImageFn` type.
pub fn get_gl_store_image_fn() -> GlStoreImageFn {
    insert_into_active_gl_textures
}

/// Remove old textures after rendering a frame
///
/// Should be called after each frame is rendered to clean up textures
/// from previous epochs that are no longer needed.
///
/// # Arguments
///
/// * `document_id` - The document to clean up
/// * `current_epoch` - The current frame epoch
pub fn remove_old_gl_textures(document_id: &DocumentId, current_epoch: Epoch) {
    crate::desktop::gl_texture_cache::remove_old_epochs(document_id, current_epoch);
}

/// Remove all textures for a document
///
/// Should be called when a window is closed to clean up all associated textures.
///
/// # Arguments
///
/// * `document_id` - The document to remove
pub fn remove_document_textures(document_id: &DocumentId) {
    crate::desktop::gl_texture_cache::remove_document(document_id);
}

/// Clear all textures
///
/// Should be called before destroying the OpenGL context.
pub fn clear_all_gl_textures() {
    crate::desktop::gl_texture_cache::clear_all();
}

/// Remove a specific texture
///
/// Useful when an image callback is explicitly deleted before its epoch expires.
///
/// # Arguments
///
/// * `document_id` - The document containing the texture
/// * `epoch` - The epoch when the texture was created
/// * `external_image_id` - The ID of the texture to remove
pub fn remove_single_gl_texture(
    document_id: &DocumentId,
    epoch: &Epoch,
    external_image_id: &ExternalImageId,
) -> Option<()> {
    crate::desktop::gl_texture_cache::remove_single_texture(document_id, epoch, external_image_id)
}