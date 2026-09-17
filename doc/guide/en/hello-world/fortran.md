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

The Fortran binding targets **Fortran 2003+** and talks to the prebuilt
`libazul` native library through `iso_c_binding`. It is generated as one
module per api.json module (`azul_types_*.f90`, `azul_ffi_*.f90`,
`azul_api.f90`, and a facade `azul_dom.f90`, `azul_css.f90`, ... per
module) behind the `azul` facade in `azul.f90`, so both `use azul` and
`use azul_dom, only: dom_t` work. Write `use azul, only: ...` with the
names you need: a bare `use azul` makes gfortran resolve every one of the
binding's procedures for your unit, which is minutes instead of seconds.
The binding has two layers:

- **the wrapper layer** — what you write against. One derived type per
  class (`dom_t`, `button_t`, `app_t`, `ref_any_t`, ...) with type-bound
  procedures (`call app%run(window)`), `character(len=*)` where the API
  wants a string, plain `integer` for unit enums (`ButtonType_Primary`,
  `Update_RefreshDom`), and ordinary Fortran procedures for callbacks.
  Factories are module procedures: `dom_create_body()`,
  `button_create('Increase counter')`.
- **`az_*`** — the raw `bind(C)` interfaces mirroring `azul.h`
  one-to-one, underneath. You need them only for something the wrapper
  layer does not cover: tagged unions (every `AzOption*` / `AzResult*`
  type) are ABI-opaque blobs because Fortran has no native `union`, so
  you construct and inspect those through the C-API helper functions.

`use azul` is the only `use` a program needs — the module re-exports the
`iso_c_binding` entities as well, so reaching down to the raw layer
costs no extra import.

The `_t` suffix on wrapper types is not decoration. Fortran folds case,
so a type named `Dom` makes `type(Dom) :: dom` — the natural variable
name — a redeclaration of the type itself. `type(dom_t) :: dom` is the
ordinary spelling.

Callbacks are ordinary module functions matching a generated abstract
interface. Registration is implicit: hand the procedure to the method
that takes it, and the binding stores it in a handle table, hands
libazul the id, and dispatches back through a generated `bind(C)`
invoker when the event fires. There is no init call to forget, no
`c_funloc`, and no out-pointer to write.

## Installation

You need **GFortran** (any recent version; on macOS `brew install gcc`
provides it, on Windows use the MinGW-w64 gfortran) and `make`. The
download set is: the native library, the generated `azul.f90` module,
the generated `Makefile`, and the counter example source.

```sh
curl -LO https://azul.rs/ui/release/$VERSION/azul-fortran-$VERSION.tar.gz
tar xzf azul-fortran-$VERSION.tar.gz      # azul.f90, Makefile, hello_world.f90

# linux
curl -O https://azul.rs/ui/release/$VERSION/libazul.so
make && ./hello_world
# macOS
curl -O https://azul.rs/ui/release/$VERSION/libazul.dylib
make && DYLD_LIBRARY_PATH=. ./hello_world
# windows (MSYS2 / MinGW-w64 shell, azul.dll next to the .exe)
curl -O https://azul.rs/ui/release/$VERSION/azul.dll
make && hello_world.exe
```

Use the shipped `Makefile` instead of invoking `gfortran` by hand: it
carries the **required** `-ffree-line-length-none` flag. The generated
`azul.f90` contains declaration lines beyond the F2008 132-column limit
(long widget/callback type names), and without the flag `-std=f2008`
turns each of them into a hard "Line truncated" error.

On macOS the `Makefile`'s embedded rpath (`$ORIGIN`) is an ELF
convention that the Mach-O loader ignores, so run the binary with
`DYLD_LIBRARY_PATH=.` as shown above (or fix the install name once with
`install_name_tool`).

Compiling the generated modules (`make -j8` compiles independent ones in
parallel; `sources.txt` lists the order if you drive the compiler by
hand) produces one `.o` plus a compiler-managed `.mod` each, which your
program `use`s — all cached by `make`, so incremental rebuilds only
recompile your own source.

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

Notice the required `--features build-dll`, as this is a flag to "build the DLL, don't link to it". The DLL lands at `target/release/libazul.{so,dylib}` (or `azul.dll`). The header/bindings are previously generated by `azul-doc codegen all` and end up at `target/codegen/`. Copy both somewhere your compiler can find them.

## Simple "Counter" Example

This is the complete, verified `hello_world.f90` (the same file the
install step downloads):

```fortran
module hello_impl
  use azul, only: ref_any_t, layout_callback_info_t, callback_info_t, dom_t, button_t, &
                  dom_create_p_with_text, dom_create_body, button_create, &
                  ButtonType_Primary, Update_RefreshDom
  implicit none

  type :: t_model
    integer :: counter = 5
  end type t_model

contains

  function layout(data, info) result(body)
    type(ref_any_t), intent(inout) :: data
    type(layout_callback_info_t), intent(inout) :: info
    type(dom_t) :: body
    class(*), pointer :: model
    type(dom_t) :: label
    type(button_t) :: button
    character(len=16) :: text

    model => data%get()
    select type (model)
    type is (t_model)
      write (text, '(I0)') model%counter
    class default
      text = '?'
    end select

    label = dom_create_p_with_text(trim(text))
    call label%with_css('font-size: 32px; margin: 0;')

    button = button_create('Increase counter')
    call button%with_button_type(ButtonType_Primary)
    call button%with_on_click(data, on_click)

    body = dom_create_body()
    call body%with_child(label)
    call body%with_child(button%dom())
  end function layout

  function on_click(data, info) result(update)
    type(ref_any_t), intent(inout) :: data
    type(callback_info_t), intent(inout) :: info
    integer :: update
    class(*), pointer :: model

    model => data%get()
    select type (model)
    type is (t_model)
      model%counter = model%counter + 1
    end select
    update = Update_RefreshDom
  end function on_click

end module hello_impl

program hello_world
  use azul, only: app_t, window_create_options_t, app_create, app_config_create, &
                  window_create_options_create, ref_any_create
  use hello_impl
  implicit none

  type(app_t) :: app
  type(window_create_options_t) :: window

  app = app_create(ref_any_create(t_model(5)), app_config_create())
  window = window_create_options_create(layout)
  call app%run(window)
end program hello_world
```

Notice that the callbacks are ordinary module functions: `layout` and `on_click` take wrapper types and *return* their result - no `bind(C)`, no `type(c_ptr)` dummies, no out-pointer to write - and each matches a generated `abstract interface` (`layout_callback_iface`, `button_on_click_callback_iface`), so the compiler checks the shape for you. They must live in a MODULE rather than as internal procedures of the main program, because an internal procedure would need a compiler-generated executable-stack trampoline that crashes on hardened systems. The intents are part of that interface: `data` and `info` are `intent(inout)`, and Fortran requires the dummy characteristics to match exactly, so copy the declaration lines from the example - a mismatch is a compile error rather than a runtime surprise. Registration is implicit, since `call button%with_on_click(data, on_click)` and `window_create_options_create(layout)` take the procedure itself, store it in a handle table and hand libazul an id that a generated invoker dispatches through; there is no `host_invoker_init` to call and therefore none to forget. `ref_any_create(t_model(5))` takes anything (`class(*)`) and *copies* it into the binding-owned handle table, so the model needs no `target, save` and cannot dangle (the table frees it when libazul drops the last clone), and inside a callback `data%get()` returns a `class(*), pointer` to that copy, which `select type` recovers and writes through persistently. Builders mutate in place: a method that would consume `self` and return `Self` (`with_css`, `with_child`, `with_button_type`) is emitted as a SUBROUTINE, so it reads `call label%with_css('...')`, while methods returning something else stay functions (`button%dom()` hands back the DOM and marks the button consumed). Strings are plain `character(len=*)` arguments and `character(len=:), allocatable` results with the binding doing the `AzString` marshalling, unit enums are plain `integer` constants without the `Az` prefix (`ButtonType_Primary`, `Update_RefreshDom`) and booleans are `logical` - nothing here needs `target`, `save`, `c_loc`, `c_f_pointer` or an explicit `use, intrinsic :: iso_c_binding` (which `use azul` already re-exports if you drop down to the raw `az_*` layer). Note also that the wrapper types carry a `_t` suffix (`type(dom_t)`), since Fortran folds case and the obvious variable name should stay free, and that compiling `azul.f90` by hand needs `-ffree-line-length-none` - the shipped `Makefile` sets it, along with an explicit `FC = gfortran` that defeats make's ancient `f77` builtin.

## Build and run

```sh
make
./hello_world                       # linux
DYLD_LIBRARY_PATH=. ./hello_world   # macOS
```

To run the same headless counter scenario the CI uses:

```sh
AZ_E2E=path/to/hello_world_counter.json AZ_BACKEND=headless make run
```

The Makefile's `$ORIGIN` rpath is Linux-only, so on macOS run with
`DYLD_LIBRARY_PATH=.` or rewrite the install name with `install_name_tool`.

You should see the window pictured on the [hello-world landing page](../hello-world.md). Click the button: the counter should increment, the layout callback then re-runs, and the new value renders.

1. `app%run(window)` opened a native window and ran the layout callback once with your data model.
2. The returned DOM was styled, laid out, and rendered.
3. The framework then continuously queries whether anything matches the event filter set up in the DOM. On click, the framework borrows your data model mutably, runs the click callback, observes the refresh return, and re-invokes the layout callback.
4. The framework determines the diff between the previous frame's DOM and the current one, and only re-updates and re-paints the counter, not the entire window.

Congratulations - once you've got the hello-world example running, you've already mastered 80% of the framework. As you might have guessed, more complex UI and styling are only composing more Dom objects together and working with the various event filters. To make this more streamlined, you can now start reading about the [architecture patterns](../architecture.md) or explore what [methods the `Dom` has to offer](../dom.md). See you in the next tutorial!
