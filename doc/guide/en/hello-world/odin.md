---
slug: hello-world/odin
title: Hello World [Odin]
language: en
canonical_slug: hello-world/odin
audience: external
maturity: wip
guide_order: 30
topic_only: false
prerequisites: [hello-world]
tracked_files:
  - api.json
  - examples/odin/hello-world.odin
default-search-keys:
  - App
  - AppConfig
  - Dom
  - Button
  - RefAny
  - WindowCreateOptions
  - Update
---

# Hello World [Odin]

## Introduction

Odin talks to Azul through a plain C-ABI binding. Odin has no C-header
importer, so the generated `azul.odin` translates the whole FFI surface
explicitly: every `AzString` / `AzDom` becomes an Odin `struct`, every
enum an Odin `enum` with an explicit backing integer, every tagged union
a `struct #raw_union`, and every exported symbol is declared inside a
`@(default_calling_convention="c") foreign azul { ... }` block.

Because an Odin procedure declared `proc "c"` **is** a real C function
pointer, callbacks are passed to Azul directly — like Zig, Go and C,
Odin needs neither a host-invoker trampoline nor a wrapper-struct dance.
You pass the procedure itself.

You need a recent **Odin** release (`dev-2024` or newer). The binding is
shipped as an `azul/` subpackage that the `package main` driver imports
with `import azul "azul"`.

## Installation

There is no package-manager story for Odin yet — you download the native
library, the generated binding into an `azul/` subdirectory, and the
hello-world driver, then build the directory with `odin build .`:

```sh
# linux
curl -O https://azul.rs/ui/release/$VERSION/libazul.so
curl --create-dirs -o azul/azul.odin https://azul.rs/ui/release/$VERSION/azul/azul.odin
curl -O https://azul.rs/ui/release/$VERSION/hello-world.odin
odin build . -out:hello-world -extra-linker-flags:"-L."
LD_LIBRARY_PATH=. ./hello-world
```

```sh
# macos
curl -O https://azul.rs/ui/release/$VERSION/libazul.dylib
curl --create-dirs -o azul/azul.odin https://azul.rs/ui/release/$VERSION/azul/azul.odin
curl -O https://azul.rs/ui/release/$VERSION/hello-world.odin
odin build . -out:hello-world -extra-linker-flags:"-L. -framework Foundation -framework AppKit -framework OpenGL -framework CoreGraphics -framework CoreText"
DYLD_LIBRARY_PATH=. ./hello-world
```

```sh
# windows
curl -O https://azul.rs/ui/release/$VERSION/azul.dll
curl --create-dirs -o azul/azul.odin https://azul.rs/ui/release/$VERSION/azul/azul.odin
curl -O https://azul.rs/ui/release/$VERSION/hello-world.odin
odin build . -out:hello-world.exe -extra-linker-flags:"-L."
hello-world.exe
```

`foreign import azul "system:azul"` (inside `azul.odin`) makes the linker
add `-lazul`; the `-L.` extra-linker-flag points it at the `libazul` you
just downloaded. The `LD_LIBRARY_PATH=.` / `DYLD_LIBRARY_PATH=.` prefix
is needed at run time because the binary embeds no rpath — the dynamic
loader has to be told where the library lives.

## Simple "Counter" Example

This is the exact `hello-world.odin` shipped in the release (the same
file the end-to-end test builds and clicks through). It uses the raw
`azul.Az*` symbols; `azul.odin` also emits idiomatic aliases without the
`Az` prefix (e.g. `azul.App_create`), which are the raw procedures under
a shorter name.

```odin
package main

import azul "azul"

MyDataModel :: struct {
	counter: u32,
}

MY_DATA_TYPE_TOKEN: u8 = 0

my_data_type_id :: proc "contextless" () -> u64 {
	return u64(uintptr(&MY_DATA_TYPE_TOKEN))
}

my_data_destructor :: proc "c" (_: rawptr) {
}

my_data_upcast :: proc(model: MyDataModel) -> azul.AzRefAny {
	local := model
	type_name_bytes := "MyDataModel"
	type_name := azul.AzString_fromUtf8(raw_data(type_name_bytes), uint(len(type_name_bytes)))
	ptr_wrapper := azul.AzGlVoidPtrConst{ ptr = &local, run_destructor = false }
	return azul.AzRefAny_newC(
		ptr_wrapper,
		uint(size_of(MyDataModel)),
		uint(align_of(MyDataModel)),
		my_data_type_id(),
		type_name,
		my_data_destructor,
		0, 0,
	)
}

my_data_downcast :: proc "contextless" (refany: ^azul.AzRefAny) -> ^MyDataModel {
	if !azul.AzRefAny_isType(refany, my_data_type_id()) {
		return nil
	}
	ptr := azul.AzRefAny_getDataPtr(refany)
	if ptr == nil { return nil }
	return cast(^MyDataModel)ptr
}

on_click :: proc "c" (data: azul.AzRefAny, info: azul.AzCallbackInfo) -> azul.AzUpdate {
	d := data
	m := my_data_downcast(&d)
	if m == nil { return azul.AzUpdate.DoNothing }
	m.counter += 1
	return azul.AzUpdate.RefreshDom
}

layout :: proc "c" (data: azul.AzRefAny, info: azul.AzLayoutCallbackInfo) -> azul.AzDom {
	d := data
	m := my_data_downcast(&d)
	if m == nil { return azul.AzDom_createBody() }

	buf: [16]u8
	n := u32_write(m.counter, buf[:])
	counter_str := azul.AzString_fromUtf8(raw_data(buf[:]), uint(n))
	label := azul.AzDom_createText(counter_str)

	label_wrapper := azul.AzDom_createDiv()
	font_size := azul.AzStyleFontSize_px(32.0)
	css_prop := azul.AzCssProperty_fontSize(font_size)
	cond := azul.AzCssPropertyWithConditions_simple(css_prop)
	azul.AzDom_addCssProperty(&label_wrapper, cond)
	azul.AzDom_addChild(&label_wrapper, label)

	btn_label := azul.AzString_fromUtf8(raw_data("Increase counter"), 16)
	button := azul.AzButton_create(btn_label)
	azul.AzButton_setButtonType(&button, azul.AzButtonType.Primary)
	data_clone := azul.AzRefAny_clone(&d)
	azul.AzButton_setOnClick(&button, data_clone, on_click)
	button_dom := azul.AzButton_dom(button)

	body := azul.AzDom_createBody()
	azul.AzDom_addChild(&body, label_wrapper)
	azul.AzDom_addChild(&body, button_dom)
	return body
}

main :: proc() {
	model := MyDataModel{ counter = 5 }
	data := my_data_upcast(model)

	window := azul.AzWindowCreateOptions_create(layout)
	window.window_state.title = azul.AzString_fromUtf8(raw_data("Hello World"), 11)
	window.window_state.size.dimensions.width = 400.0
	window.window_state.size.dimensions.height = 300.0
	window.window_state.flags.decorations = azul.AzWindowDecorations.NoTitleAutoInject
	window.window_state.flags.background_material = azul.AzWindowBackgroundMaterial.Sidebar

	app := azul.AzApp_create(data, azul.AzAppConfig_create())
	azul.AzApp_run(&app, window)
}
```

(The `u32_write` helper — a contextless integer-to-decimal formatter — is
elided above; see the shipped example for the full ~15 lines. It avoids
pulling in `core:fmt` so the `proc "c"` `layout` never needs to set up an
Odin `context`.)

### Callbacks are bare C function pointers

`on_click` and `layout` are declared `proc "c"`, which makes them
ABI-identical to the C typedefs `AzButtonOnClickCallbackType` and
`AzLayoutCallbackType`. You pass the procedure *itself*:

```odin
azul.AzButton_setOnClick(&button, data_clone, on_click)
window := azul.AzWindowCreateOptions_create(layout)
```

The typed `AzButton_setOnClick` takes the **bare fn pointer**, not an
`AzButtonOnClickCallback` struct — `azul.odin` binds the raw C variant
whose argument is the `proc "c"` typedef. There is no host-invoker, no
closure allocation, and no hidden registry: the framework stores your
pointer and calls straight back into your Odin code on the UI thread.

Helper procedures that a `proc "c"` callback calls (here `my_data_type_id`
and `my_data_downcast`) are declared `proc "contextless"` so they can be
invoked without an Odin `context` in scope.

### How RefAny works in Odin

`RefAny` is Azul's type-erased, reference-counted box for your
application state. The example hand-rolls the same three pieces the C
`AZ_REFLECT` macro generates:

- **Type identity** — `my_data_type_id()` returns the address of a
  package global (`&MY_DATA_TYPE_TOKEN`). It is process-unique and
  stable, so `AzRefAny_isType` can verify a downcast at run time.
- **Upcast** — `AzRefAny_newC` *copies* `size_of(MyDataModel)` bytes into
  a refcounted heap allocation, so pointing it at a stack local is fine;
  `run_destructor = false` tells libazul not to free the caller's
  pointer.
- **Downcast** — `AzRefAny_isType` + `AzRefAny_getDataPtr` recover a
  typed `^MyDataModel`; both callbacks bail out (`return nil` /
  `createBody()`) when the check fails.

`AzRefAny_clone(&d)` bumps the (atomic) reference count — it does not
deep-copy your struct. On click the framework matches the hit-test, calls
`on_click` with the stored `RefAny`, your code downcasts and increments
`counter`, returns `azul.AzUpdate.RefreshDom`, and the framework re-runs
`layout`, which reads the new value.

Two more things worth noticing:

- **Strings** — `AzString_fromUtf8(ptr, len)` copies the bytes into a
  refcounted heap buffer, so passing a stack `[16]u8` buffer through
  `raw_data(buf[:])` is safe: the `AzString` outlives your stack frame.
- **Typed CSS** — instead of parsing a CSS string, the example builds the
  property programmatically: `AzStyleFontSize_px(32.0)` →
  `AzCssProperty_fontSize` → `AzCssPropertyWithConditions_simple` →
  `AzDom_addCssProperty`.

## Build and run

```sh
# linux
odin build . -out:hello-world -extra-linker-flags:"-L."
LD_LIBRARY_PATH=. ./hello-world

# macos (framework flags matter — see Common errors)
odin build . -out:hello-world -extra-linker-flags:"-L. -framework Foundation -framework AppKit -framework OpenGL -framework CoreGraphics -framework CoreText"
DYLD_LIBRARY_PATH=. ./hello-world

# windows
odin build . -out:hello-world.exe -extra-linker-flags:"-L."
hello-world.exe
```

`odin build .` compiles the current directory as `package main` and
resolves `import azul "azul"` to the `azul/` subpackage next to it. You
should see the window pictured on the
[hello-world landing page](../hello-world.md). Click the button: the
counter increments, `layout` re-runs, and the new value renders.

## Common errors

- **`could not find package "azul"`** — the binding is not where the
  import expects it. `azul.odin` must live in an `azul/` subdirectory of
  the directory you run `odin build .` in (the install steps `curl` it to
  `azul/azul.odin`).
- **`undefined reference to Az...` at link time** — the linker cannot
  find `libazul`. Keep `-extra-linker-flags:"-L."` and make sure the
  native library sits in the current directory.
- **Runtime: `cannot open shared object file` / `library not found`** —
  the binary embeds no rpath, so keep the `LD_LIBRARY_PATH=.` /
  `DYLD_LIBRARY_PATH=.` prefix from the install steps.
- **Undefined symbols mentioning AppKit/OpenGL on macOS** — add the
  system frameworks: `-framework Foundation -framework AppKit -framework
  OpenGL -framework CoreGraphics -framework CoreText`.
- **Counter does not update on click** — `on_click` returned
  `AzUpdate.DoNothing`, or the downcast failed. A failed downcast usually
  means the type-id does not match: it must come from the address of the
  *same* global token used in the upcast.

## Coming Up Next

- [Application Architecture](../architecture.md) — architecting a larger Azul application
- [Document Object Model](../dom.md) — the Dom tree: node types, hierarchy, and CSS
- [Hello World [Zig]](zig.md)
