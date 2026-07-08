# azul-doc

Build tool, FFI code generator, and W3C spec verification pipeline for the
Azul CSS layout engine.

```bash
cargo build --release -p azul-doc
# output: target/release/azul-doc
```

## `api.json`

`api.json` is the master schema defining the entire Azul public API. It
describes every module, struct, enum, function, callback, and constant that
is exposed through FFI. The codegen system reads this file and generates
language bindings for Rust, C, C++, and Python.

The typical workflow is:

1. Modify Rust source code (add/change/remove public types or functions)
2. Run `autofix` to synchronize `api.json` with the workspace
3. Run `codegen all` to regenerate all language bindings
4. Build the DLL: `cargo build -p azul-dll --features build-dll --release`

### `autofix` — Synchronize api.json with Source Code

Autofix scans all public types and functions in the Rust workspace, compares
them against `api.json`, and generates patches to bring them in sync. It
handles FFI safety checks, transitive type dependencies, and module placement.

```bash
azul-doc autofix                     # Full run: generate patches into target/autofix/
azul-doc autofix explain             # Preview pending patches without applying
azul-doc autofix apply               # Apply all patches to api.json
azul-doc autofix apply <dir>         # Apply patches from a specific directory
azul-doc autofix apply safe <dir>    # Apply only safe (path-correction) patches

# Add/remove individual items
azul-doc autofix list <Type>         # Compare Type's functions: source vs api.json
azul-doc autofix add <Type.method>   # Add function to api.json (+ transitive deps)
azul-doc autofix add <Type.*>        # Add all functions for a type
azul-doc autofix remove <Type.method>  # Remove function from api.json

# Debugging / analysis
azul-doc autofix difficult           # Rank types by FFI difficulty
azul-doc autofix internal            # Show types that should be internal-only
azul-doc autofix modules             # Show types in the wrong module
azul-doc autofix deps                # Analyze function deps on difficult/internal types
azul-doc autofix debug type <T>      # Debug a type in the workspace index
azul-doc autofix debug chain <T>     # Debug type resolution chain
azul-doc autofix debug api <T>       # Debug type: api.json vs workspace
azul-doc autofix debug file <path>   # Debug parsing a specific Rust source file
```

### `normalize` — Clean Up api.json

```bash
azul-doc normalize    # Canonicalize types + remove cross-module duplicates
```

Performs all api.json cleanup in one pass:

- Canonicalize array types (`"[f32; 20]"` → `{type: "f32", arraysize: 20}`)
- Normalize pointer aliases (`"*mut c_void"` → `{target: "c_void", ref_kind: "mutptr"}`)
- Extract embedded generics (`"Foo<Bar>"` → `{target: "Foo", generic_args: ["Bar"]}`)
- Normalize enum variant pointer types
- Remove duplicate type entries across modules (keeps canonical location only)

Only writes if content actually changed. Always produces the same output on subsequent runs.

### `codegen` — Generate Language Bindings

Reads `api.json` and generates FFI bindings for all target languages.
Output goes to `target/codegen/`.

```bash
azul-doc codegen all      # Generate all targets (recommended)
azul-doc codegen rust     # Rust public API (azul.rs)
azul-doc codegen c        # C header (azul.h)
azul-doc codegen cpp      # C++ headers (azul03..23.hpp, one per C++ standard)
azul-doc codegen python   # Python/PyO3 bindings (python_api.rs)
```

`codegen all` generates 14 files: static/dynamic/build DLL APIs, re-exports,
C header, 6 C++ headers (C++03 through C++23), Rust API, Python bindings,
and memory layout tests.

#### How `codegen` Output Is Used

`dll/src/lib.rs` pulls in generated code via `include!()` macros pointing at
`target/codegen/`. Which files are included depends on the active cargo
feature:

| `azul-dll` feature | Generated code | File contains |
|---------|---------------|------------------|
| `build-dll` | `dll_api_build.rs` | `#[no_mangle] extern "C"` functions for the shared library |
| `link-static` | `dll_api_static.rs` | Types + trait impls via transmute (full Rust stack compiled in) |
| `link-dynamic` | `dll_api_dynamic.rs` | Types + `extern "C"` declarations (links against pre-built `.dylib`/`.so`/`.dll` at runtime) |
| `python-extension` | `python_api.rs` | PyO3 extension module (`import azul` from Python) |
| (always) | `reexports.rs` | Rust-friendly re-exports without `Az` prefix (`use azul::prelude::*`) |
| (test) | `memtest.rs` | Size and alignment verification tests |

`build-dll`, `link-static`, and `link-dynamic` are the three mutually exclusive
link modes. `build-dll` compiles the full desktop stack (windowing, OpenGL,
fonts) and exports C-ABI symbols. `link-static` does the same but without
`#[no_mangle]` exports. `link-dynamic` compiles no internal crates — it only
declares `extern "C"` stubs that resolve at runtime against a pre-built shared
library.

`python-extension` is independent of these three modes. It gates the
`python_api.rs` include behind `#[cfg(feature = "python-extension")]` and
uses PyO3 to wrap each API function as a Python method. The generated Python
module is compiled as a native extension (`.so`/`.pyd`) that Python loads
directly via `import azul` — no C shared library or dynamic linking involved.

## `reftest` - Run Visual Reference Tests

`azul-layout` is tested against Google Chrome via pixel-comparison layout tests 
that render XHTML through both Azul and Chrome, then compare the results. 

Test files live in `doc/working/` and `doc/xhtml1` (inactive tests). The idea is
that we copy more and more tests from `xhtml1` to `working` to activate them.

```bash
azul-doc reftest                     # Run all reftests, generate HTML report
azul-doc reftest <test_name>         # Run a single test, open in browser
azul-doc reftest headless <test>     # Run single test without Chrome, print debug to stdout
```

### `debug` - LLM-Assisted Test Debugging

When a reftest fails, `debug` bundles everything needed to diagnose the
problem into a single Gemini prompt:

- The XHTML test source
- Screenshots (Azul rendering, Chrome reference, pixel diff)
- Chrome's layout tree (JSON from DevTools)
- Azul's display list output
- Layout engine source code (budget-limited to fit token window)
- CSS parse warnings and debug messages
- An optional user question for focused analysis

Gemini receives this context and returns an analysis of what the layout
engine is doing wrong and how to fix it. You can then pass this analysis
to Claude for fixing or fix it yourself.

```bash
azul-doc debug <test> [question]        # Debug a failing reftest with Gemini
azul-doc debug <test> --dry-run         # Generate prompt without sending to Gemini
azul-doc debug <test> --no-screenshots  # Skip screenshot capture
azul-doc debug <test> --working-diff    # Include git diff of doc/working/
```

### `debug-regression` - Track Reftests Across Commits

Sometimes (note: only sometimes!) we want to debug when exactly a test started
to regress. We can use `debug-regression` to track how reftest results change 
across a set of git commits. Provide either a file containing commit hashes (one per line) 
or a single git ref. For each commit, the tool checks out the code in a worktree, 
builds the layout engine (`azul-layout` has to be buildable at the commit), 
runs all reftests, and saves the pixel-diff results. The collected data shows
visual and statistical reports.

```bash
# Run reftests at each commit listed in the file (one hash per line)
azul-doc debug-regression <commits.txt>

# Run reftests at a single git ref
azul-doc debug-regression <git-ref>

# Generate reports from previously collected data
azul-doc debug-regression visual                     # HTML report with side-by-side images
azul-doc debug-regression statistics                 # Text diff statistics (stdout)
azul-doc debug-regression statistics prompt          # Generate Gemini prompt from statistics
azul-doc debug-regression statistics send            # Send statistics to Gemini for analysis
azul-doc debug-regression statistics send -o <path>  # Save Gemini response to file
```

## `autodebug` — Automated Reftest Bug-Finding Pipeline

`autodebug` combines the reftest infrastructure with the parallel Claude agent
executor to automatically discover failing reftests, generate rich diagnostic
prompts, and dispatch agents to analyze and fix each bug.

The pipeline runs each `.xht` test at three screen sizes (mobile 375×667,
tablet 768×1024, desktop 1920×1080), compares Azul's render against Chrome
pixel-by-pixel, and for any test exceeding 0.5% pixel difference generates a
structured agent prompt containing:

- Codebase orientation (solver3, text3, cpurender)
- XHTML source, CSS warnings, layout debug trace
- Screenshot file paths (agents view images via Claude's `Read` tool)
- Pixel diff analysis: connected-component regions with position descriptions
  and heuristic cause classification (border offset, margin collapse, missing
  content, anti-aliasing, layout shift)
- Chrome layout JSON vs Azul display list

Each agent receives one failing test, works in an isolated git worktree, and
produces a commit with the fix (or a diagnostic report if the fix isn't clear).

```bash
# Full run on the default 'working' directory
azul-doc autodebug claude-exec

# Run on the xhtml1/ test directory (9000+ tests)
azul-doc autodebug claude-exec doc/xhtml1

# Generate prompts only (inspect before dispatching agents)
azul-doc autodebug claude-exec --dry-run

# Run a single test
azul-doc autodebug claude-exec --test=abspos-containing-block-initial-001

# Customize parallelism and timeout
azul-doc autodebug claude-exec --agents=8 --timeout=900

# Only test specific screen sizes
azul-doc autodebug claude-exec --sizes=mobile,desktop

# Reuse cached Chrome screenshots (faster re-runs)
azul-doc autodebug claude-exec --skip-chrome

# Re-queue previously failed tests
azul-doc autodebug claude-exec --retry-failed

# Monitor progress (in a separate terminal)
azul-doc autodebug claude-exec --status

# Collect patches from completed agents
azul-doc autodebug claude-exec --collect

# Clean up worktrees and branches
azul-doc autodebug claude-exec --cleanup

# Override the Claude model
azul-doc autodebug claude-exec --model=claude-sonnet-4-6
```

The last positional argument is the test directory (defaults to `doc/working/`).

### Performance

Phase 1 uses two key optimizations for large test sets:

- **Chrome CDP (DevTools Protocol)**: A single persistent Chrome instance
  communicates via WebSocket instead of spawning one process per screenshot.
  This reduces per-screenshot overhead from ~1-2s to ~130ms.
  Falls back to per-process mode if CDP launch fails.
- **Async font registry**: `FcFontRegistry` from rust-fontconfig spawns
  scout + builder threads in the background during Chrome Phase 1a.  By the
  time Azul renders start, all system fonts are already loaded (0s wait).
  The cache is cloned once and shared across all rayon render threads.
- **Parallel rendering**: Azul CPU renders and pixel diffs run in parallel
  via rayon.

Output goes to `doc/target/autodebug/`:

```
doc/target/autodebug/
├── screenshots/    # Chrome PNGs + Azul WebPs per test per size
├── prompts/        # Generated .md prompts + status files (.done/.failed/.taken)
├── patches/        # Extracted .patch files from agent commits
├── reports/        # Agent analysis reports (when fix is uncertain)
└── summary.json    # Overall results
```

## `apply-midlevel` — Interactive Commit Replay

Walks every commit on a reference branch (e.g. `midlevel-fixes-reference`)
and prompts you, one at a time, whether to apply, refine, skip, or reject
each one. A pre-analyzer Claude agent reads the commit + current tree and
recommends an action; you decide.

```bash
azul-doc autoreview apply-midlevel --reference=<branch-or-tag> [options]
```

Options:

| Flag | Default | Notes |
|------|---------|-------|
| `--reference=<ref>` | required | Branch or tag to replay |
| `--base=<ref>` | `origin/layout-debug-clean` | Diverge point |
| `--model=<name>` | `opus` | Apply-agent model |
| `--analyzer-model=<name>` | same as `--model` | Pre-analyzer model |
| `--no-analyze` | off | Skip the pre-analysis agent |
| `--no-telegram` | off | Don't mirror prompts to Telegram (see below) |

Per commit you choose:

```
[y] yes      apply using the current plan
[p] plan     refine the plan: add feedback, analyzer revises
[s] skip     don't apply now, come back later
[r] reject   don't apply, record as rejected with reason
[d] diff     checkout the commit so your editor refreshes
[q] quit     save progress and exit
```

Progress is saved to `doc/target/autoreview/apply-midlevel/progress.json`
after every decision. Re-running resumes where you left off.

### Answer prompts from your phone (Telegram)

A long replay can take days. To answer prompts when you're away from the
PC, pair `azul-doc` with a Telegram bot — once paired, every prompt is
mirrored to a chat with you and any reply (tap a button, or type
free-form feedback) feeds straight into the running `apply-midlevel`.
Local stdin still works; whichever input arrives first wins.

**One-time setup:**

1. **Create a bot.** Open Telegram, message [@BotFather](https://t.me/BotFather),
   send `/newbot`, follow the prompts. Pick any name and any handle ending
   in `bot`. BotFather replies with a token like `1234567890:ABCdef-...`.

2. **Pair.** Run the wizard:

   ```bash
   azul-doc telegram-setup
   ```

   Paste the token when asked. The wizard verifies the token, prints your
   bot's `@handle`, and waits for you to message `/start` to it from your
   phone. As soon as it sees your message, it stores the chat-id and sends
   a confirmation.

3. **Done.** Future runs of `apply-midlevel` automatically mirror prompts
   to your bot. Each prompt comes with a tap-keyboard:

   | Stage | Buttons | Free-form text |
   |-------|---------|----------------|
   | Pre-apply | `apply` `refine` `diff` `skip` `reject` `quit` | becomes analyzer feedback |
   | Post-apply | `accept` `edit` `revert` `quit` | becomes "edit" instructions |

   The legacy single-letter shortcuts (`y/p/s/r/d/q` and `y/e/r/q`) still
   work too — useful when typing on the local terminal.

**Diff preview on the phone.**

- Tap **diff** at the pre-apply prompt → the bot ships the reference
  commit's full patch as `<short-sha>.patch`. Telegram renders it inline
  for scrolling, and "Open in…" lets you pass it to GitHub mobile,
  GitKraken, or any diff viewer that handles `.patch` files.
- Right after a successful apply, the bot **automatically** sends the
  applied diff (`applied-<short>.patch`) before showing the
  accept/edit/revert prompt — no extra tap needed. This is the diff
  the agent actually produced; review it on your phone before deciding.

**Configuration locations** (checked in this order):

| Source | Vars / Path |
|--------|-------------|
| Env vars | `AZ_DOC_TG_TOKEN` + `AZ_DOC_TG_CHAT_ID` (also `TELEGRAM_BOT_TOKEN` + `TELEGRAM_CHAT_ID`) |
| File | `~/.config/azul-doc/telegram.toml` (created by `telegram-setup`, mode `0600`) |

**Disable per run:** add `--no-telegram` to the `apply-midlevel` invocation.

**Notifications missing on your phone?** The bot always sends with
`disable_notification: false`, so the message itself is alerting. If you
don't hear pings, the cause is almost always client-side: long-press
the bot chat in Telegram → check the bell icon isn't muted, and verify
the Telegram app has notification permission at the OS level.

**Notes:**

- `[d] diff` from the local terminal still does the editor checkout
  (detached HEAD on the commit, restored on Enter). From Telegram it
  ships a `.patch` document instead — no working-tree changes.
- The phone message itself is just commit metadata + the analyzer's
  `[CATEGORY] / Why: / Plan: / Suggested user action:` tail; the full
  diff goes through the document attachment, not the message body.
- A network blip on the long-poll prints to stderr, sleeps 5s, and retries
  — it won't kill the run.
- Diff documents are capped at 1 MiB to keep slow phone connections
  responsive; oversized diffs get truncated with an explanatory tail line.

## `autodoc` — Generate the User Guide in Parallel

Reads `doc/autodoc-groups.toml` and dispatches one Claude agent per group.
Each agent writes the markdown pages declared in its group to
`doc/guide/en/`, with YAML frontmatter that records the git SHA and tracked
source files at generation time. The companion `autodoc-check` command
later flags pages whose tracked files have changed since.

Full operational reference: [`AUTODOC.md`](./AUTODOC.md). Manifest:
[`autodoc-groups.toml`](./autodoc-groups.toml).

```bash
azul-doc autoreview autodoc                  # Dispatch one agent per group
azul-doc autoreview autodoc --agents=N       # Throttle concurrency
azul-doc autoreview autodoc --file=GROUP_ID  # Run a single group
azul-doc autoreview autodoc --dry-run        # Generate prompts only

azul-doc autoreview autodoc-check            # Flag stale pages → outdated.md
azul-doc autoreview autodoc-check --strict   # Same, exit non-zero on any stale page (CI)
azul-doc autoreview autodoc-screenshots      # Render azul-render fences to PNG
```

### Pipeline stages

```
autoreview ──► reference.md ──► autodoc ──► autodoc-screenshots ──► deploy
(per-file)     (amalgamated)    (per-group)  (PNG renders)         (website)
```

1. `autoreview` produces per-file reports under `doc/target/autoreview/reports/`.
2. Reports are amalgamated into `doc/guide/en/reference.md` (one-time, hand-curated).
3. `autodoc` reads `autodoc-groups.toml`, builds one prompt per group with
   the group's tracked source, the matching reference.md sections, the
   per-group `design_docs` (intent, not truth), and the global
   writing-style + max-effort thinking rules from `[meta]`.
4. Agents write pages to their declared paths under `doc/guide/en/`.
5. `autodoc-screenshots` renders any `azul-render` fences via `HeadlessWindow`
   into `doc/guide/en/screenshots/<page-slug>/<name>.png`.
6. `deploy` walks `doc/guide/en/` at runtime, parses each page's
   frontmatter for ordering, and emits the final HTML. The guide index
   page is built from the frontmatter (`title`, `short_desc`, `audience`,
   `guide_order`, slug hierarchy) — no separate SUMMARY file is involved.

### Manifest at a glance

`doc/autodoc-groups.toml` (schema v3) defines three trees:

| Tree | Audience | Contents |
|------|----------|----------|
| `getting-started` | external | Linear teaching path: hello-world (per-language) → architecture → understanding-refany → DOM → styling → layout → text → images → widgets → events → timers → animations → scrolling → windowing |
| `advanced` | external | Non-core systems, ramps to web: debugging / profiling / e2e-testing → I/O (xml, file-dialogs, clipboard) → background-tasks + networking → a11y + headless → codegen + bindings → deploying-web → security |
| `contributor` | contributor | Code organization, build-and-codegen, per-system internals (DOM, CSS, layout, text, images, events, rendering, shell2, menus, a11y, async, web) |

Each `[[group]]` declares `id`, `tree`, `audience`, `agent_strategy`,
`tracked_files` (or `tracked_globs`), an optional `design_docs` list,
and one or more `[[group.outputs]]` with `slug`, `path`, `title`,
`maturity`, optional `guide_order` and `prerequisites`. The global
`[meta.writing_style]` and `[meta.agent_thinking]` blocks ship into
every prompt; agents are anchored on the Servo book style and the
project's existing `shared_context_files` (AUTODOC.md, reference.md,
architecture.md, scripts/ARCHITECTURE.md, api.json).

`design_docs` lists files in `scripts/` (e.g. `TEXT_INPUT_ARCHITECTURE_V4.md`)
that capture the original system design. They are *intent*, not authoritative
source — the prompt explicitly tells the agent: **the code is truth, the
design docs are context**. The agent must verify each claim against
`tracked_files` and document divergences inline (e.g. *"the original design
proposed X; the implementation took approach Y"*) rather than reproducing
stale content.

### Two-signal staleness check

`autodoc-check` writes `doc/target/autoreview/autodoc/outdated.md` listing:

- **Source-changed** pages — `tracked_files` had commits since
  `last_generated_rev` in the page's frontmatter (`git log` query).
- **Translation-stale** pages — translated page's `source_hash` no longer
  matches the SHA-256 of its canonical English body. CI fails on either.

### Translations

Translations live in language-prefixed sibling directories under
`doc/guide/`. The English tree is canonical; every other language mirrors
it page-for-page with localized slugs.

```
doc/guide/
├── en/                                  # canonical (the only tree autodoc writes)
│   ├── reference.md
│   ├── architecture.md                  # slug: architecture
│   ├── hello-world.md                   # slug: hello-world
│   ├── layout/flex.md                   # slug: layout/flex
│   └── screenshots/<page>/<name>.png
└── de/                                  # German translation (manual or future `translate` agent)
    ├── architektur.md                   # slug: architektur,    canonical_slug: architecture
    ├── hallo-welt.md                    # slug: hallo-welt,     canonical_slug: hello-world
    └── layout/flexbox.md                # slug: layout/flexbox, canonical_slug: layout/flex
```

A translated page's frontmatter carries five extra fields beyond the
canonical set:

```yaml
---
slug: architektur                        # localized URL — used in the URL path
title: Architektur
language: de
canonical_slug: architecture             # English-side identity (the page this mirrors)
audience: external
maturity: mature
prerequisites: [hello-world]             # always English canonical_slugs; deploy localizes
source_rev: 99be42b08ac44fcb01f1508e8...  # git SHA of canonical/architecture.md at translation time
source_hash: a1b2c3d4...                  # SHA-256 of canonical body bytes (post-frontmatter)
generated_at: 2026-05-02T12:00:00Z
---
```

How the pieces interact:

| Field | Role |
|-------|------|
| `slug` | URL segment under `/<lang>/`. Localized so `/de/architektur` reads naturally in German. |
| `canonical_slug` | Anchor to the English page. Deploy uses it to find sibling translations and resolve `prerequisites`. |
| `prerequisites` | Always lists English canonical slugs. Deploy maps each to the localized slug for the current `language` at render time, so `prerequisites: [hello-world]` on a German page links to `/de/hallo-welt`, not `/en/hello-world`. |
| `source_rev` | Git SHA of the canonical at translation time. Cheap drift signal (grep `git log` for changes). |
| `source_hash` | SHA-256 of the canonical body bytes (after frontmatter). Authoritative drift signal — `autodoc-check --strict` fails CI when this no longer matches the current canonical. |
| `tracked_files` | **Empty on translations.** Translations inherit staleness from their canonical, not from source files. |

When you edit a canonical English page, its body bytes change → SHA-256
changes → every translation's `source_hash` no longer matches → next CI
deploy fails until each translation is regenerated.

There is no `translate` subcommand yet. Translations are produced
manually (or by a future agent dispatch); the spec above is what the
website and `autodoc-check` already understand.

### Outputs

```
doc/target/autoreview/autodoc/
├── prompts/<group-id>.md          # Generated agent prompts
├── prompts/<group-id>.md.taken    # Sentinel: agent in flight
├── prompts/<group-id>.md.done     # Sentinel: agent succeeded
├── prompts/<group-id>.md.failed   # Sentinel: agent failed
├── outdated.md                    # autodoc-check report
└── screenshots-manifest-<lang>.json  # Cached render manifest

doc/guide/<lang>/
├── reference.md                   # per-source-file backlog (en only — canonical)
├── architecture.md                # Canonical architecture doc (en only)
├── <slug>.md                      # Generated guide pages
├── <parent>/<child>.md            # Nested pages render as a sub-tree under <parent>.md
└── screenshots/<page>/<name>.png  # Rendered azul-render fences (en only)
```

## Website Deployment

Builds the full azul.rs website (API docs, release pages, examples, reftest
results) into `doc/target/deploy/`. For the CI usage, this would expect the `dll` 
files to exist, so for debugging CSS / doc / blog posts, etc. the `deploy debug`
command builds the website files without this command.

```bash
azul-doc deploy                      # Production build (inlined CSS, absolute URLs)
azul-doc deploy debug                # Debug build (external CSS, relative paths)
azul-doc deploy with-reftests        # Debug build + run reftests
```

## `spec` - W3C Spec Verification Pipeline

The `spec` subcommand verifies and improves CSS layout compliance against W3C
specifications. It auto-downloads W3C spec HTML sources (CSS Display 3, CSS 2.2
visuren/visudet/box/tables, CSS Text 3), extracts individual paragraphs by
keyword matching, and generates one prompt per paragraph. 

Parallel Claude agents then review each paragraph against the layout engine source 
code and produce patches. Gemini analyzes the patches for quality, architecture, 
and merge planning before agents apply them.

The patches will always contain `// +spec` comments that link the source code back to
the paragraph the implementation came from, so that humans or agents can quickly verify
where a certain W3C feature is implemented or where a certain line of code came from:

```rust
// +spec:block-formatting-context-p001 - BFC establishment rules
// +spec:css22-box-8.3.1-p1 - margin collapsing between siblings
if block.get_containing_width() > block.escaped_margin() {
   // ...
}
```

The `spec status` command scans for these markers to track coverage.

> [!NOTE]
> 
> Gemini has a much larger context window (1M) than Claude (200K) and is good for a
> more "holistic" architecture analysis. So, Gemini is used for the ".md reviews"
> and Claude agents are used for the actual implementation.

For sending the generated .md files to Gemini, use [AIStudio](https://aistudio.google.com/prompts/new_chat),
this way you don't need to spend money on Gemini. Since the Claude Code agents would
use a lot of tokens, it's generally only advisable to use this on a Max 20x Plan, `azul-doc`
will early-exit if it detects API usage.

### Full Pipeline

```
                             ┌─ review-arch ─→ --review-arch ──┐
claude-exec ──> review-md ───┤                                 ├──> agent-apply
 (patches)    (patch review) ├─ refactor-md ─→ --refactor-md ──┤   (Claude agents)
                             └─ groups-json ─→ --groups-json ──┘
```

Each analysis step (`--review-md`, `--review-arch`, `--refactor-md`, `--groups-json`) 
generates a prompt .md file that is fed to Gemini later. Gemini's response then
becomes an input flag for subsequent steps and ultimately for `agent-apply`.

Command names match flag names:

| Command | What it does | `agent-apply` flag |
|---------|--------------|--------------------|
| `review-md` | Patch quality review (CODE/ANNOT, conflicts) | `--review-md` |
| `review-arch` | Cross-patch architecture review (tunnel vision fix) | `--review-arch` |
| `refactor-md` | Refactoring plan (groundwork before applying) | `--refactor-md` |
| `groups-json` | Merge groups JSON (APPLY/MERGE/PICK_ONE/SKIP) | `--groups-json` |

#### Stage 1: Generate Patches

Spec paragraphs are auto-extracted from auto-downloaded W3C HTML sources. The
extractor matches paragraphs to features using keyword lists defined in the
skill tree (e.g. "block formatting context", "margin collapsing", "normal flow"
→ `block-formatting-context` feature). Each matched paragraph becomes a
self-contained prompt file containing the spec text, feature context, and
source files to review.

```bash
# Downloads specs, builds prompts, and runs parallel Claude agents
# Each agent sees ONE paragraph + the layout source code
azul-doc spec claude-exec # --agents=8 // default is 8 parallel agents
azul-doc spec claude-exec --retry-failed  # Retry timed out / failed prompts
# In a separate terminal
azul-doc spec claude-exec --status        # Check progress
```

#### Stage 2: Analyze Patches with Gemini

Each command generates a prompt file, feed it to Gemini via [AIStudio](https://aistudio.google.com/prompts/new_chat), 
save the output. Each step receives the outputs of all previous steps.

```bash
# 2a. Patch quality review (CODE/ANNOT categorization, conflict clusters)
azul-doc spec review-md --no-src <patch-dir>

# 2b. Architecture review — solves the "tunnel vision" problem:
#   Each claude-exec agent only saw one spec paragraph. This gives Gemini
#   all patches + original paragraphs to find cross-cutting concerns.
azul-doc spec review-arch \
  --review-md <REVIEW.md> \
  <patch-dir>

# 2c. Refactoring plan (groundwork abstractions before applying patches)
azul-doc spec refactor-md \
  --review-md <REVIEW.md> \
  --review-arch <ARCH.md> 
  <patch-dir>

# 2d. Merge groups — receives ALL prior analysis for well-informed grouping
azul-doc spec groups-json \
  --review-md <REVIEW.md> \
  --review-arch <ARCH.md> \
  --refactor-md <REFACTOR.md> \
  <patch-dir>
```

#### Stage 3: Apply Patches via Agents

Finally, after doing all of this analysis of the patches, their quality and
what we need to do to cleanly refactor them, the `groups-json` outputs a `.json`
file where the patches are "grouped", because some patches might cover similar
topics. So if we have 800 patches from an agent run, we can then group them into 
a "merge group" if similar patches patch the same functionality (this happens 
because the `claude-exec` agents don't see each other since they run in parallel).

```bash
azul-doc spec agent-apply \
  --groups-json <GROUPS.json> \
  --refactor-md <REFACTOR.md> \
  --review-md <REVIEW.md> \
  --review-arch <ARCH.md> \
  <patch-dir>
```

Important is the `GROUPS.json` here, which contains the overview of how the (sequential) agents
will run. Each agent processes one "merge group" through 5 phases:

1. **Refactoring** — implement groundwork relevant to this group
2. **LLM-apply** — apply patch semantic intent to current code (similar to `cherry-pick`, but with semantics)
3. **Compile** — `cargo check -p azul-dll --features build-dll`
4. **Review** — verify again against the original W3C spec, fix issues and compile again
5. **Commit** — use `git add -p` to create ~2-5 semantic commits by hunk (each must compile independently)

Original `.patch` files are never moved or deleted. Progress is tracked in
`<patch-dir>/agent-apply-status.json`, which records the outcome (applied /
skipped / failed), commit count, and failure reason for each group. This
status file also enables resuming — re-running `agent-apply` skips groups
that were already successfully processed. A group counts as "applied" only
if the agent exits successfully AND makes at least one git commit.

### Spec Utilities

```bash
azul-doc spec download              # Download W3C spec HTML sources
azul-doc spec tree                  # Display CSS feature skill tree
azul-doc spec build-all             # Build per-paragraph agent prompts
azul-doc spec status                # Verification progress (scans +spec: markers)
azul-doc spec paragraphs            # All paragraphs grouped by feature
azul-doc spec paragraphs <feature>  # Paragraphs for one feature (with text)
azul-doc spec annotations           # Scan source for +spec: annotation comments
```

Finally, you can always get help a `spec` command with:

```bash
azul-doc spec <command> --help      # Detailed help for any subcommand
```

### Adding New W3C Spec Documents

The goal of the `spec` command is to bring the source code up to what the original 
W3C `.html` files say the source code should do (esp. checking the various if / else statements, 
box model, semantic restrictions). Now, it might be that some new feature needs to be implemented.

1. **Register the spec** in `doc/src/spec/downloader.rs` — add the URL to
   `SPECS`. The downloader stores HTML in `doc/target/w3c_specs/`.

2. **Add a feature node** in `doc/src/spec/skill_tree.rs` — define keywords
   used to match spec paragraphs to the feature.

3. **Run agents**: `azul-doc spec claude-exec`
   (automatically runs `download` + `build-all` if prompts are missing)

4. **Repeat Stage 2 & 3**

The overall assumptions is: if a source code marker is in the Rust code, the feature has
been implemented correctly (since each `claude-exec` agent prompt contains only ~2k tokens, 
the likelyhood that the patch matches the source code is very high, the hard part is in 
correctly merging the patches due to the "tunnel vision" problem). For final verification,
run the `reftest` again and activate more tests from the `xhtml1` source.

### Other Utilities

```bash
azul-doc print [path]               # Print API tree (e.g. "print app.App.new")
azul-doc discover [pattern]         # List all public functions in workspace
azul-doc nfpm [version]             # Generate nfpm.yaml for OS packaging
```

## CI Pipeline

The GitHub Actions workflow (`.github/workflows/rust.yml`) uses azul-doc
in two modes: **ci** (on push/PR) and **deploy** (manual trigger).

### CI Mode (automatic)

1. **Lint & Static Checks** — runs `autofix`, applies patches, runs
   `normalize`, verifies no uncommitted changes remain. Then runs
   `codegen all` and `cargo test` on the DLL.
2. **Build DLL** — `codegen all` + `cargo build -p azul-dll --features build-dll`
   on Linux, macOS, and Windows.
3. **Reftests** — `codegen all` + `reftest` with Chrome screenshot caching.
4. **C/C++ Examples** — copies `azul.h` and C++ headers from
   `target/codegen/`, compiles all example files with clang/clang++.
5. **Website Build** — `codegen all` + `nfpm` + `deploy` (production).
6. **OS Packaging** — uses nfpm config to build `.deb` and `.rpm` packages.

### Deploy Mode (manual)

Downloads all CI artifacts (DLLs, website, reftests, packages), assembles
them into a single website directory, and deploys to GitHub Pages.

## Project Structure

```
doc/src/
├── main.rs             # CLI entry point and command dispatch
├── api.rs              # normalize (type canonicalization)
├── autofix/            # api.json ↔ workspace synchronization
├── codegen/            # Language binding generators
│   └── v2/             # IR builder → Rust/C/C++/Python generators
├── patch/              # dedup, api.json patch application
├── reftest/            # Pixel-comparison layout tests + LLM debugging
│   └── autodebug.rs    # Automated bug-finding pipeline (multi-res diff + agent dispatch)
├── docgen/             # Website documentation generator
├── dllgen/             # DLL build + deploy
└── spec/               # W3C spec verification pipeline
    ├── mod.rs           # Command routing and CLI parsing
    ├── executor.rs      # Agent execution, all pipeline commands
    ├── skill_tree.rs    # 16-feature skill tree with dependency ordering
    ├── downloader.rs    # W3C spec HTML fetcher
    ├── extractor.rs     # Paragraph extraction from HTML specs
    ├── reviewer.rs      # Prompt generation for review
    └── paragraphs.rs    # Known spec paragraph registry
```

Key layout source files reviewed by spec agents:

```
layout/src/solver3/
├── fc.rs           # Formatting context solver (BFC, IFC, floats)
├── sizing.rs       # Width/height calculation (CSS 2.2 §10.3/§10.6)
├── positioning.rs  # Absolute/relative positioning
├── geometry.rs     # Box model geometry structs
├── layout_tree.rs  # DOM-to-layout tree conversion
├── getters.rs      # CSS property accessors
├── mod.rs          # Containing block resolution
└── taffy_bridge.rs # Flexbox integration

layout/src/text3/
├── cache.rs        # Text layout cache, constraint builder
├── knuth_plass.rs  # Knuth-Plass line breaking algorithm
└── glyphs.rs       # Glyph metrics, line height
```
