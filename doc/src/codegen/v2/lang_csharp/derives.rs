//! The derive surface of the C# FFI value types.
//!
//! Everything the C ABI exposes crosses it as a `struct Az<T>`: a POD
//! struct, a tagged union, or a monomorphized generic alias. The
//! `IDisposable` wrapper classes in `wrappers.rs` cover the types that own
//! native memory and the types that have methods — but that is a fraction
//! of the API, and for every other type the *value* is what user code
//! holds. So this is where `Az<T>_toDbgString`, `_partialEq`, `_hash`,
//! `_cmp` / `_partialCmp`, `_clone`, `_createDefault` and `_delete` become
//! `ToString()`, `Equals`, `GetHashCode`, `CompareTo`, `Clone`,
//! `CreateDefault` and `Dispose` on the value type itself.
//!
//! Nothing here reimplements a derive in C#: every member is one call into
//! the export libazul already has, so `Equals` is Rust's `PartialEq` and
//! `CompareTo` is Rust's `Ord`, not a field-by-field guess. The marshalling
//! those exports need (each takes `const Az<T>*`) lives once, in the
//! emitted `__AzDerive` runtime, so a member stays a single
//! expression-bodied line — the surface is ~2000 types wide.
//!
//! Which members a type gets is decided by which trait functions the IR
//! HAS, never by its `TypeTraits` flags: the IR builder derives one from
//! the other, and keying on the function means a member can never name an
//! export that does not exist. The P/Invoke filter
//! (`functions::should_emit_function`) is consulted for the same reason.

use super::{
    super::{
        config::CodegenConfig,
        generator::CodeBuilder,
        ir::{CodegenIR, FunctionDef, FunctionKind, StructDef, TypeCategory},
        managed_host_invoker::managed_c_symbol,
        managed_lang_helpers::{has_delete_function, has_wrapper_class},
    },
    ffi_type_name,
};

/// Name of the emitted marshalling runtime the derive members call into.
const RUNTIME: &str = "__AzDerive";

/// The trait entry point of `kind` on `type_name`, provided the IR has it
/// *and* the P/Invoke layer declares it — a member naming an import that
/// `should_emit_function` filtered out would not compile.
fn derive_fn<'a>(
    ir: &'a CodegenIR,
    config: &CodegenConfig,
    type_name: &str,
    kind: FunctionKind,
) -> Option<&'a FunctionDef> {
    ir.functions
        .iter()
        .filter(|f| f.class_name == type_name && f.kind == kind)
        .find(|f| super::functions::should_emit_function(f, ir, config))
}

/// `NativeMethods.<symbol>` for a trait entry point, spelled exactly as
/// `functions.rs` declared it.
fn native(func: &FunctionDef) -> String {
    format!("NativeMethods.{}", managed_c_symbol(func))
}

/// Does an owning wrapper CLASS already exist for this type (`Dom`,
/// `App`, `StringVec`, …)?
///
/// If so the value type gets no OWNERSHIP member: the class' `Dispose()`
/// and finalizer are the one free path — a second one on the raw struct is
/// a double free waiting for a user who holds both — and its `Clone()` /
/// `CreateDefault()` hand back a wrapper that can be freed, where the raw
/// struct's would hand back an owned value with no way to free it. Those
/// three exports stay reachable through the class.
fn has_owning_wrapper(type_name: &str, ir: &CodegenIR) -> bool {
    has_wrapper_class(type_name, ir) && has_delete_function(type_name, ir)
}

/// May the value type free itself? Not when a wrapper class owns it, and
/// not for a borrowed slice — a `VecRef` is `{ptr, len}` into memory the
/// CALLER owns, so its `_delete` would free someone else's buffer. (Its
/// `_clone` only copies the borrow, which is why that one stays.)
fn may_free(type_name: &str, category: TypeCategory, ir: &CodegenIR) -> bool {
    !matches!(category, TypeCategory::VecRef) && !has_owning_wrapper(type_name, ir)
}

/// The interface list to append to the value type's declaration, e.g.
/// `" : System.IComparable<AzDpiScaleFactor>, System.IDisposable"`.
/// Empty when the type derives neither ordering nor a destructor.
pub fn value_type_interfaces(
    type_name: &str,
    category: TypeCategory,
    ir: &CodegenIR,
    config: &CodegenConfig,
) -> String {
    let ffi = ffi_type_name(type_name);
    let mut ifaces: Vec<String> = Vec::new();
    if ordering_fn(ir, config, type_name).is_some() {
        ifaces.push(format!("System.IComparable<{}>", ffi));
    }
    if may_free(type_name, category, ir)
        && derive_fn(ir, config, type_name, FunctionKind::Delete).is_some()
    {
        ifaces.push("System.IDisposable".to_string());
    }
    if ifaces.is_empty() {
        String::new()
    } else {
        format!(" : {}", ifaces.join(", "))
    }
}

/// The export `CompareTo` routes through: the total order when the type has
/// one, else the partial order. A type with both surfaces `_cmp` only —
/// `_partialCmp` can answer "these do not compare", which `CompareTo`
/// cannot express anyway.
pub fn ordering_fn<'a>(
    ir: &'a CodegenIR,
    config: &CodegenConfig,
    type_name: &str,
) -> Option<&'a FunctionDef> {
    derive_fn(ir, config, type_name, FunctionKind::Cmp)
        .or_else(|| derive_fn(ir, config, type_name, FunctionKind::PartialCmp))
}

/// Can `__AzDerive.Dbg` be emitted at all? It decodes the string type the
/// `_toDbgString` exports return, so it needs that type, its destructor and
/// its byte layout. Without it no `ToString()` member may be emitted either.
fn dbg_supported(ir: &CodegenIR, config: &CodegenConfig) -> bool {
    ir.structs
        .iter()
        .find(|s| matches!(s.category, TypeCategory::String))
        .is_some_and(|s| {
            derive_fn(ir, config, &s.name, FunctionKind::Delete).is_some()
                && string_byte_path(s, ir).is_some()
        })
}

/// Emit the derive members inside an already-open `struct Az<T> { … }`.
pub fn emit_value_derives(
    builder: &mut CodeBuilder,
    type_name: &str,
    category: TypeCategory,
    ir: &CodegenIR,
    config: &CodegenConfig,
) {
    let ffi = ffi_type_name(type_name);

    if dbg_supported(ir, config) {
        if let Some(f) = derive_fn(ir, config, type_name, FunctionKind::DebugToString) {
            builder.line(&format!(
                "/// <summary>Debug repr, routed through {}.</summary>",
                f.c_name
            ));
            builder.line(&format!(
                "public override string ToString() => {rt}.Dbg(this, {call});",
                rt = RUNTIME,
                call = native(f),
            ));
        }
    }

    let eq = derive_fn(ir, config, type_name, FunctionKind::PartialEq);
    let hash = derive_fn(ir, config, type_name, FunctionKind::Hash);
    if let Some(f) = eq {
        builder.line(&format!(
            "/// <summary>Value equality, routed through {}.</summary>",
            f.c_name
        ));
        // Plain `object` (not `object?`): PowerShell's Add-Type embeds this
        // file without a `#nullable` context, where `object?` trips CS8632.
        builder.line(&format!(
            "public override bool Equals(object other) => other is {ffi} __o && {rt}.Eq(this, \
             __o, {call});",
            ffi = ffi,
            rt = RUNTIME,
            call = native(f),
        ));
    }
    match (hash, eq) {
        (Some(f), _) => {
            builder.line(&format!(
                "/// <summary>Value hash, routed through {}.</summary>",
                f.c_name
            ));
            builder.line(&format!(
                "public override int GetHashCode() => {rt}.Hash(this, {call});",
                rt = RUNTIME,
                call = native(f),
            ));
        }
        (None, Some(_)) => {
            // Equals compares VALUES and the type has no value hash: equal
            // values must hash equal, and the struct's own bits include
            // pointers that differ between two equal values, so a constant
            // is the only hash that keeps the contract.
            builder.line(
                "/// <summary>Constant: equal values must hash equal, and the type has no value \
                 hash.</summary>",
            );
            builder.line("public override int GetHashCode() => 0;");
        }
        (None, None) => {}
    }

    if let Some(f) = ordering_fn(ir, config, type_name) {
        builder.line(&format!(
            "/// <summary>Ordering, routed through {}.</summary>",
            f.c_name
        ));
        builder.line(&format!(
            "public int CompareTo({ffi} other) => {rt}.Cmp(this, other, {call});",
            ffi = ffi,
            rt = RUNTIME,
            call = native(f),
        ));
    }

    // The three members below hand out an OWNED value, so they exist only
    // where the value type is the owner (see `has_owning_wrapper`).
    let owns = !has_owning_wrapper(type_name, ir);

    if let Some(f) = owns
        .then(|| derive_fn(ir, config, type_name, FunctionKind::DeepCopy))
        .flatten()
    {
        builder.line(&format!(
            "/// <summary>Deep copy, routed through {}.</summary>",
            f.c_name
        ));
        builder.line(&format!(
            "public {ffi} Clone() => {rt}.Call(this, {call});",
            ffi = ffi,
            rt = RUNTIME,
            call = native(f),
        ));
    }

    if let Some(f) = owns
        .then(|| derive_fn(ir, config, type_name, FunctionKind::Default))
        .flatten()
    {
        // `CreateDefault`, not `Default`: a tagged union with a `Default`
        // VARIANT already has a field of that name, and a member may not
        // share it (CS0102). The C spelling is unambiguous for every type.
        builder.line(&format!(
            "/// <summary>The type's default value, routed through {}.</summary>",
            f.c_name
        ));
        builder.line(&format!(
            "public static {ffi} CreateDefault() => {call}();",
            ffi = ffi,
            call = native(f),
        ));
    }

    if may_free(type_name, category, ir) {
        if let Some(f) = derive_fn(ir, config, type_name, FunctionKind::Delete) {
            builder.line(&format!(
                "/// <summary>Frees the native data this value owns, through {}.</summary>",
                f.c_name
            ));
            builder.line(
                "/// <remarks>A struct is copied by assignment and every copy points at the same \
                 native data: dispose exactly one of them, and never one the engine handed you \
                 (a callback argument).</remarks>",
            );
            builder.line(&format!(
                "public void Dispose() => {rt}.Consume(this, {call});",
                rt = RUNTIME,
                call = native(f),
            ));
        }
    }
}

/// Emit the `__AzDerive` runtime: the marshalling every derive member
/// shares. Emitted once per file, before or after its callers (C# has no
/// declaration order).
pub fn emit_derive_runtime(builder: &mut CodeBuilder, ir: &CodegenIR, config: &CodegenConfig) {
    let m = "System.Runtime.InteropServices.Marshal";

    builder.line("// --------------------------------------------------------------------------");
    builder.line("// __AzDerive: the marshalling the Az<T>_<derive> exports need.");
    builder.line("// --------------------------------------------------------------------------");
    builder.blank();
    builder.line(
        "/// <summary>Internal: every derive export takes <c>const Az&lt;T&gt;*</c>, so the value \
         reaches it as a heap copy. AllocHGlobal rather than <c>fixed</c> keeps the file \
         compilable under PowerShell's Add-Type, which has no /unsafe.</summary>",
    );
    builder.line("internal static class __AzDerive");
    builder.line("{");
    builder.indent();

    builder.line("/// <summary>Call an export that takes the value by pointer.</summary>");
    builder.line(
        "internal static R Call<T, R>(T value, System.Func<System.IntPtr, R> call) where T : \
         struct",
    );
    builder.line("{");
    builder.indent();
    builder.line(&format!(
        "var p = {m}.AllocHGlobal({m}.SizeOf<T>());",
        m = m
    ));
    builder.line("try");
    builder.line("{");
    builder.indent();
    builder.line(&format!("{m}.StructureToPtr(value, p, false);", m = m));
    builder.line("return call(p);");
    builder.dedent();
    builder.line("}");
    builder.line(&format!("finally {{ {m}.FreeHGlobal(p); }}", m = m));
    builder.dedent();
    builder.line("}");
    builder.blank();

    builder.line(
        "/// <summary>Call an export that consumes the value by pointer (a \
         destructor).</summary>",
    );
    builder.line(
        "internal static void Consume<T>(T value, System.Action<System.IntPtr> call) where T : \
         struct",
    );
    builder.line("{");
    builder.indent();
    builder.line(&format!(
        "var p = {m}.AllocHGlobal({m}.SizeOf<T>());",
        m = m
    ));
    builder.line("try");
    builder.line("{");
    builder.indent();
    builder.line(&format!("{m}.StructureToPtr(value, p, false);", m = m));
    builder.line("call(p);");
    builder.dedent();
    builder.line("}");
    builder.line(&format!("finally {{ {m}.FreeHGlobal(p); }}", m = m));
    builder.dedent();
    builder.line("}");
    builder.blank();

    builder.line("/// <summary>Call an export that takes two values by pointer.</summary>");
    builder.line(
        "private static R Both<T, R>(T a, T b, System.Func<System.IntPtr, System.IntPtr, R> call) \
         where T : struct",
    );
    builder.line("{");
    builder.indent();
    builder.line(&format!("var sz = {m}.SizeOf<T>();", m = m));
    builder.line(&format!("var pa = {m}.AllocHGlobal(sz);", m = m));
    builder.line(&format!("var pb = {m}.AllocHGlobal(sz);", m = m));
    builder.line("try");
    builder.line("{");
    builder.indent();
    builder.line(&format!("{m}.StructureToPtr(a, pa, false);", m = m));
    builder.line(&format!("{m}.StructureToPtr(b, pb, false);", m = m));
    builder.line("return call(pa, pb);");
    builder.dedent();
    builder.line("}");
    builder.line("finally");
    builder.line("{");
    builder.indent();
    builder.line(&format!("{m}.FreeHGlobal(pa);", m = m));
    builder.line(&format!("{m}.FreeHGlobal(pb);", m = m));
    builder.dedent();
    builder.line("}");
    builder.dedent();
    builder.line("}");
    builder.blank();

    builder.line("/// <summary>Value equality.</summary>");
    builder.line(
        "internal static bool Eq<T>(T a, T b, System.Func<System.IntPtr, System.IntPtr, bool> \
         call) where T : struct => Both(a, b, call);",
    );
    builder.blank();

    builder.line("/// <summary>Value hash, folded from the 64-bit C hash into an int.</summary>");
    builder.line(
        "internal static int Hash<T>(T value, System.Func<System.IntPtr, ulong> call) where T : \
         struct",
    );
    builder.line("{");
    builder.indent();
    builder.line("var h = Call(value, call);");
    builder.line("return (int)(h ^ (h >> 32));");
    builder.dedent();
    builder.line("}");
    builder.blank();

    builder.line("/// <summary>Ordering.</summary>");
    builder.line(
        "internal static int Cmp<T>(T a, T b, System.Func<System.IntPtr, System.IntPtr, byte> \
         call) where T : struct",
    );
    builder.line("{");
    builder.indent();
    builder.line("// The C ABI encodes Ordering as 0/1/2 (Less/Equal/Greater); a partial order");
    builder.line("// answers 255 for \"these do not compare\" (a NaN inside), which CompareTo has");
    builder.line("// no spelling for — sort those last so the result stays usable as a key.");
    builder.line("var o = Both(a, b, call);");
    builder.line("return o == 255 ? 1 : (int)o - 1;");
    builder.dedent();
    builder.line("}");
    builder.blank();

    emit_dbg_helper(builder, ir, config);

    builder.dedent();
    builder.line("}");
    builder.blank();
}

/// `Dbg`: run a `_toDbgString` export and decode the `AzString` it returns.
///
/// Both the string type and its destructor come from the IR (the
/// `TypeCategory::String` struct) rather than a literal name, so the helper
/// follows api.json if the type is ever renamed. Emitted only when the API
/// has a string type with a destructor — without it no `ToString()` member
/// is emitted either, because `derive_fn` would have nothing to decode.
fn emit_dbg_helper(builder: &mut CodeBuilder, ir: &CodegenIR, config: &CodegenConfig) {
    let Some(string_struct) = ir
        .structs
        .iter()
        .find(|s| matches!(s.category, TypeCategory::String))
    else {
        return;
    };
    let Some(delete) = derive_fn(ir, config, &string_struct.name, FunctionKind::Delete) else {
        return;
    };
    let Some((vec_field, ptr_field, len_field)) = string_byte_path(string_struct, ir) else {
        return;
    };
    let m = "System.Runtime.InteropServices.Marshal";
    let ffi = ffi_type_name(&string_struct.name);

    builder.line(&format!(
        "/// <summary>Run a debug-string export and decode the {} it returns.</summary>",
        ffi
    ));
    builder.line(&format!(
        "internal static string Dbg<T>(T value, System.Func<System.IntPtr, {ffi}> call) where T : \
         struct",
        ffi = ffi
    ));
    builder.line("{");
    builder.indent();
    builder.line("var s = Call(value, call);");
    builder.line(&format!("var __p = s.{}.{};", vec_field, ptr_field));
    builder.line(&format!(
        "var __n = (long)s.{}.{}.ToUInt64();",
        vec_field, len_field
    ));
    builder.line("var __text = \"\";");
    builder.line("if (__p != System.IntPtr.Zero && __n > 0)");
    builder.line("{");
    builder.indent();
    builder.line("var __bytes = new byte[__n];");
    builder.line(&format!("{m}.Copy(__p, __bytes, 0, (int)__n);", m = m));
    builder.line("__text = System.Text.Encoding.UTF8.GetString(__bytes);");
    builder.dedent();
    builder.line("}");
    builder.line("// The export handed us an owned string; free it before returning the copy.");
    builder.line(&format!("Consume(s, {});", native(delete)));
    builder.line("return __text;");
    builder.dedent();
    builder.line("}");
}

/// Where the bytes of the API's string type live: `(<vec field>, <ptr
/// field>, <len field>)`, e.g. `("vec", "ptr", "len")`. The string type
/// wraps one byte Vec, and a Vec is the `ptr` / `len` / `cap` /
/// `destructor` layout.
fn string_byte_path(s: &StructDef, ir: &CodegenIR) -> Option<(String, String, String)> {
    let vec_field = s.fields.first()?;
    let vec_struct = ir.find_struct(vec_field.type_name.trim())?;
    let ptr = vec_struct.fields.iter().find(|f| f.name == "ptr")?;
    let len = vec_struct.fields.iter().find(|f| f.name == "len")?;
    Some((vec_field.name.clone(), ptr.name.clone(), len.name.clone()))
}
