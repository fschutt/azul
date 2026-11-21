# Ricing layers, `@basetheme` gating, and the stopthemingmy.app problem (2026-09-29)

Question (Felix): stylesheets already cascade by a priority; let the user's rice file declare its
own priority (`0` = below everything the app does, `5` = above inline) plus the base theme it was
written for, and group rules with `@theme` and the dynamic selectors. Does that solve the
https://stopthemingmy.app/ problem? Research and a proposal, no code. Tree:
`fix/input-bugs-2026-09-19` @ `8020e34c3`. Related: `THEME_ARCH_VS_QT_BREEZE_2026_09_29.md`.

## 1. What the letter actually asks for

Read today (https://stopthemingmy.app/, 2019, re-signed by ~30 maintainers). Its claims:

| # | Claim | Nature |
|---|---|---|
| L1 | Stylesheets applied without QA make apps look broken or unusable; icon themes change metaphors; app icons are brand | technical + social |
| L2 | Screenshots and documentation no longer match the installed app | second order |
| L3 | "Tinkering is fine, but you are in unsupported territory; report to the theme developer, not to us" | attribution |
| L4 | Platform level: GTK should stop forcing one stylesheet on all apps; apps should get the platform stylesheet unless they *opt in* to something else | default + consent |
| L5 | Distributions must not change the stylesheet for third-party apps ("you are not doing this to Blender, Telegram") | global channel |
| L6 | "The expectation that apps can be arbitrarily restyled without manual work is and has always been an illusion" — not a technical problem | limits |

The follow-ups (libadwaita 1.0, 2021: themes off by default, named colours; GNOME 47 accent
colours) answered L4/L5 with a *narrow* channel: the platform may recolour through tokens and
switch light/dark; everything else is the app's. KDE colour schemes are the same idea from the
other side: a scheme changes the palette, never Breeze's geometry. The Breeze bug in the sibling
doc shows what happens when the geometry itself is the theme's.

## 2. Where Azul is today (verified)

- Cascade layers exist as `u8` slots (`css/src/css.rs:822-858`): `UA = 0`, `SYSTEM = 10`,
  `AUTHOR = 20` (every `Css::from_string`), `INLINE = 30` (widget and `with_css` declarations),
  `RUNTIME = 50` (reserved). Rules sort by `(priority, specificity)`, last match wins. The gaps
  are documented as reserved for `@layer`.
- The rice file `~/.config/azul/styles/<app>.css` becomes `SystemStyle.app_specific_stylesheet`
  (`css/src/system.rs:345`), loaded by the three shells when `ricing_enabled()`, and every rule is
  stamped `rule_priority::SYSTEM` (`system.rs:2372`). So today a rice **cannot** override the
  app's CSS or any widget; it only fills properties nobody declared. That is the letter's L4 by
  construction, and too little for ricers.
- No `!important`, no `@layer`. `revert` exists as a value (`css.rs:674`); its rollback semantics
  across layers should be pinned before relying on it.
- `@theme <name>` with a custom name parses but never matches (the context carries one
  `ThemeCondition`, light or dark; `doc/guide/en/styling/themes.md`). A base-theme gate needs its
  own condition.
- Widgets: light values are literals (Bootstrap, `button.rs:146-201`), dark twins read `system:`
  tokens (`themes/system_palette.rs`). Token-only recolouring therefore works in dark mode only.
- The only *global* channel is `SystemStyle` (accent, palette, fonts, metrics, and on Linux the
  riced-desktop sources under `AZ_RICING=force`: pywal, Hyprland, i3). There is no global user
  stylesheet. `:nth-child(odd|even)` is a normal selector (`doc/guide/en/styling.md:169`).

## 3. The web's answer, for calibration

CSS Cascade 5 orders origins low → high: UA, user, author (with `@layer` order inside author),
animations, then the `!important` declarations in *reverse* origin order (author, user, UA),
then transitions. Two properties matter here: the **user always has the last word**
(user `!important` beats author `!important`), and **the boundary is explicit** (origin +
importance, plus `revert` to hand a property back to the previous origin). Nobody files a bug
against a website because their Stylus theme broke it: the user installed it, per site, knowingly.
GTK's failure was not that theming existed but that it was global, silent and unattributable.

## 4. Proposal: named layers + a header + a base-theme gate

### 4.1 File-level meta-comment, low default

The first comment of the rice file carries the declaration (Felix's form). The parser strips
comments (`css/src/parser2.rs`), so the loader pre-scans the first comment before parsing:

```css
// theme: xyz:pink; priority: widgets; fallback: xyz, native
```

Decided 2026-09-29: the header is the source of truth for three things, and it is sugar for
what the cascade already has:

- `theme:` **wraps the whole file in `@theme(xyz:pink) { … }`**. The file is live only while that
  theme is in the active chain (§7.1) and inert otherwise. **No `theme:` means always active**,
  which only makes sense at `palette` or `base`. A file placed under `css/<theme>/` gets that
  theme implied when the header omits it.
- `priority:` is the `CssRuleBlock.priority` stamped on every rule of the file (today always
  `SYSTEM`, `system.rs:2372`). Names are aliases for numbers. **No header, or no `priority`, means
  `base`**: the file cannot break the app, which is today's behaviour and the responsible default.
  A priority above `base` without a `theme:` is clamped to `base` and logged once.
- `fallback:` states the chain below this theme when the `:` prefix rule is not what the author
  wants (`abc` has no prefix, so `fallback: native` says what `abc` builds on). Without it the
  chain is the prefix chain plus the app's default theme.

Inside the file, light and dark are blocks (`@media (prefers-color-scheme: dark) { … }` nested in
the theme), and the variables live inside those blocks.

| priority | slot | beats | loses to | contract |
|---|---|---|---|---|
| `palette` | inferred | — | — | the file only sets custom properties on `:root` (inside `@theme` blocks or not). Cannot touch geometry. The KDE-colour-scheme / libadwaita-accent tier, and what pywal users want. Never clamped. |
| `base` | `SYSTEM` (10) | UA | app CSS, widgets | default: fills what nobody declared |
| `app` | 25 | app author sheets | widget inline, app inline | re-skin the app's own DOM, widgets untouched |
| `widgets` | 35 | widget inline | `RUNTIME` overrides | a full theme; the app's programmatic overrides (a colour-picker preview, a drag ghost) still win |
| `force` | 60 | everything | — | web's user `!important`. Explicitly unsupported territory |

`revert` at any priority hands the property back to the layer below (pin the semantics with a test).

### 4.2 `@theme(name)` is the grouping, and the active theme is a set

`@theme(x)` groups rules that are live only while `x` is active; nothing has to be filled, the
block is either on or inert. For that to carry the base theme *and* the colour scheme *and* a
user-named theme at once, the context's single `theme: ThemeCondition`
(`css/src/dynamic_selector.rs:1292`, where `Custom(name)` never matches today) becomes a **set of
active names**: the colour scheme (`light` / `dark`, also reachable as the already parsed
`@media (prefers-color-scheme: dark)`), the active `UiTheme` name (`flat` / `flora` / `native`),
and the names activated by the header or an app setting (`monokai`). `@theme(x)` matches iff
`x` is in the set.

The whole design is **additive nesting**. Nesting at-rules is conjunctive (all conditions of a
block must hold, `CssRuleBlock.conditions`, `css/src/css.rs:868`); files and priorities are
additive (a higher priority adds declarations on top of the lower ones, it never replaces a
sheet); `revert` subtracts one declaration. Developer and user use the same vocabulary: the app
states its rules under its conditions at its priority, the rice states its rules under its
conditions at the priority it asked for, and the cascade merges them. Neither side replaces the
other's stylesheet, which is what a GTK theme did.

Nesting composes:

```css
@theme(monokai) { @theme(flora) { @os(linux:kde) { @media (prefers-color-scheme: dark) {
    .__azul_native-list-rows-row:nth-child(even) { background: var(--mk-bg-alt, system:background); }
}}}}
```

With today's single-valued context, `@theme(monokai)` nested in `@theme(dark)` could never both
hold; the set fixes that. `css/tests/test_nesting.rs` covers nested `@os`; add a case for
`@theme` × `@theme` × `@os` × `@media`.

This is the answer to Tobias Bernard's "Restyling apps at scale": a theme is only ever live
against the stylesheet it was tested with, and the engine knows which one that is, because the
base themes are compiled into the binary and versioned with azul. GTK could not do this because
its stylesheet was a global runtime resource with no version handshake. `@os`, `@media` and
`@lang` inside the rice work as in app CSS, so a rice is a *policy* ("on KDE dark, use these")
that follows the OS switch, not a static skin.

### 4.3 Variables and `@theme` are different tools; support both

- A custom property is a **value** and must always resolve: `var(--x, fallback)`. The parser
  accepts it (`parser2.rs:8, 2040`), longhands only (`margin-top: var(--m)`, not `margin: var(--m)`,
  `parser2.rs:187-189`), and a note at `parser2.rs:2079` says the resolved declarations were not
  consumed end to end at some point; pin resolution through the cascade with a test before
  building on it.
- `@theme` is **control flow**: a block is live or inert, and an inert block needs no value.
- Why both (Felix, 2026-09-29): a variable by itself cannot know whether it is being read in a
  light or a dark context, on GNOME or on KDE; it is one value. The *context* has to select the
  value, and nesting is how: `@theme(dark) { :root { --bg: #272822; } }` and
  `@os(linux:kde) { :root { --radius: 3px; } }` set the same variables per context, and the
  rest of the theme, written once against the variables, stays untouched. A DE-specific tweak is
  then one nested block, not a second theme file. Web CSS reaches the same place with
  `@media (prefers-color-scheme)` around custom-property declarations; Azul's `@theme` / `@os`
  set is the generalisation to base theme, named theme and desktop environment.
- Together: theme blocks set variables, widgets read them with `system:` fallbacks,
  `background: var(--azul-button-face, system:button-face)`. A file that only sets custom
  properties is a `palette` rice by inspection (loader lint: every declaration is a custom
  property), so it needs no header and is never clamped. That replaces the separate `@palette`
  at-rule idea and gives pywal-style recolouring one CSS mechanism.

### 4.4 Consent and attribution (the non-technical half)

```rust
AppConfig.ricing_policy = RicingPolicy {
    supported: RiceLayer::Base,   // the highest layer the app has tested; default Base
    beyond:    Beyond::AllowAndAttribute,   // or Beyond::Clamp
}
```

- `AllowAndAttribute` (default): a rice above `supported` is applied, and
  `SystemStyle.rice_status = BeyondSupport { path, layer }` is set. The About widget, the crash
  mail and `AZ_DEBUG` print: "restyled by ~/.config/azul/styles/foo.css (layer: widgets, beyond
  this app's supported layer). Report visual issues to the theme, not the app." L3 becomes
  enforceable instead of a plea, because every report carries the fact GTK never had.
- `Clamp`: the file is applied at `supported`. This is the letter's opt-out, but softened: the
  app keeps working with palette and base rices instead of switching theming off entirely.
- `AZ_RICING=off` stays as the user's own check ("is it the rice?") and the screenshot mode (L2).
- The `palette` layer is never clamped: it is the channel the letter itself accepts
  (accent, dark mode, fonts, the things macOS and Windows already do to every app).

### 4.5 Default theme = platform theme (L4)

Once `UiTheme::Native` exists it should be the default, with Flat and Flora as explicit opt-ins.
That is literally the letter's platform-level ask ("use the platform stylesheet unless they opt
in to something else"), and it is the opposite of today's default (`Flat`, `themes/mod.rs:12`).

### 4.6 Precondition: light values must come from tokens too

`palette` recolouring is only coherent if widgets read `var(--…, system:…)` for their *light*
values as well, with the platform's light palette as the default (today only the dark twins read
tokens, `system_palette.rs` header). This overlaps with the native theme work, which has to read
`system:` anyway.

## 5. Does it solve the letter?

| Claim | Answer |
|---|---|
| L1 broken apps | Not solvable for `widgets`/`force` (L6 is right); solved for `palette`/`base` by construction, and the `basetheme` clamp removes the "designed for another stylesheet" breakage. |
| L2 screenshots | `AZ_RICING=off` + `CssMockEnvironment` render the pristine app; ricers can render *their* screenshots the same way. |
| L3 attribution | Solved: rice status in About / crash mail / debug output; per-app file with a header. |
| L4 default | Solved by 4.4 plus per-layer consent; no app has to hard-code anything. |
| L5 global channel | Solved structurally: there is no global stylesheet; distributions reach Azul apps only through `SystemStyle` tokens, the channel the letter accepts. If a global rice file is ever added, cap it at `palette`. |
| L6 illusion | Made explicit instead of denied: the layer names *are* the contract, and everything above `base` is labelled unsupported in the file the user wrote. |

Icons and app icons are outside CSS; the same header could later gate icon packs
(`doc/guide/en/styling/icon-packs.md`).

## 6. Cost and order

1. Meta-comment pre-scan, wrap-in-`@theme(basetheme)`, priority stamping with slots 25/35/60,
   the no-basetheme clamp: `system.rs` loader and the rule stamper, ~½ day.
2. Theme set in `DynamicSelectorContext` (colour scheme + `UiTheme` name + header/app names),
   `@theme(x)` matching against it, nested-condition test: ~1 day.
3. `var()` resolution pinned end to end through the cascade, plus the palette-by-inspection lint:
   ~½ day if resolution already works, more if `parser2.rs:2079` is still true.
4. `ricing_policy` + `rice_status` + About/crash-mail wiring: ~½ day.
5. Light values from tokens (4.6): part of the native theme work.
6. `revert` semantics test across layers: hours.

Decisions for Felix: (a) five priority names as above, or fewer? (b) default
`Beyond::AllowAndAttribute` or `Clamp`? (c) make `Native` the default `UiTheme` once it exists?
(d) is `theme=` in the header the way a user activates a named theme, or should the app expose a
picker over the names it finds in the file?

## 7. The fuller model (Felix, 2026-09-29) and its validation

The model, in Felix's words, condensed: every widget carries every style it knows about, as
contained `@theme(...)` blocks. `AZ_THEME=xyz` (OS-global, read at startup) names the requested
theme. After `layout()` returns the DOM, the engine matches the dynamic selectors and throws out
the blocks that do not belong to the active theme. Then it loads `~/.azul/css/xyz/*.css`, the
user's rice for that theme, which adjusts on top. Every theme has light and dark sub-modes, and
inside those, variables. Spin-offs use `:`: `xyz:pink` adds a `:root` block that sets `--color`
and wins over `xyz` because it is more specific; where `xyz:pink` has no rule, `xyz` applies.
Fallbacks are user-defined, like `fr-CA → fr → en` for languages. Goal: an OS-consistent look
with sane fallbacks, and a user who can fix their rice when the app does not know a theme `abc`.

This supersedes parts of §4: the rice is per **theme directory**, not per app; the directory
*is* the `basetheme`, so the header keeps only `priority`; the "clamp on base mismatch" rule is
gone, because a rice for `abc` exists precisely when the app does not know `abc`.

### 7.1 Semantics that make it hold together

- **The active theme is a chain**, most specific first, exactly like the locale fallback list
  (`fr-CA → fr → en`): `AZ_THEME=xyz:pink` expands by prefix to `[xyz:pink, xyz]`; the theme's
  own file header extends it (`fallback: native`, §4.1), and the app's compiled-in default theme
  is always the implicit last entry. An unknown `abc` with no header is therefore
  `[abc, <default>]`: the default look, plus whatever `css/abc/` adds. That is the "sane
  fallback", and the user fixes it by writing the `abc` file.
- **A `@theme(x)` block is live iff `x` is in the chain** (prefix matching, the precedent is
  `LanguageCondition::Prefix`, `css/src/dynamic_selector.rs:2654`). Its **rank** is its index
  in the chain; unconditional declarations rank last. The fallback "if `xyz:pink` has no rule,
  `xyz` applies" needs no special check: both blocks are live and rank decides where both define.
- **Rank is a cascade layer, not selector specificity.** The sort key becomes
  `(priority, rank, selector specificity, source order)`; today it is `(priority, specificity)`
  only (`css/src/css.rs:2144-2149`) and conditions contribute nothing, so `xyz:pink` would beat
  `xyz` only by source order. Putting rank before specificity is CSS `@layer` semantics: a
  spin-off's `.btn { }` beats the base's `.btn.primary { }`, which is what a spin-off author
  expects, and what makes `:root { --color: pink }` win without touching any consumer rule.
- **Light/dark stays an orthogonal condition**, `@media (prefers-color-scheme: dark)` (parsed
  today) nested inside the theme block, never a name segment. Reason: the colour scheme flips at
  runtime through the existing restyle path; a chain change is a startup decision that reruns
  `layout()` (plan step 1, DOM recreation). Two different mechanisms, two different axes.
- **Only the colour scheme is paint-only; a theme switch is not** (Felix, 2026-09-29). Themes
  may differ in DOM structure: Flora may wrap a button in decorative nodes, Flat does not
  (`flora.rs:941`). The DOM shape comes from the highest-ranked compiled-in theme in the chain,
  the structural base; a CSS theme cannot add nodes and gets its extras through `::before`,
  `::after`, box-shadow, outline, gradients and filters, or names a structural base in its chain
  (`flora:abc`). Invariant: for a fixed chain, a widget's DOM fingerprint is identical across
  light/dark and every `@os`/`@media`/state condition, because those take the restyle path; a
  chain change always goes through DOM recreation and may change layout. The 09-12 analysis'
  I6 is corrected to say "colour scheme" for this reason.
- **Completeness for theme authors** is a coverage report, not a gate, exactly like translation
  coverage with fallback to the base language: because the chain ends in a working floor, an
  incomplete theme is inconsistent, never broken. The report, from the widget manifest: floor
  variables defined vs left to fallback; every colour variable set in both modes; widget class ×
  state × mode cells where the floor shows through (base themes only, a spin-off is incomplete
  on purpose); dead rules (selectors matching no widget class, variables set but never read); and
  the headless reftest contact sheet under the theme via the mock environment for visual review.
- **Rice directories follow the chain**: for each entry `n`, load `css/<n>/*.css` wrapped in
  `@theme(n)`, so `css/abc/` files rank above `css/native/` files at the same priority. Path
  segments instead of `:` on disk (`css/xyz/pink/`), because `:` is illegal in NTFS file names.
  Per-app adjustments: `css/<theme>/apps/<app>.css`, or the header's `app=` key; decide one.
- **Priority stays the responsibility knob**: no header, `base`; the file cannot break the app.
  `widgets` and `force` are the user's explicit choice and are attributed (§4.4).

### 7.2 What the tree does today, checked

| Piece | State | Where |
|---|---|---|
| `var()` resolution | **Parse time, per stylesheet string, condition-blind.** Every `--name` definition in that one string goes into a flat `BTreeMap` (last wins, `@theme`/`@media` around the definition ignored), and each `var()` in the same string is substituted before the cascade sees it. Element scoping is explicitly not modelled. | `css/src/parser2.rs:2040-2100` |
| Consequence | A rice's `:root { --color: pink }` never reaches a `var(--color)` in another stylesheet or in a widget's inline declarations, and `@theme(dark) { :root { --bg: … } }` next to a light definition collapses to whichever comes last. The model's variable channel does not exist yet. | |
| `Dynamic` declarations inline | Representable on nodes (`core/src/dom.rs:3447`); widgets build static values in Rust today. `env()` is documented as resolved by the cascade against the live context (`parser2.rs:2054`), the precedent for cascade-time `var()`. | |
| `:root` | Parses as a pseudo-selector (`parser2.rs:476`); used by the var tests (`parser2.rs:4182`). | |
| `@theme(name)` | `ThemeCondition::Custom(name)` parses, never matches; the context holds one value (`dynamic_selector.rs:1292`). No chain, no prefix, no rank. | |
| `AZ_THEME` | Means `light|dark` **pin** today (`dynamic_selector.rs:828-833`, `system.rs:90-98`, `apply_env_theme_pin`). Reusing the name for the chain needs a migration: `AZ_COLOR_SCHEME=light|dark` for the pin, `AZ_THEME=dark` kept as an alias for one release. | |
| Widget themes | `UiTheme::{Flat, Flora}` selects one generator at build time (`button.rs:630-635`); nothing carries several themes yet. | |
| Rice loading | One file, `styles/<app>.css`, stamped `SYSTEM` (`system.rs:2372`). | |

### 7.3 Verdict

The model is sound: chain + prefix match + rank-as-layer reproduces the described fallback and
spin-off behaviour without special cases, the colour scheme stays runtime-switchable, and an
unknown theme degrades to the default look that the user can then rice. "Every widget carries
every style" is the right *logical* model (the lint can check exhaustiveness, a DOM dump shows
all looks); as an optimisation the theme functions can take the chain and emit only its members,
which is observably identical because the chain is fixed at startup.

The one hard precondition is **cascade-level custom properties**: `--name` stored on nodes as
inheritable, conditional declarations; `var()` resolved at getter time under the dynamic context
across stylesheets, rice files and inline declarations. Everything Felix described about
light/dark sub-modes holding variables, and about `xyz:pink` being one `:root` block, depends on
it, and today's parse-time substitution is the opposite design. It is the same shape of work as
the themed UA table (`a52fdb33a`): one resolver, consulted by both cascades.

Two smaller preconditions: theme rank in the sort key, and the `AZ_THEME` migration. The
responsibility and attribution parts of §4 carry over unchanged.

Division of labour (Felix, 2026-09-29): the application ships the hard-coded base themes
(`flat`, `flora`, `native`, each carrying its light and dark sub-modes and its variables); users
build custom themes on top as `:` extensions. At the top of a chain the changes are mostly
colours, so the common rice is a `palette` file: a `:root` block of variables inside
`@theme(xyz:pink)`, no header, never clamped, unable to break the app. Escalating to `widgets` or
`force` is the rare case and is the only one that has to be declared. The `var` refactor
(cascade-level custom properties) is agreed as the first item regardless, since it is needed
for the sub-modes even without ricing.

### 7.4 Order of work

1. Cascade-level `--name` / `var()` with inheritance and conditions; test: one DOM, a `var()`
   consumer in inline declarations, definitions in two stylesheets under `@theme(dark)` and
   `@theme(light)`, the value follows the context. ~2-3 days.
2. Theme chain in `DynamicSelectorContext` (prefix match, rank), rank in the sort key, tests for
   `xyz:pink` over `xyz` regardless of source order, and for `[abc, native]`. ~1 day.
3. `AZ_THEME` chain parsing + `theme.toml` fallback list + `AZ_COLOR_SCHEME` migration. ~½ day.
4. Rice loader over chain directories, header `priority`, attribution. ~1 day.
5. Widgets: `UiTheme` enum → theme names, every widget emits its blocks for every theme it knows
   (or by chain), exhaustiveness lint. Part of the theme rework steps 1 and 3.

Decided 2026-09-29: the fallback list lives in the file header (`fallback:`), otherwise it
follows from `:`; rank sorts before selector specificity (`@layer` semantics, "a bit more
usable"); `AZ_THEME` is refactored from the light/dark pin to the theme chain. (f) decided
2026-09-29: **per-app rules use a header key `app:`**, not a subfolder, so a theme is a flat set
of files that one OS-level theme can replace wholesale. That makes `~/.azul/` the distribution
format: a "theme exchanger" downloads someone's theme and unzips its `.css` and `.json` files
into `.azul/css/<theme>/` and `.azul/icons/<theme>/`, and the active theme is always the one
name the chain starts from, so what is installed and what is live are never two questions.
Files whose `app:` names another application are inert for this one.

## 8. Icons: user icon packs and remap tables

Icons resolve outside CSS, so they need their own ricing path. What exists
(`doc/guide/en/styling/icon-packs.md`, `core/src/icon.rs`, `layout/src/icon.rs`):

- `IconProviderHandle` holds packs as a nested map, pack → name → `RefAny`. **Lookup walks the
  packs in registration order, first match wins**; pack names are namespaces, not a selection
  mechanism. An icon spec is already a per-icon fallback chain
  (`<icon>ios:open_menu,kde:three-lines,menu</icon>`, `icon.rs:288-296`).
- The resolver callback receives `SystemStyle` and turns icon data plus the `<icon>` node into a
  `StyledDom`; the default resolver handles font and image icons, SVG needs a custom resolver.
  `IconStyleOptions` has `inherit_text_color`, `prefer_grayscale`, `tint_color`.
- Dark variants are a **resolver decision** made by reading the theme; the icon data carries no
  "designed for light/dark" information (`icon.rs:550-552`). `resolve_icons_in_dom` already
  invalidates on any `SystemStyle` change (`icon.rs:733`), so a light/dark swap is runtime-safe.
- Zip packs register file names as icon names, no manifest (`layout/src/icon.rs:437`). End-user
  icon ricing is listed as "on the road map" in the guide.

Felix's proposal (2026-09-29): a global remap table `~/.azul/icons.json`, a theme-specific one
`~/.azul/icons/<theme>/remap.json` next to the replacement SVGs, and per-icon rules in the
table: which variant to use at which contrast (light/dark), or the colour to recolour with for
light/dark. He also names the gaps this exposes in the icon model itself: no light/dark design
metadata on an icon, no API to swap icon themes independently of CSS, no per-icon recolouring
rule for theme, light and dark.

Validation:

- **Order.** Registration order cannot express "user pack beats app pack" unless the user packs
  are registered first, which they cannot be (the app registers at startup). Reuse the theme
  chain: lookup order becomes chain rank, then registration order, so
  `icons/xyz/pink/` > `icons/xyz/` > global `icons.json` > app packs. One ordering concept for CSS
  and icons.
- **Two levels compose.** The icon spec's own fallback list (`ios:open_menu,kde:three-lines,menu`)
  is the app's statement; the remap table rewrites each resolved entry (`material/home →
  xyz/home.svg`). Remap first, then spec fallback, so a remap for the first entry wins and an
  unmapped entry still falls through the app's chain.
- **Metadata belongs on the icon, not in the resolver.** Add to the registered data (and to the
  remap format) `designed_for: light | dark | any`, `variants: { light: …, dark: … }`, and
  `recolor: currentColor | { light: <color|system:token>, dark: … }`. The default resolver picks
  the variant from `SystemStyle.theme` and applies `recolor` through the existing `tint_color` /
  `inherit_text_color` path. That closes the three gaps Felix listed without a new resolver API,
  and the runtime invalidation already exists.
- **Icons have no theme of their own** (decided 2026-09-29). An icon is identified by a string
  (`material/home`); a wholesale "icon theme" that swaps artwork for every name is exactly what
  the letter's L1 complained about, the metaphor changing under the app. So there is no
  `AZ_ICON_THEME`. A `.azul` theme ships a folder of SVGs plus **explicit per-name rules** in
  `remap.json`, globally or per app, per theme; the rules key off the same `AZ_THEME` chain with
  the same semantics. JSON, not CSS, because these are structural rules about files and
  conditions, and CSS is the wrong language for them. The "discover and register theme packs"
  step in `SystemStyle` discovery loads them at startup.
- **Evaluate `apply-if` at lookup time, not only at startup.** Discovery loads the rules once,
  but `theme=dark` changes at runtime; `resolve_icons_in_dom` already invalidates on every
  `SystemStyle` change (`icon.rs:733`), so matching the rules against the live context at lookup
  is what makes a dark-mode switch swap the artwork. Startup-only evaluation would leave stale
  icons.
- **App, window and tray icons go through the same rules** (decided): they are the same string
  plus `<icon/>` replacement logic, so nothing special is needed and no veto. In practice a
  distribution maintainer will rarely write these rules; the mechanism exists for the user.

Remap table sketch (`.azul/icons/remap.json` globally, `.azul/icons/<theme>/remap.json` per
theme; rules per name are tried in order, first match wins, comma in `apply-if` is AND):

```json
{
  "material/home": [
    { "file": "home-dark.svg",  "apply-if": "theme=monokai,theme=dark", "recolor": "currentColor" },
    { "file": "home.svg",       "apply-if": "theme=monokai" }
  ],
  "kde:three-lines": [
    { "file": "menu.svg", "apply-if": "os=linux:kde", "recolor": { "light": "system:text", "dark": "#e6e6e6" } }
  ],
  "app-icon": [
    { "file": "app-mono.svg", "apply-if": "app=azwriter,theme=monokai", "designed_for": "any" }
  ]
}
```

`apply-if` uses the dynamic-selector vocabulary (`theme=` chain membership by prefix, including
`light`/`dark`; `os=`; `app=`; `contrast=high`), so CSS and icon rules share one condition set.

### 8.1 Recolouring is per icon kind, and the API has to say which (Felix, 2026-09-29)

| Kind | Today | What recolouring needs |
|---|---|---|
| Font glyph (built-in Material) | text colour; `inherit_text_color` / `tint_color` become `color` (`layout/src/icon.rs:350+`) | nothing new: the glyph *is* the text colour. `recolor: currentColor` is the default. |
| Raster image (PNG, zip packs) | `prefer_grayscale` → `ColorMatrix`; `tint_color` → **`Flood(tint)` alone** (`icon.rs:341-342`). A flood with no composite step fills the whole box; verify on screen, it likely paints a square, not a tinted glyph. | mask semantics: `Flood(color)` + `Composite(In)` against the source alpha (both variants exist in `StyleFilter`, `css/src/props/style/filter.rs:55,62`). Only works for monochrome-on-alpha artwork, so the metadata must say `monochrome: true`; a full-colour raster gets `variants` (light/dark files), never a tint. |
| SVG (user packs, the ricing case) | Azul's own tessellator (lyon + agg, `layout/src/xml/svg.rs`); `SvgFillStyle` is a fill *rule*, colour is applied at draw time; no `currentColor` in the parser; SVG icons need a custom resolver (guide). | (a) `currentColor` in the SVG parser, resolved to the `<icon>` node's cascaded `color`, which makes single-colour SVGs behave like font glyphs; (b) a per-path palette remap for multi-colour art, `recolor: { "#000000": "system:text", "#ffffff": "system:background" }`, applied when tessellating; (c) `register_svg_icon` in the default resolver so users need no Rust. |

The rule: an icon declares **how** it may be recoloured (`currentColor`, `mask`, `palette`, or
`none` with `variants` only), and the resolver never guesses from the kind. That is the missing
piece in `IconStyleOptions`, whose three flags describe the *request* (grayscale, tint, inherit)
but not what the artwork can honour.

### 8.2 API deltas

- `IconData` (registered value) gains metadata: `designed_for: Light | Dark | Any`,
  `variants: { light, dark, high_contrast }`, `recolor: CurrentColor | Mask | Palette(map) | None`,
  `monochrome: bool`. Default for font icons: `CurrentColor`; for images: `None`.
- `IconProviderHandle::register_svg_icon(pack, name, svg_bytes, meta)` and
  `register_image_icon_with_meta(...)`; zip and directory packs read an optional
  `remap.json` next to the files for the metadata.
- Lookup order: `IconProviderHandle::set_pack_rank(pack, rank)` (or packs registered with a
  rank), chain rank before registration order.
- `SystemStyle.icon_style` keeps the three request flags; the default resolver combines
  request × capability: `tint` on `Mask` → flood-in; on `CurrentColor` → `color`; on `Palette` →
  remap; on `None` → variant only.
- `apply-if` matcher shared with the CSS conditions, evaluated at lookup against the live
  `SystemStyle` / theme chain.
- Needs verification (ledger E15): the raster tint path pushes a bare `Flood` with no
  `Composite(In)`; check with a reftest of one PNG icon at two tints before building on it.

Order of work: (1) metadata fields + capability-aware default resolver + the flood fix, ~1-2
days; (2) `currentColor` and palette remap in the SVG path, ~1-2 days; (3) rank-based lookup
order, ~½ day; (4) remap loader over the theme chain directories, SVG registration from files,
~1 day; (5) attribution in About next to the CSS rice status.

Decided 2026-09-29: (h) no icon theme axis, icons follow the `AZ_THEME` chain through explicit
per-name JSON rules; (i) app, window and tray icons use the same rules, allowed, no veto.

## 9. Validation: does the whole design alleviate the designer / developer / user problems?

Mechanisms, for reference: **A** contained `@theme` blocks, every widget carries every theme it
knows; **B** theme chain with `:` prefix, `fallback:`, app default last, rank before selector
specificity; **C** priority header, `base` default that cannot break the app, attribution;
**D** cascade-level variables inside nested light/dark blocks with `system:` fallbacks;
**E** `.azul/css/<theme>/` files, `app:` key, theme exchange by unzip; **F** icon remap rules;
**G** existing `CssMockEnvironment`, reftests, lint manifest, `AZ_RICING=off`; **H** native theme
as the default (planned).

| Party | Need | Verdict | By |
|---|---|---|---|
| Designer | D1 the designed look reaches the screen intact | Solved against *accidental* breakage; a `widgets`/`force` rice can still override by the user's explicit choice, which is the web's user-`!important` boundary | C, D |
| Designer | D2 a stable base to design against ("stylesheet is API") | **Partial**: nothing yet declares *which* variables a base theme exposes; "mostly colours" is a hope until it is a manifest | D, gap 1 |
| Designer | D3 iterate without a developer, preview every context | Solved for CSS and SVG icons (no Rust after §8.2); preview via mock environment; startup-only rice loading is the friction | G, gap 4 |
| Designer | D4 one theme for many apps, coherent | Solved once widgets read tokens for light values too | E, H, §4.6 |
| Designer | D5 not hand-writing states × modes × DEs × themes | Collapses to "set variables" for spin-offs; stays combinatorial for base-theme designers, made tractable by the reftest matrix rather than removed | A, D, G |
| Developer | V1 no theme can break the widgets | Solved at `palette`/`base` by construction; variables are the safe channel across priorities, so widgets are recoloured without any rule touching their declarations; `widgets`/`force` are the user's call | C, D |
| Developer | V2 attribution in bug reports | Solved if About and crash mail show rice status by default | §4.4 |
| Developer | V3 one place per widget, no whack-a-mole | Solved **conditionally** on the engine invariants I2-I5 and the pair lint I8; A makes the exhaustiveness lint possible | A, THEME_CHAIN_ANALYSIS |
| Developer | V4 test every combination headless | Solved, and uniquely so versus Qt/GTK; rice files can join the matrix through a test-only rice path | G |
| Developer | V5 an in-app theme picker without own theming code | Solved: set the chain head, list known names plus `.azul/css/*` | B, E |
| Developer | V6 cheap runtime dark switch, startup theme choice | Solved: restyle path for light/dark, DOM recreation for the chain | B |
| Developer | V7 third-party widgets follow the theme | Partial: convention plus lint, no enforcement; needs a written widget-author contract | A, gap 1 |
| User | U1 every app follows the desktop | Solved once H lands and light values come from tokens | H, §4.6 |
| User | U2 rice, spin off, share | Solved; the core of the design | B, C, E |
| User | U3 fix an app that does not know my theme | Solved: chain fallback plus `css/abc/` | B, E |
| User | U4 know when I am in unsupported territory | Solved: priority names, attribution, `AZ_RICING=off` self-check | C |
| User | U5 low effort, mostly colours, copy someone's file | Solved in mechanism; needs the variable names to be discoverable | D, gap 1 |
| User | U6 accessibility survives any rice | **Not addressed**: a `widgets`/`force` rice can remove focus rings or contrast | gap 3 |

**Gaps the design still has, and what closes them**

1. **Variable list per base theme, derived, not authored** (D2, V7, U5). Felix, 2026-09-29:
   variables are the least-liked part of the design, so they get no separate manifest to
   maintain. The list of a theme's variables is a scan of its `--` definitions (a grep at worst;
   the theme docs page is generated from the same scan, with the type inferred from the
   fallback each consumer declares). Two hard rules replace the manifest: **every `var()` must
   declare a fallback**, enforced by the lint as an error, so an unknown or mistyped variable
   always degrades to a working value; and variable names are per base theme, never per OS, only
   values differ across `@os`. The `palette` tier is then "a file whose declarations are all
   `--` definitions", and "colour-typed only" is checked against the fallbacks of the consumers
   that read those names. For CSS base themes the same scan runs on the file; for the compiled-in
   themes it runs on the declarations the theme functions emit.
2. **Version handshake** (D2). Semantic versions on both sides of a dependency, apply-and-attribute
   on mismatch; spelled out in §9.2.
3. **Accessibility floor** (U6). When `prefers-contrast` or a forced-colours mode is on, the
   engine applies the system palette and focus visuals *above* every priority, the way the web's
   `forced-colors: active` overrides author colours. The design protects the app from the user;
   this protects the user from their own rice when they need it.
4. **Live reload of rice files** (D3). `AZ_RICING=watch` re-runs discovery on file change; the
   restyle and DOM-recreation paths already exist, so this is a watcher, not a new pipeline.
5. **Support policy in the docs** (V2). The letter's "unsupported territory" sentence, in Azul's
   words, next to the priority table. Attribution only works if the norm is written down.

**Base themes need no Rust** (Felix, 2026-09-29; this replaces the architectural question that
stood here). A base theme is nothing more than a chain entry. Any unknown name becomes one by
being registered: `.azul/css/foo.css` with `// theme: abc-base; priority: widgets` *is* the base
theme `abc-base`, even though no widget knows the name. `AZ_THEME=abc-base` switches to it, and
`abc-base:pink` spins off it. It works because the chain always ends in the app's compiled-in
default theme: the CSS base theme overrides the default's inline declarations by priority and
everything it does not cover falls through to the default look. So "every widget carries every
theme it knows" is only about the compiled-in themes (`flat`, `flora`, `native`), which are the
floor; the whole space above the floor is CSS, for designers and users alike, with one language,
one lint, one preview. Two consequences:

- The **widget DOM structure and class names** (`.__azul_native-list-rows-row`, …) become API,
  because a CSS base theme addresses widgets through them. The commitment (Felix, 2026-09-29):
  azul's base widgets are meant to be as stable as HTML's `<input>`, which does not suddenly
  change; class names are never renamed, structure changes are treated as breaking and
  practically never made, new widgets get new names. Under that commitment `requires-azul` is
  informational, not a gate: a rule whose selector matches nothing is inert by construction, so
  an older theme on a newer azul loses nothing it had, and a newer widget is covered by adding
  one more `.css` file on top. The documented widget structure (gap 1's scan extended to the
  class names each widget emits) is what theme authors write against.
- `widgets` (35) is the right default priority for a CSS base theme, not 50: `RUNTIME` (50) is
  the app's own programmatic overrides (a colour-picker preview, a drag ghost), and a base theme
  should sit under those. `force` (60) remains for the user who wants above them.

### 9.2 Versioning: when are two themes compatible?

Minimal semantic versioning, in the header, on both sides of a dependency:

```css
// theme: abc-base@1.4.0; priority: widgets; azul: 0.2.*, 0.3.*, 0.4
// theme: abc-base:pink@0.3.1; requires: abc-base@^1.2
```

- **major**: variable names removed or renamed, class structure the theme relies on changed,
  a mode block dropped. A spin-off written against `1.x` is not expected to work on `2.x`.
- **minor**: additions only (new variables with fallbacks, new blocks, new widgets covered).
- **patch**: value changes.
- `requires:` (theme to theme) uses caret ranges like Cargo and never gates, see below.
- `azul:` (theme to engine) is **optional** and is the one key that does gate: a comma-separated
  OR list of versions the file applies to, `*` as wildcard (`0.2.*`), plain semver otherwise,
  absent or `*` meaning "apply everywhere". When the running azul matches none of the entries the
  **file is ignored** and one log line says so. The author has explicitly bounded the file, so
  skipping it is the honest behaviour; with the stability commitment above most files will
  simply not carry the key.

Behaviour on mismatch, per the principle "a forced rice is not safe but debuggable and
user-fixable": **apply anyway, never clamp, and attribute**. A major mismatch is a warning in
the log and a line in the rice status ("`abc-base:pink` 0.3.1 requires `abc-base` ^1.2, found
2.0.0"). The user sees exactly why their rice looks off and which file to fix; clamping would
hide the cause behind a silent fallback. Unknown variables already degrade to their `system:`
fallbacks, so a mismatched theme is ugly, not broken.

Debuggable, made concrete: one status listing (About panel and `AZ_DEBUG` output) shows the
chain, every file in it with its priority and version, each `requires` check, and per file how
many rules are live versus inert under the current context. That is the difference between
"my theme is broken" and "rule 12 of `pink.css` is inert because `@os(linux:kde)` is false".

### 9.1 Pitfalls (ways it goes wrong even when built as specified)

1. **Priority does not cross inheritance distance.** Custom properties inherit from the nearest
   definition. If the app sets `--bg` on a panel at `AUTHOR` priority, a `force` rice's
   `:root { --bg }` still loses inside that panel, because the panel's definition is closer.
   Standard CSS, and the first thing every ricer will hit. Document it, and give the lint a
   "variable redefined below `:root` by the app" report so theme authors know which subtrees
   need their own rule.
2. **A mode-blind variable silently breaks dark mode.** A palette file that sets `--accent`
   outside the light/dark blocks defines it once; dark mode gets the light literal. This is the
   very problem `@theme` nesting exists for, reappearing one level up in the user's file. Lint:
   a colour variable set in one mode block must be set in the other or at top level.
3. **Rank-before-specificity surprises CSS people.** `@theme(xyz:pink) { * { color: pink } }`
   flattens every specific rule of `xyz`. Correct by `@layer` semantics, unexpected by habit.
   Lint warning for `*` and bare type selectors in a spin-off above `palette`.
4. **Chain conflicts and cycles.** `xyz:pink` says `fallback: native`, `xyz` says
   `fallback: flora`. Define: the head's header wins, each fallback's own `fallback:` is appended
   transitively with de-duplication, cycles are cut with one warning, app default last.
5. **Reserved names.** `light`, `dark` and the base theme names live in the same set as user
   theme names; a user theme called `dark` collides with the colour scheme and with the legacy
   `AZ_THEME=dark` alias. Reserve them at load time with an error, not a silent shadow.
6. **Startup-only chain versus a live desktop.** Dark mode follows the OS at runtime; the theme
   chain is read once. A user who switches the global theme sees running apps stay put until
   restart, which KDE and GNOME users do not expect. Watch the `.azul` tree (gap 4) and re-run
   `layout()` through the DOM-recreation path.
7. **Global file, one broken app.** A global `widgets`-priority theme applies to every app; the
   letter's L5 returns for users who opt in globally. There must be a per-app way *down*: a
   per-app file with `priority: base` (or `off`) overrides the global file's priority for that
   app. Define global-versus-per-app precedence explicitly: per-app wins, in both directions.
8. **Untrusted input from the theme exchanger.** A downloaded theme is CSS, JSON and SVG going
   into Azul's own parsers. Hardening needed before "unzip into `.azul`" is a feature: no network
   fetches from theme files (`url()` to remote hosts), entity-expansion and size limits in the XML
   and SVG parsers, path traversal checks on `file:` in `remap.json`. CSS cannot execute code,
   but it can exfiltrate through a remote `background` URL if the engine fetches it.
9. **Ricing makes the app slower.** Widgets are inline today partly to avoid selector matching.
   A large `widgets`-priority rice brings class-selector matching back for every node. The
   cascade caches exist; measure with a 500-rule rice on the widgets demo before shipping.
10. **Two colour channels for one icon.** A rice can set `color` on the `<icon>` node (CSS) and
    `recolor` in `remap.json`. Define one order: an explicit JSON `recolor` colour beats the CSS
    `color`; `recolor: currentColor` means "take the CSS colour". Without this the two files
    disagree and nobody can tell which one is live.
11. **The whack-a-mole returns through variables.** Every new axis (theme rank, variable values)
    must be in `cascade_epoch` and the DL/style cache keys, or a rice edit repaints half the
    window. Variables resolved at getter time under the epoch, and the I5 test extended to a
    variable change, before any of this ships.
12. **Attribution as a blame reflex.** "You have a rice" can dismiss a real bug. The
    `AZ_RICING=off` self-check exists so a report can say "reproduces without the rice"; put
    that sentence in the bug-report template the About widget links to.

### 9.3 Why GTK and Qt themes constantly broke: every theme had to be complete

Both toolkits replace the base instead of layering on it, so a theme has no floor and must
cover everything:

- **GTK**: setting `gtk-theme-name` swaps Adwaita out entirely; a theme is a full stylesheet
  for every widget, state and CSS node, and had to be redone whenever GTK renamed nodes (3.20).
  App CSS layers on top by provider priority, but on top of whatever base the user has, so the
  app's additions compose with a floor they were not written for. A half-finished theme falls
  through to GTK's minimal built-in fallback, not to Adwaita, so it looks broken rather than
  plain.
- **Qt Widgets**: a `QStyle` is a complete painter for every primitive and control. `QProxyStyle`
  can layer over a base, but the platform picks one plugin and that plugin must paint
  everything. Breeze's black frame is an unhandled branch with nothing underneath it: a
  completeness failure without fallback.
- **Qt Quick Controls**: one style per process, each a full set of control delegates.
- **KDE colour schemes** are the exception that proves the rule: palette values only, layered
  under a style that keeps painting, never required to be complete, never broke an app.

The chain is the structural fix: every theme sits on a working floor, "complete" stops being a
requirement and becomes a coverage percentage (§7.1), and a missing rule shows the default look
instead of nothing.

**Overall.** With the preconditions (`var()` at cascade level, the theme set and rank key,
`AZ_THEME` migration, light tokens, icon capability metadata) and gaps 1-3 closed, the design
alleviates every problem in the table except the inherent combinatorics of base-theme design,
which it makes testable rather than smaller. What it does that Qt and GTK could not: theming
logic never arrives from the OS, the boundary between "safe" and "unsupported" is a word in the
user's own file, every combination is renderable headless, and a bug report can say which rice
was active. What it cannot do: make a `force` rice safe. The letter was right about that, and
the design says so instead of pretending otherwise.
