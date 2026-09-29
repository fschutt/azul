---
slug: styling/ricing
title: Ricing (User Themes)
language: en
canonical_slug: styling/ricing
audience: external
maturity: wip
guide_order: 75
topic_only: false
short_desc: Restyle any Azul app from `~/.azul/css/<theme>/`, with a priority you choose
prerequisites: [styling, styling/themes]
tracked_files:
  - css/src/rice.rs
  - css/src/system.rs
  - css/src/css.rs
last_generated_rev: 0a326afe5
generated_at: 2026-09-29T12:00:00Z
default-search-keys:
  - RiceStatus
  - RiceFileStatus
  - RicePriority
  - RicingMode
  - SystemStyle
---

# Ricing (User Themes)

## Introduction

A "rice" is a stylesheet the *user* of an app writes, not its developer: a
different accent colour, a denser list, a whole new look. Azul reads rice
files from one directory per theme, applies them to every window of every
Azul app, and says in each file how far it is allowed to reach. Nothing here
needs Rust: a theme is a folder of `.css` files.

The design goal is the one the [stopthemingmy.app](https://stopthemingmy.app/)
letter asked for: apps look the way they were tested unless the user asked
for more, and when the user did, every bug report says so.

## Where the files go

The rice root is `~/.azul` (`%USERPROFILE%\.azul` on Windows):

```text
~/.azul/
  css/
    everywhere.css        global: live under every theme (or the theme its header names)
    monokai/
      colors.css          the theme `monokai`
      fix-azwriter.css    a per-app adjustment (`app: azwriter` in its header)
      pink/
        accent.css        the spin-off `monokai:pink`
  icons/
    monokai/remap.json    icon remap tables, same directory rules
```

The app runs in a **theme chain**, most specific first: `monokai:pink` is
`[monokai:pink, monokai, <the app's default theme>]`, and a theme's header can
say what it builds on (`fallback: native`). For every entry of the chain Azul
reads `css/<entry>/*.css`. A `:` in a theme name is a directory level on disk
(`css/monokai/pink/`), because `:` is not allowed in Windows file names.

Every rule of a file under `css/<theme>/` only applies while that theme is in
the chain, as if the file were wrapped in `@theme(<theme>) { ... }`. When two
themes of the chain set the same property at the same priority, the more
specific theme wins, whatever the selectors: a spin-off's `.btn { }` beats its
base theme's `.btn.primary { }`.

A file directly in `css/` is global. With `theme: abc-base` in its header it
*is* the theme `abc-base` (that is all a base theme needs); without one it is
live under every theme, and its priority is capped at `base`.

The older per-app file still loads, as a per-app file live under every theme:
`~/.config/azul/styles/<app>.css` on Linux (`$XDG_CONFIG_HOME` if set),
`~/Library/Application Support/azul/styles/<app>.css` on macOS,
`%APPDATA%\azul\styles\<app>.css` on Windows. `<app>` is the executable's name
without its extension.

## The header

The first comment of a file is its header: `//` lines or one `/* */` block,
`key: value` items separated by `;` or line breaks.

```css
// theme: abc-base@1.4.0; priority: widgets; fallback: native
// app: azwriter; azul: 0.2.*, 0.3.*; requires: abc@^1.2
.__azul-button { border-radius: 0px; }
```

| Key | Meaning |
|---|---|
| `theme` | The theme the file belongs to, with an optional `@version`. Implied by the directory. |
| `priority` | Which layer the rules take (table below). Default `base`. |
| `fallback` | The themes this one builds on, in order. Extends the theme chain. |
| `app` | The applications the file is for (comma list). The file is inert in every other app. |
| `azul` | The Azul versions the file applies to (comma list, `*` wildcard, `0.2` = every `0.2.x`). On any other version the file is ignored and one warning says so. |
| `requires` | Themes this one was written against, with Cargo-style ranges (`^1.2`, `~1.2`, `=1.2.3`). Never blocks the file: a mismatch is applied anyway and reported. |

A file with no header is a `base` file. A header key Azul does not know is a
warning, so a typo does not go unnoticed.

## Priorities

| `priority` | Beats | Loses to | Use it for |
|---|---|---|---|
| `palette` | (values only) | - | A file of custom properties only (`--accent: ...`). Cannot change geometry. A file whose every declaration is a `--name` is a palette without saying so. |
| `base` (default) | the framework defaults | the app's CSS, widgets | Filling what nobody declared. Cannot break the app. |
| `app` | the app's stylesheets | widget and inline styles | Re-skinning the app's own content. |
| `widgets` | widget styles | the app's runtime overrides | A full theme. What a CSS base theme uses. |
| `force` | everything | - | Your last word, like the web's user `!important`. |
| `off` | - | - | Turns the file off (in a per-app file: the whole theme, see below). |

A `palette` file that sets anything but custom properties is applied as
`base`, with a warning. A file without a theme above `base` is capped at
`base`, because it would apply under every theme.

**Support policy.** A rice above `base` restyles an app beyond what it was
tested with. Report visual issues to the theme's author, not to the app's.
Before reporting a bug to an app, run it with `AZ_RICING=off`: a bug that
reproduces without the rice is the app's.

## One app, one global theme

A global `widgets` theme applies to every app, and one of them may not take it
well. A **per-app** file (`app: <that app>` in its header, or the legacy
per-app file) wins over the global files of its theme in both directions:

- its rules come after theirs;
- an explicit `priority:` in it replaces theirs for that app. `base` turns a
  global `widgets` theme down to "fill in only", `off` turns it off, and
  `widgets` turns a global `base` theme up.

```css
/* ~/.azul/css/monokai/fix-azwriter.css */
// app: azwriter; priority: base
```

## Rice files are untrusted input

A theme downloaded from somewhere is CSS going into the app's parser, so the
loader:

- drops every declaration whose `url()` points at the network (`http:`,
  `https:`, `//host`, and every other scheme but `data:` and local `file:`),
  and every `@import`, each with a warning;
- refuses a file over 1 MiB;
- refuses a theme name that is not ASCII letters, digits, `-` and `_` separated
  by `:` (no `..`, no separators), and the names `light` and `dark`, which are
  the colour scheme;
- refuses a file that resolves, through a symlink, outside `~/.azul`.

## What is loaded right now

`SystemStyle::get_rice_status` lists what the loader did: the theme chain,
every file with its priority, version, `requires` result, and how many of its
rules are live or inert right now (a rule inside `@os(windows)` on macOS is
inert), plus every warning. `RiceStatus::to_report` prints it, ending with the
support policy above:

```rust
let status = info.get_system_style().get_rice_status();
println!("{}", status.to_report());
```

```text
rice (AZ_RICING=default; root /home/me/.azul; app azwriter; azul 0.2.0)
chain: monokai:pink > monokai > flat
  [applied] widgets monokai 1.2.0 /home/me/.azul/css/monokai/colors.css: 40 rules, 38 live, 2 inert
  [applied] palette monokai:pink /home/me/.azul/css/monokai/pink/accent.css: 1 rules, 1 live, 0 inert; palette by inspection
```

With `AZ_DEBUG` set, the same listing is logged whenever the rice is loaded,
and its warnings are logged as warnings always.

## `AZ_RICING`

| Value | Effect |
|---|---|
| unset | Load the rice. |
| `off` (`disabled`, `none`, `0`) | Load no rice, and on Linux skip the riced-desktop sources (pywal, Hyprland). For kiosks, CI, screenshots, and bug reports. |
| `force` (`prefer`, `1`) | Linux: riced-desktop sources win over GNOME/KDE settings. The rice still loads. |
| `watch` (`live`, `reload`) | Load the rice, and rebuild every window when a rice file changes. For writing a theme. |

`watch` checks the files twice a second. On Windows the reload lands with the
window's next message (moving the mouse over it is enough).

## Current limits

- A widget's own inline style properties are still looked up before every
  stylesheet rule, whatever its priority, so `widgets` and `force` reach
  widgets through their class names and stylesheet rules, but not past a
  property the widget sets inline.
- A global `* { }` rule is looked up after every rule that names the element.
- Custom properties set in a rice reach `var()` uses in the same file.

## Related

- [System Themes](themes.md): the `system:*` colours and `@theme` blocks a
  rice builds on.
- [Icon Packs](icon-packs.md): icons, which rice through `icons/<theme>/` remap
  tables rather than CSS.
