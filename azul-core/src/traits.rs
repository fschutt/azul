use crate::{
    callbacks::DocumentId,
    id_tree::NodeId,
    ui_solver::{InlineTextLayout, ResolvedTextLayoutOptions},
};

pub trait GetTextLayout {
    // self is mutable so that the calculated text can be cached if it hasn't changed since the last frame
    fn get_text_layout(
        &mut self,
        document_id: &DocumentId,
        node_id: NodeId,
        text_layout_options: &ResolvedTextLayoutOptions,
    ) -> InlineTextLayout;
}
