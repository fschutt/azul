//! Native string helpers and field accessors.
//!
//! Nim reaches every struct field directly (`opts.window_state.size.dimensions
//! = ...`), which is right for plain data. A field that owns heap memory (a
//! String, a Vec, a class with `_delete`) must not be overwritten in place -
//! the old value would leak - nor copied out by assignment - both copies
//! would free the same buffer. Those fields get procs that follow the
//! field-access contract (see `raw_field_access`):
//!
//! - `get<Field>(self)`: a native `string` (decoded, the field is not
//!   consumed) or a deep copy through `Az<T>_clone`;
//! - `set<Field>(self, v)`: releases the old value with `Az<T>_delete`, then
//!   moves `v` in (a native `string` becomes a fresh AzString).
//!
//! Nested writes work because a field of a `var` object is itself an
//! l-value: `opts.window_state.setTitle("Hello")`. When an api.json method
//! already owns the name for that type (`TextInputState.getText`), the
//! accessor is spelled `get<Field>Field` / `set<Field>Field` instead.

use super::{
    super::{
        config::CodegenConfig,
        generator::CodeBuilder,
        ir::{CodegenIR, FunctionDef, TypeCategory},
        raw_field_access::{accessible_fields, RawFieldKind},
    },
    ffi_type_name, functions::should_emit_function, sanitize_identifier, to_pascal_case,
    types::field_type_for_ref_kind, ProcDedup,
};

/// The C symbol of a String-class function found by its api.json method
/// name.
fn string_fn(ir: &CodegenIR, method: &str) -> Option<String> {
    let class = ir
        .structs
        .iter()
        .find(|s| s.category == TypeCategory::String)?;
    ir.functions
        .iter()
        .find(|f| f.class_name == class.name && f.method_name == method)
        .map(|f| f.c_name.clone())
}

/// `azString(s)`: a fresh AzString owning a copy of `s` (an empty `s` is
/// never indexed); `$s`: the text of an AzString, copied, the AzString not
/// consumed; `tr(key)`: a translation key, owning a copy of `key`.
pub fn generate_string_helpers(b: &mut CodeBuilder, ir: &CodegenIR, procs: &mut ProcDedup) {
    let Some(string_class) = ir.structs.iter().find(|s| s.category == TypeCategory::String)
    else {
        return;
    };
    let st = ffi_type_name(&string_class.name);
    // The IR has no `FunctionKind` for these two - they are ordinary
    // api.json functions - so the method name is the only handle there is.
    // allow-api-name: no kind or shape distinguishes the byte constructor.
    let from_bytes = string_fn(ir, "copy_from_bytes");
    // allow-api-name: no kind or shape distinguishes the translation-key constructor.
    let tr = string_fn(ir, "tr");
    let Some(from_bytes) = from_bytes else {
        return;
    };
    let from_bytes = procs.external_name(&from_bytes);

    b.line("# ============================================================================");
    b.line("# Native strings. `azString` copies a Nim string into a fresh AzString (the");
    b.line("# caller owns it); `$` copies an AzString's text out WITHOUT consuming it.");
    b.line("# ============================================================================");
    b.blank();
    let name = procs.unique("azString", "string");
    b.line(&format!("proc {}*(s: string): {} =", name, st));
    b.line("  ## A fresh AzString owning a copy of `s` (libazul copies the bytes).");
    b.line("  var empty: uint8 = 0");
    b.line("  if s.len == 0:");
    b.line(&format!("    {}(addr empty, csize_t(0), csize_t(0))", from_bytes));
    b.line("  else:");
    b.line(&format!(
        "    {}(cast[ptr uint8](unsafeAddr s[0]), csize_t(0), csize_t(s.len))",
        from_bytes
    ));
    b.blank();
    b.line(&format!("proc `$`*(s: {}): string =", st));
    b.line("  ## The UTF-8 text of `s`, copied; `s` still owns (and frees) its buffer.");
    b.line("  result = newString(int(s.vec.len))");
    b.line("  if s.vec.len > 0:");
    b.line("    copyMem(addr result[0], s.vec.`ptr`, int(s.vec.len))");
    b.blank();
    if let Some(tr) = tr {
        let tr = procs.external_name(&tr);
        b.line(&format!("proc tr*(key: string): {} =", st));
        b.line("  ## A translation KEY (marked localizable): the layout pass replaces it");
        b.line("  ## with its translation. Owns a copy of `key`.");
        b.line(&format!("  {}(azString(key))", tr));
        b.blank();
    }
}

pub fn generate_field_accessors(
    b: &mut CodeBuilder,
    ir: &CodegenIR,
    config: &CodegenConfig,
    procs: &mut ProcDedup,
) {
    b.line("# ============================================================================");
    b.line("# Field accessors for heap-owning fields (strings, vecs, classes with a");
    b.line("# _delete). Plain-data fields are read and written directly.");
    b.line("#   get<Field>(x)     an independent value (string decoded / deep copy)");
    b.line("#   set<Field>(x, v)  releases the old value, then moves `v` in");
    b.line("# Nested: `opts.window_state.setTitle(\"Hello\")`. Assigning such a field");
    b.line("# directly (`opts.window_state.title = ...`) leaks the old value.");
    b.line("# ============================================================================");
    b.blank();

    let emitted = |f: &FunctionDef| should_emit_function(f, ir, config);
    let has_string_helpers = ir
        .structs
        .iter()
        .any(|s| s.category == TypeCategory::String)
        // allow-api-name: the byte constructor `azString` is built on.
        && string_fn(ir, "copy_from_bytes").is_some();
    for s in &ir.structs {
        let fields = accessible_fields(s, ir, config, &emitted);
        let st = ffi_type_name(&s.name);
        for f in &fields {
            if matches!(f.kind, RawFieldKind::Prim { .. } | RawFieldKind::Pod) {
                continue;
            }
            if matches!(f.kind, RawFieldKind::Str { .. }) && !has_string_helpers {
                continue;
            }
            let fty = field_type_for_ref_kind(&f.def.type_name, &f.def.ref_kind, ir);
            let field = format!("self.{}", sanitize_identifier(&f.def.name));
            let pascal = to_pascal_case(&f.def.name);
            // The api.json method wins: fall back to `<verb><Field>Field`.
            let pick = |procs: &ProcDedup, verb: &str| {
                let plain = format!("{}{}", verb, pascal);
                if procs.has_receiver(&plain, &st) {
                    format!("{}{}Field", verb, pascal)
                } else {
                    plain
                }
            };

            // Getter.
            let getter_body = match &f.kind {
                RawFieldKind::Str { .. } => Some(format!("${}", field)),
                RawFieldKind::Heap {
                    clone: Some(clone), ..
                } => Some(format!(
                    "{}(unsafeAddr {})",
                    procs.external_name(clone),
                    field
                )),
                _ => None,
            };
            if let Some(body) = getter_body {
                let name = pick(procs, "get");
                if !procs.has_receiver(&name, &st) {
                    let name = procs.unique(&name, &st);
                    let ret = if matches!(f.kind, RawFieldKind::Str { .. }) {
                        "string".to_string()
                    } else {
                        fty.clone()
                    };
                    b.line(&format!(
                        "proc {}*(self: {}): {} {{.inline.}} = {}",
                        name, st, ret, body
                    ));
                }
            }

            // Setter(s): a moved value of the field's own type, and for a
            // string field a native `string` too.
            let delete = match &f.kind {
                RawFieldKind::Str { delete } | RawFieldKind::Heap { delete, .. } => {
                    procs.external_name(delete)
                }
                _ => continue,
            };
            let name = pick(procs, "set");
            if matches!(f.kind, RawFieldKind::Str { .. }) {
                let n = procs.unique(&name, &format!("var {},string", st));
                b.line(&format!("proc {}*(self: var {}, v: string) =", n, st));
                b.line("  let fresh = azString(v)");
                b.line(&format!("  {}(addr {})", delete, field));
                b.line(&format!("  {} = fresh", field));
            }
            let n = procs.unique(&name, &format!("var {},{}", st, fty));
            b.line(&format!("proc {}*(self: var {}, v: {}) =", n, st, fty));
            b.line(&format!("  {}(addr {})", delete, field));
            b.line(&format!("  {} = v", field));
        }
    }
    b.blank();
}
