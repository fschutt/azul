---
slug: hello-world/c
title: Hello, World — C
language: en
canonical_slug: hello-world/c
audience: external
maturity: wip
guide_order: 12
topic_only: false
prerequisites: [hello-world]
tracked_files:
  - api.json
  - core/src/callbacks.rs
  - core/src/lib.rs
  - dll/src/lib.rs
last_generated_rev: 2acdeae71299faed9a65b0dddeea8d53c350e9ac
generated_at: 2026-05-01T20:34:08Z
---

# Hello, World — C

> **WIP** — the C bindings are auto-generated from `api.json`. The header file `azul.h` is committed in the repository, but the surface area still shifts. Pin to a specific commit until the API is stable.

A complete Azul GUI in one C file. The example matches `examples/c/hello-world.c` in the repository and links against the dynamic library built from `dll/`.

## Get the library and header

Build the dynamic library once from a checkout of the repository:

```sh
cargo build -p azul-dll --release --no-default-features --features build-dll
```

The output lands at `target/release/libazul.so` (Linux), `libazul.dylib` (macOS), or `azul.dll` (Windows). The header file is at `target/codegen/azul.h` after running `azul-doc codegen all` once. Copy both somewhere your C compiler can find them.

## Imports and a small string helper

```c
#include "azul.h"
#include <stdio.h>
#include <string.h>

#define AZ_STR(s) AzString_copyFromBytes((const uint8_t*)(s), 0, strlen(s))
```

`AzString_copyFromBytes` builds an Azul string from a byte range. The macro is a convenience used throughout the example.

## Data model and reflection

```c
typedef struct { uint32_t counter; } MyDataModel;
void MyDataModel_destructor(void* m) { }

AzJson MyDataModel_toJson(AzRefAny refany);
AzResultRefAnyString MyDataModel_fromJson(AzJson json);
AZ_REFLECT_JSON(MyDataModel, MyDataModel_destructor,
                MyDataModel_toJson, MyDataModel_fromJson);
```

`AZ_REFLECT_JSON` is a macro that generates the boilerplate the C API needs to upcast your struct into a `RefAny` and downcast it back. It expands into:

- `MyDataModel_upcast(MyDataModel)` — wraps the struct in a `RefAny`.
- `MyDataModel_downcastRef(AzRefAny*, MyDataModelRef*)` — returns a const pointer.
- `MyDataModel_downcastMut(AzRefAny*, MyDataModelRefMut*)` — returns a mutable pointer.
- A type tag the framework uses to verify casts at runtime.

The destructor is called when the framework drops the last `RefAny` referencing your struct. Hello-world's struct contains no heap data, so the body is empty.

The JSON callbacks are not used in hello-world; they exist so that any `RefAny` can be serialized for state hot-reload. Stub implementations satisfy the reflection macro:

```c
AzJson MyDataModel_toJson(AzRefAny refany) {
    MyDataModelRef ref = MyDataModelRef_create(&refany);
    if (!MyDataModel_downcastRef(&refany, &ref)) {
        return AzJson_null();
    }
    int64_t counter = (int64_t)ref.ptr->counter;
    MyDataModelRef_delete(&ref);
    return AzJson_int(counter);
}

AzResultRefAnyString MyDataModel_fromJson(AzJson json) {
    AzOptionI64 counter_opt = AzJson_asInt(&json);
    if (counter_opt.None.tag == AzOptionI64_Tag_None) {
        return AzResultRefAnyString_err(AZ_STR("Expected integer"));
    }
    MyDataModel model = { .counter = (uint32_t)counter_opt.Some.payload };
    return AzResultRefAnyString_ok(MyDataModel_upcast(model));
}
```

## The click callback

```c
AzUpdate on_click(AzRefAny data, AzCallbackInfo info) {
    MyDataModelRefMut d = MyDataModelRefMut_create(&data);
    if (!MyDataModel_downcastMut(&data, &d)) {
        return AzUpdate_DoNothing;
    }
    d.ptr->counter += 1;
    MyDataModelRefMut_delete(&d);
    return AzUpdate_RefreshDom;
}
```

Three things to notice:

- The signature is fixed: `AzUpdate (*)(AzRefAny, AzCallbackInfo)` — the framework calls back through this pointer at FFI ABI.
- `MyDataModelRefMut_create` + `MyDataModel_downcastMut` together perform the runtime borrow check. A failed downcast (already borrowed elsewhere, or wrong type) returns `false` and you must return `AzUpdate_DoNothing`.
- `MyDataModelRefMut_delete(&d)` releases the borrow before the function returns. Forgetting this poisons the `RefAny` and the next downcast will fail.

## The layout callback

```c
AzDom layout(AzRefAny data, AzLayoutCallbackInfo info) {
    MyDataModelRef d = MyDataModelRef_create(&data);
    if (!MyDataModel_downcastRef(&data, &d)) {
        return AzDom_createBody();
    }

    char buffer[20];
    int written = snprintf(buffer, 20, "%d", d.ptr->counter);
    MyDataModelRef_delete(&d);

    AzString label_text = AzString_copyFromBytes(
        (const uint8_t*)buffer, 0, written);
    AzDom label = AzDom_createText(label_text);
    AzDom label_wrapper = AzDom_createDiv();
    AzDom_addCssProperty(&label_wrapper, AzCssPropertyWithConditions_simple(
        AzCssProperty_fontSize(AzStyleFontSize_px(32.0))
    ));
    AzDom_addChild(&label_wrapper, label);

    AzButton button = AzButton_create(AZ_STR("Increase counter"));
    AzButton_setButtonType(&button, AzButtonType_Primary);
    AzRefAny data_clone = AzRefAny_clone(&data);
    AzButton_setOnClick(&button, data_clone, on_click);
    AzDom button_dom = AzButton_dom(button);

    AzDom body = AzDom_createBody();
    AzDom_addChild(&body, label_wrapper);
    AzDom_addChild(&body, button_dom);

    return AzDom_style(body, AzCss_empty());
}
```

This mirrors the [Rust version](rust.md) one-for-one: read the counter, build a text node wrapped in a styled div, attach a click handler to a button, append both to the body, return.

`AzRefAny_clone` increments the reference count; the clone is moved into the button so the framework can call `on_click` later with that handle.

`AzCssPropertyWithConditions_simple` wraps a property without media-query conditions — the same as inline style in Rust.

## main

```c
int main() {
    MyDataModel model = { .counter = 5 };
    AzRefAny data = MyDataModel_upcast(model);

    AzWindowCreateOptions window = AzWindowCreateOptions_create(layout);
    window.window_state.title = AZ_STR("Hello World");
    window.window_state.size.dimensions.width = 400.0;
    window.window_state.size.dimensions.height = 300.0;

    window.window_state.flags.decorations = AzWindowDecorations_NoTitleAutoInject;
    window.window_state.flags.background_material = AzWindowBackgroundMaterial_Sidebar;

    AzApp app = AzApp_create(data, AzAppConfig_create());
    AzApp_run(&app, window);
    AzApp_delete(&app);
    return 0;
}
```

`AzWindowDecorations_NoTitleAutoInject` asks the OS to draw close/min/max buttons while the framework auto-injects a draggable titlebar. `AzWindowBackgroundMaterial_Sidebar` sets the platform-native sidebar material on macOS and a translucent fallback elsewhere.

`AzApp_run` blocks until the last window closes; `AzApp_delete` drops the framework instance.

## Compile and link

Linux:

```sh
cc hello-world.c -I/path/to/azul-headers \
   -L/path/to/azul-lib -lazul -ldl -lpthread -lm \
   -Wl,-rpath,/path/to/azul-lib \
   -o hello-world
./hello-world
```

macOS:

```sh
cc hello-world.c -I/path/to/azul-headers \
   -L/path/to/azul-lib -lazul \
   -Wl,-rpath,@executable_path/. \
   -o hello-world
./hello-world
```

Windows (MSVC):

```bat
cl hello-world.c /I path\to\azul-headers /link /LIBPATH:path\to\azul-lib azul.lib
hello-world.exe
```

## Common errors

- **Linker reports unresolved `Az*` symbols** — the dynamic library is not on the linker path. Check `-L` and `-l`.
- **Runtime: "library not found"** — the loader cannot find `libazul.{so,dylib,dll}`. On Linux export `LD_LIBRARY_PATH`; on macOS use `-Wl,-rpath,@executable_path/.`; on Windows place `azul.dll` next to the `.exe`.
- **Counter does not update on click** — the click handler returned `AzUpdate_DoNothing`, or the downcast failed silently. Add a `printf` to verify.

## Next

- [DOM and Callbacks](../dom.md) — building richer trees, `IdOrClass`, the full callback API. Same surface, just translate `Az*` prefixes.
- [C Bindings](../bindings/c.md) — reference for the full FFI surface.
