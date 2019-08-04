use std::{
    sync::atomic::{AtomicUsize, Ordering},
};
use webrender::{
    ExternalImageHandler, ExternalImage, ExternalImageSource,
    api::{ExternalImageId, TexelRect, DevicePoint, Epoch, ImageRendering},
};
use {
    FastHashMap,
    gl::{GLuint, Texture},
};
use azul_core::callbacks::PipelineId;

/// Each pipeline (window) has its own OpenGL textures. GL Textures can technically
/// be shared across pipelines, however this turns out to be very difficult in practice.
pub(crate) type GlTextureStorage = FastHashMap<Epoch, FastHashMap<ExternalImageId, Texture>>;

static LAST_EXTERNAL_IMAGE_ID: AtomicUsize = AtomicUsize::new(0);

fn new_external_image_id() -> ExternalImageId {
    ExternalImageId(LAST_EXTERNAL_IMAGE_ID.fetch_add(1, Ordering::SeqCst) as u64)
}

/// Non-cleaned up textures. When a GlTexture is registered, it has to stay active as long
/// as WebRender needs it for drawing. To transparently do this, we store the epoch that the
/// texture was originally created with, and check, **after we have drawn the frame**,
/// if there are any textures that need cleanup.
///
/// Because the Texture2d is wrapped in an Rc, the destructor (which cleans up the OpenGL
/// texture) does not run until we remove the textures
///
/// Note: Because textures could be used after the current draw call (ex. for scrolling),
/// the ACTIVE_GL_TEXTURES are indexed by their epoch. Use `renderer.flush_pipeline_info()`
/// to see which textures are still active and which ones can be safely removed.
///
/// See: https://github.com/servo/webrender/issues/2940
///
/// WARNING: Not thread-safe (however, the Texture itself is thread-unsafe, so it's unlikely to ever be misused)
static mut ACTIVE_GL_TEXTURES: Option<FastHashMap<PipelineId, GlTextureStorage>> = None;

/// Inserts a new texture into the OpenGL texture cache, returns a new image ID
/// for the inserted texture
///
/// This function exists so azul doesn't have to use `lazy_static` as a dependency
pub(crate) fn insert_into_active_gl_textures(pipeline_id: PipelineId, epoch: Epoch, texture: Texture) -> ExternalImageId {

    let external_image_id = new_external_image_id();

    unsafe {
        if ACTIVE_GL_TEXTURES.is_none() {
            ACTIVE_GL_TEXTURES = Some(FastHashMap::new());
        }
        let active_textures = ACTIVE_GL_TEXTURES.as_mut().unwrap();
        let active_epochs = active_textures.entry(pipeline_id).or_insert_with(|| FastHashMap::new());
        let active_textures_for_epoch = active_epochs.entry(epoch).or_insert_with(|| FastHashMap::new());
        active_textures_for_epoch.insert(external_image_id, texture);
    }

    external_image_id
}

pub(crate) fn remove_active_pipeline(pipeline_id: &PipelineId) {
    unsafe {
        let active_textures = match ACTIVE_GL_TEXTURES.as_mut() {
            Some(s) => s,
            None => return,
        };
        active_textures.remove(pipeline_id);
    }
}

/// Destroys all textures from the pipeline `pipeline_id` where the texture is
/// **older** than the given `epoch`.
pub(crate) fn remove_epochs_from_pipeline(pipeline_id: &PipelineId, epoch: Epoch) {
    // TODO: Handle overflow of Epochs correctly (low priority)
    unsafe {
        let active_textures = match ACTIVE_GL_TEXTURES.as_mut() {
            Some(s) => s,
            None => return,
        };
        let active_epochs = match active_textures.get_mut(pipeline_id) {
            Some(s) => s,
            None => return,
        };
        active_epochs.retain(|gl_texture_epoch, _| *gl_texture_epoch > epoch);
    }
}

/// Destroys all textures, usually done before destroying the OpenGL context
pub(crate) fn clear_opengl_cache() {
    unsafe { ACTIVE_GL_TEXTURES = None; }
}

#[derive(Debug, Default, Copy, Clone)]
pub(crate) struct Compositor { }

impl ExternalImageHandler for Compositor {
    fn lock(&mut self, key: ExternalImageId, _channel_index: u8, _rendering: ImageRendering) -> ExternalImage {

        // Search all epoch hash maps for the given key
        // There does not seem to be a way to get the epoch for the key,
        // so we simply have to search all active epochs
        //
        // NOTE: Invalid textures can be generated on minimize / maximize
        // Luckily, webrender simply ignores an invalid texture, so we don't
        // need to check whether a window is maximized or minimized - if
        // we encounter an invalid ID, webrender simply won't draw anything,
        // but at least it won't crash. Usually invalid textures are also 0x0
        // pixels large - so it's not like we had anything to draw anyway.
        fn get_opengl_texture(image_key: &ExternalImageId) -> Option<(GLuint, (f32, f32))> {
            let active_textures = ACTIVE_GL_TEXTURES.as_ref()?;
            active_textures.values()
            .flat_map(|active_pipeline| active_pipeline.values())
            .find_map(|active_epoch| active_epoch.get(image_key))
            .map(|tex| (tex.texture_id, (tex.size.width as f32, tex.size.height as f32)))
        }

        let (tex, wh) = get_opengl_texture(&key)
        .map(|(tex, (w, h))| (ExternalImageSource::NativeTexture(tex), DevicePoint::new(w, h)))
        .unwrap_or((ExternalImageSource::Invalid, DevicePoint::zero()));

        ExternalImage {
            uv: TexelRect {
                uv0: DevicePoint::zero(),
                uv1: wh,
            },
            source: tex,
        }
    }

    fn unlock(&mut self, _key: ExternalImageId, _channel_index: u8) {
        // Since the renderer is currently single-threaded, there is nothing to do here
    }
}
