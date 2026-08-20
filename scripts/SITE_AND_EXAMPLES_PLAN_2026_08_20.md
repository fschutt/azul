# Site + examples plan — 2026-08-20

Everything queued during the website-deploy session. Written down because the
deploy was mid-flight and the tree was locked; none of this is committed work
yet unless it says DONE.

Design source for the widget work:
`https://claude.ai/design/p/38310714-1446-40d3-895c-8fa32c396b47?file=Azlin+Interface+Specimen.dc.html`
— readable through the `DesignSync` MCP (`projectId 38310714-1446-40d3-895c-8fa32c396b47`),
files `Azlin Interface Specimen.dc.html` (95 KB) + `support.js`. Plain `WebFetch`
403s on it; use DesignSync. It is the Flora vocabulary: EB Garamond / Grenze
Gotisch, `#E9E2D2` parchment, skeuomorphic scrollbars — the same system as
`doc/templates/flora.css`.

---

## 1. Examples overhaul (queued behind the comment-stripping agent)

The /ui front page examples are not representative of what the engine can do.
Screenshots are taken of the **C** examples (`scripts/screenshot_single.sh` uses
`examples/c/<name>.c`), so changes must land there to show up on the site.

| Example | Change | Notes |
|---|---|---|
| `widgets` | Ribbon at the TOP, replacing the "Azul Widget Showcase" heading/subtitle | Ribbon already exists (`layout/src/widgets/ribbon.rs`); this avoids needing a `ribbon.c` or teaching the harness to capture a Rust binary |
| `infinity` | Virtualised **fake-Excel grid** — column headers (A/B/C…), row numbers, cell borders, alternating fills | Shows 2D layout capability AND has zero asset dependencies. Reuses the `VirtualView` scroll math already in `infinity.c` |
| `xhtml` | Load the real Excel document | See §2 |
| `async` | Slippy map | Replaces the DB-connection mock |
| `opengl` | DONE — fixed by asset staging (`35746bd20`) | |
| `icons` | Icon-system showcase | Its `../assets/images/favicon.ico` load also fixed by `35746bd20` |

Longer term: rewrite `widgets` to the **Azlin Interface Specimen**. For now just
show the existing widgets working; the specimen port comes after the theme work
in §3.

### Evidence collected (so nobody re-derives it)

- `opengl` "doesn't load its data": the harness copied only DLL + header + `.c`
  into `target/examples-temp/<example>/`. Examples resolve assets as
  `../assets/...`, i.e. `target/examples-temp/assets`, which nothing created.
  Only `opengl.c` (`testdata.json`) and `icons.c` (`favicon.ico`) read assets.
  FIXED in `35746bd20`.
- `infinity` "is blank": NOT a code bug. The shipped PNG is titled "Infinite
  Gallery - 1000 images", which no current example produces (`infinity.c` says
  "VirtualView Test - N virtual rows", `infinity.rs` says "Pictures - N images").
  It is a screenshot of a deleted gallery version whose images never loaded.

## 2. A real `excel.xhtml`

`doc/working/excel.html` — tracked, 805 lines, 19 Aug, a full Excel mockup
(titlebar, ribbon tabs, ribbon groups, grid). Verified WELL-FORMED XML, so the
engine can load it as-is.

`examples/rust/src/xhtml.rs` instead includes `examples/rust/assets/spreadsheet.xhtml`,
which is one line:
`<html><body><h1>Test XHTML</h1><p>This is a test spreadsheet.</p></body></html>`
(`examples/assets/spreadsheet.xhtml` is an even shorter stub.)

- Point the example at the real document.
- Make a parse failure VISIBLE. It currently does
  `ResultXmlXmlError::Err(_) => Dom::create_body()` — a blank window and no
  message, which is how a broken XHTML example would look identical to a working
  one.

## 3. Widget themes — `layout/src/widgets/themes/{flora.rs,flat.rs}`

### The shape (decided 2026-08-20)

NOT a styling abstraction over one DOM. Per-theme **DOM builders**: the same
logical component produces a different tree with entirely different CSS.

    flora::button(dom) -> Dom
    flat::button(dom)  -> Dom

The Flora orb is the argument for this — its depth rig is five nested spans
(`fl-orb-well` / `-well-shadow` / `-well-bounce` / `-collar` / `-stone` /
`-gloss` / `-edge`); a flat theme needs one box. No property-level theming can
bridge that, so the theme owns the markup.

Hardcoded `CssProperty` values stay hardcoded — that is intentional and is not
what changes. What changes is WHERE they are defined: today 51 widgets each
carry their own literals (76 distinct `ColorU { r, g, b }` across
`layout/src/widgets/*.rs`), so a colour cannot be shared and a theme cannot be
coherent. The definitions MOVE into the theme files so one palette serves a
whole theme.

### The two themes

- **`flora.rs`** — matches `doc/templates/flora.css`, i.e. the redesigned
  website. That CSS is real, shipped and already debugged in a browser, and it
  is the Azlin Interface Specimen expressed as HTML/CSS. So it doubles as a
  CONFORMANCE TARGET: azul's rendering of `flora::*` should match what the
  browser does with `flora.css`. 127 `--fl-*` tokens under `:root` are the
  palette source of truth (`--fl-pg/-sur/-desk/-strip`, `--fl-bd..bd5`,
  `--fl-ink/-ink2/-soft1..3`, `--fl-acc/-deep/-glow`, `--fl-gem`, `--fl-band`,
  `--fl-shadow-1/-2`, …).
- **`flat.rs`** — the current Office-2013 AzWriter style, already present in
  `layout/src/widgets/ribbon.rs`: `#2B579A` Word blue, `#1E3E6F` pressed,
  `#444444` / `#676767` ink, `#D4D4D4` rules.

### Zero allocation: const items, not built strings

The theme building blocks are `const` items, so a themed widget allocates
nothing for its styling. This is NOT new machinery — `ribbon.rs` already proves
the idiom in-tree:

    const W13_BLUE: ColorU = ColorU { r: 43, g: 87, b: 154, a: 255 };
    const SYSTEM_UI_STR: AzString = AzString::from_const_str("system:ui");
    const SYSTEM_UI_FAMILIES: &[StyleFontFamily] = &[StyleFontFamily::System(SYSTEM_UI_STR)];
    const SYSTEM_UI_FAMILY: StyleFontFamilyVec = ...;

`flat.rs` is therefore largely a LIFT of ribbon.rs's `W13_*` constants into a
shared module — they are already the Office-2013 palette, already const.

### The public theme API

A set of building blocks out of which the individual widgets (checkbox, slider,
combobox, …) compose their visual style — optionally taking a font. The widgets
do not hardcode a look; they ask the theme for its pieces and assemble.

Crucially the theme owns the WRAPPER STRUCTURE too, not just colours. Flora
needs decorative wrapper divs — clip masks, cove/collar layers, gloss caps — and
`flat` needs none of them. So `flora::checkbox` may emit four nested divs where
`flat::checkbox` emits one. A theme that needs no decoration pays for no
decoration.

### Flora's decorations already work in a browser

Flora is full of decorative divs with clip masks, and they render CORRECTLY in a
browser today — visible on the /ui tab of the deployed site. That makes the port
a copy rather than a design exercise, and it makes every visual mismatch
INFORMATIVE: if azul renders `flora::*` differently from the browser rendering
of the same construction, that is an ENGINE BUG to fix, not a theme to tweak.
Expect this step to surface real layout/paint bugs; that is a feature of doing
it this way, not a setback.

### C API

Widgets carry an `OptionTheme`, so styling stays consistent by default while the
user can override per widget. That is an FFI surface change: it goes through
`api.json` and codegen, and per the standing rule it must be synced with
`azul-doc autofix` commands ONLY — never hand-edited patches, never broad globs.

### Order of work

1. Extract the two palettes into `themes/flora.rs` + `themes/flat.rs` as const
   items (lift ribbon.rs's `W13_*` for flat; take flora's 127 `--fl-*` tokens).
2. Define the public theme API — the building blocks, optional font, and the
   per-theme wrapper/decoration structure.
3. Add `themes/mod.rs` and the `OptionTheme` plumbing (autofix → api.json →
   codegen).
4. Move widgets over one at a time, flora first, diffing each against the
   browser's rendering of the same construction in `flora.css`. Fix the ENGINE
   where they disagree.
5. Native screenshot of the result (`AZ_DEBUG=<port>` + `{"op":
   "take_native_screenshot"}`) for real macOS window chrome.
6. Update the C `api.json` (autofix only).
7. Rewrite the examples (§1) on top of the finished themes.
8. Final /ui showcase page built from all of it.

## 4. Guide `azul-render` blocks: whitespace-only text warnings

`azul-doc deploy` emits, from the guide's own figures:

    [azul][text-without-block] WARNING: text node 4 ("\n    ") is one of 5 items
    in a Flex container ...

The offending nodes are `"\n  "` / `"\n    "` — **whitespace only**, i.e. the
pretty-printing of the XML in the markdown block.

CSS Flexbox §4 and Grid §6 both say an anonymous item containing only white
space **is not rendered**. So browsers create no box there and the author has
nothing to fix — the warning is a false positive as written.

BUT the message says "is one of 5 items in a Flex container", which suggests the
engine may be COUNTING the whitespace run as a flex item. If so, that is a real
layout bug (phantom items change `space-between`, `flex-grow` shares, item
indices), not just noisy logging.

Decide between:
- (a) lint-only: `is_text` in `layout/src/dom_lint.rs:117` should not match
  whitespace-only text. Cheap, but silences a warning that might be pointing at
  a real bug.
- (b) layout: ignore whitespace-only text runs when collecting flex/grid items,
  per spec. `layout/src/solver3/cache.rs` already has
  `is_whitespace_only_text` / `is_whitespace_only_inline_run` used for table
  structural fixups and inline runs — check whether the flex/grid item
  collection path uses them.

**Do (b) if the engine really creates an item; (a) is not sufficient on its own.**
Verify empirically first: lay out a flex container with and without
pretty-printing whitespace and compare item boxes.

## 5. Screenshots

- DONE `5df7175f1`: CI screenshot steps no longer gated on the DLL build cache
  (they were skipped on every cache hit), `if-no-files-found: warn`, the deploy's
  fallback is a `::warning::`, and the macOS rename bug is fixed
  (`-macos` → `.mac`, not `.macos`).
- DONE `35746bd20`: assets staged for example binaries.
- The committed `examples/assets/screenshots/*.png` (3–7 Jan) were an
  INTENTIONAL manual stand-in because CI had no native screenshot. Now that the
  pipeline runs, regenerate them — the reftest engine shows the layout
  capabilities are far beyond what those images depict.
- macOS window chrome: run an example with `AZ_DEBUG=<port>` and POST
  `{"op": "take_native_screenshot"}` (the wire enum is snake_case, tagged `op` —
  `layout/src/e2e/full.rs`, `DebugEvent::TakeNativeScreenshot`).
- There is no `hello-world.linux.png` in the repo at all, so linux/windows fell
  back to the **mac** image. All three live URLs currently serve the same
  162764-byte January file.

## 6. Guide figures

- DONE `66b7b5ebd`: cache removed, every figure renders on every deploy (5.9 s
  for 27), fatal on failure.
- DONE `eabfa175c`: rasterised at 2× (`SCREENSHOT_DPI_FACTOR`), `<img>` keeps the
  logical size, so retina stops upscaling. `AzulPixmap::width/height` became
  `pub`.
- TODO: drop the 26 committed PNGs and gitignore them. CI has now been observed
  rendering all 27 green (`build_website_skeleton`, run 32388074182), so the
  fallback they provided is no longer needed.

## 7. Loose ends worth fixing

- `azul-doc deploy` prints, five times:
  `WARNING — 9 non-Copy FFI type(s) own a heap resource ... will DOUBLE-FREE when
  nested in another Az wrapper and dropped by value`. A memory-safety warning
  shouting into a build log that nobody acts on. Either fix the 9 types or make
  it fail something.
- sccache: deliberately NOT enabled (rationale at `.github/workflows/rust.yml`
  ~line 2478). Worth revisiting WITH a before/after on the three DLL builds,
  whose differing feature sets make the hit rate an empirical question.
- ~430 duplicated E2E step lines could become a composite action.

## 8. Push queue (nothing pushed while the deploy runs)

Local commits, in order:

1. `62cb966f1` ci: `build_dll_e2e` moved to the preflight tier (it was queuing
   12 min behind Coverage/rust9x/Cross-Build at the 20-job concurrency ceiling)
2. `429eb301f` favicon = the site's own orb (it was the literal ASCII string
   "Favicon placeholder")
3. `66b7b5ebd` guide figures render every deploy — PUSHED, in the running deploy
4. `eabfa175c` figures at 2×
5. `5df7175f1` CI screenshots run + loud fallbacks
6. `35746bd20` example binaries get their assets

Plus the test-coverage agent's verified work, still uncommitted: core `url`
0→35 tests, workspace members 0→115 (webrender 96, azul-writer 16, azul-paint 3),
css `io` 0→1, 28 `#[ignore]`s triaged, zero-test guard on 12 CI steps, and a real
latent fix (`take_screenshot_of_node` was gated `std+cpurender` but calls
`dialogs::report::crop_png` which needs `widgets+text_layout`, so azul-layout did
not compile at all under the glyph-rasteriser's feature set).
