(* Memory test for the azul OCaml (ctypes) binding. See tests/memtest/README.md.

   The harness (scripts/run_memtest.sh) measures peak RSS across a small and a
   large AZ_MEMTEST_N (RSS that scales with N is a LEAK) and fails on any crash.
   This file only exercises the create/consume/DROP paths in a loop and exits 0.
   No event loop (App.run needs a display and hangs headless). *)

type my_data_model = { mutable counter : int }
let model = { counter = 5 }

let () =
  let n =
    match Sys.getenv_opt "AZ_MEMTEST_N" with
    | Some s -> (try int_of_string (String.trim s) with _ -> 200000)
    | None -> 200000
  in

  (* 1. The consume-by-value DROP path: App.create moves the AppConfig bytes
        (nested SystemStyle) into libazul. azul_consume marks the OCaml wrapper
        consumed so its finaliser won't double-free the moved memory; then
        dispose_app calls AzApp_delete once. *)
  let data = Azul.azul_refany_create model in
  let cfg = Azul.AppConfig.create () in
  let app = Azul.App.create data (Azul.raw_app_config cfg) in
  Azul.azul_consume cfg;
  Azul.dispose_app app;

  (* 2. Leak loop: create/destroy a droppable AppConfig N times.
        dispose_app_config calls AzAppConfig_delete (dropping the nested
        SystemStyle) deterministically each iteration. *)
  for _ = 1 to n do
    let c = Azul.AppConfig.create () in
    Azul.dispose_app_config c
  done;

  Printf.printf "memtest ocaml OK (N=%d)\n" n
