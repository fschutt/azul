use crate::{
    ui_solver::{ResolvedTextLayoutOptions, InlineTextLayout},
};



pub trait GetTextLayout {
    fn get_text_layout(&mut self, text_layout_options: &ResolvedTextLayoutOptions) -> InlineTextLayout;
}