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

- **`az_*`** — raw `bind(C)` interfaces mirroring `azul.h` one-to-one
  (`az_dom_create_body()`, `az_button_create(...)`, ...). Structs are
  passed by value with the exact C size and alignment; tagged unions
  (every `AzOption*` / `AzResult*` / union type) are emitted as
  ABI-opaque blobs, because Fortran has no native `union` — you
  construct and inspect them exclusively through the C-API helper
  functions, never through field access.
- **`azul_*`** — a small host-invoker convenience layer on top:
  `azul_register_<kind>()` turns a `bind(C)` module procedure into an
  `Az<Kind>Callback` value, and `azul_refany_create()` /
  `azul_refany_get()` wrap and recover your data model pointer.

Callbacks dispatch through libazul's host-invoker plumbing: each
registered procedure gets a handle id, and when the framework fires the
callback it calls back into a per-kind invoker inside `azul.f90` that
looks the procedure up and invokes it. You wire those invokers up once
at startup with `azul_host_invoker_init()`.

## Installation

You need **GFortran** (any recent version; on macOS `brew install gcc`
provides it, on Windows use the MinGW-w64 gfortran) and `make`. The
download set is: the native library, the generated `azul.f90` module,
the generated `Makefile`, and the counter example source.

```sh
# linux
curl -O https://azul.rs/ui/release/$VERSION/libazul.so
curl -O https://azul.rs/ui/release/$VERSION/azul.f90
curl -O https://azul.rs/ui/release/$VERSION/Makefile
curl -O https://azul.rs/ui/release/$VERSION/hello_world.f90
make
./hello_world
```

```sh
# macOS
curl -O https://azul.rs/ui/release/$VERSION/libazul.dylib
curl -O https://azul.rs/ui/release/$VERSION/azul.f90
curl -O https://azul.rs/ui/release/$VERSION/Makefile
curl -O https://azul.rs/ui/release/$VERSION/hello_world.f90
make
DYLD_LIBRARY_PATH=. ./hello_world
```

```sh
# windows (MSYS2 / MinGW-w64 shell, azul.dll next to the .exe)
curl -O https://azul.rs/ui/release/$VERSION/azul.dll
curl -O https://azul.rs/ui/release/$VERSION/azul.f90
curl -O https://azul.rs/ui/release/$VERSION/Makefile
curl -O https://azul.rs/ui/release/$VERSION/hello_world.f90
make
hello_world.exe
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
! Full-GUI Fortran hello-world: counter label + "Increase counter" button.
!
! Build & run:  make && ./hello_world     (Makefile ships next to azul.f90)
!
! Callbacks go through azul.f90's host-invoker dispatch: register a
! bind(C) module procedure via azul_register_<kind>() and the returned
! Az<Kind>Callback value round-trips its handle id back into the
! registered procedure. Callbacks MUST live in a module (not as internal
! procedures) so c_funloc() needs no executable-stack trampoline.

module hello_impl
  use, intrinsic :: iso_c_binding
  use azul
  implicit none

  type :: t_model
    integer :: counter = 5
  end type t_model
  type(t_model), target, save :: model

contains

  function mk_str(s) result(r)
    character(len=*), intent(in) :: s
    type(AzString) :: r
    character(kind=c_char), dimension(max(len(s), 1)), target :: buf
    integer :: i
    do i = 1, len(s)
      buf(i) = s(i:i)
    end do
    ! AzString_fromUtf8 copies the bytes, so the automatic buffer is fine.
    r = az_string_from_utf8(c_loc(buf(1)), int(len(s), c_size_t))
  end function mk_str

  ! ButtonOnClick user callback: bump the counter, request a DOM refresh.
  ! arg0 = AzRefAny* (model handle), arg1 = CallbackInfo*, out_ptr = AzUpdate*.
  subroutine my_on_click(arg0, arg1, out_ptr) bind(C)
    type(c_ptr), value :: arg0, arg1, out_ptr
    type(c_ptr) :: praw
    type(t_model), pointer :: m
    integer(c_int), pointer :: update_out
    praw = azul_refany_get(arg0)
    if (c_associated(praw)) then
      call c_f_pointer(praw, m)
      m%counter = m%counter + 1
    end if
    if (c_associated(out_ptr)) then
      call c_f_pointer(out_ptr, update_out)
      update_out = AzUpdate_RefreshDom
    end if
    if (c_associated(arg1)) return
  end subroutine my_on_click

  ! Layout user callback: build body > [ div.font-size-32 > text(counter),
  ! button ]. arg0 = AzRefAny*, arg1 = LayoutCallbackInfo*, out_ptr = AzDom*.
  subroutine my_layout(arg0, arg1, out_ptr) bind(C)
    type(c_ptr), value :: arg0, arg1, out_ptr
    type(c_ptr) :: praw
    type(t_model), pointer :: m
    type(AzDom), pointer :: dom_out
    type(AzDom) :: body, label_wrap
    type(AzButton) :: btn
    type(AzButtonOnClickCallback) :: click_cb
    type(AzRefAny) :: click_data
    character(len=32) :: num
    body = az_dom_create_body()
    praw = azul_refany_get(arg0)
    if (c_associated(praw)) then
      call c_f_pointer(praw, m)
      write (num, '(I0)') m%counter

      label_wrap = az_dom_create_div()
      label_wrap = az_dom_with_css(label_wrap, mk_str('font-size: 32px;'))
      label_wrap = az_dom_with_child(label_wrap, &
                                     az_dom_create_text(mk_str(trim(num))))

      click_cb = azul_register_buttononclickcallback(my_on_click)
      click_data = azul_refany_create(c_loc(model))
      btn = az_button_create(mk_str('Increase counter'))
      btn = az_button_with_button_type(btn, AzButtonType_Primary)
      btn = az_button_with_on_click(btn, click_data, click_cb)

      body = az_dom_with_child(body, label_wrap)
      body = az_dom_with_child(body, az_button_dom(btn))
    end if
    if (c_associated(out_ptr)) then
      call c_f_pointer(out_ptr, dom_out)
      dom_out = body
    end if
    if (c_associated(arg1)) return
  end subroutine my_layout

end module hello_impl

program hello_world
  use, intrinsic :: iso_c_binding
  use azul
  use hello_impl
  implicit none

  ! NB: Fortran is case-insensitive — `app` would collide with the
  ! wrapper type `App` exported by the azul module.
  type(AzRefAny) :: app_data
  type(AzLayoutCallback) :: layout_cb
  type(AzWindowCreateOptions) :: wco
  type(AzApp), target :: the_app

  print '(A)', '[azul] Fortran full-GUI hello-world starting.'

  call azul_host_invoker_init()

  app_data = azul_refany_create(c_loc(model))
  layout_cb = azul_register_layoutcallback(my_layout)

  wco = az_window_create_options_default()
  wco%window_state%layout_callback = layout_cb
  wco%window_state%title = mk_str('Hello World')

  the_app = az_app_create(app_data, az_app_config_create())
  call az_app_run(c_loc(the_app), wco)
end program hello_world
```

Six things to notice.

- **Callbacks are `bind(C)` module procedures.** They MUST live in a
  module, not as internal procedures of the main program: `c_funloc()`
  on a module procedure yields a plain C function pointer, while an
  internal procedure would require a compiler-generated
  executable-stack trampoline that crashes on hardened systems.
- **`azul_host_invoker_init()`** — call it once, before `az_app_run`.
  It hands libazul the Fortran-side invoker for every callback kind
  (layout, button-click, checkbox-toggle, ...) plus the handle
  releaser. Skip it and no callback ever fires: the window opens but
  stays blank.
- **`azul_register_<kind>(proc)`** — stores `c_funloc(proc)` in a
  handle table and returns the `Az<Kind>Callback` value you pass to the
  framework (`azul_register_layoutcallback` for the window,
  `azul_register_buttononclickcallback` for the button). When the event
  fires, libazul calls the registered invoker with the handle id, which
  looks up your procedure and calls it.
- **`azul_refany_create(c_loc(model))` / `azul_refany_get(arg0)`** —
  the RefAny round-trip. `azul_refany_create` wraps a raw `c_ptr` to
  your model (which must be a `target, save` variable so the address
  stays valid for the app's lifetime); inside a callback,
  `azul_refany_get` recovers the raw pointer and `c_f_pointer` turns it
  back into a typed Fortran pointer.
- **Results go out through `out_ptr`.** User callbacks are
  `subroutine`s, not `function`s: the framework passes an out-pointer
  (`AzUpdate*` for click callbacks, `AzDom*` for layout callbacks) and
  the invoker reads whatever you wrote there. Always write it on every
  path — `update_out = AzUpdate_RefreshDom` queues a re-layout,
  `AzUpdate_DoNothing` (or writing nothing at all: don't) skips it.
- **`mk_str` copies.** `az_string_from_utf8` copies the bytes into an
  owned, refcounted `AzString`, so building it from a stack-local
  character buffer is fine. There is no `character(*)`-taking overload
  in the binding yet; `mk_str` is the four-line idiom to write once per
  project.

Also note the naming comment in the main program: Fortran is
case-insensitive, so a variable named `app` collides with the `App`
wrapper type exported by the `azul` module. Prefix your locals
(`the_app`, `app_data`) to stay clear.

## Build and run

```sh
make
./hello_world                       # linux
DYLD_LIBRARY_PATH=. ./hello_world   # macOS
```

You should see the window pictured on the
[hello-world landing page](../hello-world.md): the label renders "5",
and every click on the button increments it — the click callback bumps
`model%counter`, returns `AzUpdate_RefreshDom`, and the framework
re-runs `my_layout` with the new value.

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
- **Window opens but the button does nothing / stays blank** — you
  forgot `call azul_host_invoker_init()` before `az_app_run`, so
  libazul has no way to dispatch into Fortran.
- **Counter renders but never updates** — the click callback did not
  write `AzUpdate_RefreshDom` through `out_ptr` (or wrote nothing:
  the out-value is read by the framework, leaving it unwritten is
  undefined). Write the out-pointer on every code path.
- **Segfault inside a callback** — the model was not declared
  `target, save`, so `c_loc(model)` went stale; or the callback is an
  internal procedure instead of a module procedure.
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

## Coming Up Next

- [Application Architecture](../architecture.md) — architecting a larger Azul application
- [Document Object Model](../dom.md) — the Dom tree: node types, hierarchy, and CSS
- [Hello World [Haskell]](haskell.md)
