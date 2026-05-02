# azul-doc autodoc — operational guide

Operational reference for the documentation pipeline built into `azul-doc`.
This document covers **what exists today and how to use it**.

---

## 1. The full flow

```
   ┌─ source code (.rs)
   │       │
   │       ▼
   │   autoreview        ── one Claude agent reviews each .rs file,
   │                        writes doc/target/autoreview/reports/
   │       │
   │       ▼
   │   reference.md      ── one section per source file, captures
   │                        the "what doc is needed" fields from
   │                        each report. Hand-curated grouping
   │                        proposal lives in autodoc-groups.toml.
   │       │
   │       ▼
   │   autodoc           ── one agent per group writes guide pages
   │                        to doc/guide/en/<slug>.md with YAML
   │                        frontmatter. Pages embed `azul-render`
   │                        fenced blocks for visual examples.
   │       │
   │       ▼
   │   autodoc-screenshots
   │                     ── headless cpurender of every fence into
   │                        doc/guide/en/screenshots/<name>.png
   │                        plus screenshots/manifest.json.
   │       │
   │       ▼
   │   translate (TODO)  ── per-language agent runs that copy the
   │                        English files into doc/guide/<lang>/
   │                        with `canonical_slug` + `source_hash`.
   │       │
   │       ▼
   │   autodoc-check     ── pre-deploy gate. Two signals:
   │                          • canonical pages: tracked source files
   │                            changed since last_generated_rev
   │                          • translations: SHA-256 body hash of
   │                            canonical no longer matches recorded
   │                            source_hash → build fail
   │       │
   │       ▼
   │   deploy            ── docgen walks doc/guide/en/ at runtime,
   │                        renders comrak HTML with `azul-render`
   │                        fences expanded to <figure>/slideshow,
   │                        copies screenshots/ into <deploy>/.
   ▼
website
```

---

## 2. Commands

All under `azul-doc autoreview <subcommand>`. The full list:

| subcommand | purpose |
|---|---|
| `autoreview` | (existing) one agent per .rs file produces reviews into `doc/target/autoreview/reports/` |
| `merge` | (existing) consolidate reports into a checklist |
| `small-fixes` | (existing) parallel agents apply minor fixes per-file |
| `midlevel-fixes` | (existing) sequential agents apply cross-file refactors |
| `apply-midlevel` | (existing) interactive replay of commits from a reference branch |
| **`autodoc`** | one agent per group from the manifest writes guide pages |
| **`autodoc-screenshots`** | render every `azul-render` fence to PNG; emit `manifest.json` |
| **`autodoc-check`** | detect canonical pages with stale source files and translations whose body hash no longer matches; pass `--strict` to exit non-zero on any staleness (used in CI) |

Common flags: `--agents=N`, `--file=GROUP_ID` (filter by group id),
`--dry-run`, `--retry-failed`, `--timeout=SECS`, `--model=NAME`,
`--strict` (autodoc-check only — fail the process on stale pages).

### Typical local workflow

```sh
# 1. Generate per-system guide pages (one agent per group, run in parallel)
azul-doc autoreview autodoc --agents=10

# 2. Render visual examples
azul-doc autoreview autodoc-screenshots

# 3. Verify nothing's stale before pushing
azul-doc autoreview autodoc-check

# 4. Build the site
azul-doc deploy
```

### Iterating on a single group

```sh
# Re-run only the "scrolling" agent
azul-doc autoreview autodoc --file=scrolling

# Re-render only its screenshots (still walks all files;
# unchanged PNGs overwrite identically)
azul-doc autoreview autodoc-screenshots
```

---

## 3. The manifest — `doc/autodoc-groups.toml`

Hand-curated. Source of truth for what gets documented and how it's
audience-targeted, ordered, and tracked.

### Top-level `[meta]`

```toml
[meta]
schema_version       = 3
existing_guides      = ["architecture", "reference"]   # pages NOT regenerated
shared_context_files = ["doc/AUTODOC.md", "doc/autodoc-groups.toml", ...]

[[meta.trees]]
id          = "getting-started"
title       = "Getting Started"
audience    = "external"
description = "Core GUI authoring — what every app needs."

[[meta.trees]]
id          = "advanced"
title       = "Advanced"
audience    = "external"
description = "Topics beyond core GUI authoring — debugging, profiling, I/O, ..."

[[meta.trees]]
id          = "contributor"
title       = "Contributors"
audience    = "contributor"
description = "Internal architecture, by sub-system."

[meta.writing_style]
reference     = "https://book.servo.org/contributing/guides/index.html"
reader_model  = "Senior systems programmer evaluating azul..."
voice         = "Direct, technical, second-person ('you/your')..."
length_target = "Soft target 800-1500 words; hard cap 4000."
avoid         = ["throat-clearing", "marketing terms", ...]
prefer        = ["fact-first sentences", "tables", ...]

[meta.agent_thinking]
mode         = "max-effort"
instructions = ["Think for at least N tokens before writing", ...]
```

### Per-group structure

```toml
[[group]]
id              = "scrolling"           # unique, used as prompt filename
tree            = "getting-started"     # which top-level tree this lives in
audience        = ["external"]          # or ["contributor"] or both
agent_strategy  = "single"              # | "split" | "dual"
tracked_files   = ["layout/src/hit_test.rs", "core/src/scrolling.rs"]
tracked_globs   = ["dll/src/desktop/shell2/*/scroll*.rs"]
design_docs     = ["SCROLL_ARCHITECTURE.md"]   # files in scripts/ — INTENT, not truth
notes           = "..."                 # optional, shown in prompt

[[group.outputs]]
slug            = "scrolling"           # URL slug (localized for translations)
path            = "doc/guide/en/scrolling.md"
title           = "Scrolling"
audience        = "external"            # only when group is dual
maturity        = "wip"                 # mature | wip | stub | draft (default mature)
guide_order     = 130                   # position in linear external guide
topic_only      = false                 # true → standalone topic page
prerequisites   = ["events"]            # canonical slugs of earlier pages
```

### `tree` and `design_docs`

- **`tree`** assigns the group to one of the `[[meta.trees]]` buckets. The
  guide index renders one section per tree.
- **`design_docs`** lists files in `scripts/` (the design-memo folder).
  These are passed to the agent as **INTENT** context — the original
  design proposals — *not* as authoritative source. The disclaimer
  rendered into the prompt is **the code is truth, the design docs are
  context**. Out-of-date design docs are common; agents should defer to
  the source files in `tracked_files` if the two disagree.

### Strategies

- **`single`**: one output file. Most common.
- **`split`**: multiple output files for the same audience (e.g.,
  `shell2-backends` writes `shell2-{common,linux,windows,macos}.md`).
- **`dual`**: writes both an external and a contributor page in one
  agent run (shares read context — the agent reads the source files
  once and frames them differently per audience).

### Adding a new system

1. Add a `[[group]]` entry with stable `id`.
2. List `tracked_files` and `tracked_globs`.
3. Declare each `[[group.outputs]]` with path under
   `doc/guide/en/<slug>.md` and the maturity tag.
4. If external + linear-spine, assign a `guide_order` (use a sparse
   integer; insertions stay cheap) and `prerequisites`.
5. `azul-doc autoreview autodoc --file=<id>` to dispatch its agent.

---

## 4. Markdown frontmatter

Every page autodoc generates begins with YAML frontmatter:

### Canonical English page

```yaml
---
slug: scrolling
title: Scrolling
language: en
canonical_slug: scrolling             # same as slug for English
audience: external
maturity: wip
guide_order: 130
topic_only: false
short_desc: One-line summary of this page, rendered indented under the
  link in the guide index. Hand-authored per page (not extracted from
  prose); localised per language.
prerequisites: [events]
tracked_files:
  - layout/src/hit_test.rs
  - core/src/scrolling.rs
last_generated_rev: 7817088bb...      # git SHA of HEAD when generated
generated_at: 2026-05-01T17:30:00Z
---
```

### Translation page

```yaml
---
slug: scrollen                        # localized URL slug
title: Scrollen
language: de
canonical_slug: scrolling             # English page this translates
audience: external
maturity: wip
guide_order: 130                      # copied from canonical
topic_only: false
prerequisites: [events]               # canonical (English) slugs;
                                      # website resolves to localized URLs
source_rev: <SHA of doc/guide/en/scrolling.md when translated>
source_hash: <SHA-256 of canonical body when translated>
generated_at: 2026-05-01T17:30:00Z
---
```

Translations **omit** `tracked_files` / `last_generated_rev` — staleness
flows from the canonical file via `source_hash`.

---

## 5. Visual examples — `azul-render` fences

Agents emit XHTML inside fenced blocks; the screenshot harness renders
each to PNG, the HTML preprocessor expands the fence into figures or
slideshows.

### Single screenshot

```
​```azul-render screenshot=hello-world width=400 height=200 subtitle="The classic output"
<body><p style="font-size: 24px; padding: 20px;">Hello, world!</p></body>
​```
```

Renders to `doc/guide/en/screenshots/hello-world.png` and inlines as:

```html
<figure class="azul-screenshot">
  <img src="/guide/screenshots/hello-world.png" width="400" height="200" loading="lazy"/>
  <figcaption>The classic output</figcaption>
</figure>
```

### Slideshow (sequence of frames)

Multiple consecutive blocks with the same `slideshow=ID` group into one
widget. Frame order = source order.

```
​```azul-render screenshot=scroll-1 slideshow=scroll-demo subtitle="Initial — scroll position 0"
<body>...</body>
​```

​```azul-render screenshot=scroll-2 slideshow=scroll-demo subtitle="After scrolling 100px"
<body>...</body>
​```

​```azul-render screenshot=scroll-3 slideshow=scroll-demo subtitle="At the bottom"
<body>...</body>
​```
```

Renders as:

```html
<div class="azul-slideshow" data-name="scroll-demo">
  <figure class="azul-slide" data-frame="0">
    <img src="/guide/screenshots/scroll-1.png" .../>
    <figcaption>Initial — scroll position 0</figcaption>
  </figure>
  <figure class="azul-slide" data-frame="1">...</figure>
  <figure class="azul-slide" data-frame="2">...</figure>
</div>
```

The website's stylesheet/JS makes the slideshow interactive (prev/next
buttons, autoplay) — that part lives in the templates, not in autodoc.

### Attribute reference

| attr | meaning | default |
|---|---|---|
| `screenshot=NAME` | unique PNG filename (no extension) — required | — |
| `width=N` | render canvas width in CSS px | 800 |
| `height=N` | render canvas height in CSS px | 600 |
| `subtitle="..."` | caption rendered as `<figcaption>` (quote if it has spaces) | empty |
| `slideshow=ID` | groups consecutive blocks under the same id | none (single image) |

The block body is XHTML — same dialect the reftest harness consumes
via `azul_layout::xml::parse_xml_to_styled_dom`. Bare snippets, body
fragments (`<body>...`), and full documents (`<html>...`) all work; the
harness wraps as needed.

---

## 6. Maturity tags

Every output declares one. Affects badge rendering and doctest behaviour.

| tag | meaning | doctest behaviour |
|---|---|---|
| `mature` | Public API stable; examples must compile and run | strict |
| `wip` | Works today but API may shift | warnings |
| `stub` | Type definitions exist; runtime not wired up | code blocks marked `ignore` |
| `draft` | Page exists but content is incomplete | as-is |

**No skipping.** Immature systems (e.g., the animation runtime is
defined but not wired through layout) get a `stub`-tagged page so the
doc structure is complete; the page becomes truthful on first
re-generation once the runtime lands.

---

## 7. The outdated check

`autodoc-check` is the pre-deploy gate. Walks every page under
`doc/guide/`, parses frontmatter, applies the appropriate check.

### Canonical English pages

Iterates `tracked_files`. For each, runs:

```
git log <last_generated_rev>..HEAD --format=%H -- <file>
```

If the result is non-empty, the page is stale. The report at
`doc/target/autoreview/autodoc/outdated.md` lists the page, the file,
and the offending commits — actionable for `autoreview autodoc --file=<id>`.

### Translation pages

Two signals, evaluated in order:

1. **Body hash check** (authoritative): re-hashes the body of
   `doc/guide/en/<canonical_slug>.md` (everything after the closing
   `---\n` of the frontmatter) with SHA-256 and compares to the
   translation's recorded `source_hash`. Mismatch → translation is
   stale and the build fails.
2. **Git rev check** (informational): runs `git log <source_rev>..HEAD
   -- doc/guide/en/<canonical_slug>.md`. Surfaces no-op commits
   (whitespace, frontmatter-only updates) so a human can ack and bump
   `source_rev` without re-translating.

Body-hash drift is the only signal that fails the build. Git-rev drift
without hash drift is just a warning.

### Wiring `autodoc-check` into CI

Pass `--strict`. The check exits non-zero on any staleness (canonical or
translation), prints the offending pages, and fails the build:

```yaml
# .github/workflows/rust.yml (excerpt)
- name: Verify guide is up to date
  run: cargo run --manifest-path doc/Cargo.toml --release -- \
       autoreview autodoc-check --strict
```

Without `--strict` the command writes the report to
`doc/target/autoreview/autodoc/outdated.md` and exits 0 — useful for
local "what's stale right now" runs.

---

## 8. Translation workflow

> **Status:** the directory layout and frontmatter schema are in place;
> the `translate` subcommand that automates per-page translation is
> still a TODO. The manual steps below show what the automated flow
> will replicate.

### Manual translation today

To translate `doc/guide/en/architecture.md` into German:

1. Read the canonical file's body (everything after the YAML
   frontmatter closing `---`).
2. Compute its SHA-256 (`shasum -a 256 < body.bin`).
3. Note the current git SHA (`git rev-parse HEAD`).
4. Write `doc/guide/de/architektur.md` with the localized URL slug
   in `slug:`, `language: de`, `canonical_slug: architecture`, the
   recorded `source_rev`, the recorded `source_hash`, and the
   translated body.
5. Commit. The next `autodoc-check` will report the translation as
   fresh.

### When the English page is updated

1. Re-run `autodoc --file=architecture-overview` (or hand-edit).
2. The body hash changes.
3. `autodoc-check` flags `doc/guide/de/architektur.md` as stale —
   build fails.
4. Update the German translation; record the new `source_rev` and
   `source_hash`.
5. Build passes.

### URL routing

Every language gets the same routing scheme: `/<lang>/<slug>`.
- English: `/en/architecture`
- German: `/de/architektur` (note the localized slug)

The website joins translations to their canonical via `canonical_slug`
to build the language-switcher links. Currently routing is unified —
the website still emits `/guide/<slug>.html` for English-only, and
will be flipped to `/<lang>/...` when a second language lands.

---

## 9. File layout

```
doc/
├── autodoc-groups.toml         ← manifest (committed, hand-curated)
├── AUTODOC.md                  ← this document
├── guide/
│   ├── en/                     ← canonical English (committed)
│   │   ├── architecture.md
│   │   ├── reference.md        ← per-source-file backlog
│   │   ├── <slug>.md           ← agent-generated guide pages
│   │   ├── <parent>/<child>.md ← nested pages render as a sub-tree
│   │   │                         under <parent>.md in the index
│   │   ├── internals/<slug>.md ← contributor-audience pages
│   │   └── screenshots/
│   │       ├── manifest.json   ← per-language metadata (subtitles, slideshows)
│   │       └── *.png           ← rendered visual examples
│   └── <lang>/                 ← future translations (de/, fr/, ja/, ...)
│       ├── ...
│       └── screenshots/        ← per-language because PNGs bake in text
└── target/
    └── autoreview/
        ├── reports/            ← one .md per source file
        ├── merge/              ← consolidated checklist (after `merge`)
        ├── small-fixes/        ← per-file prompts + sentinels
        ├── midlevel-fixes/     ← per-finding prompts + sentinels
        └── autodoc/
            ├── prompts/        ← per-group prompts + .taken/.done/.failed sentinels
            └── outdated.md     ← `autodoc-check` report
```

What's tracked in git: `doc/autodoc-*.{toml,md}`, `doc/AUTODOC.md`,
all of `doc/guide/`. What's gitignored: `doc/target/`.

---

## 10. Source-of-truth invariants

A few rules the pipeline enforces:

- **One canonical English page per system.** No duplicate slugs in
  the manifest; `autodoc` validates this on load.
- **Output paths are language-prefixed.** Every path in the manifest
  starts with `doc/guide/<lang>/`; deploy infers the language from the
  path component after `doc/guide/`.
- **Translations point at canonical via `canonical_slug`**, not via
  filename. A German page named `architektur.md` is a translation of
  `architecture` because its frontmatter says so, not because of the
  filename.
- **Prerequisites are canonical slugs.** A German page reading
  `prerequisites: [hello-world]` links to the German "hallo-welt"
  page — the website resolves canonical → localized at render time.
- **Body hash includes the body only**, not frontmatter — so updating
  `last_generated_rev` doesn't invalidate translations. Only edits
  to the prose itself trigger re-translation.

---

## 11. Extension points (still to build)

Tracked here so they don't get lost. None of these block the current
end-to-end flow.

- **`autoreview translate --lang=de`** — automate the manual
  translation steps in §8. Per-page agent given the canonical file +
  glossary. Output `auto`-status; humans review to promote to
  `human-reviewed`.
- **Per-language URL routing** in the deploy step (`/<lang>/guide/<slug>.html`).
- **Per-language guide index page** (`<lang>/guide.html`).
- **Language switcher widget** in the page template.
- **Doctest integration**: generate `doc/src/guides_doctest.rs` from
  the manifest with `#[doc = include_str!]` per page, gated by the
  maturity tag (mature → strict, wip → warn, stub → ignore). Requires
  a `lib.rs` target on the `doc` crate.
- **Slideshow CSS/JS** in the website template.

---

## 12. Quick reference card

```
# Generate (parallel agents, ~10–60 min depending on # of groups + load)
azul-doc autoreview autodoc

# Re-generate one group
azul-doc autoreview autodoc --file=<group-id>

# Render PNGs from azul-render fences
azul-doc autoreview autodoc-screenshots

# CI gate (pass --strict to fail on any staleness)
azul-doc autoreview autodoc-check --strict
# → exits 0 if everything fresh; report at doc/target/autoreview/autodoc/outdated.md

# Deploy
azul-doc deploy             # production (inline CSS)
azul-doc deploy debug       # debug (external CSS, faster)
```

```
# Files an agent writes:
#   doc/guide/en/<slug>.md        ← markdown with YAML frontmatter
#   doc/guide/en/screenshots/...  ← PNGs (after autodoc-screenshots)
#
# Files an agent must NOT touch:
#   anything outside the paths declared in its manifest entry's outputs
```
