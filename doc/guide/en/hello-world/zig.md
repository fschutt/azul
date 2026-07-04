---
slug: hello-world/zig
title: Hello World [Zig]
language: en
canonical_slug: hello-world/zig
audience: external
maturity: wip
guide_order: 22
topic_only: false
prerequisites: [hello-world]
tracked_files:
  - api.json
  - examples/zig/hello-world.zig
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

# Hello World [Zig]

## Introduction

Zig talks to Azul through `@cImport`: the generated `azul.zig` includes the
C header (`pub const C = @cImport(@cInclude("azul.h"))`) and every
`AzString` / `AzDom` / `AzApp_run` symbol comes out fully typed, with no
FFI shim in between. Because a Zig function declared with `callconv(.c)`
*is* a real C function pointer, callbacks are passed to Azul directly —
Zig is one of the few bindings that needs neither a host-invoker
trampoline nor a wrapper-struct dance for callbacks.

The counter example below uses the raw `azul.C.*` layer exclusively.
`azul.zig` also contains an idiomatic wrapper layer on top of the same
import; the raw layer is the path that is exercised end-to-end by the
test suite, so it is what this guide documents.

You need **Zig 0.14 or newer** (the example is tested against **0.16**;
the shipped `build.zig` uses the `Module` build API introduced in 0.13,
and lowercase `callconv(.c)` requires 0.14+).

## Installation

There is no package-manager story for Zig yet — you download the header,
the binding, the native library, and a minimal `build.zig` into one
directory and run `zig build run` there:

```sh
# linux
curl -O https://azul.rs/ui/release/$VERSION/libazul.so
curl -O https://azul.rs/ui/release/$VERSION/azul.h
curl -O https://azul.rs/ui/release/$VERSION/azul.zig
curl -O https://azul.rs/ui/release/$VERSION/build.zig
curl -O https://azul.rs/ui/release/$VERSION/hello-world.zig
LD_LIBRARY_PATH=. zig build run
```

```sh
# macos
curl -O https://azul.rs/ui/release/$VERSION/libazul.dylib
curl -O https://azul.rs/ui/release/$VERSION/azul.h
curl -O https://azul.rs/ui/release/$VERSION/azul.zig
curl -O https://azul.rs/ui/release/$VERSION/build.zig
curl -O https://azul.rs/ui/release/$VERSION/hello-world.zig
DYLD_LIBRARY_PATH=. zig build run
```

```sh
# windows
curl -O https://azul.rs/ui/release/$VERSION/azul.dll
curl -O https://azul.rs/ui/release/$VERSION/azul.h
curl -O https://azul.rs/ui/release/$VERSION/azul.zig
curl -O https://azul.rs/ui/release/$VERSION/build.zig
curl -O https://azul.rs/ui/release/$VERSION/hello-world.zig
zig build run
```

The `LD_LIBRARY_PATH=.` / `DYLD_LIBRARY_PATH=.` prefix is required
because the downloaded `build.zig` links `libazul` from the current
directory but does not embed an rpath — the dynamic loader has to be
told where the library lives at run time.

## Simple "Counter" Example

This is the exact `hello-world.zig` shipped in the release (the same
file the end-to-end test builds and clicks through):

```zig
// zig build run

const std = @import("std");
const azul = @import("azul.zig");
const C = azul.C;

// ── Data model ────────────────────────────────────────────────────────
//
// Mirrors the C macro `AZ_REFLECT_JSON(MyDataModel, ...)`:
//
//   1. A compile-time-unique type id (the address of a `var` we'll
//      never read or write).
//   2. An `upcast` that wraps the struct in an `AzRefAny`.
//   3. A `downcast` that recovers a typed pointer back from the
//      refany.
//
// Plain old data → empty destructor.

const MyDataModel = struct {
    counter: u32,
};

var MY_DATA_TYPE_TOKEN: u8 = 0;
fn myDataTypeId() u64 {
    return @intFromPtr(&MY_DATA_TYPE_TOKEN);
}

fn myDataDestructor(_: ?*anyopaque) callconv(.c) void {}

fn myDataUpcast(model: MyDataModel) C.AzRefAny {
    // `AzRefAny_newC` copies the bytes into its own heap allocation,
    // so handing it a stack pointer is fine. `run_destructor=false`
    // means libazul won't try to free the caller's pointer when it
    // copies — only the heap copy is freed (via myDataDestructor +
    // libazul's internal free) when the last clone drops.
    var local = model;
    const type_name_bytes = "MyDataModel";
    const type_name = C.AzString_fromUtf8(type_name_bytes.ptr, type_name_bytes.len);
    return C.AzRefAny_newC(
        .{ .ptr = @ptrCast(&local), .run_destructor = false },
        @sizeOf(MyDataModel),
        @alignOf(MyDataModel),
        myDataTypeId(),
        type_name,
        myDataDestructor,
        0, // no serialize_fn
        0, // no deserialize_fn
    );
}

fn myDataDowncast(refany: *const C.AzRefAny) ?*MyDataModel {
    if (!C.AzRefAny_isType(refany, myDataTypeId())) return null;
    const ptr = C.AzRefAny_getDataPtr(refany) orelse return null;
    return @constCast(@as(*const MyDataModel, @ptrCast(@alignCast(ptr))));
}

// ── Callback: button click ────────────────────────────────────────────

fn onClick(data: C.AzRefAny, _: C.AzCallbackInfo) callconv(.c) C.AzUpdate {
    var d = data;
    const m = myDataDowncast(&d) orelse return C.AzUpdate_DoNothing;
    m.counter += 1;
    return C.AzUpdate_RefreshDom;
}

// ── Layout callback ───────────────────────────────────────────────────

fn layout(data: C.AzRefAny, _: C.AzLayoutCallbackInfo) callconv(.c) C.AzDom {
    var d = data;
    const m = myDataDowncast(&d) orelse return C.AzDom_createBody();

    // Counter label (wrapped in a div so the font-size sticks).
    var buf: [16]u8 = undefined;
    const slice = std.fmt.bufPrint(&buf, "{d}", .{m.counter}) catch return C.AzDom_createBody();
    const counter_str = C.AzString_fromUtf8(slice.ptr, slice.len);
    const label = C.AzDom_createText(counter_str);

    var label_wrapper = C.AzDom_createDiv();
    const font_size = C.AzStyleFontSize_px(32.0);
    const css_prop = C.AzCssProperty_fontSize(font_size);
    const cond = C.AzCssPropertyWithConditions_simple(css_prop);
    C.AzDom_addCssProperty(&label_wrapper, cond);
    C.AzDom_addChild(&label_wrapper, label);

    // Increment button. The typed `AzButton_setOnClick` takes the bare
    // `AzButtonOnClickCallbackType` fn pointer directly — no AzCallback
    // struct wrapping needed since the typed-callback API change.
    const btn_label_bytes = "Increase counter";
    const btn_label = C.AzString_fromUtf8(btn_label_bytes.ptr, btn_label_bytes.len);
    var button = C.AzButton_create(btn_label);
    C.AzButton_setButtonType(&button, C.AzButtonType_Primary);
    const data_clone = C.AzRefAny_clone(&d);
    C.AzButton_setOnClick(&button, data_clone, onClick);
    const button_dom = C.AzButton_dom(button);

    // Body.
    var body = C.AzDom_createBody();
    C.AzDom_addChild(&body, label_wrapper);
    C.AzDom_addChild(&body, button_dom);
    return body;
}

// ── Main ──────────────────────────────────────────────────────────────

pub fn main() !void {
    const model = MyDataModel{ .counter = 5 };
    const data = myDataUpcast(model);

    var window = C.AzWindowCreateOptions_create(layout);
    const title_bytes = "Hello World";
    window.window_state.title = C.AzString_fromUtf8(title_bytes.ptr, title_bytes.len);
    window.window_state.size.dimensions.width = 400.0;
    window.window_state.size.dimensions.height = 300.0;

    // NoTitleAutoInject: OS draws close/min/max buttons; framework
    // auto-injects a Titlebar with drag support.
    window.window_state.flags.decorations = C.AzWindowDecorations_NoTitleAutoInject;
    window.window_state.flags.background_material = C.AzWindowBackgroundMaterial_Sidebar;

    var app = C.AzApp_create(data, C.AzAppConfig_create());
    C.AzApp_run(&app, window);
}
```

### Callbacks are bare C function pointers

`onClick` and `layout` are declared `callconv(.c)`, which makes them
ABI-identical to the C typedefs `AzButtonOnClickCallbackType` and
`AzLayoutCallbackType`. That means you pass the function *itself*:

```zig
C.AzButton_setOnClick(&button, data_clone, onClick);
var window = C.AzWindowCreateOptions_create(layout);
```

Note that the typed `AzButton_setOnClick` takes the **bare fn pointer**,
not an `AzCallback` struct. Older snippets that wrapped the pointer with
`AzCallback_create(...)` predate the typed-callback API change and no
longer compile — if you see a type error at the `setOnClick` call site,
delete the wrapping and pass the function directly. There is no
host-invoker, no closure allocation, and no hidden registry: the
framework stores your pointer and calls straight back into your Zig
code on the UI thread.

### How RefAny works in Zig

`RefAny` is Azul's type-erased, reference-counted box for your
application state — the C header ships an `AZ_REFLECT` macro for it, and
the Zig example hand-rolls the same three pieces in ~35 lines:

- **Type identity** — `myDataTypeId()` returns the address of a global
  `var`. Every Zig type you reflect gets its own token variable, so the
  address is process-unique and stable, and `AzRefAny_isType` can verify
  at run time that a downcast targets the right type.
- **Upcast** — `AzRefAny_newC` *copies* `@sizeOf(MyDataModel)` bytes
  into a refcounted heap allocation. Handing it a pointer to a stack
  local is therefore fine; `run_destructor = false` tells libazul not to
  free the caller's pointer (only the heap copy is destroyed, via your
  destructor, when the last clone drops).
- **Downcast** — `AzRefAny_isType` + `AzRefAny_getDataPtr` recover a
  typed `*MyDataModel`. Both callbacks bail out gracefully (`orelse
  return ...`) when the check fails.

`AzRefAny_clone(&d)` bumps the (atomic) reference count — it does not
deep-copy your struct. The clone's ownership moves into the button, so
the framework can hand the same data back to `onClick` later. Data flow
on click: framework matches the hit-test → calls `onClick` with the
stored `RefAny` → your code downcasts, increments `counter`, returns
`C.AzUpdate_RefreshDom` → the framework re-runs `layout`, which reads
the new value.

Two more things worth noticing:

- **Strings** — `AzString_fromUtf8(ptr, len)` copies the bytes into a
  refcounted heap buffer, which is why passing `std.fmt.bufPrint`
  output from a stack buffer is safe: the `AzString` outlives your
  stack frame.
- **Typed CSS** — instead of parsing a CSS string, the example builds
  the property programmatically: `AzStyleFontSize_px(32.0)` →
  `AzCssProperty_fontSize` → `AzCssPropertyWithConditions_simple` →
  `AzDom_addCssProperty`. (String CSS via `AzDom_setCss` works from Zig
  too, exactly as in the [C guide](c.md).)

## Build and run

```sh
# macos
DYLD_LIBRARY_PATH=. zig build run
# linux
LD_LIBRARY_PATH=. zig build run
# windows
zig build run
```

The `zig build run` flow uses the downloaded `build.zig`, which adds the
current directory to both the include path (for `azul.h`) and the
library path (for `libazul`). If you prefer a single explicit command
without `build.zig`, this is the invocation the end-to-end harness uses:

```sh
# linux
zig build-exe hello-world.zig -lc -lazul -L. -I. -rpath . -femit-bin=hello-world
LD_LIBRARY_PATH=. ./hello-world

# macos (framework flags matter — see Common errors)
zig build-exe hello-world.zig -lc -lazul -L. -I. -rpath . \
  -framework Foundation -framework AppKit -framework OpenGL \
  -framework CoreGraphics -framework CoreText -femit-bin=hello-world
DYLD_LIBRARY_PATH=. ./hello-world
```

You should see the window pictured on the
[hello-world landing page](../hello-world.md). Click the button: the
counter increments, `layout` re-runs, and the new value renders.

## Common errors

- **`error: C import failed`** — `azul.h` is not on the C include path.
  The downloaded `build.zig` adds `.` automatically; with a manual
  `zig build-exe` you must pass `-I.` yourself.
- **`build.zig` does not compile / unknown field errors** — your Zig is
  too old. The shipped `build.zig` uses the `Module` API (Zig 0.13+),
  the example's lowercase `callconv(.c)` needs 0.14+, and the release
  is tested against 0.16. Upgrade Zig rather than editing the manifest.
- **Type error at `AzButton_setOnClick`** — you passed an `AzCallback`
  struct (e.g. via `AzCallback_create`) where the typed setter expects
  the bare `AzButtonOnClickCallbackType` fn pointer. Pass `onClick`
  directly.
- **Runtime: `library not found` / `cannot open shared object file`** —
  the generated `build.zig` embeds no rpath, so keep the
  `DYLD_LIBRARY_PATH=.` / `LD_LIBRARY_PATH=.` prefix from the install
  steps (or add `-rpath .` when using `zig build-exe`).
- **Undefined symbols mentioning AppKit/OpenGL on macOS** — when the
  linker requires the system frameworks explicitly, add `-framework
  Foundation -framework AppKit -framework OpenGL -framework
  CoreGraphics -framework CoreText` (the flags the e2e harness links
  with).
- **Counter does not update on click** — `onClick` returned
  `AzUpdate_DoNothing`, or the downcast failed. A failed downcast
  usually means the type-id does not match: the id must come from the
  address of the *same* global token variable used in the upcast.

## Coming Up Next

- [Application Architecture](../architecture.md) — architecting a larger Azul application
- [Document Object Model](../dom.md) — the Dom tree: node types, hierarchy, and CSS
- [Hello World [Go]](go.md)
