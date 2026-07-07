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

You need the **.NET 10 SDK** and the native `libazul` library for your platform.
(The example project `examples/csharp/Hello.csproj` and the downloadable
`Azul.csproj` both target `net10.0`.)

azul isn't on nuget.org, but a self-hosted NuGet v3 feed lives at azul.rs and the
package bundles the native `libazul` for Linux/macOS/Windows under
`runtimes/<rid>/native` (.NET picks the right RID at runtime):

```sh
dotnet nuget add source https://azul.rs/ui/nuget/index.json --name azul
dotnet add package azul --version $VERSION
```

Or install the native library + binding by hand:

1. Download the native library for your OS from the
   [release page](https://azul.rs/ui/release/$VERSION) and
   keep it next to your binary (or on the loader path):

   ```sh
   # macOS (Apple Silicon; Intel: libazul.x86_64.dylib)
   wget -O libazul.dylib https://azul.rs/ui/release/$VERSION/libazul.dylib
   # linux
   wget -O libazul.so    https://azul.rs/ui/release/$VERSION/libazul.so
   # windows
   # download https://azul.rs/ui/release/$VERSION/azul.dll
   ```

2. Add the generated `Azul.cs` bindings to your project:

   ```sh
   wget https://azul.rs/ui/release/$VERSION/Azul.cs
   # optional project scaffold:
   wget https://azul.rs/ui/release/$VERSION/Azul.csproj
   ```

That's it for library discovery: the generated `Azul.cs` installs a
`DllImportResolver` that probes the app's base directory and the current
working directory for `libazul.dylib` / `libazul.so` / `azul.dll`, so keeping
the native library next to your project (step 1) is sufficient — no
`DYLD_LIBRARY_PATH` / `LD_LIBRARY_PATH` setup needed for `dotnet run`.

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
            if (m == null) return (int)Update.DoNothing;
            m.Counter += 1;
            return (int)Update.RefreshDom;
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
                .WithButtonType(ButtonType.Primary)
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
  click handler is `Func<IntPtr, IntPtr, int>` returning `(int)Update.*`.
- **`using var app`** disposes deterministically — `Dispose()` calls the C-side
  `delete`, so native memory is released when the `App` goes out of scope.

## Build and run

```sh
# all platforms — the native library sits next to the project (step 1),
# and the generated DllImportResolver finds it there
dotnet run
```

You should see the window pictured on the [hello-world landing page](../hello-world.md).
Click the button: the counter increments, the layout callback re-runs, and the new
value renders.

## Common errors

- **`DllNotFoundException` / `Unable to load shared library 'azul'`** — the native
  library wasn't found. Put `libazul.dylib` / `libazul.so` / `azul.dll` in the
  project directory (or next to the published executable); the generated resolver
  probes both. `DYLD_LIBRARY_PATH` / `LD_LIBRARY_PATH` also work as a fallback for
  non-standard layouts, but note macOS strips `DYLD_*` in some launch paths (SIP).
- **Counter does not advance** — `OnClick` returned `(int)Update.DoNothing`. Return
  `(int)Update.RefreshDom` after mutating.
- **`RefanyGet(...) as MyDataModel` is null** — the handle holds a different type, or
  it is borrowed elsewhere. Return `Dom.CreateBody()` / `Update.DoNothing`.

## Coming Up Next

- [Application Architecture](../architecture.md) — architecting a larger Azul application
- [Document Object Model](../dom.md) — the Dom tree: node types, hierarchy, and CSS
- [Hello World [Java]](java.md)
