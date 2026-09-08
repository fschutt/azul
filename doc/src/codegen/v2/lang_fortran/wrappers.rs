//! Idiomatic Fortran wrapper layer (F2003+), derived entirely from the IR.
//!
//! For every struct that is a *class* (has at least one C-API function)
//! we emit a derived type named `<snake>_t` (`dom_t`, `button_t`,
//! `app_t`, ...). The `_t` suffix exists because Fortran folds case:
//! `type(Button) :: button` would be illegal, `type(button_t) :: button`
//! is the ordinary spelling.
//!
//! ```fortran
//! type :: dom_t
//!   type(AzDom) :: raw
//!   logical :: owned = .false.
//! contains
//!   procedure :: delete => dom_delete
//!   procedure :: with_css => dom_with_css
//!   procedure :: with_child => dom_with_child
//! end type dom_t
//! ```
//!
//! - There is deliberately NO `final ::` subroutine: gfortran finalizes
//!   a function result after the assignment that consumed it, so a
//!   finalizer on a type returned by factories would `_delete` every
//!   value a factory ever returns. Cleanup is the explicit `delete`
//!   type-bound procedure, guarded by `owned`.
//! - Factories (constructors, `default`, static methods) are public
//!   module procedures named `<snake>_<method>`:
//!   `dom_create_p_with_text('5')`, `button_create('Increase counter')`.
//! - Instance methods are type-bound procedures. A method that consumes
//!   `self` and returns `Self` (`with_css`, `with_child`, ...) becomes an
//!   in-place SUBROUTINE (`call label%with_css('font-size: 32px;')`);
//!   any other consumer marks `self%owned = .false.` after the call.
//! - `String` arguments are `character(len=*)`, `String` results are
//!   `character(len=:), allocatable`; unit enums are plain `integer`
//!   with un-prefixed constants (`ButtonType_Primary`,
//!   `Update_RefreshDom`); `bool` is `logical`.
//! - A wrapper passed to a consuming (by-value) parameter is moved when
//!   it is owned and deep-copied when it is borrowed (the `RefAny` a
//!   callback receives), via the per-class `azul_take_<snake>` helper.
//! - Callback-wrapper arguments (`ButtonOnClickCallback`, ...) take a
//!   Fortran procedure matching the kind's typed abstract interface
//!   (see [`super::managed`]).

use std::collections::{BTreeMap, BTreeSet};

use anyhow::Result;

use super::super::config::CodegenConfig;
use super::super::generator::CodeBuilder;
use super::super::ir::{
    ArgRefKind, CallbackTypedefDef, CodegenIR, EnumDef, FunctionArg, FunctionDef, FunctionKind,
    StructDef, TypeCategory,
};
use super::super::managed_host_invoker::{
    host_invoker_kinds, layout_callback_factory_info, to_snake_case, wrapper_name,
    LayoutCallbackFactoryInfo,
};
use super::functions::{fortran_alias_for, should_emit_function};
use super::{
    ffi_type_name, map_type_to_fortran, sanitize_identifier, truncate_identifier,
    wrapper_type_name,
};

// ============================================================================
// Classification
// ============================================================================

/// How a type appears on the user-facing side of the wrapper layer.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum UserType {
    /// The `String` class: `character(len=*)` in, `character(:)` out.
    Str,
    /// A class with a `<snake>_t` wrapper (payload: IR struct name).
    Wrapper(String),
    /// A callback-wrapper struct from `HOST_INVOKER_KINDS` (payload:
    /// wrapper name, e.g. `ButtonOnClickCallback`): the user passes a
    /// procedure matching the kind's abstract interface.
    Kind(String),
    /// A unit enum: plain `integer`.
    Enum,
    /// `bool`: `logical`.
    Bool,
    /// Anything else, in its FFI spelling.
    Raw(String),
}

/// Structural facts about the `String` class needed to marshal
/// `character` values (all names come from the IR, not from literals).
pub(crate) struct StringClass {
    pub name: String,
    /// Fortran alias of `<String>_copyFromBytes(ptr, start, len)`.
    pub copy_alias: String,
    /// Fortran field names along `raw % vec % {ptr, len}`.
    pub vec_field: String,
    pub ptr_field: String,
    pub len_field: String,
}

/// One emitted module procedure for a class.
pub(crate) struct ProcPlan<'a> {
    pub func: &'a FunctionDef,
    /// Module procedure name (`dom_with_css`).
    pub name: String,
    /// Type-bound binding name when this is an instance method.
    pub binding: Option<String>,
    /// Set on the smart `<class>_create(layout)` factory that replaces
    /// the raw fn-pointer constructor.
    pub smart: Option<LayoutCallbackFactoryInfo>,
}

pub(crate) struct ClassPlan<'a> {
    pub s: &'a StructDef,
    pub snake: String,
    /// `<snake>_t`
    pub wt: String,
    pub delete_name: String,
    /// `azul_take_<snake>`, present when the class has a deep-copy fn.
    pub take_name: Option<String>,
    pub procs: Vec<ProcPlan<'a>>,
}

/// Everything the wrapper + managed emitters share: the wrapper set,
/// the string / RefAny classes, trait-function lookups and the module-
/// wide identifier table (Fortran folds case, so every public name is
/// claimed lower-cased exactly once).
pub(crate) struct Ctx<'a> {
    pub ir: &'a CodegenIR,
    pub config: &'a CodegenConfig,
    pub wrappers: BTreeSet<String>,
    pub kinds: Vec<&'a CallbackTypedefDef>,
    pub kind_names: BTreeSet<String>,
    pub string: Option<StringClass>,
    pub ref_any: Option<String>,
    pub delete: BTreeMap<String, &'a FunctionDef>,
    pub clone: BTreeMap<String, &'a FunctionDef>,
    pub classes: Vec<ClassPlan<'a>>,
    /// `(constant name, value)` for every included unit-enum variant.
    pub enum_consts: Vec<(String, usize)>,
    names: BTreeSet<String>,
}

impl<'a> Ctx<'a> {
    pub(crate) fn new(ir: &'a CodegenIR, config: &'a CodegenConfig) -> Self {
        let kinds: Vec<&CallbackTypedefDef> = host_invoker_kinds(ir).collect();
        let kind_names: BTreeSet<String> =
            kinds.iter().map(|cb| wrapper_name(cb).to_string()).collect();

        let mut delete = BTreeMap::new();
        let mut clone = BTreeMap::new();
        for f in &ir.functions {
            match f.kind {
                FunctionKind::Delete => {
                    delete.entry(f.class_name.clone()).or_insert(f);
                }
                FunctionKind::DeepCopy => {
                    clone.entry(f.class_name.clone()).or_insert(f);
                }
                _ => {}
            }
        }

        let string = find_string_class(ir);
        let ref_any = ir
            .structs
            .iter()
            .find(|s| s.category == TypeCategory::RefAny && delete.contains_key(&s.name))
            .map(|s| s.name.clone());

        let has_fn: BTreeSet<&str> = ir.functions.iter().map(|f| f.class_name.as_str()).collect();
        let wrappers: BTreeSet<String> = ir
            .structs
            .iter()
            .filter(|s| should_emit_wrapper(s, config))
            .filter(|s| has_fn.contains(s.name.as_str()))
            .filter(|s| string.as_ref().map(|st| st.name != s.name).unwrap_or(true))
            .filter(|s| !kind_names.contains(&s.name))
            .map(|s| s.name.clone())
            .collect();

        let mut ctx = Ctx {
            ir,
            config,
            wrappers,
            kinds,
            kind_names,
            string,
            ref_any,
            delete,
            clone,
            classes: Vec::new(),
            enum_consts: Vec::new(),
            names: BTreeSet::new(),
        };
        ctx.seed_names();
        ctx.plan_enum_consts();
        ctx.plan_classes();
        ctx
    }

    /// Reserve every identifier the FFI layer, the managed layer and
    /// the iso_c_binding re-exports already occupy.
    fn seed_names(&mut self) {
        for s in &self.ir.structs {
            self.names.insert(ffi_type_name(&s.name).to_lowercase());
        }
        for e in &self.ir.enums {
            let alias = ffi_type_name(&e.name);
            self.names.insert(alias.to_lowercase());
            for v in &e.variants {
                let n = truncate_identifier(&format!("{}_{}", alias, sanitize_identifier(&v.name)));
                self.names.insert(n.to_lowercase());
            }
        }
        for ta in &self.ir.type_aliases {
            self.names.insert(ffi_type_name(&ta.name).to_lowercase());
        }
        for cb in &self.ir.callback_typedefs {
            let n = ffi_type_name(&cb.name);
            self.names.insert(n.to_lowercase());
            self.names
                .insert(truncate_identifier(&format!("{}_iface", n)).to_lowercase());
        }
        for f in &self.ir.functions {
            self.names.insert(fortran_alias_for(&f.c_name).to_lowercase());
        }
        for n in super::ISO_C_REEXPORTS {
            self.names.insert(n.to_lowercase());
        }
        for n in super::managed::reserved_names(self) {
            self.names.insert(n.to_lowercase());
        }
        for n in [STRING_IN_HELPER, STRING_OUT_HELPER, "r", "self"] {
            self.names.insert(n.to_lowercase());
        }
    }

    /// Claim `want` (or `want_2`, `want_3`, ...) as a module-wide name.
    fn claim(&mut self, want: &str) -> String {
        let base = truncate_identifier(want);
        if self.names.insert(base.to_lowercase()) {
            return base;
        }
        let mut i = 2;
        loop {
            let cand = truncate_identifier(&format!("{}_{}", base, i));
            if self.names.insert(cand.to_lowercase()) {
                return cand;
            }
            i += 1;
        }
    }

    /// Claim `want` only if it is free; `None` on a case-folded clash.
    fn try_claim(&mut self, want: &str) -> Option<String> {
        let base = truncate_identifier(want);
        if self.names.insert(base.to_lowercase()) {
            Some(base)
        } else {
            None
        }
    }

    fn plan_enum_consts(&mut self) {
        let enums: Vec<&EnumDef> = self
            .ir
            .enums
            .iter()
            .filter(|e| should_emit_enum_consts(e, self.config))
            .collect();
        for e in enums {
            for (i, v) in e.variants.iter().enumerate() {
                let want = sanitize_identifier(&format!("{}_{}", e.name, v.name));
                if let Some(name) = self.try_claim(&want) {
                    self.enum_consts.push((name, i));
                }
            }
        }
    }

    fn plan_classes(&mut self) {
        let structs: Vec<&'a StructDef> = self
            .ir
            .structs
            .iter()
            .filter(|s| self.wrappers.contains(&s.name))
            .collect();
        // Wrapper type names first so no procedure can shadow one.
        let mut wts = Vec::with_capacity(structs.len());
        for s in &structs {
            wts.push(self.claim(&wrapper_type_name(&s.name)));
        }
        let mut classes = Vec::with_capacity(structs.len());
        for (s, wt) in structs.into_iter().zip(wts) {
            let snake = to_snake_case(&s.name);
            let delete_name = self.claim(&format!("{}_delete", snake));
            let take_name = if self.clone.contains_key(&s.name) {
                Some(self.claim(&format!("azul_take_{}", snake)))
            } else {
                None
            };
            let factory = layout_callback_factory_info(s, self.ir);
            let mut bindings: BTreeSet<String> = ["raw", "owned", "delete"]
                .iter()
                .map(|n| n.to_string())
                .collect();
            if self.ref_any.as_deref() == Some(s.name.as_str()) {
                bindings.insert("get".to_string());
            }
            let mut procs = Vec::new();
            let funcs: Vec<&'a FunctionDef> = self
                .ir
                .functions
                .iter()
                .filter(|f| f.class_name == s.name)
                .filter(|f| should_emit_function(f, self.ir, self.config))
                .filter(|f| !matches!(f.kind, FunctionKind::Delete | FunctionKind::EnumVariantConstructor))
                .collect();
            for f in funcs {
                let name = self.claim(&format!("{}_{}", snake, sanitize_identifier(&f.method_name)));
                let binding = if takes_self(f) {
                    let b = sanitize_identifier(&f.method_name);
                    if bindings.insert(b.to_lowercase()) {
                        Some(b)
                    } else {
                        None
                    }
                } else {
                    None
                };
                let smart = match &factory {
                    Some(info) if is_raw_layout_factory(f, info) => Some(info.clone()),
                    _ => None,
                };
                procs.push(ProcPlan {
                    func: f,
                    name,
                    binding,
                    smart,
                });
            }
            classes.push(ClassPlan {
                s,
                snake,
                wt,
                delete_name,
                take_name,
                procs,
            });
        }
        self.classes = classes;
    }

    pub(crate) fn classify(&self, type_name: &str) -> UserType {
        let t = type_name.trim();
        if self.kind_names.contains(t) {
            return UserType::Kind(t.to_string());
        }
        if let Some(st) = &self.string {
            if st.name == t {
                return UserType::Str;
            }
        }
        if self.wrappers.contains(t) {
            return UserType::Wrapper(t.to_string());
        }
        if t == "bool" {
            return UserType::Bool;
        }
        if let Some(e) = self.ir.find_enum(t) {
            if !e.is_union && e.generic_params.is_empty() {
                return UserType::Enum;
            }
        }
        UserType::Raw(map_type_to_fortran(t, self.ir))
    }

    pub(crate) fn class(&self, name: &str) -> Option<&ClassPlan<'a>> {
        self.classes.iter().find(|c| c.s.name == name)
    }

    /// `<snake>_t` for a wrapped struct.
    pub(crate) fn wt(&self, name: &str) -> String {
        self.class(name)
            .map(|c| c.wt.clone())
            .unwrap_or_else(|| wrapper_type_name(name))
    }

    /// The actual-argument expression that hands `var` (a `<snake>_t`)
    /// to a consuming C parameter: moved when owned, cloned when not.
    pub(crate) fn take_expr(&self, class: &str, var: &str) -> String {
        match self.class(class).and_then(|c| c.take_name.as_ref()) {
            Some(take) => format!("{}({})", take, var),
            None => format!("{}%raw", var),
        }
    }

    /// `call <Class>_delete(<raw>)` spelled for the delete fn's receiver
    /// kind (`raw_var` must have the `target` attribute).
    pub(crate) fn delete_call(&self, class: &str, raw_var: &str) -> Option<String> {
        let f = self.delete.get(class)?;
        let alias = fortran_alias_for(&f.c_name);
        let by_value = f
            .args
            .first()
            .map(|a| matches!(a.ref_kind, ArgRefKind::Owned))
            .unwrap_or(false);
        Some(if by_value {
            format!("call {}({})", alias, raw_var)
        } else {
            format!("call {}(c_loc({}))", alias, raw_var)
        })
    }
}

/// Name of the public `character(len=*) -> type(AzString)` helper.
pub(crate) const STRING_IN_HELPER: &str = "azul_string";
/// Name of the public `type(AzString) -> character(:)` helper
/// (consumes the AzString).
pub(crate) const STRING_OUT_HELPER: &str = "azul_string_value";

fn find_string_class(ir: &CodegenIR) -> Option<StringClass> {
    for s in ir.structs.iter().filter(|s| s.category == TypeCategory::String) {
        let copy = ir.functions.iter().find(|f| {
            f.class_name == s.name
                && f.method_name == "copy_from_bytes"
                && f.args.len() == 3
                && f.return_type.as_deref().map(|r| r.trim() == s.name).unwrap_or(false)
        });
        let Some(copy) = copy else { continue };
        if s.fields.len() != 1 {
            continue;
        }
        let vec_field = &s.fields[0];
        let Some(vec) = ir.find_struct(vec_field.type_name.trim()) else {
            continue;
        };
        let ptr = vec
            .fields
            .iter()
            .find(|f| f.type_name.trim().starts_with('*') && f.type_name.contains("u8"));
        let len = vec.fields.iter().find(|f| f.name == "len");
        if let (Some(ptr), Some(len)) = (ptr, len) {
            return Some(StringClass {
                name: s.name.clone(),
                copy_alias: fortran_alias_for(&copy.c_name),
                vec_field: sanitize_identifier(&vec_field.name),
                ptr_field: sanitize_identifier(&ptr.name),
                len_field: sanitize_identifier(&len.name),
            });
        }
    }
    None
}

fn should_emit_wrapper(s: &StructDef, config: &CodegenConfig) -> bool {
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

fn should_emit_enum_consts(e: &EnumDef, config: &CodegenConfig) -> bool {
    !e.is_union
        && e.generic_params.is_empty()
        && !e.variants.is_empty()
        && config.should_include_type(&e.name)
        && !matches!(
            e.category,
            TypeCategory::Recursive | TypeCategory::DestructorOrClone | TypeCategory::GenericTemplate
        )
}

/// Instance method: the receiver kinds AND `args[0]` is really of the
/// class type (api.json names it `self`, the snake-cased class, ...).
fn takes_self(f: &FunctionDef) -> bool {
    matches!(
        f.kind,
        FunctionKind::Method
            | FunctionKind::MethodMut
            | FunctionKind::DeepCopy
            | FunctionKind::PartialEq
            | FunctionKind::PartialCmp
            | FunctionKind::Cmp
            | FunctionKind::Hash
            | FunctionKind::DebugToString
    ) && f
        .args
        .first()
        .map(|a| a.type_name.trim() == f.class_name)
        .unwrap_or(false)
}

/// The raw `<Class>_create(<LayoutCallbackType fn ptr>)` factory that the
/// smart `create(layout)` replaces.
fn is_raw_layout_factory(f: &FunctionDef, info: &LayoutCallbackFactoryInfo) -> bool {
    matches!(f.kind, FunctionKind::Constructor | FunctionKind::StaticMethod)
        && f.args.len() == 1
        && f.args[0]
            .callback_info
            .as_ref()
            .map(|ci| ci.callback_wrapper_name == info.callback_wrapper)
            .unwrap_or(false)
        && f.return_type
            .as_deref()
            .map(|r| r.trim() == info.class_name)
            .unwrap_or(false)
}

/// Dummy-argument name: sanitized, and kept clear of the intrinsics and
/// helper names the generated bodies call.
pub(crate) fn dummy_name(name: &str) -> String {
    const CLASHES: &[&str] = &[
        "int", "nint", "char", "trim", "null", "present", "associated", "allocated", "index",
        "count", "sum", "mod", "sign", "float", "any", "all", "shape", "merge", "huge", "tiny",
        "self", "logical", "achar", "ichar", "adjustl", "repeat", "verify", "scan", "r",
    ];
    let base = if name.is_empty() {
        "arg".to_string()
    } else {
        sanitize_identifier(name)
    };
    if CLASHES.contains(&base.to_lowercase().as_str()) {
        format!("{}_", base)
    } else {
        base
    }
}

// ============================================================================
// Argument / return plans
// ============================================================================

pub(crate) struct ArgPlan {
    pub decls: Vec<String>,
    pub locals: Vec<String>,
    pub pre: Vec<String>,
    pub actual: String,
    pub post: Vec<String>,
}

/// Plan one user-facing (non-receiver) argument.
pub(crate) fn plan_arg(ctx: &Ctx, a: &FunctionArg) -> (String, ArgPlan) {
    let nm = dummy_name(&a.name);
    let owned = matches!(a.ref_kind, ArgRefKind::Owned);
    let mutable = matches!(a.ref_kind, ArgRefKind::RefMut | ArgRefKind::PtrMut);
    let intent = if mutable { "inout" } else { "in" };
    let mut plan = ArgPlan {
        decls: Vec::new(),
        locals: Vec::new(),
        pre: Vec::new(),
        actual: nm.clone(),
        post: Vec::new(),
    };
    match ctx.classify(&a.type_name) {
        UserType::Kind(k) if owned => {
            plan.decls
                .push(format!("procedure({}) :: {}", super::managed::iface_name(&k), nm));
            plan.actual = format!("{}({})", super::managed::register_name(&k), nm);
        }
        UserType::Str if owned => {
            plan.decls
                .push(format!("character(len=*), intent(in) :: {}", nm));
            plan.actual = format!("{}({})", STRING_IN_HELPER, nm);
        }
        UserType::Str => {
            let st = ctx.string.as_ref().expect("Str classified without a string class");
            let tmp = truncate_identifier(&format!("azul_tmp_{}", nm));
            plan.decls
                .push(format!("character(len=*), intent(in) :: {}", nm));
            plan.locals
                .push(format!("type({}), target :: {}", ffi_type_name(&st.name), tmp));
            plan.pre.push(format!("{} = {}({})", tmp, STRING_IN_HELPER, nm));
            plan.actual = format!("c_loc({})", tmp);
            if let Some(del) = ctx.delete_call(&st.name, &tmp) {
                plan.post.push(del);
            }
        }
        UserType::Wrapper(w) if owned => {
            plan.decls
                .push(format!("type({}), intent(in), target :: {}", ctx.wt(&w), nm));
            plan.actual = ctx.take_expr(&w, &nm);
        }
        UserType::Wrapper(w) => {
            plan.decls.push(format!(
                "type({}), intent({}), target :: {}",
                ctx.wt(&w),
                intent,
                nm
            ));
            plan.actual = format!("c_loc({}%raw)", nm);
        }
        UserType::Enum if owned => {
            plan.decls.push(format!("integer, intent(in) :: {}", nm));
            plan.actual = format!("int({}, c_int)", nm);
        }
        UserType::Bool if owned => {
            plan.decls.push(format!("logical, intent(in) :: {}", nm));
            plan.actual = format!("logical({}, c_bool)", nm);
        }
        UserType::Raw(fty) if owned => {
            plan.decls.push(format!("{}, intent(in) :: {}", fty, nm));
        }
        _ => {
            plan.decls.push(format!("type(c_ptr), intent(in) :: {}", nm));
        }
    }
    (nm, plan)
}

pub(crate) struct RetPlan {
    pub decl: String,
    /// `{}` is replaced by the C call expression.
    pub assign: Vec<String>,
}

/// Plan a function result named `r`.
pub(crate) fn plan_return(ctx: &Ctx, ret: &str) -> RetPlan {
    match ctx.classify(ret) {
        UserType::Wrapper(w) => RetPlan {
            decl: format!("type({}) :: r", ctx.wt(&w)),
            assign: vec!["r%raw = {}".to_string(), "r%owned = .true.".to_string()],
        },
        UserType::Str => RetPlan {
            decl: "character(len=:), allocatable :: r".to_string(),
            assign: vec![format!("r = {}({{}})", STRING_OUT_HELPER)],
        },
        UserType::Enum => RetPlan {
            decl: "integer :: r".to_string(),
            assign: vec!["r = int({})".to_string()],
        },
        UserType::Bool => RetPlan {
            decl: "logical :: r".to_string(),
            assign: vec!["r = logical({})".to_string()],
        },
        UserType::Kind(k) => RetPlan {
            decl: format!("type({}) :: r", ffi_type_name(&k)),
            assign: vec!["r = {}".to_string()],
        },
        UserType::Raw(fty) => RetPlan {
            decl: format!("{} :: r", fty),
            assign: vec!["r = {}".to_string()],
        },
    }
}

// ============================================================================
// Declarations (before `contains`)
// ============================================================================

pub(crate) fn generate_wrapper_decls(builder: &mut CodeBuilder, ctx: &Ctx) -> Result<()> {
    builder.line("! ----------------------------------------------------------------------");
    builder.line("! Unit-enum constants (plain integers, no Az prefix).");
    builder.line("! ----------------------------------------------------------------------");
    for (name, value) in &ctx.enum_consts {
        builder.line(&format!("integer, parameter :: {} = {}", name, value));
        builder.line(&format!("public :: {}", name));
    }
    builder.blank();

    builder.line("! ----------------------------------------------------------------------");
    builder.line("! Wrapper types. `owned` guards the explicit `delete`; there is no");
    builder.line("! finalizer on purpose (a function result would be finalized after");
    builder.line("! the assignment that consumed it).");
    builder.line("! ----------------------------------------------------------------------");
    builder.blank();
    for c in &ctx.classes {
        emit_wrapper_type_decl(builder, ctx, c);
    }

    builder.line(&format!("public :: {}", STRING_IN_HELPER));
    builder.line(&format!("public :: {}", STRING_OUT_HELPER));
    for c in &ctx.classes {
        builder.line(&format!("public :: {}", c.delete_name));
        for p in &c.procs {
            builder.line(&format!("public :: {}", p.name));
        }
    }
    builder.blank();
    Ok(())
}

fn emit_wrapper_type_decl(builder: &mut CodeBuilder, ctx: &Ctx, c: &ClassPlan) {
    for d in &c.s.doc {
        builder.line(&format!("! {}", super::sanitize_comment_line(d)));
    }
    builder.line(&format!("type :: {}", c.wt));
    builder.indent();
    builder.line(&format!("type({}) :: raw", ffi_type_name(&c.s.name)));
    builder.line("logical :: owned = .false.");
    builder.dedent();
    builder.line("contains");
    builder.indent();
    builder.line(&format!("procedure :: delete => {}", c.delete_name));
    if ctx.ref_any.as_deref() == Some(c.s.name.as_str()) {
        builder.line(&format!("procedure :: get => {}", super::managed::REF_ANY_GET));
    }
    for p in &c.procs {
        if let Some(b) = &p.binding {
            builder.line(&format!("procedure :: {} => {}", b, p.name));
        }
    }
    builder.dedent();
    builder.line(&format!("end type {}", c.wt));
    builder.line(&format!("public :: {}", c.wt));
    builder.blank();
}

// ============================================================================
// Bodies (after `contains`)
// ============================================================================

pub(crate) fn generate_wrapper_bodies(builder: &mut CodeBuilder, ctx: &Ctx) -> Result<()> {
    emit_string_helpers(builder, ctx);
    for c in &ctx.classes {
        emit_delete(builder, ctx, c);
        emit_take(builder, ctx, c);
        for p in &c.procs {
            if let Some(info) = &p.smart {
                emit_smart_factory(builder, ctx, c, p, info);
            } else if takes_self(p.func) {
                emit_method(builder, ctx, c, p);
            } else {
                emit_factory(builder, ctx, p);
            }
        }
    }
    Ok(())
}

fn emit_string_helpers(builder: &mut CodeBuilder, ctx: &Ctx) {
    let Some(st) = &ctx.string else { return };
    let az = ffi_type_name(&st.name);

    builder.line(&format!("function {}(s) result(r)", STRING_IN_HELPER));
    builder.indent();
    builder.line("character(len=*), intent(in) :: s");
    builder.line(&format!("type({}) :: r", az));
    builder.line("character(kind=c_char), target :: buf(max(len(s), 1))");
    builder.line("integer :: i");
    builder.line("do i = 1, len(s)");
    builder.line("  buf(i) = s(i:i)");
    builder.line("end do");
    builder.line(&format!(
        "r = {}(c_loc(buf(1)), 0_c_size_t, int(len(s), c_size_t))",
        st.copy_alias
    ));
    builder.dedent();
    builder.line(&format!("end function {}", STRING_IN_HELPER));
    builder.blank();

    builder.line(&format!("function {}(s) result(r)", STRING_OUT_HELPER));
    builder.indent();
    builder.line(&format!("type({}), intent(in) :: s", az));
    builder.line("character(len=:), allocatable :: r");
    builder.line(&format!("type({}), target :: tmp", az));
    builder.line("character(kind=c_char), pointer :: chars(:)");
    builder.line("integer :: n, i");
    builder.line("tmp = s");
    builder.line(&format!("n = int(tmp%{}%{})", st.vec_field, st.len_field));
    builder.line("allocate(character(len=n) :: r)");
    builder.line("if (n > 0) then");
    builder.line(&format!(
        "  call c_f_pointer(tmp%{}%{}, chars, [n])",
        st.vec_field, st.ptr_field
    ));
    builder.line("  do i = 1, n");
    builder.line("    r(i:i) = chars(i)");
    builder.line("  end do");
    builder.line("end if");
    if let Some(del) = ctx.delete_call(&st.name, "tmp") {
        builder.line(&del);
    }
    builder.dedent();
    builder.line(&format!("end function {}", STRING_OUT_HELPER));
    builder.blank();
}

fn emit_delete(builder: &mut CodeBuilder, ctx: &Ctx, c: &ClassPlan) {
    builder.line(&format!("subroutine {}(self)", c.delete_name));
    builder.indent();
    builder.line(&format!("class({}), intent(inout), target :: self", c.wt));
    if let Some(del) = ctx.delete_call(&c.s.name, "self%raw") {
        builder.line(&format!("if (self%owned) {}", del));
    }
    builder.line("self%owned = .false.");
    builder.dedent();
    builder.line(&format!("end subroutine {}", c.delete_name));
    builder.blank();
}

fn emit_take(builder: &mut CodeBuilder, ctx: &Ctx, c: &ClassPlan) {
    let (Some(take), Some(clone)) = (&c.take_name, ctx.clone.get(&c.s.name)) else {
        return;
    };
    let alias = fortran_alias_for(&clone.c_name);
    let by_value = clone
        .args
        .first()
        .map(|a| matches!(a.ref_kind, ArgRefKind::Owned))
        .unwrap_or(false);
    builder.line(&format!("function {}(x) result(r)", take));
    builder.indent();
    builder.line(&format!("type({}), intent(in), target :: x", c.wt));
    builder.line(&format!("type({}) :: r", ffi_type_name(&c.s.name)));
    builder.line("if (x%owned) then");
    builder.line("  r = x%raw");
    builder.line("else");
    if by_value {
        builder.line(&format!("  r = {}(x%raw)", alias));
    } else {
        builder.line(&format!("  r = {}(c_loc(x%raw))", alias));
    }
    builder.line("end if");
    builder.dedent();
    builder.line(&format!("end function {}", take));
    builder.blank();
}

fn emit_factory(builder: &mut CodeBuilder, ctx: &Ctx, p: &ProcPlan) {
    let f = p.func;
    let alias = fortran_alias_for(&f.c_name);
    let plans: Vec<(String, ArgPlan)> = f.args.iter().map(|a| plan_arg(ctx, a)).collect();
    let dummies: Vec<&str> = plans.iter().map(|(n, _)| n.as_str()).collect();
    let actuals: Vec<&str> = plans.iter().map(|(_, p)| p.actual.as_str()).collect();
    let ret = f.return_type.as_deref().map(|r| plan_return(ctx, r));
    emit_procedure(builder, &p.name, &dummies, &[], &plans, ret.as_ref(), &alias, &actuals, &[]);
}

fn emit_method(builder: &mut CodeBuilder, ctx: &Ctx, c: &ClassPlan, p: &ProcPlan) {
    let f = p.func;
    let alias = fortran_alias_for(&f.c_name);
    let recv = &f.args[0];
    let consumed = matches!(recv.ref_kind, ArgRefKind::Owned);
    let returns_self = f
        .return_type
        .as_deref()
        .map(|r| r.trim() == f.class_name)
        .unwrap_or(false);

    let self_decl = if consumed || matches!(recv.ref_kind, ArgRefKind::RefMut | ArgRefKind::PtrMut) {
        format!("class({}), intent(inout), target :: self", c.wt)
    } else {
        format!("class({}), intent(in), target :: self", c.wt)
    };
    let self_actual = if consumed {
        ctx.take_expr(&c.s.name, "self")
    } else {
        "c_loc(self%raw)".to_string()
    };

    let plans: Vec<(String, ArgPlan)> = f.args[1..].iter().map(|a| plan_arg(ctx, a)).collect();
    let mut dummies: Vec<&str> = vec!["self"];
    dummies.extend(plans.iter().map(|(n, _)| n.as_str()));
    let mut actuals: Vec<&str> = vec![self_actual.as_str()];
    actuals.extend(plans.iter().map(|(_, p)| p.actual.as_str()));

    if consumed && returns_self {
        // In-place builder: `call label%with_css('...')`.
        let call = format!("{}({})", alias, actuals.join(", "));
        let ret = RetPlan {
            decl: String::new(),
            assign: vec![format!("self%raw = {}", call), "self%owned = .true.".to_string()],
        };
        emit_procedure(builder, &p.name, &dummies, &[self_decl], &plans, Some(&ret), &alias, &actuals, &[]);
        return;
    }
    let ret = f.return_type.as_deref().map(|r| plan_return(ctx, r));
    let post: Vec<String> = if consumed {
        vec!["self%owned = .false.".to_string()]
    } else {
        Vec::new()
    };
    emit_procedure(builder, &p.name, &dummies, &[self_decl], &plans, ret.as_ref(), &alias, &actuals, &post);
}

/// Shared body writer. `ret.decl` empty means "subroutine whose
/// `ret.assign` lines already contain the call" (in-place builders).
#[allow(clippy::too_many_arguments)]
fn emit_procedure(
    builder: &mut CodeBuilder,
    name: &str,
    dummies: &[&str],
    head_decls: &[String],
    plans: &[(String, ArgPlan)],
    ret: Option<&RetPlan>,
    alias: &str,
    actuals: &[&str],
    post: &[String],
) {
    let is_function = ret.map(|r| !r.decl.is_empty()).unwrap_or(false);
    if is_function {
        builder.line(&format!("function {}({}) result(r)", name, dummies.join(", ")));
    } else {
        builder.line(&format!("subroutine {}({})", name, dummies.join(", ")));
    }
    builder.indent();
    for d in head_decls {
        builder.line(d);
    }
    for (_, p) in plans {
        for d in &p.decls {
            builder.line(d);
        }
    }
    if let Some(r) = ret {
        if !r.decl.is_empty() {
            builder.line(&r.decl);
        }
    }
    for (_, p) in plans {
        for l in &p.locals {
            builder.line(l);
        }
    }
    for (_, p) in plans {
        for l in &p.pre {
            builder.line(l);
        }
    }
    let call = format!("{}({})", alias, actuals.join(", "));
    match ret {
        Some(r) if !r.decl.is_empty() => {
            for a in &r.assign {
                builder.line(&a.replace("{}", &call));
            }
        }
        Some(r) => {
            for a in &r.assign {
                builder.line(a);
            }
        }
        None => builder.line(&format!("call {}", call)),
    }
    for (_, p) in plans {
        for l in &p.post {
            builder.line(l);
        }
    }
    for l in post {
        builder.line(l);
    }
    builder.dedent();
    if is_function {
        builder.line(&format!("end function {}", name));
    } else {
        builder.line(&format!("end subroutine {}", name));
    }
    builder.blank();
}

/// `window_create_options_create(layout)`: `_default()` plus the
/// registered layout callback spliced into the IR-discovered field path.
fn emit_smart_factory(
    builder: &mut CodeBuilder,
    ctx: &Ctx,
    c: &ClassPlan,
    p: &ProcPlan,
    info: &LayoutCallbackFactoryInfo,
) {
    let arg = dummy_name(&p.func.args[0].name);
    let path: Vec<String> = info.field_path.iter().map(|s| sanitize_identifier(s)).collect();
    builder.line(&format!("function {}({}) result(r)", p.name, arg));
    builder.indent();
    builder.line(&format!(
        "procedure({}) :: {}",
        super::managed::iface_name(&info.callback_wrapper),
        arg
    ));
    builder.line(&format!("type({}) :: r", c.wt));
    builder.line(&format!("r%raw = {}()", fortran_alias_for(&info.default_c_name)));
    builder.line(&format!(
        "r%raw%{} = {}({})",
        path.join("%"),
        super::managed::register_name(&info.callback_wrapper),
        arg
    ));
    builder.line("r%owned = .true.");
    builder.dedent();
    builder.line(&format!("end function {}", p.name));
    builder.blank();
    let _ = ctx;
}
