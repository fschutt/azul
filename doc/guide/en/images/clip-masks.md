---
slug: images/clip-masks
title: Clip Masks
language: en
canonical_slug: images/clip-masks
audience: external
maturity: wip
guide_order: 94
topic_only: false
short_desc: Clipping a node to an image's alpha, and to an SVG path
prerequisites: [hello-world, images]
tracked_files:
  - core/src/dom.rs
  - layout/src/solver3/display_list.rs
default-search-keys:
  - ImageMask
  - with_clip_mask
  - change_node_image_mask
  - SvgNodeData
---

# Clip Masks

A clip mask decides, per pixel, how much of a node is drawn. Anything
that produces an image can be a mask - including an image callback,
which is what makes a mask animatable.

## Introduction

Masking happens at paint time, not at layout time. A masked node
occupies exactly the same box it would otherwise; the mask only
changes which of its pixels reach the screen. That is why masking
never reflows anything, and why a mask can change every frame cheaply.

Azul has two kinds: a raster mask built from an image, and a vector
clip built from SVG geometry.

## An image as a mask

```rust,no_run
use azul::prelude::*;

let mask = ImageMask {
    image: mask_image,                       // alpha is what counts
    rect: LogicalRect::new(
        LogicalPosition::new(0.0, 0.0),
        LogicalSize::new(200.0, 200.0),
    ),
    repeat: false,
};

let avatar = Dom::create_image(photo).with_clip_mask(mask);
```

The mask image's alpha channel is the mask: opaque pixels show the
node, transparent ones hide it, and partial alpha gives a soft edge -
which is how you get an anti-aliased circular avatar without a
rounded-corner approximation.

`rect` is **element-local**. Its origin is measured from the node's own
paint box, so `(0, 0)` is the node's top-left corner and the mask moves
with the node. A mask smaller than the node clips everything outside
it; a mask larger than the node is cropped by the node's box.

`repeat` is part of the struct but is **not currently honoured** - the
mask is drawn once, at `rect`. Tile the mask image itself if you need
a repeating pattern.

## An SVG path as a mask

A node carrying SVG path geometry is clipped to that path instead. The
path is rasterised against the node's box, in the coordinate system of
the nearest `<svg>` ancestor's `viewBox` - so geometry authored at 16
units inside a `viewBox="0 0 16 16"` fills the node rather than a
sixteenth of it.

This path needs the `cpurender` feature. A build without it prints one
warning and draws the node unclipped, which is worth knowing before
concluding that a clip "does not work".

## Changing the mask from a callback

```rust,ignore
info.change_node_image_mask(dom_id, node_id, new_mask);
```

Like the other in-place mutations, this writes into the current
display list: no `layout()`, no diff, no reflow. Call it as often as
you like.

That makes the obvious animations cheap. A wipe transition is a mask
whose `rect` grows each tick; a spotlight is a radial-gradient mask
whose origin follows the cursor; a progress ring is a mask swapped for
the next frame of a pre-rendered sequence. Drive any of them from a
[timer](../animations/timers.md) and return `Update::DoNothing`.

The other way to animate a mask is not to change it at all: build the
mask from `ImageRef::callback()` and let the mask *redraw itself*, then
call `update_image_callback()` when its input changes. See
[Image Callbacks](image-callbacks.md).

## Cross-references

- [Image Callbacks](image-callbacks.md) - nodes that draw themselves.
- [Images](../images.md) - `ImageRef` construction.
- [SVG](svg.md) - the geometry model the vector clip uses.
