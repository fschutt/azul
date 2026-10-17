//! Native string helpers and field accessors.
//!
//! Odin reaches every struct field directly, which is right for plain data
//! (`opts.window_state.size.dimensions = {400, 300}`). A field that owns
//! heap memory (a String, a Vec, a class with `_delete`) must not be
//! overwritten in place - the old value would leak - nor copied out by
//! assignment - both copies would free the same buffer. Those fields get
//! procs that follow the field-access contract (see `raw_field_access`):
//!
//! - `<Class>_get_<field>(&x)`: an Odin `string` (decoded into `allocator`,
//!   the field not consumed) or a deep copy through `Az<T>_clone`;
//! - `<Class>_set_<field>(&x, v)`: releases the old value with
//!   `Az<T>_delete`, then moves `v` in (an Odin `string` becomes a fresh
//!   AzString).
//!
//! The receiver is a pointer, so nested writes reach the parent:
//! `azul.FullWindowState_set_title(&opts.window_state, "Hello")`.

use std::collections::BTreeSet;

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

/// The C symbol of the String class's byte constructor, if the IR has one.
fn string_from_bytes(ir: &CodegenIR) -> Option<(String, String)> {
    let class = ir
        .structs
        .iter()
        .find(|s| s.category == TypeCategory::String)?;
    // The IR has no `FunctionKind` for it - it is an ordinary api.json
    // constructor - so the method name is the only handle there is.
    // allow-api-name: no kind or shape distinguishes the byte constructor.
    let method = "copy_from_bytes";
    ir.functions
        .iter()
        .find(|f| f.class_name == class.name && f.method_name == method)
        .map(|f| (ffi_type_name(&class.name), f.c_name.clone()))
}

/// `az_string(s)`: a fresh AzString owning a copy of `s` ("" included);
/// `az_string_to_odin(s)`: the text of an AzString, copied, not consumed.
pub fn generate_string_helpers(b: &mut CodeBuilder, ir: &CodegenIR) {
    let Some((st, from_bytes)) = string_from_bytes(ir) else {
        return;
    };
    b.line("// ----------------------------------------------------------------------------");
    b.line("// Native strings.");
    b.line("// ----------------------------------------------------------------------------");
    b.line("// A fresh AzString owning a copy of `s` (libazul copies the bytes; \"\" is fine).");
    b.line(&format!("az_string :: proc(s: string) -> {} {{", st));
    b.line("\tif len(s) == 0 {");
    b.line("\t\tempty: u8 = 0");
    b.line(&format!("\t\treturn {}(([^]u8)(&empty), 0, 0)", from_bytes));
    b.line("\t}");
    b.line(&format!("\treturn {}(raw_data(s), 0, uint(len(s)))", from_bytes));
    b.line("}");
    b.blank();
    b.line("// The text of `s`, copied into `allocator`; `s` is NOT consumed (it still owns");
    b.line("// and frees its buffer).");
    b.line(&format!(
        "az_string_to_odin :: proc(s: {}, allocator := context.allocator) -> string {{",
        st
    ));
    b.line("\tn := int(s.vec.len)");
    b.line("\tif n == 0 || s.vec.ptr == nil {");
    b.line("\t\treturn \"\"");
    b.line("\t}");
    b.line("\tbuf := make([]u8, n, allocator)");
    b.line("\tcopy(buf, s.vec.ptr[:n])");
    b.line("\treturn string(buf)");
    b.line("}");
    b.blank();
}

pub fn generate_field_accessors(b: &mut CodeBuilder, ir: &CodegenIR, config: &CodegenConfig) {
    b.line("// ----------------------------------------------------------------------------");
    b.line("// Field accessors for heap-owning fields (strings, vecs, classes with a");
    b.line("// _delete). Plain-data fields are read and written directly.");
    b.line("//   <Class>_get_<field>(&x)     an independent value (decoded / deep copy)");
    b.line("//   <Class>_set_<field>(&x, v)  releases the old value, then moves `v` in");
    b.line("// Nested: FullWindowState_set_title(&opts.window_state, \"Hello\"). Assigning");
    b.line("// such a field directly (`opts.window_state.title = ...`) leaks the old value.");
    b.line("// ----------------------------------------------------------------------------");
    b.blank();

    let emitted = |f: &FunctionDef| should_emit_function(f, ir, config);
    // Names an api.json alias (`<Class>_<method>`) already owns: the alias
    // wins, the accessor gets a `_field` suffix.
    let mut taken: BTreeSet<String> = ir
        .functions
        .iter()
        .filter(|f| should_emit_function(f, ir, config))
        .filter_map(|f| f.c_name.strip_prefix("Az").map(str::to_string))
        .collect();
    let has_strings = string_from_bytes(ir).is_some();

    for s in &ir.structs {
        if !include_struct(s, config) {
            continue;
        }
        let st = ffi_type_name(&s.name);
        for f in accessible_fields(s, ir, config, &emitted) {
            let delete = match &f.kind {
                RawFieldKind::Str { delete } if has_strings => delete.clone(),
                RawFieldKind::Heap { delete, .. } => delete.clone(),
                _ => continue,
            };
            let fty = field_type_for_ref_kind(&f.def.type_name, &f.def.ref_kind, ir);
            let field = format!("self.{}", sanitize_identifier(&f.def.name));
            let mut name_for = |verb: &str| {
                let plain = format!("{}_{}_{}", s.name, verb, f.def.name);
                let name = if taken.contains(&plain) {
                    format!("{}_field", plain)
                } else {
                    plain
                };
                taken.insert(name.clone());
                name
            };

            match &f.kind {
                RawFieldKind::Str { .. } => {
                    let get = name_for("get");
                    b.line(&format!("// A copy of the `{}` field (the field is not consumed).", f.def.name));
                    b.line(&format!(
                        "{} :: proc(self: ^{}, allocator := context.allocator) -> string {{",
                        get, st
                    ));
                    b.line(&format!("\treturn az_string_to_odin({}, allocator)", field));
                    b.line("}");
                    let set = name_for("set");
                    b.line(&format!("// Replace `{}`: releases the old string, stores a fresh copy of `v`.", f.def.name));
                    b.line(&format!("{} :: proc(self: ^{}, v: string) {{", set, st));
                    b.line("\tfresh := az_string(v)");
                    b.line(&format!("\t{}(&{})", delete, field));
                    b.line(&format!("\t{} = fresh", field));
                    b.line("}");
                }
                RawFieldKind::Heap { clone, .. } => {
                    if let Some(clone) = clone {
                        let get = name_for("get");
                        b.line(&format!("// A deep copy of the `{}` field (the caller owns it).", f.def.name));
                        b.line(&format!("{} :: proc(self: ^{}) -> {} {{", get, st, fty));
                        b.line(&format!("\treturn {}(&{})", clone, field));
                        b.line("}");
                    }
                    let set = name_for("set");
                    b.line(&format!("// Replace `{}`: releases the old value, then takes `v` over.", f.def.name));
                    b.line(&format!("{} :: proc(self: ^{}, v: {}) {{", set, st, fty));
                    b.line(&format!("\t{}(&{})", delete, field));
                    b.line(&format!("\t{} = v", field));
                    b.line("}");
                }
                _ => {}
            }
        }
    }
    b.blank();
}
