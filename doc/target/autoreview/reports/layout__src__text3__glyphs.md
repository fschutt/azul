# Review: layout/src/text3/glyphs.rs

## Summary
- Lines: 259
- Public functions: 2 (`get_glyph_runs_simple`, `get_glyph_positions`)
- Public structs/enums: 2 (`PositionedGlyph`, `SimpleGlyphRun`)
- Findings: 0 high, 0 medium, 1 low

## Findings

### [LOW] Documentation verbosity — `get_glyph_positions` doc comment
- **Location**: `glyphs.rs:553-567`
- **Details**: The doc comment for `get_glyph_positions` is 15 lines long with `# Arguments` and `# Returns` sections for a simple function. Could be condensed to 2-3 lines.

## System Documentation
- System identified: yes — Text shaping and rendering pipeline (text3)
- Existing doc: none (no `doc/guide/text-shaping.md` or similar)
- Doc needed: A guide document covering the text3 pipeline (shaping, layout, glyph extraction, rendering) would help explain how `cache.rs`, `default.rs`, `glyphs.rs`, `selection.rs`, and `edit.rs` fit together.
