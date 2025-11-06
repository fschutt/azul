/// FXAA (Fast Approximate Anti-Aliasing) shader implementation
///
/// This provides an optional post-processing anti-aliasing effect that works
/// without requiring MSAA support (which isn't universally available) and
/// without the performance cost of supersampling.
///
/// FXAA works by:
/// 1. Detecting edges in the rendered image using luminance
/// 2. Selectively blurring along detected edges to smooth them
/// 3. Preserving sharp details in non-edge regions
///
/// The shader can be toggled on/off for performance-sensitive applications.
///
/// ## Implementation Status
///
/// The FXAA shader infrastructure is ready and integrated into
/// `GlContextPtrInner.fxaa_shader`. The shader source code is defined below but the actual
/// compilation and integration is TODO (currently set to 0 in gl.rs:1030).
///
/// ## Usage
///
/// Once implemented, FXAA can be enabled with:
///
/// ```ignore
/// let config = FxaaConfig::enabled(); // or ::high_quality(), ::balanced(), ::performance()
/// // Apply FXAA as post-processing step after rendering
/// ```
///
/// ## Performance
///
/// FXAA is significantly faster than supersampling and works without hardware MSAA support.
/// Typical overhead is 1-2ms at 1080p on modern GPUs.
use gl_context_loader::{GLint, GLuint};

/// FXAA shader configuration
#[derive(Debug, Clone)]
pub struct FxaaConfig {
    /// Enable/disable FXAA
    pub enabled: bool,
    /// Edge detection threshold (0.063 - 0.333, default: 0.125)
    /// Lower = more edges detected = more AA but potential blur
    pub edge_threshold: f32,
    /// Minimum edge threshold (0.0312 - 0.0833, default: 0.0312)
    /// Prevents AA on very low contrast edges
    pub edge_threshold_min: f32,
}

impl Default for FxaaConfig {
    fn default() -> Self {
        Self {
            enabled: false, // Disabled by default for performance
            edge_threshold: 0.125,
            edge_threshold_min: 0.0312,
        }
    }
}

impl FxaaConfig {
    /// Create config with FXAA enabled and default quality settings
    pub fn enabled() -> Self {
        Self {
            enabled: true,
            ..Default::default()
        }
    }

    /// High quality preset - more aggressive edge detection
    pub fn high_quality() -> Self {
        Self {
            enabled: true,
            edge_threshold: 0.063,
            edge_threshold_min: 0.0312,
        }
    }

    /// Balanced preset - default settings
    pub fn balanced() -> Self {
        Self {
            enabled: true,
            edge_threshold: 0.125,
            edge_threshold_min: 0.0312,
        }
    }

    /// Performance preset - less aggressive, faster
    pub fn performance() -> Self {
        Self {
            enabled: true,
            edge_threshold: 0.25,
            edge_threshold_min: 0.0625,
        }
    }
}

/// FXAA vertex shader - simple fullscreen quad pass-through
pub static FXAA_VERTEX_SHADER: &[u8] = b"#version 150

#if __VERSION__ != 100
    #define varying out
    #define attribute in
#endif

attribute vec2 vAttrXY;
varying vec2 vTexCoord;

void main() {
    vTexCoord = vAttrXY * 0.5 + 0.5; // Convert from [-1,1] to [0,1]
    gl_Position = vec4(vAttrXY, 0.0, 1.0);
}
";

/// FXAA fragment shader - implements edge-based anti-aliasing
pub static FXAA_FRAGMENT_SHADER: &[u8] = b"#version 150

precision highp float;

#if __VERSION__ == 100
    #define oFragColor gl_FragColor
    #define texture texture2D
#else
    out vec4 oFragColor;
#endif

#if __VERSION__ != 100
    #define varying in
#endif

uniform sampler2D uTexture;
uniform vec2 uTexelSize; // 1.0 / texture dimensions
uniform float uEdgeThreshold;
uniform float uEdgeThresholdMin;

varying vec2 vTexCoord;

// Luminance conversion (Rec. 709)
float luminance(vec3 color) {
    return dot(color, vec3(0.2126, 0.7152, 0.0722));
}

void main() {
    // Sample center and 4-neighborhood
    vec3 colorCenter = texture(uTexture, vTexCoord).rgb;
    vec3 colorN = texture(uTexture, vTexCoord + vec2(0.0, -uTexelSize.y)).rgb;
    vec3 colorS = texture(uTexture, vTexCoord + vec2(0.0, uTexelSize.y)).rgb;
    vec3 colorE = texture(uTexture, vTexCoord + vec2(uTexelSize.x, 0.0)).rgb;
    vec3 colorW = texture(uTexture, vTexCoord + vec2(-uTexelSize.x, 0.0)).rgb;
    
    // Calculate luminance
    float lumCenter = luminance(colorCenter);
    float lumN = luminance(colorN);
    float lumS = luminance(colorS);
    float lumE = luminance(colorE);
    float lumW = luminance(colorW);
    
    // Find min/max luminance in neighborhood
    float lumMin = min(lumCenter, min(min(lumN, lumS), min(lumE, lumW)));
    float lumMax = max(lumCenter, max(max(lumN, lumS), max(lumE, lumW)));
    float lumRange = lumMax - lumMin;
    
    // Early exit if no edge detected
    if (lumRange < max(uEdgeThresholdMin, lumMax * uEdgeThreshold)) {
        oFragColor = vec4(colorCenter, 1.0);
        return;
    }
    
    // Calculate edge direction
    float lumNS = lumN + lumS;
    float lumEW = lumE + lumW;
    
    vec2 dir;
    dir.x = lumNS - lumEW;
    dir.y = lumN - lumS;
    
    // Normalize edge direction
    float dirReduce = max((lumN + lumS + lumE + lumW) * 0.25 * 0.25, 0.0078125);
    float rcpDirMin = 1.0 / (min(abs(dir.x), abs(dir.y)) + dirReduce);
    dir = min(vec2(8.0), max(vec2(-8.0), dir * rcpDirMin)) * uTexelSize;
    
    // Sample along edge direction
    vec3 color1 = 0.5 * (
        texture(uTexture, vTexCoord + dir * (1.0/3.0 - 0.5)).rgb +
        texture(uTexture, vTexCoord + dir * (2.0/3.0 - 0.5)).rgb
    );
    
    vec3 color2 = color1 * 0.5 + 0.25 * (
        texture(uTexture, vTexCoord + dir * -0.5).rgb +
        texture(uTexture, vTexCoord + dir * 0.5).rgb
    );
    
    float lum2 = luminance(color2);
    
    // Choose appropriate sample based on luminance range
    if (lum2 < lumMin || lum2 > lumMax) {
        oFragColor = vec4(color1, 1.0);
    } else {
        oFragColor = vec4(color2, 1.0);
    }
}
";
