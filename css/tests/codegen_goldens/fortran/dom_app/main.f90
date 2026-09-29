! Build the azul modules first (target/codegen/fortran: make), then:
!   gfortran -ffree-line-length-none styles.f90 main.f90 azul*.o -L. -lazul -o main && ./main
program main
  use azul
  use azul_styles
  implicit none

end program main
