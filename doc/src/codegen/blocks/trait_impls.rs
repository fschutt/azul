//! Trait implementation generation block
//!
//! Generates trait implementations for special types like VecDestructor, VecRef, etc.
//! These are types that can't use #[derive] because their field types don't implement
//! the required traits.

use super::config::CodegenConfig;
use crate::codegen::struct_gen::StructMetadata;
use std::collections::BTreeMap;

/// Generate trait implementations for VecDestructor types
/// (Debug, PartialEq, PartialOrd, Eq, Ord, Hash)
pub fn generate_vec_destructor_impls(
    prefixed_name: &str,
    config: &CodegenConfig,
) -> String {
    let indent = config.indent(1);
    
    format!(r#"
{i}impl core::fmt::Debug for {name} {{
{i}    fn fmt(&self, f: &mut core::fmt::Formatter) -> core::fmt::Result {{
{i}        match self {{
{i}            {name}::DefaultRust => write!(f, "{name}::DefaultRust"),
{i}            {name}::NoDestructor => write!(f, "{name}::NoDestructor"),
{i}            {name}::External(fn_ptr) => write!(f, "{name}::External({{:p}})", *fn_ptr as *const ()),
{i}        }}
{i}    }}
{i}}}

{i}impl PartialEq for {name} {{
{i}    fn eq(&self, other: &Self) -> bool {{
{i}        match (self, other) {{
{i}            ({name}::DefaultRust, {name}::DefaultRust) => true,
{i}            ({name}::NoDestructor, {name}::NoDestructor) => true,
{i}            ({name}::External(a), {name}::External(b)) => (*a as usize) == (*b as usize),
{i}            _ => false,
{i}        }}
{i}    }}
{i}}}

{i}impl PartialOrd for {name} {{
{i}    fn partial_cmp(&self, other: &Self) -> Option<core::cmp::Ordering> {{
{i}        Some(self.cmp(other))
{i}    }}
{i}}}

{i}impl Eq for {name} {{ }}

{i}impl Ord for {name} {{
{i}    fn cmp(&self, other: &Self) -> core::cmp::Ordering {{
{i}        let self_ord = match self {{
{i}            {name}::DefaultRust => 0usize,
{i}            {name}::NoDestructor => 1usize,
{i}            {name}::External(f) => 2usize + (*f as usize),
{i}        }};
{i}        let other_ord = match other {{
{i}            {name}::DefaultRust => 0usize,
{i}            {name}::NoDestructor => 1usize,
{i}            {name}::External(f) => 2usize + (*f as usize),
{i}        }};
{i}        self_ord.cmp(&other_ord)
{i}    }}
{i}}}

{i}impl core::hash::Hash for {name} {{
{i}    fn hash<H: core::hash::Hasher>(&self, state: &mut H) {{
{i}        match self {{
{i}            {name}::DefaultRust => 0usize.hash(state),
{i}            {name}::NoDestructor => 1usize.hash(state),
{i}            {name}::External(f) => (2usize + (*f as usize)).hash(state),
{i}        }}
{i}    }}
{i}}}

"#, i = indent, name = prefixed_name)
}

/// Generate VecRef slice methods and From implementations
pub fn generate_vec_ref_impls(
    prefixed_name: &str,
    element_type: &str,
    is_mut: bool,
    prefix: &str,
    config: &CodegenConfig,
) -> String {
    let indent = config.indent(1);
    let unprefixed_name = prefixed_name.strip_prefix(prefix).unwrap_or(prefixed_name);
    
    // Determine the element type with prefix if it's a custom type
    let prefixed_element = if super::config::is_primitive_type(element_type) {
        element_type.to_string()
    } else {
        format!("{}{}", prefix, element_type)
    };
    
    let mut code = String::new();
    
    if is_mut {
        // Mutable VecRef: as_slice and as_mut_slice
        code.push_str(&format!(r#"
{i}impl {name} {{
{i}    pub fn as_slice(&self) -> &[{elem}] {{
{i}        unsafe {{ core::slice::from_raw_parts(self.ptr, self.len) }}
{i}    }}
{i}    pub fn as_mut_slice(&mut self) -> &mut [{elem}] {{
{i}        unsafe {{ core::slice::from_raw_parts_mut(self.ptr, self.len) }}
{i}    }}
{i}}}

{i}impl<'a> From<&'a mut [{elem}]> for {name} {{
{i}    fn from(s: &'a mut [{elem}]) -> Self {{
{i}        Self {{ ptr: s.as_mut_ptr(), len: s.len() }}
{i}    }}
{i}}}

"#, i = indent, name = prefixed_name, elem = prefixed_element));
    } else {
        // Immutable VecRef: only as_slice
        // Special case for Refstr (which is &str, not a slice)
        if unprefixed_name == "Refstr" {
            code.push_str(&format!(r#"
{i}impl {name} {{
{i}    pub fn as_str(&self) -> &str {{
{i}        unsafe {{ core::str::from_utf8_unchecked(core::slice::from_raw_parts(self.ptr, self.len)) }}
{i}    }}
{i}}}

{i}impl<'a> From<&'a str> for {name} {{
{i}    fn from(s: &'a str) -> Self {{
{i}        Self {{ ptr: s.as_ptr(), len: s.len() }}
{i}    }}
{i}}}

"#, i = indent, name = prefixed_name));
        } else {
            code.push_str(&format!(r#"
{i}impl {name} {{
{i}    pub fn as_slice(&self) -> &[{elem}] {{
{i}        unsafe {{ core::slice::from_raw_parts(self.ptr, self.len) }}
{i}    }}
{i}}}

{i}impl<'a> From<&'a [{elem}]> for {name} {{
{i}    fn from(s: &'a [{elem}]) -> Self {{
{i}        Self {{ ptr: s.as_ptr(), len: s.len() }}
{i}    }}
{i}}}

"#, i = indent, name = prefixed_name, elem = prefixed_element));
        }
    }
    
    code
}

/// Generate trait implementations for VecRef types
/// (Debug, Clone, Copy, PartialEq, PartialOrd, Eq, Ord, Hash based on element type)
pub fn generate_vec_ref_trait_impls(
    prefixed_name: &str,
    element_type: &str,
    prefix: &str,
    config: &CodegenConfig,
) -> String {
    let indent = config.indent(1);
    let unprefixed_name = prefixed_name.strip_prefix(prefix).unwrap_or(prefixed_name);
    
    // Refstr uses as_str() instead of as_slice()
    let slice_method = if unprefixed_name == "Refstr" { "as_str" } else { "as_slice" };
    
    // Check if element type supports Ord/Hash (f32/f64 don't)
    let supports_ord_hash = element_type != "f32" && element_type != "f64";
    
    let mut code = String::new();
    
    // Debug implementation
    code.push_str(&format!(r#"
{i}impl core::fmt::Debug for {name} {{
{i}    fn fmt(&self, f: &mut core::fmt::Formatter) -> core::fmt::Result {{
{i}        self.{method}().fmt(f)
{i}    }}
{i}}}

{i}impl Clone for {name} {{
{i}    fn clone(&self) -> Self {{
{i}        Self {{ ptr: self.ptr, len: self.len }}
{i}    }}
{i}}}

{i}impl Copy for {name} {{}}

{i}impl PartialEq for {name} {{
{i}    fn eq(&self, other: &Self) -> bool {{
{i}        self.{method}() == other.{method}()
{i}    }}
{i}}}

{i}impl PartialOrd for {name} {{
{i}    fn partial_cmp(&self, other: &Self) -> Option<core::cmp::Ordering> {{
{i}        self.{method}().partial_cmp(other.{method}())
{i}    }}
{i}}}

"#, i = indent, name = prefixed_name, method = slice_method));
    
    // Eq, Ord, Hash only for types that support it (not f32/f64)
    if supports_ord_hash {
        code.push_str(&format!(r#"
{i}impl Eq for {name} {{}}

{i}impl Ord for {name} {{
{i}    fn cmp(&self, other: &Self) -> core::cmp::Ordering {{
{i}        self.{method}().cmp(other.{method}())
{i}    }}
{i}}}

{i}impl core::hash::Hash for {name} {{
{i}    fn hash<H: core::hash::Hasher>(&self, state: &mut H) {{
{i}        self.{method}().hash(state)
{i}    }}
{i}}}

"#, i = indent, name = prefixed_name, method = slice_method));
    }
    
    code
}
