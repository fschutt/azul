---
slug: hello-world/nim
title: Hello World [Nim]
language: en
canonical_slug: hello-world/nim
audience: external
maturity: wip
guide_order: 29
topic_only: false
prerequisites: [hello-world]
tracked_files:
  - api.json
  - examples/nim/hello-world.nim
last_generated_rev: 0463d0e3a
generated_at: 2026-07-06T00:00:00Z
default-search-keys:
  - App
  - AppConfig
  - Dom
  - Button
  - RefAny
  - WindowCreateOptions
  - Update
---

# Hello World [Nim]

## Introduction

Nim compiles to C, so it talks to Azul through the plain C ABI with no
runtime shim in between. The generated `azul.nim` declares every C struct
as a `{.bycopy.} object`, every C enum as a size-pinned Nim enum, and
every `AzFoo_bar` function as a `proc … {.importc, cdecl, dynlib:
azulLib.}`. The `dynlib` pragma dlopens the shared library at run time,
so you do not even pass link flags — the `.so` / `.dylib` / `.dll` only
has to be discoverable.

The important part for a GUI framework is callbacks. A top-level Nim
`proc (data: AzRefAny; info: AzCallbackInfo): AzUpdate {.cdecl.}` is a
**real C function pointer** — it is handed straight to
`AzButton_setOnClick`. Nim is therefore one of the C-ABI-direct bindings
(like Zig, Odin, and C itself): no host-invoker trampoline, no handle
table, no wrapper-struct dance. The counter below passes its `onClick`
and `layout` procs directly to Azul, exactly like the C and Zig examples.

`azul.nim` also ships an idiomatic wrapper layer that drops the `Az`
prefix (`domCreateBody()`, `button.setButtonType(...)`); the counter uses
the raw `Az*` layer because that is the path exercised end-to-end by the
test suite.

You need **Nim 1.6 or newer** (tested against **2.0**).

## Installation

There is no package-manager story yet — download the native library, the
generated binding, and the hello-world into one directory and build it
there. Because `azul.nim` uses the `dynlib` pragma, `nim c` does not link
`libazul` at compile time; the library is loaded at run time, which is
why the run step sets `*_LIBRARY_PATH=.`.

```sh
# linux
curl -O https://azul.rs/ui/release/$VERSION/libazul.so
curl -O https://azul.rs/ui/release/$VERSION/azul.nim
curl -O https://azul.rs/ui/release/$VERSION/hello-world.nim
nim c -d:release hello-world.nim
LD_LIBRARY_PATH=. ./hello-world
```

```sh
# macos
curl -O https://azul.rs/ui/release/$VERSION/libazul.dylib
curl -O https://azul.rs/ui/release/$VERSION/azul.nim
curl -O https://azul.rs/ui/release/$VERSION/hello-world.nim
nim c -d:release hello-world.nim
DYLD_LIBRARY_PATH=. ./hello-world
```

```sh
# windows
curl -O https://azul.rs/ui/release/$VERSION/azul.dll
curl -O https://azul.rs/ui/release/$VERSION/azul.nim
curl -O https://azul.rs/ui/release/$VERSION/hello-world.nim
nim c -d:release hello-world.nim
.\hello-world.exe
```

The macOS framework dependencies (Foundation, AppKit, OpenGL, …) are
carried inside `libazul.dylib`'s own load commands, so they are pulled in
automatically when the library is dlopen'd — you do not link them
yourself.

## Simple "Counter" Example

This is the exact `hello-world.nim` shipped in the release (the same file
the end-to-end test builds and clicks through):

```nim
import azul

# ── Data model ─────────────────────────────────────────────────────────
type
  MyDataModel = object
    counter: uint32

var myDataTypeToken: uint8 = 0
proc myDataTypeId(): uint64 = cast[uint64](addr myDataTypeToken)

proc myDataDestructor(p: pointer) {.cdecl.} = discard

proc azStr(s: string): AzString =
  if s.len == 0:
    AzString_fromUtf8(nil, csize_t(0))
  else:
    AzString_fromUtf8(cast[ptr uint8](s.cstring), csize_t(s.len))

proc myDataUpcast(model: MyDataModel): AzRefAny =
  var local = model
  let blob = AzGlVoidPtrConst(`ptr`: cast[pointer](addr local), run_destructor: false)
  AzRefAny_newC(
    blob,
    csize_t(sizeof(MyDataModel)),
    csize_t(alignof(MyDataModel)),
    myDataTypeId(),
    azStr("MyDataModel"),
    myDataDestructor,
    csize_t(0), csize_t(0))

proc myDataDowncast(refany: ptr AzRefAny): ptr MyDataModel =
  if not AzRefAny_isType(refany, myDataTypeId()): return nil
  let p = AzRefAny_getDataPtr(refany)
  if p == nil: return nil
  cast[ptr MyDataModel](p)

# ── Callback: button click ─────────────────────────────────────────────
proc onClick(data: AzRefAny, info: AzCallbackInfo): AzUpdate {.cdecl.} =
  var d = data
  let m = myDataDowncast(addr d)
  if m == nil: return AzUpdate_DoNothing
  m.counter += 1
  return AzUpdate_RefreshDom

# ── Layout callback ────────────────────────────────────────────────────
proc layout(data: AzRefAny, info: AzLayoutCallbackInfo): AzDom {.cdecl.} =
  var d = data
  let m = myDataDowncast(addr d)
  if m == nil: return AzDom_createBody()

  let label = AzDom_createText(azStr($m.counter))
  var labelWrapper = AzDom_createDiv()
  let cond = AzCssPropertyWithConditions_simple(
    AzCssProperty_fontSize(AzStyleFontSize_px(32.0'f32)))
  AzDom_addCssProperty(addr labelWrapper, cond)
  AzDom_addChild(addr labelWrapper, label)

  var button = AzButton_create(azStr("Increase counter"))
  AzButton_setButtonType(addr button, AzButtonType_Primary)
  let dataClone = AzRefAny_clone(addr d)
  AzButton_setOnClick(addr button, dataClone, onClick)
  let buttonDom = AzButton_dom(button)

  var body = AzDom_createBody()
  AzDom_addChild(addr body, labelWrapper)
  AzDom_addChild(addr body, buttonDom)
  return body

# ── Main ───────────────────────────────────────────────────────────────
proc main() =
  let model = MyDataModel(counter: 5)
  let data = myDataUpcast(model)

  var window = AzWindowCreateOptions_create(layout)
  window.window_state.title = azStr("Hello World")
  window.window_state.size.dimensions.width = 400.0'f32
  window.window_state.size.dimensions.height = 300.0'f32
  window.window_state.flags.decorations = AzWindowDecorations_NoTitleAutoInject
  window.window_state.flags.background_material = AzWindowBackgroundMaterial_Sidebar

  var app = AzApp_create(data, AzAppConfig_create())
  AzApp_run(addr app, window)
  AzApp_delete(addr app)

main()
```

## How it works

### The data model and `RefAny`

Your application state lives in a plain Nim `object`. To hand it to Azul
you wrap it in an `AzRefAny` — a reference-counted, type-tagged box.
`myDataUpcast` copies the struct into a fresh heap allocation via
`AzRefAny_newC`; `myDataDowncast` checks the type tag and returns a typed
`ptr MyDataModel` back out. The type id is just the address of a global
byte that is never read — a cheap, process-unique token.

`AzGlVoidPtrConst` has a field literally named `ptr`, which is a Nim
keyword, so `azul.nim` emits it backtick-stropped and you construct it as
`AzGlVoidPtrConst(`ptr`: …, run_destructor: false)`.

### Callbacks pass straight through

`onClick` and `layout` are ordinary top-level procs marked `{.cdecl.}`.
That calling-convention annotation makes each one a genuine C function
pointer whose signature matches the generated `AzButtonOnClickCallbackType`
/ `AzLayoutCallbackType` proc types, so they are passed directly to
`AzButton_setOnClick` and `AzWindowCreateOptions_create`. There is no
marshalling layer — this is the whole reason Nim is a C-ABI-direct
binding.

Inside the callback you re-`downcast` the `AzRefAny` to reach your state.
`onClick` bumps the counter and returns `AzUpdate_RefreshDom`, which tells
Azul to re-run `layout`; `layout` rebuilds the DOM from the current
counter value.

### Building the DOM

`layout` builds a `<body>` containing a `<div>` (font-size 32px) wrapping
the counter text, plus an "Increase counter" button. Builder functions
such as `AzDom_addChild` / `AzDom_addCssProperty` take a `ptr AzDom` to
the node being mutated (hence `addr labelWrapper`) and consume their
by-value inputs. `AzButton_dom` converts the finished button into a DOM
node.

### Memory ownership

Azul consumes the by-value records you pass into it (the `AzDom`, the
`AzRefAny` clone, the `AzWindowCreateOptions`), so you do not free them
yourself — ownership transfers across the FFI boundary. `AzApp_run` drives
the event loop until the window closes; `AzApp_delete` then releases the
app. The one clone you make explicitly (`AzRefAny_clone`) is the copy the
button holds onto for its click handler; Azul drops it when the button is
destroyed.
