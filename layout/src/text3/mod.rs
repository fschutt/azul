//! Text layout and shaping system.
//!
//! This module provides text shaping, inline layout, editing, selection,
//! and font caching. Submodules:
//!
//! - `cache`: unified text layout cache (`UnifiedLayout`)
//! - `default`: default text shaping and line-breaking
//! - `edit`: text editing operations
//! - `glyphs`: glyph positioning and cluster mapping
//! - `knuth_plass`: Knuth-Plass line-breaking algorithm
//! - `mock_fonts`: built-in test fonts with exactly known metrics
//! - `script`: Unicode script detection
//! - `selection`: text selection and cursor utilities

pub mod cache;
pub mod default;
pub mod edit;
pub mod glyphs;
pub mod knuth_plass;
pub mod mock_fonts;
pub mod script;
pub mod selection;
