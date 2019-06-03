extern crate azul;
extern crate gleam;

use azul::prelude::*;
use gleam::gl;

const CSS: &str = "
    texture {
        width: 100%;
        height: 100%;
        border: 4px solid green;
        box-sizing: border-box;
    }
";

struct OpenGlAppState { }

impl Layout for OpenGlAppState {
    fn layout(&self, _info: LayoutInfo<Self>) -> Dom<Self> {
        Dom::gl_texture(render_my_texture, StackCheckedPointer::new_entire_struct(self))
    }
}

fn render_my_texture(info: GlCallbackInfoUnchecked<OpenGlAppState>) -> GlCallbackReturn {

    println!("rendering opengl state!");

    let physical_size = info.bounds.get_physical_size();
    let gl_context = info.layout_info.window.get_gl_context();

    // Create the texture to render to
    let textures = gl_context.gen_textures(1);
    let texture_id = textures[0];

    let framebuffers = gl_context.gen_framebuffers(1);
    let framebuffer_id = framebuffers[0];

    gl_context.bind_framebuffer(gl::FRAMEBUFFER, framebuffer_id);
    gl_context.enable(gl::TEXTURE_2D);
    gl_context.active_texture(texture_id);
    gl_context.bind_texture(gl::TEXTURE_2D, texture_id);

    gl_context.tex_parameter_i(gl::TEXTURE_2D, gl::TEXTURE_MAG_FILTER, gl::NEAREST as i32);
    gl_context.tex_parameter_i(gl::TEXTURE_2D, gl::TEXTURE_MIN_FILTER, gl::NEAREST as i32);
    gl_context.tex_parameter_i(gl::TEXTURE_2D, gl::TEXTURE_WRAP_S, gl::CLAMP_TO_EDGE as i32);
    gl_context.tex_parameter_i(gl::TEXTURE_2D, gl::TEXTURE_WRAP_T, gl::CLAMP_TO_EDGE as i32);
    gl_context.tex_image_2d(gl::TEXTURE_2D, 0, gl::RGBA as i32, physical_size.width as i32, physical_size.height as i32, 0, gl::RGBA, gl::UNSIGNED_BYTE, None);

    // Set "texture_id" as the color attachement #0
    gl_context.framebuffer_texture_2d(gl::FRAMEBUFFER, gl::COLOR_ATTACHMENT0, gl::TEXTURE_2D, texture_id, 0);

    // Check that the framebuffer is complete
    debug_assert!(gl_context.check_frame_buffer_status(gl::FRAMEBUFFER) == gl::FRAMEBUFFER_COMPLETE);

    gl_context.viewport(0, 0, physical_size.width as i32, physical_size.height as i32);
    gl_context.clear_color(0.0, 1.0, 0.0, 1.0);

    println!("deleting FB: {:?}", framebuffer_id);
    gl_context.delete_framebuffers(&[framebuffer_id]);
    gl_context.active_texture(0);
    gl_context.bind_texture(gl::TEXTURE_2D, 0);
    gl_context.disable(gl::TEXTURE_2D);
    gl_context.bind_framebuffer(gl::FRAMEBUFFER, 0);

    /*
        let mut texture = Texture::new(
            gl_context.clone(),
            physical_size.width as usize,
            physical_size.height as usize
        );

        {
            let mut fb = FrameBuffer::new(&mut texture);
            fb.bind();
            gl_context.clear_color(0.0, 1.0, 0.0, 1.0);
            fb.unbind();
        }
    */

    // gl_context.delete_textures(&textures);

    println!("texture ID: {:?}", texture_id);

    Texture {
        texture_id,
        width: physical_size.width as usize,
        height: physical_size.height as usize,
        gl_context,
    }
}

fn main() {
    let mut app = App::new(OpenGlAppState { }, AppConfig::default()).unwrap();
    let css = css::override_native(CSS).unwrap();
    let window = app.create_window(WindowCreateOptions::default(), css).unwrap();
    app.run(window).unwrap();
}