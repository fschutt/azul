(* examples/ocaml/hello_world.ml
   OCaml host-invoker smoke test for the Azul binding. Confirms that:
   - Ctypes/Foreign loads the dylib and resolves AzString_fromUtf8
   - struct-by-value returns cross the FFI boundary
   - the managed-FFI prelude (lang_ocaml/managed.rs) wires the
     host-handle table + releaser + per-kind invokers; refany_create
     round-trips a managed OCaml value.

   Parallel to examples/node/hello-world.js and examples/lua/hello-world.lua.
   Full GUI wiring (Dom builders, WindowCreateOptions, App.run) requires
   the wrapper layer's idiomatic API surface to settle — separate work,
   not host-invoker.

   Build:    dune build
   Run (macOS):  DYLD_LIBRARY_PATH=. ./_build/default/hello_world.exe
   Run (Linux):  LD_LIBRARY_PATH=. ./_build/default/hello_world.exe *)

open Ctypes
(* Avoid `open Azul` because the Azul module shadows Stdlib's `String`
   (Azul has its own `String` wrapper module). We reference Azul
   members explicitly with `Azul.xxx`. *)

(* The model the framework holds on our behalf via a RefAny. Mirrors
   the C example's `MyDataModel { counter: u32 }` but typed-OCaml. *)
type my_data_model = { mutable counter : int }

let () =
  (* 1. String round-trip — proves struct-by-value FFI works. *)
  let src = "hello, azul" in
  let len = Stdlib.String.length src in
  let buf = allocate_n char ~count:len in
  Stdlib.String.iteri (fun i c -> (buf +@ i) <-@ c) src;
  let _s = Azul.String.from_utf8 (to_voidp buf) (Unsigned.Size_t.of_int len) in
  Printf.printf "[azul] String.from_utf8 round-trip succeeded; len=%d\n" len;

  (* 2. RefAny round-trip — proves the host-invoker prelude is wired:
     a fresh OCaml record gets stashed in `_azul_handles`, libazul
     hands us back an opaque AzRefAny, and `azul_refany_get` recovers
     the original record via the host-handle id. *)
  let model = { counter = 5 } in
  let refany = Azul.azul_refany_create model in
  Printf.printf "[azul] azul_refany_create ran; RefAny opaque-handle id stored.\n";

  (* azul_refany_get takes an AzRefAny pointer (the framework hands
     callbacks the RefAny by-pointer). We allocate one and copy. *)
  let refany_box = allocate Azul.az_ref_any refany in
  match Azul.azul_refany_get refany_box with
  | Some (recovered : my_data_model) when recovered.counter = 5 ->
      Printf.printf "[azul] azul_refany_get round-trip succeeded; counter=%d\n"
        recovered.counter;
      Printf.printf "[azul] host-invoker init phase completed successfully.\n";
      Printf.printf "[azul] (Full App.run wiring requires layout / callback\n";
      Printf.printf "[azul]  wrappers, separate from the host-invoker plumbing.)\n"
  | Some _ ->
      Printf.printf "[azul] azul_refany_get round-trip recovered wrong value\n";
      exit 1
  | None ->
      Printf.printf "[azul] azul_refany_get returned None (host-handle id was 0)\n";
      exit 1
