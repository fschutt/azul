---
slug: hello-world/go
title: Hello World [Go]
language: en
canonical_slug: hello-world/go
audience: external
maturity: wip
guide_order: 23
topic_only: false
prerequisites: [hello-world]
tracked_files:
  - api.json
  - examples/go/main.go
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

# Hello World [Go]

## Introduction

The Go story is **cgo directly against `azul.h`**: the counter example
below puts the C header in a cgo preamble and calls `C.AzApp_run`,
`C.AzDom_createBody` etc. straight through. Two cgo features make this
work without any host-invoker machinery:

- `//export` turns a Go function into a real C symbol, so the framework
  can call back into Go with a plain C function pointer, and
- cgo compiles a small C preamble into your package, which is where the
  fn-pointer cast helpers live (see the walkthrough below for why they
  are needed).

Because this is cgo, you need a **C compiler** at build time (`gcc` on
Linux, Xcode Command Line Tools `clang` on macOS, MinGW `gcc` on
Windows) in addition to Go 1.21+, and `CGO_ENABLED=1` (the default on
native builds). Fair warning, straight from the binding header:
cross-compiling cgo programs is genuinely painful — build on the target
platform if you can.

A generated wrapper package (`azul.go`, `types.go`, `functions.go`,
`wrappers.go` — `package azul`) also ships with the release; the
verified example does **not** import it, it calls the C ABI directly.

## Installation

You download the prebuilt library, the C header, and the cgo
hello-world, then build — `main.go` calls the C API directly through
cgo, so no generated Go package is needed:

```sh
# linux (requires gcc on PATH)
curl -O https://azul.rs/ui/release/$VERSION/libazul.so
curl -O https://azul.rs/ui/release/$VERSION/azul.h
curl -O https://azul.rs/ui/release/$VERSION/main.go
go mod init hello-world
CGO_CFLAGS="-I." CGO_LDFLAGS="-L. -lazul -lpthread -lm -ldl" go build -o hello-world .
LD_LIBRARY_PATH=. ./hello-world
```

```sh
# macos (requires Xcode CLT: xcode-select --install)
curl -O https://azul.rs/ui/release/$VERSION/libazul.dylib
curl -O https://azul.rs/ui/release/$VERSION/azul.h
curl -O https://azul.rs/ui/release/$VERSION/main.go
go mod init hello-world
CGO_CFLAGS="-I." CGO_LDFLAGS="-L. -lazul -framework AppKit -framework OpenGL -framework CoreGraphics -framework CoreText -framework CoreFoundation" go build -o hello-world .
DYLD_LIBRARY_PATH=. ./hello-world
```

```cmd
:: windows (requires MinGW gcc on PATH)
curl -O https://azul.rs/ui/release/$VERSION/azul.dll
curl -O https://azul.rs/ui/release/$VERSION/azul.h
curl -O https://azul.rs/ui/release/$VERSION/main.go
go mod init hello-world
curl -O https://azul.rs/ui/release/$VERSION/azul.dll.lib
set CGO_ENABLED=1
set CGO_CFLAGS=-I.
set CGO_LDFLAGS=azul.dll.lib
go build -o hello-world.exe .
hello-world.exe
```

The `CGO_CFLAGS` / `CGO_LDFLAGS` environment variables are not
optional: `main.go`'s own cgo block only says `-lazul`, so the include
path, library path, and the platform-specific link flags (system libs
on Linux, frameworks on macOS, the `azul.dll.lib` import library on
Windows) must come from the environment. On Windows, cgo links the
import library directly and `azul.dll` resolves from the current
directory at run time.

## Simple "Counter" Example

This is the exact `main.go` shipped in the release (the same file the
end-to-end test builds and clicks through):

```go
// CGO_CFLAGS="-I." CGO_LDFLAGS="-L." go build && LD_LIBRARY_PATH=. ./hello-world

package main

/*
#cgo LDFLAGS: -lazul
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include "azul.h"

// Forward declarations for the Go-exported callbacks below. cgo
// generates a header `_cgo_export.h` with these too, but pulling them
// in here lets the C-side cast lift `AzCallbackType` / `AzLayoutCallbackType`
// out into single helpers.
extern AzUpdate goOnClick        (AzRefAny data, AzCallbackInfo info);
extern AzDom    goLayout         (AzRefAny data, AzLayoutCallbackInfo info);
extern void     myDataDestructor (void* m);

// AzButton_setOnClick / AzWindowCreateOptions_create take a RAW C-ABI
// function pointer (AzCallbackType / AzLayoutCallbackType), NOT the
// AzCallback wrapper struct. cgo maps a raw fn-pointer typedef to
// `*[0]byte` and a struct to `_Ctype_struct_Az...`, so returning the
// struct here is a type error at the Go call site. Return the raw
// fn-pointer types directly.
static inline AzCallbackType              make_click_callback     (void) { return (AzCallbackType)goOnClick; }
static inline AzLayoutCallbackType        make_layout_callback    (void) { return (AzLayoutCallbackType)goLayout; }
static inline AzRefAnyDestructorType      make_my_data_destructor (void) { return (AzRefAnyDestructorType)myDataDestructor; }
*/
import "C"

import (
	"fmt"
	"unsafe"
)

// ── Data model ────────────────────────────────────────────────────────
//
// Mirrors the C macro `AZ_REFLECT_JSON(MyDataModel, ...)`: a compile-
// time-unique type id (the address of a package var), an `upcast` that
// wraps the struct in an `AzRefAny`, and a `downcast` that recovers a
// typed pointer back from the refany.

type myDataModel struct {
	counter C.uint32_t
}

// The address of this package var is the per-type RTTI id.
var myDataTypeToken byte
var myDataTypeID = C.uint64_t(uintptr(unsafe.Pointer(&myDataTypeToken)))

//export myDataDestructor
func myDataDestructor(_ unsafe.Pointer) {}

func myDataUpcast(model myDataModel) C.AzRefAny {
	local := model // stack copy; AzRefAny_newC copies the bytes
	typeName := []byte("MyDataModel")
	cTypeName := C.AzString_fromUtf8((*C.uint8_t)(unsafe.Pointer(&typeName[0])), C.size_t(len(typeName)))
	ptr := C.AzGlVoidPtrConst{
		ptr:            unsafe.Pointer(&local),
		run_destructor: C.bool(false),
	}
	return C.AzRefAny_newC(
		ptr,
		C.size_t(unsafe.Sizeof(local)),
		C.size_t(unsafe.Alignof(local)),
		myDataTypeID,
		cTypeName,
		C.make_my_data_destructor(),
		0, // serialize_fn
		0, // deserialize_fn
	)
}

func myDataDowncast(refany *C.AzRefAny) *myDataModel {
	if !bool(C.AzRefAny_isType(refany, myDataTypeID)) {
		return nil
	}
	raw := C.AzRefAny_getDataPtr(refany)
	if raw == nil {
		return nil
	}
	return (*myDataModel)(raw)
}

// ── Callback: button click ────────────────────────────────────────────

//export goOnClick
func goOnClick(data C.AzRefAny, _ C.AzCallbackInfo) C.AzUpdate {
	d := data
	m := myDataDowncast(&d)
	if m == nil {
		return C.AzUpdate_DoNothing
	}
	m.counter++
	return C.AzUpdate_RefreshDom
}

// ── Layout callback ───────────────────────────────────────────────────

//export goLayout
func goLayout(data C.AzRefAny, _ C.AzLayoutCallbackInfo) C.AzDom {
	d := data
	m := myDataDowncast(&d)
	if m == nil {
		return C.AzDom_createBody()
	}

	// Counter label (wrapped in a div so the font-size sticks).
	counterStr := []byte(fmt.Sprintf("%d", m.counter))
	counterAz := C.AzString_fromUtf8((*C.uint8_t)(unsafe.Pointer(&counterStr[0])), C.size_t(len(counterStr)))
	label := C.AzDom_createText(counterAz)

	labelWrapper := C.AzDom_createDiv()
	fontSize := C.AzStyleFontSize_px(C.float(32.0))
	cssProp := C.AzCssProperty_fontSize(fontSize)
	cond := C.AzCssPropertyWithConditions_simple(cssProp)
	C.AzDom_addCssProperty(&labelWrapper, cond)
	C.AzDom_addChild(&labelWrapper, label)

	// Increment button. `AzCallback_create` wraps the Go-exported
	// fn-pointer in a `{ cb, ctx=None }` struct; the C ABI takes
	// `AzCallback` for setOnClick.
	btnLabelBytes := []byte("Increase counter")
	btnLabel := C.AzString_fromUtf8((*C.uint8_t)(unsafe.Pointer(&btnLabelBytes[0])), C.size_t(len(btnLabelBytes)))
	button := C.AzButton_create(btnLabel)
	C.AzButton_setButtonType(&button, C.AzButtonType_Primary)
	dataClone := C.AzRefAny_clone(&d)
	C.AzButton_setOnClick(&button, dataClone, C.make_click_callback())
	buttonDom := C.AzButton_dom(button)

	// Body.
	body := C.AzDom_createBody()
	C.AzDom_addChild(&body, labelWrapper)
	C.AzDom_addChild(&body, buttonDom)
	return body
}

// ── Main ──────────────────────────────────────────────────────────────

func main() {
	model := myDataModel{counter: 5}
	data := myDataUpcast(model)

	window := C.AzWindowCreateOptions_create(C.make_layout_callback())
	titleBytes := []byte("Hello World")
	window.window_state.title = C.AzString_fromUtf8((*C.uint8_t)(unsafe.Pointer(&titleBytes[0])), C.size_t(len(titleBytes)))
	window.window_state.size.dimensions.width = 400.0
	window.window_state.size.dimensions.height = 300.0

	// NoTitleAutoInject: OS draws close/min/max buttons; framework
	// auto-injects a Titlebar with drag support.
	window.window_state.flags.decorations = C.AzWindowDecorations_NoTitleAutoInject
	window.window_state.flags.background_material = C.AzWindowBackgroundMaterial_Sidebar

	app := C.AzApp_create(data, C.AzAppConfig_create())
	C.AzApp_run(&app, window)
}
```

### How callbacks work with cgo

Three moving parts, all visible in the preamble at the top of the file:

1. **`//export goOnClick`** — cgo compiles the Go function into a real
   C symbol with the exact C-ABI signature `AzUpdate (AzRefAny,
   AzCallbackInfo)`. The framework calls it like any other C function
   pointer; no reflection, no trampoline.
2. **`extern` forward declarations** — cgo generates these in
   `_cgo_export.h` anyway, but repeating them in the preamble lets the
   next piece reference the functions.
3. **`static inline make_*_callback()` cast helpers** — the one cgo
   quirk. `AzButton_setOnClick` and `AzWindowCreateOptions_create` take
   *raw C fn-pointer typedefs* (`AzButtonOnClickCallbackType`,
   `AzLayoutCallbackType`) — not a callback wrapper struct. cgo maps a
   raw fn-pointer typedef to the opaque Go type `*[0]byte`, and Go will
   not let you cast a Go function to that at the call site. So the cast
   happens once, on the C side, in a tiny helper that returns the
   already-cast pointer: `C.make_click_callback()`. (Ignore the stale
   inline comment above the button code that mentions
   `AzCallback_create` — as the preamble comment explains, the setter
   takes the raw fn pointer, and that is what the code passes.)

Data flow on click: the framework matches the hit-test, calls the
exported `goOnClick` with the button's stored `RefAny`, your code
mutates `counter` and returns `C.AzUpdate_RefreshDom`, and the
framework re-invokes `goLayout` to rebuild the DOM with the new value.

### How RefAny works in Go

`RefAny` is Azul's type-erased, refcounted box for application state.
The example hand-rolls the C `AZ_REFLECT` macro in ~30 lines:

- **Type identity** — the address of the package variable
  `myDataTypeToken` is process-unique and stable, so it serves as the
  runtime type id that `AzRefAny_isType` checks before every downcast.
- **Upcast** — `AzRefAny_newC` **copies** `unsafe.Sizeof(local)` bytes
  into libazul's own heap allocation. This is also what makes the cgo
  pointer-passing rules happy: the Go pointer `&local` is only read
  *during* the call and never retained by C, so no Go memory is pinned.
  `run_destructor: false` means libazul will not try to free the Go
  pointer — only its own heap copy is destroyed (via the exported
  destructor) when the last clone drops.
- **Downcast** — `AzRefAny_isType` + `AzRefAny_getDataPtr` recover a
  typed `*myDataModel` pointing at libazul's heap copy (which is why
  `m.counter++` persists between callbacks). Both callbacks return a
  safe default when the downcast fails.
- **`AzRefAny_clone`** — bumps the atomic refcount (no deep copy); the
  clone's ownership moves into the button so the framework can hand the
  data back to `goOnClick` later.

Strings work the same way: `C.AzString_fromUtf8(ptr, len)` copies the
bytes out of the Go byte slice into a refcounted buffer during the
call, so the slice can be garbage-collected afterwards.

## Build and run

```sh
# macos
CGO_CFLAGS="-I." CGO_LDFLAGS="-L. -lazul -framework AppKit -framework OpenGL -framework CoreGraphics -framework CoreText -framework CoreFoundation" go build -o hello-world .
DYLD_LIBRARY_PATH=. ./hello-world

# linux
CGO_CFLAGS="-I." CGO_LDFLAGS="-L. -lazul -lpthread -lm -ldl" go build -o hello-world .
LD_LIBRARY_PATH=. ./hello-world
```

You should see the window pictured on the
[hello-world landing page](../hello-world.md). Click the button: the
counter increments, `goLayout` re-runs, and the new value renders.

## Common errors

- **`found packages azul (azul.go) and main (main.go)`** — you also
  downloaded the generated Go package files (`azul.go`, `types.go`,
  `functions.go`, `wrappers.go` — all `package azul`). This example
  does not use them; keep them in a separate directory (or delete
  them), since Go allows only one package per directory.
- **`azul.h: No such file or directory` / `cannot find -lazul`** — the
  `CGO_CFLAGS="-I."` / `CGO_LDFLAGS="-L. -lazul ..."` environment
  variables are missing. `main.go`'s cgo block only contains `-lazul`;
  the search paths and platform link flags must come from the
  environment (or edit the `#cgo` lines).
- **`C compiler "gcc" not found` / `cgo: C compiler not available`** —
  cgo needs a native C toolchain: `gcc` (Linux), Xcode CLT (macOS,
  `xcode-select --install`), MinGW (Windows). Also check
  `CGO_ENABLED=1` — it defaults to `0` when cross-compiling.
- **Runtime: `library not found` / `cannot open shared object file`** —
  the loader cannot find `libazul.{so,dylib}`; keep the
  `LD_LIBRARY_PATH=.` / `DYLD_LIBRARY_PATH=.` prefix from the install
  steps (on Windows, `azul.dll` just has to sit next to the `.exe`).
- **Counter does not update on click** — `goOnClick` returned
  `C.AzUpdate_DoNothing`, or the downcast failed. A failing downcast
  usually means the type id passed to `AzRefAny_newC` and the one
  checked in `myDataDowncast` are not the same package-var address.
- **Cross-compiling fails** — expected; cgo cross-compiles need a full
  foreign C toolchain + sysroot. Build natively on each target
  platform.

## Coming Up Next

- [Application Architecture](../architecture.md) — architecting a larger Azul application
- [Document Object Model](../dom.md) — the Dom tree: node types, hierarchy, and CSS
- [Hello World [Zig]](zig.md)
