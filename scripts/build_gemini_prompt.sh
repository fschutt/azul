#!/bin/bash
# Build comprehensive Gemini prompt for performance optimization analysis
# Target: ~150K lines (Gemini 1M context ≈ 150K lines)

OUT="/Users/fschutt/Development/azul/scripts/gemini_perf_prompt.md"

cat > "$OUT" << 'PROMPT_HEADER'
# Performance Optimization Analysis Request

## Context

I'm building `git2pdf`, a tool that converts git repositories to PDF documents.
The pipeline is:

```
git repo → source files → syntax-highlighted HTML → XML DOM → styled DOM → layout → display list → PDF ops → PDF file
```

The stack consists of 4 crates:
1. **git2pdf** - CLI tool, orchestrates the pipeline
2. **printpdf** - PDF generation library with an HTML-to-PDF feature
3. **azul-layout** - CSS layout engine (flexbox, block layout, text shaping, pagination)
4. **azul-core** - Core data structures (DOM, CSS property cache, styled DOM, diff/reconciliation)
5. **azul-css** - CSS type system (property types, parsing, system fonts)
6. **rust-fontconfig** - Font discovery and resolution

## Current Performance

Processing the git2pdf repository itself (5 Rust source files, ~25 PDF pages):

**Total time: 4.1 seconds** (target: <1 second for 5 files, <10 seconds for 100+ files)

### Detailed Timing Breakdown (per file, largest first):

#### File: main.rs (251KB HTML, 12,481 nodes, 8 pages)
- preprocess HTML: 4.6ms
- parse XML: 2.3ms
- **str_to_dom: 250ms** (XML string → DOM tree with CSS parsing)
- font resolution: 28ms (collect_and_resolve_font_chains: 9.3ms)
- **reconcile_and_invalidate: 41ms** (CSS reconciliation, 12481 nodes all dirty)
- **layout loop: 404ms** (1 iteration - flexbox/block layout + text shaping)
- position adjustments: 2.1ms
- **generate_display_list: 51ms**
- generate_display_list (outer): 52ms (19,441 items)
- paginate: 9.8ms
- **layout_document_paged TOTAL: 599ms**
- display_list→ops+fonts: 8 pages
- **FILE TOTAL: 866ms**

#### File: html_generator.rs (178KB HTML, 8,905 nodes, 7 pages)
- str_to_dom: 194ms
- font resolution: 20ms (collect_and_resolve_font_chains: 5.9ms)
- reconcile_and_invalidate: 29ms
- layout loop: 296ms
- generate_display_list: 30ms + 30ms
- **FILE TOTAL: 627ms**

#### File: git_ops.rs (95KB HTML, 4,715 nodes, 3 pages)
- str_to_dom: 93ms
- font resolution: 7.1ms
- reconcile_and_invalidate: 20ms
- layout loop: 296ms (ANOMALY: same as 8905-node file!)
- **FILE TOTAL: 453ms**

#### File: crate_discovery.rs (94KB HTML, 4,677 nodes, 3 pages)
- str_to_dom: 107ms
- font resolution: 11ms
- reconcile_and_invalidate: 15ms
- layout loop: 143ms
- **FILE TOTAL: 312ms**

#### File: file_classifier.rs (76KB HTML, 3,787 nodes, 3 pages)
- str_to_dom: 76ms
- font resolution: 5.4ms
- reconcile_and_invalidate: 7ms
- layout loop: 106ms
- **FILE TOTAL: 219ms**

#### Title page (1.5KB HTML, 9 nodes, 1 page)
- str_to_dom: 2.6ms
- font resolution: 51ms (initial font loading from disk)
- layout loop: 3.2ms
- **FILE TOTAL: 60ms**

### Font Pool Build: 998ms (one-time, shared across files)

### Summary of where time goes (for all 5 files):
| Phase | Time | % of total |
|-------|------|-----------|
| Font pool build | 998ms | 24.3% |
| str_to_dom (XML→DOM) | 726ms | 17.7% |
| Layout loop | 1,249ms | 30.4% |
| Font resolution | 73ms | 1.8% |
| Reconcile/invalidate | 112ms | 2.7% |
| Display list gen | 244ms | 5.9% |
| Pagination | 17ms | 0.4% |
| PDF ops conversion | ~200ms est | 4.9% |
| Other overhead | ~487ms | 11.9% |
| **TOTAL** | **~4,106ms** | **100%** |

## Key Observations

1. **Layout loop is 30% of total** - This is CSS block/flexbox layout + text shaping for each node
2. **str_to_dom is 18%** - XML parsing + CSS property parsing + DOM construction
3. **Font pool build is 24%** - Scanning system fonts via fontconfig (one-time cost)
4. **Everything scales roughly as O(n) with node count** except the font pool
5. **Each "node" is typically a `<span>` with syntax-highlighted text** - the HTML is very flat (no deep nesting)
6. **All nodes are dirty** on first layout (no incremental)
7. **The layout loop does 1 iteration** — no reflows needed
8. **Files are processed sequentially** by the parallel pool, so total ≈ sum of all files

## What We've Already Optimized

1. **StyledDom clone removal** (9.38s → 5.91s) - was cloning the entire styled DOM
2. **Debug message disabling** (5.91s → 4.47s) - eprintln! calls in hot paths
3. **SharedFontPool** (4.47s → 4.29s) - share parsed fonts across files via Arc<Mutex>
4. **BTreeMap→Vec conversion** (4.29s → 4.1s) - CssPropertyCache outer maps to Vec for O(1) node lookup

## Architecture Questions for Gemini

Please analyze the attached source code and answer:

1. **Why is layout so slow?** For a flat DOM (mostly `<pre>` with `<span>` children), 404ms for 12K nodes seems excessive. CSS-in-Chrome would handle this in <50ms. What's the algorithmic bottleneck?

2. **Why is str_to_dom so slow?** 250ms to parse 252KB of XML with inline CSS seems very high. Is the CSS parsing per-node the bottleneck? Could we pre-compute CSS classes?

3. **Font resolution overhead**: `collect_and_resolve_font_chains` iterates ALL nodes, queries CSS properties, builds font selectors, hashes for dedup. For a monospace code display, all nodes use the same font. Can we short-circuit?

4. **Reconcile cost**: 41ms for reconcile_and_invalidate with 12K dirty nodes. Since this is first layout (all dirty), can we skip reconciliation entirely?

5. **Display list generation**: 51ms + 52ms for 19K items. Is there unnecessary cloning or allocation?

6. **Architectural improvements**: 
   - Could we batch-process spans that share the same CSS (which is almost all of them in syntax-highlighted code)?
   - Could we use a "fast path" for text-only documents (no images, no complex layout)?
   - Could we process the layout incrementally per-page instead of all-at-once?
   - Should we consider a completely different rendering approach for code listings?

7. **Cache invalidation for window resize**: If the user resizes the window, we need to relayout. Currently we'd redo everything. What's a good invalidation strategy?

8. **Memory layout**: The CssPropertyCache stores 15 `Vec<BTreeMap<CssPropertyType, CssProperty>>`. Each BTreeMap has overhead. For predominantly monospace text, most nodes share identical CSS. Could we use CSS class deduplication (e.g., all `.keyword` spans point to the same property set)?

9. **Text shaping**: The text3 module uses allsorts for shaping. For monospace fonts, every character has the same advance width. Can we completely skip shaping for monospace?

10. **Parallelism**: Files are laid out sequentially. Could we parallelize layout across files? What about within a single file (e.g., lay out independent subtrees in parallel)?

## What I Want From You

Please provide:
1. **Ranked list of optimization opportunities** with estimated impact
2. **Specific code-level suggestions** for each opportunity  
3. **Architectural refactoring proposals** (multiple approaches)
4. **A roadmap** to get from 4.1s → <1s for 5 files

Now here is the complete source code of all relevant files:

PROMPT_HEADER

echo "" >> "$OUT"
echo "---" >> "$OUT"
echo "" >> "$OUT"

# Function to add a file with header
add_file() {
    local filepath="$1"
    local display_name="$2"
    if [ -f "$filepath" ]; then
        echo "" >> "$OUT"
        echo "## File: \`$display_name\`" >> "$OUT"
        echo "" >> "$OUT"
        echo '```rust' >> "$OUT"
        cat "$filepath" >> "$OUT"
        echo '```' >> "$OUT"
        echo "" >> "$OUT"
    else
        echo "WARNING: File not found: $filepath" >&2
    fi
}

add_md() {
    local filepath="$1"
    local display_name="$2"
    if [ -f "$filepath" ]; then
        echo "" >> "$OUT"
        echo "## File: \`$display_name\`" >> "$OUT"
        echo "" >> "$OUT"
        echo '```markdown' >> "$OUT"
        cat "$filepath" >> "$OUT"
        echo '```' >> "$OUT"
        echo "" >> "$OUT"
    fi
}

echo "# SECTION 1: Architecture Documentation" >> "$OUT"
add_md "/Users/fschutt/Development/azul/scripts/ARCHITECTURE.md" "azul/scripts/ARCHITECTURE.md"
add_md "/Users/fschutt/Development/azul/scripts/STARTUP_LATENCY.md" "azul/scripts/STARTUP_LATENCY.md"

echo "" >> "$OUT"
echo "# SECTION 2: git2pdf (CLI driver)" >> "$OUT"
for f in /Users/fschutt/Development/git2pdf/src/*.rs; do
    add_file "$f" "git2pdf/src/$(basename $f)"
done

echo "" >> "$OUT"
echo "# SECTION 3: printpdf HTML module (PDF bridge)" >> "$OUT"
for f in /Users/fschutt/Development/printpdf/src/html/*.rs; do
    add_file "$f" "printpdf/src/html/$(basename $f)"
done
add_file "/Users/fschutt/Development/printpdf/src/font.rs" "printpdf/src/font.rs"
add_file "/Users/fschutt/Development/printpdf/src/ops.rs" "printpdf/src/ops.rs"
add_file "/Users/fschutt/Development/printpdf/src/text.rs" "printpdf/src/text.rs"
add_file "/Users/fschutt/Development/printpdf/src/render.rs" "printpdf/src/render.rs"

echo "" >> "$OUT"
echo "# SECTION 4: azul-core (core data structures)" >> "$OUT"
add_file "/Users/fschutt/Development/azul/core/src/prop_cache.rs" "azul/core/src/prop_cache.rs"
add_file "/Users/fschutt/Development/azul/core/src/styled_dom.rs" "azul/core/src/styled_dom.rs"
add_file "/Users/fschutt/Development/azul/core/src/diff.rs" "azul/core/src/diff.rs"
add_file "/Users/fschutt/Development/azul/core/src/dom.rs" "azul/core/src/dom.rs"
add_file "/Users/fschutt/Development/azul/core/src/xml.rs" "azul/core/src/xml.rs"
add_file "/Users/fschutt/Development/azul/core/src/debug.rs" "azul/core/src/debug.rs"
add_file "/Users/fschutt/Development/azul/core/src/ui_solver.rs" "azul/core/src/ui_solver.rs"
add_file "/Users/fschutt/Development/azul/core/src/ua_css.rs" "azul/core/src/ua_css.rs"

echo "" >> "$OUT"
echo "# SECTION 5: azul-layout solver3 (layout engine)" >> "$OUT"
add_file "/Users/fschutt/Development/azul/layout/src/solver3/layout_tree.rs" "azul/layout/src/solver3/layout_tree.rs"
add_file "/Users/fschutt/Development/azul/layout/src/solver3/sizing.rs" "azul/layout/src/solver3/sizing.rs"
add_file "/Users/fschutt/Development/azul/layout/src/solver3/positioning.rs" "azul/layout/src/solver3/positioning.rs"
add_file "/Users/fschutt/Development/azul/layout/src/solver3/display_list.rs" "azul/layout/src/solver3/display_list.rs"
add_file "/Users/fschutt/Development/azul/layout/src/solver3/getters.rs" "azul/layout/src/solver3/getters.rs"
add_file "/Users/fschutt/Development/azul/layout/src/solver3/paged_layout.rs" "azul/layout/src/solver3/paged_layout.rs"
add_file "/Users/fschutt/Development/azul/layout/src/solver3/cache.rs" "azul/layout/src/solver3/cache.rs"
add_file "/Users/fschutt/Development/azul/layout/src/solver3/pagination.rs" "azul/layout/src/solver3/pagination.rs"
add_file "/Users/fschutt/Development/azul/layout/src/solver3/taffy_bridge.rs" "azul/layout/src/solver3/taffy_bridge.rs"
add_file "/Users/fschutt/Development/azul/layout/src/solver3/geometry.rs" "azul/layout/src/solver3/geometry.rs"
add_file "/Users/fschutt/Development/azul/layout/src/solver3/fc.rs" "azul/layout/src/solver3/fc.rs"
add_file "/Users/fschutt/Development/azul/layout/src/solver3/calc.rs" "azul/layout/src/solver3/calc.rs"
add_file "/Users/fschutt/Development/azul/layout/src/solver3/counters.rs" "azul/layout/src/solver3/counters.rs"
add_file "/Users/fschutt/Development/azul/layout/src/solver3/mod.rs" "azul/layout/src/solver3/mod.rs"

echo "" >> "$OUT"
echo "# SECTION 6: Text shaping and caching" >> "$OUT"
add_file "/Users/fschutt/Development/azul/layout/src/text3/cache.rs" "azul/layout/src/text3/cache.rs"
add_file "/Users/fschutt/Development/azul/layout/src/text3/glyphs.rs" "azul/layout/src/text3/glyphs.rs"
add_file "/Users/fschutt/Development/azul/layout/src/text3/knuth_plass.rs" "azul/layout/src/text3/knuth_plass.rs"
add_file "/Users/fschutt/Development/azul/layout/src/text3/script.rs" "azul/layout/src/text3/script.rs"
add_file "/Users/fschutt/Development/azul/layout/src/text3/default.rs" "azul/layout/src/text3/default.rs"
add_file "/Users/fschutt/Development/azul/layout/src/text3/mod.rs" "azul/layout/src/text3/mod.rs"

echo "" >> "$OUT"
echo "# SECTION 7: Fragmentation and window" >> "$OUT"
add_file "/Users/fschutt/Development/azul/layout/src/fragmentation.rs" "azul/layout/src/fragmentation.rs"
add_file "/Users/fschutt/Development/azul/layout/src/window.rs" "azul/layout/src/window.rs"

echo "" >> "$OUT"
echo "# SECTION 8: XML parsing (layout module)" >> "$OUT"
add_file "/Users/fschutt/Development/azul/layout/src/xml/mod.rs" "azul/layout/src/xml/mod.rs"

echo "" >> "$OUT"
echo "# SECTION 9: Font system" >> "$OUT"
add_file "/Users/fschutt/Development/azul/layout/src/font_traits.rs" "azul/layout/src/font_traits.rs"
add_file "/Users/fschutt/Development/azul/layout/src/font.rs" "azul/layout/src/font.rs"
add_file "/Users/fschutt/Development/rust-fontconfig/src/lib.rs" "rust-fontconfig/src/lib.rs"
add_file "/Users/fschutt/Development/rust-fontconfig/src/registry.rs" "rust-fontconfig/src/registry.rs"

echo "" >> "$OUT"
echo "# SECTION 10: CSS type system (key files)" >> "$OUT"
add_file "/Users/fschutt/Development/azul/css/src/props/property.rs" "azul/css/src/props/property.rs"
add_file "/Users/fschutt/Development/azul/css/src/props/basic/font.rs" "azul/css/src/props/basic/font.rs"
add_file "/Users/fschutt/Development/azul/css/src/props/basic/length.rs" "azul/css/src/props/basic/length.rs"
add_file "/Users/fschutt/Development/azul/css/src/props/layout/dimensions.rs" "azul/css/src/props/layout/dimensions.rs"
add_file "/Users/fschutt/Development/azul/css/src/props/layout/flex.rs" "azul/css/src/props/layout/flex.rs"
add_file "/Users/fschutt/Development/azul/css/src/css.rs" "azul/css/src/css.rs"
add_file "/Users/fschutt/Development/azul/css/src/system.rs" "azul/css/src/system.rs"
add_file "/Users/fschutt/Development/azul/css/src/parser2.rs" "azul/css/src/parser2.rs"
add_file "/Users/fschutt/Development/azul/css/src/macros.rs" "azul/css/src/macros.rs"
add_file "/Users/fschutt/Development/azul/css/src/props/macros.rs" "azul/css/src/props/macros.rs"

# Count lines
LINES=$(wc -l < "$OUT")
echo ""
echo "=== Gemini prompt written to: $OUT ==="
echo "=== Total lines: $LINES ==="
echo ""
