extern crate azul_css;
extern crate azul_core;
extern crate unicode_normalization;
extern crate harfbuzz_sys;

pub mod text_layout;
pub mod text_shaping;

use azul_core::{
    traits::GetTextLayout,
    ui_solver::{ResolvedTextLayoutOptions, InlineTextLayout},
    app_resources::{Words, ScaledWords},
};

#[derive(Debug, Clone)]
pub struct InlineText<'a> {
    pub words: &'a Words,
    pub scaled_words: &'a ScaledWords,
}

impl<'a> GetTextLayout for InlineText<'a> {
    fn get_text_layout(&mut self, text_layout_options: &ResolvedTextLayoutOptions) -> InlineTextLayout {
        let layouted_text_block = text_layout::position_words(
            self.words,
            self.scaled_words,
            text_layout_options,
        );
        // TODO: Cache the layouted text block on the &mut self
        text_layout::word_positions_to_inline_text_layout(&layouted_text_block, &self.scaled_words)
    }
}