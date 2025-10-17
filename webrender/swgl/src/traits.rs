use crate::common_types::*;
use std::ffi::CStr;

/// Trait mirroring the C++ ProgramImpl struct.
/// It acts as the main entry point for a shader program.
pub trait Program {
    fn get_vertex_shader(&mut self) -> &mut dyn VertexShader;
    fn get_fragment_shader(&mut self) -> &mut dyn FragmentShader;
    fn get_uniform(&self, name: &CStr) -> i32;
    fn get_attrib(&self, name: &CStr) -> i32;
    fn bind_attrib(&mut self, name: &CStr, index: i32);
    fn interpolants_size(&self) -> usize;
}

/// Trait mirroring the C++ VertexShaderImpl struct.
pub trait VertexShader {
    fn init_batch(&mut self);
    fn load_attribs(&mut self, attribs: &[VertexAttrib], start: u32, instance: i32, count: i32);
    fn run_primitive(&mut self, interps: *mut u8, interp_stride: usize);
    
    // Uniform setters
    fn set_uniform_1i(&mut self, index: i32, value: i32);
    fn set_uniform_4fv(&mut self, index: i32, value: &[f32; 4]);
    fn set_uniform_matrix4fv(&mut self, index: i32, value: &[f32; 16]);
}

/// Trait mirroring the C++ FragmentShaderImpl struct.
pub trait FragmentShader {
    // These methods will handle interpolation across a triangle/span.
    // The raw pointers directly map to the C++ implementation for performance.
    fn read_interp_inputs(&mut self, init: *const u8, step: *const u8);
    fn run(&mut self);
    fn skip(&mut self, steps: i32);

    // Placeholder for fast-path span drawing.
    // The return value indicates if the fast path was taken.
    fn draw_span_rgba8(&mut self) -> i32 { 0 }
    fn draw_span_r8(&mut self) -> i32 { 0 }
}
