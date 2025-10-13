use alloc::collections::BTreeMap;

use azul_css::props::{basic::LayoutSize, style::StyleTransformOrigin};

use crate::{
    dom::NodeId,
    id::NodeDataContainerRef,
    resources::{OpacityKey, TransformKey},
    styled_dom::StyledDom,
    transform::{ComputedTransform3D, RotationMode},
    ui_solver::GpuOpacityKeyEvent,
};

#[derive(Default, Debug, Clone, PartialEq, PartialOrd)]
pub struct GpuValueCache {
    pub transform_keys: BTreeMap<NodeId, TransformKey>,
    pub current_transform_values: BTreeMap<NodeId, ComputedTransform3D>,
    pub opacity_keys: BTreeMap<NodeId, OpacityKey>,
    pub current_opacity_values: BTreeMap<NodeId, f32>,
}

#[derive(Debug, Clone, PartialEq, PartialOrd)]
pub enum GpuTransformKeyEvent {
    Added(NodeId, TransformKey, ComputedTransform3D),
    Changed(
        NodeId,
        TransformKey,
        ComputedTransform3D,
        ComputedTransform3D,
    ),
    Removed(NodeId, TransformKey),
}

impl GpuValueCache {
    pub fn empty() -> Self {
        Self::default()
    }

    #[must_use]
    pub fn synchronize<'a>(&mut self, styled_dom: &StyledDom) -> GpuEventChanges {
        let css_property_cache = styled_dom.get_css_property_cache();
        let node_data = styled_dom.node_data.as_container();
        let node_states = styled_dom.styled_nodes.as_container();

        let default_transform_origin = StyleTransformOrigin::default();

        #[cfg(target_arch = "x86_64")]
        unsafe {
            if !INITIALIZED.load(AtomicOrdering::SeqCst) {
                use core::arch::x86_64::__cpuid;

                let mut cpuid = __cpuid(0);
                let n_ids = cpuid.eax;

                if n_ids > 0 {
                    // cpuid instruction is present
                    cpuid = __cpuid(1);
                    USE_SSE.store((cpuid.edx & (1_u32 << 25)) != 0, AtomicOrdering::SeqCst);
                    USE_AVX.store((cpuid.ecx & (1_u32 << 28)) != 0, AtomicOrdering::SeqCst);
                }
                INITIALIZED.store(true, AtomicOrdering::SeqCst);
            }
        }

        // calculate the transform values of every single node that has a non-default transform
        let all_current_transform_events = (0..styled_dom.node_data.len())
            .into_iter()
            .filter_map(|node_id| {
                let node_id = NodeId::new(node_id);
                let styled_node_state = &node_states[node_id].state;
                let node_data = &node_data[node_id];
                let current_transform = css_property_cache
                    .get_transform(node_data, &node_id, styled_node_state)?
                    .get_property()
                    .map(|t| {
                        // TODO: look up the parent nodes size properly to resolve animation of
                        // transforms with %
                        let parent_size_width = 0.0;
                        let parent_size_height = 0.0;
                        let transform_origin = css_property_cache.get_transform_origin(
                            node_data,
                            &node_id,
                            styled_node_state,
                        );
                        let transform_origin = transform_origin
                            .as_ref()
                            .and_then(|o| o.get_property())
                            .unwrap_or(&default_transform_origin);

                        ComputedTransform3D::from_style_transform_vec(
                            t.as_ref(),
                            transform_origin,
                            parent_size_width,
                            parent_size_height,
                            RotationMode::ForWebRender,
                        )
                    });

                let existing_transform = self.current_transform_values.get(&node_id);

                match (existing_transform, current_transform) {
                    (None, None) => None, // no new transform, no old transform
                    (None, Some(new)) => Some(GpuTransformKeyEvent::Added(
                        node_id,
                        TransformKey::unique(),
                        new,
                    )),
                    (Some(old), Some(new)) => Some(GpuTransformKeyEvent::Changed(
                        node_id,
                        self.transform_keys.get(&node_id).copied()?,
                        *old,
                        new,
                    )),
                    (Some(_old), None) => Some(GpuTransformKeyEvent::Removed(
                        node_id,
                        self.transform_keys.get(&node_id).copied()?,
                    )),
                }
            })
            .collect::<Vec<GpuTransformKeyEvent>>();

        // remove / add the transform keys accordingly
        for event in all_current_transform_events.iter() {
            match &event {
                GpuTransformKeyEvent::Added(node_id, key, matrix) => {
                    self.transform_keys.insert(*node_id, *key);
                    self.current_transform_values.insert(*node_id, *matrix);
                }
                GpuTransformKeyEvent::Changed(node_id, _key, _old_state, new_state) => {
                    self.current_transform_values.insert(*node_id, *new_state);
                }
                GpuTransformKeyEvent::Removed(node_id, _key) => {
                    self.transform_keys.remove(node_id);
                    self.current_transform_values.remove(node_id);
                }
            }
        }

        // calculate the opacity of every single node that has a non-default opacity
        let all_current_opacity_events = (0..styled_dom.node_data.len())
            .into_iter()
            .filter_map(|node_id| {
                let node_id = NodeId::new(node_id);
                let styled_node_state = &node_states[node_id].state;
                let node_data = &node_data[node_id];
                let current_opacity =
                    css_property_cache.get_opacity(node_data, &node_id, styled_node_state)?;
                let current_opacity = current_opacity.get_property();
                let existing_opacity = self.current_opacity_values.get(&node_id);

                match (existing_opacity, current_opacity) {
                    (None, None) => None, // no new opacity, no old transform
                    (None, Some(new)) => Some(GpuOpacityKeyEvent::Added(
                        node_id,
                        OpacityKey::unique(),
                        new.inner.normalized(),
                    )),
                    (Some(old), Some(new)) => Some(GpuOpacityKeyEvent::Changed(
                        node_id,
                        self.opacity_keys.get(&node_id).copied()?,
                        *old,
                        new.inner.normalized(),
                    )),
                    (Some(_old), None) => Some(GpuOpacityKeyEvent::Removed(
                        node_id,
                        self.opacity_keys.get(&node_id).copied()?,
                    )),
                }
            })
            .collect::<Vec<GpuOpacityKeyEvent>>();

        // remove / add the opacity keys accordingly
        for event in all_current_opacity_events.iter() {
            match &event {
                GpuOpacityKeyEvent::Added(node_id, key, opacity) => {
                    self.opacity_keys.insert(*node_id, *key);
                    self.current_opacity_values.insert(*node_id, *opacity);
                }
                GpuOpacityKeyEvent::Changed(node_id, _key, _old_state, new_state) => {
                    self.current_opacity_values.insert(*node_id, *new_state);
                }
                GpuOpacityKeyEvent::Removed(node_id, _key) => {
                    self.opacity_keys.remove(node_id);
                    self.current_opacity_values.remove(node_id);
                }
            }
        }

        GpuEventChanges {
            transform_key_changes: all_current_transform_events,
            opacity_key_changes: all_current_opacity_events,
        }
    }
}

#[derive(Default, Debug, Clone, PartialEq, PartialOrd)]
pub struct GpuEventChanges {
    pub transform_key_changes: Vec<GpuTransformKeyEvent>,
    pub opacity_key_changes: Vec<GpuOpacityKeyEvent>,
}

impl GpuEventChanges {
    pub fn empty() -> Self {
        Self::default()
    }
    pub fn is_empty(&self) -> bool {
        self.transform_key_changes.is_empty() && self.opacity_key_changes.is_empty()
    }
    pub fn merge(&mut self, other: &mut Self) {
        self.transform_key_changes
            .extend(other.transform_key_changes.drain(..));
        self.opacity_key_changes
            .extend(other.opacity_key_changes.drain(..));
    }
}
