module counter
  use azul, only: dom_t, button_t, layout_callback_info_t, callback_info_t, &
                  dom_create_body, dom_create_p_with_text, &
                  ButtonType_Primary, Update_DoNothing, Update_RefreshDom, &
                  AppLogLevel_Error
  implicit none

  type :: model_t
    integer :: counter
  end type model_t

contains

  function layout(model, info) result(body)
    class(*), intent(inout) :: model
    type(layout_callback_info_t), intent(inout) :: info
    type(dom_t) :: body, label
    type(button_t) :: button
    character(len=16) :: text

    body = dom_create_body()
    select type (model)
    type is (model_t)
      write (text, '(I0)') model%counter
      label = dom_create_p_with_text(trim(text))
      call label%with_css('font-size: 32px; margin: 0;')

      button = button_t('Increase counter')
      call button%with_button_type(ButtonType_Primary)
      call button%with_on_click(model, on_click)

      call body%with_child(label)
      call body%with_child(button%dom())
    class default
      call info%log(AppLogLevel_Error, 'layout: the model is not a model_t')
    end select
  end function layout

  function on_click(model, info) result(update)
    class(*), intent(inout) :: model
    type(callback_info_t), intent(inout) :: info
    integer :: update

    update = Update_DoNothing
    select type (model)
    type is (model_t)
      model%counter = model%counter + 1
      update = Update_RefreshDom
    class default
      call info%log(AppLogLevel_Error, 'on_click: the model is not a model_t')
      update = Update_DoNothing
    end select
  end function on_click

end module counter

program hello_world
  use azul, only: app_t, app_create, app_config_create, window_create_options_create
  use counter, only: model_t, layout
  implicit none

  type(app_t) :: app

  app = app_create(model_t(counter=5), app_config_create())
  call app%run(window_create_options_create(layout))
end program hello_world
