/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

use std::{ops, sync::Arc};

use api::{
    units::{DevicePixelScale, LayoutToWorldTransform, LayoutVector2D, RasterPixelScale},
    ColorF, FontInstanceFlags, GlyphInstance, RasterSpace, Shadow,
};
use glyph_rasterizer::{FontInstance, FontTransform, GlyphKey, FONT_SIZE_LIMIT};

use super::{storage, VectorKey};
use crate::{
    frame_builder::FrameBuildingState,
    gpu_cache::GpuCache,
    intern,
    internal_types::LayoutPrimitiveInfo,
    picture::SurfaceInfo,
    prim_store::{
        InternablePrimitive, PrimKeyCommonData, PrimTemplateCommonData, PrimitiveInstanceKind,
        PrimitiveOpacity, PrimitiveScratchBuffer, PrimitiveStore,
    },
    renderer::MAX_VERTEX_TEXTURE_WIDTH,
    resource_cache::ResourceCache,
    scene_building::{CreateShadow, IsVisible},
    space::SpaceSnapper,
    spatial_tree::{SpatialNodeIndex, SpatialTree},
    util::{MatrixHelpers, PrimaryArc},
};

/// A run of glyphs, with associated font information.
#[derive(Debug, Clone, Eq, PartialEq, Hash)]
pub struct TextRunKey {
    pub common: PrimKeyCommonData,
    pub font: FontInstance,
    pub glyphs: PrimaryArc<Vec<GlyphInstance>>,
    pub shadow: bool,
    pub requested_raster_space: RasterSpace,
    pub reference_frame_offset: VectorKey,
}

impl TextRunKey {
    pub fn new(info: &LayoutPrimitiveInfo, text_run: TextRun) -> Self {
        TextRunKey {
            common: info.into(),
            font: text_run.font,
            glyphs: PrimaryArc(text_run.glyphs),
            shadow: text_run.shadow,
            requested_raster_space: text_run.requested_raster_space,
            reference_frame_offset: text_run.reference_frame_offset.into(),
        }
    }
}

impl intern::InternDebug for TextRunKey {}

pub struct TextRunTemplate {
    pub common: PrimTemplateCommonData,
    pub font: FontInstance,
    pub glyphs: Arc<Vec<GlyphInstance>>,
}

impl ops::Deref for TextRunTemplate {
    type Target = PrimTemplateCommonData;
    fn deref(&self) -> &Self::Target {
        &self.common
    }
}

impl ops::DerefMut for TextRunTemplate {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.common
    }
}

impl From<TextRunKey> for TextRunTemplate {
    fn from(item: TextRunKey) -> Self {
        let common = PrimTemplateCommonData::with_key_common(item.common);
        TextRunTemplate {
            common,
            font: item.font,
            glyphs: item.glyphs.0,
        }
    }
}

impl TextRunTemplate {
    /// Update the GPU cache for a given primitive template. This may be called multiple
    /// times per frame, by each primitive reference that refers to this interned
    /// template. The initial request call to the GPU cache ensures that work is only
    /// done if the cache entry is invalid (due to first use or eviction).
    pub fn update(&mut self, frame_state: &mut FrameBuildingState) {
        self.write_prim_gpu_blocks(frame_state);
        self.opacity = PrimitiveOpacity::translucent();
    }

    fn write_prim_gpu_blocks(&mut self, frame_state: &mut FrameBuildingState) {
        // corresponds to `fetch_glyph` in the shaders
        if let Some(mut request) = frame_state
            .gpu_cache
            .request(&mut self.common.gpu_cache_handle)
        {
            request.push(ColorF::from(self.font.color).premultiplied());

            let mut gpu_block = [0.0; 4];
            for (i, src) in self.glyphs.iter().enumerate() {
                // Two glyphs are packed per GPU block.

                if (i & 1) == 0 {
                    gpu_block[0] = src.point.x;
                    gpu_block[1] = src.point.y;
                } else {
                    gpu_block[2] = src.point.x;
                    gpu_block[3] = src.point.y;
                    request.push(gpu_block);
                }
            }

            // Ensure the last block is added in the case
            // of an odd number of glyphs.
            if (self.glyphs.len() & 1) != 0 {
                request.push(gpu_block);
            }

            assert!(request.current_used_block_num() <= MAX_VERTEX_TEXTURE_WIDTH);
        }
    }
}

pub type TextRunDataHandle = intern::Handle<TextRun>;

#[derive(Debug)]
pub struct TextRun {
    pub font: FontInstance,
    pub glyphs: Arc<Vec<GlyphInstance>>,
    pub shadow: bool,
    pub requested_raster_space: RasterSpace,
    pub reference_frame_offset: LayoutVector2D,
}

impl intern::Internable for TextRun {
    type Key = TextRunKey;
    type StoreData = TextRunTemplate;
    type InternData = ();
    const PROFILE_COUNTER: usize = crate::profiler::INTERNED_TEXT_RUNS;
}

impl InternablePrimitive for TextRun {
    fn into_key(self, info: &LayoutPrimitiveInfo) -> TextRunKey {
        TextRunKey::new(info, self)
    }

    fn make_instance_kind(
        key: TextRunKey,
        data_handle: TextRunDataHandle,
        prim_store: &mut PrimitiveStore,
    ) -> PrimitiveInstanceKind {
        let reference_frame_offset = key.reference_frame_offset.into();

        let run_index = prim_store.text_runs.push(TextRunPrimitive {
            used_font: key.font.clone(),
            glyph_keys_range: storage::Range::empty(),
            reference_frame_relative_offset: reference_frame_offset,
            snapped_reference_frame_relative_offset: reference_frame_offset,
            shadow: key.shadow,
            raster_scale: 1.0,
            requested_raster_space: key.requested_raster_space,
        });

        PrimitiveInstanceKind::TextRun {
            data_handle,
            run_index,
        }
    }
}

impl CreateShadow for TextRun {
    fn create_shadow(
        &self,
        shadow: &Shadow,
        blur_is_noop: bool,
        current_raster_space: RasterSpace,
    ) -> Self {
        let mut font = FontInstance {
            color: shadow.color.into(),
            ..self.font.clone()
        };
        if shadow.blur_radius > 0.0 {
            font.disable_subpixel_aa();
        }

        let requested_raster_space = if blur_is_noop {
            current_raster_space
        } else {
            RasterSpace::Local(1.0)
        };

        TextRun {
            font,
            glyphs: self.glyphs.clone(),
            shadow: true,
            requested_raster_space,
            reference_frame_offset: self.reference_frame_offset,
        }
    }
}

impl IsVisible for TextRun {
    fn is_visible(&self) -> bool {
        self.font.color.a > 0
    }
}

#[derive(Debug)]
pub struct TextRunPrimitive {
    pub used_font: FontInstance,
    pub glyph_keys_range: storage::Range<GlyphKey>,
    pub reference_frame_relative_offset: LayoutVector2D,
    pub snapped_reference_frame_relative_offset: LayoutVector2D,
    pub shadow: bool,
    pub raster_scale: f32,
    pub requested_raster_space: RasterSpace,
}

/// How the glyphs of a run under `transform` are rasterised and placed:
/// (subpixel AA may be used, the glyphs are rasterised UNDER the transform,
/// the glyph buffers get texture padding, the font is over the size limit).
///
/// The second is always `false` here. Upstream asks its rasterisers (FreeType,
/// CoreText, DirectWrite) for glyphs under any 2D transform that is more than
/// a translation and keeps them by that transform. This fork's rasteriser
/// (`wr_azul_glyph_rasterizer::FontContext::rasterize_glyph`) draws a glyph at
/// the font's size alone - it reads no transform and only ever returns
/// `GlyphFormat::Alpha` - and the text shader then draws that upright bitmap
/// through the transform (the path without `GLYPH_TRANSFORM`, chosen by the
/// glyph's format). Asking for transformed glyphs all the same changed nothing
/// of the picture and cost three things (azul audit A4-02): the glyph cache is
/// keyed by the font instance WITH its transform, so every new scale of a
/// zoomed view rasterised and uploaded every glyph on screen again, the same
/// bitmaps as before; every turned label, whose matrix is its own, kept a set
/// of glyphs of its own; and the batcher computed the bounds of such a run as
/// if its bitmaps were placed upright in device space (`batch.rs`,
/// `TRANSFORM_GLYPHS`), which is not where the shader draws them.
fn glyph_raster_mode(
    raster_space: RasterSpace,
    transform: &LayoutToWorldTransform,
    device_font_size: f32,
) -> (bool, bool, bool, bool) {
    if raster_space != RasterSpace::Screen
        || transform.has_perspective_component()
        || !transform.has_2d_inverse()
    {
        (false, false, true, device_font_size > FONT_SIZE_LIMIT)
    } else if transform.exceeds_2d_scale((FONT_SIZE_LIMIT / device_font_size) as f64) {
        (false, false, true, true)
    } else {
        (true, false, false, false)
    }
}

impl TextRunPrimitive {
    pub fn update_font_instance(
        &mut self,
        specified_font: &FontInstance,
        surface: &SurfaceInfo,
        spatial_node_index: SpatialNodeIndex,
        transform: &LayoutToWorldTransform,
        allow_subpixel: bool,
        raster_space: RasterSpace,
        spatial_tree: &SpatialTree,
    ) -> bool {
        // If local raster space is specified, include that in the scale
        // of the glyphs that get rasterized.
        // TODO(gw): Once we support proper local space raster modes, this
        //           will implicitly be part of the device pixel ratio for
        //           the (cached) local space surface, and so this code
        //           will no longer be required.
        let raster_scale = raster_space.local_scale().unwrap_or(1.0).max(0.001);

        let dps = surface.device_pixel_scale.0;
        let font_size = specified_font.size.to_f32_px();

        // Small floating point error can accumulate in the raster * device_pixel scale.
        // Round that to the nearest 100th of a scale factor to remove this error while
        // still allowing reasonably accurate scale factors when a pinch-zoom is stopped
        // at a fractional amount.
        let quantized_scale = (dps * raster_scale * 100.0).round() / 100.0;
        let mut device_font_size = font_size * quantized_scale;

        // Check there is a valid transform that doesn't exceed the font size limit.
        // Ensure the font is supposed to be rasterized in screen-space.
        // Only support transforms that can be coerced to simple 2D transforms.
        // Add texture padding to the rasterized glyph buffer when one anticipates
        // the glyph will need to be scaled when rendered.
        let (use_subpixel_aa, transform_glyphs, texture_padding, oversized) =
            glyph_raster_mode(raster_space, transform, device_font_size);

        let font_transform = if transform_glyphs {
            // Get the font transform matrix (skew / scale) from the complete transform.
            // Fold in the device pixel scale.
            self.raster_scale = 1.0;
            FontTransform::from(transform)
        } else {
            if oversized {
                // Font sizes larger than the limit need to be scaled, thus can't use subpixels.
                // In this case we adjust the font size and raster space to ensure
                // we rasterize at the limit, to minimize the amount of scaling.
                let limited_raster_scale = FONT_SIZE_LIMIT / (font_size * dps);
                device_font_size = FONT_SIZE_LIMIT;

                // Record the raster space the text needs to be snapped in. The original raster
                // scale would have been too big.
                self.raster_scale = limited_raster_scale;
            } else {
                // Record the raster space the text needs to be snapped in. We may have changed
                // from RasterSpace::Screen due to a transform with perspective or without a 2d
                // inverse, or it may have been RasterSpace::Local all along.
                self.raster_scale = raster_scale;
            }

            // Rasterize the glyph without any transform
            FontTransform::identity()
        };

        // TODO(aosmond): Snapping really ought to happen during scene building
        // as much as possible. This will allow clips to be already adjusted
        // based on the snapping requirements of the primitive. This may affect
        // complex clips that create a different task, and when we rasterize
        // glyphs without the transform (because the shader doesn't have the
        // snap offsets to adjust its clip). These rects are fairly conservative
        // to begin with and do not appear to be causing significant issues at
        // this time.
        // A run the shader draws through a scale or a rotation keeps its
        // offset as it is, as it did while such a run asked for transformed
        // glyphs (`glyph_raster_mode`): only what the glyphs are kept by has
        // changed, not where they are drawn.
        let drawn_through_transform = use_subpixel_aa && !transform.is_simple_2d_translation();
        self.snapped_reference_frame_relative_offset = if transform_glyphs
            || drawn_through_transform
        {
            // Don't touch the reference frame relative offset. We'll let the
            // shader do the snapping in device pixels.
            self.reference_frame_relative_offset
        } else {
            // TODO(dp): The SurfaceInfo struct needs to be updated to use RasterPixelScale
            //           rather than DevicePixelScale, however this is a large chunk of
            //           work that will be done as a follow up patch.
            let raster_pixel_scale = RasterPixelScale::new(surface.device_pixel_scale.0);

            // There may be an animation, so snap the reference frame relative
            // offset such that it excludes the impact, if any.
            let snap_to_device = SpaceSnapper::new_with_target(
                surface.raster_spatial_node_index,
                spatial_node_index,
                raster_pixel_scale,
                spatial_tree,
            );
            snap_to_device
                .snap_point(&self.reference_frame_relative_offset.to_point())
                .to_vector()
        };

        let mut flags = specified_font.flags;
        if transform_glyphs {
            flags |= FontInstanceFlags::TRANSFORM_GLYPHS;
        }
        if texture_padding {
            flags |= FontInstanceFlags::TEXTURE_PADDING;
        }

        // If the transform or device size is different, then the caller of
        // this method needs to know to rebuild the glyphs.
        let cache_dirty = self.used_font.transform != font_transform
            || self.used_font.size != device_font_size.into()
            || self.used_font.flags != flags;

        // Construct used font instance from the specified font instance
        self.used_font = FontInstance {
            transform: font_transform,
            size: device_font_size.into(),
            flags,
            ..specified_font.clone()
        };

        // If using local space glyphs, we don't want subpixel AA.
        if !allow_subpixel || !use_subpixel_aa {
            self.used_font.disable_subpixel_aa();

            // Disable subpixel positioning for oversized glyphs to avoid
            // thrashing the glyph cache with many subpixel variations of
            // big glyph textures. A possible subpixel positioning error
            // is small relative to the maximum font size and thus should
            // not be very noticeable.
            if oversized {
                self.used_font.disable_subpixel_position();
            }
        }

        cache_dirty
    }

    /// Gets the raster space to use when rendering this primitive.
    /// Usually this would be the requested raster space. However, if
    /// the primitive's spatial node or one of its ancestors is being pinch zoomed
    /// then we round it. This prevents us rasterizing glyphs for every minor
    /// change in zoom level, as that would be too expensive.
    fn get_raster_space_for_prim(
        &self,
        prim_spatial_node_index: SpatialNodeIndex,
        low_quality_pinch_zoom: bool,
        device_pixel_scale: DevicePixelScale,
        spatial_tree: &SpatialTree,
    ) -> RasterSpace {
        let prim_spatial_node = spatial_tree.get_spatial_node(prim_spatial_node_index);
        if prim_spatial_node.is_ancestor_or_self_zooming {
            if low_quality_pinch_zoom {
                // In low-quality mode, we set the scale to be 1.0. However, the device-pixel
                // scale selected for the zoom will be taken into account in the caller to this
                // function when it's converted from local -> device pixels. Since in this mode
                // the device-pixel scale is constant during the zoom, this gives the desired
                // performance while also allowing the scale to be adjusted to a new factor at
                // the end of a pinch-zoom.
                RasterSpace::Local(1.0)
            } else {
                let root_spatial_node_index = spatial_tree.root_reference_frame_index();

                // For high-quality mode, we quantize the exact scale factor as before. However,
                // we want to _undo_ the effect of the device-pixel scale on the picture cache
                // tiles (which changes now that they are raster roots). Divide the rounded value
                // by the device-pixel scale so that the local -> device conversion has no effect.
                let scale_factors = spatial_tree
                    .get_relative_transform(prim_spatial_node_index, root_spatial_node_index)
                    .scale_factors();

                // Round the scale up to the nearest power of 2, but don't exceed 8.
                let scale = scale_factors.0.max(scale_factors.1).min(8.0).max(1.0);
                let rounded_up = 2.0f32.powf(scale.log2().ceil());

                RasterSpace::Local(rounded_up / device_pixel_scale.0)
            }
        } else {
            // Assume that if we have a RasterSpace::Local, it is frequently changing, in which
            // case we want to undo the device-pixel scale, as we do above.
            match self.requested_raster_space {
                RasterSpace::Local(scale) => RasterSpace::Local(scale / device_pixel_scale.0),
                RasterSpace::Screen => RasterSpace::Screen,
            }
        }
    }

    pub fn request_resources(
        &mut self,
        prim_offset: LayoutVector2D,
        specified_font: &FontInstance,
        glyphs: &[GlyphInstance],
        transform: &LayoutToWorldTransform,
        surface: &SurfaceInfo,
        spatial_node_index: SpatialNodeIndex,
        allow_subpixel: bool,
        low_quality_pinch_zoom: bool,
        resource_cache: &mut ResourceCache,
        gpu_cache: &mut GpuCache,
        spatial_tree: &SpatialTree,
        scratch: &mut PrimitiveScratchBuffer,
    ) {
        let raster_space = self.get_raster_space_for_prim(
            spatial_node_index,
            low_quality_pinch_zoom,
            surface.device_pixel_scale,
            spatial_tree,
        );

        let cache_dirty = self.update_font_instance(
            specified_font,
            surface,
            spatial_node_index,
            transform,
            allow_subpixel,
            raster_space,
            spatial_tree,
        );

        if self.glyph_keys_range.is_empty() || cache_dirty {
            let subpx_dir = self.used_font.get_subpx_dir();

            let dps = surface.device_pixel_scale.0;
            let transform = match raster_space {
                RasterSpace::Local(scale) => FontTransform::new(scale * dps, 0.0, 0.0, scale * dps),
                RasterSpace::Screen => self.used_font.transform.scale(dps),
            };

            self.glyph_keys_range = scratch.glyph_keys.extend(glyphs.iter().map(|src| {
                let src_point = src.point + prim_offset;
                let device_offset = transform.transform(&src_point);
                GlyphKey::new(src.index, device_offset, subpx_dir)
            }));
        }

        resource_cache.request_glyphs(
            self.used_font.clone(),
            &scratch.glyph_keys[self.glyph_keys_range],
            gpu_cache,
        );
    }
}

/// These are linux only because FontInstancePlatformOptions varies in size by platform.
#[test]
#[cfg(target_os = "linux")]
fn test_struct_sizes() {
    use std::mem;
    // The sizes of these structures are critical for performance on a number of
    // talos stress tests. If you get a failure here on CI, there's two possibilities:
    // (a) You made a structure smaller than it currently is. Great work! Update the
    //     test expectations and move on.
    // (b) You made a structure larger. This is not necessarily a problem, but should only
    //     be done with care, and after checking if talos performance regresses badly.
    assert_eq!(mem::size_of::<TextRun>(), 72, "TextRun size changed");
    assert_eq!(
        mem::size_of::<TextRunTemplate>(),
        80,
        "TextRunTemplate size changed"
    );
    assert_eq!(mem::size_of::<TextRunKey>(), 88, "TextRunKey size changed");
    assert_eq!(
        mem::size_of::<TextRunPrimitive>(),
        80,
        "TextRunPrimitive size changed"
    );
}

#[cfg(test)]
mod audit_a4_tests {
    //! A4-02: a run under a scale or a rotation is rasterised like a run that
    //! is only moved - its glyphs are the same glyph cache entries.
    use api::{units::LayoutToWorldTransform, RasterSpace};
    use euclid::Angle;

    use super::glyph_raster_mode;

    #[test]
    fn no_2d_transform_asks_for_glyphs_rasterised_under_it() {
        let moved = LayoutToWorldTransform::translation(12.5, -3.0, 0.0);
        let plain = glyph_raster_mode(RasterSpace::Screen, &moved, 14.0);
        assert_eq!(plain, (true, false, false, false));

        let mut steps = vec![LayoutToWorldTransform::identity()];
        // The steps of a zoom, a turned label, a turned label in a zoomed view.
        for k in [1.02f32, 1.04, 1.414, 2.0, 0.7] {
            steps.push(LayoutToWorldTransform::scale(k, k, 1.0));
        }
        steps.push(LayoutToWorldTransform::rotation(0.0, 0.0, 1.0, Angle::degrees(37.0)));
        steps.push(
            LayoutToWorldTransform::rotation(0.0, 0.0, 1.0, Angle::degrees(-12.0))
                .pre_scale(1.3, 1.3, 1.0),
        );
        for transform in &steps {
            assert_eq!(
                glyph_raster_mode(RasterSpace::Screen, transform, 14.0),
                plain,
                "{transform:?}: the mode of a run that is only moved, so the same font instance \
                 and the same glyphs (mutant: `!transform.is_simple_2d_translation()` asks for \
                 glyphs under each of these)"
            );
        }

        // What needed another mode before still gets it.
        assert_eq!(
            glyph_raster_mode(RasterSpace::Local(2.0), &moved, 14.0),
            (false, false, true, false)
        );
        let huge = LayoutToWorldTransform::scale(40.0, 40.0, 1.0);
        assert_eq!(
            glyph_raster_mode(RasterSpace::Screen, &huge, 14.0),
            (false, false, true, true),
            "a scale past the font size limit"
        );
    }
}
