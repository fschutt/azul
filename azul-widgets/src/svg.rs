use std::sync::atomic::{Ordering, AtomicUsize};

static SVG_TRANSFORM_ID: AtomicUsize = AtomicUsize::new(0);
static SVG_VIEW_BOX_ID: AtomicUsize = AtomicUsize::new(0);
static VECTORIZED_FONT_ID: AtomicUsize = AtomicUsize::new(0);

#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash, Ord, PartialOrd)]
#[repr(C)]
pub struct SvgTransformId { pub id: usize }

impl SvgTransformId {
    pub fn new() -> Self {
        Self { id: SVG_TRANSFORM_ID.fetch_add(1, Ordering::SeqCst) }
    }
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash, Ord, PartialOrd)]
#[repr(C)]
pub struct SvgViewBoxId { pub id: usize }

impl SvgViewBoxId {
    pub fn new() -> Self {
        Self { id: SVG_VIEW_BOX_ID.fetch_add(1, Ordering::SeqCst) }
    }
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash)]
#[repr(C)]
pub struct VectorizedFontId { pub id: usize }

impl VectorizedFontId {
    pub fn new() -> Self {
        Self { id: VECTORIZED_FONT_ID.fetch_add(1, Ordering::SeqCst) }
    }
}

pub fn svg_xml_to_dom(xml: &Xml, parent_size: LayoutSize) -> Dom {
/*
let zoom = if self.enable_hidpi { self.zoom * hidpi_factor } else { self.zoom } * self.multisampling_factor as f32;
let pan = if self.enable_hidpi { (self.pan.0 * hidpi_factor, self.pan.1 * hidpi_factor) } else { self.pan };
let pan = (pan.0 * self.multisampling_factor as f32, pan.1 * self.multisampling_factor as f32);

*/
}