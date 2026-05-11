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
//! 2. **Module-level handle table** (parallel arrays of id + an opaque
//!    `type(c_ptr)` to a Fortran-side payload).
//! 3. **A releaser subroutine** that removes entries on last-clone drop.
//! 4. **Per-kind invoker subroutine stubs**.
//! 5. **`azul_refany_create` / `azul_refany_get`** functions.

use super::super::generator::CodeBuilder;
use super::super::ir::CodegenIR;
use super::super::managed_host_invoker::{has_return, host_invoker_kinds, wrapper_name};

/// Emit module-level declarations: handle table state, releaser proc,
/// per-kind invoker procs. Call BEFORE the `contains` keyword.
pub fn emit_managed_decls(builder: &mut CodeBuilder, ir: &CodegenIR) {
    builder.blank();
    builder.line("! ----------------------------------------------------------------------");
    builder.line("! Managed-FFI runtime helpers (host-invoker pattern).");
    builder.line("! ----------------------------------------------------------------------");
    builder.blank();

    // Module-level mutable state. Fortran's `save` keyword keeps state
    // between procedure calls in a module.
    builder.line("integer(c_int64_t), allocatable, save :: azul_handle_ids(:)");
    builder.line("type(c_ptr),        allocatable, save :: azul_handle_ptrs(:)");
    builder.line("integer(c_int64_t), save :: azul_next_handle_id = 0");
    builder.blank();

    builder.line("interface");
    builder.indent();

    // Per-kind invoker abstract interfaces. We declare them inside
    // a leading `abstract interface` block so the `procedure(iface)`
    // pointer declarations below resolve. Each has a uint64 id +
    // pointer-shaped args + optional out-pointer for non-void returns.
    let _ = ir;

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
        let _ = has_return(cb);
    }

    builder.dedent();
    builder.line("end interface");
    builder.blank();

    // Public surface decls.
    builder.line("public :: azul_refany_create");
    builder.line("public :: azul_refany_get");
    builder.line("public :: azul_host_invoker_init");
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

    // Releaser subroutine (bind(C) callable from libazul).
    builder.line("subroutine azul_releaser_impl(id) bind(C)");
    builder.indent();
    builder.line("integer(c_int64_t), value :: id");
    builder.line("integer :: i, n");
    builder.line("if (.not. allocated(azul_handle_ids)) return");
    builder.line("n = size(azul_handle_ids)");
    builder.line("do i = 1, n");
    builder.line("  if (azul_handle_ids(i) == id) then");
    builder.line("    azul_handle_ids(i) = azul_handle_ids(n)");
    builder.line("    azul_handle_ptrs(i) = azul_handle_ptrs(n)");
    builder.line("    azul_handle_ids = azul_handle_ids(1:n-1)");
    builder.line("    azul_handle_ptrs = azul_handle_ptrs(1:n-1)");
    builder.line("    return");
    builder.line("  end if");
    builder.line("end do");
    builder.dedent();
    builder.line("end subroutine azul_releaser_impl");
    builder.blank();

    // Per-kind invoker stubs (empty bodies — second-pass agent will
    // wire user callback dispatch).
    for cb in host_invoker_kinds(ir) {
        let wrapper = wrapper_name(cb);
        let snake = wrapper.to_lowercase();
        builder.line(&format!(
            "subroutine azul_{}_invoker_stub(id) bind(C)",
            snake
        ));
        builder.indent();
        builder.line("integer(c_int64_t), value :: id");
        builder.line("! First-pass plumbing only; user dispatch is the second-pass agent's job.");
        builder.line("if (id == 0) return");
        builder.dedent();
        builder.line(&format!("end subroutine azul_{}_invoker_stub", snake));
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

    // Init subroutine — register releaser + per-kind invokers.
    builder.line("subroutine azul_host_invoker_init()");
    builder.indent();
    builder.line("call az_app_set_host_handle_releaser(c_funloc(azul_releaser_impl))");
    for cb in host_invoker_kinds(ir) {
        let wrapper = wrapper_name(cb);
        let snake = wrapper.to_lowercase();
        builder.line(&format!(
            "call az_app_set_{snake}_invoker(c_funloc(azul_{snake}_invoker_stub))",
            snake = snake
        ));
    }
    builder.dedent();
    builder.line("end subroutine azul_host_invoker_init");
    builder.blank();
}
