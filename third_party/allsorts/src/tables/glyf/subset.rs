use rustc_hash::FxHashMap;

use super::{GlyfRecord, GlyfTable, Glyph, ParseError};
use crate::subset::SubsetGlyphs;
use crate::tables::glyf::CompositeGlyph;

/// A subset glyph
#[derive(Clone)]
pub struct SubsetGlyph<'a> {
    /// The old glyph id of this glyph
    pub old_id: u16,
    /// The glyph
    pub record: GlyfRecord<'a>,
}

/// A `glyf` table that has been subset.
#[derive(Clone)]
pub struct SubsetGlyf<'a> {
    glyphs: Vec<SubsetGlyph<'a>>,
    /// Maps an old glyph index to its index in the new table
    old_to_new_id: FxHashMap<u16, u16>,
}

impl<'a> GlyfTable<'a> {
    /// Returns a copy of this table that only contains the glyphs specified by `glyph_ids`.
    pub fn subset(&self, glyph_ids: &[u16]) -> Result<SubsetGlyf<'a>, ParseError> {
        let mut glyph_ids = glyph_ids.to_vec();
        let mut records = Vec::with_capacity(glyph_ids.len());

        let mut i = 0;
        while i < glyph_ids.len() {
            let glyph_id = glyph_ids[i];
            let mut record = self
                .records
                .get(usize::from(glyph_id))
                .ok_or(ParseError::BadIndex)?
                .clone();
            if record.is_composite() {
                record.parse()?;
                let GlyfRecord::Parsed(Glyph::Composite(composite)) = &mut record else {
                    unreachable!("not a composite glyph")
                };
                add_glyph(&mut glyph_ids, composite);
            }
            records.push(SubsetGlyph {
                old_id: glyph_id,
                record,
            });
            i += 1;
        }
        // Cast should be safe as there must be less than u16::MAX glyphs in a font
        let old_to_new_id = records
            .iter()
            .enumerate()
            .map(|(new_id, glyph)| (glyph.old_id, new_id as u16))
            .collect();
        Ok(SubsetGlyf {
            glyphs: records,
            old_to_new_id,
        })
    }
}

impl SubsetGlyphs for SubsetGlyf<'_> {
    fn len(&self) -> usize {
        self.glyphs.len()
    }

    fn old_id(&self, new_id: u16) -> u16 {
        self.glyphs[usize::from(new_id)].old_id
    }

    fn new_id(&self, old_id: u16) -> u16 {
        self.old_to_new_id.get(&old_id).copied().unwrap_or(0)
    }
}

impl<'a> From<SubsetGlyf<'a>> for GlyfTable<'a> {
    fn from(subset_glyphs: SubsetGlyf<'a>) -> Self {
        let records = subset_glyphs
            .glyphs
            .into_iter()
            .map(|subset_record| subset_record.record)
            .collect();

        GlyfTable { records }
    }
}

/// Add each of the child glyphs contained within a composite glyph to the subset font.
///
/// Updates the composite glyph indexes to point at the new child indexes.
fn add_glyph(glyph_ids: &mut Vec<u16>, composite: &mut CompositeGlyph) {
    for composite_glyph in composite.glyphs.iter_mut() {
        let new_id = glyph_ids
            .iter()
            .position(|&id| id == composite_glyph.glyph_index)
            .unwrap_or_else(|| {
                // Add this glyph to the list of ids to include in the subset font
                let new_id = glyph_ids.len();
                glyph_ids.push(composite_glyph.glyph_index);
                new_id
            });
        composite_glyph.glyph_index = new_id as u16;
    }
}
