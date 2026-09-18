---
slug: hello-world/zig
title: Hello World [Zig]
language: en
canonical_slug: hello-world/zig
audience: external
maturity: mature
guide_order: 22
topic_only: false
prerequisites: [hello-world]
tracked_files:
  - api.json
  - examples/zig/hello-world.zig
last_generated_rev: 2660b0c45c9ea401ad6777a203f468755167e62e
generated_at: 2026-09-16T00:00:00Z
default-search-keys:
  - App
  - AppConfig
  - Dom
  - Button
---

# Hello world [Zig]

To use `libazul` from Zig (0.16+), you will need:

- The `libazul` native library from the release page
- The `azul.zig` and `azul_c.zip` wrapper layer, which gives you full "native" Zig experience 
  and hides the "C" layer beneath
- A minimal `build.zig` and the `hello-world.zig` for your project

```sh
curl -LO https://azul.rs/ui/release/$VERSION/azul-zig-$VERSION.tar.gz
tar xzf azul-zig-$VERSION.tar.gz # azul.zig, build.zig, hello-world.zig, azul.h

# linux
curl -O https://azul.rs/ui/release/$VERSION/libazul.so
LD_LIBRARY_PATH=. zig build run
# macos
curl -O https://azul.rs/ui/release/$VERSION/libazul.dylib
DYLD_LIBRARY_PATH=. zig build run
# windows
curl -O https://azul.rs/ui/release/$VERSION/azul.dll
zig build run
```

The `LD_LIBRARY_PATH=.` / `DYLD_LIBRARY_PATH=.` prefix is required
because the downloaded `build.zig` links `libazul` from the current
directory but does not embed an rpath — the dynamic loader has to be
told where the library lives at run time.

## Simple "Counter" Example

This is the exact `hello-world.zig` shipped in the release:

```zig
const std = @import("std");
const azul = @import("azul.zig");

const MyDataModel = struct {
    counter: u32,
};

// 1. Generate a strongly-typed reference wrapper for our model
const MyModelRef = azul.ReflectModel(MyDataModel).Ref;

// 2. Callbacks are pure Zig! They receive wrapper types directly.
fn onClick(model: MyModelRef, _: azul.CallbackInfo) azul.Update {
    const m = model.get();
    m.counter += 1;
    return .RefreshDom;
}

fn layout(model: MyModelRef, _: azul.LayoutCallbackInfo) azul.Dom {
    const m = model.get();

    var buf: [16]u8 = undefined;
    const slice = std.fmt.bufPrint(&buf, "{d}", .{m.counter}) catch return azul.Dom.createBody();
    
    // 3. Azul wrappers automatically convert Zig slices to C strings
    var label = azul.Dom.createPWithText(slice);
    label.setCss("font-size: 32px; margin: 0;");

    var button = azul.Button.create("Increase counter");
    button.setButtonType(azul.C.AzButtonType_Primary);
    
    // 4. We clone our strong reference to hand ownership to the button callback
    button.setOnClick(model.clone(), onClick);
    
    var body = azul.Dom.createBody();
    body.addChild(label.inner);
    body.addChild(button.dom().inner);
    return body;
}

pub fn main(init: std.process.Init) !void {
    _ = init; 
    
    // Create the model data
    const data = .{ .counter = 5 };

    var window = azul.WindowCreateOptions.create(layout);
    
    // Since we're accessing a raw C struct field, we use the C FFI string conversion
    window.inner.window_state.title = azul.C.AzString_fromUtf8("Hello World".ptr, 11);
    window.inner.window_state.size.dimensions.width = 400.0;
    window.inner.window_state.size.dimensions.height = 300.0;

    // App.create automatically handles upcasting our data to the Azul heap!
    var app = azul.App.create(data, azul.AppConfig.create());
    app.run(window.inner);
}
```

This snippet leverages advanced Zig features to erase boilerplate:

1. **`comptime` Type Reflection (`azul.ReflectModel`)**: By passing your state struct into the `comptime` wrapper, the bindings generate a smart-pointer `Ref` type for you.
2. **Native Slice Integration**: You can pass Zig string slices directly to wrapper functions like `Dom.createPWithText("string")`. The codegen leverages `anytype` behind the scenes to do zero-cost conversions to C FFI boundaries.
3. **Pure Zig Callbacks**: Instead of wrestling with raw `AzRefAny` pointers and downcasting inside every callback, `setOnClick` uses `comptime` introspection to generate C-ABI shims for your functions, letting you write perfectly typed callbacks with pure Zig inputs and outputs. You can return `.RefreshDom` or wrapper structs directly!

### Zig 0.16.0 Notes

If you are building your UI with Zig 0.16.0, keep these new language features in mind:

- **"Juicy Main" Dependency Injection**: `main` now accepts `(init: std.process.Init)`. Rather than explicitly spinning up a `std.heap.GeneralPurposeAllocator`, `init` acts as a context object granting you boilerplate-free access to `init.gpa`. While Azul utilizes its own internal allocator for the C-layer `RefAny`, if your `MyDataModel` expands to include its own dynamically sized arrays (like `std.ArrayList`), pass `init.gpa` in.
- **Buffered I/O ("Writergate")**: Zig recently overhauled the standard library I/O (`std.io` -> `std.Io`) to enforce mandatory buffering. If you write debugging tools in your app that print via a writer, explicitly call `.flush()`, or output may not appear.
- **Enum Shorthand Formatting**: To debug enum returns like `AzUpdate` or `AzButtonType`, Zig 0.15.1+ includes a new `{t}` shorthand specifier. Passing `"{t}"` behaves identically to `@tagName(enum_val)`.

## Build and run

```sh
# macos
DYLD_LIBRARY_PATH=. zig build run
# linux
LD_LIBRARY_PATH=. zig build run
# windows
zig build run
```

If you prefer a single explicit command without `build.zig`, this is the invocation the end-to-end harness uses:

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

You should see the window pictured on the [hello-world landing page](../hello-world.md). Click the button: the counter should increment, the layout callback then re-runs, and the new value renders.

Congratulations - once you've got the hello-world example running, you've already mastered 80% of the framework. As you might have guessed, more complex UI and styling are only composing more Dom objects together and working with the various event filters. To make this more streamlined, you can now start reading about the [architecture patterns](../architecture.md) or explore what [methods the `Dom` has to offer](../dom.md). See you in the next tutorial!
