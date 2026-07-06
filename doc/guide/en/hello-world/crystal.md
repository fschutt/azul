---
slug: hello-world/crystal
title: Hello World [Crystal]
language: en
canonical_slug: hello-world/crystal
audience: external
maturity: experimental
guide_order: 31
topic_only: false
prerequisites: [hello-world]
tracked_files:
  - api.json
  - examples/crystal/hello-world.cr
default-search-keys:
  - App
  - AppConfig
  - Dom
  - Button
  - RefAny
  - WindowCreateOptions
  - Update
---

# Hello World [Crystal]

> **Experimental / CI-validated.** The Crystal binding is generated and
> compiled in CI but is not yet a front-page target. The FFI surface is
> the same one the C / Zig / Odin bindings use.

## Introduction

Crystal talks to Azul through a plain C-ABI binding. Crystal has a
first-class C interop layer (`lib`), so the generated `azul.cr` translates
the whole FFI surface explicitly into one `@[Link("azul")] lib LibAzul`
block: every `AzString` / `AzDom` becomes a Crystal `struct`, every enum a
Crystal `enum` with an explicit backing integer, every tagged union a
Crystal `union`, every callback typedef a `Proc` alias, and every exported
symbol a `fun` binding whose C name is preserved verbatim.

Because a non-capturing Crystal proc (`->(a : T) { ... }`) compiles to a
real C function pointer, callbacks are passed to Azul directly — like Zig,
Go, Odin and C, Crystal needs neither a host-invoker trampoline nor a
wrapper-struct dance. You pass the proc itself. (The proc must not capture
any outer local, or Crystal will refuse to hand it to a C function; the
example keeps its callbacks non-closure by reaching helpers through
module constants.)

You need a recent **Crystal** release (1.0 or newer). The binding is
shipped as a single `azul.cr` that the driver `require`s.

## Installation

There is no shard/package-manager story for Crystal yet — you download the
native library, the generated binding, and the hello-world driver into one
directory, then build it with `crystal build`:

```sh
# linux
curl -O $HOSTNAME/ui/release/$VERSION/libazul.so
curl -O $HOSTNAME/ui/release/$VERSION/azul.cr
curl -O $HOSTNAME/ui/release/$VERSION/hello-world.cr
crystal build hello-world.cr --link-flags "-L."
LD_LIBRARY_PATH=. ./hello-world
```

```sh
# macos
curl -O $HOSTNAME/ui/release/$VERSION/libazul.dylib
curl -O $HOSTNAME/ui/release/$VERSION/azul.cr
curl -O $HOSTNAME/ui/release/$VERSION/hello-world.cr
crystal build hello-world.cr --link-flags "-L. -framework Foundation -framework AppKit -framework OpenGL -framework CoreGraphics -framework CoreText"
DYLD_LIBRARY_PATH=. ./hello-world
```

```sh
# windows
curl -O $HOSTNAME/ui/release/$VERSION/azul.dll
curl -O $HOSTNAME/ui/release/$VERSION/azul.cr
curl -O $HOSTNAME/ui/release/$VERSION/hello-world.cr
crystal build hello-world.cr --link-flags "/LIBPATH:."
hello-world.exe
```

`@[Link("azul")]` (inside `azul.cr`) makes the linker add `-lazul`; the
`--link-flags "-L."` points it at the `libazul` you just downloaded. The
`LD_LIBRARY_PATH=.` / `DYLD_LIBRARY_PATH=.` prefix is needed at run time
because the binary embeds no rpath — the dynamic loader has to be told
where the library lives.

## Simple "Counter" Example

This is the exact `hello-world.cr` shipped in the release (the same file
the end-to-end test builds and clicks through). It calls the `LibAzul.*`
functions directly; `azul.cr` also emits idiomatic type aliases without
the `Az` prefix under a `module Azul` (e.g. `Azul::Dom`), namespaced so
they never shadow Crystal core types like `String`.

```crystal
require "./azul"

module MyData
  struct Model
    property counter : UInt32

    def initialize(@counter : UInt32)
    end
  end

  TOKEN = Pointer(UInt8).malloc(1)

  def self.type_id : UInt64
    TOKEN.address.to_u64
  end

  DESTRUCTOR = ->(_ptr : Void*) { }

  def self.upcast(model : Model) : LibAzul::AzRefAny
    local = model
    type_name = "Model"
    name = LibAzul.azString_fromUtf8(type_name.to_unsafe, LibC::SizeT.new(type_name.bytesize))

    wrapper = LibAzul::AzGlVoidPtrConst.new
    wrapper.ptr = pointerof(local).as(Void*)
    wrapper.run_destructor = false

    LibAzul.azRefAny_newC(
      wrapper,
      LibC::SizeT.new(sizeof(Model)),
      LibC::SizeT.new(alignof(Model)),
      type_id,
      name,
      DESTRUCTOR,
      LibC::SizeT.new(0),
      LibC::SizeT.new(0)
    )
  end

  def self.downcast(refany : LibAzul::AzRefAny*) : Model*
    return Pointer(Model).null unless LibAzul.azRefAny_isType(refany, type_id)
    ptr = LibAzul.azRefAny_getDataPtr(refany)
    return Pointer(Model).null if ptr.null?
    ptr.as(Model*)
  end
end

ON_CLICK = ->(data : LibAzul::AzRefAny, _info : LibAzul::AzCallbackInfo) : LibAzul::AzUpdate {
  d = data
  m = MyData.downcast(pointerof(d))
  next LibAzul::AzUpdate::DoNothing if m.null?
  m.value.counter += 1
  LibAzul::AzUpdate::RefreshDom
}

LAYOUT = ->(data : LibAzul::AzRefAny, _info : LibAzul::AzLayoutCallbackInfo) : LibAzul::AzDom {
  d = data
  m = MyData.downcast(pointerof(d))
  next LibAzul.azDom_createBody if m.null?

  text = m.value.counter.to_s
  counter_str = LibAzul.azString_fromUtf8(text.to_unsafe, LibC::SizeT.new(text.bytesize))
  label = LibAzul.azDom_createText(counter_str)

  label_wrapper = LibAzul.azDom_createDiv
  font_size = LibAzul.azStyleFontSize_px(32.0_f32)
  css_prop = LibAzul.azCssProperty_fontSize(font_size)
  cond = LibAzul.azCssPropertyWithConditions_simple(css_prop)
  LibAzul.azDom_addCssProperty(pointerof(label_wrapper), cond)
  LibAzul.azDom_addChild(pointerof(label_wrapper), label)

  btn_label = "Increase counter"
  button = LibAzul.azButton_create(
    LibAzul.azString_fromUtf8(btn_label.to_unsafe, LibC::SizeT.new(btn_label.bytesize))
  )
  LibAzul.azButton_setButtonType(pointerof(button), LibAzul::AzButtonType::Primary)
  data_clone = LibAzul.azRefAny_clone(pointerof(d))
  LibAzul.azButton_setOnClick(pointerof(button), data_clone, ON_CLICK)
  button_dom = LibAzul.azButton_dom(button)

  body = LibAzul.azDom_createBody
  LibAzul.azDom_addChild(pointerof(body), label_wrapper)
  LibAzul.azDom_addChild(pointerof(body), button_dom)
  body
}

model = MyData::Model.new(5_u32)
data = MyData.upcast(model)

window = LibAzul.azWindowCreateOptions_create(LAYOUT)
title = "Hello World"
window.window_state.title = LibAzul.azString_fromUtf8(title.to_unsafe, LibC::SizeT.new(title.bytesize))
window.window_state.size.dimensions.width = 400.0_f32
window.window_state.size.dimensions.height = 300.0_f32
window.window_state.flags.decorations = LibAzul::AzWindowDecorations::NoTitleAutoInject
window.window_state.flags.background_material = LibAzul::AzWindowBackgroundMaterial::Sidebar

app = LibAzul.azApp_create(data, LibAzul.azAppConfig_create)
LibAzul.azApp_run(pointerof(app), window)
```

### Callbacks are bare C function pointers

`ON_CLICK` and `LAYOUT` are non-capturing procs, which makes them
ABI-identical to the C typedefs `AzButtonOnClickCallbackType` and
`AzLayoutCallbackType` (emitted in `azul.cr` as `Proc` aliases). You pass
the proc *itself*:

```crystal
LibAzul.azButton_setOnClick(pointerof(button), data_clone, ON_CLICK)
window = LibAzul.azWindowCreateOptions_create(LAYOUT)
```

The typed `azButton_setOnClick` takes the **bare fn pointer**, not an
`AzButtonOnClickCallback` struct — `azul.cr` binds the raw C variant whose
argument is the `Proc` typedef. There is no host-invoker, no closure
allocation, and no hidden registry: the framework stores your pointer and
calls straight back into your Crystal code on the UI thread.

The one Crystal-specific rule: **a proc handed to a C function must not
capture.** The example keeps its callbacks closure-free by reaching every
helper through a *constant* (`MyData.type_id`, `MyData.downcast`,
`ON_CLICK`) rather than an outer local variable. Had they closed over a
local, Crystal would reject the assignment at compile time
(*"can't pass closure to C function"*).

### How RefAny works in Crystal

`RefAny` is Azul's type-erased, reference-counted box for your application
state. The example hand-rolls the same three pieces the C `AZ_REFLECT`
macro generates:

- **Type identity** — `MyData.type_id` returns the address of a one-byte
  heap token (`TOKEN`). It is process-unique and stable, so
  `azRefAny_isType` can verify a downcast at run time.
- **Upcast** — `azRefAny_newC` *copies* `sizeof(Model)` bytes into a
  refcounted heap allocation, so pointing it at a stack local is fine;
  `run_destructor = false` tells libazul not to free the caller's pointer.
- **Downcast** — `azRefAny_isType` + `azRefAny_getDataPtr` recover a typed
  `Model*`; both callbacks bail out (`Pointer(Model).null` /
  `createBody`) when the check fails.

`azRefAny_clone(pointerof(d))` bumps the (atomic) reference count — it does
not deep-copy your struct. On click the framework matches the hit-test,
calls `ON_CLICK` with the stored `RefAny`, your code downcasts and
increments `counter` (`m.value.counter += 1` writes through the pointer),
returns `AzUpdate::RefreshDom`, and the framework re-runs `LAYOUT`, which
reads the new value.

Two more things worth noticing:

- **Strings** — `azString_fromUtf8(ptr, len)` copies the bytes into a
  refcounted heap buffer, so passing a Crystal `String`'s `to_unsafe`
  pointer is safe: the `AzString` owns its own copy.
- **Typed CSS** — instead of parsing a CSS string, the example builds the
  property programmatically: `azStyleFontSize_px(32.0_f32)` →
  `azCssProperty_fontSize` → `azCssPropertyWithConditions_simple` →
  `azDom_addCssProperty`.

## Build and run

```sh
# linux
crystal build hello-world.cr --link-flags "-L."
LD_LIBRARY_PATH=. ./hello-world

# macos (framework flags matter — see Common errors)
crystal build hello-world.cr --link-flags "-L. -framework Foundation -framework AppKit -framework OpenGL -framework CoreGraphics -framework CoreText"
DYLD_LIBRARY_PATH=. ./hello-world

# windows
crystal build hello-world.cr --link-flags "/LIBPATH:."
hello-world.exe
```

You should see the window pictured on the
[hello-world landing page](../hello-world.md). Click the button: the
counter increments, `LAYOUT` re-runs, and the new value renders.

## Common errors

- **`can't find file './azul'`** — the generated binding is not next to
  the driver. `azul.cr` must sit in the same directory you run
  `crystal build hello-world.cr` in (the install steps `curl` it there).
- **`undefined reference to Az...` at link time** — the linker cannot find
  `libazul`. Keep `--link-flags "-L."` and make sure the native library
  sits in the current directory.
- **`can't pass closure to C function`** — a callback proc captured an
  outer local. Reach helpers through constants / module methods (as the
  example does) so the proc stays non-closure.
- **Runtime: `cannot open shared object file` / `library not found`** —
  the binary embeds no rpath, so keep the `LD_LIBRARY_PATH=.` /
  `DYLD_LIBRARY_PATH=.` prefix from the install steps.
- **Undefined symbols mentioning AppKit/OpenGL on macOS** — add the system
  frameworks: `-framework Foundation -framework AppKit -framework OpenGL
  -framework CoreGraphics -framework CoreText`.
- **Counter does not update on click** — `ON_CLICK` returned
  `AzUpdate::DoNothing`, or the downcast failed. A failed downcast usually
  means the type-id does not match: it must come from the address of the
  *same* `TOKEN` used in the upcast.

## Coming Up Next

- [Application Architecture](../architecture.md) — architecting a larger Azul application
- [Document Object Model](../dom.md) — the Dom tree: node types, hierarchy, and CSS
- [Hello World [Odin]](odin.md)
