# TrueType Hinting Implementation Plan for allsorts

## Problem

Azul's text rendering produces visibly different output compared to Chrome/browsers.
Text appears thinner, slightly wider, and less crisp — especially at small sizes.
The root cause is that our glyph rasterizer (`wr_glyph_rasterizer`) renders glyphs
from raw scaled outlines without applying TrueType hinting (grid-fitting). Browsers
use platform engines (CoreText, DirectWrite, FreeType) that interpret the font's
embedded bytecode instructions to snap glyph outlines to the pixel grid before
rasterization.

This is NOT a bug in allsorts — HarfBuzz produces identical shaping output.
The issue is that nobody interprets the hinting instructions. Allsorts is a shaping
engine; it parses and preserves hinting data but doesn't execute it.

## What TrueType Hinting Does

Hinting is a **pre-rasterization outline transform**. It takes scaled vector
coordinates (in fractional pixel space, F26Dot6 = 26.6 fixed-point) and moves
them so that stems, crossbars, and curves land cleanly on pixel boundaries.

Without hinting: stems may span 1.3 pixels → anti-aliased to gray mush.
With hinting: stems snapped to exactly 1 or 2 pixels → crisp black lines.

The effect is most visible at small sizes (8-20px) and on low-DPI screens.
At large sizes (48px+) or high-DPI (retina), hinting matters less.

## The Four Tables

### `cvt` (Control Value Table)
A flat array of signed 16-bit FUnit values representing canonical measurements:
stem widths, x-height, cap height, baseline, serif sizes, etc. These ensure
consistency across glyphs (all vertical stems same width, etc.).

**allsorts status:** Parsed as `CvtTable<'a>` with `ReadArrayCow<'a, I16Be>`.
Used for variable font `cvar` delta application. Structurally complete.

### `fpgm` (Font Program)
Bytecode that runs **once at font load time**. Defines reusable subroutines
(FDEFs/IDEFs) — the font's "standard library" of hinting functions. Cannot
depend on ppem since no size context exists yet.

**allsorts status:** Tag constant exists. Raw bytes preserved for subsetting.
No struct, no parsing, no execution.

### `prep` (Control Value Program)
Bytecode that runs **once per ppem/size change**. Modifies CVT entries for the
current size (e.g., at 8ppem round all stems to 1px; at 20ppem allow 2px).
Also sets Graphics State defaults for all subsequent glyph programs.

**allsorts status:** Same as fpgm — tag + raw bytes only.

### `gasp` (Grid-fitting And Scan-conversion Procedure)
NOT bytecode. A lookup table mapping ppem ranges to rendering flags:
- `GASP_GRIDFIT` (0x01): run bytecode hinting
- `GASP_DOGRAY` (0x02): use anti-aliasing
- `GASP_SYMMETRIC_GRIDFIT` (0x04): ClearType-style hinting
- `GASP_SYMMETRIC_SMOOTHING` (0x08): ClearType subpixel smoothing

**allsorts status:** Tag exists. Explicitly dropped during subsetting. No parser.

### `glyf` per-glyph instructions
Each `SimpleGlyph` and `CompositeGlyph` in the `glyf` table may contain a
bytecode program specific to that glyph.

**allsorts status:** Stored as `instructions: Box<[u8]>` in both SimpleGlyph
and CompositeGlyph. Bytes read and written correctly. Never decoded or executed.

## Rendering Pipeline (Current vs Target)

### Current (no hinting)
```
glyf outline (FUnits)
  → scale by (font_size_px / units_per_em)
  → tiny-skia fill_path with anti-aliasing
  → alpha bitmap
```

### Target (with hinting)
```
Font load:
  parse cvt, fpgm, prep, gasp
  execute fpgm → populate FDEF/IDEF subroutine table

Size change (new ppem):
  scale cvt values: FUnits → F26Dot6 pixels
  execute prep → adjust cvt, set Graphics State defaults

Per glyph:
  load outline from glyf (FUnits)
  scale all points: FUnits → F26Dot6
  consult gasp for this ppem → should we hint?
  if yes: execute glyph bytecode → points snapped to grid
  convert F26Dot6 → f32 coordinates
  → tiny-skia fill_path
  → alpha bitmap
```

## The Bytecode Interpreter

A stack-based virtual machine with ~200 opcodes. All coordinates are F26Dot6
(signed 32-bit with 6 fractional bits = 1/64 pixel precision).

### Key State
- **Stack**: operand stack (typically max 256-1024 entries, font declares max in `maxp`)
- **CVT store**: the scaled+adjusted control values
- **Storage area**: general-purpose i32 slots (font declares count in `maxp`)
- **Graphics State**: ~30 variables controlling instruction behavior:
  - `freedom_vector`, `projection_vector`: axes for point movement/measurement
  - `rp0`, `rp1`, `rp2`: reference point indices
  - `round_state`: how distances are rounded (grid, half-grid, double-grid, etc.)
  - `loop`: repetition counter
  - `minimum_distance`: smallest allowed distance after rounding
  - `cut_in`: threshold for CVT cut-in (when to use CVT vs actual distance)
  - `delta_base`, `delta_shift`: control DELTA instruction behavior
  - `auto_flip`: whether MIRP auto-flips direction
  - `zone_pointer0/1/2`: which zone (glyph or twilight) points refer to
- **Twilight zone**: extra set of points (declared in `maxp`) used for intermediate
  calculations, especially in complex composite glyph hinting
- **FDEF/IDEF tables**: subroutine definitions from fpgm
- **Glyph zone**: the actual glyph points being hinted

### Key Instruction Categories

**Point movement (the core of hinting):**
- `MDAP[r]`: Move Direct Absolute Point — round a point to grid
- `MIAP[r]`: Move Indirect Absolute Point — move point to CVT value
- `MDRP[abcde]`: Move Direct Relative Point — maintain distance from reference
- `MIRP[abcde]`: Move Indirect Relative Point — distance from CVT
- `SHP`: Shift Point by same amount as reference point was moved
- `IP`: Interpolate Point between two reference points
- `IUP[x/y]`: Interpolate Untouched Points — final smoothing pass

**Measurement:**
- `MD[o]`: Measure Distance between two points
- `MPS`: Measure Point Size (ppem)
- `MPPEM`: Measure Pixels Per EM

**Stack/arithmetic:**
- `PUSH`, `NPUSHB`, `NPUSHW`: push values
- `ADD`, `SUB`, `MUL`, `DIV`, `ABS`, `NEG`, `FLOOR`, `CEILING`
- `DUP`, `POP`, `SWAP`, `DEPTH`, `ROLL`

**Control flow:**
- `IF`, `ELSE`, `EIF`: conditional
- `JMPR`, `JROT`, `JROF`: jumps
- `CALL`, `LOOPCALL`, `FDEF`, `ENDF`: subroutines

**Graphics state:**
- `SVTCA`, `SPVTCA`, `SFVTCA`: set vectors to axes
- `SPVFS`, `SFVFS`: set vectors from stack
- `SRP0/1/2`: set reference points
- `SZP0/1/2`: set zone pointers
- `SROUND`, `S45ROUND`, `ROFF`, `RUTG`, `RDTG`, `RTG`, `RTHG`: rounding modes
- `SMD`: set minimum distance
- `SCVTCI`: set CVT cut-in

**Delta (per-ppem pixel tweaks):**
- `DELTAP1/2/3`: delta exception for points
- `DELTAC1/2/3`: delta exception for CVT values

## Implementation Plan

### Phase 1: Table Parsing (~easy)

1. **`GaspTable`** struct: version (u16) + array of `GaspRange { max_ppem: u16, behavior: u16 }`.
   Add a method `fn rendering_flags(&self, ppem: u16) -> GaspBehavior` that returns
   the flags for a given ppem.

2. **Bytecode decoder**: An enum `TTInstruction` covering all ~200 opcodes + a function
   `fn decode_instructions(bytes: &[u8]) -> Vec<TTInstruction>`. Reference:
   https://learn.microsoft.com/en-us/typography/opentype/spec/tt_instructions

3. Proper `FpgmTable` and `PrepTable` structs wrapping the raw bytecode with
   the decoder.

### Phase 2: Interpreter Core (~hard, bulk of work)

1. **Graphics State** struct with all ~30 fields, default values per spec.

2. **Interpreter** struct:
   ```
   struct HintInterpreter {
       stack: Vec<i32>,
       cvt: Vec<F26Dot6>,
       storage: Vec<i32>,
       graphics_state: GraphicsState,
       fdefs: BTreeMap<u32, Vec<TTInstruction>>,
       idefs: BTreeMap<u32, Vec<TTInstruction>>,
       twilight_zone: Vec<HintPoint>,
       glyph_zone: Vec<HintPoint>,
       ppem: u16,
   }
   ```

3. **Execute loop**: fetch instruction, dispatch, modify state. The main complexity
   is in the point movement instructions (MDAP, MDRP, MIRP, MIAP) which must:
   - Project points onto the projection vector
   - Move points along the freedom vector
   - Apply rounding according to round_state
   - Respect minimum_distance and CVT cut-in
   - Track "touched" flags per point per axis

4. **IUP (Interpolate Untouched Points)**: the final pass that smoothly adjusts
   all points not explicitly moved by instructions. This is critical for quality.

### Phase 3: Integration (~medium)

1. **Font load**: after parsing, execute fpgm to populate FDEF/IDEF tables.
   Store in a `HintState` alongside the font.

2. **Size context**: when ppem changes, scale CVT and execute prep.
   Store the result as a `SizeHintState`.

3. **Per-glyph API**: a function like:
   ```
   fn hint_glyph_outline(
       glyph: &SimpleGlyph,
       ppem: f32,
       hint_state: &HintState,
       size_state: &SizeHintState,
   ) -> HintedOutline
   ```
   Returns adjusted point coordinates that the rasterizer uses instead of
   raw scaled coordinates.

4. **wr_glyph_rasterizer change**: in `rasterize_glyph()`, after loading the
   OwnedGlyph, call the hinting function to get adjusted coordinates before
   building the tiny-skia path. The `build_path_from_outline()` function would
   use hinted points instead of raw outline points.

### Phase 4: Testing & Validation

1. Compare rasterized glyphs against FreeType output at various ppem sizes.
2. Use `gasp` table to decide when to hint (some fonts disable hinting at certain sizes).
3. Test with Times New Roman, Arial, Georgia, Verdana (all heavily hinted Microsoft fonts).
4. Visual comparison with Chrome screenshots at 12px, 16px, 24px, 48px.

## Reference Materials

- [TrueType Instruction Set (Microsoft)](https://learn.microsoft.com/en-us/typography/opentype/spec/tt_instructions)
- [TrueType Fundamentals (Microsoft)](https://learn.microsoft.com/en-us/typography/opentype/spec/ttch01)
- [Instructing TrueType Glyphs (Apple)](https://developer.apple.com/fonts/TrueType-Reference-Manual/RM05/Chap5.html)
- [FreeType ttinterp.c](https://github.com/ArtifexSoftware/thirdparty-freetype2/blob/master/src/truetype/ttinterp.c) — reference interpreter (~10K lines C)
- [gasp table spec](https://learn.microsoft.com/en-us/typography/opentype/spec/gasp)
- [cvt table spec](https://learn.microsoft.com/en-us/typography/opentype/spec/cvt)

## Where to Implement

In the **allsorts** crate, on the `pixelsnap` branch. Allsorts already parses
`cvt` and stores per-glyph instruction bytes — the data is there, it just needs
an interpreter. This is a font-level concern, not a rasterizer concern: hinting
transforms the outline before rasterization.

The only change needed in azul's `wr_glyph_rasterizer` is to call the allsorts
hinting API to get adjusted coordinates before building the tiny-skia path.

## Scope Estimate

- Phase 1 (table parsing + decoder): ~500-800 lines Rust
- Phase 2 (interpreter): ~3000-5000 lines Rust (FreeType's is ~10K lines C,
  but Rust is more concise and we can skip some rarely-used instructions initially)
- Phase 3 (integration): ~100-200 lines
- Phase 4 (testing): ongoing

The interpreter is the bulk of the work. A pragmatic approach is to implement
the most common instructions first (the ones Times New Roman / Arial actually use)
and add others as needed, logging warnings for unimplemented opcodes.
