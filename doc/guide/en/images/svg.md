---
slug: svg
title: SVG
language: en
canonical_slug: svg
audience: external
maturity: wip
guide_order: 71
topic_only: false
short_desc: Parsing and rendering SVG documents — the embedded engine, supported features, and known gaps.
prerequisites: [images]
tracked_files:
  - core/src/svg.rs
  - core/src/svg_path_parser.rs
last_generated_rev: 7ecd570e4c0c3584e5107e770058c16cb59fa6e7
generated_at: 2026-05-02T12:00:00Z
---

# SVG

> **WIP** — SVG geometry types are stable; the higher-level `usvg`-backed
> `Svg` parser and the GPU stroke pipeline still have rough edges.

SVG support in azul is built around a small, copy-friendly geometry model in
`core/src/svg.rs`. You construct paths in memory (or parse them from a `d`
string), tessellate them once, and either upload the result to a GPU buffer
or rasterize it via the CPU SVG path. Higher-level XML-driven SVG (image
embedding, gradients, full document parsing) goes through the `Svg`
helper backed by `usvg`.

## Geometry types

```rust,ignore
pub enum SvgPathElement {
    Line(SvgLine),
    QuadraticCurve(SvgQuadraticCurve),
    CubicCurve(SvgCubicCurve),
}

pub struct SvgPath { pub items: SvgPathElementVec }
pub struct SvgMultiPolygon { pub rings: SvgPathVec }
```

A `SvgPath` is one open or closed contour built from line and Bézier
segments. Several paths combine into an `SvgMultiPolygon` — interior holes
are represented by reversing the winding order of a ring (see
`core/src/svg.rs:439`). The fill rule is `NonZero` by default; switch to
`EvenOdd` via `SvgFillStyle` when interior overlaps need to cancel.

`SvgNode` (`core/src/svg.rs:500`) wraps any of the above plus a couple of
primitives so a single value can describe an entire `<path>` element:

| Variant | Holds |
|---|---|
| `Path` | one contour |
| `MultiPolygon` | a polygon with optional holes |
| `MultiPolygonCollection` | many polygons rendered as one batch |
| `MultiShape` | mixed paths, circles, rects, holes |
| `Circle`, `Rect` | analytic primitives |

## Parsing path data

`parse_svg_path_d` (`core/src/svg_path_parser.rs:329`) takes the contents of
an SVG `d=""` attribute and returns an `SvgMultiPolygon`. Every command
described in the SVG 1.1 path grammar is supported, including arcs (`A`),
which are converted to up to four cubic Béziers per quarter turn (the
constant `KAPPA = 0.5522847498` is the standard quarter-circle Bézier
approximation).

```rust,no_run
# use azul::svg::parse_svg_path_d;
let polygon = parse_svg_path_d(
    "M 10 80 C 40 10, 65 10, 95 80 S 150 150, 180 80"
).expect("invalid path");
```

Implicit command repeats (e.g. multiple coordinate pairs after one `M`)
work as in the spec: subsequent pairs after `M`/`m` become `L`/`l`. Errors
surface as `SvgPathParseError`:

| Variant | Means |
|---|---|
| `EmptyPath` | the input was whitespace only |
| `UnexpectedChar { pos, ch }` | a non-command byte where one was expected |
| `ExpectedNumber { pos }` | the parser hit a delimiter expecting a coordinate |
| `InvalidArcFlag { pos }` | the large-arc / sweep flag was not 0 or 1 |

## Closed and open paths

`SvgPath::is_closed()` returns `true` when the last segment's end equals
the first segment's start. `close()` appends an explicit line segment if
the path is not already closed (matching the SVG `Z` command). Tessellation
treats open paths as strokes only — a fill on an open path produces an
empty triangle list.

## Tessellation

A polygon becomes triangles via the `tessellate_fill` and
`tessellate_stroke` methods on `SvgMultiPolygon`. Each returns a
`TessellatedSvgNode` (`core/src/svg.rs:740`):

```rust,ignore
pub struct TessellatedSvgNode {
    pub vertices: SvgVertexVec,  // (x, y) pairs
    pub indices: U32Vec,          // triangle list
}
```

Tessellation tolerance defaults to `0.1` CSS pixels — finer than that adds
vertices without visible benefit. Set a larger value for far-away or
small-on-screen polygons:

```rust,no_run
# use azul::svg::{SvgFillStyle, SvgMultiPolygon};
# fn polygon() -> SvgMultiPolygon { panic!() }
let mut style = SvgFillStyle::default();
style.tolerance = 0.5;
let mesh = polygon().tessellate_fill(style);
```

Stroke options live in `SvgStrokeStyle`:

| Field | Meaning |
|---|---|
| `line_width` | stroke width in user units |
| `start_cap` / `end_cap` | `Butt`, `Square`, `Round` |
| `line_join` | `Miter`, `MiterClip`, `Round`, `Bevel` |
| `miter_limit` | ratio of miter length to stroke width (default `4.0`) |
| `dash_pattern` | optional `SvgDashPattern` for dashed strokes |

## Drawing on the GPU

Once tessellated, a node can be uploaded once and drawn many times by
wrapping it in a `TessellatedGPUSvgNode`:

```rust,no_run
# use azul::svg::*;
# use azul::gl::GlContextPtr;
# fn ctx() -> GlContextPtr { panic!() }
# fn mesh() -> TessellatedSvgNode { panic!() }
let gpu_mesh = TessellatedGPUSvgNode::new(&mesh(), ctx());
```

Per-frame drawing happens inside an image-rendering callback (see
[Canvas and GL Textures](canvas-gl.md)) by calling
`Texture::draw_tesselated_svg_gpu_node`, optionally with a transform list
to translate, rotate, or scale the geometry.

## Drawing on the CPU

Two helpers in `core/src/svg_path_parser.rs` build common shapes without
parsing:

| Function | Returns |
|---|---|
| `svg_circle_to_paths(cx, cy, r)` | one `SvgPath` (4 cubic Béziers, kappa-approximated) |
| `svg_rect_to_path(x, y, w, h, rx, ry)` | a rectangle with optional corner radii |

Both produce `SvgPath`, the same type your tessellator consumes. CPU
rasterization is performed by the layout crate when the geometry is set as
a node-data SVG attribute; this path is a good fallback for printing,
testing, and headless rendering.

## XML-level SVG

`Svg` and `SvgXmlNode` (`core/src/svg.rs:1268`/`:1262`) are opaque handles
to a `usvg` document. The crate-internal `parse_svg` constructor reads a
full SVG document with `SvgParseOptions`:

| Option | Default |
|---|---|
| `dpi` | `96.0` |
| `default_font_family` | `"Times New Roman"` |
| `font_size` | `12.0` |
| `shape_rendering` | `GeometricPrecision` |
| `text_rendering` | `OptimizeLegibility` |
| `image_rendering` | `OptimizeQuality` |
| `fontdb` | `System` |

The result is rendered to a `RawImage` via `SvgRenderOptions`, which lets
you fix the output size (`SvgFitTo::Width`, `SvgFitTo::Height`,
`SvgFitTo::Zoom`) or use the document's intrinsic viewport. Wrap the
returned image in `ImageRef::new_rawimage` and insert it like any other
raster image.

## Combining many polygons

`TessellatedSvgNode::from_nodes` flattens a slice of meshes into a single
vertex/index buffer. This matters for performance when drawing thousands
of tiny polygons (for example, the `opengl` example tessellates thousands
of map polygons and uploads exactly one fill mesh and one stroke mesh).
