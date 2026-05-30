---
slug: hello-world/ocaml
title: Hello World [OCaml]
language: en
canonical_slug: hello-world/ocaml
audience: external
maturity: wip
guide_order: 21
topic_only: false
prerequisites: [hello-world]
tracked_files:
  - api.json
  - examples/ocaml/hello_world.ml
last_generated_rev: 39416ebc681c6423bfdefa94dc996f613184ea0b
generated_at: 2026-05-29T00:00:00Z
default-search-keys:
  - App
  - AppConfig
  - Dom
  - Button
  - WindowCreateOptions
  - Update
---

# Hello World [OCaml]

## Introduction

The OCaml binding uses [`ctypes-foreign`](https://github.com/yallop/ocaml-ctypes) to
call the prebuilt `libazul` native library. Callbacks go through libazul's
host-invoker plumbing, so Ctypes never has to synthesize a struct-by-value
trampoline. OCaml is the most explicit of the bindings — user functions return the
*raw* `az_dom Ctypes.structure` and the codegen's invoker writes the bytes through an
out-pointer.

## Installation

You need **OCaml 4.14+** with **dune**, the **`ctypes`** + **`ctypes-foreign`**
packages, and the native `libazul` library.

### Recommended: opam

```sh
opam repo add azul https://azul.rs/opam
opam install azul
```

### Manual

```sh
opam install ctypes ctypes-foreign
# download the native library from /releases into the project dir:
wget -O libazul.dylib https://azul.rs/release/0.2.0/libazul.dylib   # macOS
```

Add the generated `azul.ml` / `azul.mli` (from the
[examples archive](/release/0.2.0/examples.zip) under `ocaml/`) to your dune project.

## Simple "Counter" Example

```ocaml
(* Avoid `open Azul`: the Azul module shadows Stdlib.String with its own
   String wrapper. Reference Azul members explicitly. *)

(* Data model. *)
type my_data_model = { mutable counter : int }
let model = { counter = 5 }

(* Click callback: returns an int Update code. *)
let on_click (data_ptr : unit Ctypes.ptr) (_info : unit Ctypes.ptr) : int =
  let ref_ptr = Ctypes.from_voidp Azul.az_ref_any data_ptr in
  match Azul.azul_refany_get ref_ptr with
  | None -> 0 (* Update.DoNothing *)
  | Some (m : my_data_model) ->
      m.counter <- m.counter + 1;
      1 (* Update.RefreshDom *)

(* Layout callback: returns the raw az_dom structure; the invoker writes the
   bytes through the out-pointer. *)
let layout (data_ptr : unit Ctypes.ptr) (_info : unit Ctypes.ptr)
  : Azul.az_dom Ctypes.structure =
  let ref_ptr = Ctypes.from_voidp Azul.az_ref_any data_ptr in
  match Azul.azul_refany_get ref_ptr with
  | None -> Azul.raw_dom (Azul.Dom.create_body ())
  | Some (m : my_data_model) ->
      let click_cb   = Azul.azul_register_callback on_click in
      let click_data = Azul.azul_refany_create m in

      (* with_* consume the receiver as their first arg; flip the arg order
         with local helpers so values flow left-to-right under |>. *)
      let with_css css d      = Azul.Dom.with_css d css in
      let with_child child d  = Azul.Dom.with_child d child in
      let as_btn_type t b     = Azul.Button.with_button_type b t in
      let on_click_ data cb b = Azul.Button.with_on_click b data cb in

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
  let data       = Azul.azul_refany_create model in
  let wco        = Azul.azul_window_create_options_with_layout layout in
  let app_config = Azul.AppConfig.create () in
  let app        = Azul.App.create data (Azul.raw_app_config app_config) in
  (* Mark wrappers consumed: their bytes are about to be moved into libazul by
     App.run. Without azul_consume, Gc.finalise would later call <X>_delete on
     moved memory. Same pattern as Node's _consume and Ruby's Azul._consume. *)
  Azul.azul_consume app_config;
  Azul.App.run app wco
```

Four things to notice.

- **`Azul.azul_refany_create` / `azul_refany_get`** — wrap/recover your value through a
  handle. The getter returns `option`, so `match ... | None -> ... | Some m -> ...`.
- **Callbacks return raw structures.** `layout` returns `az_dom Ctypes.structure`
  (`Azul.raw_dom` extracts it from a wrapper); the invoker writes the bytes out for
  you. Update codes are plain ints (`0` = DoNothing, `1` = RefreshDom).
- **`with_*` consume the receiver.** They take the receiver as the *first* argument,
  so to use `|>` you flip the arg order with small local helpers — the value then
  flows top-down.
- **Call `azul_consume`** on wrappers whose bytes are moved into libazul (e.g. the
  `AppConfig` passed to `App.create`), or OCaml's GC finalizer will later
  double-free the moved memory.

## Build and run

```sh
dune exec ./hello_world.exe
# or, after `dune build`:
#   macOS:  DYLD_LIBRARY_PATH=. ./_build/default/hello_world.exe
#   linux:  LD_LIBRARY_PATH=. ./_build/default/hello_world.exe
```

You should see the window pictured on the [hello-world landing page](../hello-world.md).
Click the button: the counter increments and the layout callback re-runs.

## Common errors

- **`Dl.dlopen` / library not found** — the native library isn't on
  `DYLD_LIBRARY_PATH` / `LD_LIBRARY_PATH`, or not in the project directory.
- **`Unbound module Azul`** — `azul.ml` / `azul.mli` aren't listed in your dune
  `modules` / not in the project.
- **SIGABRT in `<U8Vec as Drop>::drop` on exit** — you forgot `azul_consume` on a
  wrapper whose bytes were moved into libazul; the GC finalizer double-freed it.
- **Counter does not advance** — `on_click` returned `0` (DoNothing) instead of `1`.

## Coming Up Next

- [Application Architecture](../architecture.md) — architecting a larger Azul application
- [Document Object Model](../dom.md) — the Dom tree: node types, hierarchy, and CSS
- [Hello World [Rust]](rust.md)
