//! Compositor abstraction - CPU or GPU rendering.

use azul_core::geom::PhysicalSizeU32;
use azul_layout::solver3::display_list::DisplayList;

use super::error::CompositorError;

use super::debug_server::LogCategory;
use crate::{log_debug, log_error, log_info, log_trace, log_warn};

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

// ============================================================================
// AZ_BACKEND — unified backend selection
// ============================================================================

/// Backend selection for the entire application.
///
/// Replaces the fragmented `AZUL_HEADLESS`, `AZUL_RENDERER`, and
/// `AZUL_COMPOSITOR` env vars with a single `AZ_BACKEND` variable.
///
/// ```text
/// AZ_BACKEND=auto      (default) Try GPU, fall back to CPU on failure
/// AZ_BACKEND=gpu       Force GPU rendering, fail if unavailable
/// AZ_BACKEND=cpu       CPU rendering in a native window (no GL context)
/// AZ_BACKEND=headless  CPU rendering without any window (for E2E tests)
/// ```
///
/// The old env vars (`AZUL_HEADLESS`, `AZUL_RENDERER`) are still
/// recognised for backward compatibility but `AZ_BACKEND` takes priority.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum AzBackend {
    /// Try GPU, fall back to CPU on GL init failure or blacklisted GPU.
    Auto,
    /// Force GPU rendering (OpenGL / Metal / D3D). Fails if unavailable.
    Gpu,
    /// CPU rendering inside a native window (no GL context).
    Cpu,
    /// CPU rendering with no native window (headless / E2E testing).
    /// Implies `Cpu` for the compositor and `HeadlessWindow` for the
    /// window implementation.
    Headless,
}

impl Default for AzBackend {
    fn default() -> Self {
        AzBackend::Auto
    }
}

impl AzBackend {
    /// Resolve the backend from environment variable and config.
    ///
    /// Priority order:
    /// 1. `AZ_BACKEND` env var (highest)
    /// 2. `WindowCreateOptions.renderer.hw_accel` (programmatic)
    /// 3. Default: `Auto`
    pub fn resolve(hw_accel: Option<azul_core::window::HwAcceleration>) -> Self {
        // 1. AZ_BACKEND env var
        if let Ok(val) = std::env::var("AZ_BACKEND") {
            match val.to_lowercase().as_str() {
                "headless" => return AzBackend::Headless,
                "cpu" => return AzBackend::Cpu,
                "gpu" | "opengl" | "gl" => return AzBackend::Gpu,
                "auto" => return AzBackend::Auto,
                _ => {} // unrecognised — fall through
            }
        }

        // 2. Programmatic: HwAcceleration from WindowCreateOptions
        if let Some(hw) = hw_accel {
            match hw {
                azul_core::window::HwAcceleration::Disabled => return AzBackend::Cpu,
                azul_core::window::HwAcceleration::Enabled => return AzBackend::Gpu,
                azul_core::window::HwAcceleration::DontCare => {} // fall through to Auto
            }
        }

        // 3. Default
        AzBackend::Auto
    }

    /// Whether this backend needs a native platform window.
    pub fn needs_native_window(self) -> bool {
        match self {
            AzBackend::Headless => false,
            AzBackend::Cpu | AzBackend::Gpu | AzBackend::Auto => true,
        }
    }

    /// Whether this backend uses GPU rendering.
    /// Returns `None` for `Auto` (needs runtime probe).
    pub fn uses_gpu(self) -> Option<bool> {
        match self {
            AzBackend::Gpu => Some(true),
            AzBackend::Cpu | AzBackend::Headless => Some(false),
            AzBackend::Auto => None,
        }
    }

    /// Convert to CompositorMode (for the rendering pipeline).
    pub fn to_compositor_mode(self) -> CompositorMode {
        match self {
            AzBackend::Gpu => CompositorMode::GPU,
            AzBackend::Cpu | AzBackend::Headless => CompositorMode::CPU,
            AzBackend::Auto => CompositorMode::Auto,
        }
    }
}

// ============================================================================
// GPU blacklist — skip GPU on known-broken drivers
// ============================================================================

/// GPU information gathered from the GL driver.
#[derive(Debug, Clone, Default)]
pub struct GpuInfo {
    /// GL_VENDOR string (e.g. "NVIDIA Corporation")
    pub vendor: String,
    /// GL_RENDERER string (e.g. "GeForce GTX 1060/PCIe/SSE2")
    pub renderer: String,
    /// GL_VERSION string (e.g. "4.6.0 NVIDIA 535.183.01")
    pub version: String,
    /// GL_SHADING_LANGUAGE_VERSION (e.g. "4.60 NVIDIA")
    pub glsl_version: String,
}

/// Result of checking the GPU against the blacklist.
#[derive(Debug, Clone)]
pub enum GpuCheckResult {
    /// GPU is fine, proceed with GPU rendering.
    Ok(GpuInfo),
    /// GPU is blacklisted, reason given. Fall back to CPU.
    Blacklisted { info: GpuInfo, reason: String },
    /// Could not query GPU info (GL init failed). Fall back to CPU.
    QueryFailed(String),
}

/// Check if the current GPU is blacklisted.
///
/// Called during `Auto` backend resolution after successfully creating
/// an OpenGL context.  If the GPU is blacklisted, the window switches
/// to CPU rendering and the GL context is destroyed.
///
/// Known problematic configurations (see azul#220):
/// - NVIDIA with missing shader compiler (driver bug on some Linux distros)
/// - Mesa software rasteriser (llvmpipe) — works but is slower than cpurender
/// - Very old Intel HD Graphics with GL < 3.0
pub fn check_gpu_blacklist(info: &GpuInfo) -> GpuCheckResult {
    let vendor_lower = info.vendor.to_lowercase();
    let renderer_lower = info.renderer.to_lowercase();
    let version_lower = info.version.to_lowercase();

    // Mesa llvmpipe — software rasteriser, cpurender is faster
    if renderer_lower.contains("llvmpipe") || renderer_lower.contains("softpipe") {
        return GpuCheckResult::Blacklisted {
            info: info.clone(),
            reason: "Mesa software rasteriser (llvmpipe/softpipe) detected — \
                     cpurender is faster for this configuration".into(),
        };
    }

    // NVIDIA shader compiler missing (azul#220)
    // Detected by GLSL version being empty or "0.0"
    if vendor_lower.contains("nvidia") && (
        info.glsl_version.is_empty() ||
        info.glsl_version.starts_with("0.") ||
        info.glsl_version == "0"
    ) {
        return GpuCheckResult::Blacklisted {
            info: info.clone(),
            reason: "NVIDIA driver without shader compiler (azul#220) — \
                     the GL driver loads but cannot compile shaders".into(),
        };
    }

    // Intel HD with GL < 3.0 (too old for WebRender)
    if vendor_lower.contains("intel") {
        // Try to parse major GL version
        let major = version_lower
            .chars()
            .take_while(|c| c.is_ascii_digit())
            .collect::<String>()
            .parse::<u32>()
            .unwrap_or(0);
        if major > 0 && major < 3 {
            return GpuCheckResult::Blacklisted {
                info: info.clone(),
                reason: format!(
                    "Intel GPU with OpenGL {}.x — WebRender requires OpenGL 3.0+",
                    major
                ),
            };
        }
    }

    GpuCheckResult::Ok(info.clone())
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
                log_warn!(
                    LogCategory::Rendering,
                    "Warning: GPU requested but not available, using CPU fallback"
                );
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
