# WebRender Local Integration - Session Summary

**Date**: 21. Oktober 2025  
**Goal**: Integrate local webrender fork to enable font sharing with azul-layout

## Problem Statement

The crates.io version of webrender (azul-webrender 0.62.2) uses FreeType to parse fonts separately from azul-layout, causing:
1. **Duplicate Memory**: Fonts parsed twice (once for layout, once for rendering)
2. **Duplicate CPU**: Font parsing done twice
3. **C Dependencies**: FreeType prevents pure-Rust builds for Redox/WASM

## Solution

Use local `/webrender` fork with `wr_azul_glyph_rasterizer` that:
1. Already uses `ParsedFont` from azul-layout (via allsorts)
2. Uses `tiny-skia` for pure-Rust glyph rasterization
3. But currently still re-parses fonts from bytes

**Next step**: Pass `Arc<ParsedFont>` directly from azul-layout to WebRender to eliminate re-parsing.

## Changes Made

### 1. Updated dll/Cargo.toml
```toml
# OLD:
webrender = { version = "0.62.2", package = "azul-webrender", default-features = false, optional = true }

# NEW:
webrender = { path = "../webrender/core", default-features = false, optional = true }
```

### 2. Fixed Workspace Dependencies
Added to root `Cargo.toml`:
```toml
[workspace.dependencies]
glean = "60.5.0"
```

### 3. Fixed WebRender Cargo.toml Paths
Updated all relative path dependencies that don't exist in our fork:

**webrender/core/Cargo.toml**:
- `../webrender_api` → `../api` ✅
- `../wr_malloc_size_of` → crates.io `0.2` ✅
- `../webrender_build` → Removed (not available) ✅
- `../peek-poke` → crates.io `0.2` ✅
- `../wr_azul_glyph_rasterizer` → `../glyph` ✅
- `../swgl` → Commented out (not needed) ✅

**webrender/api/Cargo.toml**:
- `../wr_malloc_size_of` → crates.io `0.2` ✅
- `../peek-poke` → crates.io `0.2` ✅

**webrender/glyph/Cargo.toml**:
- `../webrender_api` → `../api` ✅
- `../wr_malloc_size_of` → crates.io `0.2` ✅
- `git = "https://github.com/maps4print/azul"` → `path = "../../layout"` ✅
- `git = "https://github.com/maps4print/azul"` → `path = "../../core"` ✅

### 4. Disabled Unused Features
- `sw_compositor` feature (software rendering, not needed)
- `serialize_program` feature (depends on webrender_build)

## Build Status

🔄 **COMPILING** - All dependency issues resolved, webrender is now building from local fork

## Next Steps (After Build Completes)

### Step 1: Add Shared Font API to wr_azul_glyph_rasterizer

Modify `webrender/glyph/src/font.rs`:

```rust
pub struct FontContext {
    fonts: FastHashMap<FontKey, FontSource>,
}

enum FontSource {
    Owned(Arc<ParsedFont>),   // Parsed from bytes (legacy)
    Shared(Arc<ParsedFont>),  // Shared from azul-layout (NEW)
}

impl FontContext {
    // NEW: Accept externally-parsed fonts
    pub fn add_shared_font(&mut self, font_key: FontKey, parsed_font: Arc<ParsedFont>) {
        self.fonts.insert(font_key, FontSource::Shared(parsed_font));
    }
}
```

### Step 2: Create SharedFontRegistry in shell2

```rust
pub struct SharedFontRegistry {
    fonts: HashMap<u64, (FontKey, Arc<ParsedFont>)>,  // font_hash -> (key, font)
    id_namespace: IdNamespace,
}

impl SharedFontRegistry {
    pub fn get_or_register_font(
        &mut self,
        font_hash: u64,
        font_manager: &FontManager<ParsedFont, PathLoader>,
        webrender_font_context: &mut FontContext,
    ) -> FontKey {
        // If already registered, return existing key
        // Otherwise, load from font_manager and register with WebRender
    }
}
```

### Step 3: Update Display List Translation

Use `get_glyph_runs()` to group glyphs by font+color:

```rust
let glyph_runs = get_glyph_runs(layout);

for glyph_run in glyph_runs {
    let font_key = shared_font_registry.get_or_register_font(
        glyph_run.font_hash,
        &layout_window.font_manager,
        &mut webrender_font_context,
    );
    
    // Look up FontInstanceKey for this font+size
    let font_instance_key = renderer_resources
        .currently_registered_fonts
        .get(&font_key)
        .and_then(|(_, instances)| instances.get(&(font_size, dpi)))?;
    
    push_text(&mut builder, &info, &glyph_run.glyphs, font_instance_key, glyph_run.color);
}
```

## Architecture Benefits

1. **Single Font Parse**: Each font loaded only once
2. **Memory Savings**: No duplicate ParsedFont instances
3. **Pure Rust**: No C dependencies (FreeType, CoreText, DirectWrite)
4. **Redox/WASM Ready**: All dependencies compile on pure-Rust targets
5. **Consistency**: Same font data used for layout and rendering

## Open Questions

1. **Font Size**: GlyphRun doesn't include font_size - need to get from StyleProperties
2. **Font Cleanup**: When to unregister fonts from WebRender?
3. **Fallback Handling**: Different glyphs may use different fallback fonts

## Files Modified

- `/Cargo.toml` - Added glean workspace dependency
- `dll/Cargo.toml` - Changed webrender to local path
- `webrender/core/Cargo.toml` - Fixed all path dependencies
- `webrender/api/Cargo.toml` - Fixed path dependencies
- `webrender/glyph/Cargo.toml` - Fixed paths, use local azul-layout/core
- `REFACTORING/FONT_SHARING_ARCHITECTURE.md` - Created architecture doc

## Completion Criteria

- [x] WebRender compiles from local fork
- [ ] Add `add_shared_font()` to FontContext
- [ ] Create SharedFontRegistry
- [ ] Update compositor2 text rendering
- [ ] Test text rendering works
- [ ] Verify memory savings
- [ ] Then: Implement scroll wheel handling
