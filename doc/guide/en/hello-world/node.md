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
Deno — the same `azul.js` covers all three runtimes. You write ordinary JS: a plain
object, a function per callback, and the smart `createWithLayout` factory.

## Installation

You need **Node.js 16+** (or Bun / Deno), the **`koffi`** package, and the native
`libazul` library.

### Recommended: npm

```sh
npm install azul
```

> ![NOTE]
> The 0.2.0 package is hosted on azul.rs. If the npm registry does not yet resolve
> it, install the tarball directly:
> ```sh
> npm install https://azul.rs/npm/azul-0.2.0.tgz
> ```

### Manual (works today)

```sh
npm install koffi
# download the native library from /releases into the working dir:
wget -O libazul.dylib https://azul.rs/release/0.2.0/libazul.dylib   # macOS
```

Drop the generated `azul.js` next to your script (it ships in the
[examples archive](/release/0.2.0/examples.zip) under `node/`).

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

The native library must be in the working directory or on `DYLD_LIBRARY_PATH` /
`LD_LIBRARY_PATH` / `PATH`. You should see the window pictured on the
[hello-world landing page](../hello-world.md).

## Common errors

- **`Cannot find module './azul.js'`** — run from the directory containing `azul.js`,
  or fix the `require` path.
- **koffi fails to load `libazul`** — the native library isn't discoverable; put it in
  the working dir or set the loader path.
- **Process aborts on click with no stack** — a callback threw; the
  `uncaughtException` logger surfaces it. Counter not advancing usually means you
  returned `Update.DoNothing`.

## Coming Up Next

- [Application Architecture](../architecture.md) — architecting a larger Azul application
- [Document Object Model](../dom.md) — the Dom tree: node types, hierarchy, and CSS
- [Hello World [OCaml]](ocaml.md)
