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
    let physical_size = hi_dpi_bounds.get_physical_size();
    let texture = info.window.read_only_window().create_texture(
        physical_size.width as u32,
        physical_size.height as u32
    );

    texture.as_surface().clear_color(0.0, 1.0, 0.0, 1.0);
    Some(texture)
}

fn main() {
    let mut app = App::new(OpenGlAppState { }, AppConfig::default()).unwrap();
    let window = app.create_window(WindowCreateOptions::default(), css::native()).unwrap();
    app.run(window).unwrap();
}