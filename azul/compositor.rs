use webrender::{
    ExternalImageHandler as WrExternalImageHandler,
    ExternalImage as WrExternalImage,
    ExternalImageSource as WrExternalImageSource,
    api::{
        TexelRect as WrTexelRect,
        DevicePoint as WrDevicePoint,
        ImageRendering as WrImageRendering,
        ExternalImageId as WrExternalImageId,
    },
};
use azul_core::gl::get_opengl_texture;

#[derive(Debug, Default, Copy, Clone)]
pub(crate) struct Compositor { }

impl WrExternalImageHandler for Compositor {
    fn lock(&mut self, key: WrExternalImageId, _channel_index: u8, _rendering: WrImageRendering) -> WrExternalImage {

        use crate::wr_translate::translate_external_image_id_wr;

        let (tex, wh) = get_opengl_texture(&translate_external_image_id_wr(key))
        .map(|(tex, (w, h))| (WrExternalImageSource::NativeTexture(tex), WrDevicePoint::new(w, h)))
        .unwrap_or((WrExternalImageSource::Invalid, WrDevicePoint::zero()));

        WrExternalImage {
            uv: WrTexelRect {
                uv0: WrDevicePoint::zero(),
                uv1: wh,
            },
            source: tex,
        }
    }

    fn unlock(&mut self, _key: WrExternalImageId, _channel_index: u8) {
        // Since the renderer is currently single-threaded, there is nothing to do here
    }
}

// see: https://github.com/servo/rust-offscreen-rendering-context/pull/65/files
#[cfg(feature="osmesa")] 
mod osmesa {

    use osmesa_sys;
    use std::ptr;

    pub struct OSMesaContext {
        buffer: Vec<u8>,
        context: osmesa_sys::OSMesaContext,
    }

    pub struct OSMesaContextHandle(osmesa_sys::OSMesaContext);

    unsafe impl Send for OSMesaContextHandle {}

    #[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
    pub enum OsMesaCreateError {
        // OSMesaCreateContext returned a null pointer (should never happen)
        NullContext,
    }

    impl OSMesaContext {

        /// Returns a memory-backed RGBA-buffer
        pub fn new(width: usize, height: usize, shared_ctxt: Option<osmesa_sys::OSMesaContext>) -> Result<Self, OsMesaCreateError> {

            let shared = shared_ctxt.unwrap_or(ptr::null_mut());

            let context = unsafe { osmesa_sys::OSMesaCreateContext(osmesa_sys::OSMESA_RGBA, shared) };

            if context.is_null() { return Err(OsMesaCreateError::NullContext); }

            Ok(OSMesaContext {
                buffer: vec![0u8; width * height * 4],
                context: context,
            })
        }
    }

    impl Drop for OSMesaContext {
        fn drop(&mut self) {
            unsafe { osmesa_sys::OSMesaDestroyContext(self.context) }
        }
    }
}
