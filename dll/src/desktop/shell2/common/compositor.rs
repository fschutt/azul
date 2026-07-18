//! Compositor abstraction — CPU or GPU rendering.
//!
//! This module defines the rendering backend selection and compositor
//! pipeline types:
//!
//! - [`AzBackend`] — resolved from the `AZ_BACKEND` env var or
//!   programmatic `HwAcceleration` setting.
//! - [`CompositorMode`] — the concrete GPU / CPU / Auto mode derived
//!   from `AzBackend`.
//! - [`Compositor`] trait — interface that concrete implementations
//!   (e.g. `CpuCompositor`, future GPU compositor) must satisfy.
//! - [`RenderContext`] — platform-specific rendering context handle
//!   (OpenGL, Metal, D3D11, Vulkan, or CPU).
//! - [`GpuInfo`] / [`check_gpu_blacklist`] — GPU driver inspection and
//!   blacklist for known-broken configurations.

use azul_core::geom::PhysicalSizeU32;
use azul_layout::solver3::display_list::DisplayList;

use super::error::CompositorError;

use super::debug_server::LogCategory;
use crate::log_warn;

/// Compositor mode selection.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[derive(Default)]
pub enum CompositorMode {
    /// Hardware GPU rendering (OpenGL/Metal/D3D/Vulkan)
    GPU,

    /// Software CPU rendering (like webrender sw_compositor)
    CPU,

    /// Automatic selection based on capabilities
    #[default]
    Auto,
}


impl CompositorMode {
    /// Parse compositor mode from environment variable string.
    pub fn from_env_str(s: &str) -> Option<Self> {
        match s.to_lowercase().as_str() {
            "cpu" => Some(CompositorMode::CPU),
            "gpu" => Some(CompositorMode::GPU),
            "auto" => Some(CompositorMode::Auto),
            _ => None,
        }
    }
}

// ============================================================================
// AZ_BACKEND — unified backend selection
// ============================================================================

/// Backend selection for the entire application.
///
/// Replaces the fragmented `AZUL_HEADLESS`, `AZUL_RENDERER`, and
/// `AZ_COMPOSITOR` env vars with a single `AZ_BACKEND` variable.
///
/// ```text
/// AZ_BACKEND=cpu       (default) CPU rendering in a native window (no GL context)
/// AZ_BACKEND=gpu       Force GPU rendering, fail if unavailable
/// AZ_BACKEND=auto      Try GPU, fall back to CPU on failure
/// AZ_BACKEND=headless  CPU rendering without any window (for E2E tests)
/// ```
///
/// The desktop shells default to **CPU** rendering on macOS, Windows and Linux
/// so that on-screen output matches the software-rendered headless e2e tests.
/// GPU/webrender is still fully supported: opt in with `AZ_BACKEND=gpu` (force)
/// or `AZ_BACKEND=auto` (try GPU, fall back to CPU).
///
/// `AZ_BACKEND` fully replaces those older variables; they are no longer read.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
#[derive(Default)]
#[allow(missing_copy_implementations)] // `Web(WebConfig)` carries non-Copy fields
pub enum AzBackend {
    /// Try GPU, fall back to CPU on GL init failure or blacklisted GPU.
    Auto,
    /// Force GPU rendering (OpenGL / Metal / D3D). Fails if unavailable.
    Gpu,
    /// CPU rendering inside a native window (no GL context).
    /// This is the desktop default (see [`AzBackend::resolve`]).
    #[default]
    Cpu,
    /// CPU rendering with no native window (headless / E2E testing).
    /// Implies `Cpu` for the compositor and `HeadlessWindow` for the
    /// window implementation.
    Headless,
    /// Web backend: serve the app as HTML over HTTP.
    /// `AZ_BACKEND=web://127.0.0.1:8080[?options]` starts an HTTP
    /// server. Layout runs natively, DOM is rendered to HTML,
    /// callbacks are transpiled to WASM (or executed server-side
    /// when stubbed). See `crate::web::config::WebConfig` for the
    /// supported query options.
    #[cfg(feature = "web")]
    Web(crate::web::config::WebConfig),
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
                // Windowing selectors (consumed by LinuxWindow::select_backend) — not a
                // render mode; leave the render backend at Auto rather than warning.
                "x11" | "wayland" => return AzBackend::Auto,
                _ => {
                    // Try parsing web://ip:port[?options]
                    #[cfg(feature = "web")]
                    match crate::web::config::parse_web_config(&val) {
                        Ok(cfg) => return AzBackend::Web(cfg),
                        Err(e) => {
                            log_warn!(
                                LogCategory::Rendering,
                                "AZ_BACKEND={:?} rejected: {:?}",
                                val,
                                e
                            );
                        }
                    }
                    log_warn!(
                        LogCategory::Rendering,
                        "Unrecognized AZ_BACKEND value {:?}, falling back to default",
                        val
                    );
                }
            }
        }

        // 2. Programmatic: HwAcceleration from WindowCreateOptions
        if let Some(hw) = hw_accel {
            match hw {
                azul_core::window::HwAcceleration::Disabled => return AzBackend::Cpu,
                azul_core::window::HwAcceleration::Enabled => return AzBackend::Gpu,
                azul_core::window::HwAcceleration::DontCare => {} // fall through to default
            }
        }

        // 3. Default: CPU (software) rendering on all desktop platforms.
        //
        // The desktop shells now default to the same CpuBackend the headless
        // e2e tests use, so "what the e2e tests render == what a user sees on
        // screen". GPU/webrender is still fully supported and re-selectable via
        // `AZ_BACKEND=gpu` (force GPU) or `AZ_BACKEND=auto` (try GPU, fall back
        // to CPU) — or programmatically via `HwAcceleration::Enabled`.
        AzBackend::Cpu
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

/// Query GPU vendor / renderer / version strings from a current GL context
/// and run the result through [`check_gpu_blacklist`].
///
/// Callers must have made the GL context current on the calling thread before
/// invoking this — `glGetString` only returns valid data for the current context.
///
/// Returns [`GpuCheckResult::QueryFailed`] when the vendor / renderer / version
/// strings are missing, which is the symptom of a broken / partially loaded
/// driver (azul#220).
pub fn query_gpu_info(
    gl: &gl_context_loader::GenericGlContext,
) -> GpuCheckResult {
    const GL_VENDOR: u32 = 0x1F00;
    const GL_RENDERER: u32 = 0x1F01;
    const GL_VERSION: u32 = 0x1F02;
    const GL_SHADING_LANGUAGE_VERSION: u32 = 0x8B8C;

    let vendor = gl.get_string(GL_VENDOR);
    let renderer = gl.get_string(GL_RENDERER);
    let version = gl.get_string(GL_VERSION);
    let glsl_version = gl.get_string(GL_SHADING_LANGUAGE_VERSION);

    if vendor.is_empty() && renderer.is_empty() && version.is_empty() {
        return GpuCheckResult::QueryFailed(
            "glGetString returned no vendor/renderer/version — GL context not current or driver broken".into(),
        );
    }

    let info = GpuInfo { vendor, renderer, version, glsl_version };
    check_gpu_blacklist(&info)
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

    // Mesa software rasterisers — they present as a real GL context but only
    // support GLES-level GLSL, so desktop GLSL-150 SVG/FXAA shaders fail to
    // compile (R1). cpurender is the right path for these anyway.
    if renderer_lower.contains("llvmpipe")
        || renderer_lower.contains("softpipe")
        || renderer_lower.contains("swrast")
        || renderer_lower.contains("software rasterizer")
    {
        return GpuCheckResult::Blacklisted {
            info: info.clone(),
            reason: "Mesa software rasteriser (llvmpipe/softpipe/swrast) detected — \
                     cpurender is faster and avoids desktop-GLSL shader-compile errors".into(),
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

    /// Vulkan context (Linux, Windows, future).
    ///
    /// Handles are stored as `u64` rather than `*mut c_void` because
    /// Vulkan defines dispatchable handles as pointers on 64-bit and
    /// opaque `uint64_t` on 32-bit — `u64` is the portable representation.
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

// SAFETY: `RenderContext` contains raw pointers that are owned by the
// platform window and must only be accessed from the thread that created
// the GL/Metal/D3D context.  The caller is responsible for ensuring that
// cross-thread access is properly synchronised (e.g. via
// `wglMakeCurrent` / `glXMakeCurrent` / `CGLSetCurrentContext`).
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


#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_compositor_mode_parsing() {
        assert_eq!(CompositorMode::from_env_str("cpu"), Some(CompositorMode::CPU));
        assert_eq!(CompositorMode::from_env_str("GPU"), Some(CompositorMode::GPU));
        assert_eq!(CompositorMode::from_env_str("Auto"), Some(CompositorMode::Auto));
        assert_eq!(CompositorMode::from_env_str("invalid"), None);
    }
}
