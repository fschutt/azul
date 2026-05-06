---
slug: styling/icon-packs
title: Icon Packs
language: en
canonical_slug: styling/icon-packs
audience: external
maturity: wip
guide_order: 44
topic_only: false
short_desc: Register icons and use them with `Dom::create_icon` or `<icon>`
prerequisites: [styling]
tracked_files:
  - core/src/icon.rs
  - layout/src/icon.rs
  - core/src/dom.rs
last_generated_rev: 7ecd570e4c0c3584e5107e770058c16cb59fa6e7
generated_at: 2026-05-02T12:00:00Z
---

# Icon Packs

> **WIP** — image and font icons resolve through the default resolver on
> all platforms; SVG and animated icons run through the same callback path
> but are not yet covered by built-in helpers.

An icon pack is a named bag of icons that the framework looks up by name
when it sees a `Dom::create_icon("home")` node (or an `<icon>` element).
Registration happens once at `AppConfig` time; the lookup runs before
every layout pass and resolves the name to a `StyledDom` subtree —
typically an `<img>` or a glyph in an icon font.

```rust
# use azul::prelude::*;
let mut config = AppConfig::default();

// 1. Register a font pack pointing at Material Icons.
let material = FontRef::from_file("fonts/MaterialIcons-Regular.ttf");
config.icons.register_icon(
    "material",
    "home",
    RefAny::new(FontIconData { font: material.clone(), icon_char: "\u{e88a}".into() }),
);
config.icons.register_icon(
    "material",
    "settings",
    RefAny::new(FontIconData { font: material, icon_char: "\u{e8b8}".into() }),
);

// 2. Register an image pack for app-specific icons.
config.icons.register_icon(
    "app",
    "logo",
    RefAny::new(ImageIconData { image: image_ref, width: 32.0, height: 32.0 }),
);

// 3. Use it in a Dom.
let dom = Dom::create_div().with_children(vec![
    Dom::create_icon("home".into()),
    Dom::create_icon("logo".into()),
]);
```

The resolution path is documented at `core/src/icon.rs:5`.

## How lookup works

Icons are stored in a nested map (`pack_name → (icon_name → RefAny)`)
inside `IconProviderHandle` (`core/src/icon.rs:130`). Lookup walks the
packs in registration order and takes the first match — pack names mostly
exist for namespacing and bulk unregistration, not for selection.

| Method on `IconProviderHandle` | Effect |
|---|---|
| `register_icon(pack, name, data)` | Add or overwrite an icon |
| `unregister_icon(pack, name)` | Remove a single icon |
| `unregister_pack(pack)` | Remove every icon in a pack |
| `set_resolver(callback)` | Replace the resolver for the whole provider |

Icon names are case-insensitive: `register_icon("app", "Home", ...)` and
`Dom::create_icon("home")` resolve to the same entry.

## The resolver callback

The resolver turns the registered `RefAny` plus the original icon node
into a `StyledDom`:

```rust,ignore
pub type IconResolverCallbackType = extern "C" fn(
    icon_data: OptionRefAny,         // the RefAny you registered, or None
    original_icon_dom: &StyledDom,   // the <icon> node with its inline styles + a11y
    system_style: &SystemStyle,      // current theme, accent, accessibility flags
) -> StyledDom;
```

The default resolver (`layout/src/icon.rs:91`) handles two `RefAny` types
out of the box:

- `ImageIconData { image, width, height }` — renders as an `<img>`.
- `FontIconData { font, icon_char }` — renders as a single-glyph text run.

For anything else (SVG, animated, vector) write your own resolver and
pass it to `IconProviderHandle::with_resolver(my_callback)`. The callback
sees `system_style`, so you can produce a different DOM for dark mode, a
high-contrast variant, or a reduced-motion fallback.

## System-style integration

The default resolver copies a curated subset of CSS properties from the
original `<icon>` node onto the resolved DOM and filters based on
`SystemStyle`:

- `prefer_grayscale` → drops explicit colour and adds a grayscale
  `<filter>` to image icons.
- `prefers_contrast` → preserves border and outline properties so the
  icon stays visible against a high-contrast background.
- `accent` and `text` colours from `SystemColors` apply when the icon's
  CSS uses `color: system:accent` or `color: system:text`.

The cascade still runs as normal — a `with_css("color: red;")` on the
`<icon>` node beats the system style.

## Built-in helpers

For typical packs, the layout crate exposes the two `RefAny` marker
types and a default-resolver function:

```rust,ignore
use azul_core::icon::IconProviderHandle;
use azul_layout::icon::{default_icon_resolver, FontIconData, ImageIconData};

let mut provider = IconProviderHandle::with_resolver(default_icon_resolver);
provider.register_icon("material", "home", RefAny::new(FontIconData {
    font: material_font.clone(),
    icon_char: "\u{e88a}".into(),
}));
```

`default_icon_resolver` is also what runs when you don't call
`set_resolver` — registering through `AppConfig::icons.register_icon` is
enough.

## Naming conventions

A pack is identified by its name string. The framework reserves no names,
but the convention is:

- `app` — your application's first-party icons.
- `material`, `phosphor`, `lucide`, ... — third-party icon fonts.
- `os` — anything you load from a platform icon theme (Windows shell
  imageres.dll, macOS NSImage, GNOME `icon-theme.cache`).

When two packs ship the same icon name, the *first registered* wins.
Register your `app` pack last if you want app icons to override third-party
ones.

## Recipes

### Toolbar with mixed packs

```azul-render screenshot=icons-toolbar width=400 height=120 subtitle="A toolbar mixing app and material icons"
<body style="font-family: sans-serif; padding: 16px;">
  <div style="display: flex; gap: 12px; padding: 8px; background: #f3f4f6;">
    <span style="display: inline-block; width: 24px; height: 24px; background: #1976d2;"></span>
    <span style="display: inline-block; width: 24px; height: 24px; background: #1976d2;"></span>
    <span style="display: inline-block; width: 24px; height: 24px; background: #1976d2;"></span>
  </div>
</body>
```

### Themed icon button

```rust,no_run
# use azul::prelude::*;
let _ = Dom::create_button("Settings", SmallAriaInfo::label("Settings"))
    .with_children(vec![Dom::create_icon("settings".into())])
    .with_css("
        display: inline-flex; gap: 6px; padding: 6px 12px;
        background: system:button-face;
        color: system:button-text;
        @theme dark { color: system:accent; }
    ");
```

The `@theme dark` rule changes the `color` resolved value, which the
default font-icon resolver picks up when it copies the icon's
`StyleTextColor`.

## Disabling and overriding

Three escape hatches:

- `IconProviderHandle::set_resolver(custom)` swaps the whole resolver
  for one provider.
- `IconProviderHandle::unregister_pack("material")` removes every icon
  in a pack — useful for "skin packs" you load and unload at runtime.
- Setting `AppConfig::icons` to a fresh `IconProviderHandle::default()`
  resets the entire provider to the empty default state.

End-user ricing of icons (replacing `material/home` with a user-chosen
SVG without recompiling) is on the road map alongside the existing
`AZUL_DISABLE_RICING` CSS hook described in [System Themes](themes.md#application-specific-overrides).

## Where to read the source

- `core/src/icon.rs:79` — `IconResolverCallbackType` (the callback contract)
- `core/src/icon.rs:130` — `IconProviderHandle` (registration API)
- `core/src/icon.rs:200` — `register_icon`
- `layout/src/icon.rs:59` — `ImageIconData`
- `layout/src/icon.rs:71` — `FontIconData`
- `layout/src/icon.rs:91` — `default_icon_resolver`
- `core/src/icon.rs:446` — `resolve_icons_in_styled_dom` (the per-frame replacement pass)
