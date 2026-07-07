---
slug: hello-world/node
title: Hello World [Node.js]
language: en
canonical_slug: hello-world/node
audience: external
maturity: wip
guide_order: 20
topic_only: false
prerequisites: [hello-world]
tracked_files:
  - api.json
  - examples/node/hello-world.js
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

# Hello World [Node.js]

## Introduction

The JavaScript binding loads the prebuilt `libazul` native library via
[koffi](https://koffi.dev/) on Node, `bun:ffi` on Bun, and `Deno.UnsafeCallback` on
Deno. **Node.js is the verified runtime**; the same `azul.js` also detects Bun and
Deno, but those paths are experimental (callback return values are not yet written
back to native memory there). You write ordinary JS: a plain object, a function per
callback, and the smart `createWithLayout` factory.

## Installation

You need **Node.js 16+** (or Bun / Deno), the **`koffi`** package, and the native
`libazul` library.

The name `azul` on npmjs.org belongs to an unrelated project, so azul ships from
its own index at azul.rs. The quickest install is the hosted tarball — it bundles
`libazul` for Linux/macOS/Windows and pulls in `koffi` automatically:

```sh
npm install https://azul.rs/ui/npm/azul-$VERSION.tgz
```

With that installed you `require('azul')`. To wire it up by hand instead:

```sh
npm install koffi
# download the native library into the working dir:
wget -O libazul.dylib https://azul.rs/ui/release/$VERSION/libazul.dylib   # macOS
wget -O libazul.so    https://azul.rs/ui/release/$VERSION/libazul.so      # linux
# windows: download https://azul.rs/ui/release/$VERSION/azul.dll
```

Then drop the generated `azul.js` binding next to your script:

```sh
wget https://azul.rs/ui/release/$VERSION/azul.js
```

## Simple "Counter" Example

```js
'use strict';

const azul = require('./azul.js');
const {
    App, AppConfig, Button, ButtonType, Dom,
    CssProperty, CssPropertyWithConditions, StyleFontSize,
    Update, WindowBackgroundMaterial, WindowCreateOptions, WindowDecorations,
    refanyCreate, refanyGet,
} = azul;

const model = { counter: 5 };

// Click callback. refanyGet recovers your object from the handle.
function onClick(dataPtr, _info) {
    const m = refanyGet(dataPtr);
    if (m == null) return Update.DoNothing;
    m.counter += 1;
    return Update.RefreshDom;
}

// Layout callback: f(data) -> Dom.
function layout(dataPtr, _info) {
    const m = refanyGet(dataPtr);
    if (m == null) return Dom.create_body();

    const label = Dom.create_div()
        .with_css_property(
            CssPropertyWithConditions.simple(
                CssProperty.font_size(StyleFontSize.px(32.0))))
        .with_child(Dom.create_text(String(m.counter)));

    const button = Button.create('Increase counter')
        .with_button_type(ButtonType.Primary)
        .on_click(model, onClick);

    return Dom.create_body()
        .with_child(label)
        .with_child(button.dom());
}

// Safety net: log koffi callback exceptions before they SIGABRT via libffi.
process.on('uncaughtException', (e) => {
    console.error('[azul] uncaught:', e && e.stack ? e.stack : e);
});

// Smart factory hides the host-invoker register + layout_callback splice;
// .with(opts) recursively assigns nested fields and converts JS strings.
const window = WindowCreateOptions.createWithLayout(layout).with({
    window_state: {
        title: 'Hello World',
        size: { dimensions: { width: 400.0, height: 300.0 } },
        flags: {
            decorations: WindowDecorations.NoTitleAutoInject,
            background_material: WindowBackgroundMaterial.Sidebar,
        },
    },
});

App.create(refanyCreate(model), AppConfig.create()).run(window);
```

Four things to notice.

- **`refanyCreate` / `refanyGet`** — wrap any JS object into a handle; the same object
  is handed back to callbacks. Guard with `if (m == null)`.
- **Callbacks are plain functions** `(dataPtr, info) => ...` returning `Update.*` (or a
  `Dom` for layout). The `createWithLayout` factory registers them for you.
- **Enums and helpers are top-level** on the module: `Update.RefreshDom`,
  `ButtonType.Primary`. For `Option`/`Result`/`Vec`, use the module helpers
  (`azul.optionToNullable`, `azul.resultUnwrap`) — koffi unions carry no methods.
- **Keep the `uncaughtException` handler** — it logs exceptions thrown inside koffi
  callbacks before libffi can `SIGABRT` the process.

## Build and run

```sh
node hello-world.js
# or:
bun  run hello-world.js
deno run --allow-ffi --unstable-ffi hello-world.js
```

`azul.js` looks for the native library (`libazul.dylib` / `libazul.so` /
`azul.dll`) in this order:

1. `$AZ_LIB` — explicit path to the library *file* (overrides everything),
2. the directory containing `azul.js` itself,
3. `$AZ_LIB_DIR` — directory containing the library,
4. the current working directory,
5. the system loader search path (`DYLD_LIBRARY_PATH` / `LD_LIBRARY_PATH` / `PATH`).

With the download steps above (library and `azul.js` in the same directory) it is
found automatically — no environment variables needed. You should see the window
pictured on the [hello-world landing page](../hello-world.md).

## Common errors

- **`Cannot find module './azul.js'`** — run from the directory containing `azul.js`,
  or fix the `require` path.
- **koffi fails to load `libazul`** — the native library isn't in any of the
  locations listed under "Build and run"; the simplest fix is to put
  `libazul.dylib` / `libazul.so` / `azul.dll` next to `azul.js`, or point
  `AZ_LIB_DIR` at the directory that contains it (or `AZ_LIB` at the file itself).
- **Process aborts on click with no stack** — a callback threw; the
  `uncaughtException` logger surfaces it. Counter not advancing usually means you
  returned `Update.DoNothing`.

## Coming Up Next

- [Application Architecture](../architecture.md) — architecting a larger Azul application
- [Document Object Model](../dom.md) — the Dom tree: node types, hierarchy, and CSS
- [Hello World [OCaml]](ocaml.md)
