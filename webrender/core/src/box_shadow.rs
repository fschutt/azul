/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */
use api::{BorderRadius, BoxShadowClipMode, ClipMode, ColorF, ColorU, PrimitiveKeyKind, PropertyBinding};
use api::units::*;
use crate::border::{ensure_no_corner_overlap, BorderRadiusAu};
use crate::clip::{ClipDataHandle, ClipInternData, ClipItemKey, ClipItemKeyKind, ClipNodeId};
use crate::command_buffer::QuadFlags;
use crate::intern::{Handle as InternHandle, InternDebug, Internable};
use crate::pattern::{Pattern, PatternBuilder, PatternBuilderContext, PatternBuilderState};
use crate::picture::calculate_uv_rect_kind;
use crate::prim_store::{InternablePrimitive, PrimKey, PrimTemplate, PrimTemplateCommonData};
use crate::prim_store::{PrimitiveInstanceKind, PrimitiveStore, RectangleKey};
use crate::quad;
use crate::render_target::RenderTargetKind;
use crate::render_task::{BlurTask, MaskSubPass, PrimTask, RenderTask, RenderTaskKind, SubPass};
use crate::scene_building::{SceneBuilder, IsVisible};
use crate::segment::EdgeAaSegmentMask;
use crate::spatial_tree::SpatialNodeIndex;
use crate::gpu_types::{BoxShadowStretchMode, TransformPaletteId, UvRectKind};
use crate::render_task_graph::RenderTaskId;
use crate::internal_types::LayoutPrimitiveInfo;
use crate::util::{extract_inner_rect_k, ScaleOffset};

pub type BoxShadowKey = PrimKey<BoxShadow>;

impl BoxShadowKey {
    pub fn new(
        info: &LayoutPrimitiveInfo,
        shadow: BoxShadow,
    ) -> Self {
        BoxShadowKey {
            common: info.into(),
            kind: shadow,
        }
    }
}

impl InternDebug for BoxShadowKey {}

#[cfg_attr(feature = "capture", derive(Serialize))]
#[cfg_attr(feature = "replay", derive(Deserialize))]
#[derive(Debug, Clone, MallocSizeOf, Hash, Eq, PartialEq)]
pub struct BoxShadow {
    pub color: ColorU,
    pub blur_radius: Au,
    pub clip_mode: BoxShadowClipMode,
    pub inner_shadow_rect: RectangleKey,
    pub outer_shadow_rect: RectangleKey,
    pub shadow_radius: BorderRadiusAu,
    pub clip: ClipDataHandle,
}

impl IsVisible for BoxShadow {
    fn is_visible(&self) -> bool {
        true
    }
}

pub type BoxShadowDataHandle = InternHandle<BoxShadow>;

impl PatternBuilder for BoxShadowTemplate {
    fn build(
        &self,
        sub_rect: Option<DeviceRect>,
        ctx: &PatternBuilderContext,
        state: &mut PatternBuilderState,
    ) -> crate::pattern::Pattern {

        let raster_spatial_node_index = ctx.spatial_tree.root_reference_frame_index();
        let pattern_rect = self.kind.outer_shadow_rect;

        // TODO(gw): Correctly account for scaled blur radius inflation, and device
        //           pixel scale here.

        let (task_size, content_origin, scale_factor, uv_rect_kind) = match sub_rect {
            Some(rect) => {
                let expanded_rect = rect.inflate(32.0, 32.0);
                let uv_rect_kind = calculate_uv_rect_kind(expanded_rect, pattern_rect.cast_unit());

                (
                    expanded_rect.size().cast_unit().to_i32(),
                    expanded_rect.min.cast_unit(),
                    DevicePixelScale::new(1.0),
                    uv_rect_kind,
                )
            }
            None => {
                (
                    pattern_rect.size().cast_unit().to_i32(),
                    pattern_rect.min.cast_unit(),
                    DevicePixelScale::new(1.0),
                    UvRectKind::Rect,
                )
            }
        };

        let blur_radius = self.kind.blur_radius * scale_factor.0;
        let clips_range = state.clip_store.push_clip_instance(self.kind.clip);
        let color_pattern = Pattern::color(self.kind.color);

        let pattern_prim_address_f = quad::write_prim_blocks(
            &mut state.frame_gpu_data.f32,
            pattern_rect,
            pattern_rect,
            color_pattern.base_color,
            color_pattern.texture_input.task_id,
            &[],
            ScaleOffset::identity(),
        );

        let pattern_task_id = state.rg_builder.add().init(RenderTask::new_dynamic(
            task_size,
            RenderTaskKind::Prim(PrimTask {
                pattern: color_pattern.kind,
                pattern_input: color_pattern.shader_input,
                raster_spatial_node_index,
                device_pixel_scale: DevicePixelScale::new(1.0),
                content_origin,
                prim_address_f: pattern_prim_address_f,
                transform_id: TransformPaletteId::IDENTITY,
                edge_flags: EdgeAaSegmentMask::empty(),
                quad_flags: QuadFlags::APPLY_RENDER_TASK_CLIP | QuadFlags::IGNORE_DEVICE_PIXEL_SCALE,
                prim_needs_scissor_rect: false,
                texture_input: color_pattern.texture_input.task_id,
            }),
        ));

        let masks = MaskSubPass {
            clip_node_range: clips_range,
            prim_spatial_node_index: raster_spatial_node_index,
            prim_address_f: pattern_prim_address_f,
        };

        let task = state.rg_builder.get_task_mut(pattern_task_id);
        task.add_sub_pass(SubPass::Masks { masks });

        let blur_task_v = state.rg_builder.add().init(RenderTask::new_dynamic(
            task_size,
            RenderTaskKind::VerticalBlur(BlurTask {
                blur_std_deviation: blur_radius,
                target_kind: RenderTargetKind::Color,
                blur_region: task_size,
            }),
        ));
        state.rg_builder.add_dependency(blur_task_v, pattern_task_id);

        let blur_task_h = state.rg_builder.add().init(RenderTask::new_dynamic(
            task_size,
            RenderTaskKind::HorizontalBlur(BlurTask {
                blur_std_deviation: blur_radius,
                target_kind: RenderTargetKind::Color,
                blur_region: task_size,
            }),
        ).with_uv_rect_kind(uv_rect_kind));
        state.rg_builder.add_dependency(blur_task_h, blur_task_v);

        Pattern::texture(
            blur_task_h,
            self.kind.color,
        )
    }

    fn get_base_color(
        &self,
        _ctx: &PatternBuilderContext,
    ) -> ColorF {
        self.kind.color
    }

    fn use_shared_pattern(
        &self,
    ) -> bool {
        false
    }

    fn can_use_nine_patch(&self) -> bool {
        false
    }
}

impl InternablePrimitive for BoxShadow {
    fn into_key(
        self,
        info: &LayoutPrimitiveInfo,
    ) -> BoxShadowKey {
        BoxShadowKey::new(info, self)
    }

    fn make_instance_kind(
        _key: BoxShadowKey,
        data_handle: BoxShadowDataHandle,
        _prim_store: &mut PrimitiveStore,
    ) -> PrimitiveInstanceKind {
        PrimitiveInstanceKind::BoxShadow {
            data_handle,
        }
    }
}

#[cfg_attr(feature = "capture", derive(Serialize))]
#[cfg_attr(feature = "replay", derive(Deserialize))]
#[derive(Debug, MallocSizeOf)]
pub struct BoxShadowData {
    pub color: ColorF,
    pub blur_radius: f32,
    pub clip_mode: BoxShadowClipMode,
    pub inner_shadow_rect: LayoutRect,
    pub outer_shadow_rect: LayoutRect,
    pub shadow_radius: BorderRadius,
    pub clip: ClipDataHandle,
}

impl From<BoxShadow> for BoxShadowData {
    fn from(shadow: BoxShadow) -> Self {
        BoxShadowData {
            color: shadow.color.into(),
            blur_radius: shadow.blur_radius.to_f32_px(),
            clip_mode: shadow.clip_mode,
            inner_shadow_rect: shadow.inner_shadow_rect.into(),
            outer_shadow_rect: shadow.outer_shadow_rect.into(),
            shadow_radius: shadow.shadow_radius.into(),
            clip: shadow.clip,
        }
    }
}

pub type BoxShadowTemplate = PrimTemplate<BoxShadowData>;

impl Internable for BoxShadow {
    type Key = BoxShadowKey;
    type StoreData = BoxShadowTemplate;
    type InternData = ();
    const PROFILE_COUNTER: usize = crate::profiler::INTERNED_BOX_SHADOWS;
}

impl From<BoxShadowKey> for BoxShadowTemplate {
    fn from(shadow: BoxShadowKey) -> Self {
        BoxShadowTemplate {
            common: PrimTemplateCommonData::with_key_common(shadow.common),
            kind: shadow.kind.into(),
        }
    }
}

#[derive(Debug, Clone, MallocSizeOf)]
#[cfg_attr(feature = "capture", derive(Serialize))]
#[cfg_attr(feature = "replay", derive(Deserialize))]
pub struct BoxShadowClipSource {
    // Parameters that define the shadow and are constant.
    pub shadow_radius: BorderRadius,
    pub blur_radius: f32,
    pub clip_mode: BoxShadowClipMode,
    pub stretch_mode_x: BoxShadowStretchMode,
    pub stretch_mode_y: BoxShadowStretchMode,

    // The current cache key (in device-pixels), and handles
    // to the cached clip region and blurred texture.
    pub cache_key: Option<(DeviceIntSize, BoxShadowCacheKey)>,
    pub render_task: Option<RenderTaskId>,

    // Local-space size of the required render task size.
    pub shadow_rect_alloc_size: LayoutSize,

    // Local-space size of the required render task size without any downscaling
    // applied. This is needed to stretch the shadow properly.
    pub original_alloc_size: LayoutSize,

    // The minimal shadow rect for the parameters above,
    // used when drawing the shadow rect to be blurred.
    pub minimal_shadow_rect: LayoutRect,

    // Local space rect for the shadow to be drawn or
    // stretched in the shadow primitive.
    pub prim_shadow_rect: LayoutRect,
}

// The blur shader samples BLUR_SAMPLE_SCALE * blur_radius surrounding texels.
pub const BLUR_SAMPLE_SCALE: f32 = 3.0;

// Maximum blur radius for box-shadows (different than blur filters).
// Taken from nsCSSRendering.cpp in Gecko.
pub const MAX_BLUR_RADIUS: f32 = 300.;

// A cache key that uniquely identifies a minimally sized
// and blurred box-shadow rect that can be stored in the
// texture cache and applied to clip-masks.
#[derive(Debug, Clone, Eq, Hash, MallocSizeOf, PartialEq)]
#[cfg_attr(feature = "capture", derive(Serialize))]
#[cfg_attr(feature = "replay", derive(Deserialize))]
pub struct BoxShadowCacheKey {
    pub blur_radius_dp: i32,
    pub clip_mode: BoxShadowClipMode,
    // NOTE(emilio): Only the original allocation size needs to be in the cache
    // key, since the actual size is derived from that.
    pub original_alloc_size: DeviceIntSize,
    pub br_top_left: DeviceIntSize,
    pub br_top_right: DeviceIntSize,
    pub br_bottom_right: DeviceIntSize,
    pub br_bottom_left: DeviceIntSize,
    pub device_pixel_scale: Au,
}

impl<'a> SceneBuilder<'a> {
    pub fn add_box_shadow(
        &mut self,
        spatial_node_index: SpatialNodeIndex,
        clip_node_id: ClipNodeId,
        prim_info: &LayoutPrimitiveInfo,
        box_offset: &LayoutVector2D,
        color: ColorF,
        mut blur_radius: f32,
        spread_radius: f32,
        border_radius: BorderRadius,
        clip_mode: BoxShadowClipMode,
        is_root_coord_system: bool,
    ) {
        if color.a == 0.0 {
            return;
        }

        // Inset shadows get smaller as spread radius increases.
        let (spread_amount, prim_clip_mode) = match clip_mode {
            BoxShadowClipMode::Outset => (spread_radius, ClipMode::ClipOut),
            BoxShadowClipMode::Inset => (-spread_radius, ClipMode::Clip),
        };

        // Ensure the blur radius is somewhat sensible.
        blur_radius = f32::min(blur_radius, MAX_BLUR_RADIUS);

        // Adjust the border radius of the box shadow per CSS-spec.
        let mut shadow_radius = adjust_border_radius_for_box_shadow(border_radius, spread_amount);

        // Apply parameters that affect where the shadow rect
        // exists in the local space of the primitive.
        let shadow_rect = prim_info
            .rect
            .translate(*box_offset)
            .inflate(spread_amount, spread_amount);

        // If blur radius is zero, we can use a fast path with
        // no blur applied.
        if blur_radius == 0.0 {
            // Trivial reject of box-shadows that are not visible.
            if box_offset.x == 0.0 && box_offset.y == 0.0 && spread_amount == 0.0 {
                return;
            }

            let mut clips = Vec::with_capacity(2);
            let (final_prim_rect, clip_radius) = match clip_mode {
                BoxShadowClipMode::Outset => {
                    if shadow_rect.is_empty() {
                        return;
                    }

                    // TODO(gw): Add a fast path for ClipOut + zero border radius!
                    clips.push(ClipItemKey {
                        kind: ClipItemKeyKind::rounded_rect(
                            prim_info.rect,
                            border_radius,
                            ClipMode::ClipOut,
                        ),
                        spatial_node_index,
                    });

                    (shadow_rect, shadow_radius)
                }
                BoxShadowClipMode::Inset => {
                    if !shadow_rect.is_empty() {
                        clips.push(ClipItemKey {
                            kind: ClipItemKeyKind::rounded_rect(
                                shadow_rect,
                                shadow_radius,
                                ClipMode::ClipOut,
                            ),
                            spatial_node_index,
                        });
                    }

                    (prim_info.rect, border_radius)
                }
            };

            clips.push(ClipItemKey {
                kind: ClipItemKeyKind::rounded_rect(
                    final_prim_rect,
                    clip_radius,
                    ClipMode::Clip,
                ),
                spatial_node_index,
            });

            self.add_primitive(
                spatial_node_index,
                clip_node_id,
                &LayoutPrimitiveInfo::with_clip_rect(final_prim_rect, prim_info.clip_rect),
                clips,
                PrimitiveKeyKind::Rectangle {
                    color: PropertyBinding::Value(color.into()),
                },
            );
        } else {
            // If we know that this is an axis-aligned (root-coord) outset box-shadow,
            // enable the new quad based render path. Complex transformed and inset
            // box-shadow support will be added to this path as a follow up.
            if is_root_coord_system &&
               clip_mode == BoxShadowClipMode::Outset &&
               blur_radius < 32.0 &&
               false {
                // Make sure corners don't overlap.
                ensure_no_corner_overlap(&mut shadow_radius, shadow_rect.size());

                // Create clip that gets applied to the primitive
                let prim_clip = ClipItemKey {
                    kind: ClipItemKeyKind::rounded_rect(
                        prim_info.rect,
                        border_radius,
                        ClipMode::ClipOut,
                    ),
                    spatial_node_index,
                };

                // Per https://drafts.csswg.org/css-backgrounds/#shadow-blur
                let blur_radius = (blur_radius * 0.5).round();
                // Range over which blue radius affects pixels (~99% within 3 * sigma)
                let sig3 = blur_radius * 3.0;

                // Clip for the pattern primitive
                let item = ClipItemKey {
                    kind: ClipItemKeyKind::rounded_rect(
                        shadow_rect,
                        shadow_radius,
                        ClipMode::Clip,
                    ),
                    spatial_node_index: self.spatial_tree.root_reference_frame_index(),
                };

                let clip = self
                    .interners
                    .clip
                    .intern(&item, || {
                        ClipInternData {
                            key: item,
                        }
                    });

                let inner_shadow_rect = shadow_rect.inflate(-sig3, -sig3);
                let outer_shadow_rect = shadow_rect.inflate( sig3,  sig3);
                let inner_shadow_rect = extract_inner_rect_k(&inner_shadow_rect, &shadow_radius, 0.5).unwrap_or(LayoutRect::zero());

                let prim = BoxShadow {
                    color: color.into(),
                    blur_radius: Au::from_f32_px(blur_radius),
                    clip_mode,

                    inner_shadow_rect: inner_shadow_rect.into(),
                    outer_shadow_rect: outer_shadow_rect.into(),
                    shadow_radius: shadow_radius.into(),
                    clip,
                };

                // Out rect is the shadow rect + extent of blur
                let prim_info = LayoutPrimitiveInfo::with_clip_rect(
                    outer_shadow_rect,
                    prim_info.clip_rect,
                );

                self.add_nonshadowable_primitive(
                    spatial_node_index,
                    clip_node_id,
                    &prim_info,
                    vec![prim_clip],
                    prim,
                );
            } else {
                // Normal path for box-shadows with a valid blur radius.
                let blur_offset = (BLUR_SAMPLE_SCALE * blur_radius).ceil();
                let mut extra_clips = vec![];

                // Add a normal clip mask to clip out the contents
                // of the surrounding primitive.
                extra_clips.push(ClipItemKey {
                    kind: ClipItemKeyKind::rounded_rect(
                        prim_info.rect,
                        border_radius,
                        prim_clip_mode,
                    ),
                    spatial_node_index,
                });

                // Get the local rect of where the shadow will be drawn,
                // expanded to include room for the blurred region.
                let dest_rect = shadow_rect.inflate(blur_offset, blur_offset);

                // Draw the box-shadow as a solid rect, using a box-shadow
                // clip mask item.
                let prim = PrimitiveKeyKind::Rectangle {
                    color: PropertyBinding::Value(color.into()),
                };

                // Create the box-shadow clip item.
                let shadow_clip_source = ClipItemKey {
                    kind: ClipItemKeyKind::box_shadow(
                        shadow_rect,
                        shadow_radius,
                        dest_rect,
                        blur_radius,
                        clip_mode,
                    ),
                    spatial_node_index,
                };

                let prim_info = match clip_mode {
                    BoxShadowClipMode::Outset => {
                        // Certain spread-radii make the shadow invalid.
                        if shadow_rect.is_empty() {
                            return;
                        }

                        // Add the box-shadow clip source.
                        extra_clips.push(shadow_clip_source);

                        // Outset shadows are expanded by the shadow
                        // region from the original primitive.
                        LayoutPrimitiveInfo::with_clip_rect(dest_rect, prim_info.clip_rect)
                    }
                    BoxShadowClipMode::Inset => {
                        // If the inner shadow rect contains the prim
                        // rect, no pixels will be shadowed.
                        if border_radius.is_zero() && shadow_rect
                            .inflate(-blur_radius, -blur_radius)
                            .contains_box(&prim_info.rect)
                        {
                            return;
                        }

                        // Inset shadows are still visible, even if the
                        // inset shadow rect becomes invalid (they will
                        // just look like a solid rectangle).
                        if !shadow_rect.is_empty() {
                            extra_clips.push(shadow_clip_source);
                        }

                        // Inset shadows draw inside the original primitive.
                        prim_info.clone()
                    }
                };

                self.add_primitive(
                    spatial_node_index,
                    clip_node_id,
                    &prim_info,
                    extra_clips,
                    prim,
                );
            }
        }
    }
}

fn adjust_border_radius_for_box_shadow(radius: BorderRadius, spread_amount: f32) -> BorderRadius {
    BorderRadius {
        top_left: adjust_corner_for_box_shadow(radius.top_left, spread_amount),
        top_right: adjust_corner_for_box_shadow(radius.top_right, spread_amount),
        bottom_right: adjust_corner_for_box_shadow(radius.bottom_right, spread_amount),
        bottom_left: adjust_corner_for_box_shadow(radius.bottom_left, spread_amount),
    }
}

fn adjust_corner_for_box_shadow(corner: LayoutSize, spread_amount: f32) -> LayoutSize {
    LayoutSize::new(
        adjust_radius_for_box_shadow(corner.width, spread_amount),
        adjust_radius_for_box_shadow(corner.height, spread_amount),
    )
}

fn adjust_radius_for_box_shadow(border_radius: f32, spread_amount: f32) -> f32 {
    if border_radius > 0.0 {
        (border_radius + spread_amount).max(0.0)
    } else {
        0.0
    }
}
