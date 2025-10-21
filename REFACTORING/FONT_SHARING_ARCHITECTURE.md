# Font Sharing Architecture Between azul-layout and WebRender

**Date**: 21. Oktober 2025  
**Goal**: Eliminate duplicate font parsing by sharing `ParsedFont` instances between azul-layout and WebRender

## Problem

Currently, fonts are parsed twice:
1. **azul-layout** parses fonts with `ParsedFont` (from allsorts) for text shaping and layout
2. **WebRender** receives font bytes and re-parses them in `wr_azul_glyph_rasterizer` for rasterization

This wastes memory and CPU time.

## Solution Architecture

### Phase 1: WebRender Integration ✅
- [x] Switch dll/Cargo.toml from crates.io webrender to local `/webrender/core`
- [x] Fix workspace dependencies (add glean to workspace)
- [ ] Verify build completes successfully

### Phase 2: Font Context Bridge 🔄

Modify `webrender/glyph/src/font.rs` to accept external `ParsedFont` instances:

```rust
pub struct FontContext {
    // OLD: fonts: FastHashMap<FontKey, Arc<ParsedFont>>,
    // NEW: Allow external fonts to be registered
    fonts: FastHashMap<FontKey, FontSource>,
}

enum FontSource {
    /// Font parsed from bytes (legacy path)
    Owned(Arc<ParsedFont>),
    /// Font shared from external source (azul-layout)
    Shared(Arc<ParsedFont>),
}

impl FontContext {
    /// NEW: Register an externally-parsed font
    pub fn add_shared_font(&mut self, font_key: FontKey, parsed_font: Arc<ParsedFont>) {
        self.fonts.insert(font_key, FontSource::Shared(parsed_font));
    }
    
    /// MODIFIED: Keep existing add_font for compatibility
    pub fn add_font(&mut self, font_key: FontKey, template: Arc<FontTemplate>) {
        // Parse and store as Owned
    }
}
```

### Phase 3: Shell2 Font Registry 🔄

Create font registry in shell2 that manages the lifetime of shared fonts:

```rust
// In dll/src/desktop/shell2/macos/mod.rs
pub struct SharedFontRegistry {
    /// Map FontRef -> (FontKey, Arc<ParsedFont>)
    fonts: HashMap<FontRef, (FontKey, Arc<ParsedFont>)>,
    /// ID namespace for generating FontKeys
    id_namespace: IdNamespace,
}

impl SharedFontRegistry {
    pub fn get_or_register_font(
        &mut self,
        font_ref: &FontRef,
        font_manager: &FontManager<ParsedFont, PathLoader>,
        webrender_font_context: &mut FontContext,
    ) -> FontKey {
        if let Some((key, _)) = self.fonts.get(font_ref) {
            return *key;
        }
        
        // Load font from azul-layout
        let parsed_font = font_manager.load_font(font_ref).expect("Font not found");
        
        // Generate WebRender FontKey
        let font_key = FontKey::unique(self.id_namespace);
        
        // Register with WebRender WITHOUT re-parsing
        webrender_font_context.add_shared_font(font_key, parsed_font.clone());
        
        // Cache mapping
        self.fonts.insert(font_ref.clone(), (font_key, parsed_font));
        
        font_key
    }
}
```

### Phase 4: Display List Translation 🔄

Update `compositor2/mod.rs` to resolve fonts during translation:

```rust
// In translate_displaylist_to_wr()
for item in &display_list.items {
    match item {
        DisplayListItem::Text { glyphs, font, color, clip_rect } => {
            // Resolve FontRef -> FontKey using shared registry
            let font_key = shared_font_registry.get_or_register_font(
                font,
                &layout_window.font_manager,
                &mut webrender_font_context,
            );
            
            // Look up FontInstanceKey for this font+size combination
            let font_instance_key = renderer_resources
                .currently_registered_fonts
                .get(&font_key)
                .and_then(|(_, instances)| instances.get(&(font_size, dpi)))
                .expect("FontInstanceKey should be registered");
            
            // Push to WebRender
            push_text(&mut builder, &info, glyphs, font_instance_key, *color);
        }
        _ => {}
    }
}
```

### Phase 5: GlyphRun Integration 🔄

Instead of using DisplayListItem::Text directly, use `get_glyph_runs()`:

```rust
// In display list generation (layout/src/solver3/display_list.rs)
let glyph_runs = crate::text3::glyphs::get_glyph_runs(layout);

for glyph_run in glyph_runs {
    // Store ParsedFont reference or hash in display list
    let font_hash = glyph_run.font_hash;
    builder.push_text_run(
        glyph_run.glyphs,
        font_hash, // Use hash instead of FontRef
        glyph_run.color,
        clip_rect,
    );
}

// Then in compositor2, resolve hash -> FontKey via registry
```

## Benefits

1. **Memory savings**: Each font parsed only once
2. **Performance**: No duplicate parsing
3. **Consistency**: Same font data used for layout and rendering
4. **Redox/WASM support**: Pure Rust, no C dependencies

## Implementation Order

1. ✅ Switch to local webrender (in progress)
2. Modify `wr_azul_glyph_rasterizer::FontContext` to accept `add_shared_font()`
3. Create `SharedFontRegistry` in shell2
4. Update display list translation to use shared fonts
5. Test and verify memory savings

## Open Questions

1. **Font size handling**: GlyphRun doesn't include font size. Need to extract from StyleProperties or store in display list
2. **Font cleanup**: When to remove fonts from WebRender? (On window close, after N frames, etc.)
3. **Fallback fonts**: How to handle when glyphs use different fallback fonts?

## Next Steps

- [x] Fix webrender workspace integration
- [ ] Wait for build to complete
- [ ] Implement `add_shared_font()` in FontContext
- [ ] Create SharedFontRegistry
- [ ] Update compositor2 text rendering
