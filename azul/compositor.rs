use std::sync::{Mutex, atomic::{Ordering, AtomicUsize}};
use webrender::{
    ExternalImageHandler, ExternalImage, ExternalImageSource,
    api::{ExternalImageId, TexelRect, DevicePixel, Epoch, ImageRendering},
};
use euclid::TypedPoint2D;
use {
    FastHashMap,
    gl::Texture,
};

static LAST_OPENGL_ID: AtomicUsize = AtomicUsize::new(0);

pub fn new_opengl_texture_id() -> usize {
    LAST_OPENGL_ID.fetch_add(1, Ordering::SeqCst)
}

lazy_static! {

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
    pub(crate) static ref ACTIVE_GL_TEXTURES: Mutex<FastHashMap<Epoch, FastHashMap<ExternalImageId, ActiveTexture>>> = Mutex::new(FastHashMap::default());
}

/// The Texture struct is public to the user
///
/// With this wrapper struct we can implement Send + Sync, but we don't want to do that
/// on the Texture itself
#[derive(Debug)]
pub(crate) struct ActiveTexture {
    pub(crate) texture: Texture,
}

// necessary because of lazy_static rules - theoretically unsafe,
// but we do addition / removal of textures on the main thread
unsafe impl Send for ActiveTexture { }
unsafe impl Sync for ActiveTexture { }

#[derive(Debug, Copy, Clone)]
pub(crate) struct Compositor { }

impl Default for Compositor {
    fn default() -> Self {
        Self { }
    }
}

impl ExternalImageHandler for Compositor {
    fn lock(&mut self, key: ExternalImageId, _channel_index: u8, _rendering: ImageRendering) -> ExternalImage {

        let gl_tex_lock = ACTIVE_GL_TEXTURES.lock().unwrap();

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
        let (tex, wh) = gl_tex_lock
            .values()
            .filter_map(|epoch_map| epoch_map.get(&key))
            .next()
            .and_then(|tex| {
                Some((
                    ExternalImageSource::NativeTexture(tex.texture.texture_id),
                    TypedPoint2D::<f32, DevicePixel>::new(tex.texture.width as f32, tex.texture.height as f32)
                ))
            })
            .unwrap_or((ExternalImageSource::Invalid, TypedPoint2D::zero()));

        ExternalImage {
            uv: TexelRect {
                uv0: TypedPoint2D::zero(),
                uv1: wh,
            },
            source: tex,
        }
    }

    fn unlock(&mut self, _key: ExternalImageId, _channel_index: u8) {
        // Since the renderer is currently single-threaded, there is nothing to do here
    }
}
