module hello_impl
  use azul
  implicit none

  type :: t_model
    integer :: counter = 5
  end type t_model

contains

  function layout(data, info) result(body)
    type(ref_any_t), intent(inout) :: data
    type(layout_callback_info_t), intent(inout) :: info
    type(dom_t) :: body
    class(*), pointer :: model
    type(dom_t) :: label
    type(button_t) :: button
    character(len=16) :: text

    model => data%get()
    select type (model)
    type is (t_model)
      write (text, '(I0)') model%counter
    class default
      text = '?'
    end select

    label = dom_create_p_with_text(trim(text))
    call label%with_css('font-size: 32px; margin: 0;')

    button = button_create('Increase counter')
    call button%with_button_type(ButtonType_Primary)
    call button%with_on_click(data, on_click)

    body = dom_create_body()
    call body%with_child(label)
    call body%with_child(button%dom())
  end function layout

  function on_click(data, info) result(update)
    type(ref_any_t), intent(inout) :: data
    type(callback_info_t), intent(inout) :: info
    integer :: update
    class(*), pointer :: model

    model => data%get()
    select type (model)
    type is (t_model)
      model%counter = model%counter + 1
    end select
    update = Update_RefreshDom
  end function on_click

end module hello_impl

program hello_world
  use azul
  use hello_impl
  implicit none

  type(app_t) :: app
  type(window_create_options_t) :: window

  app = app_create(ref_any_create(t_model(5)), app_config_create())
  window = window_create_options_create(layout)
  call app%run(window)
end program hello_world
