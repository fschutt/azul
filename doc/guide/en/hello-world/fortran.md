---
slug: hello-world/fortran
title: Hello World [Fortran]
language: en
canonical_slug: hello-world/fortran
audience: external
maturity: wip
guide_order: 26
topic_only: false
prerequisites: [hello-world]
tracked_files:
  - api.json
  - examples/fortran/hello_world.f90
  - doc/src/codegen/v2/lang_fortran/makefile.rs
last_generated_rev: dab922c5e869ab3c1ff69a2d7f4af1af19a5c27c
generated_at: 2026-07-04T00:00:00Z
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
`libazul` native library through `iso_c_binding`. Everything lives in a
single generated module, `azul.f90`, which has two layers:

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

Compiling `azul.f90` takes a few seconds and produces `azul.o` plus a
compiler-managed `azul.mod` that your program `use`s — both are cached
by `make`, so incremental rebuilds only recompile your own source.

## Simple "Counter" Example

This is the complete, verified `hello_world.f90` (the same file the
install step downloads):

```fortran
module hello_impl
  use azul
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
    call label%with_css('font-size: 32px;')

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
  use azul
  use hello_impl
  implicit none

  type(app_t) :: app
  type(window_create_options_t) :: window

  app = app_create(ref_any_create(t_model(5)), app_config_create())
  window = window_create_options_create(layout)
  call app%run(window)
end program hello_world
```

Six things to notice.

- **Callbacks are ordinary module functions.** `layout` and `on_click`
  take wrapper types and RETURN their result — no `bind(C)`, no
  `type(c_ptr)` dummies, no out-pointer to write. Each one matches a
  generated `abstract interface` (`layout_callback_iface`,
  `button_on_click_callback_iface`), so the compiler checks the shape
  for you. They must live in a MODULE rather than as internal
  procedures of the main program; an internal procedure would need a
  compiler-generated executable-stack trampoline that crashes on
  hardened systems.
- **The intents are part of the interface.** `data` and `info` are
  `intent(inout)` because a method that mutates the receiver needs it,
  and Fortran requires a procedure's dummy characteristics to match the
  abstract interface exactly. Copy the three declaration lines from the
  example; a mismatch is a compile error, not a runtime surprise.
- **Registration is implicit.** `call button%with_on_click(data, on_click)`
  and `window_create_options_create(layout)` take the procedure itself;
  the binding stores it in a handle table, hands libazul the id, and
  dispatches back through a generated invoker. There is no
  `host_invoker_init` to call and therefore none to forget — the first
  handle installs the releaser and every per-kind invoker.
- **`ref_any_create(t_model(5))` / `data%get()`** — the model
  round-trip. `ref_any_create` takes anything (`class(*)`) and COPIES
  it into the binding-owned handle table, so the model needs no
  `target, save` and cannot dangle; the table frees it when libazul
  drops the last clone of the `RefAny`. Inside a callback `data%get()`
  returns a `class(*), pointer` to that copy — `select type` recovers
  the concrete type, and writes through it persist.
- **Builders mutate in place.** A method that consumes `self` and
  returns `Self` (`with_css`, `with_child`, `with_button_type`) is
  emitted as a SUBROUTINE, so it reads `call label%with_css('...')`
  rather than `label = label%with_css('...')`. Methods that return
  something else stay functions: `button%dom()` hands the button's DOM
  back and marks the button consumed.
- **Strings are `character`.** Arguments are `character(len=*)` and
  results are `character(len=:), allocatable`; the binding does the
  `AzString` marshalling. Unit enums are plain `integer` constants with
  no `Az` prefix (`ButtonType_Primary`, `Update_RefreshDom`), and
  booleans are `logical`.

Nothing here needs `target`, `save`, `c_loc`, `c_f_pointer`, or a
`use, intrinsic :: iso_c_binding` line. If you do reach for the raw
`az_*` layer, `use azul` already re-exports those `iso_c_binding`
entities.

## Build and run

```sh
make
./hello_world                       # linux
DYLD_LIBRARY_PATH=. ./hello_world   # macOS
```

You should see the window pictured on the
[hello-world landing page](..md): the label renders "5",
and every click on the button increments it — the click callback bumps
`model%counter`, returns `Update_RefreshDom`, and the framework
re-runs `layout` with the new value.

To run the same headless counter scenario the CI uses:

```sh
AZ_E2E=path/to/hello_world_counter.json AZ_BACKEND=headless make run
```

## Common errors

- **Thousands of "Line truncated ... -Werror=line-truncation" errors
  compiling `azul.f90`** — you compiled by hand without
  `-ffree-line-length-none`. Use the shipped `Makefile`, or add the
  flag to your own build.
- **`make` tries to run `f77`** — an ancient GNU make builtin default.
  The shipped Makefile works around it; in your own Makefile set
  `FC = gfortran` explicitly (a plain `FC ?=` does *not* override the
  builtin).
- **"Interface mismatch in dummy procedure"** — your callback's
  declarations do not match the abstract interface. The dummy TYPES,
  the INTENTS and the result type all have to agree; copy them from the
  example or read the `abstract interface` block in `azul.f90`.
- **Counter renders but never updates** — the click callback returned
  something other than `Update_RefreshDom`. `Update_DoNothing` skips
  the re-layout.
- **Segfault inside a callback** — the callback is an internal
  procedure of the main program instead of a module procedure.
- **`type(dom_t) :: dom` errors on the type name** — you dropped the
  `_t`. Fortran folds case, so the wrapper types carry the suffix
  precisely so the obvious variable name stays free.
- **macOS: `dyld: Library not loaded: libazul.dylib`** — the Makefile's
  `$ORIGIN` rpath is Linux-only. Run with `DYLD_LIBRARY_PATH=.` or
  rewrite the install name with `install_name_tool`.
- **"Procedure ... is already defined" or garbled option/union values**
  — symptoms of a stale `azul.f90` from an older release. Re-download
  `azul.f90` and the `Makefile` from the same `$VERSION` as the
  library; since 0.2.0 tagged unions are ABI-exact opaque blobs and all
  factory names are unique.
- **Trying to read `AzOption*` / union fields directly** — not
  supported by design: Fortran has no unions, so these types are opaque
  byte blobs. Construct and inspect them through the C-API helper
  functions only.
