//! GPU blacklist for problematic graphics drivers and hardware.
//!
//! This module provides a mechanism to identify GPUs that should use software
//! rendering (SWGL) instead of hardware OpenGL due to known driver bugs or
//! hardware limitations.
//!
//! # Usage
//!
//! Call [`is_gpu_blacklisted`] with the OpenGL renderer and vendor strings
//! to determine if software rendering should be used.
//!
//! # Adding new entries
//!
//! To blacklist a GPU:
//! 1. Add a new entry to the appropriate `*_BLACKLIST` constant
//! 2. Use substring matching for flexibility (e.g., "Mali-4" matches "Mali-400", "Mali-450")
//! 3. Document the reason (driver bug, missing extension, etc.)
//!
//! # Platform-specific notes
//!
//! - **Windows**: Use ANGLE for OpenGL ES, check for D3D feature level
//! - **Linux**: Mali GPUs on ARM often have incomplete OpenGL ES 3.0 support
//! - **macOS**: Apple Silicon uses Metal; Intel GPUs work well with OpenGL
//! - **Android/iOS**: Mobile GPUs may need ES 3.0+ for all features
//!
//! # Related issue
//!
//! See: <https://github.com/fschutt/azul/issues/220>

use alloc::{string::String, vec::Vec};

/// GPU renderer substring patterns that should use software rendering.
///
/// These are matched against the GL_RENDERER string (case-insensitive).
///
/// Format: `(pattern, reason)`
pub const GPU_RENDERER_BLACKLIST: &[(&str, &str)] = &[
    // Mali GPUs - known issues with OpenGL ES on Linux
    // See: https://github.com/nicozanf/py4web-seo/issues/3
    ("Mali-4", "Mali-400/450 have incomplete OpenGL ES 3.0 support"),
    ("Mali-T6", "Mali-T6xx series has driver bugs with framebuffers"),
    
    // Older Adreno GPUs
    ("Adreno (TM) 2", "Adreno 2xx lacks required GL extensions"),
    ("Adreno (TM) 30", "Adreno 30x has known rendering bugs"),
    
    // Software renderers that shouldn't trigger SWGL (they're already software)
    // ("llvmpipe", "Already software rendering"),
    // ("softpipe", "Already software rendering"),
    
    // Placeholder for user-reported problematic GPUs
    // Add entries here as issues are reported
];

/// GPU vendor substring patterns that should use software rendering.
///
/// These are matched against the GL_VENDOR string (case-insensitive).
pub const GPU_VENDOR_BLACKLIST: &[(&str, &str)] = &[
    // Currently empty - vendor-level blacklisting is rarely needed
    // Add entries here if an entire vendor's drivers are problematic
];

/// Combined renderer + vendor patterns (both must match).
///
/// Format: `(renderer_pattern, vendor_pattern, reason)`
pub const GPU_COMBINED_BLACKLIST: &[(&str, &str, &str)] = &[
    // Example: A specific vendor's implementation of a GPU
    // ("Mesa DRI Intel", "Intel", "Specific Intel Mesa driver bug"),
];

/// OpenGL version requirements.
///
/// Minimum OpenGL/ES version required for hardware rendering.
/// GPUs reporting lower versions will use software rendering.
pub const MINIMUM_GL_VERSION: (u8, u8) = (3, 0);
pub const MINIMUM_GLES_VERSION: (u8, u8) = (3, 0);

/// Result of GPU blacklist check.
#[derive(Debug, Clone, PartialEq)]
pub struct GpuBlacklistResult {
    /// Whether the GPU is blacklisted
    pub is_blacklisted: bool,
    /// Reason for blacklisting (if blacklisted)
    pub reason: Option<String>,
    /// Which pattern matched (for debugging)
    pub matched_pattern: Option<String>,
}

impl GpuBlacklistResult {
    /// GPU is not blacklisted
    pub fn allowed() -> Self {
        Self {
            is_blacklisted: false,
            reason: None,
            matched_pattern: None,
        }
    }
    
    /// GPU is blacklisted
    pub fn blacklisted(reason: &str, pattern: &str) -> Self {
        Self {
            is_blacklisted: true,
            reason: Some(String::from(reason)),
            matched_pattern: Some(String::from(pattern)),
        }
    }
}

/// Check if a GPU should use software rendering based on renderer and vendor strings.
///
/// # Arguments
///
/// * `renderer` - The GL_RENDERER string from OpenGL (e.g., "Mali-400 MP")
/// * `vendor` - The GL_VENDOR string from OpenGL (e.g., "ARM")
///
pub fn is_gpu_blacklisted(renderer: &str, vendor: &str) -> GpuBlacklistResult {
    let renderer_lower = renderer.to_lowercase();
    let vendor_lower = vendor.to_lowercase();
    
    // Check renderer blacklist
    for (pattern, reason) in GPU_RENDERER_BLACKLIST {
        if renderer_lower.contains(&pattern.to_lowercase()) {
            return GpuBlacklistResult::blacklisted(reason, pattern);
        }
    }
    
    // Check vendor blacklist
    for (pattern, reason) in GPU_VENDOR_BLACKLIST {
        if vendor_lower.contains(&pattern.to_lowercase()) {
            return GpuBlacklistResult::blacklisted(reason, pattern);
        }
    }
    
    // Check combined blacklist
    for (renderer_pattern, vendor_pattern, reason) in GPU_COMBINED_BLACKLIST {
        if renderer_lower.contains(&renderer_pattern.to_lowercase())
            && vendor_lower.contains(&vendor_pattern.to_lowercase())
        {
            let combined_pattern = alloc::format!("{}+{}", renderer_pattern, vendor_pattern);
            return GpuBlacklistResult::blacklisted(reason, &combined_pattern);
        }
    }
    
    GpuBlacklistResult::allowed()
}

/// Check if the OpenGL version meets minimum requirements.
///
/// # Arguments
///
/// * `major` - OpenGL major version
/// * `minor` - OpenGL minor version  
/// * `is_es` - Whether this is OpenGL ES
///
/// # Returns
///
/// `true` if the version meets requirements, `false` if software rendering should be used.
pub fn meets_gl_version_requirements(major: u8, minor: u8, is_es: bool) -> bool {
    let (min_major, min_minor) = if is_es {
        MINIMUM_GLES_VERSION
    } else {
        MINIMUM_GL_VERSION
    };
    
    if major > min_major {
        true
    } else if major == min_major {
        minor >= min_minor
    } else {
        false
    }
}

/// Environment variable to force software rendering.
///
/// If this environment variable is set to "1", "true", or "yes",
/// software rendering will be used regardless of GPU capabilities.
pub const ENV_FORCE_SOFTWARE_RENDERING: &str = "AZUL_FORCE_SOFTWARE_RENDERING";

/// Environment variable to override GPU blacklist.
///
/// If this environment variable is set to "1", "true", or "yes",
/// the GPU blacklist will be ignored (useful for testing).
pub const ENV_IGNORE_GPU_BLACKLIST: &str = "AZUL_IGNORE_GPU_BLACKLIST";

/// Check environment variable for software rendering override.
///
/// # Note
///
/// This function requires std for environment variable access.
/// On no_std, it always returns `false`.
#[cfg(feature = "std")]
pub fn should_force_software_rendering() -> bool {
    std::env::var(ENV_FORCE_SOFTWARE_RENDERING)
        .map(|v| matches!(v.to_lowercase().as_str(), "1" | "true" | "yes"))
        .unwrap_or(false)
}

#[cfg(not(feature = "std"))]
pub fn should_force_software_rendering() -> bool {
    false
}

/// Check if GPU blacklist should be ignored.
#[cfg(feature = "std")]
pub fn should_ignore_blacklist() -> bool {
    std::env::var(ENV_IGNORE_GPU_BLACKLIST)
        .map(|v| matches!(v.to_lowercase().as_str(), "1" | "true" | "yes"))
        .unwrap_or(false)
}

#[cfg(not(feature = "std"))]
pub fn should_ignore_blacklist() -> bool {
    false
}

/// Combined check for whether to use software rendering.
///
/// Checks:
/// 1. Environment variable override (AZUL_FORCE_SOFTWARE_RENDERING)
/// 2. GPU blacklist (unless AZUL_IGNORE_GPU_BLACKLIST is set)
///
/// # Arguments
///
/// * `renderer` - The GL_RENDERER string
/// * `vendor` - The GL_VENDOR string
///
/// # Returns
///
/// `true` if software rendering should be used.
pub fn should_use_software_rendering(renderer: &str, vendor: &str) -> bool {
    // Check force override first
    if should_force_software_rendering() {
        return true;
    }
    
    // Check blacklist (unless ignored)
    if !should_ignore_blacklist() {
        let result = is_gpu_blacklisted(renderer, vendor);
        if result.is_blacklisted {
            return true;
        }
    }
    
    false
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_mali_blacklisted() {
        let result = is_gpu_blacklisted("Mali-400 MP", "ARM");
        assert!(result.is_blacklisted);
        assert!(result.reason.is_some());
    }
    
    #[test]
    fn test_nvidia_allowed() {
        let result = is_gpu_blacklisted("NVIDIA GeForce RTX 3080", "NVIDIA Corporation");
        assert!(!result.is_blacklisted);
    }
    
    #[test]
    fn test_intel_allowed() {
        let result = is_gpu_blacklisted("Intel(R) UHD Graphics 630", "Intel");
        assert!(!result.is_blacklisted);
    }
    
    #[test]
    fn test_gl_version() {
        assert!(meets_gl_version_requirements(4, 6, false));
        assert!(meets_gl_version_requirements(3, 3, false));
        assert!(meets_gl_version_requirements(3, 0, false));
        assert!(!meets_gl_version_requirements(2, 1, false));
        
        assert!(meets_gl_version_requirements(3, 2, true));
        assert!(!meets_gl_version_requirements(2, 0, true));
    }
}
