(* examples/ocaml/hello_world.ml

   OCaml port of examples/c/hello-world.c. Same data model (a counter),
   same behaviour (mouse click increments, layout rebuilds the DOM).
   Callbacks go through libazul's host-invoker plumbing — Ctypes/Foreign
   never has to synthesize a struct-by-value trampoline for user code.

   Build:    dune build
   Run (macOS):  DYLD_LIBRARY_PATH=. ./_build/default/hello_world.exe
   Run (Linux):  LD_LIBRARY_PATH=. ./_build/default/hello_world.exe *)

(* Avoid `open Azul`: the Azul module shadows Stdlib.String with its own
   `String` wrapper module. Reference Azul members explicitly. *)

(* ── Data model ─────────────────────────────────────────────────────── *)
type my_data_model = { mutable counter : int }
let model = { counter = 5 }

(* ── Callbacks ──────────────────────────────────────────────────────── *)
(* User functions return the *raw* `az_dom Ctypes.structure` because
   the codegen's invoker writes the bytes through an out-pointer. Plain
   OCaml strings flow into Dom.* / Button.* via the codegen-emitted
   auto-string-conversion (`azul_az_string` helper). *)

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

      (* CC-6: `with_*` functions take the receiver as their first arg
         and consume it. To compose them with OCaml's `|>` pipeline we
         flip the arg order at the call site with local helpers — same
         semantics, but the value flows left-to-right top-down. *)
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

(* ── Main ───────────────────────────────────────────────────────────── *)
(* We skip `WindowCreateOptions.create(layout)` because that routes
   through `AzWindowCreateOptions_create(AzLayoutCallbackType)`, which
   takes a raw fn pointer and discards the host-invoker ctx carrying
   our dispatch handle. Mutate the nested layout_callback field
   directly via Ctypes' getf+setf+setf copy-out-set-back idiom. *)

let () =
  let data = Azul.azul_refany_create model in
  let wco = Azul.azul_window_create_options_with_layout layout in
  let app_config = Azul.AppConfig.create () in
  let app = Azul.App.create data (Azul.raw_app_config app_config) in
  (* Mark wrappers consumed: their raw struct bytes are about to be
     moved into libazul by App.run. Without `azul_consume`, OCaml's
     Gc.finalise would later call `<X>_delete` on the now-moved
     memory — manifests as the SIGABRT in `<U8Vec as Drop>::drop`
     from MacOSWindow::new_with_options_internal. Same pattern as
     Node's `_consume` and Ruby's `Azul._consume`. *)
  Azul.azul_consume app_config;
  Azul.App.run app wco
