use azul::svg::*;

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