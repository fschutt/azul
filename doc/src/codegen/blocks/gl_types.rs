//! OpenGL type aliases generation
//!
//! Generates the GL type aliases needed for OpenGL interop.

use super::config::CodegenConfig;

/// Generate OpenGL type aliases for the FFI layer
pub fn generate_gl_type_aliases(config: &CodegenConfig) -> String {
    let indent = config.indent(1);
    
    format!(r#"
{i}// ===== GL Type Aliases =====
{i}pub type GLenum = u32;
{i}pub type GLboolean = u8;
{i}pub type GLbitfield = u32;
{i}pub type GLvoid = c_void;
{i}pub type GLbyte = i8;
{i}pub type GLshort = i16;
{i}pub type GLint = i32;
{i}pub type GLclampx = i32;
{i}pub type GLubyte = u8;
{i}pub type GLushort = u16;
{i}pub type GLuint = u32;
{i}pub type GLsizei = i32;
{i}pub type GLfloat = f32;
{i}pub type GLclampf = f32;
{i}pub type GLdouble = f64;
{i}pub type GLclampd = f64;
{i}pub type GLeglImageOES = *const c_void;
{i}pub type GLchar = i8;
{i}pub type GLcharARB = i8;
{i}pub type GLhandleARB = u32;
{i}pub type GLhalfARB = u16;
{i}pub type GLhalf = u16;
{i}pub type GLfixed = i32;
{i}pub type GLintptr = isize;
{i}pub type GLsizeiptr = isize;
{i}pub type GLint64 = i64;
{i}pub type GLuint64 = u64;
{i}pub type GLintptrARB = isize;
{i}pub type GLsizeiptrARB = isize;
{i}pub type GLint64EXT = i64;
{i}pub type GLuint64EXT = u64;
{i}pub type GLhalfNV = u16;
{i}pub type GLvdpauSurfaceNV = isize;

"#, i = indent)
}
