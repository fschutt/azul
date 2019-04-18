extern crate azul;

use azul::prelude::*;
use azul::azul_dependencies::glium::Surface;

struct OpenGlAppState { }

impl Layout for OpenGlAppState {
    fn layout(&self, _info: LayoutInfo<Self>) -> Dom<Self> {
        // println!("Pause");
        // let mut input = String::new();
        // let _ = std::io::stdin().read_line(&mut input);
        Dom::gl_texture(GlTextureCallback(render_my_texture), StackCheckedPointer::new(self, self).unwrap())
    }
}

fn render_my_texture(
    _state: &StackCheckedPointer<OpenGlAppState>,
    info: LayoutInfo<OpenGlAppState>,
    hi_dpi_bounds: HidpiAdjustedBounds)
-> Texture {

    let physical_size = hi_dpi_bounds.get_physical_size();

    let mut texture = Texture::new(info.gl_context.clone(), physical_size.width, physical_size.height);

    {
        let mut fb = FrameBuffer::new(&mut texture);
        fb.bind();
        info.gl_context.clear_color(0.0, 1.0, 0.0, 1.0);
        fb.unbind();
        fb.finish();
    }

    texture
}

fn main() {
    let mut app = App::new(OpenGlAppState { }, AppConfig::default()).unwrap();
    let window = app.create_window(WindowCreateOptions::default(), css::native()).unwrap();
    app.run(window).unwrap();
}