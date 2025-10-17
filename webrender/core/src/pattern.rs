/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

use api::{units::DeviceRect, ColorF};

use crate::{clip::ClipStore, render_task_graph::{RenderTaskGraphBuilder, RenderTaskId}, renderer::GpuBufferBuilder, scene::SceneProperties, spatial_tree::SpatialTree};

#[repr(u32)]
#[cfg_attr(feature = "capture", derive(Serialize))]
#[cfg_attr(feature = "replay", derive(Deserialize))]
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub enum PatternKind {
    ColorOrTexture = 0,
    RadialGradient = 1,
    ConicGradient = 2,

    Mask = 3,
    // When adding patterns, don't forget to update the NUM_PATTERNS constant.
}

pub const NUM_PATTERNS: u32 = 4;

impl PatternKind {
    pub fn from_u32(val: u32) -> Self {
        assert!(val < NUM_PATTERNS);
        unsafe { std::mem::transmute(val) }
    }
}

/// A 32bit payload used as input for the pattern-specific logic in the shader.
///
/// Patterns typically use it as a GpuBuffer offset to fetch their data.
#[cfg_attr(feature = "capture", derive(Serialize))]
#[cfg_attr(feature = "replay", derive(Deserialize))]
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub struct PatternShaderInput(pub i32, pub i32);

impl Default for PatternShaderInput {
    fn default() -> Self {
        PatternShaderInput(0, 0)
    }
}

#[cfg_attr(feature = "capture", derive(Serialize))]
#[cfg_attr(feature = "replay", derive(Deserialize))]
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub struct PatternTextureInput {
    pub task_id: RenderTaskId,
}

impl Default for PatternTextureInput {
    fn default() -> Self {
        PatternTextureInput {
            task_id: RenderTaskId::INVALID,
        }
    }
}

impl PatternTextureInput {
    pub fn new(task_id: RenderTaskId) -> Self {
        PatternTextureInput {
            task_id,
        }
    }
}

pub struct PatternBuilderContext<'a> {
    pub scene_properties: &'a SceneProperties,
    pub spatial_tree: &'a SpatialTree,
}

pub struct PatternBuilderState<'a> {
    pub frame_gpu_data: &'a mut GpuBufferBuilder,
    pub rg_builder: &'a mut RenderTaskGraphBuilder,
    pub clip_store: &'a mut ClipStore,
}

pub trait PatternBuilder {
    fn build(
        &self,
        _sub_rect: Option<DeviceRect>,
        _ctx: &PatternBuilderContext,
        _state: &mut PatternBuilderState,
    ) -> Pattern;

    fn get_base_color(
        &self,
        _ctx: &PatternBuilderContext,
    ) -> ColorF;

    fn use_shared_pattern(
        &self,
    ) -> bool;

    fn can_use_nine_patch(&self) -> bool {
        true
    }
}

#[cfg_attr(feature = "capture", derive(Serialize))]
#[derive(Clone, Debug)]
pub struct Pattern {
    pub kind: PatternKind,
    pub shader_input: PatternShaderInput,
    pub texture_input: PatternTextureInput,
    pub base_color: ColorF,
    pub is_opaque: bool,
}

impl Pattern {
    pub fn texture(task_id: RenderTaskId, color: ColorF) -> Self {
        Pattern {
            kind: PatternKind::ColorOrTexture,
            shader_input: PatternShaderInput::default(),
            texture_input: PatternTextureInput::new(task_id),
            base_color: color,
            // TODO(gw): We may want to add support to render tasks to query
            //           if they are known to be opaque.
            is_opaque: false,
        }
    }

    pub fn color(color: ColorF) -> Self {
        Pattern {
            kind: PatternKind::ColorOrTexture,
            shader_input: PatternShaderInput::default(),
            texture_input: PatternTextureInput::default(),
            base_color: color,
            is_opaque: color.a >= 1.0,
        }
    }

    pub fn clear() -> Self {
        // Opaque black with operator dest out
        Pattern {
            kind: PatternKind::ColorOrTexture,
            shader_input: PatternShaderInput::default(),
            texture_input: PatternTextureInput::default(),
            base_color: ColorF::BLACK,
            is_opaque: false,
        }
    }
}
