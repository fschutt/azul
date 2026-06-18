(* Run: dune build && LD_LIBRARY_PATH=. ./_build/default/hello_world.exe *)

type my_data_model = { mutable counter : int }
let model = { counter = 5 }

let on_click (data_ptr : unit Ctypes.ptr) (_info : unit Ctypes.ptr) : int =
  let ref_ptr = Ctypes.from_voidp Azul.az_ref_any data_ptr in
  match Azul.azul_refany_get ref_ptr with
  | None -> 0 (* Update.DoNothing *)
  | Some (m : my_data_model) ->
      m.counter <- m.counter + 1;
      1 (* Update.RefreshDom *)

let layout (data_ptr : unit Ctypes.ptr) (_info : unit Ctypes.ptr)
  : Azul.az_dom Ctypes.structure =
  let ref_ptr = Ctypes.from_voidp Azul.az_ref_any data_ptr in
  match Azul.azul_refany_get ref_ptr with
  | None -> Azul.raw_dom (Azul.Dom.create_body ())
  | Some (m : my_data_model) ->
      let click_cb = Azul.azul_register_callback on_click in
      let click_data = Azul.azul_refany_create m in

      let with_css css d        = Azul.Dom.with_css d css in
      let with_child child d    = Azul.Dom.with_child d child in
      let as_btn_type t b       = Azul.Button.with_button_type b t in
      let on_click_ data cb b   = Azul.Button.with_on_click b data cb in

      let label_div =
        Azul.Dom.create_div ()
        |> with_css "font-size: 32px;"
        |> with_child (Azul.raw_dom (Azul.Dom.create_text (string_of_int m.counter)))
      in
      let button_dom =
        Azul.Button.create "Increase counter"
        |> as_btn_type 1 (* ButtonType.Primary *)
        |> on_click_ click_data click_cb
        |> Azul.Button.dom
      in
      Azul.Dom.create_body ()
      |> with_child (Azul.raw_dom label_div)
      |> with_child button_dom
      |> Azul.raw_dom

let () =
  let data = Azul.azul_refany_create model in
  let wco = Azul.azul_window_create_options_with_layout layout in
  let app_config = Azul.AppConfig.create () in
  let app = Azul.App.create data (Azul.raw_app_config app_config) in
  (* consume: the raw bytes are moved into libazul by App.run; without this
     the GC finaliser would later call _delete on moved memory (SIGABRT). *)
  Azul.azul_consume app_config;
  Azul.App.run app wco
