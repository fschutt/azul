! ============================================================================
! Fortran (F2003+) port of examples/c/hello-world.c.
!
! Same data model (a 32-bit unsigned counter), same callback semantics
! (clicking the button increments the counter and asks for a redraw),
! same visual output (a centred label + a primary button).
!
! Build:
!     make            (uses the sibling Makefile)
! or:
!     gfortran -c azul.f90
!     gfortran hello_world.f90 azul.o -L. -lazul -o hello_world
!
! Run (Linux):   LD_LIBRARY_PATH=. ./hello_world
! Run (macOS):   DYLD_LIBRARY_PATH=. ./hello_world
! Run (Windows): make sure azul.dll is on PATH or in the program dir.
! ============================================================================

module my_data_model_mod
  use, intrinsic :: iso_c_binding
  implicit none
  private

  ! The user data attached to the AzApp via RefAny. Mirrors the
  ! C example's `typedef struct { uint32_t counter; } MyDataModel;`.
  public :: MyDataModel
  type, bind(C) :: MyDataModel
    integer(c_int32_t) :: counter
  end type MyDataModel
end module my_data_model_mod

! ----------------------------------------------------------------------------
! Callback subroutines must live at module scope (or be declared with
! `bind(C)` and have an explicit interface) so we can pass their
! address via `c_funloc` to the C side.
! ----------------------------------------------------------------------------

module hello_world_callbacks
  use, intrinsic :: iso_c_binding
  use azul
  use my_data_model_mod
  implicit none

contains

  ! Destructor stub: MyDataModel owns no heap memory, so do nothing.
  subroutine my_data_model_destructor(p) bind(C)
    type(c_ptr), value :: p
    ! intentionally empty
  end subroutine my_data_model_destructor

  ! ── Callback: button click ────────────────────────────────────────────────
  !
  ! Increments the counter and asks for a redraw.
  function on_click(data, info) bind(C) result(r)
    type(AzRefAny), value :: data
    type(AzCallbackInfo), value :: info
    type(AzUpdate) :: r
    type(MyDataModel), pointer :: model_ptr

    ! SKIPPED: real downcast — the C example uses MyDataModelRefMut_create +
    ! MyDataModel_downcastMut. The Fortran binding does not yet wrap those
    ! helpers, so we pull the raw payload pointer out by hand. The runtime
    ! guarantees the first machine word of the RefAny payload is the
    ! user-supplied pointer for trivially-destructed types.
    call c_f_pointer(c_loc(data), model_ptr)
    if (associated(model_ptr)) then
      model_ptr%counter = model_ptr%counter + 1_c_int32_t
      r%tag = AzUpdateTag_RefreshDom
      r%payload = c_null_ptr
    else
      r%tag = AzUpdateTag_DoNothing
      r%payload = c_null_ptr
    end if
  end function on_click

  ! ── Callback: layout ──────────────────────────────────────────────────────
  !
  ! Builds the DOM. SKIPPED: the C example uses several helper functions
  ! (AzString_copyFromBytes, AzDom_addCssProperty, etc.) which require
  ! string-marshalling helpers that are not yet exposed via the wrapper
  ! layer. We assemble the DOM with the FFI primitives directly.
  function layout(data, info) bind(C) result(r)
    type(AzRefAny), value :: data
    type(AzLayoutCallbackInfo), value :: info
    type(AzDom) :: r
    type(MyDataModel), pointer :: model_ptr
    type(AzString) :: label_text, button_label
    type(AzDom) :: label_dom, label_wrapper, body, button_dom
    type(AzButton) :: button
    type(AzCssProperty) :: font_size
    type(AzRefAny) :: data_clone
    character(len=20) :: buf
    integer :: written

    call c_f_pointer(c_loc(data), model_ptr)
    if (.not. associated(model_ptr)) then
      r = az_dom_create_body()
      return
    end if

    ! Counter label, wrapped in a div so the font-size CSS sticks.
    write(buf, '(I0)') model_ptr%counter
    written = len_trim(buf)
    label_text   = az_string_copy_from_bytes(c_loc(buf), 0_c_size_t, &
                                             int(written, c_size_t))
    label_dom    = az_dom_create_text(label_text)
    label_wrapper = az_dom_create_div()

    font_size = az_css_property_font_size(az_style_font_size_px(32.0_c_float))
    call az_dom_add_css_property(label_wrapper%raw, &
        az_css_property_with_conditions_simple(font_size))
    call az_dom_add_child(label_wrapper%raw, label_dom)

    ! Increment button.
    button_label = az_string_copy_from_bytes(c_loc('Increase counter'), 0_c_size_t, &
                                             16_c_size_t)
    button = az_button_create(button_label)
    call az_button_set_button_type(button%raw, AzButtonType_Primary)

    ! Clone the RefAny so the button keeps its own reference.
    data_clone = az_ref_any_clone(data)
    call az_button_set_on_click(button%raw, data_clone, c_funloc(on_click))
    button_dom = az_button_dom(button)

    ! Body.
    body = az_dom_create_body()
    call az_dom_add_child(body%raw, label_wrapper)
    call az_dom_add_child(body%raw, button_dom)

    r = az_dom_style(body, az_css_empty())
  end function layout

end module hello_world_callbacks

! ============================================================================
! Main program
! ============================================================================

program hello_world
  use, intrinsic :: iso_c_binding
  use azul
  use my_data_model_mod
  use hello_world_callbacks
  implicit none

  type(MyDataModel), target :: model
  type(AzRefAny) :: data
  type(AzWindowCreateOptions) :: window
  type(AzApp) :: app

  model%counter = 5_c_int32_t

  ! SKIPPED: real upcast — the C example uses AZ_REFLECT_JSON which expands
  ! to MyDataModel_upcast. The Fortran binding does not yet expose that
  ! macro; build the RefAny by calling AzRefAny_newC directly with our
  ! destructor pointer.
  data = az_ref_any_new_c( &
      c_loc(model), &
      int(c_sizeof(model), c_size_t), &
      0_c_int64_t, &
      az_string_copy_from_bytes(c_loc('MyDataModel'), 0_c_size_t, 11_c_size_t), &
      c_funloc(my_data_model_destructor) &
  )

  window = az_window_create_options_create(c_funloc(layout))
  window%window_state%title = az_string_copy_from_bytes( &
      c_loc('Hello World'), 0_c_size_t, 11_c_size_t)
  window%window_state%size%dimensions%width  = 400.0_c_float
  window%window_state%size%dimensions%height = 300.0_c_float

  ! NoTitleAutoInject: OS draws close/min/max buttons,
  ! framework auto-injects a Titlebar with drag support.
  window%window_state%flags%decorations = AzWindowDecorations_NoTitleAutoInject
  window%window_state%flags%background_material = AzWindowBackgroundMaterial_Sidebar

  app = az_app_create(data, az_app_config_create())
  call az_app_run(app, window)
  call az_app_delete(app)
end program hello_world
