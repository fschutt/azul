(* examples/ocaml/hello_world.ml

   OCaml port of examples/c/hello-world.c. Same data model (a counter),
   same behaviour (mouse click increments, layout rebuilds the DOM).
   Callbacks go through libazul's host-invoker plumbing — Ctypes/Foreign
   never has to synthesize a struct-by-value trampoline for user code.

   Build:    dune build
   Run (macOS):  DYLD_LIBRARY_PATH=. ./_build/default/hello_world.exe
   Run (Linux):  LD_LIBRARY_PATH=. ./_build/default/hello_world.exe *)

open Ctypes
(* Avoid `open Azul`: the Azul module shadows Stdlib.String with its own
   `String` wrapper module. Reference Azul members explicitly. *)

(* ── Data model ─────────────────────────────────────────────────────── *)
type my_data_model = { mutable counter : int }
let model = { counter = 5 }

(* ── Helpers ────────────────────────────────────────────────────────── *)

(* Copy an OCaml string into a fresh `az_string structure` via the
   public `String.from_utf8` wrapper. Extract the raw struct so we
   can pass it by-value to FFI-shaped functions (Dom.create_text,
   Button.create, etc.) which take raw `az_string`. *)
let az_str (s : string) : Azul.az_string Ctypes.structure =
  let len = Stdlib.String.length s in
  let buf = allocate_n char ~count:len in
  Stdlib.String.iteri (fun i c -> (buf +@ i) <-@ c) s;
  Azul.raw_string_wrapper
    (Azul.String.from_utf8 (to_voidp buf) (Unsigned.Size_t.of_int len))

(* ── Callbacks ──────────────────────────────────────────────────────── *)
(* User functions return the *raw* `az_dom Ctypes.structure` because
   the codegen's invoker writes the bytes through an out-pointer. We
   extract `.raw` from each `dom` wrapper at the return site. *)

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
      (* Each callback registration produces a fresh handle into the
         libazul handle table — we can reuse `model` (a real OCaml
         value living module-globally) without cloning the AzRefAny. *)
      let click_cb = Azul.azul_register_callback on_click in
      let click_data = Azul.azul_refany_create m in

      let counter_dom = Azul.Dom.create_text (az_str (string_of_int m.counter)) in
      let label_div =
        Azul.Dom.with_css
          (Azul.Dom.create_div ())
          (az_str "font-size: 32px;")
      in
      let label_div =
        Azul.Dom.with_child label_div (Azul.raw_dom counter_dom)
      in

      let button = Azul.Button.create (az_str "Increase counter") in
      let button = Azul.Button.with_button_type button 1 (* ButtonType.Primary *) in
      let button = Azul.Button.with_on_click button click_data click_cb in
      let button_dom = Azul.Button.dom button in

      let body = Azul.Dom.create_body () in
      let body = Azul.Dom.with_child body (Azul.raw_dom label_div) in
      let body = Azul.Dom.with_child body button_dom in
      Azul.raw_dom body

(* ── Main ───────────────────────────────────────────────────────────── *)
(* We skip `WindowCreateOptions.create(layout)` because that routes
   through `AzWindowCreateOptions_create(AzLayoutCallbackType)`, which
   takes a raw fn pointer and discards the host-invoker ctx carrying
   our dispatch handle. Mutate the nested layout_callback field
   directly via Ctypes' getf+setf+setf copy-out-set-back idiom. *)

let () =
  let data = Azul.azul_refany_create model in
  (* `azul_window_create_options_with_layout` is the codegen-emitted
     smart constructor that builds a default WCO + stuffs the
     host-invoker-registered AzLayoutCallback (ctx preserved) into
     window_state.layout_callback. *)
  let wco = Azul.azul_window_create_options_with_layout layout in
  let app_config = Azul.AppConfig.create () in
  let app = Azul.App.create data (Azul.raw_app_config app_config) in
  Azul.App.run app wco
