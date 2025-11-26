# Font Chain Resolution - Current State Analysis & Refactoring Plan

## Executive Summary

The current font chain resolution implementation has a critical performance bug:
**Font chains are resolved per-text-node instead of per-unique-font-stack.**

This causes the layout engine to hang on documents with many text nodes because:
- Each text node triggers a full font chain resolution (~100-200ms)
- Cache misses occur because the cache key includes unicode ranges from text
- A document with 100 text nodes = 100 × 200ms = 20 seconds just for font resolution

## Current Architecture (BROKEN)

### Call Flow (Current)

```
layout_document_paged()
    └── layout_document()
            └── calculate_intrinsic_sizes()
                    └── calculate_inline_intrinsic_sizes()   // Per text node
                            └── layout_flow()
                                    └── shape_visual_items()
                                            └── load_font_from_stack()       // Called per node!
                                                    └── fc_cache.resolve_font_chain()  // EXPENSIVE!
```

### Where Time is Spent

| Component | File | Function | Time |
|-----------|------|----------|------|
| Font cache build | rust-fontconfig/lib.rs | `FcFontCache::build()` | 1.8s ✅ OK |
| Chain resolution | rust-fontconfig/lib.rs | `resolve_font_chain_uncached()` | 200ms ❌ Per text node! |
| Total per document | | | N × 200ms where N = text nodes |

### Root Cause Analysis

**File: `/Users/fschutt/Development/azul/layout/src/text3/cache.rs`**

```rust
// Line 351 - Called during text shaping, which happens per text node
let chain = self.fc_cache.resolve_font_chain(
    &font_families,
    sample_text,  // This varies!
    weight,
    italic,
    oblique,
    &mut trace,
);
```

**File: `/Users/fschutt/Development/rust-fontconfig/src/lib.rs`**

```rust
// Line 1770 - Cache key includes unicode_ranges which varies per text!
let cache_key = FontChainCacheKey {
    font_families: expanded_families.clone(),
    unicode_ranges: unicode_ranges.clone(),  // ❌ PROBLEM: This causes cache misses!
    weight,
    italic,
    oblique,
};
```

### Debug Output Showing the Problem

```
[DEBUG cache.rs] load_font_from_stack: font_families=["Arial", "sans-serif", "serif", "monospace"]
[DEBUG rust-fontconfig] resolve_font_chain_with_os: families=["Arial", "sans-serif", "serif", "monospace"]
[DEBUG rust-fontconfig] resolve_font_chain_with_os: 10 unicode ranges        ◄── From "Q3 2024 Report"
[DEBUG rust-fontconfig] resolve_font_chain_with_os: cache MISS               ◄── Cache miss!

... text node 2 ...

[DEBUG cache.rs] load_font_from_stack: font_families=["Arial", "sans-serif", "serif", "monospace"]
[DEBUG rust-fontconfig] resolve_font_chain_with_os: families=["Arial", "sans-serif", "serif", "monospace"]
[DEBUG rust-fontconfig] resolve_font_chain_with_os: 8 unicode ranges         ◄── From "Quarterly Revenue"
[DEBUG rust-fontconfig] resolve_font_chain_with_os: cache MISS               ◄── Cache miss AGAIN!
```

Same font-family, same weight, same style → should be a cache HIT!
But cache key includes unicode_ranges, so it's always a MISS.

## Current Code Structure

### rust-fontconfig (Font Discovery)

| File | Function | Purpose | Issue |
|------|----------|---------|-------|
| lib.rs:1732 | `resolve_font_chain()` | Main entry point | Delegates to `resolve_font_chain_with_os` |
| lib.rs:1745 | `resolve_font_chain_with_os()` | Build chain with caching | Cache key includes unicode_ranges ❌ |
| lib.rs:1801 | `resolve_font_chain_uncached()` | Actual resolution | 24 families × query = slow |
| lib.rs:1866 | `fuzzy_query_by_name()` | Find font by name | Called 24× per resolution |
| lib.rs:2111 | `expand_font_families()` | sans-serif → [SF, Helvetica, ...] | Expands to 24 families |

### azul-layout (Text Layout)

| File | Function | Purpose | Issue |
|------|----------|---------|-------|
| text3/cache.rs:300 | `load_font_from_stack()` | Load font for text | Calls resolve_font_chain per node ❌ |
| text3/cache.rs:4250 | `shape_visual_items()` | Shape text | Calls load_font_from_stack per item |
| solver3/sizing.rs:200 | `calculate_inline_intrinsic_sizes()` | Calculate text size | Triggers layout_flow |

### azul-layout Local Cache (Partially Fixed)

The code at `cache.rs:328-345` attempts to cache font chains locally:

```rust
// Create cache key based only on CSS properties, NOT text content
let cache_key = FontChainKey {
    font_families: font_families.clone(),
    weight,
    italic: is_italic,
    oblique: is_oblique,
};

// Check our local cache first
let font_chain = {
    let cache = self.font_chain_cache.lock().unwrap();
    cache.get(&cache_key).cloned()
};
```

But on cache miss, it still calls `resolve_font_chain()` with a sample_text, which then
does its own caching with unicode_ranges in the key, causing confusion.

## Refactoring Plan

### Phase 1: Fix rust-fontconfig Cache Key (Quick Fix)

**Goal:** Make cache key independent of text content

**File:** `/Users/fschutt/Development/rust-fontconfig/src/lib.rs`

```rust
// BEFORE (line 1770)
let cache_key = FontChainCacheKey {
    font_families: expanded_families.clone(),
    unicode_ranges: unicode_ranges.clone(),  // ❌ Remove this
    weight,
    italic,
    oblique,
};

// AFTER
let cache_key = FontChainCacheKey {
    font_families: expanded_families.clone(),
    // unicode_ranges removed - not part of cache key
    weight,
    italic,
    oblique,
};
```

**Effort:** 30 minutes
**Risk:** Low - cache will just return same chain for same CSS properties

### Phase 2: Pre-resolve Font Chains Before Layout

**Goal:** Resolve all unique font-stacks before layout starts

**Location:** New function in `printpdf/src/html.rs` or `azul/layout/src/lib.rs`

```rust
/// Collect unique font-stacks from document and pre-resolve them
fn pre_resolve_font_chains(
    document: &StyledDocument,
    fc_cache: &FcFontCache,
) -> HashMap<FontStackKey, FontFallbackChain> {
    // 1. Walk all styled nodes
    // 2. Collect unique (font-family, weight, style) tuples
    // 3. Resolve each to a FontFallbackChain
    // 4. Return map for layout engine to use
}
```

**Changes Required:**
1. Add `pre_resolve_font_chains()` function
2. Call it after style computation, before `layout_document_paged()`
3. Pass resolved chains to layout engine
4. Update `load_font_from_stack()` to use pre-resolved chains

**Effort:** 2-3 hours
**Risk:** Medium - requires threading data through layout engine

### Phase 3: Simplify resolve_font_chain API

**Goal:** Make API clearer about what's cached

**File:** `/Users/fschutt/Development/rust-fontconfig/src/lib.rs`

```rust
/// Resolve font chain - cached by (families, weight, italic, oblique) only
/// 
/// The `representative_text` parameter is used to ensure the chain includes
/// fonts that can render typical characters, but it does NOT affect caching.
pub fn resolve_font_chain(
    &self,
    font_families: &[String],
    representative_text: &str,  // Used for unicode coverage check, not caching
    weight: FcWeight,
    italic: PatternMatch,
    oblique: PatternMatch,
    trace: &mut Vec<TraceMsg>,
) -> FontFallbackChain;
```

**Effort:** 1 hour
**Risk:** Low - documentation/API clarity

### Phase 4: Remove Duplicate Caching

**Goal:** Single source of truth for font chain cache

Currently there are two caches:
1. `rust-fontconfig` internal cache (with unicode_ranges in key)
2. `azul-layout` FontManager cache (without unicode_ranges in key)

After Phase 1, both should have same behavior, but having two caches is confusing.

**Options:**
1. Remove azul-layout cache, rely on rust-fontconfig cache (simpler)
2. Remove rust-fontconfig cache, let caller manage caching (more control)

**Recommendation:** Option 1 - keep rust-fontconfig cache as single source

**Effort:** 1 hour
**Risk:** Low

## Implementation Priority

| Phase | Priority | Effort | Impact |
|-------|----------|--------|--------|
| Phase 1 | **CRITICAL** | 30 min | Fixes hang immediately |
| Phase 2 | High | 2-3 hr | Proper architecture |
| Phase 3 | Medium | 1 hr | Code clarity |
| Phase 4 | Low | 1 hr | Code cleanup |

## Quick Fix (Phase 1 Only)

To immediately fix the hang, change the cache key in rust-fontconfig:

```rust
// /Users/fschutt/Development/rust-fontconfig/src/lib.rs
// Line 1770

// CHANGE FROM:
let cache_key = FontChainCacheKey {
    font_families: expanded_families.clone(),
    unicode_ranges: unicode_ranges.clone(),
    weight,
    italic,
    oblique,
};

// CHANGE TO:
let cache_key = FontChainCacheKey {
    font_families: expanded_families.clone(),
    weight,
    italic,
    oblique,
};

// Also update FontChainCacheKey struct to remove unicode_ranges field
```

This will make all text nodes with the same CSS font-stack share one resolved chain.

## Expected Results After Fix

| Metric | Before | After |
|--------|--------|-------|
| Font chain resolutions | N (one per text node) | K (one per unique font-stack) |
| Time for report.html (100 nodes) | 20s+ (hangs) | <3s |
| Cache hit rate | ~0% | ~97% |

## Testing

After implementing Phase 1:

```bash
cd /Users/fschutt/Development/printpdf
cargo build --example html_full
timeout 10 ./target/debug/examples/html_full
```

Expected output:
- Should complete within 5 seconds
- Should show cache HITs after first resolution
- PDF should render correctly

## Appendix: Debug Output Locations

To trace font chain resolution, these debug statements exist:

1. `rust-fontconfig/src/lib.rs:1753` - Entry point
2. `rust-fontconfig/src/lib.rs:1771` - Cache lookup
3. `rust-fontconfig/src/lib.rs:1778` - Cache hit/miss
4. `rust-fontconfig/src/lib.rs:1811` - Family iteration
5. `azul/layout/src/text3/cache.rs:301` - load_font_from_stack entry
6. `azul/layout/src/text3/cache.rs:4243` - shape_visual_items

Remove these debug statements after fixing the issue.
