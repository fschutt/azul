//! Integration layer between gl_texture_cache and the rendering system
//!
//! This module provides the glue code to integrate the low-level gl_texture_cache
//! with the high-level rendering and resource management system.
use azul_core::{
    gl::Texture,
    hit_test::DocumentId,
    resources::{Epoch, ExternalImageId},
};

/// Wrapper around `gl_texture_cache::insert_texture`
pub fn insert_into_active_gl_textures(
    document_id: DocumentId,
    epoch: Epoch,
    texture: Texture,
) -> ExternalImageId {
    crate::desktop::gl_texture_cache::insert_texture(document_id, epoch, texture)
}

/// Wrapper around `gl_texture_cache::remove_old_epochs`
pub fn remove_old_gl_textures(document_id: &DocumentId, current_epoch: Epoch) {
    crate::desktop::gl_texture_cache::remove_old_epochs(document_id, current_epoch);
}
