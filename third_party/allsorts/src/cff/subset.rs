use std::mem;

use rustc_hash::{FxHashMap, FxHashSet};

use super::{
    owned, CFFFont, CFFVariant, CIDData, Charset, CustomCharset, DictDelta, FDSelect, Font,
    FontDict, MaybeOwnedIndex, Operand, Operator, ParseError, Range, ADOBE, CFF, IDENTITY,
    ISO_ADOBE_LAST_SID, OFFSET_ZERO, STANDARD_STRINGS,
};
use crate::binary::read::ReadArrayCow;
use crate::binary::write::{WriteBinaryDep, WriteBuffer};
use crate::subset::{SubsetError, SubsetGlyphs};

/// A subset CFF font.
pub struct SubsetCFF<'a> {
    table: CFF<'a>,
    new_to_old_id: Vec<u16>,
    old_to_new_id: FxHashMap<u16, u16>,
}

impl<'a> SubsetCFF<'a> {
    pub(crate) fn new(
        table: CFF<'a>,
        new_to_old_id: Vec<u16>,
        old_to_new_id: FxHashMap<u16, u16>,
    ) -> Self {
        SubsetCFF {
            table,
            new_to_old_id,
            old_to_new_id,
        }
    }
}

impl<'a> From<SubsetCFF<'a>> for CFF<'a> {
    fn from(subset: SubsetCFF<'a>) -> CFF<'a> {
        subset.table
    }
}

impl SubsetGlyphs for SubsetCFF<'_> {
    fn len(&self) -> usize {
        self.new_to_old_id.len()
    }

    fn old_id(&self, new_id: u16) -> u16 {
        self.new_to_old_id[usize::from(new_id)]
    }

    fn new_id(&self, old_id: u16) -> u16 {
        self.old_to_new_id.get(&old_id).copied().unwrap_or(0)
    }
}

impl<'a> CFF<'a> {
    /// Create a subset of this CFF table.
    ///
    /// - `glyph_ids` contains the ids of the glyphs to retain.
    ///
    /// When subsetting a Type 1 CFF font and retaining more than 255 glyphs the
    /// `convert_cff_to_cid_if_more_than_255_glyphs` argument controls whether the Type 1 font
    /// is converted to a CID keyed font in the process. The primary motivation for this is
    /// broader compatibility, especially if the subset font is embedded in a PDF.
    ///
    /// **Known Limitations**
    ///
    /// Currently the subsetting process does not produce the smallest possible output font.
    /// There are various parts of the source font that are copied to the output font as-is.
    /// Specifically the subsetting process does not subset the String INDEX.
    ///
    /// Subsetting the String INDEX requires updating all String IDs (SID) in the font so
    /// that they point at their new position in the String INDEX.
    pub fn subset(
        &'a self,
        glyph_ids: &[u16],
        convert_cff_to_cid_if_more_than_255_glyphs: bool,
    ) -> Result<SubsetCFF<'a>, SubsetError> {
        let mut cff = self.to_owned();
        let font: &mut Font<'_> = &mut cff.fonts[0];
        let mut charset = Vec::with_capacity(glyph_ids.len());
        let mut fd_select = Vec::with_capacity(glyph_ids.len());
        let mut new_to_old_id = Vec::with_capacity(glyph_ids.len());
        let mut old_to_new_id =
            FxHashMap::with_capacity_and_hasher(glyph_ids.len(), Default::default());
        let mut glyph_data = Vec::with_capacity(glyph_ids.len());
        let mut used_local_subrs = FxHashMap::default();
        let mut used_global_subrs = FxHashSet::default();

        for &glyph_id in glyph_ids {
            let char_string = font
                .char_strings_index
                .read_object(usize::from(glyph_id))
                .ok_or(ParseError::BadIndex)?;

            let subrs = super::charstring::char_string_used_subrs(
                CFFFont::CFF(font),
                &font.char_strings_index,
                &cff.global_subr_index,
                glyph_id,
            )?;
            used_global_subrs.extend(subrs.global_subr_used);
            if !subrs.local_subr_used.is_empty() {
                used_local_subrs.insert(glyph_id, subrs.local_subr_used);
            }

            glyph_data.push(char_string.to_owned());
            // Cast should be safe as there must be less than u16::MAX glyphs in a font
            old_to_new_id.insert(glyph_id, new_to_old_id.len() as u16);
            new_to_old_id.push(glyph_id);

            if glyph_id != 0 {
                let sid_or_cid = font
                    .charset
                    .id_for_glyph(glyph_id)
                    .ok_or(ParseError::BadIndex)?;
                charset.push(sid_or_cid);
            }

            // Calculate CID/Type 1 specific updates
            match &font.data {
                CFFVariant::CID(cid) => {
                    // Find out which font DICT this glyph maps to if it's a CID font
                    // Need to know which font DICT applies to each glyph, then ideally work out which FDSelect
                    // format is the best to use. For now it's probably good enough to just use format 0
                    let fd_index = cid
                        .fd_select
                        .font_dict_index(glyph_id)
                        .ok_or(ParseError::BadIndex)?;
                    fd_select.push(fd_index);
                }
                CFFVariant::Type1(_type1) => {}
            }
        }

        cff.global_subr_index =
            rebuild_global_subr_index(&cff.global_subr_index, used_global_subrs)?;
        font.char_strings_index = MaybeOwnedIndex::Owned(owned::Index { data: glyph_data });

        // Update CID/Type 1 specific structures
        match &mut font.data {
            CFFVariant::CID(cid) => {
                // Build new local_subr_indices
                cid.local_subr_indices = rebuild_local_subr_indices(cid, used_local_subrs)?;

                // Filter out Subr ops in the Private DICT if the local subr INDEX is None for
                // that DICT.
                filter_private_dict_subr_ops(cid);

                cid.fd_select = FDSelect::Format0 {
                    glyph_font_dict_indices: ReadArrayCow::Owned(fd_select),
                };
            }
            CFFVariant::Type1(type1) => {
                // Build new local_subr_index
                type1.local_subr_index = rebuild_type_1_local_subr_index(
                    type1.local_subr_index.as_ref(),
                    used_local_subrs,
                )?;

                // Filter out Subr ops in the Private DICT if the local subr INDEX is None.
                if type1.local_subr_index.is_none() {
                    type1
                        .private_dict
                        .dict
                        .retain(|(op, _)| *op != Operator::Subrs);
                }
            }
        }

        // Update the charset
        if font.is_cid_keyed() {
            font.charset = Charset::Custom(CustomCharset::Format0 {
                glyphs: ReadArrayCow::Owned(charset),
            });
        } else if convert_cff_to_cid_if_more_than_255_glyphs && font.char_strings_index.len() > 255
        {
            font.charset = convert_type1_to_cid(&mut cff.string_index, font)?;
        } else {
            let iso_adobe = 1..=ISO_ADOBE_LAST_SID;
            if charset
                .iter()
                .zip(iso_adobe)
                .all(|(sid, iso_adobe_sid)| *sid == iso_adobe_sid)
            {
                // As per section 18 of Technical Note #5176: There are no predefined charsets for
                // CID fonts. So this branch is only taken for Type 1 fonts.
                font.charset = Charset::ISOAdobe;
            } else {
                font.charset = Charset::Custom(CustomCharset::Format0 {
                    glyphs: ReadArrayCow::Owned(charset),
                });
            }
        }

        Ok(SubsetCFF {
            table: cff,
            new_to_old_id,
            old_to_new_id,
        })
    }
}

pub(crate) fn rebuild_global_subr_index(
    src_global_subr_index: &MaybeOwnedIndex<'_>,
    used_global_subrs: FxHashSet<usize>,
) -> Result<MaybeOwnedIndex<'static>, ParseError> {
    // Return a completely empty global subr index if there are no used global subrs
    if used_global_subrs.is_empty() {
        return Ok(MaybeOwnedIndex::Owned(owned::Index { data: Vec::new() }));
    }

    // Create a destination INDEX with the same number of entries as the source INDEX (see note
    // in rebuild_local_subr_indices)
    let mut dst_global_subr_index = owned::Index {
        data: vec![Vec::new(); src_global_subr_index.len()],
    };

    copy_used_subrs(
        used_global_subrs.iter().copied(),
        src_global_subr_index,
        &mut dst_global_subr_index,
    )?;

    Ok(MaybeOwnedIndex::Owned(dst_global_subr_index))
}

pub(crate) fn rebuild_local_subr_indices(
    cid: &CIDData<'_>,
    used_subrs_by_glyph: FxHashMap<u16, FxHashSet<usize>>,
) -> Result<Vec<Option<MaybeOwnedIndex<'static>>>, ParseError> {
    // Start off with all local subr indices as absent
    let mut indices = vec![None; cid.private_dicts.len()];

    for (glyph_id, used_subrs) in used_subrs_by_glyph {
        // For each glyph determine the index of the local subr index
        let index_of_local_subr_index = cid
            .fd_select
            .font_dict_index(glyph_id)
            .map(usize::from)
            .ok_or(ParseError::BadIndex)?;

        // Get the source Local Subr INDEX that we'll be copying from
        let src_local_subrs_index = match cid.local_subr_indices.get(index_of_local_subr_index) {
            Some(Some(index)) => Some(index),
            _ => None,
        }
        .ok_or(ParseError::BadIndex)?;

        // Get the Local Subr INDEX that we'll be copying to, if it doesn't exist then create it
        //
        // To avoid needing to rewrite all CharStrings to reference updated sub-routine
        // indices we instead fill the Local Subr INDEX with empty entries so that
        // indexes into it remain stable.
        //
        // An earlier iteration of this code only populated entries in the INDEX up to the largest
        // sub-routine index that was used. However this doesn't work because the operand to
        // callsubr is biased based on the number of entries in the INDEX, so for the existing char
        // strings to continue to work the same number of entries needs to be maintained. To do that
        // we fill it with empty entries.
        let dst_local_subr_index = match &mut indices[index_of_local_subr_index] {
            Some(index) => index,
            local_subr_index @ None => {
                *local_subr_index = Some(owned::Index {
                    data: vec![Vec::new(); src_local_subrs_index.len()],
                });
                local_subr_index.as_mut().unwrap() // NOTE(unwrap): safe as we set value above
            }
        };

        copy_used_subrs(
            used_subrs.iter().copied(),
            src_local_subrs_index,
            dst_local_subr_index,
        )?;
    }

    Ok(indices
        .into_iter()
        .map(|index| index.map(MaybeOwnedIndex::Owned))
        .collect())
}

fn copy_used_subrs(
    used_subrs: impl Iterator<Item = usize>,
    src_subrs_index: &MaybeOwnedIndex<'_>,
    dst_subr_index: &mut owned::Index,
) -> Result<(), ParseError> {
    // `used_subrs` contains the indexes of sub-routines in `src_subr_index` that need to be copied.
    // For each used subr we copy it to `dst_subr_index`.
    for subr_index in used_subrs {
        // Check to see if this sub-routine has already been copied to the INDEX. We do this
        // by checking if its length is greater than zero. A defined subroutine will have a
        // non-zero length as it must at least end with either an endchar or a return operator.
        if dst_subr_index
            .data
            .get(subr_index)
            .is_some_and(|subr| !subr.is_empty())
        {
            continue;
        }

        // Retrieve the Subr contents from the source Local Subr INDEX
        let char_string = src_subrs_index
            .read_object(subr_index)
            .ok_or(ParseError::BadIndex)?;

        // Now copy the Subr into the new index. I was curious about the efficiency of
        // extend_from_slice in this context but looking at the assembly it compiles down to
        // a call to memcpy.
        debug_assert_eq!(dst_subr_index.data[subr_index].len(), 0);
        dst_subr_index.data[subr_index].reserve_exact(char_string.len());
        dst_subr_index.data[subr_index].extend_from_slice(char_string);
    }
    Ok(())
}

pub(crate) fn rebuild_type_1_local_subr_index(
    src_local_subrs_index: Option<&MaybeOwnedIndex<'_>>,
    used_subrs_by_glyph: FxHashMap<u16, FxHashSet<usize>>,
) -> Result<Option<MaybeOwnedIndex<'static>>, ParseError> {
    if used_subrs_by_glyph.is_empty() {
        return Ok(None);
    }

    // Get the source Local Subr INDEX that we'll be copying from
    let src_local_subrs_index = src_local_subrs_index.ok_or(ParseError::BadIndex)?;

    // Create a destination INDEX with the same number of entries as the source INDEX (see note
    // in rebuild_local_subr_indices)
    let mut dst_local_subr_index = owned::Index {
        data: vec![Vec::new(); src_local_subrs_index.len()],
    };

    for used_subrs in used_subrs_by_glyph.values() {
        copy_used_subrs(
            used_subrs.iter().copied(),
            src_local_subrs_index,
            &mut dst_local_subr_index,
        )?;
    }

    Ok(Some(MaybeOwnedIndex::Owned(dst_local_subr_index)))
}

fn filter_private_dict_subr_ops(cid: &mut CIDData<'_>) {
    for (private_dict, local_subr_index) in cid
        .private_dicts
        .iter_mut()
        .zip(cid.local_subr_indices.iter())
    {
        if local_subr_index.is_none() {
            private_dict.dict.retain(|(op, _)| *op != Operator::Subrs);
        }
    }
}

fn convert_type1_to_cid<'a>(
    string_index: &mut MaybeOwnedIndex<'a>,
    font: &mut Font<'a>,
) -> Result<Charset<'a>, ParseError> {
    assert!(!font.is_cid_keyed());

    // Retrieve the SIDs of Adobe and Identity, adding them if they're not in the String INDEX
    // already.
    let (adobe_sid, identity_sid) = match (string_index.index(ADOBE), string_index.index(IDENTITY))
    {
        (Some(adobe_sid), Some(identity_sid)) => (adobe_sid, identity_sid),
        (Some(adobe_sid), None) => (adobe_sid, string_index.push(IDENTITY.to_owned())),
        (None, Some(identity_sid)) => (string_index.push(ADOBE.to_owned()), identity_sid),
        (None, None) => (
            string_index.push(ADOBE.to_owned()),
            string_index.push(IDENTITY.to_owned()),
        ),
    };

    // > the standard strings take SIDs in the range 0 to (nStdStrings –1). The first string in the
    // > String INDEX corresponds to the SID whose value is equal to nStdStrings, the first
    // > non-standard string
    let adobe_sid = adobe_sid + STANDARD_STRINGS.len();
    let identity_sid = identity_sid + STANDARD_STRINGS.len();

    // Build Font DICT
    let mut font_dict = FontDict::new();
    font_dict.inner_mut().push((
        Operator::Private,
        vec![Operand::Offset(0), Operand::Offset(0)],
    )); // Size and Offset will be updated when written out

    let mut font_dict_buffer = WriteBuffer::new();
    FontDict::write_dep(&mut font_dict_buffer, &font_dict, DictDelta::new())
        .map_err(|_err| ParseError::BadValue)?;
    let font_dict_index = MaybeOwnedIndex::Owned(owned::Index {
        data: vec![font_dict_buffer.into_inner()],
    });

    let n_glyphs = u16::try_from(font.char_strings_index.len())?;

    let fd_select = FDSelect::Format3 {
        ranges: ReadArrayCow::Owned(vec![Range {
            first: 0,
            n_left: 0,
        }]),
        sentinel: n_glyphs,
    };
    let cid_data = CFFVariant::CID(CIDData {
        font_dict_index,
        private_dicts: Vec::new(),
        local_subr_indices: Vec::new(),
        fd_select,
    });

    // Swap Type1 data with CID data
    let type1_data = match mem::replace(&mut font.data, cid_data) {
        CFFVariant::Type1(data) => data,
        CFFVariant::CID(_) => unreachable!(),
    };
    match &mut font.data {
        CFFVariant::Type1(_type1) => unreachable!(),
        CFFVariant::CID(cid) => {
            cid.private_dicts = vec![type1_data.private_dict];
            cid.local_subr_indices = vec![type1_data.local_subr_index];
        }
    };

    // Update the Top DICT
    // Add ROS
    let registry = Operand::Integer(i32::try_from(adobe_sid)?);
    let ordering = Operand::Integer(i32::try_from(identity_sid)?);
    let supplement = Operand::Integer(0);
    let ros = (Operator::ROS, vec![registry, ordering, supplement]);
    font.top_dict.inner_mut().insert(0, ros);

    // Add FDSelect and FDArray offsets to Top DICT
    // Actual offsets will be filled in when writing
    font.top_dict
        .inner_mut()
        .push((Operator::FDArray, OFFSET_ZERO.to_vec()));
    font.top_dict
        .inner_mut()
        .push((Operator::FDSelect, OFFSET_ZERO.to_vec()));

    // Add CIDCount
    font.top_dict.inner_mut().push((
        Operator::CIDCount,
        vec![Operand::Integer(i32::from(n_glyphs))],
    ));

    // Remove Private DICT offset and encoding
    font.top_dict.remove(Operator::Private);
    font.top_dict.remove(Operator::Encoding);

    // Add charset
    Ok(Charset::Custom(CustomCharset::Format2 {
        ranges: ReadArrayCow::Owned(vec![Range {
            first: 1,
            n_left: n_glyphs.checked_sub(2).ok_or(ParseError::BadIndex)?,
        }]),
    }))
}
