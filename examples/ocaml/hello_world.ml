(* examples/ocaml/hello_world.ml
   Minimal OCaml smoke test for the Azul C ABI. Confirms:
   - Ctypes/Foreign loads the dylib
   - struct-by-value calls round-trip across the FFI boundary
   - the basic AzString constructors work

   OCaml's Ctypes-Foreign is libffi-backed and could in principle do
   host-invoker plumbing (`lang_ocaml/managed.rs`) the way Lua/Ruby
   do, but the FFI smoke test here just exercises the raw C ABI —
   the higher-level RefAny.wrap / callback registration story is a
   separate piece of work (same boundary the Go/Zig hello-worlds
   stop at).

   Build:    dune build
   Run:      LD_LIBRARY_PATH=. ./_build/default/hello_world.exe   (Linux)
             DYLD_LIBRARY_PATH=. ./_build/default/hello_world.exe (macOS) *)

open Ctypes
(* Avoid `open Azul` because the Azul module shadows Stdlib's `String`
   (Azul has its own `String` wrapper module). We reference Azul
   members explicitly with `Azul.xxx`. *)

let () =
  (* Build a non-empty AzString from an OCaml string. Exercises
     struct-by-value return across the FFI boundary. *)
  let src = "hello, azul" in
  let len = Stdlib.String.length src in
  let buf = allocate_n char ~count:len in
  Stdlib.String.iteri (fun i c -> (buf +@ i) <-@ c) src;
  let s = Azul.az_string_from_utf8 (to_voidp buf) (Unsigned.Size_t.of_int len) in

  (* Clone to confirm the dylib's heap allocator is wired up. *)
  let s_ptr = allocate Azul.az_string s in
  let _clone = Azul.az_string_clone s_ptr in
  Printf.printf "[azul] AzString round-trip succeeded; len=%d\n" len;

  Printf.printf "[azul] Ctypes init phase completed successfully.\n";
  Printf.printf "[azul] (Full App.run wiring requires wrapper-layer work\n";
  Printf.printf "[azul]  separate from the C ABI plumbing exercised here.)\n"
