# Review: core/src/svg.rs

## Summary
- Lines: 1436
- Public functions: ~45
- Public structs/enums: ~30
- Findings: 0 high, 2 medium, 1 low (2 high and 3 medium already resolved in current source)

## Findings

### [MEDIUM] Duplicated Structs — `SvgTransform` and `SvgRenderTransform` are identical
- **Location**: `svg.rs:1067-1074` and `svg.rs:1291-1298`
- **Details**: Both structs have exactly the same fields: `sx`, `kx`, `ky`, `sy`, `tx`, `ty` (all `f32`). `SvgTransform` is used in `SvgFillStyle` and `SvgStrokeStyle`, while `SvgRenderTransform` is used in `SvgRenderOptions`. They serve different contexts (tessellation vs rendering) but are structurally identical.
- **Recommendation**: Consider consolidating into one type, or add a type alias. If semantic distinction is important, add doc comments explaining why they are separate.

### [MEDIUM] Dead/Unused Field — `apply_line_width` documented as "currently unused"
- **Location**: `svg.rs:1153-1159`
- **Details**: The `SvgStrokeStyle::apply_line_width` field has a doc comment explicitly stating `NOTE: currently unused!`. This is dead configuration.
- **Evidence**: Grep for `apply_line_width` shows it defined in `core/src/svg.rs` and used in `layout/src/xml/svg.rs`, `api.json`, and `scripts/TODO_LIST.md` — but the comment says the actual stroke generation doesn't use it.
- **Recommendation**: Either implement the feature or remove the field and document the behavior.

### [LOW] `Indent` enum only used by `SvgXmlOptions`
- **Location**: `svg.rs:1432-1436`
- **Details**: The `Indent` enum is only used within `SvgXmlOptions` in this file. Grep shows no usage outside `core/src/svg.rs` (only `layout/src/xml/svg.rs` imports it). This is fine for now but the enum could be scoped more narrowly if it's not part of the public API contract.
- **Recommendation**: No action needed; just noting for awareness.

## System Documentation
- System identified: yes — SVG rendering pipeline (parsing, tessellation, GPU upload, drawing)
- Existing doc: none (no SVG guide in `doc/guide/`)
- Doc needed: An SVG rendering guide explaining the pipeline from `SvgParseOptions` → `Svg`/`SvgXmlNode` → `SvgNode` → `TessellatedSvgNode` → `TessellatedGPUSvgNode` → GPU draw, including the role of Lyon tessellation and WebRender integration.
