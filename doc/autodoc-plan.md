# azul-doc autodoc — design plan

This is the design doc for the documentation pipeline. It captures the
decisions that go beyond the raw group manifest in `autodoc-groups.toml`:
audience model, ordering, maturity tracking, translations, and the
executable-examples strategy.

---

## 1. Goals

1. Generate per-system documentation in parallel without doubling up.
2. Distinguish *external* (users) from *contributor* (azul devs) audiences,
   even when they cover the same system.
3. Treat external docs as a **linear teachable guide** (hello world → ... →
   advanced), not as a flat reference dump. Pages must not introduce
   concepts before their prerequisites.
4. Track **maturity** per page so readers know what's stable vs WIP vs
   stub-only.
5. Keep code samples **executable**: every guide example compiles as a
   doctest, and visual examples are verified via `HeadlessWindow` +
   screenshot diffing.
6. Allow **translations** without forking the source pipeline — English is
   canonical, translations are downstream artifacts.

---

## 2. Audience and content shape

Three distinct content shapes:

### 2a. The Guide (`/guide/...`) — linear, external

A pedagogical path through the **core GUI loop**. Read in order. Each page
introduces ≤ 2 new concepts and depends only on prior pages. Aimed at
someone who just wants to ship a desktop GUI.

Ordering (locked — no forward references):

1. `getting-started` — install + per-language entry
2. `hello-world` — minimal window, App, single Layout callback
3. `architecture` — *the* mental model, kept small (existing file)
4. `dom` — DOM building blocks, IdOrClass, simple click
5. `events` — event filters, hover/focus, keyboard
6. `css` — styling fundamentals (subset; full reference lives elsewhere)
7. `layout` — flexbox + grid by example
8. `text` — text rendering + fonts + i18n
9. `images` — static image, SVG, GPU upload (user-level)
10. `widgets` — built-in widgets, then custom widgets
11. `timers` — Timer types, frame-driven updates
12. `animations` — *requires* timers; covers easing, transitions
13. `scrolling` — scroll regions, programmatic scroll
14. `background-tasks` — Threads, async work, ConnectionStatus
15. `clipboard` — read/write, MIME types
16. `accessibility` — ARIA, screen-reader integration (consumer side)
17. `windowing` — multi-window, monitor handling
18. `web-target` — same code, WASM/SSR backend

### 2b. Topic Guides (`/guide/topics/...`) — standalone, external

Independent of the GUI loop. Read when relevant. No ordering.

- `topics/xml-parsing` — using azul's XML parser as a library
- `topics/css-parser` — using azul's CSS parser standalone
- `topics/headless-render` — server-side rendering, screenshots
- `topics/canvas-2d` — direct draw API
- `topics/font-loading` — programmatic font registration

These exist because azul's parsers and renderer are useful to people who
*aren't* writing a GUI app (e.g., for tooling, CI, layout testing).

### 2c. Internals (`/guide/internals/...`) — flat, contributor

Reference-style. No mandated order. Read when needed. Cross-links freely.
This is the contributor surface from the existing manifest (cascade,
shell2 backends, compositor, ffi-codegen, etc.).

---

## 3. Maturity model

Every output page declares one of:

| level | meaning | rendered as |
|---|---|---|
| `mature` | Public API stable, examples work, won't change without notice | (no badge) |
| `wip` | Works today but API/behavior may shift; examples may need updates | yellow "WIP" badge at top |
| `stub` | Type definitions exist, runtime not wired up. Document the *intent*, mark loudly that it does not yet function. | red "Not Yet Functional" badge at top |
| `draft` | Page exists but content is incomplete (autodoc bailed) | red "Draft" badge |

**No more skipping.** Animation, premature abstractions, etc. all get pages
— they're tagged `stub` so the reader knows. This serves two purposes:
(1) the grouping is comprehensive, (2) when the system matures, the doc
already exists and just needs to be promoted.

The maturity tag also gates **doctest execution**: `mature` pages must
have working doctests; `wip` pages should but failures are warnings not
errors; `stub` pages have their code blocks marked `ignore`.

---

## 4. Translations

**English is canonical.** Translations are downstream.

- File layout: `doc/guide/<slug>.md` is canonical English. Translations
  live at `doc/guide/<slug>.<lang>.md` (e.g. `getting-started.de.md`).
  Keeping language as a suffix avoids a parallel directory tree and makes
  it obvious which pages have which translations.
- URL routing: `/guide/<slug>.html` (English) vs `/<lang>/guide/<slug>.html`
  (translated). The build inspects the file suffix.
- A translation file declares its source revision in frontmatter:
  ```
  ---
  source_rev: a1b2c3d
  source_word_count: 1240
  status: human-reviewed | auto | stale
  ---
  ```
  When the English file changes, all translations are flagged `stale`
  by a CI check that compares git log against `source_rev`.
- **Initial translation pass**: a separate `azul-doc autoreview translate`
  subcommand. Spawns one agent per (page × language) cell, fed the English
  file + the target language code. Output is `auto`-status; humans review
  to promote to `human-reviewed`.
- Code blocks are **never translated** — only prose, headings, and
  comments inside code blocks (and only if the language has an idiomatic
  comment style).
- Out of scope for v1: RTL languages, language selector UI. Get the
  pipeline shape right first.

---

## 5. Executable examples (Rust-doc integration)

**The end state:** every code block in `doc/guide/**.md` runs as part of
`cargo test --doc`. Visual examples render through `HeadlessWindow` and
diff a baseline screenshot.

### 5a. Mechanism — markdown as doctests

The `doc` crate exposes each guide as a module with the markdown included:

```rust
// doc/src/guides.rs (generated, not hand-written)
#[doc = include_str!("../guide/hello-world.md")]
pub mod _guide_hello_world {}

#[doc = include_str!("../guide/dom.md")]
pub mod _guide_dom {}
// ...
```

`cargo test --doc -p azul-doc` compiles and runs every fenced
` ```rust ` block. Standard doctest attributes apply:

- ` ```rust ` — compile + run
- ` ```rust,no_run ` — compile only (for code that needs a real GUI)
- ` ```rust,ignore ` — skip (for `stub`-tagged pages)
- ` ```rust,should_panic ` — when teaching error cases
- Hidden setup lines (`# use azul::*;`) work as usual.

This requires nothing new — it's vanilla rustdoc.

### 5b. Visual proof — HeadlessWindow + screenshot diff

A new convention for visual examples:

```rust
/// ```rust
/// # use azul::*;
/// # use azul::testing::*;
/// let dom = Dom::body().with_child(
///     Dom::text("Hello, world!").with_inline_style("font-size: 32px;")
/// );
/// assert_screenshot_matches!(dom, "hello-world.png");
/// ```
```

`assert_screenshot_matches!` is a macro that:
1. Builds a `HeadlessWindow` with default 800×600.
2. Runs one layout + paint pass.
3. Compares pixel hash against `doc/guide/screenshots/hello-world.png`.
4. On mismatch in CI: fails. On mismatch locally with `BLESS=1`: rewrites
   the baseline.

Baseline screenshots are committed to the repo (small PNGs ~10–50KB).
When a guide page is regenerated by autodoc and its screenshot changes,
the diff is visible in the PR — you can see if the *teaching example*
actually broke.

This is the "the docs prove themselves" property. A page that can't
render its own screenshot is by definition wrong.

### 5c. Authoring rules for autodoc agents

When an agent writes a guide page, it must:

1. Use only concepts from earlier pages in the linear guide order.
2. Declare every example as a fenced block with a language tag.
3. For visual examples: include the `assert_screenshot_matches!` line
   and write the expected baseline at `doc/guide/screenshots/<slug>-<n>.png`
   using `HeadlessWindow::render_dom` from a helper script in the
   autodoc post-step.
4. Tag the page maturity (`mature` / `wip` / `stub` / `draft`) in the
   markdown frontmatter.
5. Never copy code that doesn't compile against the *current* tree —
   if the agent isn't sure, mark `ignore` and add a TODO.

The autodoc post-step runs `cargo test --doc` after all agents finish
and rejects any guide that fails to compile. Failed guides are
tagged `draft` and committed anyway (so progress isn't lost), with a
follow-up task to fix.

---

## 6. Schema additions to `autodoc-groups.toml`

The existing manifest needs these fields:

```toml
[[group]]
id = "..."
audience = ["external" | "contributor"]
agent_strategy = "single" | "split" | "dual"

# NEW: maturity defaults to "mature"; can be overridden per-output
maturity = "mature" | "wip" | "stub" | "draft"

# NEW: linear guide ordering. Only meaningful for external/non-topic pages.
# Lower = earlier. Sparse integers (10, 20, 30, ...) so insertion is cheap.
guide_order = 30

# NEW: pages that aren't part of the linear path
topic_only = true  # → routes to /guide/topics/<slug>

# NEW: prerequisites — if set, autodoc agents are told they may assume
# these concepts are already understood. Agents must NOT use anything
# outside this transitive closure.
prerequisites = ["getting-started", "hello-world", "architecture"]

# NEW: visual examples that need screenshot baselines
visual_examples = [
  { slug = "minimal-button", baseline = "screenshots/dom-minimal-button.png" },
]

[[group.outputs]]
slug = "..."
path = "..."
title = "..."
audience = "external"
maturity = "wip"  # overrides group default
```

---

## 7. Implementation phases

Numbered for ordering; each phase commits independently.

1. **Manifest schema** — add the new fields to `autodoc-groups.toml`,
   reorganize the linear guide list with `guide_order`, retag the
   currently-skipped groups (animation, etc.) as `stub`. *No code yet.*

2. **Tree navigation refactor** — replace `get_guide_list()` with a
   `SUMMARY.md`-driven walker. `SUMMARY.md` is hand-edited (or generated
   from the manifest's ordering). This is a prerequisite for everything
   else; do it before the first autodoc run so generated guides land in
   the navigation automatically.

3. **`autoreview autodoc` subcommand (English only)** — reads the
   manifest, dispatches one agent per group (file-locking pattern from
   `midlevel-fixes`), agents read source files + relevant slices of
   `reference.md`, write markdown with frontmatter (maturity tag,
   prerequisites, source_rev). Post-step regenerates `SUMMARY.md`.

4. **Doctest integration** — generate the per-guide `#[doc =
   include_str!]` modules in `doc/src/guides.rs` from the manifest.
   Wire `cargo test --doc -p azul-doc` into CI. Failing doctests block
   merges; `stub`/`draft` pages have `ignore` blocks.

5. **`HeadlessWindow` + screenshot harness** — implement
   `assert_screenshot_matches!`, baseline storage layout, the `BLESS=1`
   workflow. Adapt the existing reftest harness if possible.

6. **High-level fixes crossover analysis** (separate from autodoc but
   shares the system map) — emit `doc/target/autoreview/highlevel/
   crossovers.md` listing HIGH-severity findings that touch ≥ 2 systems.

7. **Translation pipeline** — `autoreview translate --lang=de`
   subcommand. Per-page translation agent + the staleness checker.

Phases 1–4 are the hard prerequisites. 5 is high-impact but slow to
build right. 6 is independent and can run any time. 7 is meaningful only
once phases 1–4 are stable.

---

## 8. Open questions

- **`SUMMARY.md` ordering — derived from manifest, or hand-edited?**
  Hand-edited is simpler; derived avoids drift. Lean toward derived
  with hand-override-via-frontmatter capability.
- **Screenshot determinism on different platforms.** Rendering
  pixel-identical screenshots on macOS, Linux, Windows is unrealistic.
  Use perceptual diff (e.g., DSSIM threshold) rather than exact match,
  or pin the harness to one platform in CI.
- **Auto-translation budget.** Each page × N languages = 2k+ agent runs
  for a serious l10n effort. Realistic strategy: prioritize the linear
  guide for translation, leave internals English-only.
- **What counts as "core GUI"** for the linear path? The current list of
  18 pages is a working draft. Cut to 12 if any feel optional — a
  shorter learnable spine beats completeness.

---

## 9. What this plan does *not* commit to

- A specific markdown extension dialect (CommonMark + tasklists +
  footnotes are already in use; keep that).
- A static-site generator. The existing `azul-doc` HTML pipeline stays.
- mdBook adoption. We're using its mental model (`SUMMARY.md`, ordered
  pages, doctests) without the tool itself.
