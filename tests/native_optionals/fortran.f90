! Fortran: no optional type, and a tagged union is an opaque blob. An Option
! is built with the exports (az_option_string_none / _some) and read with the
! generic azul_try_get_some / azul_try_get_ok / azul_try_get_err. Built and
! run by scripts/native_optionals_smoke.sh.
module smoke_model
  use azul, only: callback_info_t, ref_any_t, request_id_t, Update_DoNothing, Update_RefreshDom, &
                  AzOptionFileOpenResult, AzFileOpenResult, AzFilePath, azul_try_get_some, &
                  azul_string_value, file_open_result_downcast, az_option_file_open_result_delete, &
                  file_dialog_open_directory, az_option_string_none, c_loc
  implicit none

  type :: model_t
    character(len=:), allocatable :: folder
  end type model_t

contains

  ! The guide's file-dialog example, as written there (hello-world/fortran.md).
  function on_pick_folder(model, info, ref_any) result(update)
    class(*), intent(inout) :: model
    type(callback_info_t), intent(inout) :: info
    type(ref_any_t), intent(inout) :: ref_any
    integer :: update
    type(AzOptionFileOpenResult), target :: picked
    type(AzFileOpenResult) :: opened
    type(AzFilePath) :: path

    update = Update_DoNothing
    picked = file_open_result_downcast(ref_any)
    if (azul_try_get_some(picked, opened)) then
      if (azul_try_get_some(opened%path, path)) then
        select type (model)
        type is (model_t)
          model%folder = azul_string_value(path%inner)
          update = Update_RefreshDom
        end select
      end if
    end if
    call az_option_file_open_result_delete(c_loc(picked))
  end function on_pick_folder

  function on_choose_folder(model, info) result(update)
    class(*), intent(inout) :: model
    type(callback_info_t), intent(inout) :: info
    integer :: update
    type(request_id_t) :: request

    request = file_dialog_open_directory('Choose a folder', az_option_string_none(), model, on_pick_folder)
    update = Update_DoNothing
  end function on_choose_folder

end module smoke_model

program native_optionals
  use azul, only: AzIconVariants, AzString, AzUrl, AzUrlParseError, AzResultUrlUrlParseError, &
                  az_icon_variants_create_default, az_icon_variants_delete, az_option_string_some, &
                  az_option_string_none, az_option_string_delete, azul_string, azul_string_value, &
                  azul_try_get_some, azul_try_get_ok, azul_try_get_err, az_url_parse, &
                  az_result_url_url_parse_error_delete, c_loc
  use iso_fortran_env, only: error_unit
  use smoke_model, only: on_choose_folder
  implicit none

  type(AzIconVariants), target :: icons
  type(AzString) :: light
  type(AzResultUrlUrlParseError), target :: parsed
  type(AzUrl) :: url
  type(AzUrlParseError) :: err

  ! A field: None in, None out; Some in, Some out.
  icons = az_icon_variants_create_default()
  call check(.not. azul_try_get_some(icons%light, light), 'a default IconVariants has no light icon')
  call az_option_string_delete(c_loc(icons%light))
  icons%light = az_option_string_some(azul_string('light.png'))
  call check(azul_try_get_some(icons%light, light), 'Some in, Some out')
  call check(azul_string_value(light) == 'light.png', 'the value comes back')
  call az_option_string_delete(c_loc(icons%light))
  icons%light = az_option_string_none()
  call check(.not. azul_try_get_some(icons%light, light), 'None in, None out')
  call az_icon_variants_delete(c_loc(icons))

  ! A result: Ok and Err.
  parsed = az_url_parse(azul_string('https://azul.rs/guide'))
  call check(azul_try_get_ok(parsed, url) .and. .not. azul_try_get_err(parsed, err), 'Url parse Ok')
  call az_result_url_url_parse_error_delete(c_loc(parsed))
  parsed = az_url_parse(azul_string('no scheme'))
  call check(azul_try_get_err(parsed, err) .and. .not. azul_try_get_ok(parsed, url), 'Url parse Err')
  call az_result_url_url_parse_error_delete(c_loc(parsed))

  print '(a)', 'fortran native optionals: ok'

contains

  subroutine check(ok, what)
    logical, intent(in) :: ok
    character(len=*), intent(in) :: what
    if (.not. ok) then
      write (error_unit, '(a)') 'FAIL: ' // what
      stop 1
    end if
  end subroutine check

end program native_optionals
