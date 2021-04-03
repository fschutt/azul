use crate::{
    ui_solver::{ResolvedTextLayoutOptions, InlineTextLayout},
    callbacks::PipelineId,
    id_tree::NodeId,
};

pub trait GetTextLayout {
    // self is mutable so that the calculated text can be cached if it hasn't changed since the last frame
    fn get_text_layout(&mut self, pipeline_id: &PipelineId, node_id: NodeId, text_layout_options: &ResolvedTextLayoutOptions) -> InlineTextLayout;
}