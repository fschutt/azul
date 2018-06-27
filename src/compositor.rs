//! The compositor takes all the textures for a frame and draws them on top of each other.
//! This makes it possible to use OpenGL images in the background and compose SVG elements
//! into the UI.

use dom::Texture;
use {FastHashMap, FastHashSet};
use webrender::{ExternalImageHandler, ExternalImageSource};
use webrender::api::{ExternalImageId, TexelRect, DevicePixel};
use std::sync::{Arc, Mutex, atomic::{Ordering, AtomicUsize}};

use glium::{
    Program, VertexBuffer, Display,
    index::{NoIndices, PrimitiveType::TriangleStrip},
    texture::texture2d::Texture2d,
    backend::Facade,
};
use webrender::ExternalImage;
use euclid::TypedPoint2D;

lazy_static! {
    /// Non-cleaned up textures. When a GlTexture is registered, it has to stay active as long
    /// as webrender needs it for drawing. To transparently do this, we store the epoch that the
    /// texture was originally created with, and check, **after we have drawn the frame**,
    /// if there are any textures that need cleanup.
    ///
    /// Because the Texture2d is wrapped in an Rc, the destructor (which cleans up the OpenGL
    /// texture) does not run until we remove the textures
    pub(crate) static ref ACTIVE_GL_TEXTURES: Mutex<FastHashMap<ExternalImageId, ActiveTexture>> = Mutex::new(FastHashMap::default());
    pub(crate) static ref TO_DELETE_TEXTURES: Mutex<FastHashSet<ExternalImageId>> = Mutex::new(FastHashSet::default());
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

#[derive(Debug)]
pub(crate) struct Compositor { }

impl Default for Compositor {
    fn default() -> Self {
        Self { }
    }
}

impl ExternalImageHandler for Compositor {
    fn lock(&mut self, key: ExternalImageId, _channel_index: u8) -> ExternalImage {
        use glium::GlObject;

        let gl_tex_lock = ACTIVE_GL_TEXTURES.lock().unwrap();
        let tex = &gl_tex_lock[&key];

        ExternalImage {
            uv: TexelRect {
                uv0: TypedPoint2D::zero(),
                uv1: TypedPoint2D::<f32, DevicePixel>::new(tex.texture.inner.width() as f32, tex.texture.inner.height() as f32),
            },
            source: ExternalImageSource::NativeTexture(tex.texture.inner.get_id()),
        }
    }

    fn unlock(&mut self, key: ExternalImageId, _channel_index: u8) {
        TO_DELETE_TEXTURES.lock().unwrap().insert(key);
        // ACTIVE_GL_TEXTURES.lock().unwrap().remove(&key);
    }
}

// Empty test, for some reason codecov doesn't detect any files (and therefore
// doesn't report codecov % correctly) except if they have at least one test in
// the file. This is an empty test, which should be updated later on
#[test]
fn __codecov_test_compositor_file() {

}