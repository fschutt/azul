---
slug: hello-world/d
title: Hello World [D]
language: en
canonical_slug: hello-world/d
audience: external
maturity: wip
guide_order: 29
topic_only: false
prerequisites: [hello-world]
tracked_files:
  - api.json
  - examples/d/hello-world.d
default-search-keys:
  - App
  - AppConfig
  - Dom
  - Button
  - RefAny
  - WindowCreateOptions
  - Update
---

# Hello World [D]

## Introduction

D talks to Azul through a plain C-ABI binding. The generated `azul.d`
translates the whole FFI surface explicitly: every `AzString` / `AzDom`
becomes a D `struct` (C layout is D's default aggregate layout), every
enum a D `enum` with an explicit backing integer, every tagged union a D
`union` of per-variant structs, and every exported symbol is declared
inside an `extern(C) { ... }` block.

Because a D function declared `extern(C)` **is** a real C function
pointer, callbacks are passed to Azul directly — like Zig, Go, C and
Odin, D needs neither a host-invoker trampoline nor a wrapper-struct
dance. You pass the function's address.

You need a recent **D** compiler — `dmd` or `ldc2`. The generated
`azul.d` declares `module azul;`, so the `hello-world.d` driver imports it
with `import azul;` and compiles both files together.

This binding is **experimental** and CI-validated: it compiles and the
end-to-end test drives the counter from 5 → 6 → 8.

## Installation

There is no package-manager (dub) story for D yet — you download the
native library, the generated binding, and the hello-world driver, then
compile the two `.d` files together:

```sh
# linux
curl -O https://azul.rs/ui/release/$VERSION/libazul.so
curl -O https://azul.rs/ui/release/$VERSION/azul.d
curl -O https://azul.rs/ui/release/$VERSION/hello-world.d
dmd hello-world.d azul.d -L-L. -L-lazul -of=hello-world
LD_LIBRARY_PATH=. ./hello-world
```

```sh
# macos
curl -O https://azul.rs/ui/release/$VERSION/libazul.dylib
curl -O https://azul.rs/ui/release/$VERSION/azul.d
curl -O https://azul.rs/ui/release/$VERSION/hello-world.d
dmd hello-world.d azul.d -L-L. -L-lazul \
    -L-framework -LFoundation -L-framework -LAppKit \
    -L-framework -LOpenGL -L-framework -LCoreGraphics -L-framework -LCoreText \
    -of=hello-world
DYLD_LIBRARY_PATH=. ./hello-world
```

```sh
# windows
curl -O https://azul.rs/ui/release/$VERSION/azul.dll
curl -O https://azul.rs/ui/release/$VERSION/azul.d
curl -O https://azul.rs/ui/release/$VERSION/hello-world.d
dmd hello-world.d azul.d -L/LIBPATH:. azul.lib -of=hello-world.exe
hello-world.exe
```

`-L-L.` forwards `-L.` to the linker (search the current directory) and
`-L-lazul` forwards `-lazul`. The `LD_LIBRARY_PATH=.` /
`DYLD_LIBRARY_PATH=.` prefix is needed at run time because the binary
embeds no rpath — the dynamic loader has to be told where the library
lives.

## Simple "Counter" Example

This is the exact `hello-world.d` shipped in the release (the same file
the end-to-end test builds and clicks through). It uses the raw `Az*`
symbols; `azul.d` also emits idiomatic aliases without the `Az` prefix
(e.g. `App_create`), which are the raw functions under a shorter name.

```d
import azul;

struct MyDataModel {
    uint counter;
}

__gshared ubyte MY_DATA_TYPE_TOKEN = 0;

ulong my_data_type_id() {
    return cast(ulong) cast(size_t) &MY_DATA_TYPE_TOKEN;
}

extern(C) void my_data_destructor(void* ptr) {
}

AzRefAny my_data_upcast(MyDataModel model) {
    MyDataModel local = model;
    string type_name_bytes = "MyDataModel";
    AzString type_name = AzString_fromUtf8(
        cast(ubyte*) type_name_bytes.ptr, type_name_bytes.length);
    AzGlVoidPtrConst ptr_wrapper;
    ptr_wrapper.ptr = &local;
    ptr_wrapper.run_destructor = false;
    return AzRefAny_newC(
        ptr_wrapper,
        MyDataModel.sizeof,
        MyDataModel.alignof,
        my_data_type_id(),
        type_name,
        &my_data_destructor,
        0, 0,
    );
}

MyDataModel* my_data_downcast(AzRefAny* refany) {
    if (!AzRefAny_isType(refany, my_data_type_id())) {
        return null;
    }
    void* ptr = AzRefAny_getDataPtr(refany);
    if (ptr is null) { return null; }
    return cast(MyDataModel*) ptr;
}

extern(C) AzUpdate on_click(AzRefAny data, AzCallbackInfo info) {
    AzRefAny d = data;
    MyDataModel* m = my_data_downcast(&d);
    if (m is null) { return AzUpdate.DoNothing; }
    m.counter += 1;
    return AzUpdate.RefreshDom;
}

extern(C) AzDom layout(AzRefAny data, AzLayoutCallbackInfo info) {
    AzRefAny d = data;
    MyDataModel* m = my_data_downcast(&d);
    if (m is null) { return AzDom_createBody(); }

    ubyte[16] buf;
    size_t n = u32_write(m.counter, buf[]);
    AzString counter_str = AzString_fromUtf8(buf.ptr, n);
    AzDom label = AzDom_createText(counter_str);

    AzDom label_wrapper = AzDom_createDiv();
    AzStyleFontSize font_size = AzStyleFontSize_px(32.0);
    AzCssProperty css_prop = AzCssProperty_fontSize(font_size);
    AzCssPropertyWithConditions cond = AzCssPropertyWithConditions_simple(css_prop);
    AzDom_addCssProperty(&label_wrapper, cond);
    AzDom_addChild(&label_wrapper, label);

    string btn_label_bytes = "Increase counter";
    AzString btn_label = AzString_fromUtf8(
        cast(ubyte*) btn_label_bytes.ptr, btn_label_bytes.length);
    AzButton button = AzButton_create(btn_label);
    AzButton_setButtonType(&button, AzButtonType.Primary);
    AzRefAny data_clone = AzRefAny_clone(&d);
    AzButton_setOnClick(&button, data_clone, &on_click);
    AzDom button_dom = AzButton_dom(button);

    AzDom root_body = AzDom_createBody();
    AzDom_addChild(&root_body, label_wrapper);
    AzDom_addChild(&root_body, button_dom);
    return root_body;
}

void main() {
    MyDataModel model;
    model.counter = 5;
    AzRefAny data = my_data_upcast(model);

    AzWindowCreateOptions window = AzWindowCreateOptions_create(&layout);
    string title_bytes = "Hello World";
    window.window_state.title = AzString_fromUtf8(
        cast(ubyte*) title_bytes.ptr, title_bytes.length);
    window.window_state.size.dimensions.width = 400.0;
    window.window_state.size.dimensions.height = 300.0;
    window.window_state.flags.decorations = AzWindowDecorations.NoTitleAutoInject;
    window.window_state.flags.background_material = AzWindowBackgroundMaterial.Sidebar;

    AzApp app = AzApp_create(data, AzAppConfig_create());
    AzApp_run(&app, window);
}
```

(The `u32_write` helper — an integer-to-decimal formatter — is elided
above; see the shipped example for the full ~15 lines. It avoids pulling
in Phobos so the `extern(C)` `layout` stays a plain leaf function.)

### Callbacks are bare C function pointers

`on_click` and `layout` are declared `extern(C)`, which makes them
ABI-identical to the C typedefs `AzButtonOnClickCallbackType` and
`AzLayoutCallbackType`. You pass the function's *address*:

```d
AzButton_setOnClick(&button, data_clone, &on_click);
AzWindowCreateOptions window = AzWindowCreateOptions_create(&layout);
```

The typed `AzButton_setOnClick` takes the **bare fn pointer**, not an
`AzButtonOnClickCallback` struct — `azul.d` binds the raw C variant whose
argument is the `extern(C)` fn-pointer typedef. There is no host-invoker,
no closure allocation, and no hidden registry: the framework stores your
pointer and calls straight back into your D code on the UI thread.

### How RefAny works in D

`RefAny` is Azul's type-erased, reference-counted box for your
application state. The example hand-rolls the same three pieces the C
`AZ_REFLECT` macro generates:

- **Type identity** — `my_data_type_id()` returns the address of a module
  global (`&MY_DATA_TYPE_TOKEN`, marked `__gshared` so there is a single
  process-wide instance, not one per thread). It is process-unique and
  stable, so `AzRefAny_isType` can verify a downcast at run time.
- **Upcast** — `AzRefAny_newC` *copies* `MyDataModel.sizeof` bytes into a
  refcounted heap allocation, so pointing it at a stack local is fine;
  `run_destructor = false` tells libazul not to free the caller's
  pointer.
- **Downcast** — `AzRefAny_isType` + `AzRefAny_getDataPtr` recover a typed
  `MyDataModel*`; both callbacks bail out (`return null` / `createBody()`)
  when the check fails.

`AzRefAny_clone(&d)` bumps the (atomic) reference count — it does not
deep-copy your struct. On click the framework matches the hit-test, calls
`on_click` with the stored `RefAny`, your code downcasts and increments
`counter`, returns `AzUpdate.RefreshDom`, and the framework re-runs
`layout`, which reads the new value.

Two more things worth noticing:

- **Strings** — `AzString_fromUtf8(ptr, len)` copies the bytes into a
  refcounted heap buffer, so passing a stack `ubyte[16]` buffer through
  `buf.ptr` is safe: the `AzString` outlives your stack frame. For string
  literals, `"...".ptr` gives the pointer and `"...".length` the byte
  count (cast the pointer to `ubyte*`).
- **Typed CSS** — instead of parsing a CSS string, the example builds the
  property programmatically: `AzStyleFontSize_px(32.0)` →
  `AzCssProperty_fontSize` → `AzCssPropertyWithConditions_simple` →
  `AzDom_addCssProperty`.

## Build and run

```sh
# linux
dmd hello-world.d azul.d -L-L. -L-lazul -of=hello-world
LD_LIBRARY_PATH=. ./hello-world

# macos (framework flags matter — see Common errors)
dmd hello-world.d azul.d -L-L. -L-lazul \
    -L-framework -LFoundation -L-framework -LAppKit \
    -L-framework -LOpenGL -L-framework -LCoreGraphics -L-framework -LCoreText \
    -of=hello-world
DYLD_LIBRARY_PATH=. ./hello-world

# windows
dmd hello-world.d azul.d -L/LIBPATH:. azul.lib -of=hello-world.exe
hello-world.exe
```

`dmd hello-world.d azul.d` compiles the driver together with the `azul`
module and resolves `import azul;`. You should see the window pictured on
the [hello-world landing page](../hello-world.md). Click the button: the
counter increments, `layout` re-runs, and the new value renders.

## Common errors

- **`module azul is in file 'azul.d' which cannot be read`** — the
  binding is not on the compile line or the import path. Pass `azul.d`
  explicitly next to `hello-world.d`, or add its directory with `-I`.
- **`undefined reference to Az...` at link time** — the linker cannot
  find `libazul`. Keep `-L-L.` / `-L-lazul` (Unix) or the `.lib` +
  `/LIBPATH:.` (Windows) and make sure the native library sits in the
  current directory.
- **Runtime: `cannot open shared object file` / `library not found`** —
  the binary embeds no rpath, so keep the `LD_LIBRARY_PATH=.` /
  `DYLD_LIBRARY_PATH=.` prefix from the install steps.
- **Undefined symbols mentioning AppKit/OpenGL on macOS** — add the
  system frameworks: `-L-framework -LFoundation -L-framework -LAppKit
  -L-framework -LOpenGL -L-framework -LCoreGraphics -L-framework
  -LCoreText`.
- **Counter does not update on click** — `on_click` returned
  `AzUpdate.DoNothing`, or the downcast failed. A failed downcast usually
  means the type-id does not match: it must come from the address of the
  *same* global token used in the upcast.

## Coming Up Next

- [Application Architecture](../architecture.md) — architecting a larger Azul application
- [Document Object Model](../dom.md) — the Dom tree: node types, hierarchy, and CSS
- [Hello World [Zig]](zig.md)
