---
slug: hello-world/fortran
title: Hello World [Fortran]
language: en
canonical_slug: hello-world/fortran
audience: external
maturity: mature
guide_order: 26
topic_only: false
prerequisites: [hello-world]
tracked_files:
  - api.json
  - examples/fortran/hello_world.f90
  - doc/src/codegen/v2/lang_fortran/makefile.rs
last_generated_rev: 2660b0c45c9ea401ad6777a203f468755167e62e
generated_at: 2026-09-16T00:00:00Z
default-search-keys:
  - App
  - AppConfig
  - Dom
  - Button
  - RefAny
  - WindowCreateOptions
  - Update
---

# Hello World [Fortran]

## Introduction

To use `libazul` from Fortran, you need the prebuilt native library and the generated
Fortran binding: a set of modules that talk to the library through `iso_c_binding`,
behind one `azul` module. The binding is tested with gfortran and needs a Fortran 2003
compiler.

Every API class is a derived type with type-bound procedures (`dom_t`, `button_t`,
`app_t`), strings are plain `character`, enum values are integer constants
(`ButtonType_Primary`, `Update_RefreshDom`) and callbacks are ordinary module functions.
Your data model is any type you define: the binding keeps it and hands it back to each
callback as `class(*)`, and `select type` turns it back into your type.

## Installation

You need gfortran and GNU make (macOS: `brew install gcc`, Debian / Ubuntu:
`sudo apt install gfortran make`, Windows: the MinGW-w64 gfortran from MSYS2).

The release bundle `azul-fortran-$VERSION.tar.gz` contains the generated modules,
a `Makefile` and the counter example. Unpack it next to the native library:

```sh
mkdir hello-world && cd hello-world
curl -LO https://azul.rs/ui/release/$VERSION/azul-fortran-$VERSION.tar.gz
tar xzf azul-fortran-$VERSION.tar.gz

# macOS
curl -O https://azul.rs/ui/release/$VERSION/libazul.dylib
# Linux
curl -O https://azul.rs/ui/release/$VERSION/libazul.so
# Windows
curl -O https://azul.rs/ui/release/$VERSION/azul.dll

make -j8
./hello_world
```

The first `make` compiles the binding, which takes about two minutes with `-j8`. After
that, `make` only recompiles your own source. The executable looks for the library in its
own directory, so you can run it from anywhere. If the library lives somewhere else, for
example installed with Homebrew, pass its directory: `make LIBDIR="$(brew --prefix)/lib"`.

### Building from source

Only needed if you want to track `master` or patch the library locally:

```sh
# git clone https://github.com/fschutt/azul
# cd myfolder/azul
# generate the bindings from api.json (required)
cargo run -p azul-doc --release -- codegen all
# build the actual DLL with the now-generated .rs C-API bindings
cargo build -p azul-dll --release --features build-dll
```

Notice the required `--features build-dll`. The DLL lands in
`target/release/libazul.{so,dylib}` (or `azul.dll`). The Fortran binding is
generated into `target/codegen/fortran/`, together with its `Makefile`. Copy both
next to your program.

## Simple "Counter" Example

```fortran
module counter
  use azul, only: dom_t, button_t, layout_callback_info_t, callback_info_t, &
                  dom_create_body, dom_create_p_with_text, button_create, &
                  ButtonType_Primary, Update_DoNothing, Update_RefreshDom
  implicit none

  type :: model_t
    integer :: counter
  end type model_t

contains

  function layout(model, info) result(body)
    class(*), intent(inout) :: model
    type(layout_callback_info_t), intent(inout) :: info
    type(dom_t) :: body, label
    type(button_t) :: button
    character(len=16) :: text

    body = dom_create_body()
    select type (model)
    type is (model_t)
      write (text, '(I0)') model%counter
      label = dom_create_p_with_text(trim(text))
      call label%with_css('font-size: 32px; margin: 0;')

      button = button_create('Increase counter')
      call button%with_button_type(ButtonType_Primary)
      call button%with_on_click(model, on_click)

      call body%with_child(label)
      call body%with_child(button%dom())
    class default
      error stop 'layout: the model is not a model_t'
    end select
  end function layout

  function on_click(model, info) result(update)
    class(*), intent(inout) :: model
    type(callback_info_t), intent(inout) :: info
    integer :: update

    update = Update_DoNothing
    select type (model)
    type is (model_t)
      model%counter = model%counter + 1
      update = Update_RefreshDom
    class default
      error stop 'on_click: the model is not a model_t'
    end select
  end function on_click

end module counter

program hello_world
  use azul, only: app_t, app_create, app_config_create, window_create_options_create
  use counter, only: model_t, layout
  implicit none

  type(app_t) :: app

  app = app_create(model_t(counter=5), app_config_create())
  call app%run(window_create_options_create(layout))
end program hello_world
```

There are a few Fortran-specific things in this example:

1. `use azul, only: ...` imports only the names you use. A bare `use azul` also works,
   but then gfortran loads the whole binding for your file, which takes tens of seconds
   instead of one.
2. `app_create(model_t(counter=5), ...)` copies your model into the binding. Each
   callback receives that copy as `class(*), intent(inout) :: model`, and changes made
   inside `type is (model_t)` are kept: `class(*)` is "any type", the model as
   libazul holds it, and `select type` is the checked downcast back to yours. A model
   of any other type is a programming error, so `class default` stops the program
   with a message instead of silently rendering an empty body or ignoring the click.
3. `call button%with_on_click(model, on_click)` binds the model the layout callback is
   running with, not a copy, so the click changes the same counter.
4. `layout` and `on_click` are ordinary module functions. Their dummy arguments must
   match the binding's interfaces exactly (`class(*), intent(inout)` for the model,
   `intent(inout)` for `info`); a mismatch is a compile error.
5. Methods like `with_css` or `with_child` change the object in place and are called
   with `call`. `button%dom()` turns the button into a `dom_t`. An argument such a
   method takes by value moves into the result: after `call body%with_child(label)`,
   `label`'s value belongs to `body`, so don't pass `label` on again or `%delete()` it.
   Values nothing took are yours to `%delete()`; a second `%delete()` of the same
   variable does nothing.
6. A callback that returns an invalid result, such as an `integer` that is not an
   `Update` value or a `dom_t` that was never assigned, does not reach the engine.
   The binding logs the problem and uses the default result instead:

   ```
   [azul][error] azul: ButtonOnClickCallback expected an Update (0 to 2), got 7
   ```

## Build and run

```sh
make
./hello_world
```

You should see the window pictured on the [hello-world landing page](../hello-world.md).
Click the button: the counter should increment, the layout callback then re-runs, and the
new value renders.

1. `app%run(...)` opens a native window and runs the layout callback once with your model.
2. The returned `dom_t` is styled, laid out, and rendered.
3. The framework then continuously queries whether anything matches the event filter set up
   in the DOM. On click, the framework borrows your data model mutably, runs the click callback,
   observes the `Update_RefreshDom` return, and re-invokes the layout callback.
4. The framework determines the diff between the previous frame's DOM and the current one,
   and only re-updates and re-paints the counter, not the entire window.

Congratulations! Once you've got the hello-world example running, you've already mastered 80%
of the framework. As you might have guessed, more complex UI and styling are only composing
more `dom_t` objects together and working with the various event filters.

You can now start reading about the [architecture patterns](../architecture.md) or
explore what [methods the `Dom` has to offer](../dom.md).

See you in the next tutorial!
