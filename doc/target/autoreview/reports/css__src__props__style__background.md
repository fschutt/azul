# Review: css/src/props/style/background.rs

## Summary
- Lines: 2252
- Public structs/enums: 28
- Public functions (parser): 8 + various methods
- Findings: 1 high, 1 medium, 2 low

## Findings

### [HIGH] Possible Bug — `parse_css_color` used instead of `parse_color_or_system` for plain background colors
- **Location**: `background.rs:1149`
- **Details**: The fallback path for plain background colors uses `parse_css_color(input)`, but gradient stop parsers at lines 1395 and 1428 use `parse_color_or_system(color_str)`. This means `background: system:accent` will fail, while `background: linear-gradient(system:accent, blue)` will work. This is an inconsistency — system colors should be supported as plain background colors too.
- **Evidence**: Line 1149: `Err(_) => Ok(StyleBackgroundContent::Color(parse_css_color(input)?))` vs line 1395: `let color = parse_color_or_system(color_str)?`. Note that `StyleBackgroundContent::Color` takes a `ColorU`, not `ColorOrSystem`, so supporting system colors for plain backgrounds would require adding a variant or changing the type.
- **Recommendation**: Either add a `StyleBackgroundContent::SystemColor(ColorOrSystem)` variant, or change `Color(ColorU)` to `Color(ColorOrSystem)` and update all consumers.

### [MEDIUM] `..Default::default()` in gradient construction — safe but worth noting
- **Location**: `background.rs:1288`, `background.rs:1310`, `background.rs:1360`
- **Details**: Used for `LinearGradient`, `RadialGradient`, and `ConicGradient`. All defaulted fields are subsequently overwritten by the parsing logic or have safe defaults (empty `stops` vecs, `ExtendMode::Clamp`, `Direction::default()`). No bug here — this is safe.
- **Recommendation**: None — current usage is correct.

### [LOW] `CssShapeParseError` and `CssConicGradientParseError` only used internally
- **Location**: `background.rs:993`, `background.rs:944`
- **Details**: These error types are only used within `background.rs` — no other file references them.
- **Evidence**: `grep -r "CssShapeParseError"` and `grep -r "CssConicGradientParseError"` each return only `background.rs`.
- **Recommendation**: Consider reducing visibility or consolidating into `CssBackgroundParseError`.

### [LOW] Test assertion uses default position (Left, Top) instead of center for radial gradient
- **Location**: `background.rs:2093-2094`
- **Details**: The test `test_radial_gradient_circle` asserts `grad.position.horizontal == BackgroundPositionHorizontal::Left` and `vertical == Top`. Per CSS spec, the default position for radial gradients should be `center center`. The test matches the implementation's `Default::default()` which is `(Left, Top)` — but this default itself may be wrong per CSS spec where background-position defaults to `0% 0%` (equivalent to left top) for `background-position`, but radial-gradient center defaults to `center center`.
- **Recommendation**: Verify whether radial gradient `at` position should default to center. If so, the `RadialGradient::default()` needs to set `position` to `center center` rather than inheriting `StyleBackgroundPosition::default()`.

## System Documentation
- System identified: yes — CSS Styling System (background properties, gradient parsing)
- Existing doc: `doc/guide/css-styling.md`, `doc/guide/css-properties.md`
- Doc needed: n/a (covered by existing styling system docs)
