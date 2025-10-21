//! CPU compositor stub.
//!
//! This is a placeholder for the CPU-based software renderer.
//!
//! TODO: Implement based on webrender's sw_compositor.rs
//! Reference: https://github.com/servo/webrender/blob/master/swgl/src/sw_compositor.rs
//!
//! The CPU compositor should:
//! 1. Rasterize DisplayList to RGBA8 framebuffer
//! 2. Handle basic primitives: rectangles, text, images, gradients
//! 3. Support clipping and transformations
//! 4. Optimize with SIMD where possible

use azul_core::geom::PhysicalSizeU32;
use azul_layout::solver3::display_list::DisplayList;

use crate::desktop::shell2::common::{Compositor, CompositorError, CompositorMode, RenderContext};

/// CPU-based software compositor.
pub struct CpuCompositor {
    framebuffer: Vec<u8>,
    width: u32,
    height: u32,
}

impl CpuCompositor {
    /// Create a new CPU compositor.
    pub fn new_cpu(size: PhysicalSizeU32) -> Result<Self, CompositorError> {
        let width = size.width;
        let height = size.height;
        let framebuffer = vec![0u8; (width * height * 4) as usize];

        Ok(Self {
            framebuffer,
            width,
            height,
        })
    }

    /// Get framebuffer data (RGBA8).
    pub fn get_framebuffer(&self) -> &[u8] {
        &self.framebuffer
    }

    /// Clear framebuffer to color.
    fn clear(&mut self, r: u8, g: u8, b: u8, a: u8) {
        for chunk in self.framebuffer.chunks_exact_mut(4) {
            chunk[0] = r;
            chunk[1] = g;
            chunk[2] = b;
            chunk[3] = a;
        }
    }

    /// Rasterize display list to framebuffer.
    fn rasterize(&mut self, _display_list: &DisplayList) {
        // TODO: Implement actual rasterization
        // For now, just clear to white
        self.clear(255, 255, 255, 255);
    }
}

impl Compositor for CpuCompositor {
    fn new(_context: RenderContext, _mode: CompositorMode) -> Result<Self, CompositorError> {
        // Create with default size, will be resized
        Self::new_cpu(PhysicalSizeU32 {
            width: 800,
            height: 600,
        })
    }

    fn render(&mut self, display_list: &DisplayList) -> Result<(), CompositorError> {
        self.rasterize(display_list);
        Ok(())
    }

    fn resize(&mut self, new_size: PhysicalSizeU32) -> Result<(), CompositorError> {
        self.width = new_size.width;
        self.height = new_size.height;
        self.framebuffer = vec![0u8; (self.width * self.height * 4) as usize];
        Ok(())
    }

    fn get_mode(&self) -> CompositorMode {
        CompositorMode::CPU
    }

    fn try_switch_mode(&mut self, mode: CompositorMode) -> Result<(), CompositorError> {
        match mode {
            CompositorMode::CPU => Ok(()), // Already CPU
            _ => Err(CompositorError::UnsupportedMode(
                "Cannot switch from CPU to GPU at runtime".into(),
            )),
        }
    }

    fn flush(&mut self) {
        // Nothing to flush for CPU rendering
    }

    fn present(&mut self) -> Result<(), CompositorError> {
        // Framebuffer is already ready
        // Platform window will copy it to screen
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use azul_core::geom::PhysicalSize;

    use super::*;

    #[test]
    fn test_cpu_compositor_creation() {
        let size = PhysicalSizeU32 {
            width: 800,
            height: 600,
        };
        let compositor = CpuCompositor::new_cpu(size).unwrap();
        assert_eq!(compositor.get_framebuffer().len(), 800 * 600 * 4);
    }

    #[test]
    fn test_cpu_compositor_clear() {
        let size = PhysicalSize {
            width: 2,
            height: 2,
        };
        let mut compositor = CpuCompositor::new_cpu(size).unwrap();
        compositor.clear(255, 0, 0, 255);

        let fb = compositor.get_framebuffer();
        assert_eq!(&fb[0..4], &[255, 0, 0, 255]); // First pixel red
    }

    #[test]
    fn test_cpu_compositor_resize() {
        let mut compositor = CpuCompositor::new_cpu(PhysicalSize {
            width: 800,
            height: 600,
        })
        .unwrap();

        compositor
            .resize(PhysicalSize {
                width: 1024,
                height: 768,
            })
            .unwrap();

        assert_eq!(compositor.get_framebuffer().len(), 1024 * 768 * 4);
    }
}
