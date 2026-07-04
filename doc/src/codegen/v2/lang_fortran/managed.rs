//! Fortran (F2003+) managed-FFI runtime helpers (host-invoker pattern).
//!
//! Fortran's `iso_c_binding` exposes `c_funloc` / `c_funptr` for
//! routing Fortran procedures as C function pointers. The host-invoker
//! pattern routes user callbacks through pointer-arg invokers; libazul's
//! static thunks handle the by-value plumbing.
//!
//! ## Output surface
//!
//! Emitted into the `azul` module:
//!
//! 1. **Interface block declarations** for AzApp_setHostHandleReleaser,
//!    AzRefAny_newHostHandle, AzRefAny_getHostHandle, plus per-kind
//!    Az<K>_createFromHostHandle / AzApp_set<K>Invoker bindings.
//! 2. **Abstract interfaces** for the per-kind USER procedures (bind(C),
//!    one `type(c_ptr), value` arg per callback arg + `out_ptr` when the
//!    callback returns non-void).
//! 3. **Module-level handle tables**: refany payloads (id → `type(c_ptr)`)
//!    and callback procedures (id → `type(c_funptr)`), sharing one id
//!    counter so the releaser id space is unambiguous.
//! 4. **A releaser subroutine** that removes entries on last-clone drop.
//! 5. **Per-kind invoker dispatchers** with the REAL C-ABI signature
//!    `(id, arg0.. , out_ptr)` that look up the registered `c_funptr`
//!    and forward via `c_f_procpointer` — plus `azul_register_<kind>`
//!    functions that stash a user procedure and mint the `Az<K>` value.
//! 6. **`azul_refany_create` / `azul_refany_get`** functions.

use super::super::generator::CodeBuilder;
use super::super::ir::{CallbackTypedefDef, CodegenIR};
use super::super::managed_host_invoker::{has_return, host_invoker_kinds, wrapper_name};

/// `["arg0", "arg1", ..]` plus `"out_ptr"` when the callback kind returns
/// a value. Names are uniform `argN` (not the api.json arg names): several
/// of those collide with Fortran keywords (`data`, `result`).
fn invoker_arg_names(cb: &CallbackTypedefDef) -> Vec<String> {
    let mut names: Vec<String> = (0..cb.args.len()).map(|i| format!("arg{}", i)).collect();
    if has_return(cb) {
        names.push("out_ptr".to_string());
    }
    names
}

/// Emit module-level declarations: handle table state, FFI interfaces,
/// per-kind user abstract interfaces. Call BEFORE the `contains` keyword.
pub fn emit_managed_decls(builder: &mut CodeBuilder, ir: &CodegenIR) {
    builder.blank();
    builder.line("! ----------------------------------------------------------------------");
    builder.line("! Managed-FFI runtime helpers (host-invoker pattern).");
    builder.line("! ----------------------------------------------------------------------");
    builder.blank();

    // Module-level mutable state. Fortran's `save` keyword keeps state
    // between procedure calls in a module. Two parallel tables (refany
    // payload pointers / registered callback procedures) share one id
    // counter so releaser ids stay unambiguous.
    builder.line("integer(c_int64_t), allocatable, save :: azul_handle_ids(:)");
    builder.line("type(c_ptr),        allocatable, save :: azul_handle_ptrs(:)");
    builder.line("integer(c_int64_t), allocatable, save :: azul_cb_ids(:)");
    builder.line("type(c_funptr),     allocatable, save :: azul_cb_funptrs(:)");
    builder.line("integer(c_int64_t), save :: azul_next_handle_id = 0");
    builder.blank();

    builder.line("interface");
    builder.indent();

    builder.line("subroutine azul_releaser_iface(id) bind(C)");
    builder.line("  use, intrinsic :: iso_c_binding");
    builder.line("  integer(c_int64_t), value :: id");
    builder.line("end subroutine");

    // FFI bindings for the host-invoker setters/getters/constructors.
    builder.line(
        "subroutine az_app_set_host_handle_releaser(releaser) bind(C, name=\"AzApp_setHostHandleReleaser\")",
    );
    builder.line("  use, intrinsic :: iso_c_binding");
    builder.line("  type(c_funptr), value :: releaser");
    builder.line("end subroutine");

    builder.line("function az_ref_any_new_host_handle(id) bind(C, name=\"AzRefAny_newHostHandle\") result(r)");
    builder.line("  use, intrinsic :: iso_c_binding");
    builder.line("  import :: AzRefAny");
    builder.line("  integer(c_int64_t), value :: id");
    builder.line("  type(AzRefAny) :: r");
    builder.line("end function");

    builder.line("function az_ref_any_get_host_handle(refany) bind(C, name=\"AzRefAny_getHostHandle\") result(r)");
    builder.line("  use, intrinsic :: iso_c_binding");
    builder.line("  import :: AzRefAny");
    builder.line("  type(c_ptr), value :: refany");
    builder.line("  integer(c_int64_t) :: r");
    builder.line("end function");

    for cb in host_invoker_kinds(ir) {
        let wrapper = wrapper_name(cb);
        // setter
        builder.line(&format!(
            "subroutine az_app_set_{w_low}_invoker(invoker) bind(C, name=\"AzApp_set{w}Invoker\")",
            w = wrapper,
            w_low = wrapper.to_lowercase()
        ));
        builder.line("  use, intrinsic :: iso_c_binding");
        builder.line("  type(c_funptr), value :: invoker");
        builder.line("end subroutine");

        // createFromHostHandle
        builder.line(&format!(
            "function az_{w_low}_create_from_host_handle(id) bind(C, name=\"Az{w}_createFromHostHandle\") result(r)",
            w = wrapper,
            w_low = wrapper.to_lowercase()
        ));
        builder.line("  use, intrinsic :: iso_c_binding");
        builder.line(&format!("  import :: Az{}", wrapper));
        builder.line("  integer(c_int64_t), value :: id");
        builder.line(&format!("  type(Az{}) :: r", wrapper));
        builder.line("end function");
    }

    builder.dedent();
    builder.line("end interface");
    builder.blank();

    // Per-kind USER procedure interfaces. A user callback receives the
    // same raw pointer args libazul hands the invoker, minus the handle
    // id (dispatch consumes it). bind(C) so `c_funloc` on a matching
    // procedure is interoperable.
    builder.line("! Per-callback-kind user procedure interfaces. Write a bind(C)");
    builder.line("! subroutine matching one of these and pass it to the matching");
    builder.line("! azul_register_<kind>() to obtain the Az<Kind> callback value.");
    builder.line("abstract interface");
    builder.indent();
    for cb in host_invoker_kinds(ir) {
        let snake = wrapper_name(cb).to_lowercase();
        let args = invoker_arg_names(cb);
        builder.line(&format!(
            "subroutine azul_{}_user_iface({}) bind(C)",
            snake,
            args.join(", ")
        ));
        builder.line("  use, intrinsic :: iso_c_binding");
        for a in &args {
            builder.line(&format!("  type(c_ptr), value :: {}", a));
        }
        builder.line("end subroutine");
    }
    builder.dedent();
    builder.line("end interface");
    builder.blank();

    // Public surface decls.
    builder.line("public :: azul_refany_create");
    builder.line("public :: azul_refany_get");
    builder.line("public :: azul_host_invoker_init");
    for cb in host_invoker_kinds(ir) {
        let snake = wrapper_name(cb).to_lowercase();
        builder.line(&format!("public :: azul_register_{}", snake));
        builder.line(&format!("public :: azul_{}_user_iface", snake));
    }
    builder.blank();
}

/// Emit the implementation bodies. Call AFTER the `contains` keyword.
pub fn emit_managed_bodies(builder: &mut CodeBuilder, ir: &CodegenIR) {
    builder.blank();
    builder.line("! Managed-FFI prelude bodies.");
    builder.blank();

    // azul_alloc_handle
    builder.line("function azul_alloc_handle(value) result(id)");
    builder.indent();
    builder.line("type(c_ptr), value :: value");
    builder.line("integer(c_int64_t) :: id");
    builder.line("integer(c_int64_t), allocatable :: new_ids(:)");
    builder.line("type(c_ptr), allocatable :: new_ptrs(:)");
    builder.line("integer :: n");
    builder.line("azul_next_handle_id = azul_next_handle_id + 1");
    builder.line("id = azul_next_handle_id");
    builder.line("if (.not. allocated(azul_handle_ids)) then");
    builder.line("  allocate(azul_handle_ids(1))");
    builder.line("  allocate(azul_handle_ptrs(1))");
    builder.line("  azul_handle_ids(1) = id");
    builder.line("  azul_handle_ptrs(1) = value");
    builder.line("else");
    builder.line("  n = size(azul_handle_ids)");
    builder.line("  allocate(new_ids(n+1))");
    builder.line("  allocate(new_ptrs(n+1))");
    builder.line("  new_ids(1:n) = azul_handle_ids");
    builder.line("  new_ptrs(1:n) = azul_handle_ptrs");
    builder.line("  new_ids(n+1) = id");
    builder.line("  new_ptrs(n+1) = value");
    builder.line("  call move_alloc(new_ids, azul_handle_ids)");
    builder.line("  call move_alloc(new_ptrs, azul_handle_ptrs)");
    builder.line("end if");
    builder.dedent();
    builder.line("end function azul_alloc_handle");
    builder.blank();

    // azul_lookup_handle
    builder.line("function azul_lookup_handle(id) result(p)");
    builder.indent();
    builder.line("integer(c_int64_t), value :: id");
    builder.line("type(c_ptr) :: p");
    builder.line("integer :: i");
    builder.line("p = c_null_ptr");
    builder.line("if (.not. allocated(azul_handle_ids)) return");
    builder.line("do i = 1, size(azul_handle_ids)");
    builder.line("  if (azul_handle_ids(i) == id) then");
    builder.line("    p = azul_handle_ptrs(i)");
    builder.line("    return");
    builder.line("  end if");
    builder.line("end do");
    builder.dedent();
    builder.line("end function azul_lookup_handle");
    builder.blank();

    // azul_alloc_cb_handle — same id space as azul_alloc_handle, but the
    // payload is a procedure pointer (c_funptr is not a c_ptr, so it
    // lives in its own parallel table).
    builder.line("function azul_alloc_cb_handle(fp) result(id)");
    builder.indent();
    builder.line("type(c_funptr), value :: fp");
    builder.line("integer(c_int64_t) :: id");
    builder.line("integer(c_int64_t), allocatable :: new_ids(:)");
    builder.line("type(c_funptr), allocatable :: new_fps(:)");
    builder.line("integer :: n");
    builder.line("azul_next_handle_id = azul_next_handle_id + 1");
    builder.line("id = azul_next_handle_id");
    builder.line("if (.not. allocated(azul_cb_ids)) then");
    builder.line("  allocate(azul_cb_ids(1))");
    builder.line("  allocate(azul_cb_funptrs(1))");
    builder.line("  azul_cb_ids(1) = id");
    builder.line("  azul_cb_funptrs(1) = fp");
    builder.line("else");
    builder.line("  n = size(azul_cb_ids)");
    builder.line("  allocate(new_ids(n+1))");
    builder.line("  allocate(new_fps(n+1))");
    builder.line("  new_ids(1:n) = azul_cb_ids");
    builder.line("  new_fps(1:n) = azul_cb_funptrs");
    builder.line("  new_ids(n+1) = id");
    builder.line("  new_fps(n+1) = fp");
    builder.line("  call move_alloc(new_ids, azul_cb_ids)");
    builder.line("  call move_alloc(new_fps, azul_cb_funptrs)");
    builder.line("end if");
    builder.dedent();
    builder.line("end function azul_alloc_cb_handle");
    builder.blank();

    // azul_lookup_cb_handle
    builder.line("function azul_lookup_cb_handle(id) result(fp)");
    builder.indent();
    builder.line("integer(c_int64_t), value :: id");
    builder.line("type(c_funptr) :: fp");
    builder.line("integer :: i");
    builder.line("fp = c_null_funptr");
    builder.line("if (.not. allocated(azul_cb_ids)) return");
    builder.line("do i = 1, size(azul_cb_ids)");
    builder.line("  if (azul_cb_ids(i) == id) then");
    builder.line("    fp = azul_cb_funptrs(i)");
    builder.line("    return");
    builder.line("  end if");
    builder.line("end do");
    builder.dedent();
    builder.line("end function azul_lookup_cb_handle");
    builder.blank();

    // Releaser subroutine (bind(C) callable from libazul). Ids are shared
    // between both tables, so purge whichever holds the entry.
    builder.line("subroutine azul_releaser_impl(id) bind(C)");
    builder.indent();
    builder.line("integer(c_int64_t), value :: id");
    builder.line("integer :: i, n");
    builder.line("if (allocated(azul_handle_ids)) then");
    builder.line("  n = size(azul_handle_ids)");
    builder.line("  do i = 1, n");
    builder.line("    if (azul_handle_ids(i) == id) then");
    builder.line("      azul_handle_ids(i) = azul_handle_ids(n)");
    builder.line("      azul_handle_ptrs(i) = azul_handle_ptrs(n)");
    builder.line("      azul_handle_ids = azul_handle_ids(1:n-1)");
    builder.line("      azul_handle_ptrs = azul_handle_ptrs(1:n-1)");
    builder.line("      return");
    builder.line("    end if");
    builder.line("  end do");
    builder.line("end if");
    builder.line("if (allocated(azul_cb_ids)) then");
    builder.line("  n = size(azul_cb_ids)");
    builder.line("  do i = 1, n");
    builder.line("    if (azul_cb_ids(i) == id) then");
    builder.line("      azul_cb_ids(i) = azul_cb_ids(n)");
    builder.line("      azul_cb_funptrs(i) = azul_cb_funptrs(n)");
    builder.line("      azul_cb_ids = azul_cb_ids(1:n-1)");
    builder.line("      azul_cb_funptrs = azul_cb_funptrs(1:n-1)");
    builder.line("      return");
    builder.line("    end if");
    builder.line("  end do");
    builder.line("end if");
    builder.dedent();
    builder.line("end subroutine azul_releaser_impl");
    builder.blank();

    // Per-kind invoker dispatchers with the REAL C-ABI signature. libazul
    // calls these with (id, one pointer per callback arg, out_ptr when the
    // callback returns non-void); we look up the user's registered
    // procedure and forward everything but the id.
    for cb in host_invoker_kinds(ir) {
        let snake = wrapper_name(cb).to_lowercase();
        let args = invoker_arg_names(cb);
        builder.line(&format!(
            "subroutine azul_{}_invoker_impl(id, {}) bind(C)",
            snake,
            args.join(", ")
        ));
        builder.indent();
        builder.line("integer(c_int64_t), value :: id");
        for a in &args {
            builder.line(&format!("type(c_ptr), value :: {}", a));
        }
        builder.line(&format!("procedure(azul_{}_user_iface), pointer :: fp", snake));
        builder.line("type(c_funptr) :: raw");
        builder.line("raw = azul_lookup_cb_handle(id)");
        builder.line("if (.not. c_associated(raw)) return");
        builder.line("call c_f_procpointer(raw, fp)");
        builder.line(&format!("call fp({})", args.join(", ")));
        builder.dedent();
        builder.line(&format!("end subroutine azul_{}_invoker_impl", snake));
        builder.blank();
    }

    // Register-callback functions (one per kind). Stash the user procedure
    // in the callback table and mint the matching Az<K> value whose id
    // round-trips through libazul back into the dispatcher above.
    for cb in host_invoker_kinds(ir) {
        let wrapper = wrapper_name(cb);
        let snake = wrapper.to_lowercase();
        builder.line(&format!("function azul_register_{}(cb) result(r)", snake));
        builder.indent();
        builder.line(&format!("procedure(azul_{}_user_iface) :: cb", snake));
        builder.line(&format!("type(Az{}) :: r", wrapper));
        builder.line("integer(c_int64_t) :: id");
        builder.line("id = azul_alloc_cb_handle(c_funloc(cb))");
        builder.line(&format!("r = az_{}_create_from_host_handle(id)", snake));
        builder.dedent();
        builder.line(&format!("end function azul_register_{}", snake));
        builder.blank();
    }

    // Public azul_refany_create / azul_refany_get
    builder.line("function azul_refany_create(value) result(r)");
    builder.indent();
    builder.line("type(c_ptr), value :: value");
    builder.line("type(AzRefAny) :: r");
    builder.line("integer(c_int64_t) :: id");
    builder.line("id = azul_alloc_handle(value)");
    builder.line("r = az_ref_any_new_host_handle(id)");
    builder.dedent();
    builder.line("end function azul_refany_create");
    builder.blank();

    builder.line("function azul_refany_get(refany_ptr) result(p)");
    builder.indent();
    builder.line("type(c_ptr), value :: refany_ptr");
    builder.line("type(c_ptr) :: p");
    builder.line("integer(c_int64_t) :: id");
    builder.line("id = az_ref_any_get_host_handle(refany_ptr)");
    builder.line("if (id == 0) then");
    builder.line("  p = c_null_ptr");
    builder.line("else");
    builder.line("  p = azul_lookup_handle(id)");
    builder.line("end if");
    builder.dedent();
    builder.line("end function azul_refany_get");
    builder.blank();

    // Init subroutine — register releaser + per-kind invoker dispatchers.
    builder.line("subroutine azul_host_invoker_init()");
    builder.indent();
    builder.line("call az_app_set_host_handle_releaser(c_funloc(azul_releaser_impl))");
    for cb in host_invoker_kinds(ir) {
        let snake = wrapper_name(cb).to_lowercase();
        builder.line(&format!(
            "call az_app_set_{snake}_invoker(c_funloc(azul_{snake}_invoker_impl))",
            snake = snake
        ));
    }
    builder.dedent();
    builder.line("end subroutine azul_host_invoker_init");
    builder.blank();
}
