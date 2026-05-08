---
slug: svg
title: SVG
language: en
canonical_slug: svg
audience: external
maturity: wip
guide_order: 71
topic_only: false
short_desc: Parsing and rendering SVG documents
prerequisites: [images]
tracked_files:
  - core/src/svg.rs
  - core/src/svg_path_parser.rs
last_generated_rev: 7ecd570e4c0c3584e5107e770058c16cb59fa6e7
generated_at: 2026-05-02T12:00:00Z
---

# SVG

> WIP. SVG geometry types are stable. The higher-level `Svg` parser and the GPU stroke pipeline still have rough edges.

SVG support in azul is built around a small, copy-friendly geometry model. You construct paths in memory, tessellate them once, and either upload the result to a GPU buffer or rasterize it via the CPU SVG path. Higher-level XML-driven SVG (image embedding, gradients, full document parsing) goes through the `Svg` helper.

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

An `SvgPath` is one open or closed contour built from line and Bézier segments. Several paths combine into an `SvgMultiPolygon`. Interior holes are represented by reversing the winding order of a ring. The fill rule defaults to `Winding`. Switch to `EvenOdd` via `SvgFillStyle.fill_rule` when interior overlaps need to cancel.

Construct a path with `SvgPath::create(items)` and a multi-polygon with `SvgMultiPolygon::create(rings)`. Inspect with `SvgPath::is_closed`, `get_start`, `get_end`, `get_bounds`. Modify with `SvgPath::close`, `reverse`, `join_with`.

`SvgRect` covers analytic rectangles with optional corner radii (`x`, `y`, `width`, `height`, `radius_top_left`, `radius_top_right`, `radius_bottom_left`, `radius_bottom_right`). Use `SvgRect::expand`, `get_center` for layout helpers.

## Closed and open paths

`SvgPath::is_closed()` returns `true` when the last segment's end equals the first segment's start. `close()` appends an explicit line segment if the path isn't already closed (matching the SVG `Z` command). Tessellation treats open paths as strokes only. A fill on an open path produces an empty triangle list.

## Tessellation

A polygon becomes triangles via `tessellate_fill` and `tessellate_stroke` on `SvgMultiPolygon`. Each returns a `TessellatedSvgNode`:

```rust,ignore
pub struct TessellatedSvgNode {
    pub vertices: SvgVertexVec,  // (x, y) pairs
    pub indices: U32Vec,          // triangle list
}
```

`SvgFillStyle` controls fill behavior:

- `fill_rule: SvgFillRule`. `Winding` (default) or `EvenOdd`.
- `tolerance: f32`. Curve subdivision tolerance in CSS pixels.
- `anti_alias: bool`, `high_quality_aa: bool`.
- `line_join: SvgLineJoin`, `miter_limit: f32`.
- `transform`. An optional pre-tessellation transform.

```rust,no_run
# use azul::svg::{SvgFillStyle, SvgMultiPolygon};
# fn polygon() -> SvgMultiPolygon { panic!() }
let mut style = SvgFillStyle::default();
style.tolerance = 0.5;
let mesh = polygon().tessellate_fill(style);
```

Stroke options live in `SvgStrokeStyle`:

- `line_width`. Stroke width in user units.
- `start_cap` / `end_cap`. `SvgLineCap::Butt`, `SvgLineCap::Square`, `SvgLineCap::Round`.
- `line_join`. `SvgLineJoin::Miter`, `MiterClip`, `Round`, `Bevel`.
- `miter_limit`. Ratio of miter length to stroke width.
- `dash_pattern`. Optional `SvgDashPattern` for dashed strokes.

## Drawing on the GPU

Once tessellated, a node can be uploaded once and drawn many times by wrapping it in a `TessellatedGPUSvgNode`:

```rust,no_run
# use azul::svg::*;
# use azul::gl::GlContextPtr;
# fn ctx() -> GlContextPtr { panic!() }
# fn mesh() -> TessellatedSvgNode { panic!() }
let gpu_mesh = TessellatedGPUSvgNode::create(&mesh(), ctx());
```

Per-frame drawing happens inside an image-rendering callback (see [Canvas and GL Textures](canvas-gl.md)) by calling `Texture::draw_tesselated_svg_gpu_node`, optionally with a transform list to translate, rotate, or scale the geometry.

## XML-level SVG

`Svg` is a handle to a parsed SVG document. The constructors `Svg::from_string` and `Svg::from_bytes` read a full SVG document with `SvgParseOptions`:

- `dpi`. Default `96.0`.
- `default_font_family`.
- `font_size`.
- `shape_rendering`, `text_rendering`, `image_rendering`.
- `fontdb`.
- `keep_named_groups`, `languages`, `relative_image_path`.

The result is rendered to a `RawImage` via `SvgRenderOptions`, which lets you fix the output size (`SvgFitTo::Width`, `SvgFitTo::Height`, `SvgFitTo::Zoom`, `SvgFitTo::Original`) or use the document's intrinsic viewport, plus `target_size`, `background_color`, and `transform`. Wrap the returned image in `ImageRef::new_rawimage` and insert it like any other raster image.

## Combining many polygons

`TessellatedSvgNode::from_nodes` flattens a slice of meshes into a single vertex/index buffer. This matters for performance when drawing thousands of tiny polygons (for example, the `opengl` example tessellates thousands of map polygons and uploads exactly one fill mesh and one stroke mesh).

## Rendering SVG headlessly

The whole SVG-to-pixels pipeline runs without a window. There are two
shapes of "headless SVG":

**Inline SVG inside an XHTML document.** The XML parser recognises
`<svg>` tags inside `<body>` and turns them into vector nodes on the
returned `Dom`. Build the Dom with `Dom::create_from_parsed_xml`,
launch the binary with `AZ_BACKEND=headless`, and the framework
rasterises into an in-memory framebuffer instead of opening a
window. Pair with `AZ_DEBUG=<port>` and a `take_screenshot`
request returns a base64 PNG.

```sh
AZ_BACKEND=headless AZ_DEBUG=8765 ./my_svg_renderer &
curl -s -X POST http://localhost:8765/ \
    -d '{"type":"take_screenshot"}' \
  | jq -r '.data.value' | base64 -d > out.png
```

This is the path covered in [Headless Rendering](../headless-rendering.md).
The DOM-side example is in [Document Object Model](../dom.md#inline-svg).

**Standalone SVG file.** When you have an `.svg` on disk and want a
raster out, parse with `Svg::from_string` (or `Svg::from_bytes`) and
let the renderer produce a `RawImage` directly. This skips the Dom
entirely:

```rust,no_run
use azul::prelude::*;

let svg_bytes = std::fs::read("logo.svg").unwrap();
let svg = Svg::from_bytes(svg_bytes.into(), SvgParseOptions::default()).unwrap();
let raw = svg.render(SvgRenderOptions::default()).unwrap();
let image = ImageRef::new_rawimage(raw);
// `image` is now embeddable as `Dom::create_image(image)`, or you
// can write the raw RGBA buffer to a PNG with the `image` crate.
```

Both paths share the SVG renderer described above. The choice is just
about whether you want the SVG rendered next to other Dom content
(use the inline form) or as an isolated raster image (use the
standalone form).

## Coming Up Next

- [GL Canvas](canvas-gl.md) — Embedding an OpenGL canvas inside a Dom node
- [Images](../images.md) — Loading raster images and CSS backgrounds
- [Styling with CSS](../styling.md) — Stylesheets, selectors, and the cascade
