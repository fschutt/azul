! Build the azul modules first (target/codegen/fortran: make), then:
!   gfortran -ffree-line-length-none styles.f90 main.f90 azul*.o -L. -lazul -o main && ./main
program main
  use azul
  use azul_styles
  implicit none
  type(AzCssPropertyWithConditionsVec) :: style_btn_value

  style_btn_value = style_btn()
  print '(A, I0, A)', 'style_btn: ', style_btn_value%len_, ' properties'
end program main
