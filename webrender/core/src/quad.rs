/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

use api::{units::*, ClipMode, ColorF};
use euclid::point2;

use crate::batch::{BatchKey, BatchKind, BatchTextures};
use crate::clip::{ClipChainInstance, ClipIntern, ClipItemKind, ClipNodeRange, ClipSpaceConversion, ClipStore};
use crate::command_buffer::{CommandBufferIndex, PrimitiveCommand, QuadFlags};
use crate::frame_builder::{FrameBuildingContext, FrameBuildingState, PictureContext, PictureState};
use crate::gpu_types::{PrimitiveInstanceData, QuadInstance, QuadSegment, TransformPaletteId, ZBufferId};
use crate::intern::DataStore;
use crate::internal_types::TextureSource;
use crate::pattern::{Pattern, PatternBuilder, PatternBuilderContext, PatternBuilderState, PatternKind, PatternShaderInput};
use crate::prim_store::{PrimitiveInstanceIndex, PrimitiveScratchBuffer};
use crate::render_task::{MaskSubPass, RenderTask, RenderTaskAddress, RenderTaskKind, SubPass};
use crate::render_task_graph::{RenderTaskGraph, RenderTaskGraphBuilder, RenderTaskId};
use crate::renderer::{BlendMode, GpuBufferAddress, GpuBufferBuilder, GpuBufferBuilderF};
use crate::segment::EdgeAaSegmentMask;
use crate::space::SpaceMapper;
use crate::spatial_tree::{SpatialNodeIndex, SpatialTree};
use crate::surface::SurfaceBuilder;
use crate::util::{extract_inner_rect_k, MaxRect, ScaleOffset};

const MIN_AA_SEGMENTS_SIZE: f32 = 4.0;
const MIN_QUAD_SPLIT_SIZE: f32 = 256.0;
const MAX_TILES_PER_QUAD: usize = 4;

/// Describes how clipping affects the rendering of a quad primitive.
///
/// As a general rule, parts of the quad that require masking are prerendered in an
/// intermediate target and the mask is applied using multiplicative blending to
/// the intermediate result before compositing it into the destination target.
///
/// Each segment can opt in or out of masking independently.
#[derive(Debug, Copy, Clone)]
pub enum QuadRenderStrategy {
    /// The quad is not affected by any mask and is drawn directly in the destination
    /// target.
    Direct,
    /// The quad is drawn entirely in an intermediate target and a mask is applied
    /// before compositing in the destination target.
    Indirect,
    /// A rounded rectangle clip is applied to the quad primitive via a nine-patch.
    /// The segments of the nine-patch that require a mask are rendered and masked in
    /// an intermediate target, while other segments are drawn directly in the destination
    /// target.
    NinePatch {
        radius: LayoutVector2D,
        clip_rect: LayoutRect,
    },
    /// Split the primitive into coarse tiles so that each tile independently
    /// has the opportunity to be drawn directly in the destination target or
    /// via an intermediate target if it is affected by a mask.
    Tiled {
        x_tiles: u16,
        y_tiles: u16,
    }
}

pub fn prepare_quad(
    pattern_builder: &dyn PatternBuilder,
    local_rect: &LayoutRect,
    prim_instance_index: PrimitiveInstanceIndex,
    prim_spatial_node_index: SpatialNodeIndex,
    clip_chain: &ClipChainInstance,
    device_pixel_scale: DevicePixelScale,

    frame_context: &FrameBuildingContext,
    pic_context: &PictureContext,
    targets: &[CommandBufferIndex],
    interned_clips: &DataStore<ClipIntern>,

    frame_state: &mut FrameBuildingState,
    pic_state: &mut PictureState,
    scratch: &mut PrimitiveScratchBuffer,
) {
    let map_prim_to_raster = frame_context.spatial_tree.get_relative_transform(
        prim_spatial_node_index,
        pic_context.raster_spatial_node_index,
    );

    let ctx = PatternBuilderContext {
        scene_properties: frame_context.scene_properties,
        spatial_tree: frame_context.spatial_tree,
    };

    let mut state = PatternBuilderState {
        frame_gpu_data: frame_state.frame_gpu_data,
        rg_builder: frame_state.rg_builder,
        clip_store: frame_state.clip_store,
    };

    let shared_pattern = if pattern_builder.use_shared_pattern() {
        Some(pattern_builder.build(
            None,
            &ctx,
            &mut state,
        ))
    } else {
        None
    };

    let prim_is_2d_scale_translation = map_prim_to_raster.is_2d_scale_translation();
    let prim_is_2d_axis_aligned = map_prim_to_raster.is_2d_axis_aligned();

    // TODO(gw): Can't support 9-patch for box-shadows for now as should_create_task
    //           assumes pattern is solid. This is a temporary hack until as once that's
    //           fixed we can select 9-patch for box-shadows
    let can_use_nine_patch = prim_is_2d_scale_translation && pattern_builder.can_use_nine_patch();

    let strategy = get_prim_render_strategy(
        prim_spatial_node_index,
        clip_chain,
        state.clip_store,
        interned_clips,
        can_use_nine_patch,
        frame_context.spatial_tree,
    );

    let mut quad_flags = QuadFlags::empty();

    // Only use AA edge instances if the primitive is large enough to require it
    let prim_size = local_rect.size();
    if prim_size.width > MIN_AA_SEGMENTS_SIZE && prim_size.height > MIN_AA_SEGMENTS_SIZE {
        quad_flags |= QuadFlags::USE_AA_SEGMENTS;
    }

    let needs_scissor = !prim_is_2d_scale_translation;
    if !needs_scissor {
        quad_flags |= QuadFlags::APPLY_RENDER_TASK_CLIP;
    }

    // TODO(gw): For now, we don't select per-edge AA at all if the primitive
    //           has a 2d transform, which matches existing behavior. However,
    //           as a follow up, we can now easily check if we have a 2d-aligned
    //           primitive on a subpixel boundary, and enable AA along those edge(s).
    let aa_flags = if prim_is_2d_axis_aligned {
        EdgeAaSegmentMask::empty()
    } else {
        EdgeAaSegmentMask::all()
    };

    let transform_id = frame_state.transforms.get_id(
        prim_spatial_node_index,
        pic_context.raster_spatial_node_index,
        frame_context.spatial_tree,
    );

    if let QuadRenderStrategy::Direct = strategy {
        let pattern = shared_pattern.unwrap_or_else(|| {
            pattern_builder.build(
                None,
                &ctx,
                &mut state,
            )
        });

        if pattern.is_opaque {
            quad_flags |= QuadFlags::IS_OPAQUE;
        }

        let main_prim_address = write_prim_blocks(
            &mut frame_state.frame_gpu_data.f32,
            *local_rect,
            clip_chain.local_clip_rect,
            pattern.base_color,
            pattern.texture_input.task_id,
            &[],
            ScaleOffset::identity(),
        );

        // Render the primitive as a single instance. Coordinates are provided to the
        // shader in layout space.
        frame_state.push_prim(
            &PrimitiveCommand::quad(
                pattern.kind,
                pattern.shader_input,
                pattern.texture_input.task_id,
                prim_instance_index,
                main_prim_address,
                transform_id,
                quad_flags,
                aa_flags,
            ),
            prim_spatial_node_index,
            targets,
        );

        // If the pattern samples from a texture, add it as a dependency
        // of the surface we're drawing directly on to.
        if pattern.texture_input.task_id != RenderTaskId::INVALID {
            frame_state
                .surface_builder
                .add_child_render_task(pattern.texture_input.task_id, frame_state.rg_builder);
        }

        return;
    }

    let surface = &mut frame_state.surfaces[pic_context.surface_index.0];
    let Some(clipped_surface_rect) = surface.get_surface_rect(
        &clip_chain.pic_coverage_rect, frame_context.spatial_tree
    ) else {
        return;
    };

    match strategy {
        QuadRenderStrategy::Direct => {}
        QuadRenderStrategy::Indirect => {
            let pattern = shared_pattern.unwrap_or_else(|| {
                pattern_builder.build(
                    None,
                    &ctx,
                    &mut state,
                )
            });

            if pattern.is_opaque {
                quad_flags |= QuadFlags::IS_OPAQUE;
            }

            let main_prim_address = write_prim_blocks(
                &mut frame_state.frame_gpu_data.f32,
                *local_rect,
                clip_chain.local_clip_rect,
                pattern.base_color,
                pattern.texture_input.task_id,
                &[],
                ScaleOffset::identity(),
            );

            // Render the primtive as a single instance in a render task, apply a mask
            // and composite it in the current picture.
            // The coordinates are provided to the shaders:
            //  - in layout space for the render task,
            //  - in device space for the instance that draw into the destination picture.
            let task_id = add_render_task_with_mask(
                &pattern,
                clipped_surface_rect.size(),
                clipped_surface_rect.min.to_f32(),
                clip_chain.clips_range,
                prim_spatial_node_index,
                pic_context.raster_spatial_node_index,
                main_prim_address,
                transform_id,
                aa_flags,
                quad_flags,
                device_pixel_scale,
                needs_scissor,
                frame_state.rg_builder,
                &mut frame_state.surface_builder,
            );

            let rect = clipped_surface_rect.to_f32().cast_unit();
            add_composite_prim(
                pattern_builder.get_base_color(&ctx),
                prim_instance_index,
                rect,
                frame_state,
                targets,
                &[QuadSegment { rect, task_id }],
            );
        }
        QuadRenderStrategy::Tiled { x_tiles, y_tiles } => {
            // Render the primtive as a grid of tiles decomposed in device space.
            // Tiles that need it are drawn in a render task and then composited into the
            // destination picture.
            // The coordinates are provided to the shaders:
            //  - in layout space for the render task,
            //  - in device space for the instances that draw into the destination picture.
            let clip_coverage_rect = surface
                .map_to_device_rect(&clip_chain.pic_coverage_rect, frame_context.spatial_tree);
            let clipped_surface_rect = clipped_surface_rect.to_f32();

            surface.map_local_to_picture.set_target_spatial_node(
                prim_spatial_node_index,
                frame_context.spatial_tree,
            );

            let Some(pic_rect) = surface.map_local_to_picture.map(local_rect) else { return };

            let unclipped_surface_rect = surface.map_to_device_rect(
                &pic_rect, frame_context.spatial_tree
            ).round_out();

            // Set up the tile classifier for the params of this quad
            scratch.quad_tile_classifier.reset(
                x_tiles as usize,
                y_tiles as usize,
                *local_rect,
            );

            // Walk each clip, extract the local mask regions and add them to the tile classifier.
            for i in 0 .. clip_chain.clips_range.count {
                let clip_instance = state.clip_store.get_instance_from_range(&clip_chain.clips_range, i);
                let clip_node = &interned_clips[clip_instance.handle];

                // Construct a prim <-> clip space converter
                let conversion = ClipSpaceConversion::new(
                    prim_spatial_node_index,
                    clip_node.item.spatial_node_index,
                    frame_context.spatial_tree,
                );

                // For now, we only handle axis-aligned mappings
                let transform = match conversion {
                    ClipSpaceConversion::Local => ScaleOffset::identity(),
                    ClipSpaceConversion::ScaleOffset(scale_offset) => scale_offset,
                    ClipSpaceConversion::Transform(..) => {
                        // If the clip transform is not axis-aligned, just assume the entire primitive
                        // local rect is affected by the clip, for now. It's no worse than what
                        // we were doing previously for all tiles.
                        scratch.quad_tile_classifier.add_mask_region(*local_rect);
                        continue;
                    }
                };

                // Add regions to the classifier depending on the clip kind
                match clip_node.item.kind {
                    ClipItemKind::Rectangle { mode, ref rect } => {
                        let rect = transform.map_rect(rect);
                        scratch.quad_tile_classifier.add_clip_rect(rect, mode);
                    }
                    ClipItemKind::RoundedRectangle { mode: ClipMode::Clip, ref rect, ref radius } => {
                        // For rounded-rects with Clip mode, we need a mask for each corner,
                        // and to add the clip rect itself (to cull tiles outside that rect)

                        // Map the local rect and radii
                        let rect = transform.map_rect(rect);
                        let r_tl = transform.map_size(&radius.top_left);
                        let r_tr = transform.map_size(&radius.top_right);
                        let r_br = transform.map_size(&radius.bottom_right);
                        let r_bl = transform.map_size(&radius.bottom_left);

                        // Construct the mask regions for each corner
                        let c_tl = LayoutRect::from_origin_and_size(
                            LayoutPoint::new(rect.min.x, rect.min.y),
                            r_tl,
                        );
                        let c_tr = LayoutRect::from_origin_and_size(
                            LayoutPoint::new(
                                rect.max.x - r_tr.width,
                                rect.min.y,
                            ),
                            r_tr,
                        );
                        let c_br = LayoutRect::from_origin_and_size(
                            LayoutPoint::new(
                                rect.max.x - r_br.width,
                                rect.max.y - r_br.height,
                            ),
                            r_br,
                        );
                        let c_bl = LayoutRect::from_origin_and_size(
                            LayoutPoint::new(
                                rect.min.x,
                                rect.max.y - r_bl.height,
                            ),
                            r_bl,
                        );

                        scratch.quad_tile_classifier.add_clip_rect(rect, ClipMode::Clip);
                        scratch.quad_tile_classifier.add_mask_region(c_tl);
                        scratch.quad_tile_classifier.add_mask_region(c_tr);
                        scratch.quad_tile_classifier.add_mask_region(c_br);
                        scratch.quad_tile_classifier.add_mask_region(c_bl);
                    }
                    ClipItemKind::RoundedRectangle { mode: ClipMode::ClipOut, ref rect, ref radius } => {
                        // Try to find an inner rect within the clip-out rounded rect that we can
                        // use to cull inner tiles. If we can't, the entire rect needs to be masked
                        match extract_inner_rect_k(rect, radius, 0.5) {
                            Some(ref rect) => {
                                let rect = transform.map_rect(rect);
                                scratch.quad_tile_classifier.add_clip_rect(rect, ClipMode::ClipOut);
                            }
                            None => {
                                scratch.quad_tile_classifier.add_mask_region(*local_rect);
                            }
                        }
                    }
                    ClipItemKind::BoxShadow { .. } => {
                        panic!("bug: old box-shadow clips unexpected in this path");
                    }
                    ClipItemKind::Image { .. } => {
                        panic!("bug: image clips unexpected in this path");
                    }
                }
            }

            // Classify each tile within the quad to be Pattern / Mask / Clipped
            let tile_info = scratch.quad_tile_classifier.classify();
            scratch.quad_direct_segments.clear();
            scratch.quad_indirect_segments.clear();

            let mut x_coords = vec![unclipped_surface_rect.min.x];
            let mut y_coords = vec![unclipped_surface_rect.min.y];

            let dx = (unclipped_surface_rect.max.x - unclipped_surface_rect.min.x) as f32 / x_tiles as f32;
            let dy = (unclipped_surface_rect.max.y - unclipped_surface_rect.min.y) as f32 / y_tiles as f32;

            for x in 1 .. (x_tiles as i32) {
                x_coords.push((unclipped_surface_rect.min.x as f32 + x as f32 * dx).round());
            }
            for y in 1 .. (y_tiles as i32) {
                y_coords.push((unclipped_surface_rect.min.y as f32 + y as f32 * dy).round());
            }

            x_coords.push(unclipped_surface_rect.max.x);
            y_coords.push(unclipped_surface_rect.max.y);

            for y in 0 .. y_coords.len()-1 {
                let y0 = y_coords[y];
                let y1 = y_coords[y+1];

                if y1 <= y0 {
                    continue;
                }

                for x in 0 .. x_coords.len()-1 {
                    let x0 = x_coords[x];
                    let x1 = x_coords[x+1];

                    if x1 <= x0 {
                        continue;
                    }

                    // Check whether this tile requires a mask
                    let tile_info = &tile_info[y * x_tiles as usize + x];
                    let is_direct = match tile_info.kind {
                        QuadTileKind::Clipped => {
                            // This tile was entirely clipped, so we can skip drawing it
                            continue;
                        }
                        QuadTileKind::Pattern { has_mask } => {
                            prim_is_2d_scale_translation && !has_mask && shared_pattern.is_some()
                        }
                    };

                    let int_rect = DeviceRect {
                        min: point2(x0, y0),
                        max: point2(x1, y1),
                    };

                    let int_rect = match clipped_surface_rect.intersection(&int_rect) {
                        Some(rect) => rect,
                        None => continue,
                    };

                    let rect = int_rect.to_f32();

                    if is_direct {
                        scratch.quad_direct_segments.push(QuadSegment { rect: rect.cast_unit(), task_id: RenderTaskId::INVALID });
                    } else {
                        let pattern = match shared_pattern {
                            Some(ref shared_pattern) => shared_pattern.clone(),
                            None => {
                                pattern_builder.build(
                                    Some(rect),
                                    &ctx,
                                    &mut state,
                                )
                            }
                        };

                        if pattern.is_opaque {
                            quad_flags |= QuadFlags::IS_OPAQUE;
                        }

                        let main_prim_address = write_prim_blocks(
                            &mut state.frame_gpu_data.f32,
                            *local_rect,
                            clip_chain.local_clip_rect,
                            pattern.base_color,
                            pattern.texture_input.task_id,
                            &[],
                            ScaleOffset::identity(),
                        );

                        let task_id = add_render_task_with_mask(
                            &pattern,
                            int_rect.round().to_i32().size(),
                            rect.min,
                            clip_chain.clips_range,
                            prim_spatial_node_index,
                            pic_context.raster_spatial_node_index,
                            main_prim_address,
                            transform_id,
                            aa_flags,
                            quad_flags,
                            device_pixel_scale,
                            needs_scissor,
                            state.rg_builder,
                            &mut frame_state.surface_builder,
                        );

                        scratch.quad_indirect_segments.push(QuadSegment { rect: rect.cast_unit(), task_id });
                    }
                }
            }

            if !scratch.quad_direct_segments.is_empty() {
                let local_to_device = map_prim_to_raster.as_2d_scale_offset()
                    .expect("bug: nine-patch segments should be axis-aligned only")
                    .then_scale(device_pixel_scale.0);

                let device_prim_rect: DeviceRect = local_to_device.map_rect(&local_rect);

                let pattern = match shared_pattern {
                    Some(ref shared_pattern) => shared_pattern.clone(),
                    None => {
                        pattern_builder.build(
                            Some(device_prim_rect),
                            &ctx,
                            &mut state,
                        )
                    }
                };

                add_pattern_prim(
                    &pattern,
                    local_to_device.inverse(),
                    prim_instance_index,
                    device_prim_rect.cast_unit(),
                    clip_coverage_rect.cast_unit(),
                    pattern.is_opaque,
                    frame_state,
                    targets,
                    &scratch.quad_direct_segments,
                );
            }

            if !scratch.quad_indirect_segments.is_empty() {
                add_composite_prim(
                    pattern_builder.get_base_color(&ctx),
                    prim_instance_index,
                    clip_coverage_rect.cast_unit(),
                    frame_state,
                    targets,
                    &scratch.quad_indirect_segments,
                );
            }
        }
        QuadRenderStrategy::NinePatch { clip_rect, radius } => {
            // Render the primtive as a nine-patch decomposed in device space.
            // Nine-patch segments that need it are drawn in a render task and then composited into the
            // destination picture.
            // The coordinates are provided to the shaders:
            //  - in layout space for the render task,
            //  - in device space for the instances that draw into the destination picture.
            let clip_coverage_rect = surface
                .map_to_device_rect(&clip_chain.pic_coverage_rect, frame_context.spatial_tree);

            let local_to_device = map_prim_to_raster.as_2d_scale_offset()
                .expect("bug: nine-patch segments should be axis-aligned only")
                .then_scale(device_pixel_scale.0);

            let device_prim_rect: DeviceRect = local_to_device.map_rect(&local_rect);

            let local_corner_0 = LayoutRect::new(
                clip_rect.min,
                clip_rect.min + radius,
            );

            let local_corner_1 = LayoutRect::new(
                clip_rect.max - radius,
                clip_rect.max,
            );

            let pic_corner_0 = pic_state.map_local_to_pic.map(&local_corner_0).unwrap();
            let pic_corner_1 = pic_state.map_local_to_pic.map(&local_corner_1).unwrap();

            let surface_rect_0 = surface.map_to_device_rect(
                &pic_corner_0,
                frame_context.spatial_tree,
            ).round_out().to_i32();

            let surface_rect_1 = surface.map_to_device_rect(
                &pic_corner_1,
                frame_context.spatial_tree,
            ).round_out().to_i32();

            let p0 = surface_rect_0.min;
            let p1 = surface_rect_0.max;
            let p2 = surface_rect_1.min;
            let p3 = surface_rect_1.max;

            let mut x_coords = [p0.x, p1.x, p2.x, p3.x];
            let mut y_coords = [p0.y, p1.y, p2.y, p3.y];

            x_coords.sort_by(|a, b| a.partial_cmp(b).unwrap());
            y_coords.sort_by(|a, b| a.partial_cmp(b).unwrap());

            scratch.quad_direct_segments.clear();
            scratch.quad_indirect_segments.clear();

            // TODO: re-land clip-out mode.
            let mode = ClipMode::Clip;

            fn should_create_task(mode: ClipMode, x: usize, y: usize) -> bool {
                match mode {
                    // Only create render tasks for the corners.
                    ClipMode::Clip => x != 1 && y != 1,
                    // Create render tasks for all segments (the
                    // center will be skipped).
                    ClipMode::ClipOut => true,
                }
            }

            for y in 0 .. y_coords.len()-1 {
                let y0 = y_coords[y];
                let y1 = y_coords[y+1];

                if y1 <= y0 {
                    continue;
                }

                for x in 0 .. x_coords.len()-1 {
                    if mode == ClipMode::ClipOut && x == 1 && y == 1 {
                        continue;
                    }

                    let x0 = x_coords[x];
                    let x1 = x_coords[x+1];

                    if x1 <= x0 {
                        continue;
                    }

                    let rect = DeviceIntRect::new(point2(x0, y0), point2(x1, y1));

                    let device_rect = match rect.intersection(&clipped_surface_rect) {
                        Some(rect) => rect,
                        None => {
                            continue;
                        }
                    };

                    if should_create_task(mode, x, y) {
                        let pattern = shared_pattern
                            .as_ref()
                            .expect("bug: nine-patch expects shared pattern, for now");

                        if pattern.is_opaque {
                            quad_flags |= QuadFlags::IS_OPAQUE;
                        }

                        let main_prim_address = write_prim_blocks(
                            &mut state.frame_gpu_data.f32,
                            *local_rect,
                            clip_chain.local_clip_rect,
                            pattern.base_color,
                            pattern.texture_input.task_id,
                            &[],
                            ScaleOffset::identity(),
                        );

                        let task_id = add_render_task_with_mask(
                            &pattern,
                            device_rect.size(),
                            device_rect.min.to_f32(),
                            clip_chain.clips_range,
                            prim_spatial_node_index,
                            pic_context.raster_spatial_node_index,
                            main_prim_address,
                            transform_id,
                            aa_flags,
                            quad_flags,
                            device_pixel_scale,
                            false,
                            state.rg_builder,
                            &mut frame_state.surface_builder,
                        );
                        scratch.quad_indirect_segments.push(QuadSegment {
                            rect: device_rect.to_f32().cast_unit(),
                            task_id,
                        });
                    } else {
                        scratch.quad_direct_segments.push(QuadSegment {
                            rect: device_rect.to_f32().cast_unit(),
                            task_id: RenderTaskId::INVALID,
                        });
                    };
                }
            }

            if !scratch.quad_direct_segments.is_empty() {
                let pattern =  pattern_builder.build(
                    None,
                    &ctx,
                    &mut state,
                );

                add_pattern_prim(
                    &pattern,
                    local_to_device.inverse(),
                    prim_instance_index,
                    device_prim_rect.cast_unit(),
                    clip_coverage_rect.cast_unit(),
                    pattern.is_opaque,
                    frame_state,
                    targets,
                    &scratch.quad_direct_segments,
                );
            }

            if !scratch.quad_indirect_segments.is_empty() {
                add_composite_prim(
                    pattern_builder.get_base_color(&ctx),
                    prim_instance_index,
                    clip_coverage_rect.cast_unit(),
                    frame_state,
                    targets,
                    &scratch.quad_indirect_segments,
                );
            }
        }
    }
}

fn get_prim_render_strategy(
    prim_spatial_node_index: SpatialNodeIndex,
    clip_chain: &ClipChainInstance,
    clip_store: &ClipStore,
    interned_clips: &DataStore<ClipIntern>,
    can_use_nine_patch: bool,
    spatial_tree: &SpatialTree,
) -> QuadRenderStrategy {
    if !clip_chain.needs_mask {
        return QuadRenderStrategy::Direct
    }

    fn tile_count_for_size(size: f32) -> u16 {
        (size / MIN_QUAD_SPLIT_SIZE).min(MAX_TILES_PER_QUAD as f32).max(1.0).ceil() as u16
    }

    let prim_coverage_size = clip_chain.pic_coverage_rect.size();
    let x_tiles = tile_count_for_size(prim_coverage_size.width);
    let y_tiles = tile_count_for_size(prim_coverage_size.height);
    let try_split_prim = x_tiles > 1 || y_tiles > 1;

    if !try_split_prim {
        return QuadRenderStrategy::Indirect;
    }

    if can_use_nine_patch && clip_chain.clips_range.count == 1 {
        let clip_instance = clip_store.get_instance_from_range(&clip_chain.clips_range, 0);
        let clip_node = &interned_clips[clip_instance.handle];

        if let ClipItemKind::RoundedRectangle { ref radius, mode: ClipMode::Clip, rect, .. } = clip_node.item.kind {
            let max_corner_width = radius.top_left.width
                                        .max(radius.bottom_left.width)
                                        .max(radius.top_right.width)
                                        .max(radius.bottom_right.width);
            let max_corner_height = radius.top_left.height
                                        .max(radius.bottom_left.height)
                                        .max(radius.top_right.height)
                                        .max(radius.bottom_right.height);

            if max_corner_width <= 0.5 * rect.size().width &&
                max_corner_height <= 0.5 * rect.size().height {

                let clip_prim_coords_match = spatial_tree.is_matching_coord_system(
                    prim_spatial_node_index,
                    clip_node.item.spatial_node_index,
                );

                if clip_prim_coords_match {
                    let map_clip_to_prim = SpaceMapper::new_with_target(
                        prim_spatial_node_index,
                        clip_node.item.spatial_node_index,
                        LayoutRect::max_rect(),
                        spatial_tree,
                    );

                    if let Some(rect) = map_clip_to_prim.map(&rect) {
                        return QuadRenderStrategy::NinePatch {
                            radius: LayoutVector2D::new(max_corner_width, max_corner_height),
                            clip_rect: rect,
                        };
                    }
                }
            }
        }
    }

    QuadRenderStrategy::Tiled {
        x_tiles,
        y_tiles,
    }
}

fn add_render_task_with_mask(
    pattern: &Pattern,
    task_size: DeviceIntSize,
    content_origin: DevicePoint,
    clips_range: ClipNodeRange,
    prim_spatial_node_index: SpatialNodeIndex,
    raster_spatial_node_index: SpatialNodeIndex,
    prim_address_f: GpuBufferAddress,
    transform_id: TransformPaletteId,
    aa_flags: EdgeAaSegmentMask,
    quad_flags: QuadFlags,
    device_pixel_scale: DevicePixelScale,
    needs_scissor_rect: bool,
    rg_builder: &mut RenderTaskGraphBuilder,
    surface_builder: &mut SurfaceBuilder,
) -> RenderTaskId {
    let task_id = rg_builder.add().init(RenderTask::new_dynamic(
        task_size,
        RenderTaskKind::new_prim(
            pattern.kind,
            pattern.shader_input,
            raster_spatial_node_index,
            device_pixel_scale,
            content_origin,
            prim_address_f,
            transform_id,
            aa_flags,
            quad_flags,
            needs_scissor_rect,
            pattern.texture_input.task_id,
        ),
    ));

    // If the pattern samples from a texture, add it as a dependency
    // of the indirect render task that relies on it.
    if pattern.texture_input.task_id != RenderTaskId::INVALID {
        rg_builder.add_dependency(task_id, pattern.texture_input.task_id);
    }

    if clips_range.count > 0 {
        let masks = MaskSubPass {
            clip_node_range: clips_range,
            prim_spatial_node_index,
            prim_address_f,
        };

        let task = rg_builder.get_task_mut(task_id);
        task.add_sub_pass(SubPass::Masks { masks });
    }

    surface_builder.add_child_render_task(task_id, rg_builder);

    task_id
}

fn add_pattern_prim(
    pattern: &Pattern,
    pattern_transform: ScaleOffset,
    prim_instance_index: PrimitiveInstanceIndex,
    rect: LayoutRect,
    clip_rect: LayoutRect,
    is_opaque: bool,
    frame_state: &mut FrameBuildingState,
    targets: &[CommandBufferIndex],
    segments: &[QuadSegment],
) {
    let prim_address = write_prim_blocks(
        &mut frame_state.frame_gpu_data.f32,
        rect,
        clip_rect,
        pattern.base_color,
        pattern.texture_input.task_id,
        segments,
        pattern_transform,
    );

    frame_state.set_segments(segments, targets);

    let mut quad_flags = QuadFlags::IGNORE_DEVICE_PIXEL_SCALE
        | QuadFlags::APPLY_RENDER_TASK_CLIP;

    if is_opaque {
        quad_flags |= QuadFlags::IS_OPAQUE;
    }

    frame_state.push_cmd(
        &PrimitiveCommand::quad(
            pattern.kind,
            pattern.shader_input,
            pattern.texture_input.task_id,
            prim_instance_index,
            prim_address,
            TransformPaletteId::IDENTITY,
            quad_flags,
            // TODO(gw): No AA on composite, unless we use it to apply 2d clips
            EdgeAaSegmentMask::empty(),
        ),
        targets,
    );
}

fn add_composite_prim(
    base_color: ColorF,
    prim_instance_index: PrimitiveInstanceIndex,
    rect: LayoutRect,
    frame_state: &mut FrameBuildingState,
    targets: &[CommandBufferIndex],
    segments: &[QuadSegment],
) {
    assert!(!segments.is_empty());

    let composite_prim_address = write_prim_blocks(
        &mut frame_state.frame_gpu_data.f32,
        rect,
        rect,
        // TODO: The base color for composite prim should be opaque white
        // (or white with some transparency to support an opacity directly
        // in the quad primitive). However, passing opaque white
        // here causes glitches with Adreno GPUs on Windows specifically
        // (See bug 1897444).
        base_color,
        RenderTaskId::INVALID,
        segments,
        ScaleOffset::identity(),
    );

    frame_state.set_segments(segments, targets);

    let quad_flags = QuadFlags::IGNORE_DEVICE_PIXEL_SCALE
        | QuadFlags::APPLY_RENDER_TASK_CLIP;

    frame_state.push_cmd(
        &PrimitiveCommand::quad(
            PatternKind::ColorOrTexture,
            PatternShaderInput::default(),
            RenderTaskId::INVALID,
            prim_instance_index,
            composite_prim_address,
            TransformPaletteId::IDENTITY,
            quad_flags,
            // TODO(gw): No AA on composite, unless we use it to apply 2d clips
            EdgeAaSegmentMask::empty(),
        ),
        targets,
    );
}

pub fn write_prim_blocks(
    builder: &mut GpuBufferBuilderF,
    prim_rect: LayoutRect,
    clip_rect: LayoutRect,
    pattern_base_color: ColorF,
    pattern_texture_input: RenderTaskId,
    segments: &[QuadSegment],
    scale_offset: ScaleOffset,
) -> GpuBufferAddress {
    let mut writer = builder.write_blocks(5 + segments.len() * 2);

    writer.push_one(prim_rect);
    writer.push_one(clip_rect);
    writer.push_render_task(pattern_texture_input);
    writer.push_one(scale_offset);
    writer.push_one(pattern_base_color.premultiplied());

    for segment in segments {
        writer.push_one(segment.rect);
        writer.push_render_task(segment.task_id)
    }

    writer.finish()
}

pub fn add_to_batch<F>(
    kind: PatternKind,
    pattern_input: PatternShaderInput,
    dst_task_address: RenderTaskAddress,
    transform_id: TransformPaletteId,
    prim_address_f: GpuBufferAddress,
    quad_flags: QuadFlags,
    edge_flags: EdgeAaSegmentMask,
    segment_index: u8,
    src_task_id: RenderTaskId,
    z_id: ZBufferId,
    render_tasks: &RenderTaskGraph,
    gpu_buffer_builder: &mut GpuBufferBuilder,
    mut f: F,
) where F: FnMut(BatchKey, PrimitiveInstanceData) {

    // See the corresponfing #defines in ps_quad.glsl
    #[repr(u8)]
    enum PartIndex {
        Center = 0,
        Left = 1,
        Top = 2,
        Right = 3,
        Bottom = 4,
        All = 5,
    }

    // See QuadHeader in ps_quad.glsl
    let mut writer = gpu_buffer_builder.i32.write_blocks(1);
    writer.push_one([
        transform_id.0 as i32,
        z_id.0,
        pattern_input.0,
        pattern_input.1,
    ]);
    let prim_address_i = writer.finish();

    let texture = match src_task_id {
        RenderTaskId::INVALID => TextureSource::Invalid,
        _ => {
            let texture = render_tasks
                .resolve_texture(src_task_id)
                .expect("bug: valid task id must be resolvable");

            texture
        }
    };

    let textures = BatchTextures::prim_textured(
        texture,
        TextureSource::Invalid,
    );

    let default_blend_mode = if quad_flags.contains(QuadFlags::IS_OPAQUE) {
        BlendMode::None
    } else {
        BlendMode::PremultipliedAlpha
    };

    let edge_flags_bits = edge_flags.bits();

    let prim_batch_key = BatchKey {
        blend_mode: default_blend_mode,
        kind: BatchKind::Quad(kind),
        textures,
    };

    let aa_batch_key = BatchKey {
        blend_mode: BlendMode::PremultipliedAlpha,
        kind: BatchKind::Quad(kind),
        textures,
    };

    let mut instance = QuadInstance {
        dst_task_address,
        prim_address_i,
        prim_address_f,
        edge_flags: edge_flags_bits,
        quad_flags: quad_flags.bits(),
        part_index: PartIndex::All as u8,
        segment_index,
    };

    if edge_flags.is_empty() {
        // No antialisaing.
        f(prim_batch_key, instance.into());
    } else if quad_flags.contains(QuadFlags::USE_AA_SEGMENTS) {
        // Add instances for the antialisaing. This gives the center part
        // an opportunity to stay in the opaque pass.
        if edge_flags.contains(EdgeAaSegmentMask::LEFT) {
            let instance = QuadInstance {
                part_index: PartIndex::Left as u8,
                ..instance
            };
            f(aa_batch_key, instance.into());
        }
        if edge_flags.contains(EdgeAaSegmentMask::RIGHT) {
            let instance = QuadInstance {
                part_index: PartIndex::Top as u8,
                ..instance
            };
            f(aa_batch_key, instance.into());
        }
        if edge_flags.contains(EdgeAaSegmentMask::TOP) {
            let instance = QuadInstance {
                part_index: PartIndex::Right as u8,
                ..instance
            };
            f(aa_batch_key, instance.into());
        }
        if edge_flags.contains(EdgeAaSegmentMask::BOTTOM) {
            let instance = QuadInstance {
                part_index: PartIndex::Bottom as u8,
                ..instance
            };
            f(aa_batch_key, instance.into());
        }

        instance = QuadInstance {
            part_index: PartIndex::Center as u8,
            ..instance
        };

        f(prim_batch_key, instance.into());
    } else {
        // Render the anti-aliased quad with a single primitive.
        f(aa_batch_key, instance.into());
    }
}

/// Classification result for a tile within a quad
#[allow(dead_code)]
#[cfg_attr(feature = "capture", derive(Serialize))]
#[derive(Debug, Copy, Clone, PartialEq)]
pub enum QuadTileKind {
    // Clipped out - can be skipped
    Clipped,
    // Requires the pattern only, can draw directly
    Pattern {
        has_mask: bool,
    },
}

#[cfg_attr(feature = "capture", derive(Serialize))]
#[derive(Copy, Clone, Debug)]
pub struct QuadTileInfo {
    rect: LayoutRect,
    kind: QuadTileKind,
}

impl Default for QuadTileInfo {
    fn default() -> Self {
        QuadTileInfo {
            rect: LayoutRect::zero(),
            kind: QuadTileKind::Pattern { has_mask: false },
        }
    }
}

/// A helper struct for classifying a set of tiles within a quad depending on
/// what strategy they can be used to draw them.
#[cfg_attr(feature = "capture", derive(Serialize))]
pub struct QuadTileClassifier {
    buffer: [QuadTileInfo; MAX_TILES_PER_QUAD * MAX_TILES_PER_QUAD],
    mask_regions: Vec<LayoutRect>,
    clip_in_regions: Vec<LayoutRect>,
    clip_out_regions: Vec<LayoutRect>,
    rect: LayoutRect,
    x_tiles: usize,
    y_tiles: usize,
}

impl QuadTileClassifier {
    pub fn new() -> Self {
        QuadTileClassifier {
            buffer: [QuadTileInfo::default(); MAX_TILES_PER_QUAD * MAX_TILES_PER_QUAD],
            mask_regions: Vec::new(),
            clip_in_regions: Vec::new(),
            clip_out_regions: Vec::new(),
            rect: LayoutRect::zero(),
            x_tiles: 0,
            y_tiles: 0,
        }
    }

    pub fn reset(
        &mut self,
        x_tiles: usize,
        y_tiles: usize,
        rect: LayoutRect,
    ) {
        assert_eq!(self.x_tiles, 0);
        assert_eq!(self.y_tiles, 0);

        self.x_tiles = x_tiles;
        self.y_tiles = y_tiles;
        self.rect = rect;
        self.mask_regions.clear();
        self.clip_in_regions.clear();
        self.clip_out_regions.clear();

        // TODO(gw): Might be some f32 accuracy issues with how we construct these,
        //           should be more robust here...

        let tw = (rect.max.x - rect.min.x) / x_tiles as f32;
        let th = (rect.max.y - rect.min.y) / y_tiles as f32;

        for y in 0 .. y_tiles {
            for x in 0 .. x_tiles {
                let info = &mut self.buffer[y * x_tiles + x];

                let p0 = LayoutPoint::new(
                    rect.min.x + x as f32 * tw,
                    rect.min.y + y as f32 * th,
                );
                let p1 = LayoutPoint::new(
                    p0.x + tw,
                    p0.y + th,
                );

                info.rect = LayoutRect::new(p0, p1);
                info.kind = QuadTileKind::Pattern { has_mask: false };
            }
        }
    }

    /// Add an area that needs a clip mask / indirect area
    pub fn add_mask_region(
        &mut self,
        mask_region: LayoutRect,
    ) {
        self.mask_regions.push(mask_region);
    }

    // TODO(gw): Make use of this to skip tiles that are completely clipped out in a follow up!
    pub fn add_clip_rect(
        &mut self,
        clip_rect: LayoutRect,
        clip_mode: ClipMode,
    ) {
        match clip_mode {
            ClipMode::Clip => {
                self.clip_in_regions.push(clip_rect);
            }
            ClipMode::ClipOut => {
                self.clip_out_regions.push(clip_rect);

                self.add_mask_region(self.rect);
            }
        }
    }

    /// Classify all the tiles in to categories, based on the provided masks and clip regions
    pub fn classify(
        &mut self,
    ) -> &[QuadTileInfo] {
        assert_ne!(self.x_tiles, 0);
        assert_ne!(self.y_tiles, 0);

        let tile_count = self.x_tiles * self.y_tiles;
        let tiles = &mut self.buffer[0 .. tile_count];

        for info in tiles.iter_mut() {
            // If a clip region contains the entire tile, it's clipped
            for clip_region in &self.clip_in_regions {
                match info.kind {
                    QuadTileKind::Clipped => {},
                    QuadTileKind::Pattern { .. } => {
                        if !clip_region.intersects(&info.rect) {
                            info.kind = QuadTileKind::Clipped;
                        }
                    }
                }

            }

            // If a tile doesn't intersect with a clip-out region, it's clipped
            for clip_region in &self.clip_out_regions {
                match info.kind {
                    QuadTileKind::Clipped => {},
                    QuadTileKind::Pattern { .. } => {
                        if clip_region.contains_box(&info.rect) {
                            info.kind = QuadTileKind::Clipped;
                        }
                    }
                }
            }

            // If a tile intersects with a mask region, and isn't clipped, it needs a mask
            for mask_region in &self.mask_regions {
                match info.kind {
                    QuadTileKind::Clipped | QuadTileKind::Pattern { has_mask: true, .. } => {},
                    QuadTileKind::Pattern { ref mut has_mask, .. } => {
                        if mask_region.intersects(&info.rect) {
                            *has_mask = true;
                        }
                    }
                }
            }
        }

        self.x_tiles = 0;
        self.y_tiles = 0;

        tiles
    }
}

#[cfg(test)]
fn qc_new(xc: usize, yc: usize, x0: f32, y0: f32, w: f32, h: f32) -> QuadTileClassifier {
    let mut qc = QuadTileClassifier::new();

    qc.reset(
        xc,
        yc,
        LayoutRect::new(LayoutPoint::new(x0, y0), LayoutPoint::new(x0 + w, y0 + h),
    ));

    qc
}

#[cfg(test)]
fn qc_verify(mut qc: QuadTileClassifier, expected: &[QuadTileKind]) {
    let tiles = qc.classify();

    assert_eq!(tiles.len(), expected.len());

    for (tile, ex) in tiles.iter().zip(expected.iter()) {
        assert_eq!(tile.kind, *ex, "Failed for tile {:?}", tile.rect.to_rect());
    }
}

#[cfg(test)]
const P: QuadTileKind = QuadTileKind::Pattern { has_mask: false };

#[cfg(test)]
const C: QuadTileKind = QuadTileKind::Clipped;

#[cfg(test)]
const M: QuadTileKind = QuadTileKind::Pattern { has_mask: true };

#[test]
fn quad_classify_1() {
    let qc = qc_new(3, 3, 0.0, 0.0, 100.0, 100.0);
    qc_verify(qc, &[
        P, P, P,
        P, P, P,
        P, P, P,
    ]);
}

#[test]
fn quad_classify_2() {
    let mut qc = qc_new(3, 3, 0.0, 0.0, 100.0, 100.0);

    let rect = LayoutRect::new(LayoutPoint::new(0.0, 0.0), LayoutPoint::new(100.0, 100.0));
    qc.add_clip_rect(rect, ClipMode::Clip);

    qc_verify(qc, &[
        P, P, P,
        P, P, P,
        P, P, P,
    ]);
}

#[test]
fn quad_classify_3() {
    let mut qc = qc_new(3, 3, 0.0, 0.0, 100.0, 100.0);

    let rect = LayoutRect::new(LayoutPoint::new(40.0, 40.0), LayoutPoint::new(60.0, 60.0));
    qc.add_clip_rect(rect, ClipMode::Clip);

    qc_verify(qc, &[
        C, C, C,
        C, P, C,
        C, C, C,
    ]);
}

#[test]
fn quad_classify_4() {
    let mut qc = qc_new(3, 3, 0.0, 0.0, 100.0, 100.0);

    let rect = LayoutRect::new(LayoutPoint::new(30.0, 30.0), LayoutPoint::new(70.0, 70.0));
    qc.add_clip_rect(rect, ClipMode::Clip);

    qc_verify(qc, &[
        P, P, P,
        P, P, P,
        P, P, P,
    ]);
}

#[test]
fn quad_classify_5() {
    let mut qc = qc_new(3, 3, 0.0, 0.0, 100.0, 100.0);

    let rect = LayoutRect::new(LayoutPoint::new(30.0, 30.0), LayoutPoint::new(70.0, 70.0));
    qc.add_clip_rect(rect, ClipMode::ClipOut);

    qc_verify(qc, &[
        M, M, M,
        M, C, M,
        M, M, M,
    ]);
}

#[test]
fn quad_classify_6() {
    let mut qc = qc_new(3, 3, 0.0, 0.0, 100.0, 100.0);

    let rect = LayoutRect::new(LayoutPoint::new(40.0, 40.0), LayoutPoint::new(60.0, 60.0));
    qc.add_clip_rect(rect, ClipMode::ClipOut);

    qc_verify(qc, &[
        M, M, M,
        M, M, M,
        M, M, M,
    ]);
}

#[test]
fn quad_classify_7() {
    let mut qc = qc_new(3, 3, 0.0, 0.0, 100.0, 100.0);

    let rect = LayoutRect::new(LayoutPoint::new(20.0, 10.0), LayoutPoint::new(90.0, 80.0));
    qc.add_mask_region(rect);

    qc_verify(qc, &[
        M, M, M,
        M, M, M,
        M, M, M,
    ]);
}

#[test]
fn quad_classify_8() {
    let mut qc = qc_new(3, 3, 0.0, 0.0, 100.0, 100.0);

    let rect = LayoutRect::new(LayoutPoint::new(40.0, 40.0), LayoutPoint::new(60.0, 60.0));
    qc.add_mask_region(rect);

    qc_verify(qc, &[
        P, P, P,
        P, M, P,
        P, P, P,
    ]);
}

#[test]
fn quad_classify_9() {
    let mut qc = qc_new(4, 4, 100.0, 200.0, 100.0, 100.0);

    let rect = LayoutRect::new(LayoutPoint::new(90.0, 180.0), LayoutPoint::new(140.0, 240.0));
    qc.add_mask_region(rect);

    qc_verify(qc, &[
        M, M, P, P,
        M, M, P, P,
        P, P, P, P,
        P, P, P, P,
    ]);
}

#[test]
fn quad_classify_10() {
    let mut qc = qc_new(4, 4, 100.0, 200.0, 100.0, 100.0);

    let mask_rect = LayoutRect::new(LayoutPoint::new(90.0, 180.0), LayoutPoint::new(140.0, 240.0));
    qc.add_mask_region(mask_rect);

    let clip_rect = LayoutRect::new(LayoutPoint::new(120.0, 220.0), LayoutPoint::new(160.0, 280.0));
    qc.add_clip_rect(clip_rect, ClipMode::Clip);

    qc_verify(qc, &[
        M, M, P, C,
        M, M, P, C,
        P, P, P, C,
        P, P, P, C,
    ]);
}

#[test]
fn quad_classify_11() {
    let mut qc = qc_new(4, 4, 100.0, 200.0, 100.0, 100.0);

    let mask_rect = LayoutRect::new(LayoutPoint::new(90.0, 180.0), LayoutPoint::new(140.0, 240.0));
    qc.add_mask_region(mask_rect);

    let clip_rect = LayoutRect::new(LayoutPoint::new(120.0, 220.0), LayoutPoint::new(160.0, 280.0));
    qc.add_clip_rect(clip_rect, ClipMode::Clip);

    let clip_out_rect = LayoutRect::new(LayoutPoint::new(130.0, 200.0), LayoutPoint::new(160.0, 240.0));
    qc.add_clip_rect(clip_out_rect, ClipMode::ClipOut);

    qc_verify(qc, &[
        M, M, M, C,
        M, M, M, C,
        M, M, M, C,
        M, M, M, C,
    ]);
}

#[test]
fn quad_classify_12() {
    let mut qc = qc_new(4, 4, 100.0, 200.0, 100.0, 100.0);

    let clip_out_rect = LayoutRect::new(LayoutPoint::new(130.0, 200.0), LayoutPoint::new(160.0, 240.0));
    qc.add_clip_rect(clip_out_rect, ClipMode::ClipOut);

    let clip_rect = LayoutRect::new(LayoutPoint::new(120.0, 220.0), LayoutPoint::new(160.0, 280.0));
    qc.add_clip_rect(clip_rect, ClipMode::Clip);

    let mask_rect = LayoutRect::new(LayoutPoint::new(90.0, 180.0), LayoutPoint::new(140.0, 240.0));
    qc.add_mask_region(mask_rect);

    qc_verify(qc, &[
        M, M, M, C,
        M, M, M, C,
        M, M, M, C,
        M, M, M, C,
    ]);
}
