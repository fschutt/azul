//! `AzString` -> V string, and field accessors.
//!
//! V reaches every struct field directly (`pub mut:`), which is right for
//! plain data (`opts.window_state.size.dimensions = AzLogicalSize{...}`). A
//! field that owns heap memory (a String, a Vec, a class with `_delete`)
//! must not be overwritten in place - the old value would leak - nor copied
//! out by assignment - both copies would free the same buffer. Those fields
//! get methods that follow the field-access contract (see
//! `raw_field_access`):
//!
//! - `x.get_<field>()`: a V `string` (copied, the field not consumed) or a
//!   deep copy through `Az<T>_clone`;
//! - `x.set_<field>(v)`: releases the old value with `Az<T>_delete`, then
//!   moves `v` in (a V `string` becomes a fresh AzString).
//!
//! A `mut` receiver is the field's own memory, so nested writes reach the
//! parent: `opts.window_state.set_title('Hello')`. The binding has no
//! api.json method wrappers, so nothing else claims these names.

use super::{
    super::{
        config::CodegenConfig,
        generator::CodeBuilder,
        ir::{CodegenIR, FunctionDef, TypeCategory},
        raw_field_access::{accessible_fields, RawFieldKind},
    },
    ffi_type_name, field_type_for_ref_kind, include_struct, sanitize_identifier,
    should_emit_function,
};

/// `az_string_to_v(&s)`: the text of an AzString, copied, not consumed.
pub fn generate_string_helpers(b: &mut CodeBuilder, ir: &CodegenIR) {
    let Some(class) = ir.structs.iter().find(|s| s.category == TypeCategory::String) else {
        return;
    };
    let st = ffi_type_name(&class.name);
    b.line("// The text of `s`, copied; `s` is NOT consumed (it still owns and frees its");
    b.line("// buffer).");
    b.line(&format!("pub fn az_string_to_v(s &{}) string {{", st));
    b.line("\tif s.vec.len == 0 || isnil(s.vec.ptr) {");
    b.line("\t\treturn ''");
    b.line("\t}");
    b.line("\treturn unsafe { tos(s.vec.ptr, int(s.vec.len)).clone() }");
    b.line("}");
    b.blank();
}

pub fn generate_field_accessors(b: &mut CodeBuilder, ir: &CodegenIR, config: &CodegenConfig) {
    b.line("// ----------------------------------------------------------------------------");
    b.line("// Field accessors for heap-owning fields (strings, vecs, classes with a");
    b.line("// _delete). Plain-data fields are read and written directly.");
    b.line("//   x.get_<field>()   an independent value (copied string / deep copy)");
    b.line("//   x.set_<field>(v)  releases the old value, then moves `v` in");
    b.line("// Nested: `opts.window_state.set_title('Hello')`. Assigning such a field");
    b.line("// directly (`opts.window_state.title = ...`) leaks the old value.");
    b.line("// ----------------------------------------------------------------------------");
    b.blank();

    let has_strings = ir.structs.iter().any(|s| s.category == TypeCategory::String);
    let emitted = |f: &FunctionDef| should_emit_function(f, ir, config);
    for s in &ir.structs {
        if !include_struct(s, config) {
            continue;
        }
        let st = ffi_type_name(&s.name);
        for f in accessible_fields(s, ir, config, &emitted) {
            let fty = field_type_for_ref_kind(&f.def.type_name, &f.def.ref_kind, ir);
            let field = format!("self.{}", sanitize_identifier(&f.def.name));
            let name = &f.def.name;
            match &f.kind {
                RawFieldKind::Str { delete } if has_strings => {
                    b.line(&format!("// A copy of the `{}` field (the field is not consumed).", name));
                    b.line(&format!("pub fn (self &{}) get_{}() string {{", st, name));
                    b.line(&format!("\treturn az_string_to_v(&{})", field));
                    b.line("}");
                    b.blank();
                    b.line(&format!("// Replace `{}`: releases the old string, stores a fresh copy of `v`.", name));
                    b.line(&format!("pub fn (mut self {}) set_{}(v string) {{", st, name));
                    b.line("\tfresh := az_str(v)");
                    b.line(&format!("\tC.{}(&{})", delete, field));
                    b.line(&format!("\t{} = fresh", field));
                    b.line("}");
                    b.blank();
                }
                RawFieldKind::Heap { delete, clone } => {
                    if let Some(clone) = clone {
                        b.line(&format!("// A deep copy of the `{}` field (the caller owns it).", name));
                        b.line(&format!("pub fn (self &{}) get_{}() {} {{", st, name, fty));
                        b.line(&format!("\treturn C.{}(&{})", clone, field));
                        b.line("}");
                        b.blank();
                    }
                    b.line(&format!("// Replace `{}`: releases the old value, then takes `v` over.", name));
                    b.line(&format!("pub fn (mut self {}) set_{}(v {}) {{", st, name, fty));
                    b.line(&format!("\tC.{}(&{})", delete, field));
                    b.line(&format!("\t{} = v", field));
                    b.line("}");
                    b.blank();
                }
                _ => {}
            }
        }
    }
}
