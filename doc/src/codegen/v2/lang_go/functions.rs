//! Raw C-call layer for the Go (cgo) generator: `functions*.go`.
//!
//! One Go function per libazul export, named exactly like the C symbol
//! (`AzDom_addChild`), taking and returning the Go-native types from
//! `types.go`. This is the ONLY place (besides the callback plumbing) that
//! imports `"C"`, and the only `C.` names it uses are the functions it
//! calls plus the fixed-size primitive casts (`C.int32_t`, `C.bool`, ...).
//! No C *type* is ever named: every pointer crosses the boundary as
//! `unsafe.Pointer` / `void*`.
//!
//! # Three call shapes
//!
//! * **Byref** — any call with an owned aggregate argument (struct, enum,
//!   union, or an alias like `GLuint`; the same predicate `lang_c` and
//!   `lang_rust` use for `emit_byref_twin`) goes through the exported
//!   `<symbol>Byref(out*, args*)` twin. The C side receives pointers to
//!   Go-owned memory and moves out of them (the argument is CONSUMED, as
//!   in the by-value call); the return lands in a Go-owned out variable.
//!   A Go struct is therefore never copied by value through cgo — the
//!   ledger's B2 Go-GC crash (`0x8` enum tags in pointer-typed cgo fields
//!   during stack growth) cannot recur.
//! * **Direct** — calls with only primitives / pointers / function
//!   pointers and no aggregate return call `<symbol>` itself, declared in
//!   the preamble with a `void*`/primitive prototype (ABI-identical to
//!   the azul.h one, which is NOT included in these files).
//! * **Shim** — an aggregate RETURN with no aggregate argument has no
//!   Byref twin, and small structs return in registers whose classification
//!   depends on the field types (SysV SSE classes, AAPCS HFAs), so a
//!   `void*` prototype would be wrong. These get a `static inline` C shim
//!   (`azgo_<symbol>(out*, ...)`) in a preamble that includes azul.h.
//!
//! # Why several files
//!
//! cgo's gcc probe grows super-linearly with the number of `C.` names in
//! ONE file (6 s at 5.4k names, 356 s at 9.4k). The probe is per file, so
//! the raw layer is chunked into files of at most [`CHUNK`] functions.

use std::collections::HashSet;

use anyhow::Result;

use super::super::config::CodegenConfig;
use super::super::generator::CodeBuilder;
use super::super::ir::{ArgRefKind, CodegenIR, FunctionArg, FunctionDef, FunctionKind};
use super::super::managed_host_invoker::managed_c_symbol;
use super::types::{go_pointer_to, go_value_type};
use super::{ffi_type_name, primitive_to_cgo, primitive_to_go, sanitize_identifier};

/// Maximum number of C functions referenced by one generated Go file.
pub const CHUNK: usize = 1500;

// ============================================================================
// Call plan
// ============================================================================

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Target {
    /// `C.<symbol>(...)` with a `void*`/primitive extern prototype.
    Direct,
    /// `C.<symbol>Byref(&out, &aggregates..., scalars...)`.
    Byref,
    /// `C.azgo_<symbol>(&out, ...)` — a `static inline` shim over azul.h.
    Shim,
}

/// How one argument crosses the cgo boundary.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Pass {
    /// Primitive by value: `C.<cgo>(x)`; the extern prototype says `<c>`.
    Prim { cgo: String, c: String },
    /// A pointer (any ref_kind, `*const T`, `&T`) or a C function pointer:
    /// `unsafe.Pointer(x)`; prototype `void*`.
    Pointer,
    /// An owned aggregate: `unsafe.Pointer(&x)`; prototype `void*`. In a
    /// Byref twin the callee takes the pointer; in a shim it dereferences.
    ByAddress,
}

#[derive(Debug, Clone)]
pub(crate) struct ArgPlan {
    pub go_name: String,
    pub go_type: String,
    pub pass: Pass,
    /// The C spelling of the argument (`const AzDom*`, `AzDom`, `int32_t`),
    /// used only inside shims for the cast back to the real prototype.
    pub c_type: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Ret {
    Void,
    /// `return <go>(C.f(...))` / out-variable of type `<go>`.
    Prim { go: String, c: String },
    /// Pointer-shaped return (`*AzT`, `unsafe.Pointer`, a callback typedef):
    /// the extern prototype says `void*`.
    Pointer { go: String, c: String },
    /// Struct / union / enum by value: via `__ret` out-pointer.
    Aggregate { go: String, c: String },
}

#[derive(Debug, Clone)]
pub(crate) struct CallPlan {
    /// Go function name (the api.json C name, e.g. `AzDom_addChild`).
    pub go_name: String,
    /// The symbol actually linked (`<c_name>` or `<c_name>Struct`), without
    /// the `Byref` suffix.
    pub symbol: String,
    pub target: Target,
    pub args: Vec<ArgPlan>,
    pub ret: Ret,
}

impl CallPlan {
    /// The `C.` name the Go body calls.
    pub fn c_call_name(&self) -> String {
        match self.target {
            Target::Direct => self.symbol.clone(),
            Target::Byref => format!("{}Byref", self.symbol),
            Target::Shim => format!("azgo_{}", self.symbol),
        }
    }
}

/// The `lang_c` / `lang_rust` `emit_byref_twin` predicate, verbatim: an
/// owned argument whose type name starts with an uppercase letter and is
/// not a bare function-pointer typedef.
pub(crate) fn is_byref_aggregate(arg: &FunctionArg) -> bool {
    matches!(arg.ref_kind, ArgRefKind::Owned)
        && arg
            .type_name
            .trim()
            .chars()
            .next()
            .is_some_and(|c| c.is_ascii_uppercase())
        && !arg.type_name.ends_with("CallbackType")
        && !arg.type_name.ends_with("FnType")
}

/// Follow simple (non-generic) aliases to their target spelling.
fn resolve_alias<'a>(t: &'a str, ir: &'a CodegenIR) -> &'a str {
    let mut cur = t.trim();
    for _ in 0..16 {
        match ir.find_type_alias(cur) {
            Some(a) if a.monomorphized_def.is_none() && !a.target.contains('<') => {
                cur = a.target.trim();
            }
            _ => break,
        }
    }
    cur
}

fn is_pointer_spelling(t: &str) -> bool {
    t.starts_with('*') || t.starts_with('&')
}

fn is_fn_pointer(t: &str, ir: &CodegenIR) -> bool {
    ir.callback_typedefs.iter().any(|c| c.name == t)
        || t.ends_with("CallbackType")
        || t.ends_with("FnType")
}

/// C spelling of a by-value type name (`u32` -> `uint32_t`, `Dom` -> `AzDom`).
pub(crate) fn c_value_type(t: &str) -> String {
    let t = t.trim();
    for (prefix, cst) in [("*const ", "const "), ("*mut ", ""), ("&mut ", ""), ("&", "const ")] {
        if let Some(rest) = t.strip_prefix(prefix) {
            return format!("{}{}*", cst, c_value_type(rest));
        }
    }
    match t {
        "bool" => "bool".into(),
        "u8" => "uint8_t".into(),
        "i8" => "int8_t".into(),
        "u16" => "uint16_t".into(),
        "i16" => "int16_t".into(),
        "u32" => "uint32_t".into(),
        "i32" => "int32_t".into(),
        "u64" => "uint64_t".into(),
        "i64" => "int64_t".into(),
        "usize" => "size_t".into(),
        "isize" => "intptr_t".into(),
        "f32" => "float".into(),
        "f64" => "double".into(),
        "c_void" | "()" | "void" => "void".into(),
        "c_char" => "char".into(),
        "c_uchar" => "unsigned char".into(),
        "c_int" => "int".into(),
        "c_uint" => "unsigned int".into(),
        _ => ffi_type_name(t),
    }
}

fn c_arg_type(arg: &FunctionArg) -> String {
    let base = c_value_type(&arg.type_name);
    match arg.ref_kind {
        ArgRefKind::Owned => base,
        ArgRefKind::Ref | ArgRefKind::Ptr => format!("const {}*", base),
        ArgRefKind::RefMut | ArgRefKind::PtrMut => format!("{}*", base),
    }
}

fn plan_arg(arg: &FunctionArg, ir: &CodegenIR) -> ArgPlan {
    let t = arg.type_name.trim();
    let go_name = sanitize_identifier(&arg.name);
    let c_type = c_arg_type(arg);
    let go_type = match arg.ref_kind {
        ArgRefKind::Owned => go_value_type(t, ir),
        _ => go_pointer_to(&go_value_type(t, ir)),
    };
    let pass = if !matches!(arg.ref_kind, ArgRefKind::Owned) || is_pointer_spelling(t) {
        Pass::Pointer
    } else if is_byref_aggregate(arg) {
        Pass::ByAddress
    } else if is_fn_pointer(t, ir) {
        Pass::Pointer
    } else {
        let resolved = resolve_alias(t, ir);
        match primitive_to_cgo(resolved) {
            Some(cgo) => Pass::Prim {
                cgo: cgo.to_string(),
                c: c_value_type(resolved),
            },
            // Lowercase, non-primitive, non-pointer: treat as an opaque word.
            None => Pass::Pointer,
        }
    };
    ArgPlan {
        go_name,
        go_type,
        pass,
        c_type,
    }
}

fn plan_ret(ret: Option<&str>, ir: &CodegenIR) -> Ret {
    let Some(t) = ret.map(str::trim) else {
        return Ret::Void;
    };
    if t.is_empty() || t == "()" || t == "void" {
        return Ret::Void;
    }
    let go = go_value_type(t, ir);
    let c = c_value_type(t);
    if is_pointer_spelling(t) || is_fn_pointer(t, ir) {
        return Ret::Pointer { go, c };
    }
    let resolved = resolve_alias(t, ir);
    if is_pointer_spelling(resolved) {
        return Ret::Pointer { go, c };
    }
    if let Some(p) = primitive_to_go(resolved) {
        if p.is_empty() {
            return Ret::Void;
        }
        return Ret::Prim {
            go,
            c: c_value_type(resolved),
        };
    }
    Ret::Aggregate { go, c }
}

/// Build the call plan for one IR function, or `None` if the function is
/// not surfaced in the raw layer (enum-variant constructors have native
/// Go constructors in types.go).
pub(crate) fn plan(f: &FunctionDef, ir: &CodegenIR) -> Option<CallPlan> {
    if matches!(f.kind, FunctionKind::EnumVariantConstructor) {
        return None;
    }
    let args: Vec<ArgPlan> = f.args.iter().map(|a| plan_arg(a, ir)).collect();
    let ret = plan_ret(f.return_type.as_deref(), ir);
    let any_aggregate = f.args.iter().any(is_byref_aggregate);
    let target = if any_aggregate && f.fn_body.is_some() {
        Target::Byref
    } else if any_aggregate || matches!(ret, Ret::Aggregate { .. }) {
        Target::Shim
    } else {
        Target::Direct
    };
    Some(CallPlan {
        go_name: f.c_name.clone(),
        symbol: managed_c_symbol(f),
        target,
        args,
        ret,
    })
}

// ============================================================================
// Emission
// ============================================================================

/// C prototype line for a Direct / Byref call (no azul.h in scope).
fn extern_prototype(p: &CallPlan) -> String {
    let mut params: Vec<String> = Vec::with_capacity(p.args.len() + 1);
    let ret_c = match p.target {
        Target::Byref => {
            if p.ret != Ret::Void {
                params.push("void*".into());
            }
            "void".to_string()
        }
        _ => match &p.ret {
            Ret::Void | Ret::Aggregate { .. } => "void".into(),
            Ret::Prim { c, .. } => c.clone(),
            Ret::Pointer { .. } => "void*".into(),
        },
    };
    for a in &p.args {
        params.push(match &a.pass {
            Pass::Prim { c, .. } => c.clone(),
            Pass::Pointer | Pass::ByAddress => "void*".into(),
        });
    }
    if params.is_empty() {
        params.push("void".into());
    }
    format!("extern {} {}({});", ret_c, p.c_call_name(), params.join(", "))
}

/// `static inline` shim over the real azul.h prototype.
fn shim_definition(p: &CallPlan) -> String {
    let mut params: Vec<String> = Vec::with_capacity(p.args.len() + 1);
    let mut call_args: Vec<String> = Vec::with_capacity(p.args.len());
    let has_ret = p.ret != Ret::Void;
    if has_ret {
        params.push("void* __ret".into());
    }
    for a in &p.args {
        let pname = format!("a_{}", a.go_name.trim_end_matches('_'));
        match &a.pass {
            Pass::Prim { c, .. } => {
                params.push(format!("{} {}", c, pname));
                call_args.push(pname);
            }
            Pass::Pointer => {
                params.push(format!("void* {}", pname));
                call_args.push(format!("({}){}", a.c_type, pname));
            }
            Pass::ByAddress => {
                params.push(format!("void* {}", pname));
                call_args.push(format!("*({}*){}", a.c_type, pname));
            }
        }
    }
    if params.is_empty() {
        params.push("void".into());
    }
    let call = format!("{}({})", p.symbol, call_args.join(", "));
    let body = match &p.ret {
        Ret::Void => format!("{};", call),
        Ret::Prim { c, .. } | Ret::Pointer { c, .. } | Ret::Aggregate { c, .. } => {
            format!("*({}*)__ret = {};", c, call)
        }
    };
    format!(
        "static inline void {}({}) {{ {} }}",
        p.c_call_name(),
        params.join(", "),
        body
    )
}

fn emit_go_function(b: &mut CodeBuilder, p: &CallPlan) {
    let params: Vec<String> = p
        .args
        .iter()
        .map(|a| format!("{} {}", a.go_name, a.go_type))
        .collect();
    let ret_go = match &p.ret {
        Ret::Void => String::new(),
        Ret::Prim { go, .. } | Ret::Pointer { go, .. } | Ret::Aggregate { go, .. } => go.clone(),
    };
    let header = if ret_go.is_empty() {
        format!("func {}({}) {{", p.go_name, params.join(", "))
    } else {
        format!("func {}({}) {} {{", p.go_name, params.join(", "), ret_go)
    };
    b.line(&header);
    b.indent();

    let mut call_args: Vec<String> = Vec::with_capacity(p.args.len() + 1);
    let via_out = p.target != Target::Direct && p.ret != Ret::Void;
    if via_out {
        b.line(&format!("var azRet {}", ret_go));
        call_args.push("unsafe.Pointer(&azRet)".into());
    }
    for a in &p.args {
        call_args.push(match &a.pass {
            Pass::Prim { cgo, .. } => format!("{}({})", cgo, a.go_name),
            Pass::Pointer => format!("unsafe.Pointer({})", a.go_name),
            Pass::ByAddress => format!("unsafe.Pointer(&{})", a.go_name),
        });
    }
    let call = format!("C.{}({})", p.c_call_name(), call_args.join(", "));
    if via_out {
        b.line(&call);
        b.line("return azRet");
    } else {
        match &p.ret {
            Ret::Void => b.line(&call),
            Ret::Prim { go, .. } => b.line(&format!("return {}({})", go, call)),
            Ret::Pointer { go, .. } => {
                if go == "unsafe.Pointer" {
                    b.line(&format!("return {}", call));
                } else {
                    b.line(&format!("return ({})({})", go, call));
                }
            }
            Ret::Aggregate { .. } => unreachable!("aggregate returns never go Direct"),
        }
    }
    b.dedent();
    b.line("}");
    b.blank();
}

fn emit_file(b: &mut CodeBuilder, plans: &[&CallPlan], shim: bool, index: usize, total: usize) {
    b.line("// ============================================================================");
    b.line(&format!(
        "// {} - raw libazul calls ({}/{}). Auto-generated by azul-doc codegen v2 (lang_go).",
        file_name(shim, index),
        index + 1,
        total
    ));
    b.line("// DO NOT EDIT MANUALLY.");
    b.line("// ============================================================================");
    b.line("//");
    if shim {
        b.line("// Functions that return an aggregate by value and have no *Byref twin: each");
        b.line("// goes through a static inline C shim that writes the result into Go-owned");
        b.line("// memory, so the register classification of small structs stays with the");
        b.line("// C compiler and Go never names a C type.");
    } else {
        b.line("// Owned aggregates cross the boundary by pointer through the *Byref twins");
        b.line("// (consumed by the callee, exactly like the by-value call); everything else");
        b.line("// is a primitive or a pointer. The prototypes below are ABI-identical to the");
        b.line("// azul.h ones, spelled with void* so no C type has to be named from Go.");
    }
    b.blank();
    b.line("package azul");
    b.blank();
    b.line("/*");
    if shim {
        b.line("#include \"azul.h\"");
        for p in plans {
            b.line(&shim_definition(p));
        }
    } else {
        b.line("#include <stdbool.h>");
        b.line("#include <stdint.h>");
        b.line("#include <stddef.h>");
        for p in plans {
            b.line(&extern_prototype(p));
        }
    }
    b.line("*/");
    b.line("import \"C\"");
    b.blank();
    b.line("import \"unsafe\"");
    b.blank();
    b.line("var _ unsafe.Pointer");
    b.blank();
    for p in plans {
        emit_go_function(b, p);
    }
}

fn file_name(shim: bool, index: usize) -> String {
    match (shim, index) {
        (false, 0) => "functions.go".into(),
        (false, i) => format!("functions_{}.go", i + 1),
        (true, 0) => "functions_shim.go".into(),
        (true, i) => format!("functions_shim_{}.go", i + 1),
    }
}

/// All call plans of the IR, deduplicated by Go name, in IR order.
pub(crate) fn plans(ir: &CodegenIR, config: &CodegenConfig) -> Vec<CallPlan> {
    let mut seen: HashSet<String> = HashSet::new();
    let mut out = Vec::new();
    for f in &ir.functions {
        if !config.should_include_type(&f.class_name) {
            continue;
        }
        let Some(p) = plan(f, ir) else { continue };
        if !seen.insert(p.go_name.clone()) {
            continue;
        }
        out.push(p);
    }
    out
}

/// Generate the raw layer as `(relative path, contents)` pairs.
pub fn generate_files(ir: &CodegenIR, config: &CodegenConfig) -> Result<Vec<(String, String)>> {
    let all = plans(ir, config);
    let direct: Vec<&CallPlan> = all.iter().filter(|p| p.target != Target::Shim).collect();
    let shims: Vec<&CallPlan> = all.iter().filter(|p| p.target == Target::Shim).collect();
    let mut files = Vec::new();
    for (shim, set) in [(false, direct), (true, shims)] {
        let chunks: Vec<&[&CallPlan]> = if set.is_empty() {
            vec![&[][..]]
        } else {
            set.chunks(CHUNK).collect()
        };
        let total = chunks.len();
        for (i, chunk) in chunks.into_iter().enumerate() {
            let mut b = CodeBuilder::new(&config.indent);
            emit_file(&mut b, chunk, shim, i, total);
            files.push((file_name(shim, i), b.finish()));
        }
    }
    Ok(files)
}

#[cfg(test)]
mod tests {
    use super::super::types::tests::fixture_ir;
    use super::*;

    fn arg(name: &str, ty: &str, rk: ArgRefKind) -> FunctionArg {
        FunctionArg {
            name: name.into(),
            type_name: ty.into(),
            ref_kind: rk,
            doc: None,
            callback_info: None,
        }
    }

    fn func(c_name: &str, class: &str, args: Vec<FunctionArg>, ret: Option<&str>) -> FunctionDef {
        FunctionDef {
            c_name: c_name.into(),
            class_name: class.into(),
            method_name: c_name.rsplit('_').next().unwrap().into(),
            kind: FunctionKind::Method,
            args,
            return_type: ret.map(str::to_string),
            fn_body: Some("body".into()),
            doc: vec![],
            is_const: false,
            is_unsafe: false,
        }
    }

    fn ir_with_functions() -> CodegenIR {
        let mut ir = fixture_ir();
        ir.functions.push(func(
            "AzDom_addChild",
            "Dom",
            vec![arg("dom", "Dom", ArgRefKind::RefMut), arg("child", "Dom", ArgRefKind::Owned)],
            None,
        ));
        ir.functions.push(func(
            "AzDom_createBody",
            "Dom",
            vec![],
            Some("Dom"),
        ));
        ir.functions.push(func(
            "AzDom_len",
            "Dom",
            vec![arg("dom", "Dom", ArgRefKind::Ref), arg("flag", "bool", ArgRefKind::Owned)],
            Some("usize"),
        ));
        ir.functions.push(func(
            "AzOptionDom_take",
            "OptionDom",
            vec![arg("opt", "OptionDom", ArgRefKind::Owned)],
            Some("Dom"),
        ));
        ir.functions.push(func(
            "AzDom_ptr",
            "Dom",
            vec![arg("dom", "Dom", ArgRefKind::Ref)],
            Some("*const c_void"),
        ));
        ir
    }

    fn gen() -> Vec<(String, String)> {
        generate_files(&ir_with_functions(), &CodegenConfig::c_header()).unwrap()
    }

    #[test]
    fn owned_aggregate_goes_through_byref_twin() {
        let files = gen();
        let direct = &files.iter().find(|(n, _)| n == "functions.go").unwrap().1;
        assert!(direct.contains("extern void AzDom_addChildByref(void*, void*);"), "{direct}");
        assert!(direct.contains("func AzDom_addChild(dom *AzDom, child AzDom) {\n    C.AzDom_addChildByref(unsafe.Pointer(dom), unsafe.Pointer(&child))\n}"), "{direct}");
        // Return through the out-pointer, argument consumed by address.
        assert!(direct.contains("extern void AzOptionDom_takeByref(void*, void*);"));
        assert!(direct.contains("func AzOptionDom_take(opt AzOptionDom) AzDom {\n    var azRet AzDom\n    C.AzOptionDom_takeByref(unsafe.Pointer(&azRet), unsafe.Pointer(&opt))\n    return azRet\n}"), "{direct}");
    }

    #[test]
    fn scalar_only_calls_are_direct_with_primitive_prototype() {
        let files = gen();
        let direct = &files.iter().find(|(n, _)| n == "functions.go").unwrap().1;
        assert!(direct.contains("extern size_t AzDom_len(void*, bool);"), "{direct}");
        assert!(direct.contains("func AzDom_len(dom *AzDom, flag bool) uintptr {\n    return uintptr(C.AzDom_len(unsafe.Pointer(dom), C.bool(flag)))\n}"), "{direct}");
        assert!(direct.contains("extern void* AzDom_ptr(void*);"));
        assert!(direct.contains("func AzDom_ptr(dom *AzDom) unsafe.Pointer {\n    return C.AzDom_ptr(unsafe.Pointer(dom))\n}"), "{direct}");
    }

    #[test]
    fn aggregate_return_without_twin_gets_a_shim_over_azul_h() {
        let files = gen();
        let shim = &files.iter().find(|(n, _)| n == "functions_shim.go").unwrap().1;
        assert!(shim.contains("#include \"azul.h\""));
        assert!(shim.contains("static inline void azgo_AzDom_createBody(void* __ret) { *(AzDom*)__ret = AzDom_createBody(); }"), "{shim}");
        assert!(shim.contains("func AzDom_createBody() AzDom {\n    var azRet AzDom\n    C.azgo_AzDom_createBody(unsafe.Pointer(&azRet))\n    return azRet\n}"), "{shim}");
        // The direct file never includes azul.h and never names a C type.
        let direct = &files.iter().find(|(n, _)| n == "functions.go").unwrap().1;
        assert!(!direct.contains("#include \"azul.h\""));
        // Every `C.Az` there is a function call, never a type cast / value.
        for (i, _) in direct.match_indices("C.Az") {
            let rest = &direct[i + 4..];
            let ident_end = rest.find(|c: char| !(c.is_ascii_alphanumeric() || c == '_')).unwrap_or(rest.len());
            assert!(rest[ident_end..].starts_with('('), "C type named in direct file: {}", &rest[..ident_end]);
        }
        assert!(!direct.contains("*C."), "{direct}");
    }

    #[test]
    fn byref_predicate_matches_lang_c() {
        assert!(is_byref_aggregate(&arg("x", "Dom", ArgRefKind::Owned)));
        assert!(is_byref_aggregate(&arg("x", "GLuint", ArgRefKind::Owned)));
        assert!(!is_byref_aggregate(&arg("x", "CallbackType", ArgRefKind::Owned)));
        assert!(!is_byref_aggregate(&arg("x", "u32", ArgRefKind::Owned)));
        assert!(!is_byref_aggregate(&arg("x", "Dom", ArgRefKind::Ref)));
    }
}
