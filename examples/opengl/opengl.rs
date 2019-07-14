#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

extern crate azul;
extern crate gleam;

use azul::prelude::*;
use azul::widgets::button::Button;
use gleam::gl;

const CSS: &str = "
    texture {
        width: 100%;
        height: 100%;
        border: 4px solid green;
        box-sizing: border-box;
    }

    #the_button {
        width: 200px;
        height: 50px;
        position: absolute;
        top: 50px;
        left: 50px;
    }
";

struct OpenGlAppState { }

impl Layout for OpenGlAppState {
    fn layout(&self, _info: LayoutInfo<Self>) -> Dom<Self> {
        Dom::gl_texture(render_my_texture, StackCheckedPointer::new_entire_struct(self))
        .with_child(Button::with_label("Hello").dom().with_id("the_button"))
    }
}

fn render_my_texture(info: GlCallbackInfoUnchecked<OpenGlAppState>) -> GlCallbackReturn {

    println!("rendering opengl state!");

    let texture_size = info.bounds.get_logical_size();
    let gl_context = info.layout_info.window.get_gl_context();

    let framebuffers = gl_context.gen_framebuffers(1);
    gl_context.bind_framebuffer(gl::FRAMEBUFFER, framebuffers[0]);

    gl_context.enable(gl::TEXTURE_2D);

    // Create the texture to render to
    let textures = gl_context.gen_textures(1);

    gl_context.bind_texture(gl::TEXTURE_2D, textures[0]);
    gl_context.tex_image_2d(gl::TEXTURE_2D, 0, gl::RGB as i32, texture_size.width as i32, texture_size.height as i32, 0, gl::RGB, gl::UNSIGNED_BYTE, None);

    gl_context.tex_parameter_i(gl::TEXTURE_2D, gl::TEXTURE_MAG_FILTER, gl::NEAREST as i32);
    gl_context.tex_parameter_i(gl::TEXTURE_2D, gl::TEXTURE_MIN_FILTER, gl::NEAREST as i32);
    gl_context.tex_parameter_i(gl::TEXTURE_2D, gl::TEXTURE_WRAP_S, gl::CLAMP_TO_EDGE as i32);
    gl_context.tex_parameter_i(gl::TEXTURE_2D, gl::TEXTURE_WRAP_T, gl::CLAMP_TO_EDGE as i32);

    let depthbuffers = gl_context.gen_renderbuffers(1);
    gl_context.bind_renderbuffer(gl::RENDERBUFFER, depthbuffers[0]);
    gl_context.renderbuffer_storage(gl::RENDERBUFFER, gl::DEPTH_COMPONENT, texture_size.width as i32, texture_size.height as i32);
    gl_context.framebuffer_renderbuffer(gl::FRAMEBUFFER, gl::DEPTH_ATTACHMENT, gl::RENDERBUFFER, depthbuffers[0]);

    // Set "textures[0]" as the color attachement #0
    gl_context.framebuffer_texture_2d(gl::FRAMEBUFFER, gl::COLOR_ATTACHMENT0, gl::TEXTURE_2D, textures[0], 0);

    // gl_context.draw_buffers(&[gl::COLOR_ATTACHMENT0]);

    // Check that the framebuffer is complete
    debug_assert!(gl_context.check_frame_buffer_status(gl::FRAMEBUFFER) == gl::FRAMEBUFFER_COMPLETE);

    // Disable SRGB and multisample, otherwise, WebRender will crash
    gl_context.disable(gl::FRAMEBUFFER_SRGB);
    gl_context.disable(gl::MULTISAMPLE);
    gl_context.disable(gl::POLYGON_SMOOTH);

    // DRAW HERE
    gl_context.viewport(0, 0, texture_size.width as i32, texture_size.height as i32);
    gl_context.clear_color(0.0, 1.0, 0.0, 1.0);
    gl_context.clear(gl::COLOR_BUFFER_BIT);
    gl_context.clear_depth(0.0);
    gl_context.clear(gl::DEPTH_BUFFER_BIT);

    gl_context.delete_framebuffers(&framebuffers);
    gl_context.delete_renderbuffers(&depthbuffers);
    gl_context.active_texture(0);
    gl_context.bind_texture(gl::TEXTURE_2D, 0);
    gl_context.bind_framebuffer(gl::FRAMEBUFFER, 0);
    gl_context.bind_renderbuffer(gl::RENDERBUFFER, 0);

    Some(Texture {
        texture_id: textures[0],
        size: texture_size,
        gl_context,
    })
}

fn main() {
    let mut app = App::new(OpenGlAppState { }, AppConfig::default()).unwrap();
    let css = css::override_native(CSS).unwrap();
    let window = app.create_window(WindowCreateOptions::default(), css).unwrap();
    app.run(window).unwrap();
}