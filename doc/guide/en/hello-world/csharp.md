---
slug: hello-world/csharp
title: Hello World [C#]
language: en
canonical_slug: hello-world/csharp
audience: external
maturity: wip
guide_order: 15
topic_only: false
prerequisites: [hello-world]
tracked_files:
  - api.json
  - examples/csharp/hello-world.cs
last_generated_rev: 39416ebc681c6423bfdefa94dc996f613184ea0b
generated_at: 2026-05-29T00:00:00Z
default-search-keys:
  - App
  - AppConfig
  - Dom
  - Button
  - WindowCreateOptions
  - Update
---

# Hello World [C#]

## Introduction

The C# binding talks to the prebuilt `libazul` native library through P/Invoke, but
you almost never see that layer. You write idiomatic C# — plain classes, the
wrapper-class `App.Create(...).Run(wco)` path, and a typed layout delegate that
returns a `Dom` — and the generated `Azul.cs` handles the marshalling. No
`Marshal.AllocHGlobal`, no `.Raw` extraction, no `IntPtr` ceremony in your code.

## Installation

You need **.NET 8+** and the native `libazul` library for your platform.

### Recommended: NuGet package

```sh
dotnet add package Azul
```

> [!NOTE]
> The 0.2.0 NuGet feed is hosted on azul.rs. If `nuget.org` does not yet resolve
> the package, add the azul.rs source first:
> ```sh
> dotnet nuget add source https://azul.rs/nuget/index.json -n azul
> ```
> If a package is not yet published for your platform, use the manual route below.

### Manual

1. Download the native library for your OS from the [/releases](/releases) page and
   keep it next to your binary (or on the loader path):

   ```sh
   # macOS
   wget -O libazul.dylib https://azul.rs/release/0.2.0/libazul.dylib
   # linux
   wget -O libazul.so    https://azul.rs/release/0.2.0/libazul.so
   # windows
   # download https://azul.rs/release/0.2.0/azul.dll
   ```

2. Add the generated `Azul.cs` bindings to your project (ships in the
   [examples archive](/ui/release/0.2.0/examples.zip) under `csharp/`).

The native library must be discoverable at runtime via `DYLD_LIBRARY_PATH` (macOS),
`LD_LIBRARY_PATH` (Linux), or `PATH` (Windows).

## Simple "Counter" Example

```csharp
using System;
using Azul;

namespace HelloWorld
{
    // Plain C# class - the "single source of truth" for app state.
    public sealed class MyDataModel
    {
        public uint Counter;
        public MyDataModel(uint counter) { Counter = counter; }
    }

    public static class Program
    {
        private static readonly MyDataModel _model = new MyDataModel(5);

        // Click callback: returns an Update as an int. RefanyGet recovers
        // your object from the type-erased handle; `as T` is null on mismatch.
        private static int OnClick(IntPtr dataPtr, IntPtr infoPtr)
        {
            var m = HostInvoker.RefanyGet(dataPtr) as MyDataModel;
            if (m == null) return (int)AzUpdate.DoNothing;
            m.Counter += 1;
            return (int)AzUpdate.RefreshDom;
        }

        // Layout callback: f(data) -> Dom. Runs on startup and again after any
        // callback that returns Update.RefreshDom.
        private static Dom Layout(IntPtr dataPtr, IntPtr infoPtr)
        {
            var m = HostInvoker.RefanyGet(dataPtr) as MyDataModel;
            if (m == null) return Dom.CreateBody();

            var label = Dom.CreateDiv()
                .WithCss("font-size: 32px;")
                .WithChild(Dom.CreateText(m.Counter.ToString()));

            var buttonDom = Button.Create("Increase counter")
                .WithButtonType(AzButtonType.Primary)
                .OnClick(m, new Func<IntPtr, IntPtr, int>(OnClick))
                .Dom();

            return Dom.CreateBody()
                .WithChild(label)
                .WithChild(buttonDom);
        }

        public static int Main(string[] args)
        {
            // `using` disposes the App (and calls the C-side delete) on exit.
            using var app = App.Create(HostInvoker.RefanyWrap(_model), AppConfig.Create());
            app.Run(WindowCreateOptions.Create(new Func<IntPtr, IntPtr, Dom>(Layout)));
            return 0;
        }
    }
}
```

Four things to notice.

- **`HostInvoker.RefanyWrap` / `RefanyGet`** — your `MyDataModel` is wrapped into a
  type-erased handle when you hand it to `App.Create`, and the *same* instance is
  handed back to every callback. `RefanyGet(ptr) as MyDataModel` is the runtime
  cast; it returns `null` on a type mismatch, so return `Update.DoNothing` /
  `Dom.CreateBody()` in that case.
- **Wrapper-class API, no IntPtr ceremony.** `App.Create(...).Run(...)`,
  `Dom.CreateBody().WithChild(...)`, and
  `Button.Create(label).WithButtonType(...).OnClick(...).Dom()` read like normal
  fluent C#. The `WithCss("...")` builder accepts any CSS string, including
  `:hover { }` / `@media` / `@os(...)` inline queries.
- **Callbacks are delegates.** A layout callback is `Func<IntPtr, IntPtr, Dom>`; a
  click handler is `Func<IntPtr, IntPtr, int>` returning `(int)AzUpdate.*`.
- **`using var app`** disposes deterministically — `Dispose()` calls the C-side
  `delete`, so native memory is released when the `App` goes out of scope.

## Build and run

```sh
# macOS
DYLD_LIBRARY_PATH=. dotnet run
# linux
LD_LIBRARY_PATH=. dotnet run
# windows (azul.dll on PATH or in the working dir)
dotnet run
```

You should see the window pictured on the [hello-world landing page](../hello-world.md).
Click the button: the counter increments, the layout callback re-runs, and the new
value renders.

## Common errors

- **`DllNotFoundException` / `Unable to load shared library 'azul'`** — the native
  library isn't on the loader path. Set `DYLD_LIBRARY_PATH` / `LD_LIBRARY_PATH`, or
  put `azul.dll` next to the executable on Windows.
- **Counter does not advance** — `OnClick` returned `(int)AzUpdate.DoNothing`. Return
  `(int)AzUpdate.RefreshDom` after mutating.
- **`RefanyGet(...) as MyDataModel` is null** — the handle holds a different type, or
  it is borrowed elsewhere. Return `Dom.CreateBody()` / `AzUpdate.DoNothing`.

## Coming Up Next

- [Application Architecture](../architecture.md) — architecting a larger Azul application
- [Document Object Model](../dom.md) — the Dom tree: node types, hierarchy, and CSS
- [Hello World [Java]](java.md)
