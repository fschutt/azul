//! `gdef` font table utilities.
//!
//! <https://docs.microsoft.com/en-us/typography/opentype/spec/gdef>

use crate::layout::GDEFTable;

pub const GLYPH_CLASS_NONE: u16 = 0;
pub const GLYPH_CLASS_BASE: u16 = 1;
pub const GLYPH_CLASS_LIGATURE: u16 = 2;
pub const GLYPH_CLASS_MARK: u16 = 3;
pub const GLYPH_CLASS_COMPONENT: u16 = 4;

pub fn gdef_is_mark(opt_gdef_table: Option<&GDEFTable>, glyph_index: u16) -> bool {
    glyph_class(opt_gdef_table, glyph_index) == GLYPH_CLASS_MARK
}

pub fn glyph_class(opt_gdef_table: Option<&GDEFTable>, glyph: u16) -> u16 {
    opt_gdef_table
        .and_then(|gdef| gdef.opt_glyph_classdef.as_ref())
        .map(|glyph_classdef| glyph_classdef.glyph_class_value(glyph))
        .unwrap_or(GLYPH_CLASS_NONE)
}

pub fn mark_attach_class(opt_gdef_table: Option<&GDEFTable>, glyph: u16) -> u16 {
    opt_gdef_table
        .and_then(|gdef| gdef.opt_mark_attach_classdef.as_ref())
        .map(|mark_attach_classdef| mark_attach_classdef.glyph_class_value(glyph))
        .unwrap_or(GLYPH_CLASS_NONE)
}

pub fn glyph_is_mark_in_set(opt_gdef_table: Option<&GDEFTable>, glyph: u16, index: usize) -> bool {
    gdef_is_mark(opt_gdef_table, glyph)
        && opt_gdef_table
            .and_then(|gdef| gdef.opt_mark_glyph_sets.as_ref())
            .and_then(|mark_glyph_sets| mark_glyph_sets.get(index))
            .is_some_and(|mark_set| mark_set.glyph_coverage_value(glyph).is_some())
}
