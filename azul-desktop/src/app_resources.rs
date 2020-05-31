use webrender::api::{RenderApi as WrRenderApi};
pub use azul_core::app_resources::*;

/// Wrapper struct because it's not possible to implement traits on foreign types
pub struct WrApi {
    pub api: WrRenderApi,
}

impl FontImageApi for WrApi {
    fn new_image_key(&self) -> ImageKey {
        use crate::wr_translate::translate_image_key_wr;
        translate_image_key_wr(self.api.generate_image_key())
    }
    fn new_font_key(&self) -> FontKey {
        use crate::wr_translate::translate_font_key_wr;
        translate_font_key_wr(self.api.generate_font_key())
    }
    fn new_font_instance_key(&self) -> FontInstanceKey {
        use crate::wr_translate::translate_font_instance_key_wr;
        translate_font_instance_key_wr(self.api.generate_font_instance_key())
    }
    fn update_resources(&self, updates: Vec<ResourceUpdate>) {
        use crate::wr_translate::wr_translate_resource_update;
        let wr_updates = updates.into_iter().map(wr_translate_resource_update).collect();
        self.api.update_resources(wr_updates);
    }
    fn flush_scene_builder(&self) {
        self.api.flush_scene_builder();
    }
}
