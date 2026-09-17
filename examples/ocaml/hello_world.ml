type my_data_model = { mutable counter : int }

let layout (m : my_data_model) : Azul.Dom.t =
  let label =
    Azul.Dom.p ~css:"font-size: 32px; margin: 0;" (Int.to_string m.counter)
  in
  let button =
    Azul.Button.create "Increase counter"
      ~btn_type:`Primary
      ~on_click:(fun () ->
          m.counter <- m.counter + 1;
          `RefreshDom)
  in
  Azul.Dom.body ~children:[ label; button ]

let () =
  let model = { counter = 5 } in
  let window = Azul.WindowCreateOptions.create ~layout () in
  let app_config = Azul.AppConfig.create () in
  let app = Azul.App.create ~model ~app_config () in

  Azul.App.run app window
