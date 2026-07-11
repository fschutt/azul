//! FXAA (Fast Approximate Anti-Aliasing) shader implementation.
//!
//! Post-processing AA that detects edges via luminance and selectively blurs them.
//! Faster than supersampling and works without hardware MSAA support.
//!
//! Shader compilation: `GlContextPtrInner.fxaa_shader` (see `core/src/gl.rs`).
//! FXAA pass: `apply_fxaa` / `apply_fxaa_with_config` (see `layout/src/xml/svg.rs`).
//!
//! Presets: `FxaaConfig::enabled()`, `::high_quality()`, `::balanced()`, `::performance()`

/// FXAA shader configuration
#[derive(Debug, Clone, Copy)]
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
    #[must_use] pub fn enabled() -> Self {
        Self {
            enabled: true,
            ..Default::default()
        }
    }

    /// High quality preset - more aggressive edge detection
    #[must_use] pub const fn high_quality() -> Self {
        Self {
            enabled: true,
            edge_threshold: 0.063,
            edge_threshold_min: 0.0312,
        }
    }

    /// Balanced preset - default settings
    #[must_use] pub const fn balanced() -> Self {
        Self {
            enabled: true,
            edge_threshold: 0.125,
            edge_threshold_min: 0.0312,
        }
    }

    /// Performance preset - less aggressive, faster
    #[must_use] pub const fn performance() -> Self {
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

#[cfg(test)]
mod autotest_generated {
    use super::*;

    /// Documented bounds from the `FxaaConfig::edge_threshold` doc comment.
    const EDGE_THRESHOLD_RANGE: (f32, f32) = (0.063, 0.333);
    /// Documented bounds from the `FxaaConfig::edge_threshold_min` doc comment.
    const EDGE_THRESHOLD_MIN_RANGE: (f32, f32) = (0.0312, 0.0833);

    /// `f32::abs` lives in `std`, and this crate is `no_std`-capable.
    fn fabs(x: f32) -> f32 {
        if x < 0.0 { -x } else { x }
    }

    /// `FxaaConfig` derives neither `PartialEq` nor `Eq`, so compare field-wise.
    fn same(a: FxaaConfig, b: FxaaConfig) -> bool {
        a.enabled == b.enabled
            && a.edge_threshold.to_bits() == b.edge_threshold.to_bits()
            && a.edge_threshold_min.to_bits() == b.edge_threshold_min.to_bits()
    }

    fn all_presets() -> [(&'static str, FxaaConfig); 5] {
        [
            ("default", FxaaConfig::default()),
            ("enabled", FxaaConfig::enabled()),
            ("high_quality", FxaaConfig::high_quality()),
            ("balanced", FxaaConfig::balanced()),
            ("performance", FxaaConfig::performance()),
        ]
    }

    fn find(haystack: &[u8], needle: &[u8]) -> Option<usize> {
        if needle.is_empty() || haystack.len() < needle.len() {
            return None;
        }
        (0..=haystack.len() - needle.len()).find(|&i| &haystack[i..i + needle.len()] == needle)
    }

    fn contains(haystack: &[u8], needle: &[u8]) -> bool {
        find(haystack, needle).is_some()
    }

    // ---------------------------------------------------------------------
    // Preset values: exact literals, documented ranges, ordering
    // ---------------------------------------------------------------------

    #[test]
    fn default_is_disabled_with_documented_defaults() {
        let d = FxaaConfig::default();
        assert!(!d.enabled, "FXAA must default to off for performance");
        assert_eq!(d.edge_threshold, 0.125_f32);
        assert_eq!(d.edge_threshold_min, 0.0312_f32);
    }

    #[test]
    fn enabled_is_default_with_only_the_flag_flipped() {
        // `enabled()` is spelled `..Default::default()`; this pins that it never
        // silently picks up a different quality preset if Default ever changes.
        let e = FxaaConfig::enabled();
        let d = FxaaConfig::default();
        assert!(e.enabled);
        assert_eq!(e.edge_threshold.to_bits(), d.edge_threshold.to_bits());
        assert_eq!(
            e.edge_threshold_min.to_bits(),
            d.edge_threshold_min.to_bits()
        );
    }

    #[test]
    fn balanced_matches_enabled_default() {
        // "Balanced preset - default settings" — the doc claims these are the same.
        assert!(
            same(FxaaConfig::balanced(), FxaaConfig::enabled()),
            "balanced() drifted away from the documented default settings"
        );
    }

    #[test]
    fn every_preset_except_default_is_enabled() {
        for (name, cfg) in all_presets() {
            if name == "default" {
                assert!(!cfg.enabled, "{name} should be disabled");
            } else {
                assert!(cfg.enabled, "{name}() must enable FXAA");
            }
        }
    }

    #[test]
    fn preset_thresholds_are_within_documented_ranges() {
        let (lo, hi) = EDGE_THRESHOLD_RANGE;
        let (min_lo, min_hi) = EDGE_THRESHOLD_MIN_RANGE;
        for (name, cfg) in all_presets() {
            assert!(
                cfg.edge_threshold >= lo && cfg.edge_threshold <= hi,
                "{name}: edge_threshold {} outside documented [{lo}, {hi}]",
                cfg.edge_threshold
            );
            assert!(
                cfg.edge_threshold_min >= min_lo && cfg.edge_threshold_min <= min_hi,
                "{name}: edge_threshold_min {} outside documented [{min_lo}, {min_hi}]",
                cfg.edge_threshold_min
            );
        }
    }

    #[test]
    fn preset_thresholds_are_finite_and_positive() {
        for (name, cfg) in all_presets() {
            assert!(!cfg.edge_threshold.is_nan(), "{name}: edge_threshold is NaN");
            assert!(
                cfg.edge_threshold.is_finite(),
                "{name}: edge_threshold is not finite"
            );
            assert!(
                !cfg.edge_threshold_min.is_nan(),
                "{name}: edge_threshold_min is NaN"
            );
            assert!(
                cfg.edge_threshold_min.is_finite(),
                "{name}: edge_threshold_min is not finite"
            );
            // A zero/negative threshold makes the shader's `lumRange < max(...)`
            // early-exit unreachable -> AA runs on every single fragment.
            assert!(
                cfg.edge_threshold > 0.0,
                "{name}: edge_threshold must be > 0"
            );
            assert!(
                cfg.edge_threshold_min > 0.0,
                "{name}: edge_threshold_min must be > 0"
            );
        }
    }

    #[test]
    fn min_threshold_never_exceeds_edge_threshold() {
        // The shader computes `max(uEdgeThresholdMin, lumMax * uEdgeThreshold)`;
        // with lumMax <= 1.0 the relative term can only ever win if
        // edge_threshold >= edge_threshold_min. If min > threshold, the relative
        // threshold is dead code for every possible luminance.
        for (name, cfg) in all_presets() {
            assert!(
                cfg.edge_threshold_min <= cfg.edge_threshold,
                "{name}: edge_threshold_min ({}) > edge_threshold ({}) makes the \
                 relative threshold unreachable",
                cfg.edge_threshold_min,
                cfg.edge_threshold
            );
        }
    }

    #[test]
    fn presets_are_ordered_by_aggressiveness() {
        // high_quality = most edges detected (lowest threshold),
        // performance = fewest (highest threshold), balanced in between.
        let hq = FxaaConfig::high_quality();
        let bal = FxaaConfig::balanced();
        let perf = FxaaConfig::performance();
        assert!(
            hq.edge_threshold < bal.edge_threshold,
            "high_quality must detect more edges than balanced"
        );
        assert!(
            bal.edge_threshold < perf.edge_threshold,
            "balanced must detect more edges than performance"
        );
        assert!(
            hq.edge_threshold_min <= bal.edge_threshold_min,
            "high_quality min-threshold must not exceed balanced's"
        );
        assert!(
            bal.edge_threshold_min <= perf.edge_threshold_min,
            "balanced min-threshold must not exceed performance's"
        );
    }

    #[test]
    fn const_presets_are_usable_in_const_context() {
        // The three `const fn` presets must stay const-evaluable: shader setup
        // sites may store them in statics.
        const HQ: FxaaConfig = FxaaConfig::high_quality();
        const BAL: FxaaConfig = FxaaConfig::balanced();
        const PERF: FxaaConfig = FxaaConfig::performance();
        assert!(same(HQ, FxaaConfig::high_quality()));
        assert!(same(BAL, FxaaConfig::balanced()));
        assert!(same(PERF, FxaaConfig::performance()));
    }

    #[test]
    fn presets_are_deterministic_across_calls() {
        for _ in 0..64 {
            assert!(same(FxaaConfig::enabled(), FxaaConfig::enabled()));
            assert!(same(FxaaConfig::high_quality(), FxaaConfig::high_quality()));
            assert!(same(FxaaConfig::balanced(), FxaaConfig::balanced()));
            assert!(same(FxaaConfig::performance(), FxaaConfig::performance()));
            assert!(same(FxaaConfig::default(), FxaaConfig::default()));
        }
    }

    #[test]
    fn config_is_copy_not_aliased() {
        let original = FxaaConfig::high_quality();
        let mut copy = original;
        copy.enabled = false;
        copy.edge_threshold = f32::NAN;
        copy.edge_threshold_min = f32::INFINITY;
        assert!(!copy.enabled);
        assert!(copy.edge_threshold.is_nan());
        // `original` must be untouched (Copy, no interior mutability / no heap).
        assert!(original.enabled);
        assert_eq!(original.edge_threshold, 0.063_f32);
        assert_eq!(original.edge_threshold_min, 0.0312_f32);
    }

    #[test]
    fn threshold_bits_round_trip_through_f32_repr() {
        // These values are uploaded verbatim as GL float uniforms; a bit-level
        // round-trip guards against any lossy re-encoding in between.
        for (name, cfg) in all_presets() {
            let t = f32::from_bits(cfg.edge_threshold.to_bits());
            let m = f32::from_bits(cfg.edge_threshold_min.to_bits());
            assert_eq!(t.to_bits(), cfg.edge_threshold.to_bits(), "{name}");
            assert_eq!(m.to_bits(), cfg.edge_threshold_min.to_bits(), "{name}");
        }
    }

    // ---------------------------------------------------------------------
    // The shader's threshold math, replayed in Rust against every preset
    // ---------------------------------------------------------------------

    /// Mirrors the shader's early-exit predicate:
    /// `lumRange < max(uEdgeThresholdMin, lumMax * uEdgeThreshold)`.
    fn shader_skips_aa(cfg: FxaaConfig, lum_min: f32, lum_max: f32) -> bool {
        let lum_range = lum_max - lum_min;
        let threshold = if cfg.edge_threshold_min > lum_max * cfg.edge_threshold {
            cfg.edge_threshold_min
        } else {
            lum_max * cfg.edge_threshold
        };
        lum_range < threshold
    }

    #[test]
    fn flat_regions_never_trigger_aa() {
        // Uniform luminance (lumRange == 0) must always take the early-exit path,
        // for every preset and across the whole legal luminance domain.
        for (name, cfg) in all_presets() {
            for step in 0..=32u32 {
                let lum = f32::from(step as u16) / 32.0;
                assert!(
                    shader_skips_aa(cfg, lum, lum),
                    "{name}: flat region at lum={lum} would be blurred"
                );
            }
        }
    }

    #[test]
    fn maximum_contrast_edge_always_triggers_aa() {
        // A pure black/white edge (lumRange == 1.0) must never be skipped:
        // that requires edge_threshold < 1.0 AND edge_threshold_min < 1.0.
        for (name, cfg) in all_presets() {
            assert!(
                !shader_skips_aa(cfg, 0.0, 1.0),
                "{name}: a full-contrast edge would be skipped by the shader"
            );
        }
    }

    #[test]
    fn dark_low_contrast_edges_are_gated_by_the_min_threshold() {
        // Near-black gradients: `lumMax * edge_threshold` collapses toward 0, so
        // only edge_threshold_min prevents AA-ing sensor noise. A tiny ramp well
        // below the min threshold must still be skipped.
        for (name, cfg) in all_presets() {
            let lum_min = 0.0;
            let lum_max = cfg.edge_threshold_min * 0.5;
            assert!(
                shader_skips_aa(cfg, lum_min, lum_max),
                "{name}: sub-min-threshold dark gradient (range {lum_max}) would be AA'd"
            );
        }
    }

    #[test]
    fn threshold_predicate_is_nan_safe() {
        // GL_RGBA16F / GL_RGBA32F render targets can legitimately carry NaN.
        // The predicate must not panic and must fall through to "no AA" (all
        // float comparisons against NaN are false, so lumRange < t is false).
        for (_, cfg) in all_presets() {
            let skipped = shader_skips_aa(cfg, f32::NAN, f32::NAN);
            assert!(!skipped, "NaN luminance must not take the flat-region path");
            let _ = shader_skips_aa(cfg, f32::NEG_INFINITY, f32::INFINITY);
            let _ = shader_skips_aa(cfg, f32::MIN, f32::MAX);
            let _ = shader_skips_aa(cfg, f32::MAX, f32::MIN);
        }
    }

    #[test]
    fn extreme_luminance_inputs_do_not_produce_nan_thresholds() {
        // lumMax * edge_threshold with a huge lumMax must stay finite-or-inf,
        // never NaN (NaN would silently disable the early exit).
        for (name, cfg) in all_presets() {
            for &lum_max in &[0.0_f32, 1.0, 1e30, f32::MAX, f32::MIN_POSITIVE] {
                let t = lum_max * cfg.edge_threshold;
                assert!(!t.is_nan(), "{name}: threshold went NaN at lumMax={lum_max}");
            }
        }
    }

    // ---------------------------------------------------------------------
    // Shader source bytes: what the GL driver actually receives
    // ---------------------------------------------------------------------

    #[test]
    fn shaders_are_non_empty_ascii_utf8() {
        for (name, src) in [
            ("vertex", FXAA_VERTEX_SHADER),
            ("fragment", FXAA_FRAGMENT_SHADER),
        ] {
            assert!(!src.is_empty(), "{name} shader is empty");
            let text = core::str::from_utf8(src)
                .unwrap_or_else(|e| panic!("{name} shader is not valid UTF-8: {e}"));
            // GLSL 1.50 source must be ASCII outside of comments; a stray
            // non-ASCII byte (e.g. a smart quote from an editor) is a hard
            // compile error on some drivers.
            assert!(
                text.is_ascii(),
                "{name} shader contains non-ASCII bytes (unicode smuggled into GLSL)"
            );
        }
    }

    #[test]
    fn shaders_contain_no_interior_nul_byte() {
        // These are handed to glShaderSource; an embedded NUL truncates the
        // source at the driver and yields a baffling "missing main()" error.
        for (name, src) in [
            ("vertex", FXAA_VERTEX_SHADER),
            ("fragment", FXAA_FRAGMENT_SHADER),
        ] {
            assert!(
                !src.contains(&0u8),
                "{name} shader contains an interior NUL byte"
            );
        }
    }

    #[test]
    fn version_directive_is_the_first_token() {
        // GLSL requires #version to precede everything but comments/whitespace.
        for (name, src) in [
            ("vertex", FXAA_VERTEX_SHADER),
            ("fragment", FXAA_FRAGMENT_SHADER),
        ] {
            let text = core::str::from_utf8(src).expect("utf8");
            assert!(
                text.starts_with("#version 150"),
                "{name} shader must open with `#version 150`, got: {:?}",
                &text[..text.len().min(24)]
            );
        }
    }

    #[test]
    fn shaders_have_balanced_braces_and_parens() {
        for (name, src) in [
            ("vertex", FXAA_VERTEX_SHADER),
            ("fragment", FXAA_FRAGMENT_SHADER),
        ] {
            let mut braces: i32 = 0;
            let mut parens: i32 = 0;
            for &b in src {
                match b {
                    b'{' => braces += 1,
                    b'}' => braces -= 1,
                    b'(' => parens += 1,
                    b')' => parens -= 1,
                    _ => {}
                }
                assert!(braces >= 0, "{name} shader closes a brace it never opened");
                assert!(parens >= 0, "{name} shader closes a paren it never opened");
            }
            assert_eq!(braces, 0, "{name} shader has unbalanced braces");
            assert_eq!(parens, 0, "{name} shader has unbalanced parens");
        }
    }

    #[test]
    fn shaders_define_main() {
        assert!(contains(FXAA_VERTEX_SHADER, b"void main()"));
        assert!(contains(FXAA_FRAGMENT_SHADER, b"void main()"));
    }

    #[test]
    fn fragment_shader_declares_every_uniform_the_host_feeds() {
        // These names are the contract with the GL pass in layout/src/xml/svg.rs;
        // renaming one of them here silently turns the uniform lookup into -1.
        for uniform in [
            &b"uniform sampler2D uTexture;"[..],
            &b"uniform vec2 uTexelSize;"[..],
            &b"uniform float uEdgeThreshold;"[..],
            &b"uniform float uEdgeThresholdMin;"[..],
        ] {
            assert!(
                contains(FXAA_FRAGMENT_SHADER, uniform),
                "fragment shader is missing uniform declaration: {}",
                core::str::from_utf8(uniform).unwrap()
            );
        }
    }

    #[test]
    fn vertex_and_fragment_varyings_match() {
        // vTexCoord is written by the vertex stage and read by the fragment
        // stage; a name mismatch is a link error at runtime only.
        assert!(contains(FXAA_VERTEX_SHADER, b"varying vec2 vTexCoord;"));
        assert!(contains(FXAA_FRAGMENT_SHADER, b"varying vec2 vTexCoord;"));
        assert!(contains(FXAA_VERTEX_SHADER, b"attribute vec2 vAttrXY;"));
        // The fragment stage must not redeclare the vertex attribute.
        assert!(!contains(FXAA_FRAGMENT_SHADER, b"attribute vec2 vAttrXY;"));
    }

    #[test]
    fn both_shaders_guard_the_es100_compatibility_defines() {
        // `#define varying out` (VS) / `#define varying in` (FS) must stay behind
        // a `__VERSION__ != 100` guard, or ES2 builds break.
        assert!(contains(FXAA_VERTEX_SHADER, b"#if __VERSION__ != 100"));
        assert!(contains(FXAA_VERTEX_SHADER, b"#define varying out"));
        assert!(contains(FXAA_FRAGMENT_SHADER, b"#if __VERSION__ != 100"));
        assert!(contains(FXAA_FRAGMENT_SHADER, b"#define varying in"));
        assert!(contains(FXAA_FRAGMENT_SHADER, b"#if __VERSION__ == 100"));
        assert!(contains(FXAA_FRAGMENT_SHADER, b"#define oFragColor gl_FragColor"));
    }

    #[test]
    fn luminance_weights_are_rec709_and_sum_to_one() {
        // Parse the literal `dot(color, vec3(...))` weights straight out of the
        // shader text. Weights that don't sum to 1.0 would darken/brighten the
        // edge-detection luminance and desync it from the thresholds above.
        let text = core::str::from_utf8(FXAA_FRAGMENT_SHADER).expect("utf8");
        let start = find(FXAA_FRAGMENT_SHADER, b"dot(color, vec3(")
            .expect("fragment shader must compute luminance via dot(color, vec3(..))")
            + b"dot(color, vec3(".len();
        let rest = &text[start..];
        let end = rest.find(')').expect("unterminated vec3(");
        let mut weights = [0.0_f32; 3];
        let mut count = 0usize;
        for part in rest[..end].split(',') {
            let v: f32 = part
                .trim()
                .parse()
                .unwrap_or_else(|_| panic!("non-numeric luminance weight: {part:?}"));
            assert!(
                count < 3,
                "luminance vec3 has more than 3 components: {:?}",
                &rest[..end]
            );
            weights[count] = v;
            count += 1;
        }
        assert_eq!(count, 3, "luminance vec3 must have exactly 3 components");

        // Rec. 709 coefficients.
        assert_eq!(weights[0], 0.2126_f32, "R weight is not Rec.709");
        assert_eq!(weights[1], 0.7152_f32, "G weight is not Rec.709");
        assert_eq!(weights[2], 0.0722_f32, "B weight is not Rec.709");

        let sum = weights[0] + weights[1] + weights[2];
        assert!(
            fabs(sum - 1.0) < 1e-6,
            "luminance weights sum to {sum}, not 1.0 — white would not map to lum 1.0"
        );

        // Consequence the thresholds rely on: luminance(white) == 1.0, so
        // lumMax is bounded by 1.0 for any LDR color, which is what makes
        // `lumMax * edge_threshold <= edge_threshold` hold.
        for (name, cfg) in all_presets() {
            assert!(
                sum * cfg.edge_threshold <= cfg.edge_threshold + 1e-6,
                "{name}: relative threshold can exceed edge_threshold for white"
            );
        }
    }

    #[test]
    fn fragment_shader_writes_the_output_on_every_path() {
        // Both the early-exit branch and the two edge branches must assign
        // oFragColor; an unwritten output is undefined-value garbage.
        let text = core::str::from_utf8(FXAA_FRAGMENT_SHADER).expect("utf8");
        let writes = text.matches("oFragColor =").count();
        assert!(
            writes >= 3,
            "expected >= 3 oFragColor writes (early-exit + 2 branches), found {writes}"
        );
    }

    #[test]
    fn shader_direction_clamp_is_symmetric() {
        // `min(vec2(8.0), max(vec2(-8.0), dir * rcpDirMin))` — the FXAA span
        // clamp must be symmetric, otherwise edges blur asymmetrically.
        assert!(contains(FXAA_FRAGMENT_SHADER, b"min(vec2(8.0), max(vec2(-8.0)"));
        // dirReduce must be clamped away from zero, or rcpDirMin divides by 0.
        assert!(contains(FXAA_FRAGMENT_SHADER, b"0.0078125"));
        assert!(contains(FXAA_FRAGMENT_SHADER, b"max("));
    }

    #[test]
    fn shader_dir_reduce_never_divides_by_zero() {
        // Replay `1.0 / (min(|dir.x|, |dir.y|) + dirReduce)` for the worst case:
        // an entirely black neighborhood, where every luminance is 0.
        let (lum_n, lum_s, lum_e, lum_w) = (0.0_f32, 0.0, 0.0, 0.0);
        let dir_x = (lum_n + lum_s) - (lum_e + lum_w);
        let dir_y = lum_n - lum_s;
        let dir_reduce = {
            let raw = (lum_n + lum_s + lum_e + lum_w) * 0.25 * 0.25;
            if raw > 0.007_812_5 { raw } else { 0.007_812_5 }
        };
        let min_abs = if fabs(dir_x) < fabs(dir_y) {
            fabs(dir_x)
        } else {
            fabs(dir_y)
        };
        let rcp = 1.0_f32 / (min_abs + dir_reduce);
        assert!(
            rcp.is_finite(),
            "rcpDirMin must stay finite even for an all-black neighborhood"
        );
        assert_eq!(rcp, 128.0_f32, "1.0 / 0.0078125 == 128.0");
    }
}
