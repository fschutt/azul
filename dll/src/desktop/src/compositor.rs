use azul_core::gl::get_opengl_texture;
use webrender::api::{
    ExternalImage as WrExternalImage, ExternalImageHandler as WrExternalImageHandler,
    ExternalImageId as WrExternalImageId, ExternalImageSource as WrExternalImageSource,
    ImageRendering as WrImageRendering,
    units::{DevicePoint as WrDevicePoint, TexelRect as WrTexelRect},
};

#[derive(Debug, Default, Copy, Clone)]
pub(crate) struct Compositor {}

impl WrExternalImageHandler for Compositor {
    fn lock(
        &mut self,
        key: WrExternalImageId,
        _channel_index: u8,
        _rendering: WrImageRendering,
    ) -> WrExternalImage {
        use crate::wr_translate::translate_external_image_id_wr;

        let twh = get_opengl_texture(&translate_external_image_id_wr(key)).map(|(tex, (w, h))| {
            (
                WrExternalImageSource::NativeTexture(tex),
                WrDevicePoint::new(w, h),
            )
        });

        let (tex, wh) = match twh {
            Some(s) => s,
            None => (WrExternalImageSource::Invalid, WrDevicePoint::zero()),
        };

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
