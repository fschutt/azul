#![deny(missing_docs)]

//! Utilities for accessing glyph information such as advance.

use std::borrow::Cow;
use std::collections::HashMap;
use std::sync::Arc;

use ouroboros::self_referencing;
use rustc_hash::FxHashMap;

use crate::binary::read::ReadScope;
use crate::error::ParseError;
use crate::font::Encoding;
use crate::macroman::macroman_to_char;
use crate::post::PostTable;
use crate::tables::cmap::CmapSubtable;
use crate::tables::{HheaTable, HmtxTable, MaxpTable};
use crate::GlyphId;

/// Retrieve glyph advance.
///
/// Since the `hhea` and `vhea` tables share the same format this function will return horizontal
/// or vertical advance depending on whether `hhea` or `vhea` is supplied to the `hhea` argument.
pub fn advance(
    maxp: &MaxpTable,
    hhea: &HheaTable,
    hmtx_data: &[u8],
    glyph: GlyphId,
) -> Result<u16, ParseError> {
    // Avoid parsing hmtx in this case
    if i32::from(glyph) > i32::from(maxp.num_glyphs) - 1 {
        return Ok(0);
    }

    let glyph = usize::from(glyph);
    let num_glyphs = usize::from(maxp.num_glyphs);
    let num_metrics = usize::from(hhea.num_h_metrics);
    let hmtx = ReadScope::new(hmtx_data).read_dep::<HmtxTable<'_>>((num_glyphs, num_metrics))?;

    if glyph < num_metrics {
        Ok(hmtx
            .h_metrics
            .get_item(glyph)
            .map_or(0, |x| x.advance_width))
    } else if num_metrics > 0 {
        Ok(hmtx
            .h_metrics
            .get_item(num_metrics - 1)
            .map_or(0, |metrics| metrics.advance_width))
    } else {
        Err(ParseError::BadIndex)
    }
}

#[self_referencing]
struct Post {
    data: Box<[u8]>,
    #[borrows(data)]
    #[not_covariant]
    table: PostTable<'this>,
}

/// Structure for looking up glyph names.
pub struct GlyphNames {
    post: Option<Post>,
    cmap: Option<CmapMappings>,
}

struct CmapMappings {
    encoding: Encoding,
    mappings: HashMap<u16, u32>,
}

impl GlyphNames {
    /// Construct a new `GlyphNames` instance.
    pub fn new(
        cmap_subtable: &Option<(Encoding, CmapSubtable<'_>)>,
        post_data: Option<Box<[u8]>>,
    ) -> Self {
        let post = post_data.and_then(|data| {
            PostTryBuilder {
                data,
                table_builder: |data| ReadScope::new(data).read::<PostTable<'_>>(),
            }
            .try_build()
            .ok()
        });
        let cmap = cmap_subtable
            .as_ref()
            .and_then(|(encoding, subtable)| CmapMappings::new(*encoding, subtable));
        GlyphNames { post, cmap }
    }

    /// Look up the name of `gid` in the `post` and `cmap` tables.
    pub fn glyph_name<'a>(&self, gid: GlyphId) -> Cow<'a, str> {
        // Glyph 0 is always .notdef
        if gid == 0 {
            return Cow::from(".notdef");
        }

        self.glyph_name_from_post(gid)
            .or_else(|| self.glyph_name_from_cmap(gid))
            .unwrap_or_else(|| Cow::from(format!("g{}", gid)))
    }

    /// Determine the set of unique glyph names for the supplied glyph ids.
    pub fn unique_glyph_names<'a>(&self, ids: &[GlyphId]) -> Vec<Cow<'a, str>> {
        unique_glyph_names(ids.iter().map(|&gid| self.glyph_name(gid)), ids.len())
    }

    fn glyph_name_from_post<'a>(&self, gid: GlyphId) -> Option<Cow<'a, str>> {
        let post = self.post.as_ref()?;
        post.glyph_name(gid)
    }

    fn glyph_name_from_cmap<'a>(&self, gid: GlyphId) -> Option<Cow<'a, str>> {
        let cmap = self.cmap.as_ref()?;
        cmap.glyph_name(gid)
    }
}

fn unique_glyph_names<'a>(
    names: impl Iterator<Item = Cow<'a, str>>,
    capacity: usize,
) -> Vec<Cow<'a, str>> {
    let mut seen = FxHashMap::with_capacity_and_hasher(capacity, Default::default());
    let mut unique_names = Vec::with_capacity(capacity);

    for name in names.map(Arc::new) {
        let alt = *seen
            .entry(Arc::clone(&name))
            .and_modify(|alt| *alt += 1)
            .or_insert(0);
        let unique_name = if alt == 0 {
            name
        } else {
            // name is not unique, generate a new name for it
            Arc::new(Cow::from(format!("{}.alt{:02}", name, alt)))
        };

        unique_names.push(unique_name)
    }
    drop(seen);

    // NOTE(unwrap): Safe as `seen` is the only other thing that holds a reference
    // to name and it's been dropped.
    unique_names
        .into_iter()
        .map(|name| Arc::try_unwrap(name).unwrap())
        .collect()
}

impl Post {
    fn glyph_name<'a>(&self, gid: GlyphId) -> Option<Cow<'a, str>> {
        self.with_table(|post: &PostTable<'_>| {
            match post.glyph_name(gid) {
                Ok(Some(glyph_name)) if glyph_name != ".notdef" => {
                    // Doesn't seem possible to avoid this allocation
                    Some(Cow::from(glyph_name.to_owned()))
                }
                _ => None,
            }
        })
    }
}

impl CmapMappings {
    fn new(encoding: Encoding, subtable: &CmapSubtable<'_>) -> Option<CmapMappings> {
        let mappings = subtable.mappings().ok()?;

        Some(CmapMappings { encoding, mappings })
    }

    fn glyph_name<'a>(&self, gid: GlyphId) -> Option<Cow<'a, str>> {
        let &ch = self.mappings.get(&gid)?;
        match self.encoding {
            Encoding::AppleRoman => glyph_names::glyph_name(macroman_to_unicode(ch)?),
            Encoding::Unicode => glyph_names::glyph_name(ch),
            Encoding::Symbol => None,
            Encoding::Big5 => None, // FIXME
        }
    }
}

fn macroman_to_unicode(ch: u32) -> Option<u32> {
    macroman_to_char(ch as u8).map(|ch| ch as u32)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_unique_glyph_names() {
        let names = vec!["A"; 3].into_iter().map(Cow::from);
        let unique_names = unique_glyph_names(names, 3);
        assert_eq!(
            unique_names,
            &[Cow::from("A"), Cow::from("A.alt01"), Cow::from("A.alt02")]
        );
    }
}
