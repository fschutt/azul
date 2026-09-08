! Memory test for the azul Fortran binding. See tests/memtest/README.md.
!
! The harness (scripts/run_memtest.sh) measures peak RSS across a small and a
! large AZ_MEMTEST_N (RSS that scales with N is a LEAK) and fails on any crash.
! This file only exercises the create/consume/DROP paths in a loop and exits 0.
! No event loop (app%run needs a display and hangs headless).
!
! Uses the wrapper layer, like examples/fortran. Build with the shipped
! Makefile.fortran (drop azul.f90 + libazul.so next to this file).

program mem_test
  use azul
  implicit none

  type :: t_model
    integer :: counter
  end type t_model

  integer :: n, i, ios, ln
  type(app_t) :: the_app
  type(app_config_t) :: cfg
  character(len=32) :: nval

  n = 200000
  call get_environment_variable('AZ_MEMTEST_N', nval, ln)
  if (ln > 0) then
    read(nval, *, iostat=ios) i
    if (ios == 0) n = i
  end if

  ! 1. The consume-by-value DROP path: app_create moves the AppConfig bytes
  !    (nested SystemStyle) into libazul; the%delete drops the App once. The
  !    RefAny owns a binding-side copy of t_model, freed by the handle-table
  !    releaser when libazul drops the last clone.
  the_app = app_create(ref_any_create(t_model(5)), app_config_create())
  call the_app%delete()

  ! 2. Leak loop: create/destroy a droppable AppConfig N times.
  !    app_config_delete drops the nested SystemStyle every iteration.
  do i = 1, n
    cfg = app_config_create()
    call cfg%delete()
  end do

  print '(A,I0,A)', 'memtest fortran OK (N=', n, ')'
end program mem_test
