//! Raw call layer for the Go (purego) generator: `functions.go`.
//!
//! One Go function per libazul export, named exactly like the api.json C
//! symbol (`AzDom_addChild`), taking and returning the Go-native types from
//! `types.go`. Nothing here is cgo: each function owns a purego function
//! value that is bound to the dylib symbol on first use (`sync.Once` +
//! `azRegister`, see `azul.go`). Unused exports therefore cost nothing at
//! load time, and an export purego cannot bind at all (more than
//! [`PUREGO_MAX_ARGS`] integer arguments) only fails when it is called.
//!
//! # Two call shapes
//!
//! * **Byref** — a function that returns a value aggregate and/or takes an
//!   owned value aggregate calls the exported `<symbol>Byref` twin: the
//!   return travels through a leading out-pointer, every owned aggregate
//!   argument by pointer (CONSUMED by the callee, exactly like the by-value
//!   call), everything else unchanged. "Value aggregate" is
//!   [`CodegenIR::is_value_aggregate`] — the single predicate `lang_c` and
//!   `lang_rust` use to emit the twins, so the Go signature and the DLL
//!   export always agree. purego never sees a struct by value this way,
//!   which is what makes the same code work on Linux and Windows (purego
//!   passes structs by value on macOS only).
//! * **Direct** — everything else calls `<symbol>` itself. C enums, type
//!   aliases and primitives cross by value (purego dispatches on the Go
//!   kind, so the named enum types from `types.go` are fine), pointers as
//!   typed Go pointers (`*AzDom`, `unsafe.Pointer`, callback typedefs), which
//!   purego keeps alive for the duration of the call.

use std::collections::HashSet;

use anyhow::Result;

use super::super::config::CodegenConfig;
use super::super::generator::CodeBuilder;
use super::super::ir::{ArgRefKind, CodegenIR, FunctionArg, FunctionDef, FunctionKind};
use super::super::managed_host_invoker::managed_c_symbol;
use super::types::{go_pointer_to, go_value_type};
use super::{primitive_to_go, sanitize_identifier};

/// purego binds at most this many integer-class arguments per call
/// (`maxArgs` in purego's `syscall.go` on 64-bit targets). Functions above
/// the limit are still emitted so the package compiles, but their body
/// panics with a message naming the limit instead of registering.
pub const PUREGO_MAX_ARGS: usize = 15;

// ============================================================================
// Call plan
// ============================================================================

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Target {
    /// `<symbol>(...)`: no value aggregate crosses by value.
    Direct,
    /// `<symbol>Byref(&out, &aggregates..., scalars...)`.
    Byref,
}

/// How one argument crosses the purego boundary.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Pass {
    /// By value: primitives, C enums, type aliases. The purego parameter
    /// has the Go type of the argument.
    Value,
    /// Already a pointer on the Go side (`*AzT`, `unsafe.Pointer`, a
    /// fn-pointer typedef): passed as is.
    Pointer,
    /// An owned value aggregate: `&x` into the Byref twin, which consumes it.
    ByAddress,
}

#[derive(Debug, Clone)]
pub(crate) struct ArgPlan {
    pub go_name: String,
    /// Type in the public Go signature (`AzDom` for a consumed aggregate).
    pub go_type: String,
    pub pass: Pass,
}

impl ArgPlan {
    /// Type of the purego function-value parameter.
    fn lib_type(&self) -> String {
        match self.pass {
            Pass::Value | Pass::Pointer => self.go_type.clone(),
            Pass::ByAddress => format!("*{}", self.go_type),
        }
    }

    /// Expression handed to the purego function value.
    fn call_expr(&self) -> String {
        match self.pass {
            Pass::Value | Pass::Pointer => self.go_name.clone(),
            Pass::ByAddress => format!("&{}", self.go_name),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Ret {
    Void,
    /// Returned by purego itself: primitive, C enum, pointer.
    Value { go: String },
    /// Value aggregate: written by the Byref twin through a leading
    /// out-pointer.
    Aggregate { go: String },
}

#[derive(Debug, Clone)]
pub(crate) struct CallPlan {
    /// Go function name (the api.json C name, e.g. `AzDom_addChild`).
    pub go_name: String,
    /// The symbol actually bound (`<c_name>` or `<c_name>Struct`), without
    /// the `Byref` suffix.
    pub symbol: String,
    pub target: Target,
    pub args: Vec<ArgPlan>,
    pub ret: Ret,
}

impl CallPlan {
    /// The dylib symbol the function value is bound to.
    pub fn bound_symbol(&self) -> String {
        match self.target {
            Target::Direct => self.symbol.clone(),
            Target::Byref => format!("{}Byref", self.symbol),
        }
    }

    fn returns_via_out_pointer(&self) -> bool {
        matches!(self.ret, Ret::Aggregate { .. })
    }

    /// Number of purego parameters (out-pointer included).
    fn purego_arity(&self) -> usize {
        self.args.len() + usize::from(self.returns_via_out_pointer())
    }
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

/// A C function pointer, i.e. one of the IR's callback typedefs.
///
/// The name-suffix fallback this replaced ("ends with CallbackType/FnType")
/// caught two api.json classes that are NOT function pointers at all -
/// `CoreCallbackType` and `CoreRenderImageCallbackType` are aliases of
/// `usize`, core's type-erased storage - and no api.json function takes
/// either, so the fallback only ever misclassified them.
fn is_fn_pointer(t: &str, ir: &CodegenIR) -> bool {
    ir.callback_typedefs.iter().any(|c| c.name == t)
}

/// The `lang_c` / `lang_rust` twin predicate for one argument: owned and a
/// value aggregate.
pub(crate) fn is_byref_aggregate(arg: &FunctionArg, ir: &CodegenIR) -> bool {
    matches!(arg.ref_kind, ArgRefKind::Owned) && ir.is_value_aggregate(&arg.type_name)
}

fn plan_arg(arg: &FunctionArg, ir: &CodegenIR) -> ArgPlan {
    let t = arg.type_name.trim();
    let go_name = sanitize_identifier(&arg.name);
    let (go_type, pass) = if !matches!(arg.ref_kind, ArgRefKind::Owned) {
        (go_pointer_to(&go_value_type(t, ir)), Pass::Pointer)
    } else if is_pointer_spelling(t)
        || is_fn_pointer(t, ir)
        || is_pointer_spelling(resolve_alias(t, ir))
    {
        (go_value_type(t, ir), Pass::Pointer)
    } else if is_byref_aggregate(arg, ir) {
        (go_value_type(t, ir), Pass::ByAddress)
    } else {
        (go_value_type(t, ir), Pass::Value)
    };
    ArgPlan {
        go_name,
        go_type,
        pass,
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
    if primitive_to_go(resolve_alias(t, ir)).is_some_and(str::is_empty) {
        return Ret::Void;
    }
    if ir.is_value_aggregate(t) {
        Ret::Aggregate { go }
    } else {
        Ret::Value { go }
    }
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
    let target = if f.args.iter().any(|a| is_byref_aggregate(a, ir))
        || matches!(ret, Ret::Aggregate { .. })
    {
        Target::Byref
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

fn emit_go_function(b: &mut CodeBuilder, p: &CallPlan) {
    let params: Vec<String> = p
        .args
        .iter()
        .map(|a| format!("{} {}", a.go_name, a.go_type))
        .collect();
    let ret_go = match &p.ret {
        Ret::Void => String::new(),
        Ret::Value { go } | Ret::Aggregate { go } => go.clone(),
    };
    let header = if ret_go.is_empty() {
        format!("func {}({}) {{", p.go_name, params.join(", "))
    } else {
        format!("func {}({}) {} {{", p.go_name, params.join(", "), ret_go)
    };

    if p.purego_arity() > PUREGO_MAX_ARGS {
        b.line(&format!(
            "// {} takes {} arguments; purego binds at most {}.",
            p.go_name,
            p.purego_arity(),
            PUREGO_MAX_ARGS
        ));
        b.line(&header);
        b.indent();
        b.line(&format!(
            "panic(\"azul: {} takes {} arguments, purego supports at most {}\")",
            p.go_name,
            p.purego_arity(),
            PUREGO_MAX_ARGS
        ));
        b.dedent();
        b.line("}");
        return;
    }

    // The purego function value + its one-time binding.
    let mut lib_params: Vec<String> = Vec::with_capacity(p.purego_arity());
    if p.returns_via_out_pointer() {
        lib_params.push(format!("*{}", ret_go));
    }
    lib_params.extend(p.args.iter().map(ArgPlan::lib_type));
    let lib_ret = match &p.ret {
        Ret::Value { go } => format!(" {}", go),
        Ret::Void | Ret::Aggregate { .. } => String::new(),
    };
    b.line(&format!(
        "var lib_{} func({}){}",
        p.go_name,
        lib_params.join(", "),
        lib_ret
    ));
    b.line(&format!("var once_{} sync.Once", p.go_name));
    b.blank();

    b.line(&header);
    b.indent();
    b.line(&format!(
        "once_{n}.Do(func() {{ azRegister(&lib_{n}, \"{s}\") }})",
        n = p.go_name,
        s = p.bound_symbol()
    ));

    let mut call_args: Vec<String> = Vec::with_capacity(p.purego_arity());
    if p.returns_via_out_pointer() {
        b.line(&format!("var azRet {}", ret_go));
        call_args.push("&azRet".into());
    }
    call_args.extend(p.args.iter().map(ArgPlan::call_expr));
    let call = format!("lib_{}({})", p.go_name, call_args.join(", "));
    match &p.ret {
        Ret::Void => b.line(&call),
        Ret::Value { .. } => b.line(&format!("return {}", call)),
        Ret::Aggregate { .. } => {
            b.line(&call);
            b.line("return azRet");
        }
    }
    b.dedent();
    b.line("}");
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

/// Generate the contents of `functions.go`.
pub fn generate(ir: &CodegenIR, config: &CodegenConfig) -> Result<String> {
    let mut b = CodeBuilder::new(&config.indent);
    b.line("// ============================================================================");
    b.line("// functions.go - raw calls, one Go function per libazul export (purego).");
    b.line("// Auto-generated by azul-doc codegen v2 (lang_go). DO NOT EDIT MANUALLY.");
    b.line("// ============================================================================");
    b.line("//");
    b.line("// Each function binds its libazul symbol on first use (sync.Once + azRegister,");
    b.line("// see azul.go), so LoadLibrary stays cheap and unused exports never resolve.");
    b.line("// Owned aggregates (structs, tagged unions) cross by pointer through the");
    b.line("// exported *Byref twins and are CONSUMED by the callee; aggregate returns");
    b.line("// arrive through the twin's leading out-pointer. C enums and primitives");
    b.line("// cross by value, pointers as typed Go pointers (purego keeps them alive");
    b.line("// for the duration of the call).");
    b.blank();
    b.line("package azul");
    b.blank();
    b.line("import (");
    b.line("    \"sync\"");
    b.line("    \"unsafe\"");
    b.line(")");
    b.blank();
    b.line("var _ unsafe.Pointer");
    b.blank();
    for p in plans(ir, config) {
        emit_go_function(&mut b, &p);
        b.blank();
    }
    Ok(b.finish())
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
        ir.functions.push(func("AzDom_createBody", "Dom", vec![], Some("Dom")));
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
        // A C enum, an alias and a fn-pointer typedef by value: no twin.
        ir.functions.push(func(
            "AzDom_setUpdate",
            "Dom",
            vec![
                arg("dom", "Dom", ArgRefKind::RefMut),
                arg("update", "Update", ArgRefKind::Owned),
                arg("code", "ScanCode", ArgRefKind::Owned),
                arg("cb", "CallbackType", ArgRefKind::Owned),
            ],
            Some("Update"),
        ));
        let many: Vec<FunctionArg> = (0..16)
            .map(|i| arg(&format!("a{i}"), "u32", ArgRefKind::Owned))
            .collect();
        ir.functions.push(func("AzDom_sixteen", "Dom", many, None));
        let mut ctor = func("AzOptionDom_Some", "OptionDom", vec![], Some("OptionDom"));
        ctor.kind = FunctionKind::EnumVariantConstructor;
        ir.functions.push(ctor);
        ir
    }

    fn gen() -> String {
        generate(&ir_with_functions(), &CodegenConfig::c_header()).unwrap()
    }

    #[test]
    fn owned_aggregate_goes_through_byref_twin() {
        let src = gen();
        assert!(src.contains("var lib_AzDom_addChild func(*AzDom, *AzDom)\n"), "{src}");
        assert!(src.contains(
            "func AzDom_addChild(dom *AzDom, child AzDom) {\n    once_AzDom_addChild.Do(func() { azRegister(&lib_AzDom_addChild, \"AzDom_addChildByref\") })\n    lib_AzDom_addChild(dom, &child)\n}"
        ), "{src}");
        // Return through the leading out-pointer, argument consumed by address.
        assert!(src.contains("var lib_AzOptionDom_take func(*AzDom, *AzOptionDom)\n"), "{src}");
        assert!(src.contains(
            "func AzOptionDom_take(opt AzOptionDom) AzDom {\n    once_AzOptionDom_take.Do(func() { azRegister(&lib_AzOptionDom_take, \"AzOptionDom_takeByref\") })\n    var azRet AzDom\n    lib_AzOptionDom_take(&azRet, &opt)\n    return azRet\n}"
        ), "{src}");
    }

    #[test]
    fn aggregate_return_without_aggregate_args_uses_the_byref_twin_too() {
        let src = gen();
        assert!(src.contains("var lib_AzDom_createBody func(*AzDom)\n"), "{src}");
        assert!(src.contains(
            "func AzDom_createBody() AzDom {\n    once_AzDom_createBody.Do(func() { azRegister(&lib_AzDom_createBody, \"AzDom_createBodyByref\") })\n    var azRet AzDom\n    lib_AzDom_createBody(&azRet)\n    return azRet\n}"
        ), "{src}");
        assert!(!src.contains("azgo_"), "{src}");
    }

    #[test]
    fn scalars_enums_aliases_and_pointers_are_direct() {
        let src = gen();
        assert!(src.contains("var lib_AzDom_len func(*AzDom, bool) uintptr\n"), "{src}");
        assert!(src.contains(
            "func AzDom_len(dom *AzDom, flag bool) uintptr {\n    once_AzDom_len.Do(func() { azRegister(&lib_AzDom_len, \"AzDom_len\") })\n    return lib_AzDom_len(dom, flag)\n}"
        ), "{src}");
        assert!(src.contains("var lib_AzDom_ptr func(*AzDom) unsafe.Pointer\n"), "{src}");
        // A C enum, an alias and a fn-pointer typedef are not aggregates:
        // by value, direct symbol, enum returned by value.
        assert!(src.contains("var lib_AzDom_setUpdate func(*AzDom, AzUpdate, AzScanCode, AzCallbackType) AzUpdate\n"), "{src}");
        assert!(src.contains("azRegister(&lib_AzDom_setUpdate, \"AzDom_setUpdate\")"), "{src}");
        assert!(src.contains("return lib_AzDom_setUpdate(dom, update, code, cb)"), "{src}");
    }

    #[test]
    fn registration_is_lazy_and_pointers_are_typed() {
        let src = gen();
        assert!(!src.contains("initFunctions"), "{src}");
        assert!(!src.contains("uintptr(unsafe.Pointer"), "{src}");
        assert!(!src.contains("import \"C\""), "{src}");
        // One `var once_<fn> sync.Once` per fixture function (the header
        // comment mentions the type once more, so count declarations only).
        assert_eq!(src.matches(" sync.Once\n").count(), 6, "{src}");
    }

    #[test]
    fn beyond_purego_arity_the_body_panics_instead_of_binding() {
        let src = gen();
        assert!(src.contains("func AzDom_sixteen(a0 uint32, a1 uint32"), "{src}");
        assert!(src.contains("panic(\"azul: AzDom_sixteen takes 16 arguments, purego supports at most 15\")"), "{src}");
        assert!(!src.contains("lib_AzDom_sixteen"), "{src}");
    }

    #[test]
    fn enum_variant_constructors_are_not_surfaced() {
        let src = gen();
        assert!(!src.contains("AzOptionDom_Some"), "{src}");
    }

    #[test]
    fn byref_predicate_is_the_shared_ir_one() {
        let ir = fixture_ir();
        assert!(is_byref_aggregate(&arg("x", "Dom", ArgRefKind::Owned), &ir));
        assert!(is_byref_aggregate(&arg("x", "OptionDom", ArgRefKind::Owned), &ir));
        assert!(!is_byref_aggregate(&arg("x", "Update", ArgRefKind::Owned), &ir));
        assert!(!is_byref_aggregate(&arg("x", "ScanCode", ArgRefKind::Owned), &ir));
        assert!(!is_byref_aggregate(&arg("x", "CallbackType", ArgRefKind::Owned), &ir));
        assert!(!is_byref_aggregate(&arg("x", "u32", ArgRefKind::Owned), &ir));
        assert!(!is_byref_aggregate(&arg("x", "Dom", ArgRefKind::Ref), &ir));
    }
}
