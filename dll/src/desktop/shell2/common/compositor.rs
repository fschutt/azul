//! Compositor abstraction - CPU or GPU rendering.

use azul_core::geom::PhysicalSizeU32;
use azul_layout::solver3::display_list::DisplayList;

use super::error::CompositorError;

/// Compositor mode selection.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum CompositorMode {
    /// Hardware GPU rendering (OpenGL/Metal/D3D/Vulkan)
    GPU,

    /// Software CPU rendering (like webrender sw_compositor)
    CPU,

    /// Automatic selection based on capabilities
    Auto,
}

impl Default for CompositorMode {
    fn default() -> Self {
        CompositorMode::Auto
    }
}

impl CompositorMode {
    /// Parse compositor mode from string (for environment variable).
    pub fn from_str(s: &str) -> Option<Self> {
        match s.to_lowercase().as_str() {
            "cpu" => Some(CompositorMode::CPU),
            "gpu" => Some(CompositorMode::GPU),
            "auto" => Some(CompositorMode::Auto),
            _ => None,
        }
    }

    /// Get compositor mode from environment variable AZUL_COMPOSITOR.
    pub fn from_env() -> Option<Self> {
        std::env::var("AZUL_COMPOSITOR")
            .ok()
            .and_then(|s| Self::from_str(&s))
    }
}

/// Render context handle - platform-specific rendering context.
#[derive(Debug, Clone, Copy)]
pub enum RenderContext {
    /// OpenGL context (all platforms)
    OpenGL {
        /// Platform-specific GL context pointer
        context: *mut core::ffi::c_void,
    },

    /// Metal context (macOS)
    #[cfg(target_os = "macos")]
    Metal {
        /// MTLDevice pointer
        device: *mut core::ffi::c_void,
        /// MTLCommandQueue pointer
        command_queue: *mut core::ffi::c_void,
    },

    /// Direct3D 11 context (Windows)
    #[cfg(target_os = "windows")]
    D3D11 {
        /// ID3D11Device pointer
        device: *mut core::ffi::c_void,
        /// ID3D11DeviceContext pointer
        context: *mut core::ffi::c_void,
    },

    /// Vulkan context (Linux, Windows, future)
    Vulkan {
        /// VkInstance handle
        instance: u64,
        /// VkPhysicalDevice handle
        physical_device: u64,
        /// VkDevice handle
        device: u64,
    },

    /// CPU-only rendering (no GPU context)
    CPU,
}

// Safety: RenderContext contains raw pointers but they're owned by the window
unsafe impl Send for RenderContext {}
unsafe impl Sync for RenderContext {}

/// Compositor abstraction - renders DisplayList to window.
pub trait Compositor {
    /// Initialize compositor with window context and mode.
    fn new(context: RenderContext, mode: CompositorMode) -> Result<Self, CompositorError>
    where
        Self: Sized;

    /// Render a display list to the window.
    fn render(&mut self, display_list: &DisplayList) -> Result<(), CompositorError>;

    /// Resize framebuffer to new size.
    fn resize(&mut self, new_size: PhysicalSizeU32) -> Result<(), CompositorError>;

    /// Get current compositor mode.
    fn get_mode(&self) -> CompositorMode;

    /// Try to switch compositor mode at runtime.
    /// Returns Ok(()) if switch successful, Err if not possible.
    fn try_switch_mode(&mut self, mode: CompositorMode) -> Result<(), CompositorError>;

    /// Flush pending rendering commands.
    fn flush(&mut self);

    /// Present rendered frame to window (swap buffers).
    fn present(&mut self) -> Result<(), CompositorError>;
}

/// System capabilities for compositor selection.
#[derive(Debug, Clone, Default)]
pub struct SystemCapabilities {
    pub has_opengl: bool,
    pub has_metal: bool,
    pub has_d3d11: bool,
    pub has_vulkan: bool,
    pub opengl_version: Option<String>,
}

impl SystemCapabilities {
    /// Check if any GPU rendering is available.
    pub fn has_any_gpu(&self) -> bool {
        self.has_opengl || self.has_metal || self.has_d3d11 || self.has_vulkan
    }

    /// Detect system capabilities.
    pub fn detect() -> Self {
        // TODO: Implement actual detection
        // For now, assume OpenGL is available on all platforms
        Self {
            has_opengl: true,
            has_metal: cfg!(target_os = "macos"),
            has_d3d11: cfg!(target_os = "windows"),
            has_vulkan: false, // TODO: Detect Vulkan
            opengl_version: Some("3.3".into()),
        }
    }
}

/// Select appropriate compositor mode based on capabilities and request.
pub fn select_compositor_mode(
    requested: CompositorMode,
    capabilities: &SystemCapabilities,
) -> CompositorMode {
    match requested {
        CompositorMode::Auto => {
            // Prefer GPU if available, fallback to CPU
            if capabilities.has_any_gpu() {
                CompositorMode::GPU
            } else {
                CompositorMode::CPU
            }
        }
        CompositorMode::GPU => {
            // Request GPU, fallback to CPU if not available
            if capabilities.has_any_gpu() {
                CompositorMode::GPU
            } else {
                eprintln!("Warning: GPU requested but not available, using CPU fallback");
                CompositorMode::CPU
            }
        }
        CompositorMode::CPU => CompositorMode::CPU,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_compositor_mode_parsing() {
        assert_eq!(CompositorMode::from_str("cpu"), Some(CompositorMode::CPU));
        assert_eq!(CompositorMode::from_str("GPU"), Some(CompositorMode::GPU));
        assert_eq!(CompositorMode::from_str("Auto"), Some(CompositorMode::Auto));
        assert_eq!(CompositorMode::from_str("invalid"), None);
    }

    #[test]
    fn test_capabilities_detection() {
        let caps = SystemCapabilities::detect();
        assert!(caps.has_opengl); // Should always have OpenGL as fallback
    }

    #[test]
    fn test_compositor_selection() {
        let caps = SystemCapabilities {
            has_opengl: true,
            ..Default::default()
        };

        assert_eq!(
            select_compositor_mode(CompositorMode::Auto, &caps),
            CompositorMode::GPU
        );

        let caps_no_gpu = SystemCapabilities::default();
        assert_eq!(
            select_compositor_mode(CompositorMode::Auto, &caps_no_gpu),
            CompositorMode::CPU
        );
    }
}
