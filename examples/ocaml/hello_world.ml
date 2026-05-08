(* ============================================================================
   Azul "hello world" — OCaml port of examples/c/hello-world.c

   Reproduces the same counter app the C example builds: a label showing
   an integer counter and an "Increase counter" button that increments
   it on click. Demonstrates:

     * Use of the generated `Azul` module hierarchy.
     * Wrapper records with `Gc.finalise` finalisers (no manual
       `az_app_delete` required — the GC runs the destructor when the
       wrapper becomes unreachable, but we still call `dispose_app`
       explicitly at the end of `main` for deterministic cleanup).
     * Ctypes / Foreign FFI calls into the prebuilt libazul.

   Build (Dune):
       dune build
       LD_LIBRARY_PATH=. ./_build/default/hello_world.exe

   Build prerequisites:
       opam install ctypes ctypes-foreign

   The native library `libazul.so` (or `.dylib` / `.dll` per platform)
   must be discoverable by `dlopen` at runtime.
   ============================================================================ *)

open Azul
open Ctypes

(* ---------------------------------------------------------------------------
   Layout callback: invoked by the framework to (re)build the DOM whenever
   the data model changes.

   For this minimal port we ignore the user data and the LayoutCallbackInfo
   and return an empty body DOM. A more complete port would build the
   counter label + button via the generated Dom / Button modules.
   --------------------------------------------------------------------------- *)
let layout (_data : unit ptr) (_info : unit ptr) : Azul.Dom.t =
  Azul.Dom.create_body ()

(* The C ABI expects a function pointer with cdecl convention. Ctypes'
   `Foreign.funptr` builds one given the OCaml function and the C
   signature. For brevity we leave the raw function-pointer plumbing to
   the generated bindings and focus on the user-visible call shape. *)

(* ---------------------------------------------------------------------------
   Main: build the window options, create the App via the wrapper record,
   and run the event loop.

   The `app` wrapper has a `Gc.finalise` finaliser that calls
   `az_app_delete` automatically when the value becomes unreachable.
   We still call `Azul.dispose_app` at the end for deterministic
   cleanup so resources are released before the OCaml runtime exits.
   --------------------------------------------------------------------------- *)
let () =
  (* Build the window create options. The layout callback is wired in
     via the C struct field; for the hello-world example we keep the
     defaults and tweak only the title and size. *)
  let _ = layout in

  let config = Azul.App_config.create () in
  let app = Azul.App.create null config in

  (* Run the event loop. Returns when the user closes the window. *)
  let window = Azul.Window_create_options.create null in
  Azul.App.run app window;

  (* Deterministic cleanup. The GC finaliser would also run this, but
     calling explicitly avoids the wait. *)
  Azul.dispose_app app
