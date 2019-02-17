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
-> Option<Texture>
{
    let texture = info.window.read_only_window().create_texture(
        hi_dpi_bounds.physical_size.width as u32,
        hi_dpi_bounds.physical_size.height as u32
    );

    texture.as_surface().clear_color(0.0, 1.0, 0.0, 1.0);
    Some(texture)
}

fn main() {
    let app = App::new(OpenGlAppState { }, AppConfig::default());
    let window = Window::new(WindowCreateOptions::default(), css::native()).unwrap();
    app.run(window).unwrap();
}