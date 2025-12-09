//! Primitive type aliases generation
//!
//! Generates type aliases for primitives used in generic instantiations.

use super::config::CodegenConfig;

/// Generate primitive type aliases for the FFI layer
pub fn generate_primitive_type_aliases(config: &CodegenConfig) -> String {
    let indent = config.indent(1);
    let prefix = &config.prefix;
    
    format!(r#"
{i}// ===== Primitive Type Aliases (for generic instantiations) =====
{i}pub type {p}I32 = i32;
{i}pub type {p}U32 = u32;
{i}pub type {p}F32 = f32;
{i}pub type {p}Usize = usize;
{i}pub type {p}C_void = c_void;
{i}// Non-prefixed aliases for primitive types
{i}pub type Usize = usize;
{i}pub type U8 = u8;
{i}pub type I16 = i16;
{i}pub type Char = char;

"#, i = indent, p = prefix)
}
