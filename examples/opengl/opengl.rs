#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

extern crate azul;
extern crate azul_widgets;

use azul_widgets::button::Button;
use azul::{
    prelude::*,
    gl::{self, Texture, TextureFlags, GlContextPtr},
    callbacks::{GlCallbackInfo, GlCallbackReturn},
    window::LogicalSize,
};

struct OpenGlAppState { }

extern "C" fn layout(data: RefAny, _: LayoutInfo) -> Dom {
    Dom::gl_texture(data.clone(), render_my_texture).with_child(            // <- the Rc<OpenGlAppState> is cloned here
        Button::with_label("Hello").dom().with_id("the_button".into())      //        |
    )                                                                       //        |
}                                                                           //        |
                                                                            //        |
extern "C" fn render_my_texture(info: GlCallbackInfo) -> GlCallbackReturn { //        |
                                                                            //        |
    // to get access to the OpenGlAppState:                                 //        |
    // let state = info.get_data::<OpenGlAppState>()?;                      // <------| - and the cloned RefAny can be
    // or mutable access:                                                   //        |   downcasted here in the callback
    // let state = info.get_data_mut::<OpenGlAppState>()?;                  // <------|

    let gl_context = info.get_gl_context();
    let texture_size = info.get_bounds().get_logical_size();

    println!("rendering frame ...");

    GlCallbackReturn {
        // If the texture is None, the rect will simply be rendered as transparent
        texture: render_my_texture_inner(gl_context, texture_size).into()
    }
}

fn render_my_texture_inner(gl_context: GlContextPtr, texture_size: LogicalSize) -> Option<Texture> {

    let framebuffers = gl_context.gen_framebuffers(1);
    gl_context.bind_framebuffer(gl::FRAMEBUFFER, framebuffers.get(0).copied()?);

    gl_context.enable(gl::TEXTURE_2D);

    // Create the texture to render to
    let textures = gl_context.gen_textures(1);

    gl_context.bind_texture(gl::TEXTURE_2D, textures.get(0).copied()?);
    gl_context.tex_image_2d(gl::TEXTURE_2D, 0, gl::RGB as i32, texture_size.width as i32, texture_size.height as i32, 0, gl::RGB, gl::UNSIGNED_BYTE, None.into());

    gl_context.tex_parameter_i(gl::TEXTURE_2D, gl::TEXTURE_MAG_FILTER, gl::NEAREST as i32);
    gl_context.tex_parameter_i(gl::TEXTURE_2D, gl::TEXTURE_MIN_FILTER, gl::NEAREST as i32);
    gl_context.tex_parameter_i(gl::TEXTURE_2D, gl::TEXTURE_WRAP_S, gl::CLAMP_TO_EDGE as i32);
    gl_context.tex_parameter_i(gl::TEXTURE_2D, gl::TEXTURE_WRAP_T, gl::CLAMP_TO_EDGE as i32);

    let depthbuffers = gl_context.gen_renderbuffers(1);
    gl_context.bind_renderbuffer(gl::RENDERBUFFER, depthbuffers.get(0).copied()?);
    gl_context.renderbuffer_storage(gl::RENDERBUFFER, gl::DEPTH_COMPONENT, texture_size.width as i32, texture_size.height as i32);
    gl_context.framebuffer_renderbuffer(gl::FRAMEBUFFER, gl::DEPTH_ATTACHMENT, gl::RENDERBUFFER, depthbuffers.get(0).copied()?);

    // Set "textures[0]" as the color attachement #0
    gl_context.framebuffer_texture_2d(gl::FRAMEBUFFER, gl::COLOR_ATTACHMENT0, gl::TEXTURE_2D, textures.get(0).copied()?, 0);

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

    // cleanup: note: no delete_textures(), OpenGL texture ID is returned to azul
    gl_context.delete_framebuffers(framebuffers.as_ref().into());
    gl_context.delete_renderbuffers(depthbuffers.as_ref().into());
    gl_context.active_texture(0);
    gl_context.bind_texture(gl::TEXTURE_2D, 0);
    gl_context.bind_framebuffer(gl::FRAMEBUFFER, 0);
    gl_context.bind_renderbuffer(gl::RENDERBUFFER, 0);

    Some(Texture {
        texture_id: textures.get(0).copied()?,
        flags: TextureFlags::default(),
        size: texture_size,
        gl_context,
    })
}

fn main() {
    let app = App::new(RefAny::new(OpenGlAppState { }), AppConfig::default(), layout);
    let az_css: azul::str::String = String::from("
        texture {
            width: 100%;
            height: 100%;
            border: 4px solid green;
            border-radius: 50px;
            box-sizing: border-box;
        }
        #the_button {
            width: 200px;
            height: 50px;
            position: absolute;
            top: 50px;
            left: 50px;
        }
    ").into();
    println!("css string: {}", az_css);
    let css = Css::override_native(az_css); // .unwrap()
    println!("sizeof Css: {}", std::mem::size_of::<Css>());
    let w = WindowCreateOptions::new(css);
    println!("sizeof WindowCreateOptions: {}", std::mem::size_of::<WindowCreateOptions>());
    println!("printing: {:?}", w);
    println!("printing ok");
    app.run(w);
}