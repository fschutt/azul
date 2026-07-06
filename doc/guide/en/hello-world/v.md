---
slug: hello-world/v
title: Hello World [V]
language: en
canonical_slug: hello-world/v
audience: external
maturity: wip
guide_order: 31
topic_only: false
prerequisites: [hello-world]
tracked_files:
  - api.json
  - examples/v/hello-world.v
default-search-keys:
  - App
  - AppConfig
  - Dom
  - Button
  - RefAny
  - WindowCreateOptions
  - Update
---

# Hello World [V]

> **Experimental / CI-validated.** V is an archetype-A (C-ABI-direct)
> backend, off the front-page tab set. The binding is generated and
> exercised by CI rather than hand-maintained.

## Introduction

V talks to Azul through a plain C-ABI binding. The generated `azul.v`
translates the whole FFI surface explicitly: every `AzString` / `AzDom`
becomes a V `struct`, every enum a V `enum` with an explicit backing
integer, every tagged union a V `union`, every callback typedef a V
`fn`-type alias, and every exported symbol a top-level
`fn C.AzApp_create(...)` extern declaration.

Because a top-level V function compiles to a plain C function, callbacks
are passed to Azul directly — like C, Zig and Odin, V needs neither a
host-invoker trampoline nor a wrapper-struct dance. You pass the function
itself.

You need a recent **V** toolchain (`v version` ≥ 0.4). The binding is
shipped as an `azul/` subpackage that the `module main` driver imports
with `import azul`; you call the raw C symbols through V's `C.` namespace
(`C.AzApp_create`) and use the `azul.Az*` types.

## Installation

There is no package-manager story for V yet — you download the native
library, the generated binding into an `azul/` subdirectory, and the
hello-world driver, then build the directory with `v run .`:

```sh
# linux
curl -O https://azul.rs/ui/release/$VERSION/libazul.so
curl --create-dirs -o azul/azul.v https://azul.rs/ui/release/$VERSION/azul/azul.v
curl -O https://azul.rs/ui/release/$VERSION/hello-world.v
LD_LIBRARY_PATH=. v run .
```

```sh
# macos
curl -O https://azul.rs/ui/release/$VERSION/libazul.dylib
curl --create-dirs -o azul/azul.v https://azul.rs/ui/release/$VERSION/azul/azul.v
curl -O https://azul.rs/ui/release/$VERSION/hello-world.v
DYLD_LIBRARY_PATH=. v run .
```

```sh
# windows
curl -O https://azul.rs/ui/release/$VERSION/azul.dll
curl --create-dirs -o azul/azul.v https://azul.rs/ui/release/$VERSION/azul/azul.v
curl -O https://azul.rs/ui/release/$VERSION/hello-world.v
v run .
```

The `#flag -L.` + `#flag -lazul` lines inside `azul.v` make V's C backend
link against the `libazul` you just downloaded (`-L.` points at the
current directory). The `LD_LIBRARY_PATH=.` / `DYLD_LIBRARY_PATH=.` prefix
is needed at run time because the binary embeds no rpath — the dynamic
loader has to be told where the library lives.

## Simple "Counter" Example

This is the exact `hello-world.v` shipped in the release (the same file
the end-to-end test builds and clicks through). Functions are called
through V's `C.` namespace; types come from the imported `azul` module.

```v
module main

import azul

struct MyDataModel {
mut:
	counter u32
}

const my_data_type_id = u64(0x617a756c5f6d646d) // "azul_mdm"

fn my_data_destructor(ptr voidptr) {
}

fn az_str(s string) azul.AzString {
	return C.AzString_fromUtf8(s.str, usize(s.len))
}

fn my_data_upcast(model MyDataModel) azul.AzRefAny {
	mut local := model
	blob := azul.AzGlVoidPtrConst{
		ptr:            voidptr(&local)
		run_destructor: false
	}
	return C.AzRefAny_newC(blob, usize(sizeof(MyDataModel)), usize(4),
		my_data_type_id, az_str('MyDataModel'), my_data_destructor, usize(0), usize(0))
}

fn my_data_downcast(refany &azul.AzRefAny) &MyDataModel {
	if !C.AzRefAny_isType(refany, my_data_type_id) {
		return unsafe { nil }
	}
	ptr := C.AzRefAny_getDataPtr(refany)
	if isnil(ptr) {
		return unsafe { nil }
	}
	return unsafe { &MyDataModel(ptr) }
}

fn on_click(data azul.AzRefAny, info azul.AzCallbackInfo) azul.AzUpdate {
	mut d := data
	m := my_data_downcast(&d)
	if isnil(m) {
		return azul.AzUpdate.DoNothing
	}
	unsafe {
		m.counter++
	}
	return azul.AzUpdate.RefreshDom
}

fn layout(data azul.AzRefAny, info azul.AzLayoutCallbackInfo) azul.AzDom {
	mut d := data
	m := my_data_downcast(&d)
	if isnil(m) {
		return C.AzDom_createBody()
	}

	counter_val := unsafe { m.counter }
	label := C.AzDom_createText(az_str(counter_val.str()))

	mut label_wrapper := C.AzDom_createDiv()
	css_prop := C.AzCssProperty_fontSize(C.AzStyleFontSize_px(32.0))
	C.AzDom_addCssProperty(&label_wrapper, C.AzCssPropertyWithConditions_simple(css_prop))
	C.AzDom_addChild(&label_wrapper, label)

	mut button := C.AzButton_create(az_str('Increase counter'))
	C.AzButton_setButtonType(&button, azul.AzButtonType.Primary)
	data_clone := C.AzRefAny_clone(&d)
	C.AzButton_setOnClick(&button, data_clone, on_click)
	button_dom := C.AzButton_dom(button)

	mut body := C.AzDom_createBody()
	C.AzDom_addChild(&body, label_wrapper)
	C.AzDom_addChild(&body, button_dom)
	return body
}

fn main() {
	data := my_data_upcast(MyDataModel{ counter: 5 })

	mut window := C.AzWindowCreateOptions_create(layout)
	window.window_state.title = az_str('Hello World')
	window.window_state.size.dimensions.width = 400.0
	window.window_state.size.dimensions.height = 300.0
	window.window_state.flags.decorations = azul.AzWindowDecorations.NoTitleAutoInject
	window.window_state.flags.background_material = azul.AzWindowBackgroundMaterial.Sidebar

	mut app := C.AzApp_create(data, C.AzAppConfig_create())
	C.AzApp_run(&app, window)
}
```

### Callbacks are bare C function pointers

`on_click` and `layout` are ordinary top-level V functions, which the V
compiler lowers to plain C functions — ABI-identical to the C typedefs
`AzButtonOnClickCallbackType` and `AzLayoutCallbackType`. You pass the
function *itself*:

```v
C.AzButton_setOnClick(&button, data_clone, on_click)
window := C.AzWindowCreateOptions_create(layout)
```

The raw `AzButton_setOnClick` takes the **bare fn pointer**, not an
`AzButtonOnClickCallback` struct — `azul.v` declares the raw C variant
whose argument is the `fn (AzRefAny, AzCallbackInfo) AzUpdate` typedef.
There is no host-invoker, no closure allocation, and no hidden registry:
the framework stores your pointer and calls straight back into your V
code on the UI thread.

### How RefAny works in V

`RefAny` is Azul's type-erased, reference-counted box for your
application state. The example hand-rolls the same three pieces the C
`AZ_REFLECT` macro generates:

- **Type identity** — `my_data_type_id` is a fixed, process-stable `u64`.
  (The C macro uses the address of a global; any unique constant is
  equally valid, and simpler in V.) `AzRefAny_isType` verifies a downcast
  against it at run time.
- **Upcast** — `AzRefAny_newC` *copies* `sizeof(MyDataModel)` bytes into a
  refcounted heap allocation, so pointing it at a stack local is fine;
  `run_destructor: false` tells libazul not to free the caller's pointer.
- **Downcast** — `AzRefAny_isType` + `AzRefAny_getDataPtr` recover a typed
  `&MyDataModel`; both callbacks bail out (`isnil(...)` →
  `AzUpdate.DoNothing` / `createBody()`) when the check fails.

`C.AzRefAny_clone(&d)` bumps the (atomic) reference count — it does not
deep-copy your struct. On click the framework matches the hit-test, calls
`on_click` with the stored `RefAny`, your code downcasts and increments
`counter`, returns `azul.AzUpdate.RefreshDom`, and the framework re-runs
`layout`, which reads the new value.

Two more things worth noticing:

- **Strings** — `AzString_fromUtf8(ptr, len)` copies the bytes into a
  refcounted heap buffer, so passing `s.str` / `s.len` from a temporary V
  `string` is safe: the `AzString` outlives the caller's frame.
- **Typed CSS** — instead of parsing a CSS string, the example builds the
  property programmatically: `AzStyleFontSize_px(32.0)` →
  `AzCssProperty_fontSize` → `AzCssPropertyWithConditions_simple` →
  `AzDom_addCssProperty`.

## Build and run

```sh
# linux
LD_LIBRARY_PATH=. v run .

# macos
DYLD_LIBRARY_PATH=. v run .

# windows (azul.dll in the current directory)
v run .
```

`v run .` compiles the current directory (`hello-world.v` + the imported
`azul/` subpackage) and runs it. You should see the window pictured on the
[hello-world landing page](../hello-world.md). Click the button: the
counter increments, `layout` re-runs, and the new value renders. To ship a
standalone binary, build with `v -prod . -o hello-world` instead.

## Common errors

- **`module "azul" not found`** — the binding is not where the import
  expects it. `azul.v` must live in an `azul/` subdirectory of the
  directory you run `v run .` in (the install steps `curl` it to
  `azul/azul.v`).
- **`undefined reference to Az...` at link time** — the linker cannot find
  `libazul`. Keep the `#flag -L.` line in `azul.v` and make sure the
  native library sits in the current directory.
- **Runtime: `cannot open shared object file` / `library not found`** —
  the binary embeds no rpath, so keep the `LD_LIBRARY_PATH=.` /
  `DYLD_LIBRARY_PATH=.` prefix from the install steps.
- **Undefined symbols mentioning AppKit/OpenGL on macOS** — the
  `#flag darwin -framework …` line in `azul.v` supplies the system
  frameworks; make sure it survived any manual edits.
- **Counter does not update on click** — `on_click` returned
  `AzUpdate.DoNothing`, or the downcast failed. A failed downcast means
  the type id did not match: `my_data_type_id` must be the *same* constant
  in the upcast and the downcast.

## Coming Up Next

- [Application Architecture](../architecture.md) — architecting a larger Azul application
- [Document Object Model](../dom.md) — the Dom tree: node types, hierarchy, and CSS
- [Hello World [Odin]](odin.md)
