use std::collections::HashMap;
use std::ffi::CStr;

// Declare modules
pub mod common;
pub mod traits;
pub mod types;
pub(crate) mod shaders;

use crate::shaders::{brush_image_texture_2d, brush_solid, brush_solid_alpha_pass};
use traits::Program;

/// The main factory function that creates a shader program instance.
/// It takes the shader name and a set of features (defines).
pub fn create_cpu_program(name: &str, features: &[&str]) -> Option<Box<dyn Program>> {
    let mut key = name.to_string();
    if !features.is_empty() {
        // Sort features for a canonical key
        let mut sorted_features = features.to_vec();
        sorted_features.sort_unstable();
        key.push(' ');
        key.push_str(&sorted_features.join(","));
    }

    match key.as_str() {
        "brush_solid" => Some(brush_solid::loader()),
        "brush_solid ALPHA_PASS" => Some(brush_solid_alpha_pass::loader()),
        "brush_image TEXTURE_2D" => Some(brush_image_texture_2d::loader()),
        // ... all other shaders will be added here
        _ => {
            // For now, we just return None for shaders not yet translated.
            // Eventually, this could panic for correctness.
            None
        }
    }
}