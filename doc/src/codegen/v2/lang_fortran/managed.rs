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
//! WRAPPER types (`type(ref_any_t), intent(inout) :: arg0`), not raw
//! `type(c_ptr)`s, and whose result is the mapped return
//! (`type(dom_t)` for `LayoutCallback`, `integer` for `Update`-returning
//! kinds). The `bind(C)` boundary is the generated invoker, not the
//! user's procedure:
//!
//! ```text
//! libazul --(id, const AzRefAny*, const AzCallbackInfo*, AzUpdate*)-->
//!     azul_button_on_click_callback_invoker  (bind(C))
//!         --> handle slot lookup --> fp(a0, a1) --> out_ptr write
//! ```
//!
//! so a user callback is an ordinary Fortran module function and needs
//! no `iso_c_binding`, no `c_f_pointer`, and no out-pointer writes.

use super::{
    super::{
        generator::CodeBuilder,
        ir::CallbackTypedefDef,
        managed_host_invoker::{has_return, host_invoker_kinds, to_snake_case, wrapper_name},
    },
    ffi_type_name, map_type_to_fortran, truncate_identifier,
    wrappers::{Ctx, UserType},
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

// ============================================================================
// Signature plan for one callback kind
// ============================================================================

/// How one callback argument crosses the invoker boundary.
struct IfaceArg {
    /// Dummy declaration inside the abstract interface.
    dummy: String,
    /// User-facing local in the invoker (`a0`, `a1`, ...).
    local: String,
    /// FFI-typed pointer local the incoming `type(c_ptr)` is mapped to.
    ptr: String,
    /// Statements that fill `local` from the incoming pointer.
    unpack: Vec<String>,
    /// Actual-argument expression handed to the user procedure.
    actual: String,
}

/// How the callback's result crosses back.
struct IfaceRet {
    /// Result declaration inside the abstract interface (`type(dom_t) :: r`).
    iface_decl: String,
    /// Local result declaration inside the invoker.
    local_decl: String,
    /// Declaration of the out-parameter pointer.
    out_decl: String,
    /// Assignment written through the out pointer.
    write: String,
}

fn plan_iface_arg(ctx: &Ctx, idx: usize, type_name: &str) -> IfaceArg {
    let dummy = format!("arg{}", idx);
    let local = format!("azul_a{}", idx);
    let ptr = format!("azul_p{}", idx);
    let ffi = map_type_to_fortran(type_name, ctx.ir);
    let (dummy_decl, local_decl, init, assign) = match ctx.classify(type_name) {
        UserType::Wrapper(w) => {
            let wt = ctx.wt(&w);
            (
                format!("type({}), intent(inout) :: {}", wt, dummy),
                format!("type({}) :: {}", wt, local),
                None,
                format!("{}%raw = {}", local, ptr),
            )
        }
        UserType::Enum => (
            format!("integer, intent(in) :: {}", dummy),
            format!("integer :: {}", local),
            Some(format!("{} = 0", local)),
            format!("{} = int({})", local, ptr),
        ),
        UserType::Bool => (
            format!("logical, intent(in) :: {}", dummy),
            format!("logical :: {}", local),
            Some(format!("{} = .false.", local)),
            format!("{} = logical({})", local, ptr),
        ),
        // Strings and every other shape stay in their FFI spelling: no
        // callback kind in HOST_INVOKER_KINDS takes one today, and a
        // borrowed `AzString` must not be consumed by the `character`
        // helper (which deletes what it reads).
        UserType::Str | UserType::Kind(_) | UserType::Raw(_) => (
            format!("{}, intent(in) :: {}", ffi, dummy),
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
        local: local_decl,
        ptr: format!("{}, pointer :: {}", ffi, ptr),
        unpack,
        actual: local,
    }
}

fn plan_iface_ret(ctx: &Ctx, ret: &str) -> IfaceRet {
    let ffi = map_type_to_fortran(ret, ctx.ir);
    let (iface, local, write) = match ctx.classify(ret) {
        UserType::Wrapper(w) => {
            let wt = ctx.wt(&w);
            (
                format!("type({}) :: r", wt),
                format!("type({}) :: azul_r", wt),
                "azul_out = azul_r%raw".to_string(),
            )
        }
        UserType::Enum => (
            "integer :: r".to_string(),
            "integer :: azul_r".to_string(),
            "azul_out = int(azul_r, c_int)".to_string(),
        ),
        UserType::Bool => (
            "logical :: r".to_string(),
            "logical :: azul_r".to_string(),
            "azul_out = logical(azul_r, c_bool)".to_string(),
        ),
        UserType::Str | UserType::Kind(_) | UserType::Raw(_) => (
            format!("{} :: r", ffi),
            format!("{} :: azul_r", ffi),
            "azul_out = azul_r".to_string(),
        ),
    };
    IfaceRet {
        iface_decl: iface,
        local_decl: local,
        out_decl: format!("{}, pointer :: azul_out", ffi),
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

// ============================================================================
// Declarations (before `contains`)
// ============================================================================

/// Emit the abstract interfaces, the handle table and the host-invoker
/// FFI block. Must run AFTER the wrapper type declarations (the
/// interfaces `import` them) and BEFORE `contains`.
pub(crate) fn emit_managed_decls(builder: &mut CodeBuilder, ctx: &Ctx) {
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

    if ctx.ref_any.is_some() {
        builder.line(&format!("public :: {}", REF_ANY_CREATE));
    }
    for cb in &ctx.kinds {
        let k = wrapper_name(cb);
        builder.line(&format!("public :: {}", iface_name(k)));
        builder.line(&format!("public :: {}", register_name(k)));
    }
    builder.blank();
}

fn emit_abstract_interface(builder: &mut CodeBuilder, ctx: &Ctx, cb: &CallbackTypedefDef) {
    let k = wrapper_name(cb);
    let name = iface_name(k);
    let args: Vec<IfaceArg> = cb
        .args
        .iter()
        .enumerate()
        .map(|(i, a)| plan_iface_arg(ctx, i, &a.type_name))
        .collect();
    let names: Vec<String> = (0..cb.args.len()).map(|i| format!("arg{}", i)).collect();
    let ret = ret_type(cb).map(|r| plan_iface_ret(ctx, r));

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
}

fn emit_invoker(builder: &mut CodeBuilder, ctx: &Ctx, cb: &CallbackTypedefDef) {
    let k = wrapper_name(cb);
    let name = invoker_name(k);
    let args: Vec<IfaceArg> = cb
        .args
        .iter()
        .enumerate()
        .map(|(i, a)| plan_iface_arg(ctx, i, &a.type_name))
        .collect();
    let dummies: Vec<String> = (0..cb.args.len()).map(|i| format!("arg{}", i)).collect();
    let ret = ret_type(cb).map(|r| plan_iface_ret(ctx, r));

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
        builder.line(&a.local);
        builder.line(&a.ptr);
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
    for a in &args {
        for l in &a.unpack {
            builder.line(l);
        }
    }
    let actuals: Vec<&str> = args.iter().map(|a| a.actual.as_str()).collect();
    match &ret {
        Some(r) => {
            builder.line(&format!("azul_r = azul_fp({})", actuals.join(", ")));
            builder.line("if (c_associated(out_ptr)) then");
            builder.line("  call c_f_pointer(out_ptr, azul_out)");
            builder.line(&format!("  {}", r.write));
            builder.line("end if");
        }
        None => builder.line(&format!("call azul_fp({})", actuals.join(", "))),
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

    builder.line(&format!("function {}(object) result(r)", REF_ANY_CREATE));
    builder.indent();
    builder.line("class(*), intent(in) :: object");
    builder.line(&format!("type({}) :: r", wt));
    builder.line("integer :: slot");
    builder.line(&format!("slot = {}()", HANDLE_NEW));
    // The binding OWNS this copy; the releaser frees it when libazul
    // drops the last clone of the RefAny (Python/Haskell semantics).
    builder.line(&format!(
        "allocate({}(slot)%object, source=object)",
        HANDLE_TABLE
    ));
    builder.line(&format!(
        "r%raw = {}({}(slot)%id)",
        FFI_REF_ANY_NEW, HANDLE_TABLE
    ));
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
