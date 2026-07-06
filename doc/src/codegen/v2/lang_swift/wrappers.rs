//! Swift idiomatic wrapper layer (extensions on the imported C types).
//!
//! The flat `public let App_create = AzApp_create` aliases (see
//! [`super::functions`]) drop the `Az` prefix but keep the raw C call
//! shape (`App_create(&dom, child)`). This module adds the *idiomatic*
//! surface the Zig backend emits as `pub const App = struct { ... }`:
//! methods hang off the type itself, so a caller writes
//!
//! ```swift
//! var dom = AzDom.createDiv()
//! dom.addChild(label)          // instance method, mutates in place
//! let s = AzString("hello")    // typed String interop
//! print(s.string)              // AzString -> Swift String
//! ```
//!
//! Because Swift imports every `AzFoo` with its authoritative C layout,
//! we can `extension AzFoo { ... }` and thread `self` straight into the
//! C ABI with no re-declaration of the record. Instance methods whose C
//! symbol takes `self` by pointer (`AzFoo*`) are `mutating func`s that
//! pass `&self`; methods that take `self` by value pass `self`.
//!
//! Mirrors the Zig `emit_struct_wrapper` self-by-value / self-by-pointer
//! detection (args[0] ref kind) so the Swift signature matches what the
//! C header declares.

use std::collections::HashSet;

use super::super::config::CodegenConfig;
use super::super::generator::CodeBuilder;
use super::super::ir::{ArgRefKind, CodegenIR, FunctionArg, FunctionDef, FunctionKind};
use super::{sanitize_identifier, should_emit_function};

pub fn generate_wrappers(b: &mut CodeBuilder, ir: &CodegenIR, config: &CodegenConfig) {
    b.line("// ----------------------------------------------------------------------------");
    b.line("// Idiomatic wrapper layer: methods hang off the imported `AzFoo` types via");
    b.line("// Swift extensions, so `dom.addChild(child)` / `AzDom.createDiv()` work. The");
    b.line("// raw `Az*` C symbols and the flat aliases above remain available.");
    b.line("// ----------------------------------------------------------------------------");
    b.blank();

    // Every class that owns at least one emittable function gets an extension.
    // Collect the class names in a stable order (structs then enums, as they
    // appear in the IR) and de-duplicate.
    let mut seen_class: HashSet<String> = HashSet::new();
    let mut classes: Vec<String> = Vec::new();
    for s in &ir.structs {
        if seen_class.insert(s.name.clone()) {
            classes.push(s.name.clone());
        }
    }
    for e in &ir.enums {
        if seen_class.insert(e.name.clone()) {
            classes.push(e.name.clone());
        }
    }

    for class in classes {
        emit_class_extension(b, ir, config, &class);
    }
}

fn emit_class_extension(b: &mut CodeBuilder, ir: &CodegenIR, config: &CodegenConfig, class: &str) {
    let funcs: Vec<&FunctionDef> = ir
        .functions_for_class(class)
        .filter(|f| should_emit_function(f, ir, config))
        .filter(|f| {
            matches!(
                f.kind,
                FunctionKind::Constructor
                    | FunctionKind::StaticMethod
                    | FunctionKind::Default
                    | FunctionKind::Method
                    | FunctionKind::MethodMut
                    | FunctionKind::DeepCopy
            )
        })
        // Skip functions with a callback-typed argument: the canonical C
        // symbol takes the bare fn-pointer typedef (or the `...Struct`
        // variant), which does not match the wrapper-struct type the IR
        // arg carries. Those stay reachable via the raw `Az*` symbol /
        // flat alias, and callbacks are passed C-direct anyway.
        .filter(|f| f.args.iter().all(|a| a.callback_info.is_none()))
        .collect();

    let is_string = class == "String";
    if funcs.is_empty() && !is_string {
        return;
    }

    let ffi_type = format!("Az{}", class.strip_prefix("Az").unwrap_or(class));
    b.line(&format!("public extension {} {{", ffi_type));
    b.indent();

    // AzString gets the typed Swift String interop the SHIPPED bar asks for.
    if is_string {
        emit_string_interop(b);
    }

    // Swift permits overloading by signature, but two members with the same
    // full name+params error. De-dup by idiomatic method name (matching the
    // Zig backend's `seen` set).
    let mut seen: HashSet<String> = HashSet::new();

    // Static factories first (constructors / static methods / default).
    for f in &funcs {
        if !matches!(
            f.kind,
            FunctionKind::Constructor | FunctionKind::StaticMethod | FunctionKind::Default
        ) {
            continue;
        }
        let name = sanitize_identifier(&idiomatic_method_name(&f.method_name));
        if !seen.insert(name.clone()) {
            b.line(&format!(
                "// SKIPPED static `{}` — duplicate idiomatic name (raw {} still callable).",
                name, f.c_name
            ));
            continue;
        }
        emit_static_factory(b, f, class, &ffi_type, &name);
    }

    // Instance methods.
    for f in &funcs {
        if !matches!(
            f.kind,
            FunctionKind::Method | FunctionKind::MethodMut | FunctionKind::DeepCopy
        ) {
            continue;
        }
        let label = if matches!(f.kind, FunctionKind::DeepCopy) {
            "clone".to_string()
        } else {
            idiomatic_method_name(&f.method_name)
        };
        let name = sanitize_identifier(&label);
        if !seen.insert(name.clone()) {
            b.line(&format!(
                "// SKIPPED method `{}` — duplicate idiomatic name (raw {} still callable).",
                name, f.c_name
            ));
            continue;
        }
        emit_instance_method(b, f, class, &ffi_type, &name);
    }

    b.dedent();
    b.line("}");
    b.blank();
}

/// Typed `AzString` <-> Swift `String` interop.
fn emit_string_interop(b: &mut CodeBuilder) {
    b.line("/// Build an AzString from a Swift String (copies the UTF-8 bytes into a");
    b.line("/// refcounted native buffer, so the source String may be a temporary).");
    b.line("init(_ s: String) {");
    b.indent();
    b.line("let bytes = Array(s.utf8)");
    b.line("self = bytes.withUnsafeBufferPointer { AzString_fromUtf8($0.baseAddress, $0.count) }");
    b.dedent();
    b.line("}");
    b.blank();
    b.line("/// Decode the wrapped UTF-8 bytes into a Swift String.");
    b.line("var string: String {");
    b.indent();
    b.line("guard let p = vec.ptr, vec.len > 0 else { return \"\" }");
    b.line("return String(decoding: UnsafeBufferPointer(start: p, count: vec.len), as: UTF8.self)");
    b.dedent();
    b.line("}");
    b.blank();
}

fn emit_static_factory(
    b: &mut CodeBuilder,
    f: &FunctionDef,
    class: &str,
    ffi_type: &str,
    name: &str,
) {
    for d in &f.doc {
        b.line(&format!("/// {}", d.replace('\n', " ")));
    }
    let params = format_params(&f.args, /* skip_self */ false);
    let call = format_call_args(&f.args, /* skip_self */ false, /* self_expr */ None);
    let ret = return_clause(f, class, ffi_type);
    b.line(&format!("static func {}({}){} {{", name, params, ret.0));
    b.indent();
    b.line(&format!("{}{}({})", ret.1, f.c_name, call));
    b.dedent();
    b.line("}");
    b.blank();
}

fn emit_instance_method(
    b: &mut CodeBuilder,
    f: &FunctionDef,
    class: &str,
    ffi_type: &str,
    name: &str,
) {
    for d in &f.doc {
        b.line(&format!("/// {}", d.replace('\n', " ")));
    }
    // args[0] is the implicit self. Owned => C takes self by value; any ref
    // kind => C takes a pointer to self, so the method must be `mutating` and
    // pass `&self`.
    let self_by_value = f
        .args
        .first()
        .map(|a| matches!(a.ref_kind, ArgRefKind::Owned))
        .unwrap_or(false);
    let self_expr = if self_by_value { "self" } else { "&self" };
    let modifier = if self_by_value { "" } else { "mutating " };

    let params = format_params(&f.args, /* skip_self */ true);
    let call = format_call_args(&f.args, /* skip_self */ true, Some(self_expr));
    let ret = return_clause(f, class, ffi_type);

    b.line(&format!("{}func {}({}){} {{", modifier, name, params, ret.0));
    b.indent();
    b.line(&format!("{}{}({})", ret.1, f.c_name, call));
    b.dedent();
    b.line("}");
    b.blank();
}

/// Returns `(" -> Ret" or "", "return " or "")`.
fn return_clause(f: &FunctionDef, class: &str, ffi_type: &str) -> (String, String) {
    match f.return_type.as_deref() {
        None => (String::new(), String::new()),
        Some(rt) if rt.trim() == "void" || rt.trim() == "()" => (String::new(), String::new()),
        Some(rt) => {
            let ty = if rt.trim() == class {
                ffi_type.to_string()
            } else {
                map_type(rt)
            };
            (format!(" -> {}", ty), "return ".to_string())
        }
    }
}

fn format_params(args: &[FunctionArg], skip_self: bool) -> String {
    let iter = args.iter().skip(if skip_self { 1 } else { 0 });
    iter.map(|a| format!("_ {}: {}", param_name(&a.name), map_arg_type(&a.type_name, a.ref_kind)))
        .collect::<Vec<_>>()
        .join(", ")
}

fn format_call_args(args: &[FunctionArg], skip_self: bool, self_expr: Option<&str>) -> String {
    let mut out: Vec<String> = Vec::new();
    if let Some(se) = self_expr {
        out.push(se.to_string());
    }
    for a in args.iter().skip(if skip_self { 1 } else { 0 }) {
        out.push(param_name(&a.name));
    }
    out.join(", ")
}

/// Swift parameter identifier: keyword-safe, never empty.
fn param_name(name: &str) -> String {
    let n = name.trim();
    if n.is_empty() {
        return "arg".to_string();
    }
    sanitize_identifier(n)
}

/// Map an IR argument type to its Swift (Clang-imported) spelling.
fn map_arg_type(type_name: &str, ref_kind: ArgRefKind) -> String {
    let base = map_type(type_name);
    // A pointer already spelled in the type string is handled by map_type;
    // only apply the ref-kind wrapper to a plain owned base.
    if type_name.trim_start().starts_with('*') || type_name.trim_start().starts_with('&') {
        return base;
    }
    match ref_kind {
        ArgRefKind::Owned => base,
        ArgRefKind::Ref | ArgRefKind::Ptr => pointer_of(&base, false),
        ArgRefKind::RefMut | ArgRefKind::PtrMut => pointer_of(&base, true),
    }
}

/// Map a bare IR type name (no ref kind) to a Swift type spelling.
fn map_type(type_name: &str) -> String {
    let t = type_name.trim();

    if let Some(rest) = t.strip_prefix("*const ") {
        return pointer_of(&map_type(rest), false);
    }
    if let Some(rest) = t.strip_prefix("*mut ") {
        return pointer_of(&map_type(rest), true);
    }
    if let Some(rest) = t.strip_prefix("&mut ") {
        return pointer_of(&map_type(rest), true);
    }
    if let Some(rest) = t.strip_prefix('&') {
        return pointer_of(&map_type(rest), false);
    }

    match t {
        "bool" => "Bool".to_string(),
        // GLboolean is a C `unsigned char`, imported as UInt8 (NOT Bool).
        "u8" | "c_uchar" | "GLboolean" => "UInt8".to_string(),
        "i8" | "c_char" | "char" => "Int8".to_string(),
        "u16" => "UInt16".to_string(),
        "i16" => "Int16".to_string(),
        "u32" | "c_uint" | "GLuint" | "GLenum" | "GLbitfield" => "UInt32".to_string(),
        "i32" | "c_int" | "GLint" | "GLsizei" => "Int32".to_string(),
        "u64" | "GLuint64" => "UInt64".to_string(),
        "i64" | "GLint64" => "Int64".to_string(),
        "f32" | "GLfloat" | "GLclampf" => "Float".to_string(),
        "f64" | "GLdouble" | "GLclampd" => "Double".to_string(),
        // size_t / ssize_t import as Int in Swift.
        "usize" | "size_t" | "uintptr_t" | "isize" | "ssize_t" | "intptr_t" | "GLsizeiptr"
        | "GLintptr" => "Int".to_string(),
        "c_void" | "void" | "()" => "Void".to_string(),
        // Everything else is an imported FFI type (struct / enum / union /
        // callback-fn typedef), visible under its verbatim Az name.
        other => format!("Az{}", other.strip_prefix("Az").unwrap_or(other)),
    }
}

/// Swift pointer spelling. A pointer to `void` becomes the raw variant.
fn pointer_of(base: &str, mutable: bool) -> String {
    if base == "Void" {
        return if mutable {
            "UnsafeMutableRawPointer".to_string()
        } else {
            "UnsafeRawPointer".to_string()
        };
    }
    if mutable {
        format!("UnsafeMutablePointer<{}>", base)
    } else {
        format!("UnsafePointer<{}>", base)
    }
}

/// `new` -> `create` (matches the Zig backend; keeps `init` free for the
/// hand-written String interop). Everything else passes through.
fn idiomatic_method_name(method_name: &str) -> String {
    match method_name {
        "new" => "create".to_string(),
        other => other.to_string(),
    }
}
