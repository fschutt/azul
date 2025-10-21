/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

/*!
A GPU based renderer for the web.

It serves as an experimental render backend for [Servo](https://servo.org/),
but it can also be used as such in a standalone application.

# External dependencies
WebRender currently depends on [FreeType](https://www.freetype.org/)

# Api Structure
The main entry point to WebRender is the [`crate::Renderer`].

By calling [`Renderer::new(...)`](crate::Renderer::new) you get a [`Renderer`], as well as
a [`RenderApiSender`](api::RenderApiSender). Your [`Renderer`] is responsible to render the
previously processed frames onto the screen.

By calling [`yourRenderApiSender.create_api()`](api::RenderApiSender::create_api), you'll
get a [`RenderApi`](api::RenderApi) instance, which is responsible for managing resources
and documents. A worker thread is used internally to untie the workload from the application
thread and therefore be able to make better use of multicore systems.

## Frame

What is referred to as a `frame`, is the current geometry on the screen.
A new Frame is created by calling [`set_display_list()`](api::Transaction::set_display_list)
on the [`RenderApi`](api::RenderApi). When the geometry is processed, the application will be
informed via a [`RenderNotifier`](api::RenderNotifier), a callback which you pass to
[`Renderer::new`].
More information about [stacking contexts][stacking_contexts].

[`set_display_list()`](api::Transaction::set_display_list) also needs to be supplied with
[`BuiltDisplayList`](api::BuiltDisplayList)s. These are obtained by finalizing a
[`DisplayListBuilder`](api::DisplayListBuilder). These are used to draw your geometry. But it
doesn't only contain trivial geometry, it can also store another
[`StackingContext`](api::StackingContext), as they're nestable.

[stacking_contexts]: https://developer.mozilla.org/en-US/docs/Web/CSS/CSS_Positioning/Understanding_z_index/The_stacking_context
*/

#![allow(
    clippy::unreadable_literal,
    clippy::new_without_default,
    clippy::too_many_arguments,
    unexpected_cfgs,
    unused
)]

// Cribbed from the |matches| crate, for simplicity.
macro_rules! matches {
    ($expression:expr, $($pattern:tt)+) => {
        match $expression {
            $($pattern)+ => true,
            _ => false
        }
    }
}

#[macro_use]
extern crate bitflags;
#[macro_use]
extern crate lazy_static;
#[macro_use]
extern crate log;
#[macro_use]
extern crate derive_more;
extern crate svg_fmt;

#[macro_use]
mod profiler;
mod telemetry;

mod api_resources;
mod batch;
mod border;
mod box_shadow;
mod bump_allocator;
#[cfg(any(feature = "capture", feature = "replay"))]
mod capture;
mod clip;
mod command_buffer;
mod composite;
mod compositor;
mod debug_colors;
mod debug_font_data;
mod debug_item;
mod device;
mod ellipse;
mod filterdata;
mod frame_allocator;
mod frame_builder;
mod freelist;
mod glyph_cache;
mod gpu_cache;
mod gpu_types;
mod hit_test;
mod image_source;
mod image_tiling;
mod internal_types;
mod lru_cache;
mod pattern;
mod picture;
mod picture_graph;
mod picture_textures;
mod prepare;
mod prim_store;
mod print_tree;
mod quad;
mod rectangle_occlusion;
mod render_backend;
mod render_target;
mod render_task;
mod render_task_cache;
mod render_task_graph;
mod renderer;
mod resource_cache;
mod scene;
mod scene_builder_thread;
mod scene_building;
mod screen_capture;
mod segment;
mod space;
mod spatial_node;
mod spatial_tree;
mod surface;
mod texture_cache;
mod texture_pack;
mod tile_cache;
mod util;
mod visibility;

///
pub mod intern;
///
pub mod render_api;

pub mod shader_source {
    include!(concat!(env!("OUT_DIR"), "/shaders.rs"));
}

extern crate bincode;
extern crate byteorder;
pub extern crate euclid;
extern crate fxhash;
extern crate num_traits;
extern crate plane_split;
extern crate rayon;
#[cfg(feature = "ron")]
extern crate ron;
#[macro_use]
extern crate smallvec;
#[cfg(all(feature = "capture", feature = "png"))]
extern crate png;
#[cfg(test)]
extern crate rand;
extern crate time;

pub extern crate api;
extern crate webrender_build;

pub use api as webrender_api;
pub use glyph_rasterizer;
pub use webrender_build::shader::{ProgramSourceDigest, ShaderKind};

#[doc(hidden)]
pub use crate::composite::{
    Compositor, CompositorCapabilities, CompositorConfig, CompositorSurfaceTransform,
};
#[cfg(feature = "sw_compositor")]
pub use crate::compositor::sw_compositor;
pub use crate::{
    composite::{
        MappableCompositor, MappedTileInfo, NativeSurfaceId, NativeSurfaceInfo, NativeTileId,
        PartialPresentCompositor, SWGLCompositeSurfaceInfo, WindowVisibility,
    },
    device::{
        get_gl_target, get_unoptimized_shader_source, Device, FormatDesc, ProgramBinary,
        ProgramCache, ProgramCacheObserver, UploadMethod, VertexUsageHint,
    },
    hit_test::SharedHitTester,
    intern::ItemUid,
    internal_types::FastHashMap,
    picture::{
        CompareHelperResult, InvalidationReason, PrimitiveCompareResult, TileDescriptor, TileId,
        TileNode, TileNodeKind, TileOffset,
    },
    profiler::{set_profiler_hooks, ProfilerHooks},
    render_api::*,
    renderer::{
        init::{
            create_webrender_instance, AsyncPropertySampler, RenderBackendHooks, SceneBuilderHooks,
            WebRenderOptions, ONE_TIME_USAGE_HINT,
        },
        CpuProfile, DebugFlags, GpuProfile, GraphicsApi, GraphicsApiInfo, PipelineInfo,
        RenderResults, Renderer, RendererError, RendererStats, ShaderPrecacheFlags, Shaders,
        SharedShaders, MAX_VERTEX_TEXTURE_WIDTH,
    },
    screen_capture::{AsyncScreenshotHandle, RecordedFrameHandle},
    texture_cache::TextureCacheConfig,
    tile_cache::{DirtyTileDebugInfo, PictureCacheDebugInfo, SliceDebugInfo, TileDebugInfo},
};
