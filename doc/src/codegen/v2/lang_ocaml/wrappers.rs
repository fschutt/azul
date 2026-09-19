//! Idiomatic OCaml wrappers: smart-constructed records with
//! `Gc.finalise` finalisers and a nested `Azul` module hierarchy.
//!
//! For every IR struct that has a matching `<TypeName>_delete` C function
//! (a *wrapped* struct) we emit a record `type app = { mutable raw : az_app
//! Ctypes.structure; mutable disposed : bool }` with `make_app` (arms a
//! `Gc.finalise` that calls `AzApp_delete` once), `dispose_app` and
//! `raw_app` (see `emit_wrapper_record_impl`).
//!
//! On top of that, one nested module per IR class (`Azul.App`, `Azul.Dom`,
//! ...) whose surface is derived from the IR by these type-driven rules —
//! they hold for every class of the given shape, never for a named one:
//!
//! - **Methods** (`emit_method_impl` / `build_method_signature`): an Owned
//!   `String` arg is an OCaml `string`; an Owned arg of a wrapped struct is
//!   that struct's record, passed as `.raw` and marked `disposed` after the
//!   call (the C ABI moved the bytes, so the finaliser must not free them
//!   again — this includes `self` passed by value); a unit enum is its ADT
//!   `<Enum>.t`; a returned wrapped struct comes back as its record.
//! - **`create_raw`**: the IR constructor named `create` (or `new`), 1:1.
//! - **`create`** (`SmartCtor`): ONE smart constructor per class with such a
//!   constructor: `?<x>` per self-consuming `with_<x>` builder (callback
//!   builders per `smart_callback_setter_info`, typed by the callback's
//!   typedef), `?<kind>` for a class matching `layout_callback_factory_info`,
//!   `~<name>` for composite (struct / RefAny) constructor args — a lone
//!   `RefAny` arg is the host model, `~model`, and is wrapped for the user —
//!   positional scalars, and a trailing `()` when nothing is positional.
//!   When the class has a `dom()` conversion, `create` returns that `dom`.
//! - **Tag helpers** (`TagHelper`): `create_<tag>()` on a class with
//!   `with_child` becomes `<tag> ~children`; `create_<tag>_with_text(text)`
//!   becomes `<tag> ?css text` (`?css` when the class has `with_css`).
//!
//! Enum modules (`emit_enum_modules_for`) live in their own units, below the
//! per-class modules, so every class module can name `<Enum>.t`.

use std::collections::BTreeSet;

use anyhow::Result;

use super::{
    super::{
        config::CodegenConfig,
        generator::CodeBuilder,
        ir::{
            ArgRefKind, CallbackTypedefDef, CodegenIR, EnumDef, EnumVariantKind, FieldRefKind,
            FunctionArg, FunctionDef, FunctionKind, StructDef, TypeCategory,
        },
        managed_host_invoker::{
            callback_typedef_for, layout_callback_factory_info, smart_callback_setter_info,
            wrapper_name,
        },
    },
    functions::ocaml_binding_name,
    inner_pointer_form_type,
    managed::{
        callback_kind_label, invoker_arg, invoker_arg_type, layout_factory_fn_name,
        register_fn_name, InvokerArg,
    },
    map_type_to_ocaml_typ, ocaml_ffi_type_name, ocaml_module_name, ocaml_wrapper_type_name,
    sanitize_doc, sanitize_identifier, to_snake_case, unit_enum_module,
};

// ============================================================================
// Entry points
// ============================================================================

/// The structs that get a wrapper record: wrappable AND owning native memory
/// (a `_delete` export). Shared by every emitter that must agree on "is this
/// type a record or a raw structure" — records, methods, smart constructors,
/// and the managed prelude.
pub fn record_types<'a>(ir: &'a CodegenIR, config: &CodegenConfig) -> BTreeSet<&'a str> {
    let delete_set = collect_delete_targets(ir);
    ir.structs
        .iter()
        .filter(|s| should_wrap(s, config) && delete_set.contains(s.name.as_str()))
        .map(|s| s.name.as_str())
        .collect()
}

pub fn emit_idiomatic_module_interface_for(
    builder: &mut CodeBuilder,
    ir: &CodegenIR,
    config: &CodegenConfig,
    belongs: &dyn Fn(&str) -> bool,
) -> Result<()> {
    builder
        .line("(* -------------------------------------------------------------------------- *)");
    builder
        .line("(* Idiomatic per-class submodules (interface).                                *)");
    builder
        .line("(*                                                                            *)");
    builder.line("(* The Dune library name `azul` causes this file to be reachable as the      *)");
    builder.line("(* `Azul` module from the outside; the per-class submodules below appear as  *)");
    builder
        .line("(* `Azul.App`, `Azul.WindowCreateOptions`, etc.                               *)");
    builder
        .line("(* -------------------------------------------------------------------------- *)");
    builder.blank();

    let records = record_types(ir, config);
    for s in ir.structs.iter().filter(|s| belongs(&s.name)) {
        if !should_wrap(s, config) || !class_has_visible_methods(&s.name, ir) {
            continue;
        }
        let plan = ClassPlan::build(s, ir, &records);
        plan.emit_interface(builder);
    }

    builder.blank();
    Ok(())
}

pub fn emit_wrapper_records_for(
    builder: &mut CodeBuilder,
    ir: &CodegenIR,
    config: &CodegenConfig,
    belongs: &dyn Fn(&str) -> bool,
) -> Result<()> {
    builder
        .line("(* -------------------------------------------------------------------------- *)");
    builder
        .line("(* Wrapper records + Gc.finalise finalisers.                                  *)");
    builder
        .line("(* -------------------------------------------------------------------------- *)");
    builder.blank();

    let records = record_types(ir, config);
    for s in ir.structs.iter().filter(|s| belongs(&s.name)) {
        if !records.contains(s.name.as_str()) {
            continue;
        }
        emit_wrapper_record_impl(builder, s);
    }
    Ok(())
}

pub fn emit_idiomatic_module_implementation_for(
    builder: &mut CodeBuilder,
    ir: &CodegenIR,
    config: &CodegenConfig,
    belongs: &dyn Fn(&str) -> bool,
) -> Result<()> {
    builder
        .line("(* -------------------------------------------------------------------------- *)");
    builder
        .line("(* Idiomatic per-class submodules (implementation).                           *)");
    builder
        .line("(* -------------------------------------------------------------------------- *)");
    builder.blank();

    // Polymorphic-variant views — must be defined identically to the
    // .mli so the interface matches the implementation. Without these
    // `dune build` fails with
    //   The type az_foo_view is required but not provided.
    emit_union_variant_interface(builder, ir, config, belongs);

    let records = record_types(ir, config);
    for s in ir.structs.iter().filter(|s| belongs(&s.name)) {
        if !should_wrap(s, config) || !class_has_visible_methods(&s.name, ir) {
            continue;
        }
        let plan = ClassPlan::build(s, ir, &records);
        plan.emit_implementation(builder);
    }

    builder.blank();
    Ok(())
}

/// `azul_enums_<module>.ml`: one module per enum of the api.json module —
/// the ADT of every unit enum (`type t = | DoNothing | RefreshDom`,
/// `to_int` / `of_int`) and the derive capabilities of every enum, all
/// typed on `t`. No `.mli`: the whole surface is the interface.
pub fn emit_enum_modules_for(
    builder: &mut CodeBuilder,
    ir: &CodegenIR,
    config: &CodegenConfig,
    belongs: &dyn Fn(&str) -> bool,
) -> Result<()> {
    for e in ir.enums.iter().filter(|e| belongs(&e.name)) {
        if !config.should_include_type(&e.name) || !e.generic_params.is_empty() {
            continue;
        }
        if !e.is_union {
            // Same predicate as `map_type_to_ocaml_typ`'s `<Enum>.t`, so a
            // signature never names a module this layer did not emit.
            if unit_enum_module(&e.name, ir).is_some() {
                emit_unit_enum_module(builder, e, ir);
            }
            continue;
        }
        if matches!(
            e.category,
            TypeCategory::Recursive
                | TypeCategory::GenericTemplate
                | TypeCategory::DestructorOrClone
        ) {
            continue;
        }
        emit_union_enum_module(builder, e, ir);
    }
    Ok(())
}

// ============================================================================
// Filters
// ============================================================================

fn should_wrap(s: &StructDef, config: &CodegenConfig) -> bool {
    if !config.should_include_type(&s.name) {
        return false;
    }
    if !s.generic_params.is_empty() {
        return false;
    }
    !matches!(
        s.category,
        TypeCategory::Recursive
            | TypeCategory::VecRef
            | TypeCategory::DestructorOrClone
            | TypeCategory::GenericTemplate
    )
}

fn collect_delete_targets(ir: &CodegenIR) -> BTreeSet<&str> {
    ir.functions
        .iter()
        .filter(|f| f.kind == FunctionKind::Delete)
        .map(|f| f.class_name.as_str())
        .collect()
}

fn class_has_visible_methods(class_name: &str, ir: &CodegenIR) -> bool {
    // Trait-only classes ARE admitted. When first tried the module could
    // carry only `equal` / `hash` / `to_string`, so every other derive a
    // class declared flipped from "absent" to "missing" and the gap grew
    // 272 -> 1321. The module now carries the whole list - equal, hash,
    // to_string, compare, partial_compare, clone, default - so a class with
    // nothing but trait exports is strictly better off with one.
    ir.functions_for_class(class_name).next().is_some()
}

// ============================================================================
// Wrapper records (.ml)
// ============================================================================

fn emit_wrapper_record_impl(builder: &mut CodeBuilder, s: &StructDef) {
    let wrapper = ocaml_wrapper_type_name(&s.name);
    let ffi = ocaml_ffi_type_name(&s.name);
    // The C `_delete` symbol is `Az<TypeName>_delete`; the OCaml-side
    // `foreign` binding is named by `to_snake_case` of that symbol.
    // Delete bindings go through `ocaml_binding_name` (the `ffi_`
    // prefix) so we route through the actual foreign-imported value.
    let delete_binding = ocaml_binding_name(&format!("Az{}_delete", s.name));

    if !s.doc.is_empty() {
        for d in &s.doc {
            builder.line(&format!("(* {} *)", sanitize_doc(d)));
        }
    }
    builder.line(&format!(
        "type {} = {{ mutable raw : {} Ctypes.structure; mutable disposed : bool }}",
        wrapper, ffi
    ));
    builder.line(&format!(
        "let make_{} (raw : {} Ctypes.structure) : {} =",
        wrapper, ffi, wrapper
    ));
    builder.indent();
    builder.line("let r = { raw; disposed = false } in");
    builder.line("Gc.finalise");
    builder.line("  (fun a ->");
    builder.line("     if not a.disposed then begin");
    // _delete usually expects a pointer to the FFI struct.
    builder.line(&format!(
        "       (try {} (Ctypes.addr a.raw) with _ -> ());",
        delete_binding
    ));
    builder.line("       a.disposed <- true");
    builder.line("     end)");
    builder.line("  r;");
    builder.line("r");
    builder.dedent();
    builder.blank();

    builder.line(&format!(
        "let dispose_{} (a : {}) : unit =",
        wrapper, wrapper
    ));
    builder.indent();
    builder.line("if not a.disposed then begin");
    builder.indent();
    builder.line(&format!(
        "(try {} (Ctypes.addr a.raw) with _ -> ());",
        delete_binding
    ));
    builder.line("a.disposed <- true");
    builder.dedent();
    builder.line("end");
    builder.dedent();
    builder.blank();

    builder.line(&format!(
        "let raw_{} (a : {}) : {} Ctypes.structure = a.raw",
        wrapper, wrapper, ffi
    ));
    builder.blank();
}

// ============================================================================
// Class plan: what the .mli and the .ml must agree on
// ============================================================================

/// How one user argument crosses the wrapper boundary. Type-driven.
enum ArgPass {
    /// Owned IR `String`: accept `string`, pass `(azul_az_string x)`.
    String,
    /// Unit enum: accept `<Enum>.t`, pass `(<Enum>.to_int x)`.
    UnitEnum(String),
    /// Owned wrapped struct: accept its record, pass `x.raw`, mark it
    /// `disposed` after the call (the C ABI moved the bytes).
    Record(String),
    /// Everything else: the Ctypes value as-is.
    Raw,
}

struct ArgPlan {
    /// OCaml identifier (snake, keyword-safe).
    id: String,
    /// Type in the `val` signature.
    sig: String,
    pass: ArgPass,
    /// Scalar args stay positional in smart constructors; composites are
    /// labelled.
    scalar: bool,
}

fn is_primitive(t: &str) -> bool {
    matches!(
        t,
        "bool"
            | "u8"
            | "c_uchar"
            | "i8"
            | "c_char"
            | "char"
            | "u16"
            | "i16"
            | "u32"
            | "c_uint"
            | "i32"
            | "c_int"
            | "u64"
            | "i64"
            | "f32"
            | "f64"
            | "usize"
            | "isize"
    )
}

impl ArgPlan {
    fn build(a: &FunctionArg, ir: &CodegenIR, records: &BTreeSet<&str>) -> ArgPlan {
        let id = sanitize_identifier(&to_snake_case(&a.name));
        let t = a.type_name.trim();
        let (sig, pass, scalar) = match a.ref_kind {
            ArgRefKind::Owned => {
                if t == "String" {
                    ("string".to_string(), ArgPass::String, true)
                } else if records.contains(t) {
                    let r = ocaml_wrapper_type_name(t);
                    (r.clone(), ArgPass::Record(r), false)
                } else if let Some(m) = unit_enum_module(t, ir) {
                    (format!("{}.t", m), ArgPass::UnitEnum(m), true)
                } else {
                    (map_type_to_ocaml_typ(t, ir), ArgPass::Raw, is_primitive(t))
                }
            }
            ArgRefKind::Ref | ArgRefKind::RefMut | ArgRefKind::Ptr | ArgRefKind::PtrMut => {
                (inner_pointer_form_type(t, ir), ArgPass::Raw, false)
            }
        };
        ArgPlan {
            id,
            sig,
            pass,
            scalar,
        }
    }

    /// The parameter as written after `let f`.
    fn param(&self) -> String {
        match &self.pass {
            ArgPass::Record(r) => format!("({} : {})", self.id, r),
            _ => self.id.clone(),
        }
    }

    /// The expression handed to the `foreign` binding.
    fn call_expr(&self) -> String {
        match &self.pass {
            ArgPass::String => format!("(azul_az_string {})", self.id),
            ArgPass::UnitEnum(m) => format!("({}.to_int {})", m, self.id),
            ArgPass::Record(_) => format!("{}.raw", self.id),
            ArgPass::Raw => self.id.clone(),
        }
    }

    fn consumes(&self) -> bool {
        matches!(self.pass, ArgPass::Record(_))
    }
}

/// How a return value comes back. Type-driven.
enum RetPlan {
    Unit,
    /// A wrapped struct: `make_<record>` arms the finaliser.
    Record(String),
    /// A unit enum: decoded to `<Enum>.t`.
    UnitEnum(String),
    /// Anything else: the Ctypes value as-is.
    Raw(String),
}

impl RetPlan {
    fn build(return_type: Option<&str>, ir: &CodegenIR, records: &BTreeSet<&str>) -> RetPlan {
        let t = return_type.map(str::trim).unwrap_or("");
        if matches!(t, "" | "void" | "()" | "c_void") {
            return RetPlan::Unit;
        }
        if records.contains(t) {
            return RetPlan::Record(ocaml_wrapper_type_name(t));
        }
        if let Some(m) = unit_enum_module(t, ir) {
            return RetPlan::UnitEnum(m);
        }
        RetPlan::Raw(map_type_to_ocaml_typ(t, ir))
    }

    fn sig(&self) -> String {
        match self {
            RetPlan::Unit => "unit".to_string(),
            RetPlan::Record(r) => r.clone(),
            RetPlan::UnitEnum(m) => format!("{}.t", m),
            RetPlan::Raw(t) => t.clone(),
        }
    }

    /// Wrap the raw call expression into the surfaced value.
    fn wrap(&self, call: &str) -> String {
        match self {
            RetPlan::Unit | RetPlan::Raw(_) => call.to_string(),
            RetPlan::Record(r) => format!("make_{} ({})", r, call),
            RetPlan::UnitEnum(m) => format!(
                "(match {}.of_int ({}) with | Some x -> x | None -> failwith \"invalid {} \
                 discriminant\")",
                m, call, m
            ),
        }
    }

    /// The inverse, for values a host closure hands back to libazul: a
    /// record is moved out (`.raw`) and consumed; everything else is the
    /// raw value already.
    fn to_raw(&self, expr: &str) -> String {
        match self {
            RetPlan::Record(r) => format!(
                "(let (__d : {}) = {} in let __r = __d.raw in __d.disposed <- true; __r)",
                r, expr
            ),
            _ => expr.to_string(),
        }
    }
}

/// One wrapper method, planned once for both files.
struct MethodPlan<'a> {
    func: &'a FunctionDef,
    name: String,
    takes_self: bool,
    self_by_value: bool,
    args: Vec<ArgPlan>,
    ret: RetPlan,
    /// `t` when the method returns its own class, else `ret.sig()`.
    ret_sig: String,
}

impl<'a> MethodPlan<'a> {
    fn build(
        func: &'a FunctionDef,
        class_name: &str,
        ir: &CodegenIR,
        records: &BTreeSet<&str>,
    ) -> MethodPlan<'a> {
        let takes_self = matches!(
            func.kind,
            FunctionKind::Method | FunctionKind::MethodMut | FunctionKind::DeepCopy
        );
        // For instance methods the first IR arg IS the implicit self — skip
        // it regardless of how api.json named it.
        let user_args: Vec<&FunctionArg> = func
            .args
            .iter()
            .skip(if takes_self && !func.args.is_empty() { 1 } else { 0 })
            .filter(|a| !func.is_receiver_arg(a))
            .collect();
        let self_by_value = takes_self
            && func
                .args
                .first()
                .map(|a| matches!(a.ref_kind, ArgRefKind::Owned))
                .unwrap_or(false);
        let ret = RetPlan::build(func.return_type.as_deref(), ir, records);
        let returns_self = func
            .return_type
            .as_deref()
            .map(|r| r.trim() == class_name)
            .unwrap_or(false);
        MethodPlan {
            func,
            name: method_emission_name(func, ir),
            takes_self,
            self_by_value,
            args: user_args
                .into_iter()
                .map(|a| ArgPlan::build(a, ir, records))
                .collect(),
            ret_sig: if returns_self {
                "t".to_string()
            } else {
                ret.sig()
            },
            ret,
        }
    }

    fn signature(&self) -> String {
        let mut atoms: Vec<String> = Vec::new();
        if self.takes_self {
            atoms.push("t".to_string());
        }
        atoms.extend(self.args.iter().map(|a| a.sig.clone()));
        if atoms.is_empty() {
            atoms.push("unit".to_string());
        }
        format!("val {} : {} -> {}", self.name, atoms.join(" -> "), self.ret_sig)
    }

    fn implementation(&self, has_wrapper: bool) -> String {
        let mut params: Vec<String> = Vec::new();
        let mut call_args: Vec<String> = Vec::new();
        let mut consumed: Vec<String> = Vec::new();
        if self.takes_self {
            // Type-annotate `self` so OCaml resolves the `.raw` / `.disposed`
            // field access to THIS wrapper's record (all wrappers share the
            // field labels).
            params.push("(self : t)".to_string());
            call_args.push(match (has_wrapper, self.self_by_value) {
                (true, true) => "self.raw".to_string(),
                (true, false) => "(Ctypes.addr self.raw)".to_string(),
                (false, true) => "self".to_string(),
                (false, false) => "(Ctypes.addr self)".to_string(),
            });
            if has_wrapper && self.self_by_value {
                consumed.push("self".to_string());
            }
        }
        for a in &self.args {
            params.push(a.param());
            call_args.push(a.call_expr());
            if a.consumes() {
                consumed.push(a.id.clone());
            }
        }
        let param_str = if params.is_empty() {
            "()".to_string()
        } else {
            params.join(" ")
        };
        let raw_binding = ocaml_binding_name(&self.func.c_name);
        let call = if call_args.is_empty() {
            format!("{} ()", raw_binding)
        } else {
            format!("{} {}", raw_binding, call_args.join(" "))
        };
        let value = self.ret.wrap(&call);
        // Consume AFTER the call: the C ABI took the bytes by value, so the
        // record's `Gc.finalise` must not re-fire `Az<X>_delete` on them.
        let consume_str: String = consumed
            .iter()
            .map(|c| format!("{}.disposed <- true", c))
            .collect::<Vec<_>>()
            .join("; ");
        let body = if consumed.is_empty() {
            value
        } else if matches!(self.ret, RetPlan::Unit) {
            format!("{}; {}", value, consume_str)
        } else {
            format!("let __ret = {} in {}; __ret", value, consume_str)
        };
        format!("let {} {} = {}", self.name, param_str, body)
    }
}

// ----------------------------------------------------------------------------
// Smart constructor
// ----------------------------------------------------------------------------

/// What `create` builds on.
enum BaseCtor {
    /// The IR constructor, reachable as `create_raw`.
    Raw { name: String },
    /// A class matching `layout_callback_factory_info`: `_default()` plus
    /// the host-invoker splice, through `azul_<class>_with_layout`.
    Layout {
        label: String,
        closure_sig: String,
        adapter: String,
        factory: String,
        default_ctor: String,
    },
}

/// One of the base constructor's own arguments.
enum CtorArg {
    Positional(ArgPlan),
    Labelled(ArgPlan),
    /// The lone `RefAny` of the constructor: the host model, wrapped for the
    /// caller.
    Model { label: String },
}

enum OptionKind {
    /// `?label:<sig>` -> `<method> __obj v`.
    Value { method: String, sig: String },
    /// `?label:<closure_sig>` -> `<method> __obj (azul_refany_create ())
    /// (<register> <adapter>)`.
    Callback {
        method: String,
        closure_sig: String,
        adapter: String,
        register: String,
    },
}

struct CtorOption {
    label: String,
    kind: OptionKind,
}

struct SmartCtor {
    base: BaseCtor,
    options: Vec<CtorOption>,
    args: Vec<CtorArg>,
    /// `(<dom method>, its return)` when the class converts to a DOM.
    finish: Option<(String, RetPlan)>,
    ret_sig: String,
}

fn is_ctor_kind(kind: FunctionKind) -> bool {
    matches!(kind, FunctionKind::Constructor | FunctionKind::StaticMethod)
}

/// The IR constructor `create` surfaces (as `create_raw`): the one named
/// `create`, else the one named `new`. Must return the class.
fn base_constructor<'a>(class_name: &'a str, ir: &'a CodegenIR) -> Option<&'a FunctionDef> {
    let returns_class = |f: &FunctionDef| {
        f.return_type
            .as_deref()
            .map(|r| r.trim() == class_name)
            .unwrap_or(false)
    };
    let has_literal_create = ir
        .functions_for_class(class_name)
        .any(|f| is_ctor_kind(f.kind) && f.method_name == "create" && returns_class(f));
    ir.functions_for_class(class_name).find(|f| {
        is_ctor_kind(f.kind)
            && returns_class(f)
            && (f.method_name == "create" || (f.method_name == "new" && !has_literal_create))
    })
}

fn is_base_constructor(func: &FunctionDef, ir: &CodegenIR) -> bool {
    base_constructor(&func.class_name, ir)
        .map(|b| std::ptr::eq(b, func))
        .unwrap_or(false)
}

/// The host closure of a `with_<x>(self, data: RefAny, cb)` builder. The
/// constructor supplies the `RefAny` itself (a unit), so the closure never
/// sees it. For the standard `(data, info)` pair with a unit-enum return the
/// info is dropped too: `unit -> Update.t`. Every other kind gets its real
/// typed argument list (the invoker's view, minus the RefAny) and return.
fn setter_closure(cb: &CallbackTypedefDef, ir: &CodegenIR, records: &BTreeSet<&str>) -> (String, String) {
    let args: Vec<InvokerArg> = cb.args.iter().map(|a| invoker_arg(a, ir)).collect();
    let ret = RetPlan::build(cb.return_type.as_deref(), ir, records);
    let standard_pair = args.len() == 2 && matches!(args[0], InvokerArg::Model);
    if standard_pair && matches!(ret, RetPlan::UnitEnum(_)) {
        return (
            format!("unit -> {}", ret.sig()),
            "(fun _ _ -> __f ())".to_string(),
        );
    }
    let mut params: Vec<String> = Vec::new();
    let mut names: Vec<String> = Vec::new();
    let mut sig: Vec<String> = Vec::new();
    for (i, a) in args.iter().enumerate() {
        if matches!(a, InvokerArg::Model) {
            params.push("_".to_string());
            continue;
        }
        let n = format!("__a{}", i);
        params.push(n.clone());
        names.push(n);
        sig.push(invoker_arg_type(a));
    }
    if names.is_empty() {
        names.push("()".to_string());
        sig.push("unit".to_string());
    }
    if params.is_empty() {
        params.push("()".to_string());
    }
    sig.push(ret.sig());
    let call = ret.to_raw(&format!("__f {}", names.join(" ")));
    (
        sig.join(" -> "),
        format!("(fun {} -> {})", params.join(" "), call),
    )
}

/// The host closure of a layout-callback factory: it receives the app model
/// (`'m`, the `RefAny` `App.create ~model` wrapped) and returns the DOM. For
/// the standard `(data, info)` pair the info is dropped: `'m -> dom`; extra
/// payload args, if any, are passed typed.
fn layout_closure(cb: &CallbackTypedefDef, ir: &CodegenIR, records: &BTreeSet<&str>) -> (String, String) {
    let args: Vec<InvokerArg> = cb.args.iter().map(|a| invoker_arg(a, ir)).collect();
    let ret = RetPlan::build(cb.return_type.as_deref(), ir, records);
    let drop_info = args.len() == 2 && matches!(args[0], InvokerArg::Model);
    let mut params: Vec<String> = Vec::new();
    let mut names: Vec<String> = Vec::new();
    let mut sig: Vec<String> = Vec::new();
    for (i, a) in args.iter().enumerate() {
        match a {
            InvokerArg::Model => {
                params.push("__m".to_string());
                names.push("__m".to_string());
                sig.push("'m".to_string());
            }
            _ if drop_info => params.push("_".to_string()),
            _ => {
                let n = format!("__a{}", i);
                params.push(n.clone());
                names.push(n);
                sig.push(invoker_arg_type(a));
            }
        }
    }
    if names.is_empty() {
        names.push("()".to_string());
        sig.push("unit".to_string());
    }
    if params.is_empty() {
        params.push("()".to_string());
    }
    sig.push(ret.sig());
    let call = ret.to_raw(&format!("__f {}", names.join(" ")));
    (
        sig.join(" -> "),
        format!("(fun {} -> {})", params.join(" "), call),
    )
}

/// A self-consuming `with_<x>` builder returning the class becomes the
/// optional `?<x>` of `create`.
fn builder_option(
    f: &FunctionDef,
    s: &StructDef,
    ir: &CodegenIR,
    records: &BTreeSet<&str>,
) -> Option<CtorOption> {
    if !matches!(
        f.kind,
        FunctionKind::Method | FunctionKind::MethodMut | FunctionKind::DeepCopy
    ) {
        return None;
    }
    if f.return_type.as_deref().map(str::trim) != Some(s.name.as_str()) {
        return None;
    }
    let raw_label = f.method_name.strip_prefix("with_")?;
    let label = sanitize_identifier(&to_snake_case(raw_label));
    let method = method_emission_name(f, ir);
    if let Some((_, wrapper)) = smart_callback_setter_info(f) {
        let cb = ir
            .callback_typedefs
            .iter()
            .find(|c| c.name == callback_typedef_for(&wrapper))?;
        let (closure_sig, adapter) = setter_closure(cb, ir, records);
        return Some(CtorOption {
            label,
            kind: OptionKind::Callback {
                method,
                closure_sig,
                adapter,
                register: register_fn_name(&wrapper),
            },
        });
    }
    if f.args.len() != 2 || !f.is_receiver_arg(&f.args[0]) {
        return None;
    }
    let arg = ArgPlan::build(&f.args[1], ir, records);
    Some(CtorOption {
        label,
        kind: OptionKind::Value {
            method,
            sig: arg.sig,
        },
    })
}

/// The `dom(self) -> <wrapped struct>` conversion of a widget class.
fn dom_conversion(s: &StructDef, ir: &CodegenIR, records: &BTreeSet<&str>) -> Option<(String, RetPlan)> {
    let f = ir.functions_for_class(&s.name).find(|f| {
        f.method_name == "dom"
            && matches!(
                f.kind,
                FunctionKind::Method | FunctionKind::MethodMut | FunctionKind::DeepCopy
            )
            && f.args.len() == 1
            && f.is_receiver_arg(&f.args[0])
            && f.return_type
                .as_deref()
                .map(|r| r.trim() != s.name && records.contains(r.trim()))
                .unwrap_or(false)
    })?;
    Some((
        method_emission_name(f, ir),
        RetPlan::build(f.return_type.as_deref(), ir, records),
    ))
}

impl SmartCtor {
    fn build(s: &StructDef, ir: &CodegenIR, records: &BTreeSet<&str>) -> Option<SmartCtor> {
        let base_func = base_constructor(&s.name, ir)?;
        let mut used: BTreeSet<String> = BTreeSet::new();
        let mut options: Vec<CtorOption> = Vec::new();
        let mut args: Vec<CtorArg> = Vec::new();

        let base = match layout_callback_factory_info(s, ir) {
            Some(info) => {
                let cb = ir
                    .callback_typedefs
                    .iter()
                    .find(|c| wrapper_name(c) == info.callback_wrapper)?;
                let default_func = ir
                    .functions
                    .iter()
                    .find(|f| f.c_name == info.default_c_name)?;
                let mut label = callback_kind_label(&info.callback_wrapper);
                if label.is_empty() {
                    label = sanitize_identifier(&to_snake_case(&base_func.args[0].name));
                }
                used.insert(label.clone());
                let (closure_sig, adapter) = layout_closure(cb, ir, records);
                BaseCtor::Layout {
                    label,
                    closure_sig,
                    adapter,
                    factory: layout_factory_fn_name(&info.class_name),
                    default_ctor: method_emission_name(default_func, ir),
                }
            }
            None => {
                let refany_count = base_func
                    .args
                    .iter()
                    .filter(|a| {
                        a.type_name.trim() == "RefAny" && matches!(a.ref_kind, ArgRefKind::Owned)
                    })
                    .count();
                for a in &base_func.args {
                    if a.type_name.trim() == "RefAny" && matches!(a.ref_kind, ArgRefKind::Owned) {
                        let label = if refany_count == 1 {
                            "model".to_string()
                        } else {
                            sanitize_identifier(&to_snake_case(&a.name))
                        };
                        used.insert(label.clone());
                        args.push(CtorArg::Model { label });
                        continue;
                    }
                    let plan = ArgPlan::build(a, ir, records);
                    used.insert(plan.id.clone());
                    if plan.scalar {
                        args.push(CtorArg::Positional(plan));
                    } else {
                        args.push(CtorArg::Labelled(plan));
                    }
                }
                BaseCtor::Raw {
                    name: method_emission_name(base_func, ir),
                }
            }
        };

        for f in ir.functions_for_class(&s.name) {
            let Some(opt) = builder_option(f, s, ir, records) else {
                continue;
            };
            // A builder whose label collides with a constructor argument (or
            // an earlier builder) cannot become a second parameter of the
            // same name; it stays reachable as the method itself.
            if !used.insert(opt.label.clone()) {
                continue;
            }
            options.push(opt);
        }

        let finish = dom_conversion(s, ir, records);
        let ret_sig = match &finish {
            Some((_, ret)) => ret.sig(),
            None => "t".to_string(),
        };
        Some(SmartCtor {
            base,
            options,
            args,
            finish,
            ret_sig,
        })
    }

    fn has_positional(&self) -> bool {
        self.args.iter().any(|a| matches!(a, CtorArg::Positional(_)))
    }

    fn signature(&self) -> String {
        let mut atoms: Vec<String> = Vec::new();
        if let BaseCtor::Layout {
            label, closure_sig, ..
        } = &self.base
        {
            atoms.push(format!("?{}:({})", label, closure_sig));
        }
        for o in &self.options {
            match &o.kind {
                OptionKind::Value { sig, .. } => atoms.push(format!("?{}:{}", o.label, sig)),
                OptionKind::Callback { closure_sig, .. } => {
                    atoms.push(format!("?{}:({})", o.label, closure_sig))
                }
            }
        }
        for a in &self.args {
            match a {
                CtorArg::Labelled(p) => atoms.push(format!("{}:{}", p.id, p.sig)),
                CtorArg::Model { label } => atoms.push(format!("{}:'a", label)),
                CtorArg::Positional(_) => {}
            }
        }
        for a in &self.args {
            if let CtorArg::Positional(p) = a {
                atoms.push(p.sig.clone());
            }
        }
        if !self.has_positional() {
            atoms.push("unit".to_string());
        }
        format!("val create : {} -> {}", atoms.join(" -> "), self.ret_sig)
    }

    fn emit_implementation(&self, builder: &mut CodeBuilder) {
        let mut params: Vec<String> = Vec::new();
        if let BaseCtor::Layout { label, .. } = &self.base {
            params.push(format!("?{}", label));
        }
        for o in &self.options {
            params.push(format!("?{}", o.label));
        }
        for a in &self.args {
            match a {
                CtorArg::Labelled(p) => params.push(format!("~{}", p.id)),
                CtorArg::Model { label } => params.push(format!("~{}", label)),
                CtorArg::Positional(_) => {}
            }
        }
        for a in &self.args {
            if let CtorArg::Positional(p) = a {
                params.push(p.id.clone());
            }
        }
        if !self.has_positional() {
            params.push("()".to_string());
        }
        builder.line(&format!(
            "let create {} : {} =",
            params.join(" "),
            self.ret_sig
        ));
        builder.indent();
        match &self.base {
            BaseCtor::Raw { name } => {
                let call_args: Vec<String> = self
                    .args
                    .iter()
                    .map(|a| match a {
                        CtorArg::Positional(p) | CtorArg::Labelled(p) => p.id.clone(),
                        CtorArg::Model { label } => format!("(azul_refany_create {})", label),
                    })
                    .collect();
                if call_args.is_empty() {
                    builder.line(&format!("let __obj = {} () in", name));
                } else {
                    builder.line(&format!(
                        "let __obj = {} {} in",
                        name,
                        call_args.join(" ")
                    ));
                }
            }
            BaseCtor::Layout {
                label,
                adapter,
                factory,
                default_ctor,
                ..
            } => {
                builder.line(&format!(
                    "let __obj = (match {} with | Some __f -> {} {} | None -> {} ()) in",
                    label, factory, adapter, default_ctor
                ));
            }
        }
        for o in &self.options {
            match &o.kind {
                OptionKind::Value { method, .. } => builder.line(&format!(
                    "let __obj = (match {} with | Some __v -> {} __obj __v | None -> __obj) in",
                    o.label, method
                )),
                OptionKind::Callback {
                    method,
                    adapter,
                    register,
                    ..
                } => builder.line(&format!(
                    "let __obj = (match {} with | Some __f -> {} __obj (azul_refany_create ()) \
                     ({} {}) | None -> __obj) in",
                    o.label, method, register, adapter
                )),
            }
        }
        match &self.finish {
            Some((method, _)) => builder.line(&format!("{} __obj", method)),
            None => builder.line("__obj"),
        }
        builder.dedent();
    }
}

// ----------------------------------------------------------------------------
// Tag helpers (Dom-shaped classes)
// ----------------------------------------------------------------------------

enum TagKind {
    /// `<tag> ~children`: `create_<tag> ()` folded with `with_child`.
    Container { child_method: String },
    /// `<tag> ?css text`: `create_<tag>_with_text text`, `?css` through
    /// `with_css` when the class has it.
    Text { css_method: Option<String> },
}

struct TagHelper {
    name: String,
    ctor: String,
    kind: TagKind,
}

/// A self-consuming `<name>(self, <arg>) -> Self` builder where `arg`
/// matches `pred`.
fn builder_named<'a>(
    s: &'a StructDef,
    ir: &'a CodegenIR,
    name: &str,
    pred: impl Fn(&FunctionArg) -> bool,
) -> Option<&'a FunctionDef> {
    ir.functions_for_class(&s.name).find(|f| {
        f.method_name == name
            && matches!(
                f.kind,
                FunctionKind::Method | FunctionKind::MethodMut | FunctionKind::DeepCopy
            )
            && f.args.len() == 2
            && f.is_receiver_arg(&f.args[0])
            && matches!(f.args[1].ref_kind, ArgRefKind::Owned)
            && pred(&f.args[1])
            && f.return_type.as_deref().map(str::trim) == Some(s.name.as_str())
    })
}

fn tag_helpers(s: &StructDef, ir: &CodegenIR, taken: &BTreeSet<String>) -> Vec<TagHelper> {
    let returns_class = |f: &FunctionDef| {
        f.return_type.as_deref().map(str::trim) == Some(s.name.as_str())
    };
    let with_child = builder_named(s, ir, "with_child", |a| a.type_name.trim() == s.name)
        .map(|f| method_emission_name(f, ir));
    let with_css = builder_named(s, ir, "with_css", |a| a.type_name.trim() == "String")
        .map(|f| method_emission_name(f, ir));
    let mut out: Vec<TagHelper> = Vec::new();
    let mut names: BTreeSet<String> = BTreeSet::new();
    let mut claim = |name: &str, names: &mut BTreeSet<String>| -> bool {
        !taken.contains(name) && names.insert(name.to_string())
    };
    // Text forms first: they own the bare tag name.
    for f in ir.functions_for_class(&s.name) {
        if !is_ctor_kind(f.kind) || !returns_class(f) || f.args.len() != 1 {
            continue;
        }
        if f.args[0].type_name.trim() != "String" || !matches!(f.args[0].ref_kind, ArgRefKind::Owned)
        {
            continue;
        }
        let Some(tag) = f
            .method_name
            .strip_prefix("create_")
            .and_then(|r| r.strip_suffix("_with_text"))
        else {
            continue;
        };
        let name = sanitize_identifier(&to_snake_case(tag));
        if !claim(&name, &mut names) {
            continue;
        }
        out.push(TagHelper {
            name,
            ctor: method_emission_name(f, ir),
            kind: TagKind::Text {
                css_method: with_css.clone(),
            },
        });
    }
    if let Some(child_method) = with_child {
        for f in ir.functions_for_class(&s.name) {
            if !is_ctor_kind(f.kind) || !returns_class(f) || !f.args.is_empty() {
                continue;
            }
            let Some(tag) = f.method_name.strip_prefix("create_") else {
                continue;
            };
            let name = sanitize_identifier(&to_snake_case(tag));
            if !claim(&name, &mut names) {
                continue;
            }
            out.push(TagHelper {
                name,
                ctor: method_emission_name(f, ir),
                kind: TagKind::Container {
                    child_method: child_method.clone(),
                },
            });
        }
    }
    out
}

impl TagHelper {
    fn signature(&self) -> String {
        match &self.kind {
            TagKind::Container { .. } => {
                format!("val {} : children:t list -> t", self.name)
            }
            TagKind::Text {
                css_method: Some(_),
            } => format!("val {} : ?css:string -> string -> t", self.name),
            TagKind::Text { css_method: None } => format!("val {} : string -> t", self.name),
        }
    }

    fn implementation(&self) -> String {
        match &self.kind {
            TagKind::Container { child_method } => format!(
                "let {} ~children : t = List.fold_left (fun __acc __c -> {} __acc __c) ({} ()) \
                 children",
                self.name, child_method, self.ctor
            ),
            TagKind::Text {
                css_method: Some(css),
            } => format!(
                "let {} ?css text : t = let __d = {} text in (match css with | Some __c -> {} __d \
                 __c | None -> __d)",
                self.name, self.ctor, css
            ),
            TagKind::Text { css_method: None } => {
                format!("let {} text : t = {} text", self.name, self.ctor)
            }
        }
    }
}

// ----------------------------------------------------------------------------
// The per-class module
// ----------------------------------------------------------------------------

struct ClassPlan<'a> {
    s: &'a StructDef,
    ir: &'a CodegenIR,
    module_name: String,
    wrapper: String,
    ffi: String,
    has_wrapper: bool,
    methods: Vec<MethodPlan<'a>>,
    smart: Option<SmartCtor>,
    tags: Vec<TagHelper>,
}

impl<'a> ClassPlan<'a> {
    fn build(s: &'a StructDef, ir: &'a CodegenIR, records: &BTreeSet<&str>) -> ClassPlan<'a> {
        let methods: Vec<MethodPlan<'a>> = ir
            .functions_for_class(&s.name)
            .filter(|f| !f.kind.is_trait_function())
            .map(|f| MethodPlan::build(f, &s.name, ir, records))
            .collect();
        let smart = SmartCtor::build(s, ir, records);
        let mut taken: BTreeSet<String> = methods.iter().map(|m| m.name.clone()).collect();
        for fixed in [
            "create",
            "equal",
            "hash",
            "to_string",
            "compare",
            "partial_compare",
            "to_list",
            "to_array",
        ] {
            taken.insert(fixed.to_string());
        }
        let tags = tag_helpers(s, ir, &taken);
        ClassPlan {
            s,
            ir,
            module_name: ocaml_module_name(&s.name),
            wrapper: ocaml_wrapper_type_name(&s.name),
            ffi: ocaml_ffi_type_name(&s.name),
            has_wrapper: records.contains(s.name.as_str()),
            methods,
            smart,
            tags,
        }
    }

    fn type_line(&self) -> String {
        if self.has_wrapper {
            format!("type t = {}", self.wrapper)
        } else {
            format!("type t = {} Ctypes.structure", self.ffi)
        }
    }

    fn emit_interface(&self, builder: &mut CodeBuilder) {
        let (s, ir) = (self.s, self.ir);
        builder.line(&format!("module {} : sig", self.module_name));
        builder.indent();
        builder.line(&self.type_line());
        if matches!(s.category, TypeCategory::String) {
            // Plain comment, not a docstring: a docstring here would be an
            // "ambiguous documentation comment" (escalated to error).
            builder.line("(* Decode the wrapped UTF-8 bytes into an OCaml string. *)");
            builder.line("val to_string : t -> string");
        }
        for m in &self.methods {
            builder.line(&m.signature());
        }
        if let Some(smart) = &self.smart {
            builder.line("(* Smart constructor: optional builders, labelled composites. *)");
            builder.line(&smart.signature());
        }
        for t in &self.tags {
            builder.line(&t.signature());
        }
        // The `.mli` SEALS the module: the implementation defines these, and
        // without the `val` a consumer can reach none of them. Same
        // predicates both sides so the two cannot drift.
        if ocaml_has_equal(s, ir) {
            builder.line("(* Equality routed through the C ABI. *)");
            builder.line("val equal : t -> t -> bool");
        }
        if ocaml_has_hash(s, ir) {
            builder.line("(* Hash routed through the C ABI. *)");
            builder.line("val hash : t -> int");
        }
        if ocaml_has_to_string(s, ir) {
            builder.line("(* Debug rendering routed through the C ABI. *)");
            builder.line("val to_string : t -> string");
        }
        if ocaml_has_compare(s, ir) {
            builder.line("(* Total order routed through the C ABI; OCaml convention. *)");
            builder.line("val compare : t -> t -> int");
        }
        if ocaml_has_partial_compare(s, ir) {
            builder.line("(* Partial order routed through the C ABI; None = incomparable. *)");
            builder.line("val partial_compare : t -> t -> int option");
        }
        emit_ocaml_vec_to_list_signature_if_vec(builder, s, ir);
        emit_ocaml_vec_to_array_signature_if_primitive(builder, s);
        builder.dedent();
        builder.line("end");
        builder.blank();
    }

    fn emit_implementation(&self, builder: &mut CodeBuilder) {
        let (s, ir) = (self.s, self.ir);
        builder.line(&format!("module {} = struct", self.module_name));
        builder.indent();
        builder.line(&self.type_line());

        // AzString gets a `to_string` helper that decodes the wrapped UTF-8
        // bytes. AzString's C-side layout is `{ vec: AzU8Vec }`, AzU8Vec is
        // `{ ptr, len, cap, destructor }`; the field accessors come from
        // types.rs.
        if matches!(s.category, TypeCategory::String) {
            builder.line("(* Decode the wrapped UTF-8 bytes into an OCaml string. *)");
            builder.line("let to_string (self : t) : string =");
            builder.indent();
            builder.line("let raw = self.raw in");
            builder.line("let vec = Ctypes.getf raw az_string_field_vec in");
            builder.line("let vec_ptr = Ctypes.getf vec az_u8_vec_field_ptr in");
            builder.line(
                "let vec_len = Unsigned.Size_t.to_int (Ctypes.getf vec az_u8_vec_field_len) in",
            );
            builder.line("if Ctypes.is_null vec_ptr || vec_len = 0 then \"\"");
            builder.line(
                "else Ctypes.string_from_ptr (Ctypes.from_voidp Ctypes.char vec_ptr) \
                 ~length:vec_len",
            );
            builder.dedent();
            builder.blank();
        }

        for m in &self.methods {
            for d in &m.func.doc {
                builder.line(&format!("(* {} *)", sanitize_doc(d)));
            }
            builder.line(&m.implementation(self.has_wrapper));
        }

        if let Some(smart) = &self.smart {
            builder.line("(* Smart constructor: optional builders, labelled composites. *)");
            smart.emit_implementation(builder);
        }
        for t in &self.tags {
            builder.line(&t.implementation());
        }

        emit_ocaml_eq_hash_if_supported(builder, s, ir, self.has_wrapper);
        emit_ocaml_compare_if_supported(builder, s, ir, self.has_wrapper);
        emit_ocaml_partial_compare_if_supported(builder, s, ir, self.has_wrapper);
        emit_ocaml_to_string_if_supported(builder, s, ir, self.has_wrapper);
        emit_ocaml_vec_to_list_if_vec(builder, s, ir, self.has_wrapper);
        emit_ocaml_vec_to_array_if_primitive(builder, s, self.has_wrapper);

        builder.dedent();
        builder.line("end");
        builder.blank();
    }
}

// ============================================================================
// Vec helpers, derive helpers
// ============================================================================

/// V7 (OCaml) — `.mli` signature for `to_list` when this struct is a Vec
/// wrapper (`TypeCategory::Vec`) AND the element type has a `_clone`
/// export OR is a primitive type. Returns the raw FFI element type;
/// users wrap manually via the element module's `make_*` if needed
/// (the module-emit order doesn't guarantee the element wrapper
/// module is in scope when the Vec module is being declared).
fn emit_ocaml_vec_to_list_signature_if_vec(
    builder: &mut CodeBuilder,
    s: &StructDef,
    ir: &CodegenIR,
) {
    let Some(spec) = detect_vec_to_list_shape(s, ir) else {
        return;
    };
    builder.line(&format!(
        "(* Yield a Lua-style {} list cloned out of the Vec — each element is *)",
        spec.return_doc
    ));
    builder.line("(* an independent heap allocation that survives the Vec being closed. *)");
    builder.line(&format!("val to_list : t -> {}", spec.return_type));
}

/// V7 (OCaml) — `.ml` implementation for `to_list`. Walks the Vec's
/// ptr/len fields, clones each element (when available) into a fresh
/// allocation, returns an OCaml list. Per-element shape matches Lua's
/// `to_lua_array` clone-via path (`lang_lua/wrappers.rs:244-311`).
fn emit_ocaml_vec_to_list_if_vec(
    builder: &mut CodeBuilder,
    s: &StructDef,
    ir: &CodegenIR,
    has_wrapper: bool,
) {
    let Some(spec) = detect_vec_to_list_shape(s, ir) else {
        return;
    };
    let vec_snake = ocaml_ffi_type_name(&s.name);
    let self_t = if has_wrapper { "self.raw" } else { "self" };

    builder.line(&format!(
        "(* Clone each element into an OCaml {} list. *)",
        spec.return_doc
    ));
    builder.line(&format!("let to_list (self : t) : {} =", spec.return_type));
    builder.indent();
    builder.line(&format!(
        "let __ptr = Ctypes.getf {} {}_field_ptr in",
        self_t, vec_snake
    ));
    builder.line(&format!(
        "let __len = Unsigned.Size_t.to_int (Ctypes.getf {} {}_field_len) in",
        self_t, vec_snake
    ));
    builder.line("if Ctypes.is_null __ptr || __len = 0 then []");
    builder.line("else");
    builder.indent();
    builder.line("let rec __aux i acc =");
    builder.indent();
    builder.line("if i < 0 then acc");
    builder.line("else");
    builder.indent();
    builder.line(&format!("let __elem = {} in", spec.element_expr));
    builder.line("__aux (i - 1) (__elem :: acc)");
    builder.dedent();
    builder.dedent();
    builder.line("in __aux (__len - 1) []");
    builder.dedent();
    builder.dedent();
    builder.blank();
}

/// Per-element extraction recipe for a Vec to_list emitter. Built once
/// from the IR; used by both signature and impl emitters so the .mli
/// and .ml stay consistent.
struct VecToListSpec {
    /// OCaml return type after `t ->` in the val-declaration / let-impl.
    /// `int list`, `float list`, `az_dom Ctypes.structure list`, etc.
    return_type: String,
    /// Short human description for the docstring (`"int"`, `"AzDom"`).
    return_doc: String,
    /// OCaml expression that yields the i-th element of the Vec (uses
    /// `__ptr` and `i` as free variables). Either a primitive deref or
    /// a `c_AzElem_clone` invocation.
    element_expr: String,
}

/// Decide what shape `to_list` should take for this struct. Returns
/// `None` when the struct isn't a Vec, when we can't detect the
/// element type, or when the element has no `_clone` export AND isn't
/// a primitive (we don't want to emit a dangling-by-design iterator).
fn detect_vec_to_list_shape(s: &StructDef, ir: &CodegenIR) -> Option<VecToListSpec> {
    // `TypeCategory::Vec` is unreliable: the IR builder only stamps it
    // on the four "C-API direct" Vec types (`AzU8Vec`, `AzStringVec`,
    // `AzGLuintVec`, `AzGLintVec`) plus on non-Vec C-API-direct types
    // like `StringMenuItem` (`ir_builder.rs:2225-2254`). For the dozens
    // of `Az<X>Vec` types that fall through to `TypeCategory::Regular`
    // we'd skip emission. Use the actual struct layout instead — Vecs
    // are uniformly `{ ptr: *const T, len: usize, cap: usize, destructor }`.
    let first = s.fields.first()?;
    let second = s.fields.get(1)?;
    if first.name != "ptr" || !matches!(first.ref_kind, FieldRefKind::Ptr | FieldRefKind::PtrMut) {
        return None;
    }
    if second.name != "len" || second.type_name.trim() != "usize" {
        return None;
    }
    let elem_rust = first.type_name.trim().to_string();

    // Skip primitive elements: the field accessor's ptr type is
    // `ptr void` (the OCaml types codegen drops to that fallback when
    // the element isn't an `az_<X>` ctype view), so a raw `!@(__ptr +@ i)`
    // dereferences a void pointer to `unit`. Untangling that needs a
    // per-primitive `Ctypes.from_voidp <view>` cast and a Ctypes-native
    // return type (`Unsigned.UInt8.t list` etc.), which falls outside
    // the V7 handoff scope ("per-element clone via `c_AzElem_clone`").
    // The four primitive-keyed Vecs (`U8Vec`, `U32Vec`, `F32Vec`, …)
    // get a follow-up entry in `VEC_ITERATOR_PLAN_2026_05_15.md`.
    if ocaml_primitive_for_rust(&elem_rust).is_some() {
        return None;
    }

    // A RECURSIVE element is emitted as an opaque pointer
    // (`type az_xml_node_child = (unit, [ `C ]) Ctypes_static.pointer`), not a
    // `Ctypes.structure`, so this helper's `structure list` shape does not
    // type-check against it. `ocamlfind ocamlc` catches it:
    //   This expression has type az_xml_node_child list
    //   but an expression was expected of type
    //     az_xml_node_child Ctypes.structure list
    // These Vecs became eligible only once the recursive-type carve-out let
    // their element export `_clone`; the carve-out is right, this helper just
    // does not apply to a pointer-shaped element.
    if ir
        .find_struct(&elem_rust)
        .is_some_and(|e| e.category == TypeCategory::Recursive)
        || ir
            .find_enum(&elem_rust)
            .is_some_and(|e| e.category == TypeCategory::Recursive)
    {
        return None;
    }

    // Wrapper-class element: call `Az<Elem>_clone` so the yielded
    // element owns independent heap allocations. Without `_clone` we
    // skip — handing the user a `Ctypes.structure` over the Vec's
    // internal buffer would dangle as soon as the Vec is closed.
    let has_clone = ir
        .functions
        .iter()
        .any(|f| f.class_name == elem_rust && matches!(f.kind, FunctionKind::DeepCopy));
    if !has_clone {
        return None;
    }
    let elem_ffi = ocaml_ffi_type_name(&elem_rust);
    let clone_binding = ocaml_binding_name(&format!("Az{}_clone", elem_rust));
    Some(VecToListSpec {
        return_type: format!("{} Ctypes.structure list", elem_ffi),
        return_doc: format!("Az{}", elem_rust),
        // `Ctypes.(+@) __ptr i` is pointer arithmetic — element-sized
        // offset from `__ptr`. The clone returns the struct by value
        // so the list entry is independent of the Vec's backing buffer.
        element_expr: format!("{} (Ctypes.(+@) __ptr i)", clone_binding),
    })
}

/// Map a primitive Rust type name to its OCaml-native equivalent + a
/// short doc word. Returns `None` for non-primitive types.
fn ocaml_primitive_for_rust(rust: &str) -> Option<(&'static str, &'static str)> {
    Some(match rust {
        "bool" => ("bool", "bool"),
        "u8" | "i8" | "u16" | "i16" | "u32" | "i32" | "usize" | "isize" => ("int", "int"),
        "u64" => ("Unsigned.uint64", "uint64"),
        "i64" => ("Signed.int64", "int64"),
        "f32" | "f64" => ("float", "float"),
        _ => return None,
    })
}

/// V7.2 (OCaml) — per-primitive-Vec emission recipe. Built once from
/// the IR; consumed by both signature and impl emitters so the .mli
/// and .ml agree on naming + types.
struct PrimVecArraySpec {
    /// OCaml return type after `t ->` (`bytes`, `int array`, `float array`).
    return_type: &'static str,
    /// Short doc word (`Bytes.t`, `int array`, `float array`).
    return_doc: &'static str,
    /// Ctypes view value used by `Ctypes.from_voidp <view> ptr`.
    ctypes_view: &'static str,
    /// OCaml expression mapping a single read element to the
    /// return-element type. `__elem` is the bound `let` over the
    /// ith read. Use `__elem` verbatim when no conversion needed.
    elem_to_ocaml: &'static str,
}

/// Decide what shape `to_array` should take for this struct. Returns
/// `None` when the struct isn't a Vec OR the element type isn't a
/// primitive (the wrapper-element path handles those via `to_list`).
fn detect_primitive_vec_array_shape(s: &StructDef) -> Option<PrimVecArraySpec> {
    // Same layout probe as `detect_vec_to_list_shape`: first field
    // must be `ptr` with a pointer ref_kind, second must be `len:
    // usize`. This catches the real Az<X>Vec types and rejects the
    // `TypeCategory::Vec`-tagged non-Vec sentinels (StringMenuItem,
    // InstantPtr, …).
    let first = s.fields.first()?;
    let second = s.fields.get(1)?;
    if first.name != "ptr" || !matches!(first.ref_kind, FieldRefKind::Ptr | FieldRefKind::PtrMut) {
        return None;
    }
    if second.name != "len" || second.type_name.trim() != "usize" {
        return None;
    }
    let elem_rust = first.type_name.trim();
    Some(match elem_rust {
        "u8" => PrimVecArraySpec {
            return_type: "bytes",
            return_doc: "Bytes.t",
            ctypes_view: "Ctypes.uint8_t",
            // u8 array decodes naturally to OCaml's `bytes` via
            // string_from_ptr + Bytes.of_string. Handled inline in
            // the emitter (special-cased) — the per-element formula
            // here is unused for u8.
            elem_to_ocaml: "",
        },
        "i8" => PrimVecArraySpec {
            return_type: "int array",
            return_doc: "int array",
            ctypes_view: "Ctypes.int8_t",
            elem_to_ocaml: "__elem",
        },
        "u16" => PrimVecArraySpec {
            return_type: "int array",
            return_doc: "int array",
            ctypes_view: "Ctypes.uint16_t",
            elem_to_ocaml: "Unsigned.UInt16.to_int __elem",
        },
        "i16" => PrimVecArraySpec {
            return_type: "int array",
            return_doc: "int array",
            ctypes_view: "Ctypes.int16_t",
            elem_to_ocaml: "__elem",
        },
        "u32" => PrimVecArraySpec {
            return_type: "int array",
            return_doc: "int array",
            ctypes_view: "Ctypes.uint32_t",
            elem_to_ocaml: "Unsigned.UInt32.to_int __elem",
        },
        "i32" => PrimVecArraySpec {
            return_type: "int array",
            return_doc: "int array",
            ctypes_view: "Ctypes.int32_t",
            elem_to_ocaml: "Signed.Int32.to_int __elem",
        },
        "u64" => PrimVecArraySpec {
            return_type: "Unsigned.UInt64.t array",
            return_doc: "uint64 array",
            ctypes_view: "Ctypes.uint64_t",
            elem_to_ocaml: "__elem",
        },
        "i64" => PrimVecArraySpec {
            return_type: "Signed.Int64.t array",
            return_doc: "int64 array",
            ctypes_view: "Ctypes.int64_t",
            elem_to_ocaml: "__elem",
        },
        "f32" => PrimVecArraySpec {
            return_type: "float array",
            return_doc: "float array",
            ctypes_view: "Ctypes.float",
            elem_to_ocaml: "__elem",
        },
        "f64" => PrimVecArraySpec {
            return_type: "float array",
            return_doc: "float array",
            ctypes_view: "Ctypes.double",
            elem_to_ocaml: "__elem",
        },
        _ => return None,
    })
}

/// V7.2 (OCaml) — `.mli` signature for `to_array` on primitive-element
/// Vecs. u8 surfaces as `bytes`; all other integer Vecs as `int array`
/// (lossy-narrowed for u64 — see the spec); float Vecs as `float array`.
fn emit_ocaml_vec_to_array_signature_if_primitive(builder: &mut CodeBuilder, s: &StructDef) {
    let Some(spec) = detect_primitive_vec_array_shape(s) else {
        return;
    };
    builder.line(&format!(
        "(* Bulk-copy the Vec's elements into an OCaml-native {}. *)",
        spec.return_doc
    ));
    builder.line(&format!("val to_array : t -> {}", spec.return_type));
}

/// V7.2 (OCaml) — `.ml` implementation for `to_array`. Casts the
/// void-pointer field to the typed Ctypes view, then either
/// `string_from_ptr` (u8 → bytes) or `Array.init` (all others).
fn emit_ocaml_vec_to_array_if_primitive(
    builder: &mut CodeBuilder,
    s: &StructDef,
    has_wrapper: bool,
) {
    let Some(spec) = detect_primitive_vec_array_shape(s) else {
        return;
    };
    let vec_snake = ocaml_ffi_type_name(&s.name);
    let self_t = if has_wrapper { "self.raw" } else { "self" };
    let is_u8 = spec.ctypes_view == "Ctypes.uint8_t";

    builder.line(&format!(
        "(* Bulk-copy the Vec's elements into an OCaml-native {}. *)",
        spec.return_doc
    ));
    builder.line(&format!("let to_array (self : t) : {} =", spec.return_type));
    builder.indent();
    builder.line(&format!(
        "let __ptr = Ctypes.getf {} {}_field_ptr in",
        self_t, vec_snake
    ));
    builder.line(&format!(
        "let __len = Unsigned.Size_t.to_int (Ctypes.getf {} {}_field_len) in",
        self_t, vec_snake
    ));
    if is_u8 {
        // u8 → bytes: `Ctypes.string_from_ptr` copies `__len` bytes
        // from the casted `char ptr` and returns an OCaml string;
        // wrap with `Bytes.of_string` so the signature gives `bytes`
        // (mutable byte array, matching Java/Kotlin/C# `byte[]`).
        builder.line("if Ctypes.is_null __ptr || __len = 0 then Bytes.empty");
        builder.line(
            "else Bytes.of_string (Ctypes.string_from_ptr (Ctypes.from_voidp Ctypes.char __ptr) \
             ~length:__len)",
        );
    } else {
        builder.line(&"if Ctypes.is_null __ptr || __len = 0 then [||]".to_string());
        builder.line("else");
        builder.indent();
        builder.line(&format!(
            "let __typed = Ctypes.from_voidp {} __ptr in",
            spec.ctypes_view
        ));
        builder.line("Array.init __len (fun i ->");
        builder.indent();
        builder.line("let __elem = Ctypes.(!@) (Ctypes.(+@) __typed i) in");
        builder.line(&format!("{})", spec.elem_to_ocaml));
        builder.dedent();
        builder.dedent();
    }
    builder.dedent();
    builder.blank();
}

/// Phase I.3.6 (OCaml): emit `to_string` per-module helper routed
/// through `Az<X>_toDbgString`. Decodes the returned AzString via the
/// existing `string_from_ptr` pattern from String.to_string. Skips the
/// String wrapper itself (already has the vec-direct decoder).
fn emit_ocaml_to_string_if_supported(
    builder: &mut CodeBuilder,
    s: &StructDef,
    ir: &CodegenIR,
    has_wrapper: bool,
) {
    if matches!(s.category, TypeCategory::String) {
        return;
    }
    let dbg_sym = format!("Az{}_toDbgString", s.name);
    let has_dbg = s.traits.is_debug && ir.functions.iter().any(|f| f.c_name == dbg_sym);
    if !has_dbg {
        return;
    }
    // Skip when the user-facing surface already defines `to_string`
    // (e.g. `AzUrl_toString` maps to `Url.to_string : t -> az_string`).
    // We can't override without breaking the .mli signature.
    if ir
        .functions
        .iter()
        .any(|f| f.class_name == s.name && idiomatic_method_name(&f.method_name) == "to_string")
    {
        return;
    }
    let self_t = if has_wrapper { "t.raw" } else { "t" };
    let raw_dbg = ocaml_binding_name(&dbg_sym);
    let raw_string_delete = ocaml_binding_name("AzString_delete");
    builder.line(&format!("(* String repr routed through {}. *)", dbg_sym));
    builder.line("let to_string (t : t) : string =");
    builder.indent();
    builder.line(&format!(
        "let __s = {} (Ctypes.addr {}) in",
        raw_dbg, self_t
    ));
    builder.line("let vec = Ctypes.getf __s az_string_field_vec in");
    builder.line("let vec_ptr = Ctypes.getf vec az_u8_vec_field_ptr in");
    builder.line("let vec_len = Unsigned.Size_t.to_int (Ctypes.getf vec az_u8_vec_field_len) in");
    builder.line(
        "let __out = if Ctypes.is_null vec_ptr || vec_len = 0 then \"\" else \
         Ctypes.string_from_ptr (Ctypes.from_voidp Ctypes.char vec_ptr) ~length:vec_len in",
    );
    // The AzString returned by value from toDbgString owns a heap
    // buffer; nothing else ever frees it (no wrapper, no finaliser).
    // Consume it here — the bytes were copied into __out above.
    builder.line(&format!("{} (Ctypes.addr __s);", raw_string_delete));
    builder.line("__out");
    builder.dedent();
    builder.blank();
}

/// Phase I.2.8 (OCaml): emit `equal` + `hash` module helpers routed
/// through the C-ABI `_partialEq` / `_hash` exports. Pure type-driven.
fn emit_ocaml_eq_hash_if_supported(
    builder: &mut CodeBuilder,
    s: &StructDef,
    ir: &CodegenIR,
    has_wrapper: bool,
) {
    let eq_sym = format!("Az{}_partialEq", s.name);
    let has_eq = s.traits.is_partial_eq && ir.functions.iter().any(|f| f.c_name == eq_sym);
    let hash_sym = format!("Az{}_hash", s.name);
    let has_hash = s.traits.is_hash && ir.functions.iter().any(|f| f.c_name == hash_sym);

    let self_a = if has_wrapper { "a.raw" } else { "a" };
    let self_b = if has_wrapper { "b.raw" } else { "b" };
    let self_t = if has_wrapper { "t.raw" } else { "t" };
    let raw_eq = ocaml_binding_name(&eq_sym);
    let raw_hash = ocaml_binding_name(&hash_sym);

    if has_eq {
        builder.line(&format!("(* Equality routed through {}. *)", eq_sym));
        builder.line("let equal (a : t) (b : t) : bool =");
        builder.indent();
        builder.line(&format!(
            "{} (Ctypes.addr {}) (Ctypes.addr {})",
            raw_eq, self_a, self_b
        ));
        builder.dedent();
        builder.blank();
    }

    if has_hash {
        builder.line(&format!("(* Hash routed through {}. *)", hash_sym));
        builder.line("let hash (t : t) : int =");
        builder.indent();
        builder.line(&format!(
            "Unsigned.UInt64.to_int ({} (Ctypes.addr {}))",
            raw_hash, self_t
        ));
        builder.dedent();
        builder.blank();
    }
}

// ============================================================================
// Polymorphic-variant signature for tagged unions
// ============================================================================

fn emit_union_variant_interface(
    builder: &mut CodeBuilder,
    ir: &CodegenIR,
    config: &CodegenConfig,
    belongs: &dyn Fn(&str) -> bool,
) {
    let mut emitted_header = false;
    for e in ir.enums.iter().filter(|e| belongs(&e.name)) {
        if !config.should_include_type(&e.name) {
            continue;
        }
        if !e.is_union {
            continue;
        }
        if !e.generic_params.is_empty() {
            continue;
        }
        if matches!(
            e.category,
            TypeCategory::Recursive
                | TypeCategory::DestructorOrClone
                | TypeCategory::GenericTemplate
        ) {
            continue;
        }
        if !emitted_header {
            builder.line("(* Polymorphic-variant views for tagged-union enums. The actual *)");
            builder.line("(* payload conversion lives in the implementation; here we expose *)");
            builder.line("(* the variant signature for pattern-matching. *)");
            emitted_header = true;
        }
        let view_name = format!("{}_view", ocaml_ffi_type_name(&e.name));
        builder.line(&format!("type {} = ", view_name));
        builder.indent();
        let mut first = true;
        for v in &e.variants {
            let lit = polymorphic_variant_literal(&v.name);
            let line = match &v.kind {
                EnumVariantKind::Unit => format!("`{}", lit),
                EnumVariantKind::Tuple(_) | EnumVariantKind::Struct(_) => {
                    // Payload-bearing variants are surfaced as opaque
                    // ints (offset into the FFI payload). Users go
                    // through the FFI struct directly when they need
                    // typed payload access.
                    format!("`{} of int", lit)
                }
            };
            if first {
                builder.line(&format!("[ {}", line));
                first = false;
            } else {
                builder.line(&format!("| {}", line));
            }
        }
        builder.line("]");
        builder.dedent();
    }
}


// ============================================================================
// Helpers
// ============================================================================

/// Pick an idiomatic OCaml method name from the api.json method name.
/// `new` is renamed to `create` (OCaml's `new` is a class-related
/// keyword; even though we don't use OCaml classes, `create` reads
/// better and matches the C# / Ada conventions).
fn idiomatic_method_name(method_name: &str) -> String {
    let snake = to_snake_case(method_name);
    if snake == "new" {
        return "create".to_string();
    }
    sanitize_identifier(&snake)
}

/// The name a method is emitted under, used by BOTH the `.mli` and the `.ml`
/// emitters (they must agree).
///
/// The class's base constructor (`create`, or `new` when there is no
/// `create` — see `base_constructor`) is emitted as `create_raw`: the smart
/// `create` takes the plain name. Everything else is the snake-cased api.json
/// name with the reserved-word guard (so a `new` next to a literal `create`
/// becomes `new_`).
fn method_emission_name(func: &FunctionDef, ir: &CodegenIR) -> String {
    if is_base_constructor(func, ir) {
        return "create_raw".to_string();
    }
    sanitize_identifier(&to_snake_case(&func.method_name))
}

/// Produce a polymorphic-variant tag literal. Backticks come from the
/// caller; this function ensures the tag itself is a valid OCaml
/// identifier (must start uppercase or be an identifier-like token).
fn polymorphic_variant_literal(name: &str) -> String {
    // Polymorphic variants accept any capitalised identifier.
    let mut chars = name.chars();
    match chars.next() {
        Some(c) if c.is_ascii_uppercase() => name.to_string(),
        Some(c) => {
            let mut out = String::with_capacity(name.len());
            out.extend(c.to_uppercase());
            out.push_str(chars.as_str());
            out
        }
        None => "Empty".to_string(),
    }
}


// ============================================================================
// Enum modules (the `azul_enums_<module>.ml` layer)
// ============================================================================

/// The derive capabilities a tagged-union enum exports, as a module
/// `type t = az_x Ctypes.structure` + `equal` / `hash` / `to_string` /
/// `compare` / `partial_compare` / `clone` / `default`.
fn emit_union_enum_module(builder: &mut CodeBuilder, e: &EnumDef, ir: &CodegenIR) {
    let caps: Vec<&FunctionDef> = ir
        .functions_for_class(&e.name)
        .filter(|f| {
            matches!(
                f.kind,
                FunctionKind::PartialEq
                    | FunctionKind::Hash
                    | FunctionKind::DebugToString
                    | FunctionKind::Default
                    | FunctionKind::DeepCopy
                    | FunctionKind::Cmp
                    | FunctionKind::PartialCmp
            )
        })
        .collect();
    if caps.is_empty() {
        return;
    }
    let module = ocaml_module_name(&e.name);
    let ffi = ocaml_ffi_type_name(&e.name);
    builder.line(&format!("module {} = struct", module));
    builder.indent();
    builder.line(&format!("type t = {} Ctypes.structure", ffi));
    let mut seen: Vec<FunctionKind> = Vec::new();
    for f in caps {
        if seen.contains(&f.kind) {
            continue;
        }
        seen.push(f.kind);
        let raw = ocaml_binding_name(&f.c_name);
        match f.kind {
            FunctionKind::Default => builder.line(&format!("let default () : t = {} ()", raw)),
            FunctionKind::DeepCopy => {
                builder.line(&format!("let clone (t : t) : t = {} (Ctypes.addr t)", raw))
            }
            FunctionKind::PartialCmp => {
                // NOT `compare`: the ABI answers 255 for "incomparable", and a
                // total `compare` has no honest value for that.
                builder.line("let partial_compare (a : t) (b : t) : int option =");
                builder.indent();
                builder.line(&format!(
                    "match Unsigned.UInt8.to_int ({} (Ctypes.addr a) (Ctypes.addr b)) with",
                    raw
                ));
                builder.line("| 0 -> Some (-1)");
                builder.line("| 1 -> Some 0");
                builder.line("| 2 -> Some 1");
                builder.line("| _ -> None");
                builder.dedent();
            }
            FunctionKind::Cmp => {
                // 0 = Less, 1 = Equal, 2 = Greater on the C side; OCaml's
                // `compare` wants negative / zero / positive.
                builder.line("let compare (a : t) (b : t) : int =");
                builder.indent();
                builder.line(&format!(
                    "match Unsigned.UInt8.to_int ({} (Ctypes.addr a) (Ctypes.addr b)) with",
                    raw
                ));
                builder.line("| 0 -> -1");
                builder.line("| 1 -> 0");
                builder.line("| _ -> 1");
                builder.dedent();
            }
            FunctionKind::PartialEq => builder.line(&format!(
                "let equal (a : t) (b : t) : bool = {} (Ctypes.addr a) (Ctypes.addr b)",
                raw
            )),
            FunctionKind::Hash => builder.line(&format!(
                "let hash (t : t) : int = Unsigned.UInt64.to_int ({} (Ctypes.addr t))",
                raw
            )),
            FunctionKind::DebugToString => {
                emit_az_string_decoder(builder, "to_string (t : t)", &format!("{} (Ctypes.addr t)", raw));
            }
            _ => {}
        }
    }
    builder.dedent();
    builder.line("end");
    builder.blank();
}

/// `let <header> : string = <decode the AzString the expression returns>`.
/// The AzString is returned by value and owns its heap buffer; nothing else
/// frees it, so it is deleted here once the bytes are copied out.
fn emit_az_string_decoder(builder: &mut CodeBuilder, header: &str, az_string_expr: &str) {
    let del = ocaml_binding_name("AzString_delete");
    builder.line(&format!("let {} : string =", header));
    builder.indent();
    builder.line(&format!("let __s = {} in", az_string_expr));
    builder.line("let vec = Ctypes.getf __s az_string_field_vec in");
    builder.line("let vec_ptr = Ctypes.getf vec az_u8_vec_field_ptr in");
    builder.line("let vec_len = Unsigned.Size_t.to_int (Ctypes.getf vec az_u8_vec_field_len) in");
    builder.line(
        "let __out = if Ctypes.is_null vec_ptr || vec_len = 0 then \"\" else \
         Ctypes.string_from_ptr (Ctypes.from_voidp Ctypes.char vec_ptr) ~length:vec_len in",
    );
    builder.line(&format!("{} (Ctypes.addr __s);", del));
    builder.line("__out");
    builder.dedent();
}

/// A unit-only enum's module: the ADT (`type t = | A | B`), `to_int` /
/// `of_int` pinning the C ABI numbering, and its derive capabilities, all
/// typed on `t`.
///
/// `type az_x = int` is the FFI view (types.rs); the entry points take
/// `ptr az_x`, so the capabilities `Ctypes.allocate` a cell holding
/// `to_int x` — an int is not addressable.
fn emit_unit_enum_module(builder: &mut CodeBuilder, e: &EnumDef, ir: &CodegenIR) {
    let module = ocaml_module_name(&e.name);
    let ffi = ocaml_ffi_type_name(&e.name);
    let variant = |v: &str| sanitize_identifier(&super::to_pascal_case(v));
    let cell = |x: &str| format!("(Ctypes.allocate {} (to_int {}))", ffi, x);

    for d in &e.doc {
        builder.line(&format!("(* {} *)", sanitize_doc(d)));
    }
    builder.line(&format!("module {} = struct", module));
    builder.indent();
    builder.line("type t =");
    builder.indent();
    for v in &e.variants {
        builder.line(&format!("| {}", variant(&v.name)));
    }
    builder.dedent();
    builder.line("let to_int (x : t) : int =");
    builder.indent();
    builder.line("match x with");
    for (idx, v) in e.variants.iter().enumerate() {
        builder.line(&format!("| {} -> {}", variant(&v.name), idx));
    }
    builder.dedent();
    builder.line("let of_int (x : int) : t option =");
    builder.indent();
    builder.line("match x with");
    for (idx, v) in e.variants.iter().enumerate() {
        builder.line(&format!("| {} -> Some {}", idx, variant(&v.name)));
    }
    builder.line("| _ -> None");
    builder.dedent();

    let mut seen: Vec<FunctionKind> = Vec::new();
    for f in ir.functions_for_class(&e.name) {
        if !matches!(
            f.kind,
            FunctionKind::PartialEq
                | FunctionKind::Hash
                | FunctionKind::DebugToString
                | FunctionKind::Cmp
                | FunctionKind::PartialCmp
                | FunctionKind::Default
        ) || seen.contains(&f.kind)
        {
            continue;
        }
        seen.push(f.kind);
        let raw = ocaml_binding_name(&f.c_name);
        match f.kind {
            FunctionKind::PartialEq => builder.line(&format!(
                "let equal (a : t) (b : t) : bool = {} {} {}",
                raw,
                cell("a"),
                cell("b")
            )),
            FunctionKind::Hash => builder.line(&format!(
                "let hash (x : t) : int = Unsigned.UInt64.to_int ({} {})",
                raw,
                cell("x")
            )),
            FunctionKind::DebugToString => {
                emit_az_string_decoder(builder, "to_string (x : t)", &format!("{} {}", raw, cell("x")));
            }
            FunctionKind::Cmp => {
                builder.line("let compare (a : t) (b : t) : int =");
                builder.indent();
                builder.line(&format!(
                    "match Unsigned.UInt8.to_int ({} {} {}) with",
                    raw,
                    cell("a"),
                    cell("b")
                ));
                builder.line("| 0 -> -1");
                builder.line("| 1 -> 0");
                builder.line("| _ -> 1");
                builder.dedent();
            }
            FunctionKind::PartialCmp => {
                builder.line("let partial_compare (a : t) (b : t) : int option =");
                builder.indent();
                builder.line(&format!(
                    "match Unsigned.UInt8.to_int ({} {} {}) with",
                    raw,
                    cell("a"),
                    cell("b")
                ));
                builder.line("| 0 -> Some (-1)");
                builder.line("| 1 -> Some 0");
                builder.line("| 2 -> Some 1");
                builder.line("| _ -> None");
                builder.dedent();
            }
            FunctionKind::Default => builder.line(&format!(
                "let default () : t = match of_int ({} ()) with | Some x -> x | None -> failwith \
                 \"invalid {} discriminant\"",
                raw, module
            )),
            _ => {}
        }
    }

    builder.dedent();
    builder.line("end");
    builder.blank();
}

/// Does this class get a module-level `compare`?
///
/// Shared by the `.ml` and the `.mli` on purpose: the two drifting is exactly
/// how OCaml came to define `equal`/`hash`/`to_string` that no consumer could
/// reach.
fn ocaml_has_compare(s: &StructDef, ir: &CodegenIR) -> bool {
    let sym = format!("Az{}_cmp", s.name);
    (s.traits.is_ord || s.traits.is_partial_ord) && ir.functions.iter().any(|f| f.c_name == sym)
}

/// `compare` routed through `Az<X>_cmp`.
///
/// The C ABI answers 0 = Less, 1 = Equal, 2 = Greater; OCaml's `compare`
/// wants negative / zero / positive. The two encodings are both total orders
/// over the same three outcomes, so the mapping is exact rather than invented:
/// 0 -> -1, 1 -> 0, 2 -> 1.
fn emit_ocaml_compare_if_supported(
    builder: &mut CodeBuilder,
    s: &StructDef,
    ir: &CodegenIR,
    has_wrapper: bool,
) {
    if !ocaml_has_compare(s, ir) {
        return;
    }
    let sym = format!("Az{}_cmp", s.name);
    let raw = ocaml_binding_name(&sym);
    let self_a = if has_wrapper { "a.raw" } else { "a" };
    let self_b = if has_wrapper { "b.raw" } else { "b" };
    builder.line(&format!("(* Total order routed through {}. *)", sym));
    builder.line("let compare (a : t) (b : t) : int =");
    builder.indent();
    builder.line(&format!(
        "match Unsigned.UInt8.to_int ({} (Ctypes.addr {}) (Ctypes.addr {})) with",
        raw, self_a, self_b
    ));
    builder.line("| 0 -> -1");
    builder.line("| 1 -> 0");
    builder.line("| _ -> 1");
    builder.dedent();
    builder.blank();
}

/// Does this class get a module-level `equal`? Shared by both emitters.
fn ocaml_has_equal(s: &StructDef, ir: &CodegenIR) -> bool {
    let sym = format!("Az{}_partialEq", s.name);
    s.traits.is_partial_eq && ir.functions.iter().any(|f| f.c_name == sym)
}

/// Does this class get a module-level `hash`? Shared by both emitters.
fn ocaml_has_hash(s: &StructDef, ir: &CodegenIR) -> bool {
    let sym = format!("Az{}_hash", s.name);
    s.traits.is_hash && ir.functions.iter().any(|f| f.c_name == sym)
}

/// Does this class get a module-level `to_string` routed through
/// `_toDbgString`? Not when it IS the string type, and not when the ordinary
/// surface already spells `to_string` - overriding would break the signature.
fn ocaml_has_to_string(s: &StructDef, ir: &CodegenIR) -> bool {
    if matches!(s.category, TypeCategory::String) {
        return false;
    }
    let sym = format!("Az{}_toDbgString", s.name);
    if !(s.traits.is_debug && ir.functions.iter().any(|f| f.c_name == sym)) {
        return false;
    }
    !ir.functions
        .iter()
        .any(|f| f.class_name == s.name && idiomatic_method_name(&f.method_name) == "to_string")
}

/// Does this class get `partial_compare`? Only when it exports `_partialCmp`
/// and NOT `_cmp` - a type with a total order already has `compare`, which is
/// the stronger and more idiomatic surface.
fn ocaml_has_partial_compare(s: &StructDef, ir: &CodegenIR) -> bool {
    if ocaml_has_compare(s, ir) {
        return false;
    }
    let sym = format!("Az{}_partialCmp", s.name);
    s.traits.is_partial_ord && ir.functions.iter().any(|f| f.c_name == sym)
}

/// `partial_compare` routed through `Az<X>_partialCmp`.
///
/// Deliberately NOT spelled `compare`. The ABI answers 255 for "incomparable"
/// (`None` on the Rust side) and OCaml's `compare : t -> t -> int` has no
/// honest value for that - any int it returned would assert an ordering the
/// type does not have. `int option` says exactly what PartialOrd means.
fn emit_ocaml_partial_compare_if_supported(
    builder: &mut CodeBuilder,
    s: &StructDef,
    ir: &CodegenIR,
    has_wrapper: bool,
) {
    if !ocaml_has_partial_compare(s, ir) {
        return;
    }
    let sym = format!("Az{}_partialCmp", s.name);
    let raw = ocaml_binding_name(&sym);
    let a = if has_wrapper { "a.raw" } else { "a" };
    let b = if has_wrapper { "b.raw" } else { "b" };
    builder.line(&format!("(* Partial order routed through {}. *)", sym));
    builder.line("let partial_compare (a : t) (b : t) : int option =");
    builder.indent();
    builder.line(&format!(
        "match Unsigned.UInt8.to_int ({} (Ctypes.addr {}) (Ctypes.addr {})) with",
        raw, a, b
    ));
    builder.line("| 0 -> Some (-1)");
    builder.line("| 1 -> Some 0");
    builder.line("| 2 -> Some 1");
    builder.line("| _ -> None");
    builder.dedent();
    builder.blank();
}
