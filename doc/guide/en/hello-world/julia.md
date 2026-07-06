---
slug: hello-world/julia
title: Hello World [Julia]
language: en
canonical_slug: hello-world/julia
audience: external
maturity: wip
guide_order: 31
topic_only: false
prerequisites: [hello-world]
tracked_files:
  - api.json
  - examples/julia/hello-world.jl
default-search-keys:
  - App
  - AppConfig
  - Dom
  - Button
  - RefAny
  - WindowCreateOptions
  - Update
---

# Hello World [Julia]

> **Experimental / CI-validated.** The Julia binding is an off-frontpage,
> C-ABI-direct target. It is generated and exercised end-to-end by CI, but
> is not part of the curated front-page language set.

## Introduction

Julia talks to Azul through a plain C-ABI binding. The generated `azul.jl`
translates the whole FFI surface explicitly: every `AzString` / `AzDom`
becomes an **isbits** Julia `struct`, every enum an `@enum` with an
explicit backing integer, every tagged union an isbits byte-blob `struct`,
and every exported symbol a thin `ccall` wrapper function inside
`module Azul`.

Because `@cfunction(f, Ret, (Args...,))` mints a **real C function
pointer** from any top-level Julia function, callbacks are passed to Azul
directly — like Odin, Zig, Go and C, Julia needs neither a host-invoker
trampoline nor a wrapper-struct dance. You pass the pointer itself. (A
callback fired from a *foreign* thread needs extra `@cfunction` care, but
the counter's callbacks fire on the main event-loop thread, so this is
fine.)

You need a recent **Julia** (1.6+). No packages are required — `ccall`,
`@cfunction` and `@enum` are all in `Base`. The binding is shipped as an
`azul/` subdirectory that the driver loads with
`include(".../azul/azul.jl"); using .Azul`.

## Installation

There is no package-manager story for Julia yet — you download the native
library, the generated binding into an `azul/` subdirectory, and the
hello-world driver, then run it (Julia JIT-compiles on the fly, so there is
no separate build step):

```sh
# linux
curl -O https://azul.rs/ui/release/$VERSION/libazul.so
curl --create-dirs -o azul/azul.jl https://azul.rs/ui/release/$VERSION/azul/azul.jl
curl -O https://azul.rs/ui/release/$VERSION/hello-world.jl
AZUL_LIB=$PWD/libazul.so julia hello-world.jl
```

```sh
# macos
curl -O https://azul.rs/ui/release/$VERSION/libazul.dylib
curl --create-dirs -o azul/azul.jl https://azul.rs/ui/release/$VERSION/azul/azul.jl
curl -O https://azul.rs/ui/release/$VERSION/hello-world.jl
AZUL_LIB=$PWD/libazul.dylib julia hello-world.jl
```

```sh
# windows (PowerShell)
curl -O https://azul.rs/ui/release/$VERSION/azul.dll
curl --create-dirs -o azul/azul.jl https://azul.rs/ui/release/$VERSION/azul/azul.jl
curl -O https://azul.rs/ui/release/$VERSION/hello-world.jl
$env:AZUL_LIB = "$PWD\azul.dll"; julia hello-world.jl
```

The binding's `ccall((:AzApp_create, LIBAZUL), …)` `dlopen`s `libazul` at
call time. `LIBAZUL` defaults to the plain library name (`libazul` /
`azul`); set the `AZUL_LIB` environment variable to an absolute path — as
above — so the loader finds the library you just downloaded without having
to touch `LD_LIBRARY_PATH` / `DYLD_LIBRARY_PATH`.

## Simple "Counter" Example

This is the exact `hello-world.jl` shipped in the release (the same file
the end-to-end test runs and clicks through). It uses the raw `Azul.Az*`
names; `azul.jl` also emits idiomatic aliases without the `Az` prefix
(e.g. `Azul.App_create`), which are the raw wrappers under a shorter name.

```julia
include(joinpath(@__DIR__, "azul", "azul.jl"))
using .Azul

struct MyDataModel
    counter::UInt32
end

const MY_DATA_TOKEN = Ref{UInt8}(0)
my_data_type_id() = UInt64(UInt(pointer_from_objref(MY_DATA_TOKEN)))
my_data_destructor(::Ptr{Cvoid})::Cvoid = nothing
vptr(r::Ref) = Ptr{Cvoid}(pointer_from_objref(r))

function my_data_upcast(model::MyDataModel)
    local_ref = Ref(model)
    return GC.@preserve local_ref begin
        wrapper = Azul.AzGlVoidPtrConst(vptr(local_ref), false)
        dtor = @cfunction(my_data_destructor, Cvoid, (Ptr{Cvoid},))
        Azul.AzRefAny_newC(
            wrapper,
            Csize_t(sizeof(MyDataModel)),
            Csize_t(Base.datatype_alignment(MyDataModel)),
            my_data_type_id(),
            Azul.az_string("MyDataModel"),
            dtor, C_NULL, C_NULL)
    end
end

function my_data_ptr(dref::Ref{Azul.AzRefAny})
    p = vptr(dref)
    Azul.AzRefAny_isType(p, my_data_type_id()) || return Ptr{MyDataModel}(C_NULL)
    return Ptr{MyDataModel}(Azul.AzRefAny_getDataPtr(p))
end

function on_click(data::Azul.AzRefAny, info::Azul.AzCallbackInfo)::Azul.AzUpdate
    dref = Ref(data)
    return GC.@preserve dref begin
        mp = my_data_ptr(dref)
        if mp == Ptr{MyDataModel}(C_NULL)
            Azul.AzUpdate_DoNothing
        else
            m = unsafe_load(mp)
            unsafe_store!(mp, MyDataModel(m.counter + UInt32(1)))
            Azul.AzUpdate_RefreshDom
        end
    end
end

function layout(data::Azul.AzRefAny, info::Azul.AzLayoutCallbackInfo)::Azul.AzDom
    dref = Ref(data)
    counter = GC.@preserve dref begin
        mp = my_data_ptr(dref)
        mp == Ptr{MyDataModel}(C_NULL) ? nothing : unsafe_load(mp).counter
    end
    counter === nothing && return Azul.AzDom_createBody()

    label = Azul.AzDom_createText(Azul.az_string(string(counter)))
    label_wrapper = Ref(Azul.AzDom_createDiv())
    css_prop = Azul.AzCssProperty_fontSize(Azul.AzStyleFontSize_px(32.0f0))
    cond = Azul.AzCssPropertyWithConditions_simple(css_prop)
    GC.@preserve label_wrapper begin
        Azul.AzDom_addCssProperty(vptr(label_wrapper), cond)
        Azul.AzDom_addChild(vptr(label_wrapper), label)
    end

    button = Ref(Azul.AzButton_create(Azul.az_string("Increase counter")))
    on_click_ptr = @cfunction(on_click, Azul.AzUpdate, (Azul.AzRefAny, Azul.AzCallbackInfo))
    button_dom = GC.@preserve button dref begin
        Azul.AzButton_setButtonType(vptr(button), Azul.AzButtonType_Primary)
        data_clone = Azul.AzRefAny_clone(vptr(dref))
        Azul.AzButton_setOnClick(vptr(button), data_clone, on_click_ptr)
        Azul.AzButton_dom(button[])
    end

    body = Ref(Azul.AzDom_createBody())
    GC.@preserve body begin
        Azul.AzDom_addChild(vptr(body), label_wrapper[])
        Azul.AzDom_addChild(vptr(body), button_dom)
    end
    return body[]
end

function main()
    data = my_data_upcast(MyDataModel(UInt32(5)))
    layout_ptr = @cfunction(layout, Azul.AzDom, (Azul.AzRefAny, Azul.AzLayoutCallbackInfo))
    window = Azul.AzWindowCreateOptions_create(layout_ptr)

    ws = window.window_state
    window = Azul.setfields(window;
        window_state = Azul.setfields(ws;
            title = Azul.az_string("Hello World"),
            size = Azul.setfields(ws.size;
                dimensions = Azul.setfields(ws.size.dimensions; width = 400.0f0, height = 300.0f0)),
            flags = Azul.setfields(ws.flags;
                decorations = Azul.AzWindowDecorations_NoTitleAutoInject,
                background_material = Azul.AzWindowBackgroundMaterial_Sidebar)))

    app = Ref(Azul.AzApp_create(data, Azul.AzAppConfig_create()))
    GC.@preserve app Azul.AzApp_run(vptr(app), window)
end

main()
```

### Callbacks are bare C function pointers

`@cfunction(on_click, AzUpdate, (AzRefAny, AzCallbackInfo))` compiles
`on_click` into a native C function pointer (`Ptr{Cvoid}`) that is
ABI-identical to the C typedef `AzButtonOnClickCallbackType`. You pass the
pointer *itself*:

```julia
Azul.AzButton_setOnClick(vptr(button), data_clone, on_click_ptr)
window = Azul.AzWindowCreateOptions_create(layout_ptr)
```

The typed `AzButton_setOnClick` takes the **bare fn pointer**, not an
`AzButtonOnClickCallback` struct — `azul.jl` binds the raw C variant whose
argument is the callback typedef. There is no host-invoker, no closure
allocation, and no hidden registry: the framework stores your pointer and
calls straight back into your Julia code on the UI thread.

### Structs are isbits (passed by value)

Every `Az*` type is emitted as an **isbits** Julia `struct`, so `ccall`
passes it by value with the C ABI — no boxing, no wrapper. The trade-off:
isbits structs are *immutable*, so you cannot write
`window.window_state.title = …`. The generated `setfields(x; field = …)`
helper returns a copy with the named fields replaced, and nested updates
compose:

```julia
window = Azul.setfields(window;
    window_state = Azul.setfields(ws; title = Azul.az_string("Hello World")))
```

For C functions that mutate through a `&mut self` pointer
(`AzDom_addChild`, `AzButton_setOnClick`), keep the value in a `Ref`, pass
a `Ptr{Cvoid}` to its storage (`vptr(ref)` under `GC.@preserve`), and read
the result back with `ref[]`.

### Tagged unions

A Rust `#[repr(C, u8)]` tagged union (e.g. `AzCssProperty`) has no Julia
equivalent, so `azul.jl` emits it as an isbits byte-blob `struct` whose
size and alignment are computed *at module load* from the per-variant
structs. It is byte-for-byte ABI-compatible with the C union; you never
read its fields in Julia — you construct it via a C function
(`AzCssProperty_fontSize(…)`) and pass it straight back
(`AzCssPropertyWithConditions_simple(…)`).

### How RefAny works in Julia

`RefAny` is Azul's type-erased, reference-counted box for your application
state. The example hand-rolls the same three pieces the C `AZ_REFLECT`
macro generates:

- **Type identity** — `my_data_type_id()` returns the address of a
  module-global `Ref` (`pointer_from_objref(MY_DATA_TOKEN)`). It is
  process-unique and stable, so `AzRefAny_isType` can verify a downcast at
  run time.
- **Upcast** — `AzRefAny_newC` *copies* `sizeof(MyDataModel)` bytes into a
  refcounted heap allocation, so pointing it at a `Ref`-boxed local is
  fine; `run_destructor = false` tells libazul not to free the caller's
  pointer.
- **Downcast** — `AzRefAny_isType` + `AzRefAny_getDataPtr` recover a
  `Ptr{MyDataModel}`; both callbacks bail out when the check fails.

`AzRefAny_clone(vptr(dref))` bumps the (atomic) reference count — it does
not deep-copy your struct. On click the framework calls `on_click` with the
stored `RefAny`; your code recovers the pointer, `unsafe_store!`s the
incremented counter, returns `Azul.AzUpdate_RefreshDom`, and the framework
re-runs `layout`, which reads the new value.

Two more things worth noticing:

- **Strings** — `az_string(s)` wraps `AzString_fromUtf8(ptr, len)`, which
  copies the bytes into a refcounted heap buffer, so a temporary Julia
  string is safe.
- **Typed CSS** — instead of parsing a CSS string, the example builds the
  property programmatically: `AzStyleFontSize_px(32.0f0)` →
  `AzCssProperty_fontSize` → `AzCssPropertyWithConditions_simple` →
  `AzDom_addCssProperty`.

## Run

```sh
# linux
AZUL_LIB=$PWD/libazul.so julia hello-world.jl

# macos
AZUL_LIB=$PWD/libazul.dylib julia hello-world.jl

# windows (PowerShell)
$env:AZUL_LIB = "$PWD\azul.dll"; julia hello-world.jl
```

You should see the window pictured on the
[hello-world landing page](../hello-world.md). Click the button: the
counter increments, `layout` re-runs, and the new value renders.

## Common errors

- **`could not load library "libazul"`** — the loader cannot find the
  native library. Set `AZUL_LIB` to its absolute path (the install steps do
  this), or put it on `LD_LIBRARY_PATH` / `DYLD_LIBRARY_PATH` / `PATH`.
- **`UndefVarError: Azul not defined`** — the binding is not where the
  `include` expects it. `azul.jl` must live in an `azul/` subdirectory next
  to `hello-world.jl` (the install steps `curl` it to `azul/azul.jl`).
- **`@cfunction` / signature errors** — the callback's Julia signature must
  match the C typedef exactly: `on_click(::AzRefAny, ::AzCallbackInfo) ->
  AzUpdate`, `layout(::AzRefAny, ::AzLayoutCallbackInfo) -> AzDom`. All
  three types are isbits, so they cross the boundary by value.
- **Counter does not update on click** — `on_click` returned
  `AzUpdate_DoNothing`, or the downcast failed. A failed downcast usually
  means the type-id does not match: it must come from the address of the
  *same* global token used in the upcast.

## Coming Up Next

- [Application Architecture](../architecture.md) — architecting a larger Azul application
- [Document Object Model](../dom.md) — the Dom tree: node types, hierarchy, and CSS
- [Hello World [Odin]](odin.md)
