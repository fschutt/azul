type my_data_model = { mutable counter : int }

(* Converts between my_data_model and Azul.RefAny.t in both directions *)
let model : my_data_model Azul.RefAny.key = Azul.RefAny.key "my_data_model"

let on_click (m : my_data_model) (_info : Azul.CallbackInfo.t) : Azul.Update.t =
  m.counter <- m.counter + 1;
  Azul.Update.RefreshDom

let layout (m : my_data_model) (_info : Azul.LayoutCallbackInfo.t) : Azul.Dom.t =
  let label =
    Azul.Dom.p ~css:"font-size: 32px; margin: 0;" (Int.to_string m.counter)
  in
  let button =
    Azul.Button.create "Increase counter"
      ~button_type:Azul.ButtonType.Primary
      ~on_click:(Azul.RefAny.bind model m on_click)
  in
  Azul.Dom.body ~children:[ label; button ]

let () =
  let data = Azul.RefAny.upcast model { counter = 5 } in
  let window = Azul.WindowCreateOptions.create ~layout:(Azul.RefAny.lift model layout) () in
  let app_config = Azul.AppConfig.create () in
  let app = Azul.App.create ~data ~app_config () in

  Azul.App.run app window
