# Stress-test fonts (TEST-ONLY)

Synthetic fonts that exercise the `text3` text-layout / shaping engine with
hand-chosen, **round** metrics so Rust tests can assert **exact arithmetic**
instead of "roughly this wide".

* **Not shipped.** These are loaded at runtime with `std::fs::read` (never
  `include_bytes!`), so they live in this excluded test dir and are absent from
  the published `azul-layout` crate. (Contrast: the committed `azul-mock-mono` /
  `azul-mock-wide` fonts from `scripts/gen_mock_fonts.py` *are* shipped.)
* **Regenerate:** `python3 scripts/gen_stress_fonts.py` (fontTools ≥ 4.63).
  Output is **byte-for-byte deterministic** — `head.created`/`modified` are
  pinned to `0` and nothing embeds timestamps/UUIDs, so re-running reproduces
  identical files. If you change a metric here, update this manifest.
* **Every glyph outline is one filled rectangle** — a black box `(50, 0)` to
  `(advance-50, 700)`. Nothing about glyph *shape* matters; only *metrics* and
  the presence/behaviour of OpenType layout tables (GSUB/GPOS/vmtx) do. A CPU
  rasterizer will draw these as visible coloured boxes.
* **Except sections 7 to 9:** those are small subsets of real OFL fonts, kept
  for what real Arabic and Devanagari fonts do and the synthetic ones cannot
  (dots split off their letters, mark and cursive anchors, dotted circles for
  broken syllables). They are not made by the script above; each one's licence
  text sits next to it.

## Shared conventions (unless a font overrides them)

| property             | value |
|----------------------|-------|
| unitsPerEm           | 1000  |
| ascent (hhea/typo)   | 800   |
| descent (hhea/typo)  | -200  |
| lineGap              | 0     |
| glyph box y-range    | 0 .. 700 |
| glyph box x-inset    | 50 units each side (lsb = 50 for boxes) |
| style / weight       | Regular / 400 |
| vendor id            | `AZUL` |
| `.notdef`            | glyph id 0, a visible box, advance = the font's first-listed advance |
| space (U+0020)       | blank (no contour) but carries a full advance |

**Glyph-id formula for the ASCII fonts** (liga, kern, prop): the 95 printable
ASCII codepoints `0x20..0x7E` map to glyph ids `1..95` in codepoint order, i.e.
`gid(cp) = cp - 0x1F` (space `0x20` → gid 1, `~` `0x7E` → gid 95). Glyph id 0 is
always `.notdef`.

### hmtx / vmtx compression caveat

fontTools compresses trailing equal advances (standard OpenType): `hhea.
numberOfHMetrics` / `vhea.numberOfVMetrics` may be **less than numGlyphs**. This
does **not** change any glyph's advance — glyphs at index ≥ `numberOf*Metrics`
inherit the last full record's advance (which is identical), and their side
bearing is still stored per-glyph. Read advances through a proper hmtx lookup,
not by assuming `numberOfHMetrics == numGlyphs`. Observed values:

| font        | numGlyphs | numberOfHMetrics | numberOfVMetrics |
|-------------|-----------|------------------|------------------|
| liga        | 99        | 99               | —                |
| kern        | 96        | 1                | —                |
| arabic      | 26        | 1                | —                |
| vertical    | 18        | 1                | 1                |
| cjk-massive | 20994     | 1                | —                |
| prop        | 96        | 90               | —                |

All advances are still exactly as documented per font below.

---

## 1. azul-mock-liga.ttf — GSUB ligatures

| | |
|---|---|
| family name | **Azul Mock Liga** |
| upem / ascent / descent | 1000 / 800 / -200 |
| numGlyphs | 99 (`.notdef` + 95 ASCII + 3 ligatures) |
| cmap | 95 entries, U+0020..U+007E → gid 1..95 (`gid = cp - 0x1F`) |
| advance (all cmapped glyphs) | **500** (monospace); `.notdef` = 500 |
| tables | `cmap glyf head hhea hmtx loca maxp name OS/2 post GSUB` |

**Ligature glyphs** (not in cmap — reachable only via GSUB `liga`):

| glyph | gid | advance | input sequence (codepoints) |
|-------|-----|---------|-----------------------------|
| `f_i`   | 96 | **700** | `f`(U+0066) + `i`(U+0069) |
| `f_f`   | 97 | **700** | `f`(U+0066) + `f`(U+0066) |
| `f_f_i` | 98 | **900** | `f`(U+0066) + `f`(U+0066) + `i`(U+0069) |

Relevant base gids: `f` = U+0066 = gid 71, `i` = U+0069 = gid 74.

**Shaping:** GSUB `liga` (scripts DFLT + latn), one ligature lookup (type 4),
ligature set keyed by first glyph `f`. Order is **ffi first**, then fi, then ff,
so the 3-component ligature wins within the set:
* `f i`   → `f_i`   — glyph count 2→1, total advance 500+500=1000 → **700**
* `f f`   → `f_f`   — glyph count 2→1, total advance 1000 → **700**
* `f f i` → `f_f_i` — glyph count 3→1, total advance 1500 → **900**

---

## 2. azul-mock-kern.ttf — GPOS pair kerning

| | |
|---|---|
| family name | **Azul Mock Kern** |
| upem / ascent / descent | 1000 / 800 / -200 |
| numGlyphs | 96 (`.notdef` + 95 ASCII) |
| cmap | 95 entries, U+0020..U+007E → gid 1..95 |
| advance (all glyphs) | **500** (monospace); `.notdef` = 500 |
| tables | `cmap glyf head hhea hmtx loca maxp name OS/2 post GPOS` |

**Shaping:** GPOS `kern` (scripts DFLT + latn), one PairPos lookup (type 2):

* pair `A`(U+0041, gid 34) + `V`(U+0056, gid 55): **Value1.XAdvance = -200**.

So `A V` un-kerned = 500+500 = 1000; kerned advance of the pair = **800**
(the `A` advance is reduced by 200; `V` unchanged). No glyph substitution — the
glyph count stays 2.

---

## 3. azul-mock-arabic.ttf — Arabic joining + required ligature (RTL)

| | |
|---|---|
| family name | **Azul Mock Arabic** |
| upem / ascent / descent | 1000 / 800 / -200 |
| numGlyphs | 26 |
| advance (every glyph) | **500** (incl. `.notdef`, `space`, and `lam_alef`) |
| tables | `cmap glyf head hhea hmtx loca maxp name OS/2 post GSUB GDEF` |

Uses **real Arabic codepoints** — the shaper derives joining behaviour from
Unicode joining types, so do **not** remap to Latin.

**cmap (6 entries → nominal glyphs):**

| codepoint | letter | nominal glyph | gid | joining type |
|-----------|--------|---------------|-----|--------------|
| U+0628 | beh  | `beh`   | 1 | dual-joining |
| U+062A | teh  | `teh`   | 2 | dual-joining |
| U+0644 | lam  | `lam`   | 3 | dual-joining |
| U+0645 | meem | `meem`  | 4 | dual-joining |
| U+0627 | alef | `alef`  | 5 | right-joining |
| U+0020 | space| `space` | 6 | (blank, adv 500) |

**Positional-form glyphs** (not in cmap; produced by GSUB single subs):

| letter | isol | init | medi | fina |
|--------|------|------|------|------|
| beh  | gid 7  | gid 8  | gid 9  | gid 10 |
| teh  | gid 11 | gid 12 | gid 13 | gid 14 |
| lam  | gid 15 | gid 16 | gid 17 | gid 18 |
| meem | gid 19 | gid 20 | gid 21 | gid 22 |
| alef | gid 23 | — | — | gid 24 |

`alef` is right-joining, so it has **only** isol + fina forms (no init/medi).
`lam_alef` ligature glyph = **gid 25**.

**GSUB** (scripts DFLT + arab). Single-substitution lookups, one per feature —
each substitutes the nominal glyph for its positional variant:

* `isol`: beh→beh.isol, teh→teh.isol, lam→lam.isol, meem→meem.isol, alef→alef.isol
* `init`: beh→beh.init, teh→teh.init, lam→lam.init, meem→meem.init
* `medi`: beh→beh.medi, teh→teh.medi, lam→lam.medi, meem→meem.medi
* `fina`: beh→beh.fina, teh→teh.fina, lam→lam.fina, meem→meem.fina, alef→alef.fina

**Required ligature** — GSUB `rlig`, one ligature lookup (type 4). Every input
variant collapses lam+alef to the single `lam_alef` glyph (redundant coverage so
the ligature fires regardless of feature-ordering between positional and rlig):

* `lam.init alef.fina` → `lam_alef`
* `lam.medi alef.fina` → `lam_alef`
* `lam alef` (nominal) → `lam_alef`

**GDEF:** GlyphClassDef marks `lam_alef` as class 2 (ligature); all other glyphs
class 1 (base).

**Expected shaping (logical order shown; final layout is RTL):**

| input (logical) | glyphs out | note |
|-----------------|-----------|------|
| `beh`             | [beh.isol]                     | isolated |
| `beh teh`         | [beh.init, teh.fina]           | init + fina |
| `beh teh meem`    | [beh.init, teh.medi, meem.fina]| init + medi + fina |
| `lam alef`        | [lam_alef]                     | lam.init+alef.fina → rlig; count 2→1, adv 1000→500 |
| `beh lam alef`    | [beh.init, lam_alef]           | lam.medi+alef.fina → rlig |

The ligature signal is the **glyph-count reduction** (2→1); all advances are 500.

---

## 4. azul-mock-vertical.ttf — vertical metrics (vmtx/vhea)

| | |
|---|---|
| family name | **Azul Mock Vertical** |
| upem / ascent / descent | 1000 / 800 / -200 (horizontal) |
| numGlyphs | 18 (`.notdef` + 16 ideographs + space) |
| horizontal advance (all) | **1000** |
| tables | `cmap glyf head hhea hmtx loca maxp name OS/2 post vhea vmtx` |

**cmap (17 entries):** U+4E00..U+4E0F (16 CJK ideographs) → gid 1..16
(`gid = cp - 0x4DFF`), plus U+0020 space → gid 17.

**Vertical metrics:**

* `vmtx`: every glyph **advanceHeight = 1000**; topSideBearing = 100 for the box
  glyphs (vertical origin 800 − box top 700), 0 for the blank space.
* `vhea`: vertAscender = **500**, vertDescender = **-500**, lineGap = 0,
  advanceHeightMax = 1000.

So a run of N ideographs in `vertical-rl`/`vertical-lr` occupies exactly
`N * 1000` units of block height at 1000 upem (1 em per glyph).

---

## 5. azul-mock-cjk-massive.ttf — many-glyphs / cmap-size stress

| | |
|---|---|
| family name | **Azul Mock CJK Massive** |
| upem / ascent / descent | 1000 / 800 / -200 |
| numGlyphs | **20994** (`.notdef` + 20992 ideographs + space) |
| advance (all) | **1000** |
| tables | `cmap glyf head hhea hmtx loca maxp name OS/2 post` (no GSUB/GPOS) |
| post | format **3.0** (no glyph-name strings, to keep the file small) |

**cmap (20993 entries):** the full CJK Unified Ideographs block
**U+4E00..U+9FFF** → gid 1..20992 (`gid = cp - 0x4DFF`), plus U+0020 space →
gid 20993. Every ideograph is a 1000-wide box. File size ≈ 636 KiB.

Purpose: stress the many-glyphs path (large cmap, large glyf/loca/hmtx). Actual
glyph count used = **20994** (full block, not the reduced U+4E00..U+5FFF option).

---

## 6. azul-mock-prop.ttf — proportional advances (line-break / justify)

| | |
|---|---|
| family name | **Azul Mock Prop** |
| upem / ascent / descent | 1000 / 800 / -200 |
| numGlyphs | 96 (`.notdef` + 95 ASCII) |
| cmap | 95 entries, U+0020..U+007E → gid 1..95 |
| tables | `cmap glyf head hhea hmtx loca maxp name OS/2 post` |

**Advance rule:** every printable ASCII glyph = **500**, with these exceptions:

| advance | codepoints |
|---------|-----------|
| **250** | space `U+0020` (also `.notdef` = 250) |
| **200** | `,` U+002C, `.` U+002E, `i` U+0069, `l` U+006C |
| **900** | `M` U+004D, `W` U+0057, `m` U+006D, `w` U+0077 |
| **500** | all other 86 printable ASCII codepoints |

Advance histogram over the 95 ASCII glyphs: `{200: 4, 250: 1, 500: 86, 900: 4}`.

Purpose: varying widths make Knuth-Plass line-breaking and justification
non-trivial. Worked example — the string `"Ww ll"` (no ligatures/kerning here):
`W`(900) + `w`(900) + space(250) + `l`(200) + `l`(200) = **2450** units =
2.45 em, versus 2450/1000·font_size px.

---

## 7. NotoSansArabic-DotsAndMarks.ttf — dots split off letters, mark anchors

| | |
|---|---|
| family name | **Noto Sans Arabic** (Regular 2.012, a subset) |
| licence | SIL OFL 1.1, `NotoSansArabic-OFL.txt` |
| upem | 1000 |
| numGlyphs | 42 |
| cmap | U+0020, U+0627 U+0628 U+062A U+062B U+062D U+062F U+0639 U+0644 U+0645, U+0650 |
| GSUB | `ccmp init medi fina rlig locl aalt`; `ccmp` splits beh / teh / theh into a dotless letter plus a separate dot glyph (one-to-many substitution) |
| GPOS | `mark` (mark to base) and `mkmk` (mark to mark) only, no kerning |

**Made with:** `pyftsubset NotoSansArabic-Regular.ttf --text="بالعالم تحدث بِ"
--layout-features='*' --no-hinting`.

**What HarfBuzz 15.0.0 gives** (font units, RTL, glyphs in logical order,
offsets relative to the pen at the glyph):

| text | glyph | cluster (byte) | offset |
|------|-------|----------------|--------|
| `بالعالم` | dot below of the beh | 0 | (31, -3) |
| `تحدث` | two dots of the teh | 0 | (5, -59) |
| `تحدث` | three dots of the theh | 6 | (344, -165) |
| `بِ` | dot below of the beh | 0 | (410, -23) |
| `بِ` | kasra U+0650 (mark to mark on the dot) | 2 | (344, -211) |

---

## 8. Amiri-CursiveSample.ttf — cursive attachment

| | |
|---|---|
| family name | **Amiri** (Regular 1.002, a subset) |
| licence | SIL OFL 1.1, `Amiri-OFL.txt` |
| upem | 1000 |
| numGlyphs | 51 |
| cmap | U+0645 meem, U+062C jeem, U+062F dal |
| GPOS | `curs` only: one lookup, flag `RightToLeft` + `IgnoreMarks` |

**Made with:** `pyftsubset Amiri-Regular.ttf --text="مجد" --layout-features='*'
--no-hinting`.

`مجد` shapes (logical order) to the initial meem (advance 550, exit anchor
(0, 254)), the medial jeem (advance 55, entry anchor (55, 235)) and the final
dal (advance 471). The lookup's `RightToLeft` flag makes the first glyph the
one that moves, so HarfBuzz 15.0.0 draws the meem at offset (0, -19): its exit
then meets the jeem's entry. The advances do not change, the anchors' x
already match them.

---

## 9. NotoSansDevanagari-BrokenSyllables.ttf — dotted circles

| | |
|---|---|
| family name | **Noto Sans Devanagari** (Regular 2.000, a subset) |
| licence | SIL OFL 1.1, `NotoSansDevanagari-OFL.txt` |
| upem | 1000 |
| numGlyphs | 8 |
| cmap | U+0020, U+0915 ka, U+093E aa sign, U+093F i sign, U+25CC dotted circle |
| GSUB | `abvs` only; no GPOS |

**Made with:** `pyftsubset NotoSansDevanagari-Regular.ttf --text="◌ािक "
--layout-features='*' --no-hinting`.

A vowel sign with no consonant before it is a broken syllable: the shaper
draws it on a dotted circle it inserts, which stands for no text. What
HarfBuzz 15.0.0 gives (glyphs in order, clusters in UTF-8 bytes):

| text | glyphs (cluster) |
|------|------------------|
| `ाक` | dotted circle (0), aa sign (0), ka (3) |
| `िक` | i sign (0), dotted circle (0), ka (3) - the i sign is drawn before its base |
| `◌िक` | i sign (0), dotted circle (0), ka (6) - here the dotted circle is the text's own |
