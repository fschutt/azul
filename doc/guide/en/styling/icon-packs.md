---
slug: styling/icon-packs
title: Icon Packs
language: en
canonical_slug: styling/icon-packs
audience: external
maturity: wip
guide_order: 74
topic_only: false
short_desc: Register icons and use them with `Dom::create_icon` or `<icon>`
prerequisites: [styling]
tracked_files:
  - core/src/icon.rs
  - layout/src/icon.rs
  - layout/src/icon_remap.rs
  - core/src/dom.rs
last_generated_rev: 7ecd570e4c0c3584e5107e770058c16cb59fa6e7
generated_at: 2026-05-02T12:00:00Z
default-search-keys:
  - IconProviderHandle
  - IconResolverCallbackType
  - IconStyleOptions
  - IconMeta
  - IconRecolor
  - AppConfig
  - SystemStyle
  - Dom
  - StyledDom
---

# Icon Packs

## Introduction

*WIP.* Font, image and SVG icons resolve through the default resolver on
all platforms. Animated icons run through the same callback path with a
custom resolver.

An icon pack is a named bag of icons that the framework looks up by name
when it sees a `Dom::create_icon("home")` node (or an `<icon>` element).
Registration happens once on `AppConfig.icon_provider`; the lookup runs
before every layout pass and resolves the name to a DOM subtree
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

// 2. Register an image icon for an app-specific logo.
config.icon_provider.register_image_icon(
    "app".into(),
    "logo".into(),
    image_ref,
);

// 3. Register an SVG icon. No custom resolver is needed.
config.icon_provider.register_svg_icon(
    "app".into(),
    "brush".into(),
    svg_bytes.into(),
    IconMeta::create_for_svg(svg_bytes.into()),
);

// 4. Use them in a Dom.
let dom = Dom::create_div().with_children(vec![
    Dom::create_icon("home".into()),
    Dom::create_icon("logo".into()),
    Dom::create_icon("brush".into()),
].into());
```

## How lookup works

Icons are stored on `IconProviderHandle` as a nested map of pack to
icon-name to data. Lookup walks the packs in **rank order, then
registration order**, and takes the first match. A pack has no rank until
you give it one, and packs without a rank are searched after every ranked
pack:

```rust,ignore
// "user" was registered after "app", but must win.
config.icon_provider.set_pack_rank("user".into(), 0);
```

An icon spec can be a fallback list (`ios:open_menu,kde:three-lines,menu`).
Bare entries follow the lookup order above. A `pack:name` entry is looked
up only in that pack.

Methods on `IconProviderHandle`:

- `register_icon(pack, name, data)`. Adds or overwrites an icon with arbitrary data.
- `register_font_icon(pack, name, font, char)`. Adds a font-glyph icon.
- `register_image_icon(pack, name, image)`. Adds a full-colour image icon.
- `register_image_icon_with_meta(pack, name, image, meta)`. Adds an image icon with metadata.
- `register_svg_icon(pack, name, svg_bytes, meta)`. Adds an SVG icon. Returns `false` for input
  that is not an SVG document or is larger than 1 MiB.
- `register_dom_icon(pack, name, dom)`. Adds a whole DOM as an icon.
- `unregister_icon(pack, name)`. Removes a single icon.
- `unregister_pack(pack)`. Removes every icon in a pack. Registering into it again puts the pack
  at the back of the registration order.
- `set_pack_rank(pack, rank)`. Lower ranks are searched first.
- `add_icon_remap_rule(name, apply_if, target)`. Adds a remap rule (see below).
- `set_resolver(callback)`. Replaces the resolver for the whole provider.

Icon names are case-insensitive: registering `"Home"` and looking up
`"home"` resolve to the same entry.

## Icon metadata: what the artwork can honour

Every registered icon carries an `IconMeta`. The metadata describes what
the artwork can do. It is separate from what the system asks for:

- `designed_for`: `Light`, `Dark` or `Any`. The mode the artwork was drawn for.
- `variants`: `light`, `dark` and `high_contrast`. Each is an icon spec to draw instead in that
  mode. High contrast wins when the user asks for it.
- `recolor`: how the icon may be recoloured.
- `monochrome`: the artwork is one colour on alpha, so a colour can be flooded through its alpha.

The defaults are `IconMeta::create_for_font()` (follows the text colour)
and `IconMeta::create_for_image()` (never recoloured).
`IconMeta::create_for_mask()` is for monochrome raster ink, such as a
symbolic PNG. `IconMeta::create_for_svg(bytes)` reads the document: an
SVG painted in `currentColor` follows the text colour.

The resolver combines the system's request (`IconStyleOptions`) with the
icon's `recolor`. It never guesses from the kind of icon:

| `recolor`      | Font glyph                  | Raster or SVG                                          |
|----------------|-----------------------------|--------------------------------------------------------|
| `CurrentColor` | the cascaded `color`        | monochrome: flooded with the `<icon>`'s cascaded `color` |
| `Mask`         | the cascaded `color`        | a tint is flooded through the artwork's alpha          |
| `Palette(map)` | as drawn                    | the listed paints are swapped when the SVG is drawn    |
| `Fixed(colors)`| that colour, per mode       | monochrome: flooded with that colour, per mode         |
| `None`         | as drawn                    | only the variant for the mode                          |

A tint is always `flood(tint) composite(in)`: the colour is kept only
where the artwork has alpha. Full-colour artwork is never tinted. Give it
`variants` for the other modes instead:

```rust,ignore
config.icon_provider.register_image_icon_with_meta(
    "app".into(),
    "logo".into(),
    logo_light,
    IconMeta::create_for_image().with_dark_variant("logo-dark".into()),
);
config.icon_provider.register_image_icon("app".into(), "logo-dark".into(), logo_dark);
```

The variant is picked from the window's mode, so an app pinned to dark on
a light desktop gets the dark artwork. A light/dark switch swaps the
artwork on the next frame.

## SVG icons

`register_svg_icon` is enough to use an SVG. `currentColor` in the
document takes the `<icon>` node's cascaded `color`, like a font glyph
does. A document whose only paint is `currentColor` follows the colour
exactly, including a `color` it inherits from its container. A palette
remap swaps literal paints as the document is drawn:

```rust,ignore
let meta = IconMeta::create_for_image().with_recolor(IconRecolor::Palette(vec![
    IconColorMapping { from: ColorU::BLACK, to: /* system:text */ text_token },
].into()));
```

## The resolver callback

The resolver turns a registered icon plus the original `<icon>` node into
a `Dom`. The signature is `IconResolverCallbackType`:

```rust,ignore
extern "C" fn(
    icon_data: OptionRefAny,         // the data you registered, or None
    original_icon_node: &NodeData,   // the <icon> node with its inline styles
    system_style: &SystemStyle,      // the window's mode, accent, accessibility flags
) -> Dom;
```

The default resolver handles font, image, SVG and DOM icons. For anything
else, write your own resolver and pass it to
`IconProviderHandle::with_resolver(my_callback)` or
`IconProviderHandle::set_resolver(my_callback)`. The `system_style` a
resolver receives is in the window's mode, so a resolver that reads
`system_style.mode` sees the mode the window shows.

## System-style integration

The default resolver copies the inline CSS properties of the original
`<icon>` node onto the resolved DOM. `IconStyleOptions` carries the
system's request in three fields:

- `inherit_text_color`: `Mask` artwork follows the cascaded `color`.
- `prefer_grayscale`: image and SVG icons get a grayscale filter.
- `tint_color`: recolours icons whose `recolor` allows it (see the table above).

The cascade still runs as normal. A `with_css("color: red;")` on the
`<icon>` node beats the system style, unless the icon has an explicit
`Fixed` colour.

## Naming conventions

A pack is identified by its name string. The framework reserves no names,
but the convention is:

- `app`: your application's first-party icons.
- `material`, `phosphor`, `lucide`, ...: third-party icon fonts.
- `system`: icons loaded from a platform icon theme.
- `user-icons`, `user-icons/<theme>`: files from the user's icon rules (see below).

When two packs ship the same icon name, the pack with the lower rank wins,
then the *first registered*. Give your `app` pack a rank if app icons
should override third-party ones regardless of registration order.

## User icon rules (`~/.azul/icons`)

End users can replace icons without recompiling. At startup the app reads
`~/.azul/icons/remap.json` and one `remap.json` per theme directory
(`~/.azul/icons/<theme>/remap.json`). A spin-off theme lives in a
subdirectory: `xyz/pink/` is the theme `xyz:pink`. Each table maps an
icon name to rules. The rules for a name are tried in order and the first
match wins:

```json
{
  "material/home": [
    { "file": "home-dark.svg", "apply-if": "theme=monokai,mode=dark", "recolor": "currentColor" },
    { "file": "home.svg", "apply-if": "theme=monokai" }
  ],
  "kde:three-lines": [
    { "file": "menu.svg", "apply-if": "os=linux:kde", "recolor": { "light": "system:text", "dark": "#e6e6e6" } }
  ],
  "app-icon": [
    { "file": "app-mono.svg", "apply-if": "app=azwriter", "designed_for": "any" }
  ]
}
```

- `file` names an SVG or raster file next to the table. The path must stay inside that
  directory. `..`, absolute paths and symlinks that lead out of it are refused. `icon` redirects
  to a registered icon spec instead.
- `apply-if` uses the dynamic-selector vocabulary of CSS conditions: `theme=<name>` (a theme in the
  theme chain; `light` and `dark` mean the mode), `mode=light|dark`, `os=` (the same content as
  `@os(...)`), `app=<executable name>` and `contrast=high|normal`. A comma means AND. An unknown
  term never matches.
- `recolor` is `"currentColor"` (take the CSS colour), `"mask"`, `"none"`, a colour
  (`"#e6e6e6"`, `"system:text"`), `{ "light": ..., "dark": ... }`, or a palette keyed by colours
  (`{ "#000000": "system:text" }`). An explicit colour beats the `<icon>`'s CSS `color`.
- `designed_for` (`light`, `dark`, `any`) and `monochrome` complete the metadata.

The conditions are evaluated at every lookup against the window's live
context, so switching to dark mode swaps the artwork. A theme directory's
rules apply only while that theme is in the theme chain, and a spin-off's
rules beat its base theme's. The global table comes after every theme.
The remap runs before the icon spec's own fallback list: a rule for any
entry of `ios:open_menu,kde:three-lines,menu` beats the app's chain, and
an entry without a rule still falls through it.

Theme directories named `light` or `dark` are refused, because those
names belong to the mode. Set `AZ_RICING=off` to start an app without the
user's rules, for example to check whether a bug reproduces without them.
The same variable turns off the user stylesheet described in
[System Themes](themes.md#how-user-theming-layers-with-component-css).

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

The `@theme dark` rule changes the cascaded `color`, which a font icon
and a `currentColor` SVG icon follow.

## Disabling and overriding

A few escape hatches:

- `IconProviderHandle::set_resolver(custom)` swaps the whole resolver for one provider.
- `IconProviderHandle::unregister_pack("material".into())` removes every icon in a pack. Useful
  for "skin packs" you load and unload at runtime.
- `AZ_RICING=off` skips the user's icon rules.
