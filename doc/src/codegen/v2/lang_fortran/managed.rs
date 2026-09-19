//! Fortran (F2003+) managed-FFI runtime: the handle table, the typed
//! per-kind callback interfaces, and the `RefAny` object store.
//!
//! Everything here is derived from the IR — the kind list comes from
//! [`host_invoker_kinds`], each kind's Fortran signature from its
//! `CallbackTypedefDef`, and the wrapper spellings from
//! [`super::wrappers::Ctx`]. There are no per-function or per-kind
//! literals.
//!
//! ## The handle table
//!
//! One `save`d array of
//!
//! ```fortran
//! type :: azul_handle_t
//!   integer(c_int64_t) :: id = 0
//!   class(*), pointer :: object => null()
//!   procedure(callback_iface), pointer, nopass :: callback => null()
//!   procedure(layout_callback_iface), pointer, nopass :: layout_callback => null()
//!   ! ... one component per HOST_INVOKER_KINDS kind ...
//! end type azul_handle_t
//! ```
//!
//! serves both purposes libazul's host-handle protocol needs: a
//! `RefAny` payload (the `object` component, a deep copy the binding
//! owns and the releaser frees on last-clone drop) and a registered
//! user procedure (one typed procedure-pointer component per callback
//! kind). Both mint ids from the same counter, so
//! `AzApp_setHostHandleReleaser` can free either with one entry point.
//!
//! Minting the first handle lazily installs the releaser and every
//! per-kind invoker ([`ENSURE`]), so user code never calls an `init`
//! function — forgetting it used to mean "the window opens but nothing
//! ever fires".
//!
//! ## Typed callbacks
//!
//! Each kind gets an `abstract interface` whose dummies are the
//! WRAPPER types (`type(callback_info_t), intent(inout) :: arg1`), not
//! raw `type(c_ptr)`s, and whose result is the mapped return
//! (`type(dom_t)` for `LayoutCallback`, `integer` for `Update`-returning
//! kinds). The `bind(C)` boundary is the generated invoker, not the
//! user's procedure:
//!
//! ```text
//! libazul --(id, const AzRefAny*, const AzCallbackInfo*, AzUpdate*)-->
//!     azul_button_on_click_callback_invoker  (bind(C))
//!         --> handle slot lookup --> model lookup --> fp(model, a1)
//!         --> result check --> out_ptr write
//! ```
//!
//! so a user callback is an ordinary Fortran module function and needs
//! no `iso_c_binding`, no `c_f_pointer`, and no out-pointer writes.
//!
//! ## The model
//!
//! A kind whose first argument is the owned `RefAny` (the user's data)
//! hands the user procedure the MODEL itself, `class(*), intent(inout)`,
//! which the procedure recovers with `select type`. That is the only
//! place Fortran can name the user's type: an abstract interface cannot
//! be generic over it, so the downcast is the language's own checked
//! one. Going the other way, every consuming `RefAny` parameter of the
//! wrapper layer takes `class(*)` ([`MODEL_OF`]): a `ref_any_t` is used
//! as is, the model of a callback that is running is cloned (so
//! `call button%with_on_click(model, on_click)` inside `layout` binds
//! the SAME model), anything else is copied into a new handle.
//!
//! The invoker never lets a bad value reach the engine: a `RefAny` that
//! holds no Fortran value, an enum result outside the enum, or a
//! wrapper result that was never assigned is reported through the
//! kind's `CallbackInfo`-style `log(Error, ...)` argument (stderr when
//! the kind has none) and the engine's pre-filled default is kept.

use super::{
    super::{
        generator::CodeBuilder,
        ir::{ArgRefKind, CallbackTypedefDef, FunctionKind},
        managed_host_invoker::{
            arg_name_or_default, has_return, host_invoker_kinds, to_snake_case, wrapper_name,
        },
    },
    ffi_type_name,
    functions::{fortran_alias_for, should_emit_function},
    map_type_to_fortran, truncate_identifier,
    wrappers::{Ctx, UserType, STRING_IN_HELPER},
};

// ============================================================================
// Names (every module-level identifier this file emits)
// ============================================================================

/// Derived type holding one handle-table entry.
pub(crate) const HANDLE_TYPE: &str = "azul_handle_t";
/// The `save`d handle table itself.
pub(crate) const HANDLE_TABLE: &str = "azul_handles";
/// Monotonic id counter (ids are never reused).
pub(crate) const LAST_ID: &str = "azul_last_handle_id";
/// Guard for the one-shot invoker installation.
pub(crate) const INSTALLED: &str = "azul_invokers_installed";
/// `azul_handle_new() -> integer` (slot index of a fresh entry).
pub(crate) const HANDLE_NEW: &str = "azul_handle_new";
/// `azul_handle_slot(id) -> integer` (0 when unknown).
pub(crate) const HANDLE_SLOT: &str = "azul_handle_slot";
/// `bind(C)` releaser handed to `AzApp_setHostHandleReleaser`.
pub(crate) const HANDLE_RELEASE: &str = "azul_handle_release";
/// Lazy one-shot install of the releaser + every per-kind invoker.
pub(crate) const ENSURE: &str = "azul_ensure_invokers";
/// Public `ref_any_create(object) -> ref_any_t`.
pub(crate) const REF_ANY_CREATE: &str = "ref_any_create";
/// Implementation of the `get` type-bound procedure on the RefAny
/// wrapper: `data%get()` -> `class(*), pointer`.
pub(crate) const REF_ANY_GET: &str = "azul_ref_any_get";

/// `azul_ref_any_of(x) -> type(AzRefAny)`: the upcast every consuming
/// `RefAny` parameter goes through (see the module docs).
pub(crate) const MODEL_OF: &str = "azul_ref_any_of";
/// Capacity of the running-callback stack [`MODEL_OF`] searches.
const CTX_MAX: &str = "azul_ctx_max";
/// `const AzRefAny*` of each running callback's model.
const CTX_REF: &str = "azul_ctx_ref";
/// Handle id of each running callback's model.
const CTX_ID: &str = "azul_ctx_id";
/// Number of running callbacks (may exceed [`CTX_MAX`]; the excess is
/// simply not searched).
const CTX_DEPTH: &str = "azul_ctx_depth";
const CTX_PUSH: &str = "azul_ctx_push";
const CTX_POP: &str = "azul_ctx_pop";
/// `azul_int_str(i) -> character(:)` for the invoker's messages.
const INT_STR: &str = "azul_int_str";
/// `azul_report(msg)`: stderr, for kinds with no `log` argument.
const REPORT: &str = "azul_report";

const FFI_SET_RELEASER: &str = "azul_ffi_set_host_handle_releaser";
const FFI_REF_ANY_NEW: &str = "azul_ffi_ref_any_new_host_handle";
const FFI_REF_ANY_GET: &str = "azul_ffi_ref_any_get_host_handle";

/// Abstract-interface name for a callback kind
/// (`ButtonOnClickCallback` -> `button_on_click_callback_iface`).
pub(crate) fn iface_name(kind: &str) -> String {
    truncate_identifier(&format!("{}_iface", to_snake_case(kind)))
}

/// `azul_register_<snake>(proc) -> type(Az<Kind>)`.
pub(crate) fn register_name(kind: &str) -> String {
    truncate_identifier(&format!("azul_register_{}", to_snake_case(kind)))
}

/// The `bind(C)` dispatcher libazul calls for this kind.
fn invoker_name(kind: &str) -> String {
    truncate_identifier(&format!("azul_{}_invoker", to_snake_case(kind)))
}

/// The handle-table procedure-pointer component for this kind.
fn slot_component(kind: &str) -> String {
    truncate_identifier(&to_snake_case(kind))
}

/// FFI alias of `Az<Kind>_createFromHostHandle`.
fn ffi_from_handle(kind: &str) -> String {
    truncate_identifier(&format!("azul_ffi_{}_from_handle", to_snake_case(kind)))
}

/// FFI alias of `AzApp_set<Kind>Invoker`.
fn ffi_set_invoker(kind: &str) -> String {
    truncate_identifier(&format!("azul_ffi_set_{}_invoker", to_snake_case(kind)))
}

/// Every module-level name this file claims, so
/// [`super::wrappers::Ctx`] can keep the wrapper layer clear of them
/// (Fortran folds case, so one lower-cased claim per name).
pub(crate) fn reserved_names(ctx: &Ctx) -> Vec<String> {
    let mut out: Vec<String> = [
        HANDLE_TYPE,
        HANDLE_TABLE,
        LAST_ID,
        INSTALLED,
        HANDLE_NEW,
        HANDLE_SLOT,
        HANDLE_RELEASE,
        ENSURE,
        REF_ANY_CREATE,
        REF_ANY_GET,
        MODEL_OF,
        CTX_MAX,
        CTX_REF,
        CTX_ID,
        CTX_DEPTH,
        CTX_PUSH,
        CTX_POP,
        INT_STR,
        REPORT,
        FFI_SET_RELEASER,
        FFI_REF_ANY_NEW,
        FFI_REF_ANY_GET,
    ]
    .iter()
    .map(|s| s.to_string())
    .collect();
    for cb in &ctx.kinds {
        let k = wrapper_name(cb);
        out.push(iface_name(k));
        out.push(register_name(k));
        out.push(invoker_name(k));
        out.push(ffi_from_handle(k));
        out.push(ffi_set_invoker(k));
    }
    out
}

/// The module-level names of the runtime that stay out of the public
/// surface. `azul_api` is `public` by default (so `azul` can re-export the
/// whole binding through it), which makes every one of these an explicit
/// `private ::` entry; each must exist, or the statement is an error.
pub(crate) fn private_names(ctx: &Ctx) -> Vec<String> {
    let mut out: Vec<String> = [
        HANDLE_TYPE,
        HANDLE_TABLE,
        LAST_ID,
        INSTALLED,
        HANDLE_NEW,
        HANDLE_SLOT,
        HANDLE_RELEASE,
        ENSURE,
        CTX_MAX,
        CTX_REF,
        CTX_ID,
        CTX_DEPTH,
        CTX_PUSH,
        CTX_POP,
        INT_STR,
        REPORT,
        FFI_SET_RELEASER,
    ]
    .iter()
    .map(|s| s.to_string())
    .collect();
    if ctx.ref_any.is_some() {
        for n in [REF_ANY_GET, MODEL_OF, FFI_REF_ANY_NEW, FFI_REF_ANY_GET] {
            out.push(n.to_string());
        }
    }
    for cb in &ctx.kinds {
        let k = wrapper_name(cb);
        out.push(invoker_name(k));
        out.push(ffi_from_handle(k));
        out.push(ffi_set_invoker(k));
    }
    out
}

// ============================================================================
// Signature plan for one callback kind
// ============================================================================

/// How one callback argument crosses the invoker boundary.
struct IfaceArg {
    /// Dummy declaration inside the abstract interface.
    dummy: String,
    /// Invoker locals (the user-facing value and the FFI-typed pointer the
    /// incoming `type(c_ptr)` is mapped to).
    locals: Vec<String>,
    /// Statements that fill the user-facing local from the incoming
    /// pointer (empty for the model, which [`emit_invoker`] looks up).
    unpack: Vec<String>,
    /// Actual-argument expression handed to the user procedure.
    actual: String,
    /// This is the kind's model (first argument, an owned `RefAny`).
    model: bool,
}

/// How the callback's result crosses back.
struct IfaceRet {
    /// Result declaration inside the abstract interface (`type(dom_t) :: r`).
    iface_decl: String,
    /// Local result declaration inside the invoker.
    local_decl: String,
    /// Declaration of the out-parameter pointer.
    out_decl: String,
    /// `(condition, message)`: when the condition holds the result is not
    /// a value the engine may see; the message says why and the engine's
    /// pre-filled default is kept.
    reject: Option<(String, String)>,
    /// Statements writing the result through the out pointer.
    write: Vec<String>,
}

/// A kind argument that can log: `(index into cb.args, FFI alias of its
/// `log(level, message)` method, the level enum's `Error` value)`.
struct Logger {
    arg: usize,
    alias: String,
    level: usize,
}

/// The kind's first argument is the user's model: an owned `RefAny`.
fn is_model_arg(ctx: &Ctx, cb: &CallbackTypedefDef, idx: usize) -> bool {
    idx == 0
        && ctx.ref_any.as_deref() == Some(cb.args[0].type_name.trim())
        && matches!(cb.args[0].ref_kind, ArgRefKind::Owned)
}

/// Dummy names of a kind's abstract interface: `model` for the model, the
/// api.json argument name when there is one, `info` for the `*Info` type a
/// kind receives, else the argument's type in snake case
/// (`check_box_state`), or `arg<i>` for a primitive. They only show up in
/// compiler messages ("INTENT mismatch in argument 'info'"), but there
/// they should read like the user's own code.
fn iface_dummy_names(ctx: &Ctx, cb: &CallbackTypedefDef) -> Vec<String> {
    let mut used: std::collections::BTreeSet<String> = ["r".to_string()].into_iter().collect();
    (0..cb.args.len())
        .map(|i| {
            let base = if is_model_arg(ctx, cb, i) {
                "model".to_string()
            } else {
                let from_ir = arg_name_or_default(cb, i);
                let ty = cb.args[i].type_name.trim();
                if !from_ir.starts_with("_arg") {
                    from_ir
                } else if ty.ends_with("Info") {
                    "info".to_string()
                } else if ctx.ir.find_struct(ty).is_some() || ctx.ir.find_enum(ty).is_some() {
                    to_snake_case(ty)
                } else {
                    format!("arg{}", i)
                }
            };
            let base = super::wrappers::dummy_name(&base);
            let mut name = base.clone();
            let mut n = 2;
            while !used.insert(name.to_lowercase()) {
                name = truncate_identifier(&format!("{}_{}", base, n));
                n += 1;
            }
            name
        })
        .collect()
}

/// `iface` is the dummy's name in the abstract interface; the invoker's
/// own `bind(C)` dummies are always `arg<idx>`.
fn plan_iface_arg(ctx: &Ctx, cb: &CallbackTypedefDef, idx: usize, iface: &str) -> IfaceArg {
    let dummy = format!("arg{}", idx);
    let local = format!("azul_a{}", idx);
    if is_model_arg(ctx, cb, idx) {
        return IfaceArg {
            dummy: format!("class(*), intent(inout) :: {}", iface),
            locals: vec![
                format!("class(*), pointer :: {}", local),
                "integer(c_int64_t) :: azul_mid".to_string(),
                "integer :: azul_mslot".to_string(),
            ],
            unpack: Vec::new(),
            actual: local,
            model: true,
        };
    }
    let type_name = cb.args[idx].type_name.as_str();
    let ptr = format!("azul_p{}", idx);
    let ffi = map_type_to_fortran(type_name, ctx.ir);
    let (dummy_decl, local_decl, init, assign) = match ctx.classify(type_name) {
        UserType::Wrapper(w) => {
            let wt = ctx.wt(&w);
            (
                format!("type({}), intent(inout) :: {}", wt, iface),
                format!("type({}) :: {}", wt, local),
                None,
                format!("{}%raw = {}", local, ptr),
            )
        }
        UserType::Enum => (
            format!("integer, intent(in) :: {}", iface),
            format!("integer :: {}", local),
            Some(format!("{} = 0", local)),
            format!("{} = int({})", local, ptr),
        ),
        UserType::Bool => (
            format!("logical, intent(in) :: {}", iface),
            format!("logical :: {}", local),
            Some(format!("{} = .false.", local)),
            format!("{} = logical({})", local, ptr),
        ),
        // Strings and every other shape stay in their FFI spelling: no
        // callback kind in HOST_INVOKER_KINDS takes one today, and a
        // borrowed `AzString` must not be consumed by the `character`
        // helper (which deletes what it reads).
        UserType::Str | UserType::Kind(_) | UserType::Raw(_) => (
            format!("{}, intent(in) :: {}", ffi, iface),
            format!("{} :: {}", ffi, local),
            None,
            format!("{} = {}", local, ptr),
        ),
    };
    let mut unpack = Vec::new();
    if let Some(i) = init {
        unpack.push(i);
    }
    unpack.push(format!("if (c_associated({})) then", dummy));
    unpack.push(format!("  call c_f_pointer({}, {})", dummy, ptr));
    unpack.push(format!("  {}", assign));
    unpack.push("end if".to_string());
    IfaceArg {
        dummy: dummy_decl,
        locals: vec![local_decl, format!("{}, pointer :: {}", ffi, ptr)],
        unpack,
        actual: local,
        model: false,
    }
}

/// `a Dom` / `an Update`.
fn with_article(noun: &str) -> String {
    let vowel = noun
        .chars()
        .next()
        .map(|c| "AEIOUaeiou".contains(c))
        .unwrap_or(false);
    format!("{} {}", if vowel { "an" } else { "a" }, noun)
}

fn plan_iface_ret(ctx: &Ctx, kind: &str, ret: &str) -> IfaceRet {
    let ffi = map_type_to_fortran(ret, ctx.ir);
    let ret_name = ret.trim();
    let (iface, local, reject, write) = match ctx.classify(ret) {
        UserType::Wrapper(w) => {
            let wt = ctx.wt(&w);
            // `owned` is default-initialised to .false. and every
            // constructor sets it, so a result the user never assigned
            // arrives here unowned (its `raw` is garbage).
            (
                format!("type({}) :: r", wt),
                format!("type({}) :: azul_r", wt),
                Some((
                    ".not. azul_r%owned".to_string(),
                    format!(
                        "'azul: {} expected {} result, got a {} that was never assigned'",
                        kind,
                        with_article(ret_name),
                        wt
                    ),
                )),
                vec!["azul_out = azul_r%raw".to_string()],
            )
        }
        UserType::Enum => {
            // An `integer` result has no default: one the user left
            // unassigned (or set to anything else) must not reach the
            // engine as an out-of-range C enum.
            let n = ctx
                .ir
                .find_enum(ret_name)
                .map(|e| e.variants.len())
                .unwrap_or(0);
            (
                "integer :: r".to_string(),
                "integer :: azul_r".to_string(),
                (n > 0).then(|| {
                    (
                        format!("azul_r < 0 .or. azul_r > {}", n - 1),
                        format!(
                            "'azul: {} expected {} (0 to {}), got ' // {}(azul_r)",
                            kind,
                            with_article(ret_name),
                            n - 1,
                            INT_STR
                        ),
                    )
                }),
                vec!["azul_out = int(azul_r, c_int)".to_string()],
            )
        }
        UserType::Bool => (
            "logical :: r".to_string(),
            "logical :: azul_r".to_string(),
            None,
            // Never convert the user's logical bit pattern: write one of
            // the two values a C `bool` may hold.
            vec![
                "if (azul_r) then".to_string(),
                "  azul_out = .true._c_bool".to_string(),
                "else".to_string(),
                "  azul_out = .false._c_bool".to_string(),
                "end if".to_string(),
            ],
        ),
        UserType::Str | UserType::Kind(_) | UserType::Raw(_) => (
            format!("{} :: r", ffi),
            format!("{} :: azul_r", ffi),
            None,
            vec!["azul_out = azul_r".to_string()],
        ),
    };
    IfaceRet {
        iface_decl: iface,
        local_decl: local,
        out_decl: format!("{}, pointer :: azul_out", ffi),
        reject,
        write,
    }
}

fn ret_type(cb: &CallbackTypedefDef) -> Option<&str> {
    if has_return(cb) {
        cb.return_type.as_deref()
    } else {
        None
    }
}

/// The first argument of the kind whose type has a
/// `log(<unit enum>, String)` method (`CallbackInfo`).
fn kind_logger(ctx: &Ctx, cb: &CallbackTypedefDef) -> Option<Logger> {
    let string = ctx.string.as_ref()?;
    for (i, a) in cb.args.iter().enumerate() {
        if is_model_arg(ctx, cb, i) {
            continue;
        }
        let ty = a.type_name.trim();
        let Some(f) = ctx.ir.functions.iter().find(|f| {
            f.class_name == ty
                && f.method_name == "log"
                && matches!(f.kind, FunctionKind::Method | FunctionKind::MethodMut)
                && f.args.len() == 3
                && !matches!(f.args[0].ref_kind, ArgRefKind::Owned)
                && matches!(f.args[2].ref_kind, ArgRefKind::Owned)
                && f.args[2].type_name.trim() == string.name
                && should_emit_function(f, ctx.ir, ctx.config)
        }) else {
            continue;
        };
        let Some(e) = ctx.ir.find_enum(f.args[1].type_name.trim()) else {
            continue;
        };
        if e.is_union || e.variants.is_empty() {
            continue;
        }
        let level = e
            .variants
            .iter()
            .position(|v| v.name == "Error")
            .unwrap_or(0);
        return Some(Logger {
            arg: i,
            alias: fortran_alias_for(&f.c_name),
            level,
        });
    }
    None
}

/// Statements that report `msg` (a Fortran character expression): through
/// the kind's logging argument when it has one, else on stderr.
fn report(log: Option<&Logger>, msg: &str) -> Vec<String> {
    match log {
        Some(l) => vec![
            format!("if (c_associated(arg{})) then", l.arg),
            format!(
                "  call {}(arg{}, {}_c_int, {}({}))",
                l.alias,
                l.arg,
                l.level,
                STRING_IN_HELPER,
                msg
            ),
            "else".to_string(),
            format!("  call {}({})", REPORT, msg),
            "end if".to_string(),
        ],
        None => vec![format!("call {}({})", REPORT, msg)],
    }
}

// ============================================================================
// Declarations (before `contains`)
// ============================================================================

/// Emit the abstract interfaces, the handle table and the host-invoker
/// FFI block. Must run AFTER the wrapper type declarations (the
/// interfaces `import` them) and BEFORE `contains`.
pub(crate) fn emit_managed_decls(builder: &mut CodeBuilder, ctx: &Ctx, split: &super::Split) {
    builder.line("! ----------------------------------------------------------------------");
    builder.line("! Callback interfaces. Write an ordinary module procedure matching one");
    builder.line("! of these and pass it straight to the method that takes the callback");
    builder.line("! (`call button%with_on_click(data, on_click)`); the binding registers");
    builder.line("! it and dispatches through the handle table below.");
    builder.line("! ----------------------------------------------------------------------");
    builder.blank();

    builder.line("abstract interface");
    builder.indent();
    for cb in &ctx.kinds {
        emit_abstract_interface(builder, ctx, cb);
    }
    builder.dedent();
    builder.line("end interface");
    builder.blank();

    builder.line("! ----------------------------------------------------------------------");
    builder.line("! Handle table: one entry per RefAny payload or registered callback.");
    builder.line("! `object` is the binding-owned deep copy of a user's model, freed by");
    builder.line("! the releaser when libazul drops the last clone of the RefAny.");
    builder.line("! ----------------------------------------------------------------------");
    builder.line(&format!("type :: {}", HANDLE_TYPE));
    builder.indent();
    builder.line("integer(c_int64_t) :: id = 0");
    builder.line("class(*), pointer :: object => null()");
    for cb in &ctx.kinds {
        let k = wrapper_name(cb);
        builder.line(&format!(
            "procedure({}), pointer, nopass :: {} => null()",
            iface_name(k),
            slot_component(k)
        ));
    }
    builder.dedent();
    builder.line(&format!("end type {}", HANDLE_TYPE));
    builder.blank();

    builder.line(&format!(
        "type({}), allocatable, save :: {}(:)",
        HANDLE_TYPE, HANDLE_TABLE
    ));
    builder.line(&format!("integer(c_int64_t), save :: {} = 0", LAST_ID));
    builder.line(&format!("logical, save :: {} = .false.", INSTALLED));
    builder.line("! The models of the callbacks running right now, innermost last:");
    builder.line(&format!("! `{}` clones one of these instead of copying the value.", MODEL_OF));
    builder.line(&format!("integer, parameter :: {} = 64", CTX_MAX));
    builder.line(&format!("type(c_ptr), save :: {}({})", CTX_REF, CTX_MAX));
    builder.line(&format!("integer(c_int64_t), save :: {}({}) = 0", CTX_ID, CTX_MAX));
    builder.line(&format!("integer, save :: {} = 0", CTX_DEPTH));
    builder.blank();

    builder.line("! Host-handle C ABI (managed-binding exports of libazul).");
    builder.line("interface");
    builder.indent();
    builder.line(&format!(
        "subroutine {}(releaser) bind(C, name=\"AzApp_setHostHandleReleaser\")",
        FFI_SET_RELEASER
    ));
    builder.line("  import");
    builder.line("  type(c_funptr), value :: releaser");
    builder.line("end subroutine");
    if let Some(ra) = &ctx.ref_any {
        builder.line(&format!(
            "function {}(id) bind(C, name=\"AzRefAny_newHostHandle\") result(r)",
            FFI_REF_ANY_NEW
        ));
        builder.line("  import");
        builder.line("  integer(c_int64_t), value :: id");
        builder.line(&format!("  type({}) :: r", ffi_type_name(ra)));
        builder.line("end function");
        builder.line(&format!(
            "function {}(refany) bind(C, name=\"AzRefAny_getHostHandle\") result(r)",
            FFI_REF_ANY_GET
        ));
        builder.line("  import");
        builder.line("  type(c_ptr), value :: refany");
        builder.line("  integer(c_int64_t) :: r");
        builder.line("end function");
    }
    for cb in &ctx.kinds {
        let k = wrapper_name(cb);
        builder.line(&format!(
            "subroutine {}(invoker) bind(C, name=\"AzApp_set{}Invoker\")",
            ffi_set_invoker(k),
            k
        ));
        builder.line("  import");
        builder.line("  type(c_funptr), value :: invoker");
        builder.line("end subroutine");
        builder.line(&format!(
            "function {}(id) bind(C, name=\"Az{}_createFromHostHandle\") result(r)",
            ffi_from_handle(k),
            k
        ));
        builder.line("  import");
        builder.line("  integer(c_int64_t), value :: id");
        builder.line(&format!("  type({}) :: r", ffi_type_name(k)));
        builder.line("end function");
    }
    builder.dedent();
    builder.line("end interface");
    builder.blank();

    // Each public name is attributed to the api.json module of the type
    // it serves (the RefAny class, the kind's wrapper struct), so the
    // per-module facades re-export it.
    if let Some(ra) = &ctx.ref_any {
        builder.line(&split.marker(ra));
        builder.line(&format!("public :: {}", REF_ANY_CREATE));
    }
    for cb in &ctx.kinds {
        let k = wrapper_name(cb);
        builder.line(&split.marker(k));
        builder.line(&format!("public :: {}", iface_name(k)));
        builder.line(&format!("public :: {}", register_name(k)));
    }
    builder.blank();
}

fn emit_abstract_interface(builder: &mut CodeBuilder, ctx: &Ctx, cb: &CallbackTypedefDef) {
    let k = wrapper_name(cb);
    let name = iface_name(k);
    let names = iface_dummy_names(ctx, cb);
    let args: Vec<IfaceArg> = (0..cb.args.len())
        .map(|i| plan_iface_arg(ctx, cb, i, &names[i]))
        .collect();
    let ret = ret_type(cb).map(|r| plan_iface_ret(ctx, k, r));

    for d in &cb.doc {
        builder.line(&format!("! {}", super::sanitize_comment_line(d)));
    }
    if ret.is_some() {
        builder.line(&format!(
            "function {}({}) result(r)",
            name,
            names.join(", ")
        ));
    } else {
        builder.line(&format!("subroutine {}({})", name, names.join(", ")));
    }
    builder.indent();
    builder.line("import");
    for a in &args {
        builder.line(&a.dummy);
    }
    if let Some(r) = &ret {
        builder.line(&r.iface_decl);
    }
    builder.dedent();
    if ret.is_some() {
        builder.line(&format!("end function {}", name));
    } else {
        builder.line(&format!("end subroutine {}", name));
    }
    builder.blank();
}

// ============================================================================
// Bodies (after `contains`)
// ============================================================================

pub(crate) fn emit_managed_bodies(builder: &mut CodeBuilder, ctx: &Ctx) {
    emit_handle_table(builder, ctx);
    for cb in &ctx.kinds {
        emit_invoker(builder, ctx, cb);
    }
    for cb in &ctx.kinds {
        emit_register(builder, ctx, cb);
    }
    emit_ref_any(builder, ctx);
}

fn emit_handle_table(builder: &mut CodeBuilder, ctx: &Ctx) {
    // azul_handle_new
    builder.line(&format!("function {}() result(slot)", HANDLE_NEW));
    builder.indent();
    builder.line("integer :: slot");
    builder.line(&format!(
        "type({}), allocatable :: azul_grown(:)",
        HANDLE_TYPE
    ));
    builder.line("integer :: n");
    builder.line(&format!("call {}()", ENSURE));
    builder.line(&format!(
        "if (.not. allocated({})) allocate({}(0))",
        HANDLE_TABLE, HANDLE_TABLE
    ));
    builder.line(&format!("n = size({})", HANDLE_TABLE));
    builder.line("allocate(azul_grown(n + 1))");
    builder.line(&format!("azul_grown(1:n) = {}(1:n)", HANDLE_TABLE));
    builder.line(&format!("call move_alloc(azul_grown, {})", HANDLE_TABLE));
    builder.line("slot = n + 1");
    builder.line(&format!("{} = {} + 1", LAST_ID, LAST_ID));
    builder.line(&format!("{}(slot)%id = {}", HANDLE_TABLE, LAST_ID));
    builder.dedent();
    builder.line(&format!("end function {}", HANDLE_NEW));
    builder.blank();

    // azul_handle_slot
    builder.line(&format!("function {}(id) result(slot)", HANDLE_SLOT));
    builder.indent();
    builder.line("integer(c_int64_t), intent(in) :: id");
    builder.line("integer :: slot");
    builder.line("integer :: i");
    builder.line("slot = 0");
    builder.line(&format!("if (.not. allocated({})) return", HANDLE_TABLE));
    builder.line(&format!("do i = 1, size({})", HANDLE_TABLE));
    builder.line(&format!("  if ({}(i)%id == id) then", HANDLE_TABLE));
    builder.line("    slot = i");
    builder.line("    return");
    builder.line("  end if");
    builder.line("end do");
    builder.dedent();
    builder.line(&format!("end function {}", HANDLE_SLOT));
    builder.blank();

    // azul_handle_release — libazul calls this on last-clone drop.
    builder.line(&format!("subroutine {}(id) bind(C)", HANDLE_RELEASE));
    builder.indent();
    builder.line("integer(c_int64_t), value :: id");
    builder.line(&format!(
        "type({}), allocatable :: azul_kept(:)",
        HANDLE_TYPE
    ));
    builder.line("integer :: slot, n");
    builder.line(&format!("slot = {}(id)", HANDLE_SLOT));
    builder.line("if (slot == 0) return");
    builder.line(&format!(
        "if (associated({}(slot)%object)) deallocate({}(slot)%object)",
        HANDLE_TABLE, HANDLE_TABLE
    ));
    builder.line(&format!("n = size({})", HANDLE_TABLE));
    builder.line("allocate(azul_kept(n - 1))");
    builder.line(&format!("azul_kept(1:slot-1) = {}(1:slot-1)", HANDLE_TABLE));
    builder.line(&format!("azul_kept(slot:n-1) = {}(slot+1:n)", HANDLE_TABLE));
    builder.line(&format!("call move_alloc(azul_kept, {})", HANDLE_TABLE));
    builder.dedent();
    builder.line(&format!("end subroutine {}", HANDLE_RELEASE));
    builder.blank();

    // azul_ensure_invokers — lazy, so user code needs no init call.
    builder.line(&format!("subroutine {}()", ENSURE));
    builder.indent();
    builder.line(&format!("if ({}) return", INSTALLED));
    builder.line(&format!("{} = .true.", INSTALLED));
    builder.line(&format!(
        "call {}(c_funloc({}))",
        FFI_SET_RELEASER, HANDLE_RELEASE
    ));
    for cb in &ctx.kinds {
        let k = wrapper_name(cb);
        builder.line(&format!(
            "call {}(c_funloc({}))",
            ffi_set_invoker(k),
            invoker_name(k)
        ));
    }
    builder.dedent();
    builder.line(&format!("end subroutine {}", ENSURE));
    builder.blank();

    // The running-callback stack. Depth keeps counting past CTX_MAX so
    // push/pop stay balanced; only the first CTX_MAX entries are kept.
    builder.line(&format!("subroutine {}(ref, id)", CTX_PUSH));
    builder.indent();
    builder.line("type(c_ptr), intent(in) :: ref");
    builder.line("integer(c_int64_t), intent(in) :: id");
    builder.line(&format!("{d} = {d} + 1", d = CTX_DEPTH));
    builder.line(&format!("if ({} <= {}) then", CTX_DEPTH, CTX_MAX));
    builder.line(&format!("  {}({}) = ref", CTX_REF, CTX_DEPTH));
    builder.line(&format!("  {}({}) = id", CTX_ID, CTX_DEPTH));
    builder.line("end if");
    builder.dedent();
    builder.line(&format!("end subroutine {}", CTX_PUSH));
    builder.blank();

    builder.line(&format!("subroutine {}()", CTX_POP));
    builder.indent();
    builder.line(&format!("{d} = max({d} - 1, 0)", d = CTX_DEPTH));
    builder.dedent();
    builder.line(&format!("end subroutine {}", CTX_POP));
    builder.blank();

    builder.line(&format!("function {}(i) result(s)", INT_STR));
    builder.indent();
    builder.line("integer, intent(in) :: i");
    builder.line("character(len=:), allocatable :: s");
    builder.line("character(len=24) :: buf");
    builder.line("write (buf, '(I0)') i");
    builder.line("s = trim(buf)");
    builder.dedent();
    builder.line(&format!("end function {}", INT_STR));
    builder.blank();

    builder.line(&format!("subroutine {}(msg)", REPORT));
    builder.indent();
    builder.line("character(len=*), intent(in) :: msg");
    builder.line("write (error_unit, '(a)') msg");
    builder.dedent();
    builder.line(&format!("end subroutine {}", REPORT));
    builder.blank();
}

fn emit_invoker(builder: &mut CodeBuilder, ctx: &Ctx, cb: &CallbackTypedefDef) {
    let k = wrapper_name(cb);
    let name = invoker_name(k);
    let names = iface_dummy_names(ctx, cb);
    let args: Vec<IfaceArg> = (0..cb.args.len())
        .map(|i| plan_iface_arg(ctx, cb, i, &names[i]))
        .collect();
    let dummies: Vec<String> = (0..cb.args.len()).map(|i| format!("arg{}", i)).collect();
    let ret = ret_type(cb).map(|r| plan_iface_ret(ctx, k, r));
    let log = kind_logger(ctx, cb);
    let model = args.iter().any(|a| a.model);

    let mut sig = vec!["id".to_string()];
    sig.extend(dummies.iter().cloned());
    if ret.is_some() {
        sig.push("out_ptr".to_string());
    }

    builder.line(&format!("subroutine {}({}) bind(C)", name, sig.join(", ")));
    builder.indent();
    builder.line("integer(c_int64_t), value :: id");
    for d in &dummies {
        builder.line(&format!("type(c_ptr), value :: {}", d));
    }
    if ret.is_some() {
        builder.line("type(c_ptr), value :: out_ptr");
    }
    builder.line(&format!("procedure({}), pointer :: azul_fp", iface_name(k)));
    for a in &args {
        for l in &a.locals {
            builder.line(l);
        }
    }
    if let Some(r) = &ret {
        builder.line(&r.local_decl);
        builder.line(&r.out_decl);
    }
    builder.line("integer :: slot");
    builder.line(&format!("slot = {}(id)", HANDLE_SLOT));
    builder.line("if (slot == 0) return");
    // Copy the procedure pointer out of the table BEFORE calling: the
    // callback may register more handles, and growing the table moves
    // every entry.
    builder.line(&format!(
        "azul_fp => {}(slot)%{}",
        HANDLE_TABLE,
        slot_component(k)
    ));
    builder.line("if (.not. associated(azul_fp)) return");
    for (i, a) in args.iter().enumerate() {
        if !a.model {
            for l in &a.unpack {
                builder.line(l);
            }
            continue;
        }
        // The model: the object behind the RefAny's host handle. Only
        // pointers are copied, so the table growing during the call
        // cannot move it.
        builder.line(&format!("azul_mid = {}(arg{})", FFI_REF_ANY_GET, i));
        builder.line(&format!("azul_mslot = {}(azul_mid)", HANDLE_SLOT));
        builder.line(&format!("{} => null()", a.actual));
        builder.line(&format!(
            "if (azul_mslot > 0) {} => {}(azul_mslot)%object",
            a.actual, HANDLE_TABLE
        ));
        builder.line(&format!("if (.not. associated({})) then", a.actual));
        builder.indent();
        for l in report(
            log.as_ref(),
            &format!(
                "'azul: {} expected a Fortran model, got a RefAny that holds none'",
                k
            ),
        ) {
            builder.line(&l);
        }
        builder.line("return");
        builder.dedent();
        builder.line("end if");
    }
    if model {
        builder.line(&format!("call {}(arg0, azul_mid)", CTX_PUSH));
    }
    let actuals: Vec<&str> = args.iter().map(|a| a.actual.as_str()).collect();
    match &ret {
        Some(_) => builder.line(&format!("azul_r = azul_fp({})", actuals.join(", "))),
        None => builder.line(&format!("call azul_fp({})", actuals.join(", "))),
    }
    if model {
        builder.line(&format!("call {}()", CTX_POP));
    }
    if let Some(r) = &ret {
        if let Some((cond, msg)) = &r.reject {
            builder.line(&format!("if ({}) then", cond));
            builder.indent();
            for l in report(log.as_ref(), msg) {
                builder.line(&l);
            }
            builder.line("return");
            builder.dedent();
            builder.line("end if");
        }
        builder.line("if (c_associated(out_ptr)) then");
        builder.indent();
        builder.line("call c_f_pointer(out_ptr, azul_out)");
        for l in &r.write {
            builder.line(l);
        }
        builder.dedent();
        builder.line("end if");
    }
    builder.dedent();
    builder.line(&format!("end subroutine {}", name));
    builder.blank();
}

fn emit_register(builder: &mut CodeBuilder, _ctx: &Ctx, cb: &CallbackTypedefDef) {
    let k = wrapper_name(cb);
    let name = register_name(k);
    builder.line(&format!("function {}(cb) result(r)", name));
    builder.indent();
    builder.line(&format!("procedure({}) :: cb", iface_name(k)));
    builder.line(&format!("type({}) :: r", ffi_type_name(k)));
    builder.line("integer :: slot");
    builder.line(&format!("slot = {}()", HANDLE_NEW));
    builder.line(&format!(
        "{}(slot)%{} => cb",
        HANDLE_TABLE,
        slot_component(k)
    ));
    builder.line(&format!(
        "r = {}({}(slot)%id)",
        ffi_from_handle(k),
        HANDLE_TABLE
    ));
    builder.dedent();
    builder.line(&format!("end function {}", name));
    builder.blank();
}

fn emit_ref_any(builder: &mut CodeBuilder, ctx: &Ctx) {
    let Some(ra) = &ctx.ref_any else { return };
    let wt = ctx.wt(ra);

    // azul_ref_any_of: the upcast behind every consuming RefAny parameter.
    builder.line(&format!("function {}(x) result(r)", MODEL_OF));
    builder.indent();
    builder.line("class(*), intent(in), target :: x");
    builder.line(&format!("type({}) :: r", ffi_type_name(ra)));
    builder.line("integer :: i, slot");
    let clone = ctx.clone.get(ra.as_str()).map(|f| {
        let by_value = f
            .args
            .first()
            .map(|a| matches!(a.ref_kind, ArgRefKind::Owned))
            .unwrap_or(false);
        (fortran_alias_for(&f.c_name), by_value)
    });
    if let Some((_, true)) = &clone {
        builder.line(&format!("type({}), pointer :: azul_src", ffi_type_name(ra)));
    }
    builder.line("select type (x)");
    builder.line(&format!("type is ({})", wt));
    builder.line(&format!("  r = {}", ctx.take_expr(ra, "x")));
    builder.line("  return");
    builder.line("end select");
    // The model of a running callback: clone its RefAny, so the value the
    // user was handed and the value a new callback gets are one object.
    if let Some((alias, by_value)) = clone {
        builder.line(&format!("do i = min({}, {}), 1, -1", CTX_DEPTH, CTX_MAX));
        builder.indent();
        builder.line(&format!("slot = {}({}(i))", HANDLE_SLOT, CTX_ID));
        builder.line("if (slot == 0) cycle");
        // `associated` compares addresses only, and a model's first
        // component shares its address: the type must match too.
        builder.line(&format!(
            "if (same_type_as({t}(slot)%object, x) .and. associated({t}(slot)%object, x)) then",
            t = HANDLE_TABLE
        ));
        builder.indent();
        if by_value {
            builder.line(&format!("call c_f_pointer({}(i), azul_src)", CTX_REF));
            builder.line(&format!("r = {}(azul_src)", alias));
        } else {
            builder.line(&format!("r = {}({}(i))", alias, CTX_REF));
        }
        builder.line("return");
        builder.dedent();
        builder.line("end if");
        builder.dedent();
        builder.line("end do");
    }
    // Anything else: the binding OWNS a copy; the releaser frees it when
    // libazul drops the last clone of the RefAny.
    builder.line(&format!("slot = {}()", HANDLE_NEW));
    builder.line(&format!(
        "allocate({}(slot)%object, source=x)",
        HANDLE_TABLE
    ));
    builder.line(&format!("r = {}({}(slot)%id)", FFI_REF_ANY_NEW, HANDLE_TABLE));
    builder.dedent();
    builder.line(&format!("end function {}", MODEL_OF));
    builder.blank();

    builder.line(&format!("function {}(object) result(r)", REF_ANY_CREATE));
    builder.indent();
    builder.line("class(*), intent(in), target :: object");
    builder.line(&format!("type({}) :: r", wt));
    builder.line(&format!("r%raw = {}(object)", MODEL_OF));
    builder.line("r%owned = .true.");
    builder.dedent();
    builder.line(&format!("end function {}", REF_ANY_CREATE));
    builder.blank();

    builder.line(&format!("function {}(self) result(p)", REF_ANY_GET));
    builder.indent();
    builder.line(&format!("class({}), intent(in), target :: self", wt));
    builder.line("class(*), pointer :: p");
    builder.line("integer(c_int64_t) :: id");
    builder.line("integer :: slot");
    builder.line("p => null()");
    builder.line(&format!("id = {}(c_loc(self%raw))", FFI_REF_ANY_GET));
    builder.line("if (id == 0) return");
    builder.line(&format!("slot = {}(id)", HANDLE_SLOT));
    builder.line("if (slot == 0) return");
    builder.line(&format!("p => {}(slot)%object", HANDLE_TABLE));
    builder.dedent();
    builder.line(&format!("end function {}", REF_ANY_GET));
    builder.blank();
}
