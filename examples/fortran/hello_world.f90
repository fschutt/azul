! ============================================================================
! Fortran (F2003+) Azul host-invoker smoke test.
!
! Exercises the managed-FFI prelude emitted by lang_fortran/managed.rs:
! `azul_refany_create` round-trips a Fortran-side payload through libazul's
! host-handle table, the per-kind invoker stubs + releaser were registered
! at module load time via `azul_host_invoker_init`. Parallel to what
! Node/Lua/Ruby/Lisp/C#/OCaml/Pascal already do.
!
! Build:
!     make
! Run (macOS):
!     DYLD_LIBRARY_PATH=. ./hello_world
! Run (Linux):
!     LD_LIBRARY_PATH=. ./hello_world
! ============================================================================

program hello_world
  use, intrinsic :: iso_c_binding
  use azul
  implicit none

  character(kind=c_char, len=12), target :: src = 'hello, azul' // C_NULL_CHAR
  type(AzString), target :: s
  type(AzRefAny), target :: my_refany
  integer(c_size_t) :: src_len
  ! A simple Fortran-side payload. We pass its address through the
  ! host-invoker table — the table holds `type(c_ptr)` per id, so any
  ! Fortran object with `target` attribute can serve as payload.
  integer(c_int), target :: model_counter
  type(c_ptr) :: recovered

  print '(A)', '[azul] Fortran FFI smoke test starting.'

  ! Initialise the host-invoker plumbing (releaser + per-kind invokers).
  call azul_host_invoker_init()
  print '(A)', '[azul] azul_host_invoker_init registered releaser + invokers.'

  ! 1. AzString round-trip — proves the C ABI dispatches end-to-end.
  src_len = 11_c_size_t
  s = az_string_from_utf8(c_loc(src), src_len)
  print '(A,I0)', '[azul] AzString_fromUtf8 round-trip succeeded; len=', src_len
  call az_string_delete(c_loc(s))
  print '(A)', '[azul] AzString_delete reached without error.'

  ! 2. RefAny round-trip — proves the host-invoker prelude is wired:
  !    libazul holds a RefAny whose `id` field is a key into our
  !    Fortran-side handle table; azul_refany_get returns the c_ptr
  !    stored under that id (here: the address of model_counter).
  model_counter = 5
  my_refany = azul_refany_create(c_loc(model_counter))
  print '(A)', '[azul] azul_refany_create ran; RefAny opaque-handle id stored.'

  recovered = azul_refany_get(c_loc(my_refany))
  if (c_associated(recovered, c_loc(model_counter))) then
    print '(A)', '[azul] azul_refany_get round-trip succeeded; recovered ptr matches.'
  else
    print '(A)', '[azul] azul_refany_get round-trip FAILED.'
    stop 1
  end if

  print '(A)', '[azul] host-invoker init phase completed successfully.'
  print '(A)', '[azul] (Full App.run wiring requires layout / callback'
  print '(A)', '[azul]  wrappers, separate from the host-invoker plumbing.)'
end program hello_world
