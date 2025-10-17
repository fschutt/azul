//! Paged media layout engine.
//!
//! This module provides basic infrastructure for multi-page document layout.

use std::sync::Arc;

use azul_core::geom::LogicalSize;

use crate::{
    solver3::display_list::DisplayList,
    text3::cache::{ParsedFontTrait, UnifiedLayout},
};

#[derive(Debug, Clone)]
pub struct Page<T: ParsedFontTrait> {
    pub layout: Arc<UnifiedLayout<T>>,
    pub page_number: usize,
    pub page_size: LogicalSize,
}

#[allow(unused_variables)]
pub fn layout_to_pages<T: ParsedFontTrait + 'static>(page_size: LogicalSize) -> Vec<Page<T>> {
    Vec::new()
}

pub fn generate_display_lists_from_paged_layout<T: ParsedFontTrait>(
    pages: &[Page<T>],
) -> Vec<DisplayList> {
    pages.iter().map(|_| DisplayList::default()).collect()
}
