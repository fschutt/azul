use azul_core::geom::LogicalSize;

/// Information about scrollbar requirements and dimensions
#[derive(Debug, Clone)]
#[repr(C)]
pub struct ScrollbarInfo {
    pub needs_horizontal: bool,
    pub needs_vertical: bool,
    pub scrollbar_width: f32,
    pub scrollbar_height: f32,
}

impl ScrollbarInfo {
    /// Checks if the presence of scrollbars reduces the available inner size,
    /// which would necessitate a reflow of the content.
    pub fn needs_reflow(&self) -> bool {
        self.scrollbar_width > 0.0 || self.scrollbar_height > 0.0
    }

    /// Takes a size (representing a content-box) and returns a new size
    /// reduced by the dimensions of any active scrollbars.
    pub fn shrink_size(&self, size: LogicalSize) -> LogicalSize {
        LogicalSize {
            width: (size.width - self.scrollbar_width).max(0.0),
            height: (size.height - self.scrollbar_height).max(0.0),
        }
    }
}
