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

## Introduction

*WIP.* Image and font icons resolve through the default resolver on all platforms; SVG and animated icons run through the same callback path but are not yet covered by built-in helpers.

An icon pack is a named bag of icons that the framework looks up by name
when it sees a `Dom::create_icon("home")` node (or an `<icon>` element).
Registration happens once on `AppConfig.icon_provider`; the lookup runs
before every layout pass and resolves the name to a `StyledDom` subtree
(typically an `<img>` or a glyph in an icon font).

```rust,ignore
use azul::prelude::*;
let mut config = AppConfig::create(/* ... */);

// 1. Register a font-icon pack pointing at Material Icons.
let material = /* a FontRef built once at startup */;
config.icon_provider.register_font_icon(
    "material".into(),
    "home".into(),
    material.clone(),
    "\u{e88a}".into(),
);
config.icon_provider.register_font_icon(
    "material".into(),
    "settings".into(),
    material,
    "\u{e8b8}".into(),
);

// 2. Register an image-icon pack for app-specific icons.
config.icon_provider.register_image_icon(
    "app".into(),
    "logo".into(),
    image_ref,
);

// 3. Use it in a Dom.
let dom = Dom::create_div().with_children(vec![
    Dom::create_icon("home".into()),
    Dom::create_icon("logo".into()),
].into());
```

## How lookup works

Icons are stored on `IconProviderHandle` as a nested map of pack to
icon-name to data. Lookup walks the packs in registration order and takes
the first match. Pack names mostly exist for namespacing and bulk
unregistration, not for selection.

Methods on `IconProviderHandle`:

- `register_icon(pack, name, data)`. Adds or overwrites an icon with arbitrary data.
- `register_font_icon(pack, name, font, char)`. Adds a font-glyph icon.
- `register_image_icon(pack, name, image)`. Adds an image icon.
- `unregister_icon(pack, name)`. Removes a single icon.
- `unregister_pack(pack)`. Removes every icon in a pack.
- `set_resolver(callback)`. Replaces the resolver for the whole provider.

Icon names are case-insensitive: registering `"Home"` and looking up
`"home"` resolve to the same entry.

## The resolver callback

The resolver turns a registered icon plus the original `<icon>` node into
a `StyledDom`. The signature is `IconResolverCallbackType`:

```rust,ignore
extern "C" fn(
    icon_data: OptionRefAny,         // the data you registered, or None
    original_icon_dom: &StyledDom,   // the <icon> node with its inline styles
    system_style: &SystemStyle,      // current theme, accent, accessibility flags
) -> StyledDom;
```

The default resolver handles font icons (registered via
`register_font_icon`) and image icons (via `register_image_icon`) out of
the box. For anything else (SVG, animated, vector) write your own
resolver and pass it to `IconProviderHandle::with_resolver(my_callback)`
or `IconProviderHandle::set_resolver(my_callback)`. The callback sees
`SystemStyle`, so you can produce a different DOM for dark mode, a
high-contrast variant, or a reduced-motion fallback.

## System-style integration

The default resolver copies a curated subset of CSS properties from the
original `<icon>` node onto the resolved DOM and filters based on
`SystemStyle`. The `IconStyleOptions` type controls per-icon behaviour
through three fields:

- `inherit_text_color` makes the icon adopt the cascaded `color`.
- `prefer_grayscale` drops explicit colour and adds a grayscale filter to
  image icons.
- `tint_color` overrides the icon's fill with a single colour.

The cascade still runs as normal. A `with_css("color: red;")` on the
`<icon>` node beats the system style.

## Naming conventions

A pack is identified by its name string. The framework reserves no names,
but the convention is:

- `app`: your application's first-party icons.
- `material`, `phosphor`, `lucide`, ...: third-party icon fonts.
- `os`: anything you load from a platform icon theme (Windows shell
  imageres.dll, macOS NSImage, GNOME `icon-theme.cache`).

When two packs ship the same icon name, the *first registered* wins.
Register your `app` pack last if you want app icons to override
third-party ones.

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
use azul::prelude::*;
let _ = Dom::create_button("Settings", SmallAriaInfo::label("Settings"))
    .with_children(vec![Dom::create_icon("settings".into())].into())
    .with_css("
        display: inline-flex; gap: 6px; padding: 6px 12px;
        background: system:button-face;
        color: system:button-text;
        @theme dark { color: system:accent; }
    ");
```

The `@theme dark` rule changes the cascaded `color`, which the default
font-icon resolver picks up when `inherit_text_color` is set.

## Disabling and overriding

A few escape hatches:

- `IconProviderHandle::set_resolver(custom)` swaps the whole resolver
  for one provider.
- `IconProviderHandle::unregister_pack("material".into())` removes every
  icon in a pack. Useful for "skin packs" you load and unload at runtime.

End-user ricing of icons (replacing `material/home` with a user-chosen
SVG without recompiling) is on the road map alongside the existing
`AZUL_DISABLE_RICING` CSS hook described in
[System Themes](themes.md#application-specific-overrides).

## Coming Up Next

- [Images](../images.md) — Loading raster images and CSS backgrounds
- [Built-in Widgets](../widgets.md) — Built-in widgets and how to write your own
- [Layout](../layout.md) — Overview of the layout solver
