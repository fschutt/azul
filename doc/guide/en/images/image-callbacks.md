---
slug: images/image-callbacks
title: Image Callbacks
language: en
canonical_slug: images/image-callbacks
audience: external
maturity: wip
guide_order: 93
topic_only: false
short_desc: Nodes that draw themselves - CPU or GPU - and how to make one repaint
prerequisites: [hello-world, images, events/callbacks]
tracked_files:
  - core/src/resources.rs
  - core/src/callbacks.rs
  - layout/src/callbacks.rs
  - examples/azul-paint/src/lib.rs
default-search-keys:
  - ImageRef
  - RenderImageCallback
  - RenderImageCallbackInfo
  - update_image_callback
  - update_all_image_callbacks
  - get_bounds
---

# Image Callbacks

An image node does not have to hold pixels. `ImageRef::callback()`
makes a node that produces its content on demand, once layout knows how
big it is - a paint canvas, a chart, a map tile, a video frame.

## Introduction

This is the same mechanism a GL canvas uses, but it is not GL-specific:
the callback may return a raw CPU image just as well as a texture, and
most custom drawing in azul does. [GL Canvas](canvas-gl.md) covers the
texture-and-FXAA path specifically; this page is the general shape.

## The callback

```rust,no_run
use azul::prelude::*;

extern "C" fn render(mut data: RefAny, mut info: RenderImageCallbackInfo) -> ImageRef {
    let size = info.get_bounds().get_logical_size();
    let (w, h) = (size.width.max(1.0) as u32, size.height.max(1.0) as u32);
    // ... draw into a buffer of w x h ...
    ImageRef::null_image(w as usize, h as usize,
                         RawImageFormat::RGBA8, U8VecRef::from(&[][..]))
}

fn canvas(state: RefAny) -> Dom {
    Dom::create_image(ImageRef::callback(
        RenderImageCallback::create(render).to_core(),
        state,
    ))
}
```

The `RefAny` is the callback's own state, separate from the app model,
and it is where a canvas keeps its pixel buffer between frames.

`RenderImageCallbackInfo` is deliberately small. `get_bounds()` is the
laid-out box - the only reliable source of the size, since CSS decides
it. `get_callback_node_id()` is the node being rendered.
`get_gl_context()` is the GL context, present only when a GPU backend
is running, so a callback that wants to work everywhere needs a CPU
path too. `log()` is how the callback reports trouble: it runs inside
the paint pass, where a panic would take the frame with it.

Return an `ImageRef` of *some* size in every path. Returning a
zero-sized or invalid image makes the node draw nothing with no
diagnostic; a `null_image` of the right dimensions at least shows the
node exists.

## Making it repaint

The callback is not re-run every frame. It runs when the node is
mounted, when its size changes, and when you ask:

```rust,ignore
info.update_image_callback(dom_id, node_id);   // this node
info.update_all_image_callbacks();             // every image callback
```

The single-node form is the one to use. The blanket form re-renders
every image callback in the window, which is right after a device
change - a GL context loss, a DPI change - and wasteful otherwise.

`update_image_callback()` takes a `NodeId`, not a `DomNodeId`, and
node-finding methods return a `NodeHierarchyItemId`, whose raw value is
offset by one - zero means "no node". Convert deliberately:

```rust,ignore
if let Some(node) = info.get_node_id_by_marker(marker).into_option() {
    let raw = node.node.into_raw();
    if raw != 0 {
        info.update_image_callback(node.dom, NodeId { inner: raw });
    }
}
```

That marker indirection is how a callback on a *different* node - a
toolbar button, a pointer handler on an overlay - reaches the canvas.
See [Node Tree & Hit Testing](../events/node-tree.md).

## Redrawing only when something changed

A pointer-move handler fires far more often than the canvas needs to be
rasterised. The pattern the paint example uses is a revision counter in
the callback's own state:

```rust,ignore
struct CanvasCache {
    model: RefAny,     // the document being drawn
    image: Option<ImageRef>,
    rendered_rev: u64, // which revision `image` was rendered from
}
```

The render callback compares the model's current revision with
`rendered_rev`, returns the cached `ImageRef` when they match, and
rasterises only when they differ. With that in place, calling
`update_image_callback()` on every pointer move is cheap: most calls
hit the cache, and the ones that do not had real work to do.

## Animation

For continuous animation, drive the repaint from a
[timer](../animations/timers.md) that calls `update_image_callback()`
on each tick, and return `Update::DoNothing` - the node repaints
without a relayout and without running your `layout()` function.

Return `Update::RefreshDom` only when the *structure* changed. An
animation that rebuilds the DOM every frame will be smooth on your
machine and not on anyone else's.

## Swapping an image without a rebuild

For a node whose image is data rather than a drawing, there is no need
for a callback at all:

```rust,ignore
info.change_node_image(dom_id, node_id, new_image, UpdateImageType::Content);
```

`UpdateImageType::Content` replaces the node's image content;
`UpdateImageType::Background` replaces its CSS background image. Both
write into the current display list and skip `layout()` entirely, which
makes them the right call for a frame-by-frame swap - a video player
pushing decoded frames, a sprite animation, a live preview.

The named image cache is the other half of this:

```rust,ignore
info.add_image_to_cache("logo".into(), image_ref);
info.remove_image_from_cache("logo".into());
```

A cached image is addressable by name from any later layout pass, so a
loader thread can decode an image, put it in the cache, and let the
next `layout()` reference it without threading an `ImageRef` through
the model.

`ImageRef` is intern-keyed and reference counted: handing the same
clone to two consecutive `Dom`s reuses one upload, and the GPU memory
is freed when the last clone drops. To change pixels you build a new
`ImageRef` - which is why a swap costs an upload and a redraw through a
callback does not.

## Cross-references

- [GL Canvas](canvas-gl.md) - the GPU texture path in detail.
- [Clip Masks](clip-masks.md) - masking a node with an image.
- [Images](../images.md) - `ImageRef` construction and sizing.
