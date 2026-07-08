! Memory test for the azul Fortran binding. See tests/memtest/README.md.
!
! The harness (scripts/run_memtest.sh) measures peak RSS across a small and a
! large AZ_MEMTEST_N (RSS that scales with N is a LEAK) and fails on any crash.
! This file only exercises the create/consume/DROP paths in a loop and exits 0.
! No event loop (az_app_run needs a display and hangs headless).
!
! Uses the raw az_* C-ABI wrappers, like examples/fortran. Build with the
! shipped Makefile.fortran (drop azul.f90 + libazul.so next to this file).

program mem_test
  use, intrinsic :: iso_c_binding
  use azul
  implicit none

  integer :: n, i, ios, ln
  integer(c_int), target :: model_val
  type(AzRefAny) :: app_data
  type(AzAppConfig), target :: cfg
  type(AzApp), target :: the_app
  character(len=32) :: nval

  n = 200000
  call get_environment_variable('AZ_MEMTEST_N', nval, ln)
  if (ln > 0) then
    read(nval, *, iostat=ios) i
    if (ios == 0) n = i
  end if

  ! Host-invoker dispatch must be initialised before azul_refany_create.
  call azul_host_invoker_init()

  ! 1. The consume-by-value DROP path: az_app_create moves the AppConfig bytes
  !    (nested SystemStyle) into libazul; az_app_delete drops the App once.
  model_val = 5
  app_data = azul_refany_create(c_loc(model_val))
  the_app = az_app_create(app_data, az_app_config_create())
  call az_app_delete(c_loc(the_app))

  ! 2. Leak loop: create/destroy a droppable AppConfig N times.
  !    az_app_config_delete drops the nested SystemStyle every iteration.
  do i = 1, n
    cfg = az_app_config_create()
    call az_app_config_delete(c_loc(cfg))
  end do

  print '(A,I0,A)', 'memtest fortran OK (N=', n, ')'
end program mem_test
