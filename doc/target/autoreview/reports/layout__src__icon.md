# Review: layout/src/icon.rs

## Summary
- Lines: 520
- Public functions: 9 (`default_icon_resolver`, `register_image_icon`, `register_icons_from_zip`, `register_font_icon`, `register_material_icons`, `register_embedded_material_icons`, `create_default_icon_provider`, `get_material_icons_font_bytes`)
- Public structs/enums: 2 (`ImageIconData`, `FontIconData`)
- Findings: 0 high, 1 medium, 1 low

## Findings

### [MEDIUM] Dead Code — `create_default_icon_provider` has no call sites outside its module
- **Location**: `icon.rs:472`
- **Details**: Re-exported from `lib.rs:125` but no code in the codebase calls it. The actual wiring in `dll/src/desktop/app.rs:47-50` directly calls `set_resolver` and `register_embedded_material_icons` instead.
- **Evidence**: Grep for `create_default_icon_provider` returned only: `icon.rs:472` (def), `lib.rs:125` (re-export), `icon.rs:516` (test).
- **Recommendation**: Consider removing if it's not part of the intended public API, or document it as a convenience for external consumers.

### [LOW] Grayscale matrix is a magic constant
- **Location**: `icon.rs:248-269`
- **Details**: The 4x5 grayscale color matrix is constructed inline with 20 `FloatValue::new(...)` calls. The luminance weights (0.2126, 0.7152, 0.0722) are well-commented with the standard formula, which is good. However, the entire matrix could be a named constant for clarity and reuse.
- **Evidence**: The comment on line 241 documents the weights. The matrix is only used in one place.
- **Recommendation**: Extract to a `const GRAYSCALE_COLOR_MATRIX: StyleColorMatrix` if `StyleColorMatrix` supports const construction, or a lazy static / function. Low priority since it's well-commented.

## System Documentation
- System identified: yes — Icon resolution system (icon provider, resolver callbacks, icon pack registration)
- Existing doc: none (no `icon.md` or `icons.md` in `doc/guide/`)
- Doc needed: A guide document explaining the icon system architecture: how `IconProviderHandle` (core) stores icon packs, how resolvers convert `RefAny` data to `StyledDom`, how `SystemStyle` affects rendering, and how to register custom icon packs. The `core/src/icon.rs` and `layout/src/icon.rs` split (data model vs. rendering) should be documented.
