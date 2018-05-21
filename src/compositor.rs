//! The compositor takes all the textures for a frame and draws them on top of each other.
//! This makes it possible to use OpenGL images in the background and compose SVG elements
//! into the UI.

use glium::texture::compressed_srgb_texture2d::CompressedSrgbTexture2d;

#[derive(Default, Debug)]
pub struct Compositor {
    textures: Vec<CompressedSrgbTexture2d>
}

impl Compositor {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn push_texture(&mut self, texture: CompressedSrgbTexture2d) {
        self.textures.push(texture);
    }

/*
    /// Draw all textures onto a final texture, which can then be displayed on the screen
    pub fn compose(self) -> CompressedSrgbTexture2d {

    }
*/
}