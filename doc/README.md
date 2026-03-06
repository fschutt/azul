# azul-doc

Build tool, FFI code generator, and W3C spec verification pipeline for the
Azul CSS layout engine.

## Building

```bash
cargo build --release -p azul-doc
# Binary: target/release/azul-doc
```

## api.json

`api.json` is the master schema defining the entire Azul public API. It
describes every module, struct, enum, function, callback, and constant that
is exposed through FFI. The codegen system reads this file and generates
language bindings for Rust, C, C++, and Python.

The typical workflow is:

1. Modify Rust source code (add/change/remove public types or functions)
2. Run `autofix` to synchronize `api.json` with the workspace
3. Run `codegen all` to regenerate all language bindings
4. Build the DLL: `cargo build -p azul-dll --features build-dll --release`

### autofix — Synchronize api.json with Source Code

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

### normalize — Clean Up api.json

```bash
azul-doc normalize    # Canonicalize types + remove cross-module duplicates
```

Performs all api.json cleanup in one pass:
- Canonicalize array types (`"[f32; 20]"` → `{type: "f32", arraysize: 20}`)
- Normalize pointer aliases (`"*mut c_void"` → `{target: "c_void", ref_kind: "mutptr"}`)
- Extract embedded generics (`"Foo<Bar>"` → `{target: "Foo", generic_args: ["Bar"]}`)
- Normalize enum variant pointer types
- Remove duplicate type entries across modules (keeps canonical location only)

Only writes if content actually changed. Idempotent.

### codegen — Generate Language Bindings

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

### Other Utilities

```bash
azul-doc print [path]               # Print API tree (e.g. "print app.App.new")
azul-doc discover [pattern]         # List all public functions in workspace
azul-doc unused                     # Find unreachable types in api.json
azul-doc unused patch               # Generate removal patches for unused types
azul-doc nfpm [version]             # Generate nfpm.yaml for OS packaging
```

## Reference Tests (reftests)

Pixel-comparison layout tests that render XHTML through both Azul and Chrome,
then compare the results. Test files live in `doc/working/`.

```bash
azul-doc reftest                     # Run all reftests, generate HTML report
azul-doc reftest <test_name>         # Run a single test, open in browser
azul-doc reftest headless <test>     # Run single test headlessly (console output)
```

### LLM-Assisted Debugging

```bash
azul-doc debug <test> [question]     # Debug a failing reftest with Gemini
azul-doc debug <test> --dry-run      # Generate prompt without sending to Gemini
azul-doc debug <test> --no-screenshots  # Skip screenshot capture
```

### Regression Analysis

```bash
azul-doc debug-regression <commits.txt>   # Run reftests across git history
azul-doc debug-regression <git-ref>       # Regression for a single ref
azul-doc debug-regression visual          # Generate visual HTML regression report
azul-doc debug-regression statistics      # Text diff statistics
azul-doc debug-regression statistics send # Send statistics to Gemini for analysis
```

## Website Deployment

Builds the full azul.rs website (API docs, release pages, examples, reftest
results) into `doc/target/deploy/`.

```bash
azul-doc deploy               # Production build (inlined CSS, absolute URLs)
azul-doc deploy debug          # Debug build (external CSS, relative paths)
azul-doc fast-deploy-with-reftests   # Debug build + run reftests
```

## W3C Spec Verification Pipeline

The `spec` subcommand verifies and improves CSS layout compliance against W3C
specifications. It downloads W3C spec HTML sources (CSS Display 3, CSS 2.2
visuren/visudet/box/tables, CSS Text 3), extracts individual paragraphs by
keyword matching, and generates one prompt per paragraph. Parallel Claude
agents then review each paragraph against the layout engine source code and
produce patches. Gemini analyzes the patches for quality, architecture, and
merge planning before agents apply them.

### Full Pipeline

```
                            ┌─ review-arch ─→ --review-arch ──┐
claude-exec ──> review-md ──┤                                  ├──> agent-apply
 (patches)    (patch review) ├─ refactor-md ─→ --refactor-md ──┤    (Claude agents)
                            └─ groups-json ─→ --groups-json ──┘
```

Each analysis step generates a prompt that you feed to Gemini. Gemini's output
becomes an input flag for subsequent steps and ultimately for `agent-apply`.
Command names match flag names:

| Command | What it does | `agent-apply` flag |
|---------|--------------|--------------------|
| `review-md` | Patch quality review (CODE/ANNOT, conflicts) | `--review-md` |
| `review-arch` | Cross-patch architecture review (tunnel vision fix) | `--review-arch` |
| `refactor-md` | Refactoring plan (groundwork before applying) | `--refactor-md` |
| `groups-json` | Merge groups JSON (APPLY/MERGE/PICK_ONE/SKIP) | `--groups-json` |

#### Stage 1: Generate Patches

Spec paragraphs are auto-extracted from downloaded W3C HTML sources. The
extractor matches paragraphs to features using keyword lists defined in the
skill tree (e.g. "block formatting context", "margin collapsing", "normal flow"
→ `block-formatting-context` feature). Each matched paragraph becomes a
self-contained prompt file containing the spec text, feature context, and
source files to review.

```bash
# Downloads specs, builds prompts, and runs parallel Claude agents
# Each agent sees ONE paragraph + the layout source code
azul-doc spec claude-exec --agents=8

azul-doc spec claude-exec --status        # Check progress
azul-doc spec claude-exec --retry-failed  # Retry timed out / failed prompts
```

#### Stage 2: Analyze Patches with Gemini

Each command generates a prompt file. Feed it to Gemini, save the output.
Each step receives the outputs of all previous steps.

```bash
# 2a. Patch quality review (CODE/ANNOT categorization, conflict clusters)
azul-doc spec review-md --no-src <patch-dir>

# 2b. Architecture review — solves the "tunnel vision" problem:
#   Each claude-exec agent only saw one spec paragraph. This gives Gemini
#   all patches + original paragraphs to find cross-cutting concerns.
azul-doc spec review-arch --review-md <REVIEW.md> <patch-dir>

# 2c. Refactoring plan (groundwork abstractions before applying patches)
azul-doc spec refactor-md --review-md <REVIEW.md> --review-arch <ARCH.md> <patch-dir>

# 2d. Merge groups — receives ALL prior analysis for well-informed grouping
azul-doc spec groups-json \
  --review-md <REVIEW.md> \
  --review-arch <ARCH.md> \
  --refactor-md <REFACTOR.md> \
  <patch-dir>
```

#### Stage 3: Apply Patches via Agents

```bash
azul-doc spec agent-apply \
  --groups-json <GROUPS.json> \
  --refactor-md <REFACTOR.md> \
  --review-md <REVIEW.md> \
  --review-arch <ARCH.md> \
  <patch-dir>
```

Each agent processes one merge group through 4 phases:
1. **Refactoring** — implement groundwork relevant to this group
2. **LLM-apply** — apply patch semantic intent to current code (not literal diff)
3. **Compile** — `cargo check -p azul-dll --features build-dll`
4. **Review** — verify against W3C spec, fix issues, compile again

Patches are moved to `applied/`, `skipped/`, or `failed/` as they're processed.

### Spec Utilities

```bash
azul-doc spec download              # Download W3C spec HTML sources
azul-doc spec tree                  # Display CSS feature skill tree
azul-doc spec build-all             # Build per-paragraph agent prompts
azul-doc spec extract <feature>     # Show spec paragraphs matched by a feature
azul-doc spec status                # Verification progress (scans +spec: markers)
azul-doc spec paragraphs            # List all known spec paragraph IDs
azul-doc spec annotations           # Scan source for +spec: annotation comments
azul-doc spec <command> --help      # Detailed help for any subcommand
```

### Adding New W3C Spec Documents

1. **Register the spec** in `doc/src/spec/downloader.rs` — add the URL to
   `SPECS`. The downloader stores HTML in `doc/target/w3c_specs/`.

2. **Add a feature node** in `doc/src/spec/skill_tree.rs` — define keywords
   used to match spec paragraphs to the feature.

3. **Run agents**: `azul-doc spec claude-exec --agents=4`
   (automatically runs `download` + `build-all` if prompts are missing)

### Annotation Format

Source code annotations link implementation to spec paragraphs:

```rust
// +spec:block-formatting-context-p001 - BFC establishment rules
// +spec:css22-box-8.3.1-p1 - margin collapsing between siblings
```

The `spec status` command scans for these markers to track coverage.

## Project Structure

```
doc/src/
├── main.rs             # CLI entry point and command dispatch
├── api.rs              # normalize (type canonicalization)
├── autofix/            # api.json ↔ workspace synchronization
├── codegen/            # Language binding generators (v1 legacy, v2 current)
│   └── v2/             # IR builder → Rust/C/C++/Python generators
├── patch/              # dedup, api.json patch application
├── reftest/            # Pixel-comparison layout tests + LLM debugging
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
