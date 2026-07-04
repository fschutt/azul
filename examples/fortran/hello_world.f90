! Full-GUI Fortran hello-world: counter label + "Increase counter" button.
!
! Build & run:  make && ./hello_world     (Makefile ships next to azul.f90)
!
! Callbacks go through azul.f90's host-invoker dispatch: register a
! bind(C) module procedure via azul_register_<kind>() and the returned
! Az<Kind>Callback value round-trips its handle id back into the
! registered procedure. Callbacks MUST live in a module (not as internal
! procedures) so c_funloc() needs no executable-stack trampoline.

module hello_impl
  use, intrinsic :: iso_c_binding
  use azul
  implicit none

  type :: t_model
    integer :: counter = 5
  end type t_model
  type(t_model), target, save :: model

contains

  function mk_str(s) result(r)
    character(len=*), intent(in) :: s
    type(AzString) :: r
    character(kind=c_char), dimension(max(len(s), 1)), target :: buf
    integer :: i
    do i = 1, len(s)
      buf(i) = s(i:i)
    end do
    ! AzString_fromUtf8 copies the bytes, so the automatic buffer is fine.
    r = az_string_from_utf8(c_loc(buf(1)), int(len(s), c_size_t))
  end function mk_str

  ! ButtonOnClick user callback: bump the counter, request a DOM refresh.
  ! arg0 = AzRefAny* (model handle), arg1 = CallbackInfo*, out_ptr = AzUpdate*.
  subroutine my_on_click(arg0, arg1, out_ptr) bind(C)
    type(c_ptr), value :: arg0, arg1, out_ptr
    type(c_ptr) :: praw
    type(t_model), pointer :: m
    integer(c_int), pointer :: update_out
    praw = azul_refany_get(arg0)
    if (c_associated(praw)) then
      call c_f_pointer(praw, m)
      m%counter = m%counter + 1
    end if
    if (c_associated(out_ptr)) then
      call c_f_pointer(out_ptr, update_out)
      update_out = AzUpdate_RefreshDom
    end if
    if (c_associated(arg1)) return
  end subroutine my_on_click

  ! Layout user callback: build body > [ div.font-size-32 > text(counter),
  ! button ]. arg0 = AzRefAny*, arg1 = LayoutCallbackInfo*, out_ptr = AzDom*.
  subroutine my_layout(arg0, arg1, out_ptr) bind(C)
    type(c_ptr), value :: arg0, arg1, out_ptr
    type(c_ptr) :: praw
    type(t_model), pointer :: m
    type(AzDom), pointer :: dom_out
    type(AzDom) :: body, label_wrap
    type(AzButton) :: btn
    type(AzButtonOnClickCallback) :: click_cb
    type(AzRefAny) :: click_data
    character(len=32) :: num
    body = az_dom_create_body()
    praw = azul_refany_get(arg0)
    if (c_associated(praw)) then
      call c_f_pointer(praw, m)
      write (num, '(I0)') m%counter

      label_wrap = az_dom_create_div()
      label_wrap = az_dom_with_css(label_wrap, mk_str('font-size: 32px;'))
      label_wrap = az_dom_with_child(label_wrap, &
                                     az_dom_create_text(mk_str(trim(num))))

      click_cb = azul_register_buttononclickcallback(my_on_click)
      click_data = azul_refany_create(c_loc(model))
      btn = az_button_create(mk_str('Increase counter'))
      btn = az_button_with_button_type(btn, AzButtonType_Primary)
      btn = az_button_with_on_click(btn, click_data, click_cb)

      body = az_dom_with_child(body, label_wrap)
      body = az_dom_with_child(body, az_button_dom(btn))
    end if
    if (c_associated(out_ptr)) then
      call c_f_pointer(out_ptr, dom_out)
      dom_out = body
    end if
    if (c_associated(arg1)) return
  end subroutine my_layout

end module hello_impl

program hello_world
  use, intrinsic :: iso_c_binding
  use azul
  use hello_impl
  implicit none

  ! NB: Fortran is case-insensitive — `app` would collide with the
  ! wrapper type `App` exported by the azul module.
  type(AzRefAny) :: app_data
  type(AzLayoutCallback) :: layout_cb
  type(AzWindowCreateOptions) :: wco
  type(AzApp), target :: the_app

  print '(A)', '[azul] Fortran full-GUI hello-world starting.'

  call azul_host_invoker_init()

  app_data = azul_refany_create(c_loc(model))
  layout_cb = azul_register_layoutcallback(my_layout)

  wco = az_window_create_options_default()
  wco%window_state%layout_callback = layout_cb
  wco%window_state%title = mk_str('Hello World')

  the_app = az_app_create(app_data, az_app_config_create())
  call az_app_run(c_loc(the_app), wco)
end program hello_world
