//! What the shader sources the engine compiles say about EXTERNAL images (video and camera frames, `samplerExternalOES`).
//!
//! The Android emulator's guest GLES encoder reads the source text of every shader for the words that name an external sampler and treats the samplers
//! it finds declared with them as external, without asking the preprocessor whether the lines are live: a shader of ordinary 2D textures that carries
//! them only in a branch its features skip (`shared.glsl` used to: `uniform samplerExternalOES sColor0;` under `#elif defined(WR_FEATURE_TEXTURE_EXTERNAL)`)
//! then has its `sColor0` moved to the external binding when the program's samplers are set, and every texture of its draws samples as nothing (a black
//! window on the emulator, 2026-11-28). The words belong in the shaders of external images and in no other.

use super::*;

/// The words that name an external image sampler, in any form (type, extension directive, extension name).
const EXTERNAL_WORDS: [&str; 3] = ["samplerExternal", "image_external", "YUV_target"];

/// The shader sources of the repository: `webrender/core/res/<name>.glsl`.
fn source_of(name: &str) -> Cow<'static, str> {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("core")
        .join("res")
        .join(format!("{name}.glsl"));
    Cow::Owned(shader_source_from_file(&path))
}

/// Every feature the engine can ask for on an OpenGL ES device (`renderer/shade.rs` sets them from what the driver says it supports).
fn gles_flags() -> ShaderFeatureFlags {
    ShaderFeatureFlags::GLES
        | ShaderFeatureFlags::ADVANCED_BLEND_EQUATION
        | ShaderFeatureFlags::DUAL_SOURCE_BLENDING
        | ShaderFeatureFlags::DITHERING
        | ShaderFeatureFlags::TEXTURE_EXTERNAL
        | ShaderFeatureFlags::TEXTURE_EXTERNAL_ESSL1
        | ShaderFeatureFlags::TEXTURE_EXTERNAL_BT709
        | ShaderFeatureFlags::DEBUG
}

/// The features of a configuration of `get_shader_features` (`"ALPHA_PASS,TEXTURE_2D"`).
fn features_of(config: &str) -> Vec<&str> {
    config.split(',').filter(|feature| !feature.is_empty()).collect()
}

#[test]
fn a_shader_that_does_not_sample_external_images_never_names_an_external_sampler() {
    let shaders = get_shader_features(gles_flags());
    let mut checked = 0;
    for (name, configs) in &shaders {
        for config in configs {
            let features = features_of(config);
            if features.iter().any(|feature| feature.starts_with("TEXTURE_EXTERNAL")) {
                continue;
            }
            let (vertex, fragment) = build_shader_strings(ShaderVersion::GlEs, &features, name, &source_of);
            for (kind, text) in [("vertex", &vertex), ("fragment", &fragment)] {
                for word in EXTERNAL_WORDS {
                    assert!(
                        !text.contains(word),
                        "the {kind} shader of {name} [{config}] carries `{word}`: a guest that scans the text for it makes the shader's samplers external"
                    );
                }
            }
            checked += 1;
        }
    }
    assert!(checked > 40, "only {checked} programs were looked at: the feature list is not what it was");
}

#[test]
fn a_shader_of_external_images_names_the_sampler_and_the_extension_in_its_prefix() {
    let (vertex, fragment) = build_shader_strings(ShaderVersion::GlEs, &["TEXTURE_EXTERNAL"], "composite", &source_of);
    for text in [&vertex, &fragment] {
        assert!(text.contains("#extension GL_OES_EGL_image_external_essl3 : require\n"));
        assert!(text.contains("#define WR_SAMPLER_EXTERNAL samplerExternalOES\n"));
        // The directive comes before any code, as GLSL ES wants it: nothing but `#` lines, comments and blanks above it.
        let above: Vec<&str> = text.split("#extension GL_OES_EGL_image_external_essl3").next().unwrap_or("").lines().collect();
        assert!(above.iter().all(|line| line.is_empty() || line.starts_with('#') || line.starts_with("//")), "{above:?}");
    }
    let (vertex, _) = build_shader_strings(ShaderVersion::GlEs, &["TEXTURE_EXTERNAL_ESSL1"], "composite", &source_of);
    assert!(vertex.starts_with("#version 100\n"));
    assert!(vertex.contains("#extension GL_OES_EGL_image_external : require\n"));
    assert!(vertex.contains("#define WR_SAMPLER_EXTERNAL samplerExternalOES\n"));
    let (vertex, _) = build_shader_strings(ShaderVersion::GlEs, &["TEXTURE_EXTERNAL_BT709"], "composite", &source_of);
    assert!(vertex.contains("#extension GL_EXT_YUV_target : require\n"));
    assert!(vertex.contains("#define WR_SAMPLER_EXTERNAL_YUV __samplerExternal2DY2YEXT\n"));
}

#[test]
fn the_prefix_of_a_shader_of_2d_textures_has_none_of_it() {
    let mut prefix = String::new();
    build_shader_prefix_string(ShaderVersion::GlEs, &["TEXTURE_2D", "ALPHA_PASS"], ShaderKind::Fragment, "brush_image", &mut |s| prefix.push_str(s));
    assert!(prefix.starts_with("#version 300 es\n"));
    assert!(prefix.contains("#define WR_FEATURE_TEXTURE_2D\n"));
    for word in EXTERNAL_WORDS {
        assert!(!prefix.contains(word), "{prefix}");
    }
}
