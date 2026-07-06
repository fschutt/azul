---
slug: hello-world/swift
title: Hello World [Swift]
language: en
canonical_slug: hello-world/swift
audience: external
maturity: wip
guide_order: 35
topic_only: false
prerequisites: [hello-world]
tracked_files:
  - api.json
  - examples/swift/hello-world.swift
default-search-keys:
  - App
  - AppConfig
  - Dom
  - Button
  - RefAny
  - WindowCreateOptions
  - Update
---

# Hello World [Swift]

> **Experimental / CI-validated.** The Swift binding is built and run by the
> macOS end-to-end matrix (the `macos-14` runner already ships the Swift
> toolchain), so it is exercised on every release — but it is newer than the
> front-page bindings and is not yet package-manager distributed.

## Introduction

Swift talks to Azul through the generated C header `azul.h`. Swift has
first-class C interoperability, so — unlike Odin — the binding does **not**
redeclare the FFI surface by hand. Instead a tiny `module.modulemap` exposes
`azul.h` as a Clang module named `CAzul`, and Swift's importer reproduces
every `AzString` / `AzDom` struct, every enum, and every `#[repr(C)]` tagged
union with its *authoritative* C layout — for free.

This is the same strategy the Zig binding uses (`@cImport`). It matters
because a native Swift `struct` does **not** have a guaranteed C-compatible
layout, and Swift cannot spell a `repr(C)` tagged union at all — so
hand-translating azul's many union types into pure Swift would be unsound.
Importing the C header sidesteps the whole problem.

Because a Swift function with a C-compatible signature converts implicitly to
a `@convention(c)` function pointer — a *real* C function pointer — callbacks
are passed to Azul directly, like Zig, Go, Odin and C. No host-invoker
trampoline, no wrapper-struct dance: you pass the function itself.

You need the **Swift** toolchain (Swift 5.7+; the examples are tested with
Swift 6). `azul.swift` is a thin idiomatic layer: it does
`@_exported import CAzul` (re-exporting the whole C surface) and emits
`Az`-stripped procedure aliases such as `App_create = AzApp_create`.

## Installation

There is no Swift-package-manager story yet — you download the native library,
the generated `azul.h` + `azul.swift`, the static `module.modulemap`, and the
hello-world driver, then compile the directory with `swiftc`:

```sh
# linux
curl -O https://azul.rs/ui/release/$VERSION/libazul.so
curl -O https://azul.rs/ui/release/$VERSION/azul.h
curl -O https://azul.rs/ui/release/$VERSION/azul.swift
curl -O https://azul.rs/ui/release/$VERSION/module.modulemap
curl -O https://azul.rs/ui/release/$VERSION/hello-world.swift
swiftc -I. hello-world.swift azul.swift -L. -lazul -o hello-world
LD_LIBRARY_PATH=. ./hello-world
```

```sh
# macos
curl -O https://azul.rs/ui/release/$VERSION/libazul.dylib
curl -O https://azul.rs/ui/release/$VERSION/azul.h
curl -O https://azul.rs/ui/release/$VERSION/azul.swift
curl -O https://azul.rs/ui/release/$VERSION/module.modulemap
curl -O https://azul.rs/ui/release/$VERSION/hello-world.swift
swiftc -I. hello-world.swift azul.swift -L. -lazul \
  -framework Foundation -framework AppKit -framework OpenGL \
  -framework CoreGraphics -framework CoreText \
  -o hello-world
DYLD_LIBRARY_PATH=. ./hello-world
```

```sh
# windows
curl -O https://azul.rs/ui/release/$VERSION/azul.dll
curl -O https://azul.rs/ui/release/$VERSION/azul.h
curl -O https://azul.rs/ui/release/$VERSION/azul.swift
curl -O https://azul.rs/ui/release/$VERSION/module.modulemap
curl -O https://azul.rs/ui/release/$VERSION/hello-world.swift
swiftc -I. hello-world.swift azul.swift -L. -lazul -o hello-world.exe
hello-world.exe
```

`-I.` lets Swift find `module.modulemap` (and thus resolve `import CAzul`);
`-L. -lazul` links the native library. The `LD_LIBRARY_PATH=.` /
`DYLD_LIBRARY_PATH=.` prefix is needed at run time because the binary embeds
no rpath — the dynamic loader has to be told where the library lives.

## Simple "Counter" Example

This is the exact `hello-world.swift` shipped in the release (the same file the
end-to-end test builds and clicks through). It uses the raw `Az*` symbols
imported from `CAzul`; `azul.swift` also emits idiomatic aliases without the
`Az` prefix (e.g. `App_create`), which are the same procedures under a shorter
name.

```swift
import CAzul

struct MyDataModel {
    var counter: UInt32
}

private let myDataToken = UnsafeMutablePointer<UInt8>.allocate(capacity: 1)
private let myDataTypeId = UInt64(UInt(bitPattern: myDataToken))

func myDataDestructor(_ ptr: UnsafeMutableRawPointer?) {}

func azString(_ s: String) -> AzString {
    let bytes = Array(s.utf8)
    return bytes.withUnsafeBufferPointer { AzString_fromUtf8($0.baseAddress, $0.count) }
}

func myDataUpcast(_ model: MyDataModel) -> AzRefAny {
    var local = model
    let typeName = azString("MyDataModel")
    return withUnsafePointer(to: &local) { p in
        let wrapper = AzGlVoidPtrConst(ptr: UnsafeRawPointer(p), run_destructor: false)
        return AzRefAny_newC(
            wrapper,
            MemoryLayout<MyDataModel>.size,
            MemoryLayout<MyDataModel>.alignment,
            myDataTypeId,
            typeName,
            myDataDestructor,
            0, 0
        )
    }
}

func myDataDowncast(_ refany: inout AzRefAny) -> UnsafeMutablePointer<MyDataModel>? {
    if !AzRefAny_isType(&refany, myDataTypeId) { return nil }
    guard let ptr = AzRefAny_getDataPtr(&refany) else { return nil }
    return UnsafeMutableRawPointer(mutating: ptr).assumingMemoryBound(to: MyDataModel.self)
}

func onClick(_ data: AzRefAny, _ info: AzCallbackInfo) -> AzUpdate {
    var d = data
    guard let m = myDataDowncast(&d) else { return AzUpdate_DoNothing }
    m.pointee.counter += 1
    return AzUpdate_RefreshDom
}

func layout(_ data: AzRefAny, _ info: AzLayoutCallbackInfo) -> AzDom {
    var d = data
    guard let m = myDataDowncast(&d) else { return AzDom_createBody() }

    let counterStr = azString(String(m.pointee.counter))
    let label = AzDom_createText(counterStr)

    var labelWrapper = AzDom_createDiv()
    let fontSize = AzStyleFontSize_px(32.0)
    let cssProp = AzCssProperty_fontSize(fontSize)
    let cond = AzCssPropertyWithConditions_simple(cssProp)
    AzDom_addCssProperty(&labelWrapper, cond)
    AzDom_addChild(&labelWrapper, label)

    var button = AzButton_create(azString("Increase counter"))
    AzButton_setButtonType(&button, AzButtonType_Primary)
    let dataClone = AzRefAny_clone(&d)
    AzButton_setOnClick(&button, dataClone, onClick)
    let buttonDom = AzButton_dom(button)

    var body = AzDom_createBody()
    AzDom_addChild(&body, labelWrapper)
    AzDom_addChild(&body, buttonDom)
    return body
}

@main
struct HelloWorld {
    static func main() {
        let model = MyDataModel(counter: 5)
        let data = myDataUpcast(model)

        var window = AzWindowCreateOptions_create(layout)
        window.window_state.title = azString("Hello World")
        window.window_state.size.dimensions.width = 400.0
        window.window_state.size.dimensions.height = 300.0
        window.window_state.flags.decorations = AzWindowDecorations_NoTitleAutoInject
        window.window_state.flags.background_material = AzWindowBackgroundMaterial_Sidebar

        var app = AzApp_create(data, AzAppConfig_create())
        AzApp_run(&app, window)
    }
}
```

The driver's executable code lives in a `@main` type: Swift only allows plain
top-level statements in a file literally named `main.swift`, and `azul.swift`
compiles alongside `hello-world.swift` as one module. `@main` provides the
entry point while letting the driver keep its `hello-world.swift` name.

### Callbacks are bare C function pointers

`onClick` and `layout` are ordinary top-level Swift funcs whose signatures are
ABI-identical to the C typedefs `AzButtonOnClickCallbackType` and
`AzLayoutCallbackType`. Because they capture no context, Swift converts a
reference to them into a `@convention(c)` C function pointer automatically —
you pass the function *itself*:

```swift
AzButton_setOnClick(&button, dataClone, onClick)
var window = AzWindowCreateOptions_create(layout)
```

The C header declares `AzButton_setOnClick`'s third parameter as the **bare fn
pointer** typedef (not an `AzButtonOnClickCallback` struct), so there is no
host-invoker, no closure allocation, and no hidden registry: the framework
stores your pointer and calls straight back into your Swift code on the UI
thread.

Helper functions the callbacks call (`myDataDowncast`, `azString`) are plain
top-level funcs — fine to reference from a `@convention(c)` context as long as
nothing local is captured.

### How RefAny works in Swift

`RefAny` is Azul's type-erased, reference-counted box for your application
state. The example hand-rolls the same three pieces the C `AZ_REFLECT` macro
generates:

- **Type identity** — `myDataTypeId` is the address of a one-byte heap
  allocation made once at startup. It is process-unique and stable, so
  `AzRefAny_isType` can verify a downcast at run time. (A global's storage
  works too; a dedicated allocation avoids any doubt about Swift global-address
  stability.)
- **Upcast** — `AzRefAny_newC` *copies* `MemoryLayout<MyDataModel>.size` bytes
  into a refcounted heap allocation, so pointing it at a stack local via
  `withUnsafePointer` is fine; `run_destructor: false` tells libazul not to
  free the caller's pointer.
- **Downcast** — `AzRefAny_isType` + `AzRefAny_getDataPtr` recover a typed
  `UnsafeMutablePointer<MyDataModel>`; both callbacks bail out (`nil` /
  `createBody()`) when the check fails.

`AzRefAny_clone(&d)` bumps the (atomic) reference count — it does not deep-copy
your struct. On click the framework matches the hit-test, calls `onClick` with
the stored `RefAny`, your code downcasts and increments `counter`, returns
`AzUpdate_RefreshDom`, and the framework re-runs `layout`, which reads the new
value.

Two more things worth noticing:

- **Strings** — `AzString_fromUtf8(ptr, len)` copies the bytes into a
  refcounted heap buffer, so building an `AzString` from a temporary
  `Array(s.utf8)` buffer is safe: the `AzString` outlives the buffer.
- **Typed CSS** — instead of parsing a CSS string, the example builds the
  property programmatically: `AzStyleFontSize_px(32.0)` →
  `AzCssProperty_fontSize` → `AzCssPropertyWithConditions_simple` →
  `AzDom_addCssProperty`.

### Why a Clang module (not pure Swift)?

Odin, Zig and Go can each redeclare every `AzFoo` record in-language because
they guarantee a C-compatible struct layout. Swift deliberately does not — the
compiler may reorder or pad the fields of a native `struct`, and there is no
Swift spelling for a `repr(C)` tagged union. Importing `azul.h` through
`module.modulemap` gives Swift the exact C layout for every type, which is the
only sound option and is exactly what makes passing structs by value across the
FFI boundary correct.

## Build and run

```sh
# linux
swiftc -I. hello-world.swift azul.swift -L. -lazul -o hello-world
LD_LIBRARY_PATH=. ./hello-world

# macos (framework flags matter — see Common errors)
swiftc -I. hello-world.swift azul.swift -L. -lazul \
  -framework Foundation -framework AppKit -framework OpenGL \
  -framework CoreGraphics -framework CoreText \
  -o hello-world
DYLD_LIBRARY_PATH=. ./hello-world

# windows
swiftc -I. hello-world.swift azul.swift -L. -lazul -o hello-world.exe
hello-world.exe
```

You should see the window pictured on the
[hello-world landing page](../hello-world.md). Click the button: the counter
increments, `layout` re-runs, and the new value renders.

## Common errors

- **`no such module 'CAzul'`** — Swift cannot find `module.modulemap`. Keep
  `-I.` and make sure `module.modulemap` and `azul.h` sit in the directory you
  compile from.
- **`undefined symbol: _Az...` at link time** — the linker cannot find
  `libazul`. Keep `-L. -lazul` and make sure the native library sits in the
  current directory.
- **Runtime: `Library not loaded` / `cannot open shared object file`** — the
  binary embeds no rpath, so keep the `LD_LIBRARY_PATH=.` /
  `DYLD_LIBRARY_PATH=.` prefix from the install steps.
- **Undefined symbols mentioning AppKit/OpenGL on macOS** — add the system
  frameworks: `-framework Foundation -framework AppKit -framework OpenGL
  -framework CoreGraphics -framework CoreText`.
- **Counter does not update on click** — `onClick` returned
  `AzUpdate_DoNothing`, or the downcast failed. A failed downcast means the
  type-id did not match: it must come from the *same* `myDataTypeId` used in
  the upcast.

## Coming Up Next

- [Application Architecture](../architecture.md) — architecting a larger Azul application
- [Document Object Model](../dom.md) — the Dom tree: node types, hierarchy, and CSS
- [Hello World [Zig]](zig.md)
