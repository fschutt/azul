(* OCaml: [None] where an Option is asked for, an [option] back from fields
   and downcasts (read with [let*]), a [result] for a Result, and
   [Update.or_do_nothing] ending a callback on a missing value. Built and run
   by scripts/native_optionals_smoke.sh. *)

type my_model = { mutable folder : string; mutable link : string }

let model : my_model Azul.RefAny.key = Azul.RefAny.key "my_model"

let ( let* ) = Option.bind

(* The guide's file-dialog example, as written there (hello-world/ocaml.md). *)
let on_pick_folder (m : my_model) _info result =
  Azul.Update.or_do_nothing (fun () ->
      let* picked = Azul.FileOpenResult.downcast result in
      let* path = Azul.FileOpenResult.get_path picked in
      m.folder <- Azul.FilePath.get_inner path;
      Some Azul.Update.RefreshDom)

let on_choose_folder (m : my_model) _info =
  let _ =
    Azul.FileDialog.open_directory "Choose a folder" None
      (Azul.RefAny.upcast model m)
      (Azul.RefAny.lift model on_pick_folder)
  in
  Azul.Update.DoNothing

let on_open_link (m : my_model) _info =
  Azul.Update.or_do_nothing_on_error (fun () ->
      Result.map (fun _url -> Azul.Update.RefreshDom) (Azul.Url.parse m.link))

let check ok what =
  if not ok then begin
    prerr_endline ("FAIL: " ^ what);
    exit 1
  end

let () =
  (* A field: None in, None out; Some in, Some out. *)
  let icons = Azul.IconVariants.create_default () in
  check (Azul.IconVariants.get_light icons = None) "a default IconVariants has no light icon";
  Azul.IconVariants.set_light icons (Some "light.png");
  check (Azul.IconVariants.get_light icons = Some "light.png") "Some in, Some out";
  Azul.IconVariants.set_light icons None;
  check (Azul.IconVariants.get_light icons = None) "None in, None out";
  (* A result: Ok and Error. *)
  check (Result.is_ok (Azul.Url.parse "https://azul.rs/guide")) "Url.parse Ok";
  check (Result.is_error (Azul.Url.parse "no scheme")) "Url.parse Error";
  (* The guards on their own. *)
  check (Azul.Update.or_do_nothing (fun () -> None) = Azul.Update.DoNothing) "or_do_nothing";
  check
    (Azul.Update.or_do_nothing_on_error (fun () -> Error "no") = Azul.Update.DoNothing)
    "or_do_nothing_on_error";
  ignore (on_choose_folder, on_open_link);
  print_endline "ocaml native optionals: ok"
