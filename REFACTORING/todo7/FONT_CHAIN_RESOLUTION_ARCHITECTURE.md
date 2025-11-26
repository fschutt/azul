# Font Chain Resolution Architecture

## Overview

This document describes the correct architecture for font resolution in the text layout pipeline.
The system consists of three phases that must be executed in order, with clear caching boundaries.

## Architecture Phases

```
┌─────────────────────────────────────────────────────────────────────────────┐
│                        PHASE 1: FONT CACHE BUILD                            │
│                         (Once per process)                                  │
│                                                                             │
│  • Scan system font directories                                             │
│  • Parse font metadata (name, weight, style, unicode ranges from OS/2)      │
│  • Build searchable index (token-based for fuzzy matching)                  │
│  • Time: ~1.5-2.0 seconds for ~800 fonts                                    │
│                                                                             │
│  Input:  System font directories                                            │
│  Output: FcFontCache with 800+ fonts indexed                                │
└─────────────────────────────────────────────────────────────────────────────┘
                                    │
                                    ▼
┌─────────────────────────────────────────────────────────────────────────────┐
│                     PHASE 2: FONT CHAIN RESOLUTION                          │
│                   (Once per unique CSS font-stack)                          │
│                                                                             │
│  • Resolve CSS font-family names to actual system fonts                     │
│  • Expand generic families (sans-serif → [Helvetica, Arial, ...])           │
│  • Fuzzy match specific font names ("My Font" → "MyFont-Regular.ttf")       │
│  • Build fallback chain for Unicode coverage                                │
│  • Time: ~100-200ms per unique font-stack                                   │
│                                                                             │
│  Input:  ["Arial", "sans-serif", "serif"]                                   │
│  Output: FontFallbackChain with resolved fonts + unicode coverage info      │
│                                                                             │
│  Cache Key: (font_families, weight, italic, oblique)                        │
│  Cache Key DOES NOT include: text content, unicode ranges from text         │
└─────────────────────────────────────────────────────────────────────────────┘
                                    │
                                    ▼
┌─────────────────────────────────────────────────────────────────────────────┐
│                       PHASE 3: CHARACTER RESOLUTION                         │
│                   (Per character during text shaping)                       │
│                                                                             │
│  • Given a pre-built FontFallbackChain, find font for specific character    │
│  • Simple lookup: iterate chain, check unicode coverage, return first match │
│  • Time: <1μs per character (just array iteration + range check)            │
│                                                                             │
│  Input:  FontFallbackChain + character (e.g., '你')                         │
│  Output: FontId of font that can render this character                      │
│                                                                             │
│  No caching needed - lookup is O(n) where n = fonts in chain (~5-20)        │
└─────────────────────────────────────────────────────────────────────────────┘
```

## Data Flow

```
HTML/CSS Document
       │
       ▼
┌──────────────────┐
│  Parse Styles    │
│  Extract unique  │
│  font-stacks     │
└──────────────────┘
       │
       │  Unique stacks: { ["Arial", "sans-serif"], ["Georgia", "serif"], ... }
       │  (Usually 3-10 unique stacks per document)
       ▼
┌──────────────────┐
│  PHASE 2:        │     ◄── Do this BEFORE layout starts
│  Resolve Chains  │         One call per unique stack
└──────────────────┘         Cached result reused for ALL nodes with same stack
       │
       │  Resolved: { 
       │    ["Arial", "sans-serif"] → FontFallbackChain { Arial, SF, Helvetica, ... },
       │    ["Georgia", "serif"]    → FontFallbackChain { Georgia, Times, Palatino, ... }
       │  }
       ▼
┌──────────────────┐
│  Layout Engine   │     ◄── Now layout can proceed
│  (intrinsic      │         Font chains are already resolved
│   sizing, etc.)  │
└──────────────────┘
       │
       │  For each text node:
       │    1. Get pre-resolved chain for this node's font-stack
       │    2. For each character, call chain.resolve_char(ch)
       │    3. Group consecutive chars by resolved font
       │    4. Shape each group with its font
       ▼
┌──────────────────┐
│  PHASE 3:        │
│  Shape Text      │
│  (per-font runs) │
└──────────────────┘
```

## Key Principles

### 1. Font Chain Resolution is Expensive
- Fuzzy matching font names against 800+ fonts
- Expanding generic families to OS-specific lists
- Building unicode coverage maps
- **Must be done ONCE per unique CSS font-stack, not per text node**

### 2. Character Resolution is Cheap
- Simple array iteration with range checks
- Pre-computed unicode ranges from Phase 2
- **Can be done per-character without performance concern**

### 3. Cache Key Design

**WRONG Cache Key:**
```rust
struct CacheKey {
    font_families: Vec<String>,
    unicode_ranges: Vec<UnicodeRange>,  // ❌ From text content
    weight: FcWeight,
    ...
}
```
This creates a new cache entry for every unique text string!

**CORRECT Cache Key:**
```rust
struct CacheKey {
    font_families: Vec<String>,
    weight: FcWeight,
    italic: bool,
    oblique: bool,
    // NO text or unicode_ranges - these vary per text node
}
```

### 4. When to Resolve Font Chains

| Phase | When | What |
|-------|------|------|
| Style Computation | Before layout | Collect unique font-stacks |
| Pre-Layout | Before intrinsic sizing | Resolve all unique stacks → chains |
| Layout | During intrinsic sizing | Use pre-resolved chains for char lookup |
| Shaping | After layout | Shape text runs using resolved fonts |

## API Design

### rust-fontconfig API

```rust
impl FcFontCache {
    /// Phase 1: Build cache (once per process)
    pub fn build() -> Self;
    
    /// Phase 2: Resolve font chain (once per unique CSS font-stack)
    /// 
    /// IMPORTANT: `sample_text` is used to determine Unicode coverage needs,
    /// but the cache key should NOT include the text content.
    pub fn resolve_font_chain(
        &self,
        font_families: &[String],      // CSS font-family list
        sample_text: &str,             // Representative text for unicode coverage
        weight: FcWeight,
        italic: PatternMatch,
        oblique: PatternMatch,
        trace: &mut Vec<TraceMsg>,
    ) -> FontFallbackChain;
}

impl FontFallbackChain {
    /// Phase 3: Resolve character (per-character, very fast)
    pub fn resolve_char(&self, cache: &FcFontCache, ch: char) -> Option<(String, String)>;
}
```

### Layout Engine Integration

```rust
// BEFORE layout starts
fn pre_resolve_font_chains(
    document: &Document,
    fc_cache: &FcFontCache,
) -> HashMap<FontStackKey, FontFallbackChain> {
    // 1. Collect all unique font-stacks from document styles
    let unique_stacks = collect_unique_font_stacks(document);
    
    // 2. Resolve each stack to a chain (this is the expensive part)
    let mut resolved = HashMap::new();
    for stack in unique_stacks {
        let key = FontStackKey::from(&stack);
        let chain = fc_cache.resolve_font_chain(
            &stack.families,
            "AaBbCc 你好 العربية",  // Representative sample
            stack.weight,
            stack.italic,
            stack.oblique,
            &mut Vec::new(),
        );
        resolved.insert(key, chain);
    }
    
    resolved  // Pass this to layout engine
}

// DURING layout
fn shape_text(
    text: &str,
    font_stack: &FontStack,
    pre_resolved_chains: &HashMap<FontStackKey, FontFallbackChain>,
    fc_cache: &FcFontCache,
) -> ShapedText {
    // Get pre-resolved chain (O(1) lookup)
    let chain = pre_resolved_chains.get(&FontStackKey::from(font_stack))
        .expect("Font chain should have been pre-resolved");
    
    // Group characters by resolved font (O(n) where n = text length)
    let runs = group_by_font(text, chain, fc_cache);
    
    // Shape each run with its font
    runs.iter().map(|run| shape_run(run)).collect()
}
```

## Performance Characteristics

| Operation | Time | Frequency |
|-----------|------|-----------|
| Font cache build | 1.5-2.0s | Once per process |
| Font chain resolution | 100-200ms | Once per unique CSS font-stack |
| Character resolution | <1μs | Per character |
| Font loading (disk → memory) | 10-50ms | Once per font used |
| Text shaping (HarfBuzz) | 1-10ms | Per text run |

For a typical document with:
- 3 unique font-stacks (body, headings, code)
- 1000 text nodes
- 50,000 characters total

Expected times:
- Font cache: 2s (once)
- Chain resolution: 3 × 150ms = 450ms (once)
- Character resolution: 50,000 × 1μs = 50ms (during layout)
- Total font overhead: ~2.5s (one-time) + 50ms (per layout)

## What NOT to Do

### ❌ Resolve chain per text node
```rust
// BAD: This is O(nodes × 800 fonts × fuzzy_match_cost)
for node in text_nodes {
    let chain = fc_cache.resolve_font_chain(&node.font_stack, &node.text, ...);
    // ...
}
```

### ❌ Include text in cache key
```rust
// BAD: Every unique text string creates a new cache entry
let cache_key = (font_families, unicode_ranges_from_text, weight);
```

### ❌ Fuzzy match during shaping
```rust
// BAD: Fuzzy matching is expensive
for char in text.chars() {
    let font = fc_cache.fuzzy_query_by_name(&family, ...);  // ❌
}
```

## What TO Do

### ✅ Pre-resolve chains before layout
```rust
// GOOD: Resolve all unique stacks upfront
let chains = pre_resolve_font_chains(&document, &fc_cache);
layout_document(&document, &chains, ...);
```

### ✅ Cache by CSS properties only
```rust
// GOOD: Cache key based on CSS, not text content
let cache_key = (font_families, weight, italic, oblique);
```

### ✅ Use resolve_char() for per-character lookup
```rust
// GOOD: resolve_char is O(chain_length) = O(5-20), very fast
for char in text.chars() {
    let font_id = chain.resolve_char(&fc_cache, char);  // ✅
}
```
