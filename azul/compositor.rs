use webrender::api::{
    units::{
        TexelRect as WrTexelRect,
        DevicePoint as WrDevicePoint,
    },
    ExternalImageHandler as WrExternalImageHandler,
    ExternalImage as WrExternalImage,
    ExternalImageSource as WrExternalImageSource,
    ImageRendering as WrImageRendering,
    ExternalImageId as WrExternalImageId,
};
use azul_core::gl::get_opengl_texture;

#[derive(Debug, Default, Copy, Clone)]
pub(crate) struct Compositor { }

impl WrExternalImageHandler for Compositor {
    fn lock(&mut self, key: WrExternalImageId, _channel_index: u8, _rendering: WrImageRendering) -> WrExternalImage {

        use crate::wr_translate::translate_external_image_id_wr;

        let (tex, wh) = get_opengl_texture(&translate_external_image_id_wr(key))
        .map(|(tex, (w, h))| (WrExternalImageSource::NativeTexture(tex), WrDevicePoint::new(w, h)))
        .unwrap_or((WrExternalImageSource::Invalid, WrDevicePoint::zero()));

        WrExternalImage {
            uv: WrTexelRect {
                uv0: WrDevicePoint::zero(),
                uv1: wh,
            },
            source: tex,
        }
    }

    fn unlock(&mut self, _key: WrExternalImageId, _channel_index: u8) {
        // Since the renderer is currently single-threaded, there is nothing to do here
    }
}

// see: https://github.com/servo/rust-offscreen-rendering-context/pull/65/files
#[cfg(feature="osmesa")] 
mod osmesa {

    use osmesa_sys::OSMesaContext;
    use gleam::Gl;

    pub use platform::{OSMesaContext, OSMesaContextHandle};

    pub struct OSMesaContext {
        buffer: Vec<u8>,
        context: osmesa_sys::OSMesaContext,
    }

    pub struct OSMesaContextHandle(osmesa_sys::OSMesaContext);

    unsafe impl Send for OSMesaContextHandle {}

    #[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
    pub enum OsMesaCreateError {
        // OSMesaCreateContext returned a null pointer (should never happen)
        NullContext,
    }

    impl OSMesaContext {

        /// Returns a memory-backed RGBA-buffer
        pub fn new(width: usize, height: usize, shared_ctxt: Option<OSMesaContext>) -> Result<Self, OsMesaCreateError> {

            let shared = shared_ctxt.unwrap_or(ptr::null_mut());

            let context = unsafe { osmesa_sys::OSMesaCreateContext(osmesa_sys::OSMESA_RGBA, shared) };

            if context.is_null() { return Err(OsMesaCreateError::NullContext); }

            Ok(OSMesaContext {
                buffer: vec![0u8; width * height * 4],
                context: context,
            })
        }

        pub fn resize(&mut self, width: usize, height: usize) {
            self.buffer = vec![0u8; width * height * 4];
        }
    }

    impl Gl for OSMesaContext {
        fn get_type(&self) -> GlType { (self.get_type)() }
        fn create_program(&self) -> GLuint { (self.create_program)() }
        fn flush(&self) { (self.flush)() }
        fn finish(&self) { (self.finish)() }
        fn get_error(&self) -> GLenum { (self.get_error)() }
        fn pop_group_marker_ext(&self) { (self.pop_group_marker_ext)() }
        fn pop_debug_group_khr(&self) { (self.pop_debug_group_khr)() }
        fn blend_barrier_khr(&self) { (self.blend_barrier_khr)() }
        fn get_debug_messages(&self) -> Vec<DebugMessage> { (self.get_debug_messages)() }
        fn buffer_data_untyped(&self, target: GLenum, size: GLsizeiptr, data: *const GLvoid, usage: GLenum) { (self.buffer_data_untyped)(target, size, data, usage) }
        fn buffer_sub_data_untyped(&self, target: GLenum, offset: isize, size: GLsizeiptr, data: *const GLvoid) { (self.buffer_sub_data_untyped)(target, offset, size, data) }
        fn map_buffer(&self, target: GLenum, access: GLbitfield) -> *mut c_void { (self.map_buffer)(target, access) }
        fn map_buffer_range(&self, target: GLenum, offset: GLintptr, length: GLsizeiptr, access: GLbitfield) -> *mut c_void { (self.map_buffer_range)(target, offset, length, access) }
        fn unmap_buffer(&self, target: GLenum) -> GLboolean { (self.unmap_buffer)(target) }
        fn tex_buffer(&self, target: GLenum, internal_format: GLenum, buffer: GLuint) { (self.tex_buffer)(target, internal_format, buffer) }
        fn shader_source(&self, shader: GLuint, strings: &[&[u8]]) { (self.shader_source)(shader, strings) }
        fn read_buffer(&self, mode: GLenum) { (self.read_buffer)(mode) }
        fn read_pixels_into_buffer(&self, x: GLint, y: GLint, width: GLsizei, height: GLsizei, format: GLenum, pixel_type: GLenum, dst_buffer: &mut [u8]) { (self.read_pixels_into_buffer)(x, y, width, height, format, pixel_type, dst_buffer) }
        fn read_pixels(&self, x: GLint, y: GLint, width: GLsizei, height: GLsizei, format: GLenum, pixel_type: GLenum) -> Vec<u8> { (self.read_pixels)(x, y, width, height, format, pixel_type) }
        unsafe fn read_pixels_into_pbo(&self, x: GLint, y: GLint, width: GLsizei, height: GLsizei, format: GLenum, pixel_type: GLenum) { (self.fn)(x, y, width, height, format, pixel_type) }
        fn sample_coverage(&self, value: GLclampf, invert: bool) { (self.sample_coverage)(value, invert) }
        fn polygon_offset(&self, factor: GLfloat, units: GLfloat) { (self.polygon_offset)(factor, units) }
        fn pixel_store_i(&self, name: GLenum, param: GLint) { (self.pixel_store_i)(name, param) }
        fn gen_buffers(&self, n: GLsizei) -> Vec<GLuint> { (self.gen_buffers)(n) }
        fn gen_renderbuffers(&self, n: GLsizei) -> Vec<GLuint> { (self.gen_renderbuffers)(n) }
        fn gen_framebuffers(&self, n: GLsizei) -> Vec<GLuint> { (self.gen_framebuffers)(n) }
        fn gen_textures(&self, n: GLsizei) -> Vec<GLuint> { (self.gen_textures)(n) }
        fn gen_vertex_arrays(&self, n: GLsizei) -> Vec<GLuint> { (self.gen_vertex_arrays)(n) }
        fn gen_queries(&self, n: GLsizei) -> Vec<GLuint> { (self.gen_queries)(n) }
        fn begin_query(&self, target: GLenum, id: GLuint) { (self.begin_query)(target, id) }
        fn end_query(&self, target: GLenum) { (self.end_query)(target) }
        fn query_counter(&self, id: GLuint, target: GLenum) { (self.query_counter)(id, target) }
        fn get_query_object_iv(&self, id: GLuint, pname: GLenum) -> i32 { (self.get_query_object_iv)(id, pname) }
        fn get_query_object_uiv(&self, id: GLuint, pname: GLenum) -> u32 { (self.get_query_object_uiv)(id, pname) }
        fn get_query_object_i64v(&self, id: GLuint, pname: GLenum) -> i64 { (self.get_query_object_i64v)(id, pname) }
        fn get_query_object_ui64v(&self, id: GLuint, pname: GLenum) -> u64 { (self.get_query_object_ui64v)(id, pname) }
        fn delete_queries(&self, queries: &[GLuint]) { (self.delete_queries)(queries) }
        fn delete_vertex_arrays(&self, vertex_arrays: &[GLuint]) { (self.delete_vertex_arrays)(vertex_arrays) }
        fn delete_buffers(&self, buffers: &[GLuint]) { (self.delete_buffers)(buffers) }
        fn delete_renderbuffers(&self, renderbuffers: &[GLuint]) { (self.delete_renderbuffers)(renderbuffers) }
        fn delete_framebuffers(&self, framebuffers: &[GLuint]) { (self.delete_framebuffers)(framebuffers) }
        fn delete_textures(&self, textures: &[GLuint]) { (self.delete_textures)(textures) }
        fn framebuffer_renderbuffer(&self, target: GLenum, attachment: GLenum, renderbuffertarget: GLenum, renderbuffer: GLuint) { (self.framebuffer_renderbuffer)(target, attachment, renderbuffertarget, renderbuffer) }
        fn renderbuffer_storage(&self, target: GLenum, internalformat: GLenum, width: GLsizei, height: GLsizei) { (self.renderbuffer_storage)(target, internalformat, width, height) }
        fn depth_func(&self, func: GLenum) { (self.depth_func)(func) }
        fn active_texture(&self, texture: GLenum) { (self.active_texture)(texture) }
        fn attach_shader(&self, program: GLuint, shader: GLuint) { (self.attach_shader)(program, shader) }
        fn bind_attrib_location(&self, program: GLuint, index: GLuint, name: &str) { (self.bind_attrib_location)(program, index, name) }
        unsafe fn get_uniform_iv(&self, program: GLuint, location: GLint, result: &mut [GLint]) { (self.fn)(program, location, result) }
        unsafe fn get_uniform_fv(&self, program: GLuint, location: GLint, result: &mut [GLfloat]) { (self.fn)(program, location, result) }
        fn get_uniform_block_index(&self, program: GLuint, name: &str) -> GLuint { (self.get_uniform_block_index)(program, name) }
         fn get_uniform_indices(&self,  program: GLuint, names: &[&str]) -> Vec<GLuint> { (self.get_uniform_indices)(program, names) }
        fn bind_buffer_base(&self, target: GLenum, index: GLuint, buffer: GLuint) { (self.bind_buffer_base)(target, index, buffer) }
        fn bind_buffer_range(&self, target: GLenum, index: GLuint, buffer: GLuint, offset: GLintptr, size: GLsizeiptr) { (self.bind_buffer_range)(target, index, buffer, offset, size) }
        fn uniform_block_binding(&self, program: GLuint, uniform_block_index: GLuint, uniform_block_binding: GLuint) { (self.uniform_block_binding)(program, uniform_block_index, uniform_block_binding) }
        fn bind_buffer(&self, target: GLenum, buffer: GLuint) { (self.bind_buffer)(target, buffer) }
        fn bind_vertex_array(&self, vao: GLuint) { (self.bind_vertex_array)(vao) }
        fn bind_renderbuffer(&self, target: GLenum, renderbuffer: GLuint) { (self.bind_renderbuffer)(target, renderbuffer) }
        fn bind_framebuffer(&self, target: GLenum, framebuffer: GLuint) { (self.bind_framebuffer)(target, framebuffer) }
        fn bind_texture(&self, target: GLenum, texture: GLuint) { (self.bind_texture)(target, texture) }
        fn draw_buffers(&self, bufs: &[GLenum]) { (self.draw_buffers)(bufs) }
        fn tex_image_2d(&self, target: GLenum, level: GLint, internal_format: GLint, width: GLsizei, height: GLsizei, border: GLint, format: GLenum, ty: GLenum, opt_data: Option<&[u8]>) { (self.tex_image_2d)(target, level, internal_format, width, height, border, format, ty, opt_data) }
        fn compressed_tex_image_2d(&self, target: GLenum, level: GLint, internal_format: GLenum, width: GLsizei, height: GLsizei, border: GLint, data: &[u8]) { (self.compressed_tex_image_2d)(target, level, internal_format, width, height, border, data) }
        fn compressed_tex_sub_image_2d(&self, target: GLenum, level: GLint, xoffset: GLint, yoffset: GLint, width: GLsizei, height: GLsizei, format: GLenum, data: &[u8]) { (self.compressed_tex_sub_image_2d)(target, level, xoffset, yoffset, width, height, format, data) }
        fn tex_image_3d(&self, target: GLenum, level: GLint, internal_format: GLint, width: GLsizei, height: GLsizei, depth: GLsizei, border: GLint, format: GLenum, ty: GLenum, opt_data: Option<&[u8]>) { (self.tex_image_3d)(target, level, internal_format, width, height, depth, border, format, ty, opt_data) }
        fn copy_tex_image_2d(&self, target: GLenum, level: GLint, internal_format: GLenum, x: GLint, y: GLint, width: GLsizei, height: GLsizei, border: GLint) { (self.copy_tex_image_2d)(target, level, internal_format, x, y, width, height, border) }
        fn copy_tex_sub_image_2d(&self, target: GLenum, level: GLint, xoffset: GLint, yoffset: GLint, x: GLint, y: GLint, width: GLsizei, height: GLsizei) { (self.copy_tex_sub_image_2d)(target, level, xoffset, yoffset, x, y, width, height) }
        fn copy_tex_sub_image_3d(&self, target: GLenum, level: GLint, xoffset: GLint, yoffset: GLint, zoffset: GLint, x: GLint, y: GLint, width: GLsizei, height: GLsizei) { (self.copy_tex_sub_image_3d)(target, level, xoffset, yoffset, zoffset, x, y, width, height) }
        fn tex_sub_image_2d(&self, target: GLenum, level: GLint, xoffset: GLint, yoffset: GLint, width: GLsizei, height: GLsizei, format: GLenum, ty: GLenum, data: &[u8]) { (self.tex_sub_image_2d)(target, level, xoffset, yoffset, width, height, format, ty, data) }
        fn tex_sub_image_2d_pbo(&self, target: GLenum, level: GLint, xoffset: GLint, yoffset: GLint, width: GLsizei, height: GLsizei, format: GLenum, ty: GLenum, offset: usize) { (self.tex_sub_image_2d_pbo)(target, level, xoffset, yoffset, width, height, format, ty, offset) }
        fn tex_sub_image_3d(&self, target: GLenum, level: GLint, xoffset: GLint, yoffset: GLint, zoffset: GLint, width: GLsizei, height: GLsizei, depth: GLsizei, format: GLenum, ty: GLenum, data: &[u8]) { (self.tex_sub_image_3d)(target, level, xoffset, yoffset, zoffset, width, height, depth, format, ty, data) }
        fn tex_sub_image_3d_pbo(&self, target: GLenum, level: GLint, xoffset: GLint, yoffset: GLint, zoffset: GLint, width: GLsizei, height: GLsizei, depth: GLsizei, format: GLenum, ty: GLenum, offset: usize) { (self.tex_sub_image_3d_pbo)(target, level, xoffset, yoffset, zoffset, width, height, depth, format, ty, offset) }
        fn tex_storage_2d(&self, target: GLenum, levels: GLint, internal_format: GLenum, width: GLsizei, height: GLsizei) { (self.tex_storage_2d)(target, levels, internal_format, width, height) }
        fn tex_storage_3d(&self, target: GLenum, levels: GLint, internal_format: GLenum, width: GLsizei, height: GLsizei, depth: GLsizei) { (self.tex_storage_3d)(target, levels, internal_format, width, height, depth) }
        fn get_tex_image_into_buffer(&self, target: GLenum, level: GLint, format: GLenum, ty: GLenum, output: &mut [u8]) { (self.get_tex_image_into_buffer)(target, level, format, ty, output) }
        unsafe fn copy_image_sub_data(&self, src_name: GLuint, src_target: GLenum, src_level: GLint, src_x: GLint, src_y: GLint, src_z: GLint, dst_name: GLuint, dst_target: GLenum, dst_level: GLint, dst_x: GLint, dst_y: GLint, dst_z: GLint, src_width: GLsizei, src_height: GLsizei, src_depth: GLsizei) { (self.fn)(src_name, src_target, src_level, src_x, src_y, src_z, dst_name, dst_target, dst_level, dst_x, dst_y, dst_z, src_width, src_height, src_depth) }
        fn invalidate_framebuffer(&self, target: GLenum, attachments: &[GLenum]) { (self.invalidate_framebuffer)(target, attachments) }
        fn invalidate_sub_framebuffer(&self, target: GLenum, attachments: &[GLenum], xoffset: GLint, yoffset: GLint, width: GLsizei, height: GLsizei) { (self.invalidate_sub_framebuffer)(target, attachments, xoffset, yoffset, width, height) }
        unsafe fn get_integer_v(&self, name: GLenum, result: &mut [GLint]) { (self.fn)(name, result) }
        unsafe fn get_integer_64v(&self, name: GLenum, result: &mut [GLint64]) { (self.fn)(name, result) }
        unsafe fn get_integer_iv(&self, name: GLenum, index: GLuint, result: &mut [GLint]) { (self.fn)(name, index, result) }
        unsafe fn get_integer_64iv(&self, name: GLenum, index: GLuint, result: &mut [GLint64]) { (self.fn)(name, index, result) }
        unsafe fn get_boolean_v(&self, name: GLenum, result: &mut [GLboolean]) { (self.fn)(name, result) }
        unsafe fn get_float_v(&self, name: GLenum, result: &mut [GLfloat]) { (self.fn)(name, result) }
        fn get_framebuffer_attachment_parameter_iv(&self, target: GLenum, attachment: GLenum, pname: GLenum) -> GLint { (self.get_framebuffer_attachment_parameter_iv)(target, attachment, pname) }
        fn get_renderbuffer_parameter_iv(&self, target: GLenum, pname: GLenum) -> GLint { (self.get_renderbuffer_parameter_iv)(target, pname) }
        fn get_tex_parameter_iv(&self, target: GLenum, name: GLenum) -> GLint { (self.get_tex_parameter_iv)(target, name) }
        fn get_tex_parameter_fv(&self, target: GLenum, name: GLenum) -> GLfloat { (self.get_tex_parameter_fv)(target, name) }
        fn tex_parameter_i(&self, target: GLenum, pname: GLenum, param: GLint) { (self.tex_parameter_i)(target, pname, param) }
        fn tex_parameter_f(&self, target: GLenum, pname: GLenum, param: GLfloat) { (self.tex_parameter_f)(target, pname, param) }
        fn framebuffer_texture_2d(&self, target: GLenum, attachment: GLenum, textarget: GLenum, texture: GLuint, level: GLint) { (self.framebuffer_texture_2d)(target, attachment, textarget, texture, level) }
        fn framebuffer_texture_layer(&self, target: GLenum, attachment: GLenum, texture: GLuint, level: GLint, layer: GLint) { (self.framebuffer_texture_layer)(target, attachment, texture, level, layer) }
        fn blit_framebuffer(&self, src_x0: GLint, src_y0: GLint, src_x1: GLint, src_y1: GLint, dst_x0: GLint, dst_y0: GLint, dst_x1: GLint, dst_y1: GLint, mask: GLbitfield, filter: GLenum) { (self.blit_framebuffer)(src_x0, src_y0, src_x1, src_y1, dst_x0, dst_y0, dst_x1, dst_y1, mask, filter) }
        fn vertex_attrib_4f(&self, index: GLuint, x: GLfloat, y: GLfloat, z: GLfloat, w: GLfloat) { (self.vertex_attrib_4f)(index, x, y, z, w) }
        fn vertex_attrib_pointer_f32(&self, index: GLuint, size: GLint, normalized: bool, stride: GLsizei, offset: GLuint) { (self.vertex_attrib_pointer_f32)(index, size, normalized, stride, offset) }
        fn vertex_attrib_pointer(&self, index: GLuint, size: GLint, type_: GLenum, normalized: bool, stride: GLsizei, offset: GLuint) { (self.vertex_attrib_pointer)(index, size, type_, normalized, stride, offset) }
        fn vertex_attrib_i_pointer(&self, index: GLuint, size: GLint, type_: GLenum, stride: GLsizei, offset: GLuint) { (self.vertex_attrib_i_pointer)(index, size, type_, stride, offset) }
        fn vertex_attrib_divisor(&self, index: GLuint, divisor: GLuint) { (self.vertex_attrib_divisor)(index, divisor) }
        fn viewport(&self, x: GLint, y: GLint, width: GLsizei, height: GLsizei) { (self.viewport)(x, y, width, height) }
        fn scissor(&self, x: GLint, y: GLint, width: GLsizei, height: GLsizei) { (self.scissor)(x, y, width, height) }
        fn line_width(&self, width: GLfloat) { (self.line_width)(width) }
        fn use_program(&self, program: GLuint) { (self.use_program)(program) }
        fn validate_program(&self, program: GLuint) { (self.validate_program)(program) }
        fn draw_arrays(&self, mode: GLenum, first: GLint, count: GLsizei) { (self.draw_arrays)(mode, first, count) }
        fn draw_arrays_instanced(&self, mode: GLenum, first: GLint, count: GLsizei, primcount: GLsizei) { (self.draw_arrays_instanced)(mode, first, count, primcount) }
        fn draw_elements(&self, mode: GLenum, count: GLsizei, element_type: GLenum, indices_offset: GLuint) { (self.draw_elements)(mode, count, element_type, indices_offset) }
        fn draw_elements_instanced(&self, mode: GLenum, count: GLsizei, element_type: GLenum, indices_offset: GLuint, primcount: GLsizei) { (self.draw_elements_instanced)(mode, count, element_type, indices_offset, primcount) }
        fn blend_color(&self, r: f32, g: f32, b: f32, a: f32) { (self.blend_color)(r, g, b, a) }
        fn blend_func(&self, sfactor: GLenum, dfactor: GLenum) { (self.blend_func)(sfactor, dfactor) }
        fn blend_func_separate(&self, src_rgb: GLenum, dest_rgb: GLenum, src_alpha: GLenum, dest_alpha: GLenum) { (self.blend_func_separate)(src_rgb, dest_rgb, src_alpha, dest_alpha) }
        fn blend_equation(&self, mode: GLenum) { (self.blend_equation)(mode) }
        fn blend_equation_separate(&self, mode_rgb: GLenum, mode_alpha: GLenum) { (self.blend_equation_separate)(mode_rgb, mode_alpha) }
        fn color_mask(&self, r: bool, g: bool, b: bool, a: bool) { (self.color_mask)(r, g, b, a) }
        fn cull_face(&self, mode: GLenum) { (self.cull_face)(mode) }
        fn front_face(&self, mode: GLenum) { (self.front_face)(mode) }
        fn enable(&self, cap: GLenum) { (self.enable)(cap) }
        fn disable(&self, cap: GLenum) { (self.disable)(cap) }
        fn hint(&self, param_name: GLenum, param_val: GLenum) { (self.hint)(param_name, param_val) }
        fn is_enabled(&self, cap: GLenum) -> GLboolean { (self.is_enabled)(cap) }
        fn is_shader(&self, shader: GLuint) -> GLboolean { (self.is_shader)(shader) }
        fn is_texture(&self, texture: GLenum) -> GLboolean { (self.is_texture)(texture) }
        fn is_framebuffer(&self, framebuffer: GLenum) -> GLboolean { (self.is_framebuffer)(framebuffer) }
        fn is_renderbuffer(&self, renderbuffer: GLenum) -> GLboolean { (self.is_renderbuffer)(renderbuffer) }
        fn check_frame_buffer_status(&self, target: GLenum) -> GLenum { (self.check_frame_buffer_status)(target) }
        fn enable_vertex_attrib_array(&self, index: GLuint) { (self.enable_vertex_attrib_array)(index) }
        fn disable_vertex_attrib_array(&self, index: GLuint) { (self.disable_vertex_attrib_array)(index) }
        fn uniform_1f(&self, location: GLint, v0: GLfloat) { (self.uniform_1f)(location, v0) }
        fn uniform_1fv(&self, location: GLint, values: &[f32]) { (self.uniform_1fv)(location, values) }
        fn uniform_1i(&self, location: GLint, v0: GLint) { (self.uniform_1i)(location, v0) }
        fn uniform_1iv(&self, location: GLint, values: &[i32]) { (self.uniform_1iv)(location, values) }
        fn uniform_1ui(&self, location: GLint, v0: GLuint) { (self.uniform_1ui)(location, v0) }
        fn uniform_2f(&self, location: GLint, v0: GLfloat, v1: GLfloat) { (self.uniform_2f)(location, v0, v1) }
        fn uniform_2fv(&self, location: GLint, values: &[f32]) { (self.uniform_2fv)(location, values) }
        fn uniform_2i(&self, location: GLint, v0: GLint, v1: GLint) { (self.uniform_2i)(location, v0, v1) }
        fn uniform_2iv(&self, location: GLint, values: &[i32]) { (self.uniform_2iv)(location, values) }
        fn uniform_2ui(&self, location: GLint, v0: GLuint, v1: GLuint) { (self.uniform_2ui)(location, v0, v1) }
        fn uniform_3f(&self, location: GLint, v0: GLfloat, v1: GLfloat, v2: GLfloat) { (self.uniform_3f)(location, v0, v1, v2) }
        fn uniform_3fv(&self, location: GLint, values: &[f32]) { (self.uniform_3fv)(location, values) }
        fn uniform_3i(&self, location: GLint, v0: GLint, v1: GLint, v2: GLint) { (self.uniform_3i)(location, v0, v1, v2) }
        fn uniform_3iv(&self, location: GLint, values: &[i32]) { (self.uniform_3iv)(location, values) }
        fn uniform_3ui(&self, location: GLint, v0: GLuint, v1: GLuint, v2: GLuint) { (self.uniform_3ui)(location, v0, v1, v2) }
        fn uniform_4f(&self, location: GLint, x: GLfloat, y: GLfloat, z: GLfloat, w: GLfloat) { (self.uniform_4f)(location, x, y, z, w) }
        fn uniform_4i(&self, location: GLint, x: GLint, y: GLint, z: GLint, w: GLint) { (self.uniform_4i)(location, x, y, z, w) }
        fn uniform_4iv(&self, location: GLint, values: &[i32]) { (self.uniform_4iv)(location, values) }
        fn uniform_4ui(&self, location: GLint, x: GLuint, y: GLuint, z: GLuint, w: GLuint) { (self.uniform_4ui)(location, x, y, z, w) }
        fn uniform_4fv(&self, location: GLint, values: &[f32]) { (self.uniform_4fv)(location, values) }
        fn uniform_matrix_2fv(&self, location: GLint, transpose: bool, value: &[f32]) { (self.uniform_matrix_2fv)(location, transpose, value) }
        fn uniform_matrix_3fv(&self, location: GLint, transpose: bool, value: &[f32]) { (self.uniform_matrix_3fv)(location, transpose, value) }
        fn uniform_matrix_4fv(&self, location: GLint, transpose: bool, value: &[f32]) { (self.uniform_matrix_4fv)(location, transpose, value) }
        fn depth_mask(&self, flag: bool) { (self.depth_mask)(flag) }
        fn depth_range(&self, near: f64, far: f64) { (self.depth_range)(near, far) }
        fn get_active_attrib(&self, program: GLuint, index: GLuint) -> (i32, u32, String) { (self.get_active_attrib)(program, index) }
        fn get_active_uniform(&self, program: GLuint, index: GLuint) -> (i32, u32, String) { (self.get_active_uniform)(program, index) }
        fn get_active_uniforms_iv(&self, program: GLuint, indices: Vec<GLuint>, pname: GLenum) -> Vec<GLint> { (self.get_active_uniforms_iv)(program, indices, pname) }
        fn get_active_uniform_block_i(&self, program: GLuint, index: GLuint, pname: GLenum) -> GLint { (self.get_active_uniform_block_i)(program, index, pname) }
        fn get_active_uniform_block_iv(&self, program: GLuint, index: GLuint, pname: GLenum) -> Vec<GLint> { (self.get_active_uniform_block_iv)(program, index, pname) }
        fn get_active_uniform_block_name(&self, program: GLuint, index: GLuint) -> String { (self.get_active_uniform_block_name)(program, index) }
        fn get_attrib_location(&self, program: GLuint, name: &str) -> c_int { (self.get_attrib_location)(program, name) }
        fn get_frag_data_location(&self, program: GLuint, name: &str) -> c_int { (self.get_frag_data_location)(program, name) }
        fn get_uniform_location(&self, program: GLuint, name: &str) -> c_int { (self.get_uniform_location)(program, name) }
        fn get_program_info_log(&self, program: GLuint) -> String { (self.get_program_info_log)(program) }
        unsafe fn get_program_iv(&self, program: GLuint, pname: GLenum, result: &mut [GLint]) { (self.fn)(program, pname, result) }
        fn get_program_binary(&self, program: GLuint) -> (Vec<u8>, GLenum) { (self.get_program_binary)(program) }
        fn program_binary(&self, program: GLuint, format: GLenum, binary: &[u8]) { (self.program_binary)(program, format, binary) }
        fn program_parameter_i(&self, program: GLuint, pname: GLenum, value: GLint) { (self.program_parameter_i)(program, pname, value) }
        unsafe fn get_vertex_attrib_iv(&self, index: GLuint, pname: GLenum, result: &mut [GLint]) { (self.fn)(index, pname, result) }
        unsafe fn get_vertex_attrib_fv(&self, index: GLuint, pname: GLenum, result: &mut [GLfloat]) { (self.fn)(index, pname, result) }
        fn get_vertex_attrib_pointer_v(&self, index: GLuint, pname: GLenum) -> GLsizeiptr { (self.get_vertex_attrib_pointer_v)(index, pname) }
        fn get_buffer_parameter_iv(&self, target: GLuint, pname: GLenum) -> GLint { (self.get_buffer_parameter_iv)(target, pname) }
        fn get_shader_info_log(&self, shader: GLuint) -> String { (self.get_shader_info_log)(shader) }
        fn get_string(&self, which: GLenum) -> String { (self.get_string)(which) }
        fn get_string_i(&self, which: GLenum, index: GLuint) -> String { (self.get_string_i)(which, index) }
        unsafe fn get_shader_iv(&self, shader: GLuint, pname: GLenum, result: &mut [GLint]) { (self.fn)(shader, pname, result) }
        fn get_shader_precision_format(&self, shader_type: GLuint, precision_type: GLuint) -> (GLint, GLint, GLint) { (self.get_shader_precision_format)(shader_type, precision_type) }
        fn compile_shader(&self, shader: GLuint) { (self.compile_shader)(shader) }
        fn delete_program(&self, program: GLuint) { (self.delete_program)(program) }
        fn create_shader(&self, shader_type: GLenum) -> GLuint { (self.create_shader)(shader_type) }
        fn delete_shader(&self, shader: GLuint) { (self.delete_shader)(shader) }
        fn detach_shader(&self, program: GLuint, shader: GLuint) { (self.detach_shader)(program, shader) }
        fn link_program(&self, program: GLuint) { (self.link_program)(program) }
        fn clear_color(&self, r: f32, g: f32, b: f32, a: f32) { (self.clear_color)(r, g, b, a) }
        fn clear(&self, buffer_mask: GLbitfield) { (self.clear)(buffer_mask) }
        fn clear_depth(&self, depth: f64) { (self.clear_depth)(depth) }
        fn clear_stencil(&self, s: GLint) { (self.clear_stencil)(s) }
        fn stencil_mask(&self, mask: GLuint) { (self.stencil_mask)(mask) }
        fn stencil_mask_separate(&self, face: GLenum, mask: GLuint) { (self.stencil_mask_separate)(face, mask) }
        fn stencil_func(&self, func: GLenum, ref_: GLint, mask: GLuint) { (self.stencil_func)(func, ref_, mask) }
        fn stencil_func_separate(&self, face: GLenum, func: GLenum, ref_: GLint, mask: GLuint) { (self.stencil_func_separate)(face, func, ref_, mask) }
        fn stencil_op(&self, sfail: GLenum, dpfail: GLenum, dppass: GLenum) { (self.stencil_op)(sfail, dpfail, dppass) }
        fn stencil_op_separate(&self, face: GLenum, sfail: GLenum, dpfail: GLenum, dppass: GLenum) { (self.stencil_op_separate)(face, sfail, dpfail, dppass) }
        fn egl_image_target_texture2d_oes(&self, target: GLenum, image: GLeglImageOES) { (self.egl_image_target_texture2d_oes)(target, image) }
        fn generate_mipmap(&self, target: GLenum) { (self.generate_mipmap)(target) }
        fn insert_event_marker_ext(&self, message: &str) { (self.insert_event_marker_ext)(message) }
        fn push_group_marker_ext(&self, message: &str) { (self.push_group_marker_ext)(message) }
        fn debug_message_insert_khr(&self, source: GLenum, type_: GLenum, id: GLuint, severity: GLenum, message: &str) { (self.debug_message_insert_khr)(source, type_, id, severity, message) }
        fn push_debug_group_khr(&self, source: GLenum, id: GLuint, message: &str) { (self.push_debug_group_khr)(source, id, message) }
        fn fence_sync(&self, condition: GLenum, flags: GLbitfield) -> GLsync { (self.fence_sync)(condition, flags) }
        fn client_wait_sync(&self, sync: GLsync, flags: GLbitfield, timeout: GLuint64) { (self.client_wait_sync)(sync, flags, timeout) }
        fn wait_sync(&self, sync: GLsync, flags: GLbitfield, timeout: GLuint64) { (self.wait_sync)(sync, flags, timeout) }
        fn delete_sync(&self, sync: GLsync) { (self.delete_sync)(sync) }
        fn texture_range_apple(&self, target: GLenum, data: &[u8]) { (self.texture_range_apple)(target, data) }
        fn gen_fences_apple(&self, n: GLsizei) -> Vec<GLuint> { (self.gen_fences_apple)(n) }
        fn delete_fences_apple(&self, fences: &[GLuint]) { (self.delete_fences_apple)(fences) }
        fn set_fence_apple(&self, fence: GLuint) { (self.set_fence_apple)(fence) }
        fn finish_fence_apple(&self, fence: GLuint) { (self.finish_fence_apple)(fence) }
        fn test_fence_apple(&self, fence: GLuint) { (self.test_fence_apple)(fence) }
        fn test_object_apple(&self, object: GLenum, name: GLuint) -> GLboolean { (self.test_object_apple)(object, name) }
        fn finish_object_apple(&self, object: GLenum, name: GLuint) { (self.finish_object_apple)(object, name) }
        fn get_frag_data_index( &self, program: GLuint, name: &str) -> GLint { (self.get_frag_data_index)(program, name) }
        fn bind_frag_data_location_indexed( &self, program: GLuint, color_number: GLuint, index: GLuint, name: &str) { (self.bind_frag_data_location_indexed)(program, color_number, index, name) }
        fn provoking_vertex_angle(&self, mode: GLenum) { (self.provoking_vertex_angle)(mode) }
    }

    impl Drop for OSMesaContext {
        fn drop(&mut self) {
            unsafe { osmesa_sys::OSMesaDestroyContext(self.context) }
        }
    }
}
