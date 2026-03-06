# azul-doc

Build tool and W3C spec verification pipeline for the Azul CSS layout engine.

## Building

```bash
cargo build --release -p azul-doc
# Binary: target/release/azul-doc
```

## General Commands

```bash
azul-doc codegen          # Generate code from api.json
azul-doc normalize        # Normalize api.json format
azul-doc dedup            # Remove duplicate types from api.json
azul-doc reftest          # Run reference tests
```

## W3C Spec Verification Pipeline

The `spec` subcommand provides a semi-automated pipeline for verifying and
improving CSS layout compliance against W3C specifications. The pipeline uses
AI agents (Gemini for analysis, Claude for code changes) to systematically
process spec paragraphs.

### Full Pipeline (End-to-End)

```
                            ┌─ review-arch ─→ --review-arch ──┐
claude-exec ──> review-md ──┤                                  ├──> agent-apply
 (patches)    (patch review) ├─ refactor-md ─→ --refactor-md ──┤    (Claude agents)
                            └─ groups-json ─→ --groups-json ──┘
```

Each stage generates a prompt that you feed to Gemini. Gemini's output becomes
an input flag for `agent-apply`. The command names match the flag names:

| Command | What it does | `agent-apply` flag |
|---------|--------------|--------------------|
| `review-md` | Patch quality review (CODE/ANNOT, conflicts) | `--review-md` |
| `review-arch` | Cross-patch architecture review (tunnel vision fix) | `--review-arch` |
| `refactor-md` | Refactoring plan (groundwork before applying) | `--refactor-md` |
| `groups-json` | Merge groups JSON (APPLY/MERGE/PICK_ONE/SKIP) | `--groups-json` |

#### Stage 1: Generate Patches

```bash
# Run parallel Claude agents on all spec paragraphs
# (automatically downloads specs and builds prompts if needed)
azul-doc spec claude-exec --agents=8

# Check progress
azul-doc spec claude-exec --status

# Retry any that timed out or failed
azul-doc spec claude-exec --retry-failed
```

#### Stage 2: Analyze Patches with Gemini

Each command generates a prompt file. Feed it to Gemini, save the output.

```bash
# 2a. Patch quality review (CODE/ANNOT categorization, conflict clusters)
azul-doc spec review-md --no-src <patch-dir>
# → feed prompt to Gemini → save Gemini output as REVIEW.md

# 2b. Architecture review — solves the "tunnel vision" problem:
#   Each claude-exec agent only saw one spec paragraph. This prompt gives
#   Gemini all patches + original paragraphs to find cross-cutting concerns.
azul-doc spec review-arch --review-md <REVIEW.md> <patch-dir>
# → feed prompt to Gemini → save Gemini output as ARCH.md

# 2c. Refactoring plan (groundwork abstractions before applying patches)
azul-doc spec refactor-md --review-md <REVIEW.md> --review-arch <ARCH.md> <patch-dir>
# → feed prompt to Gemini → save Gemini output as REFACTOR.md

# 2d. Merge groups — receives ALL prior analysis for well-informed grouping
azul-doc spec groups-json \
  --review-md <REVIEW.md> \
  --review-arch <ARCH.md> \
  --refactor-md <REFACTOR.md> \
  <patch-dir>
# → feed prompt to Gemini → save JSON output as GROUPS.json
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

The goal is clean semantic commits (~50-60), not 800 individual patches.

Patches are moved to subdirectories as they're processed:
- `applied/` — successfully applied
- `skipped/` — SKIP groups
- `failed/` — agent failures

### Flags for agent-apply

| Flag | Required | Description |
|------|----------|-------------|
| `--groups-json <path>` | yes | Merge groups JSON (from `groups-json` / Gemini) |
| `--refactor-md <path>` | no | Refactoring plan (from `refactor-md` / Gemini) |
| `--review-md <path>` | no | Patch quality review (from `review-md` / Gemini) |
| `--review-arch <path>` | no | Architecture review (from `review-arch` / Gemini) |

### Utility Commands

```bash
azul-doc spec status                # Verification progress (scans +spec: markers)
azul-doc spec extract <feature-id>  # Show spec paragraphs matched by a feature
azul-doc spec paragraphs            # List all known spec paragraph IDs
azul-doc spec annotations           # Scan source for +spec: annotation comments
azul-doc spec tree                  # Display the CSS feature skill tree
azul-doc spec <command> --help      # Detailed help for any subcommand
```

### Adding New W3C Spec Documents

To extend coverage to additional CSS specifications:

1. **Register the spec** in `doc/src/spec/downloader.rs`:
   - Add the spec URL to the `SPECS` list
   - The downloader fetches HTML and stores it in `doc/target/w3c_specs/`

2. **Add a feature node** to the skill tree in `doc/src/spec/skill_tree.rs`:
   - Define a `SkillNode` with: id, name, description, difficulty (1-5),
     dependencies, keywords, source files to review, and whether it needs
     the text engine (`needs_text_engine`)
   - Keywords are used to match spec paragraphs to the feature

3. **Run agents** on the new feature:
   ```bash
   azul-doc spec claude-exec --agents=4
   ```
   (`claude-exec` automatically runs `download` + `build-all` if prompts are missing)

### Annotation Format

Source code annotations link implementation to spec paragraphs:

```rust
// +spec:block-formatting-context-p001 - BFC establishment rules
// +spec:css22-box-8.3.1-p1 - margin collapsing between siblings
```

The `spec status` command scans for these markers to track coverage.

### Architecture

```
doc/src/spec/
├── mod.rs          # Command routing and CLI parsing
├── executor.rs     # Agent execution, all pipeline commands
├── skill_tree.rs   # 16-feature skill tree with dependency ordering
├── downloader.rs   # W3C spec HTML fetcher
├── extractor.rs    # Paragraph extraction from HTML specs
├── reviewer.rs     # Prompt generation for review
└── paragraphs.rs   # Known spec paragraph registry
```

Key layout source files reviewed by agents:
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
