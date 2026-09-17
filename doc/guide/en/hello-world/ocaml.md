---
slug: hello-world/ocaml
title: Hello World [OCaml]
language: en
canonical_slug: hello-world/ocaml
audience: external
maturity: mature
guide_order: 21
topic_only: false
prerequisites: [hello-world]
tracked_files:
  - api.json
  - examples/ocaml/hello_world.ml
last_generated_rev: 2660b0c45c9ea401ad6777a203f468755167e62e
generated_at: 2026-09-16T00:00:00Z
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

To use `libazul` from OCaml (4.14+), you need both the native prebuilt library as well as the
pre-rendered OCaml bindings, which use [`ctypes-foreign`](https://github.com/yallop/ocaml-ctypes) 
under the hood. The bindings abstract away the need to interface with the C types directly.

## Installation

There is no opam package yet. You will need to build the native library from source and generate the OCaml bindings yourself using the `azul-doc` tool.

First, install the required OCaml dependencies:

```sh
opam install ctypes ctypes-foreign dune
```

Then, clone the Azul repository, generate the bindings, and compile the native library:

```sh
git clone https://github.com/fschutt/azul
cd azul
# Generate the OCaml bindings from api.json (ends up in target/codegen/ocaml)
cargo run -p azul-doc --release -- codegen all
# Build the native library (lands in target/release/libazul.so or libazul.dylib)
cargo build -p azul-dll --release --features build-dll
```

Create a new directory for your app and copy the generated OCaml files and the native library into it:

```sh
mkdir -p my_app && cd my_app
cp ../target/codegen/ocaml/* .
cp ../target/release/libazul.* .
```

Now, create a file named `hello_world.ml` with the following code. Note that the copied `dune` file already contains the commented-out executable block for `hello_world`, so you can just uncomment it!

## Simple "Counter" Example

```ocaml
type my_data_model = { mutable counter : int }

let layout (m : my_data_model) : Azul.Dom.t =
  let label =
    Azul.Dom.p ~css:"font-size: 32px; margin: 0;" (Int.to_string m.counter)
  in
  let button =
    Azul.Button.create "Increase counter"
      ~button_type:`Primary
      ~on_click:(fun () ->
          m.counter <- m.counter + 1;
          `RefreshDom)
  in
  Azul.Dom.body ~children:[ label; button ]

let () =
  let model = { counter = 5 } in
  let window = Azul.WindowCreateOptions.create ~layout () in
  let app_config = Azul.AppConfig.create () in
  let app = Azul.App.create ~model ~app_config () in

  Azul.App.run app window
```

## Build and run

```sh
dune exec ./hello_world.exe
# or, after `dune build`:
#   macOS:  DYLD_LIBRARY_PATH=. ./_build/default/hello_world.exe
#   linux:  LD_LIBRARY_PATH=. ./_build/default/hello_world.exe
```

You should see the window pictured on the [hello-world landing page](../hello-world.md). 
Click the button: the counter should increment, the layout callback then re-runs, and the new value renders.

1. `Azul.App.run` opens a native window and runs the layout callback once with your model.
2. The returned DOM is styled, laid out, and rendered.
3. The framework then continuously queries whether anything matches the event filter set 
   up in the DOM. On click, the framework borrows your data model mutably, runs the click 
   callback, observes the refresh return, and re-invokes the layout callback.
4. The framework determines the diff between the previous frame's DOM and the current one, 
   and only re-updates and re-paints the counter, not the entire window.

Congratulations! Once you've got the hello-world example running, you've already mastered 80% 
of the framework. As you might have guessed, more complex UI and styling are only composing 
more Dom objects together and working with the various event filters. 

You can now start reading about the [architecture patterns](../architecture.md) or 
explore what [methods the `Dom` has to offer](../dom.md). 

See you in the next tutorial!
