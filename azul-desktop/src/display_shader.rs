use azul_core::gl::{GlShader, GlShaderCreateError, GlContextPtr, Texture};
use gleam::gl;

/// When called with glDrawArrays(0, 3), generates a simple triangle that
/// spans the whole screen.
const DISPLAY_VERTEX_SHADER: &str = "
    #version 130
    out vec2 vTexCoords;
    void main() {
        float x = -1.0 + float((gl_VertexID & 1) << 2);
        float y = -1.0 + float((gl_VertexID & 2) << 1);
        vTexCoords = vec2((x+1.0)*0.5, (y+1.0)*0.5);
        gl_Position = vec4(x, y, 0, 1);
    }
";

/// Shader that samples an input texture (`fScreenTex`) to the output FB.
const DISPLAY_FRAGMENT_SHADER: &str = "
    #version 130
    in vec2 vTexCoords;
    uniform sampler2D fScreenTex;
    out vec4 fColorOut;

    void main() {
        fColorOut = texture(fScreenTex, vTexCoords);
    }
";

#[derive(Debug)]
pub struct DisplayShader {
    pub(crate) shader: GlShader,
}

impl DisplayShader {

    pub fn compile(context: &GlContextPtr) -> Result<Self, GlShaderCreateError> {
        let shader = GlShader::new(context, DISPLAY_VERTEX_SHADER, DISPLAY_FRAGMENT_SHADER)?;
        Ok(DisplayShader { shader })
    }
    /*

    // Draws a texture to the currently bound framebuffer. Use `context..bind_framebuffer(gl::FRAMEBUFFER, 0)`
    // to draw to the window.
    pub fn draw_texture_to_current_fb(&mut self, texture: &Texture) {

        // Save the current state that will be modified in the function

        // Compile or get the cached shader
        let texture_location = texture.gl_context.get_uniform_location(self.shader.program_id, "fScreenTex".into());

        // The uniform value for a sampler refers to the texture unit, not the texture id, i.e.:
        //
        // TEXTURE0 = uniform_1i(location, 0);
        // TEXTURE1 = uniform_1i(location, 1);

        texture.gl_context.active_texture(gl::TEXTURE0);
        texture.gl_context.bind_texture(gl::TEXTURE_2D, texture.texture_id);
        texture.gl_context.use_program(self.shader.program_id);
        texture.gl_context.uniform_1i(texture_location, 0);

        // The vertices are generated in the vertex shader using gl_VertexID, however,
        // drawing without a VAO is not allowed (except for glDrawArraysInstanced,
        // which is only available in OGL 3.3)

        let vao = texture.gl_context.gen_vertex_arrays(1);
        texture.gl_context.bind_vertex_array(vao.get(0).copied().unwrap());
        texture.gl_context.viewport(0, 0, texture.size.width as i32, texture.size.height as i32); // TODO: use framebuffer_size instead?
        texture.gl_context.draw_arrays(gl::TRIANGLE_STRIP, 0, 3);
        texture.gl_context.delete_vertex_arrays(vao.as_ref().into());

        texture.gl_context.bind_vertex_array(0);
        texture.gl_context.use_program(0);
        texture.gl_context.bind_texture(gl::TEXTURE_2D, 0);
        // texture.gl_context.active_texture();
    }

    */
}