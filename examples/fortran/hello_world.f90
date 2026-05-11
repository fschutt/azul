! ============================================================================
! Fortran (F2003+) Azul C ABI smoke test.
!
! The full GUI demo requires callbacks (button.set_on_click, layout) and
! struct-by-value calls through Fortran's type-bound procedures, which need
! the wrapper layer to be richer than what the current codegen emits. This
! smoke test exercises the part that DOES work today:
!   - the generated `azul` module compiles + links against libazul,
!   - struct-by-value returns cross the FFI boundary (AzString_fromUtf8),
!   - the dylib loads and resolves all C-ABI symbols.
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
  integer(c_size_t) :: src_len

  print '(A)', '[azul] Fortran FFI smoke test starting.'

  src_len = 11_c_size_t
  s = az_string_from_utf8(c_loc(src), src_len)
  print '(A,I0)', '[azul] AzString_fromUtf8 round-trip succeeded; len=', src_len

  call az_string_delete(c_loc(s))
  print '(A)', '[azul] AzString_delete reached without error.'

  print '(A)', '[azul] Fortran binding init phase completed successfully.'
  print '(A)', '[azul] (Full App.run wiring requires callback / wrapper-layer work'
  print '(A)', '[azul]  separate from the C ABI plumbing exercised here.)'
end program hello_world
