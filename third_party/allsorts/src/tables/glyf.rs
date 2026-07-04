#![deny(missing_docs)]

//! Parsing and writing of the `glyf` table.
//!
//! > This table contains information that describes the glyphs in the font in the TrueType outline
//! > format. Information regarding the rasterizer (scaler) refers to the TrueType rasterizer.
//!
//! — <https://docs.microsoft.com/en-us/typography/opentype/spec/glyf>

mod outline;
mod subset;
mod variation;

use std::mem;
use std::sync::Arc;

use bitflags::bitflags;
use log::warn;
use pathfinder_geometry::transform2d::Matrix2x2F;
use pathfinder_geometry::vector::Vector2F;
use rustc_hash::FxHashMap;

use crate::binary::read::{
    ReadBinary, ReadBinaryDep, ReadCtxt, ReadFrom, ReadScope, ReadUnchecked,
};
use crate::binary::write::{WriteBinary, WriteBinaryDep, WriteContext};
use crate::binary::{word_align, I16Be, U16Be, I8, U8};
use crate::error::{ParseError, WriteError};
use crate::tables::loca::{owned, LocaTable};
use crate::tables::os2::Os2;
use crate::tables::{
    read_and_box_table, F2Dot14, FontTableProvider, HeadTable, HheaTable, HmtxTable,
    IndexToLocFormat, MaxpTable,
};
use crate::{tag, SafeFrom};

pub use outline::{GlyfVisitorContext, VariableGlyfContext, VariableGlyfContextStore};
pub use subset::SubsetGlyph;

/// Recursion limit for nested composite glyphs
///
/// "There is no minimum nesting depth that must be supported" so we use the same value as Harfbuzz.
#[allow(unused)]
const COMPOSITE_GLYPH_RECURSION_LIMIT: u8 = 6;

bitflags! {
    /// Flags for [simple glyphs](https://learn.microsoft.com/en-us/typography/opentype/spec/glyf#simple-glyph-description)
    #[rustfmt::skip]
    #[derive(Debug, Clone, Copy, Eq, PartialEq)]
    pub struct SimpleGlyphFlag: u8 {
        #[allow(missing_docs)]
        const ON_CURVE_POINT                       = 0b00000001;
        #[allow(missing_docs)]
        const X_SHORT_VECTOR                       = 0b00000010;
        #[allow(missing_docs)]
        const Y_SHORT_VECTOR                       = 0b00000100;
        #[allow(missing_docs)]
        const REPEAT_FLAG                          = 0b00001000;
        #[allow(missing_docs)]
        const X_IS_SAME_OR_POSITIVE_X_SHORT_VECTOR = 0b00010000;
        #[allow(missing_docs)]
        const Y_IS_SAME_OR_POSITIVE_Y_SHORT_VECTOR = 0b00100000;
    }
}

bitflags! {
    /// Flags for [composite glyphs](https://learn.microsoft.com/en-us/typography/opentype/spec/glyf#composite-glyph-description)
    #[derive(Debug, Clone, Copy, Eq, PartialEq)]
    pub struct CompositeGlyphFlag: u16 {
        /// Bit 0: If this is set, the arguments are 16-bit (uint16 or int16); otherwise, they are
        /// bytes (uint8 or int8).
        const ARG_1_AND_2_ARE_WORDS = 0x0001;
        /// Bit 1: If this is set, the arguments are signed xy values; otherwise, they are unsigned
        /// point numbers.
        const ARGS_ARE_XY_VALUES = 0x0002;
        /// Bit 2: For the xy values if the preceding is true.
        const ROUND_XY_TO_GRID = 0x0004;
        /// Bit 3: This indicates that there is a simple scale for the component.
        ///
        /// Otherwise, scale = 1.0.
        const WE_HAVE_A_SCALE = 0x0008;
        /// Bit 4: Reserved, set to 0
        /// Bit 5: Indicates at least one more glyph after this one.
        const MORE_COMPONENTS = 0x0020;
        /// Bit 6: The x direction will use a different scale from the y direction.
        const WE_HAVE_AN_X_AND_Y_SCALE = 0x0040;
        /// Bit 7: There is a 2 by 2 transformation that will be used to scale the component.
        const WE_HAVE_A_TWO_BY_TWO = 0x0080;
        /// Bit 8: Following the last component are instructions for the composite character.
        const WE_HAVE_INSTRUCTIONS = 0x0100;
        /// Bit 9: If set, this forces the aw and lsb (and rsb) for the composite to be equal to
        /// those from this original glyph. This works for hinted and unhinted characters.
        const USE_MY_METRICS = 0x0200;
        /// Bit 10: If set, the components of the compound glyph overlap.
        ///
        /// Use of this flag is not required in OpenType — that is, it is valid to have components
        /// overlap without having this flag set. It may affect behaviors in some platforms,
        /// however. (See Apple’s specification for details regarding behavior in Apple platforms.)
        /// When used, it must be set on the flag word for the first component. See additional
        /// remarks, above, for the similar OVERLAP_SIMPLE flag used in simple-glyph descriptions.
        const OVERLAP_COMPOUND = 0x0400;
        /// Bit 11: The composite is designed to have the component offset scaled.
        const SCALED_COMPONENT_OFFSET = 0x0800;
        /// Bit 12: The composite is designed not to have the component offset scaled.
        const UNSCALED_COMPONENT_OFFSET = 0x1000;
        // 0xE010 	Reserved 	Bits 4, 13, 14 and 15 are reserved: set to 0.
    }
}

/// `glyf` table
///
/// This table contains glyph outlines. Functionality is provided for reading glyphs,
/// serializing to a `glyf` table, and subsetting.
///
/// **See also:** [LocaGlyf].<br>
/// **Reference:** <https://docs.microsoft.com/en-us/typography/opentype/spec/glyf>
#[derive(Debug, PartialEq)]
pub struct GlyfTable<'a> {
    records: Vec<GlyfRecord<'a>>,
}

/// Alternate representation of `glyf` table
///
/// This is an alternate structure for the `glyf` table that combines `glyf` and
/// `loca` data together. This makes it easier to access glyphs. `LocaGlyph` also
/// contains a glyph cache so repeated calls to [glyph][Self::glyph] will only
/// fetch and parse the glyph once.
///
/// `LocaGlyf` also implements [OutlineBuilder][crate::outline::OutlineBuilder],
/// which allows the outline of the glyph to be visited.
///
/// **Reference:** <https://docs.microsoft.com/en-us/typography/opentype/spec/glyf>
pub struct LocaGlyf {
    /// Flag that indicates whether this structure has been loaded.
    ///
    /// This allows and empty version of the table to be created, removing the need
    /// to wrap an unloaded table in Option or similar.
    loaded: bool,
    /// Data from `loca` table.
    loca: owned::LocaTable,
    /// Raw `glyf` table data (owned copy, or a zero-copy view into shared
    /// already-resident font bytes — see [`GlyfBytes`]).
    glyf: GlyfBytes,
    /// Cache of parsed glyphs indexed by glyph ID.
    cache: FxHashMap<u16, Arc<Glyph>>,
}

/// Backing storage for the `glyf` table.
///
/// The classic path copies the whole `glyf` table onto the heap
/// ([`GlyfBytes::Owned`]) — for a large font (e.g. a CJK or macOS system
/// `.ttc`) that is a ~20-40 MB allocation kept for the font's lifetime, even
/// though the source bytes are usually already resident (mmap'd) elsewhere.
/// [`GlyfBytes::Shared`] instead keeps an `Arc` to those source bytes plus the
/// `glyf` table's `(offset, len)`, so no copy is made. See
/// [`LocaGlyf::load_shared`].
pub enum GlyfBytes {
    /// Owned heap copy of the `glyf` table.
    Owned(Box<[u8]>),
    /// Zero-copy view: `owner.as_ref()[offset..offset + len]` is the `glyf`
    /// table. `owner` keeps the whole font buffer alive.
    Shared {
        /// The font buffer the `glyf` table lives inside; kept alive so the
        /// view stays valid.
        owner: Arc<dyn AsRef<[u8]> + Send + Sync>,
        /// Byte offset of the `glyf` table within `owner`.
        offset: usize,
        /// Length of the `glyf` table in bytes.
        len: usize,
    },
}

impl GlyfBytes {
    /// The `glyf` table bytes.
    #[inline]
    pub fn as_bytes(&self) -> &[u8] {
        match self {
            GlyfBytes::Owned(b) => b,
            GlyfBytes::Shared { owner, offset, len } => {
                // Bounds were validated at construction (`load_shared`); clamp
                // defensively so a bad range degrades to a short read (parse
                // error) rather than a panic.
                let all = (**owner).as_ref();
                let end = offset.saturating_add(*len).min(all.len());
                all.get(*offset..end).unwrap_or(&[])
            }
        }
    }
}

impl From<Box<[u8]>> for GlyfBytes {
    fn from(b: Box<[u8]>) -> Self {
        GlyfBytes::Owned(b)
    }
}

/// If `table` is a sub-slice that lives inside `owner`'s allocation, return its
/// `(offset, len)` within `owner`; otherwise `None` (the provider returned an
/// owned/relocated copy, so a zero-copy view is impossible). Uses address-range
/// containment — valid because a `Cow::Borrowed` from a table provider points
/// directly into the font buffer `owner` wraps.
fn glyf_within(table: &[u8], owner: &[u8]) -> Option<(usize, usize)> {
    let (base, obytes) = (owner.as_ptr() as usize, owner.len());
    let (tstart, tlen) = (table.as_ptr() as usize, table.len());
    let offset = tstart.checked_sub(base)?;
    // Must lie fully within owner. (A zero-length table has no address to
    // anchor and isn't worth sharing — fall back to Owned.)
    if tlen == 0 || offset.checked_add(tlen)? > obytes {
        return None;
    }
    Some((offset, tlen))
}

/// A record from the `glyf` table that maybe parsed
#[derive(Debug, PartialEq, Clone)]
pub enum GlyfRecord<'a> {
    /// An unparsed glyph
    Present {
        /// The number of contours of this glyph
        ///
        /// - Zero for empty glyphs
        /// - Negative for composite glyphs
        number_of_contours: i16,
        /// A scope for parsing the glyph
        scope: ReadScope<'a>,
    },
    /// A parsed glyph
    Parsed(Glyph),
}

/// Storage for [phantom points](https://learn.microsoft.com/en-us/typography/opentype/spec/tt_instructing_glyphs#phantoms)
pub type PhantomPoints = [Point; 4];

/// A single glyph
#[derive(Debug, PartialEq, Clone)]
pub enum Glyph {
    /// A glyph with no outline
    Empty(EmptyGlyph),
    /// A glyph with an outline
    Simple(SimpleGlyph),
    /// A glyph composed of other glyphs
    Composite(CompositeGlyph),
}

/// A glyph with no outline
#[derive(Debug, PartialEq, Clone)]
pub struct EmptyGlyph {
    /// The [phantom points](https://learn.microsoft.com/en-us/typography/opentype/spec/tt_instructing_glyphs#phantoms) of this glyph
    pub phantom_points: Option<PhantomPoints>,
}

/// A glyph with an outline
#[derive(Debug, PartialEq, Clone)]
pub struct SimpleGlyph {
    /// The bounding box of this glyph
    pub bounding_box: BoundingBox,
    /// The end points of the contours of this glyph
    ///
    /// Array of point indices for the last point of each contour, in increasing numeric order.
    pub end_pts_of_contours: Vec<u16>,
    /// Hinting instruction byte code
    pub instructions: Box<[u8]>,
    /// Contour point coordinates
    pub coordinates: Vec<(SimpleGlyphFlag, Point)>,
    /// Phantom points, only populated when applying glyph variation deltas
    pub phantom_points: Option<Box<PhantomPoints>>,
}

/// A glyph composed from other glyphs
#[derive(Debug, PartialEq, Clone)]
pub struct CompositeGlyph {
    /// The bounding box of this glyph
    pub bounding_box: BoundingBox,
    /// The glyph components
    pub glyphs: Vec<CompositeGlyphComponent>,
    /// Hinting instruction byte code
    pub instructions: Box<[u8]>,
    /// Phantom points, only populated when applying glyph variation deltas
    pub phantom_points: Option<Box<PhantomPoints>>,
}

/// A component of a [CompositeGlyph]
#[derive(Debug, PartialEq, Clone)]
pub struct CompositeGlyphComponent {
    /// Flags for this component
    pub flags: CompositeGlyphFlag,
    /// The index of the child glyph for this component
    pub glyph_index: u16,
    /// First argument
    ///
    /// Meaning depends on `flags`
    pub argument1: CompositeGlyphArgument,
    /// Second argument
    ///
    /// Meaning depends on `flags`
    pub argument2: CompositeGlyphArgument,
    /// Optional scale applied to this child component
    pub scale: Option<CompositeGlyphScale>,
}

/// Variable size composite glyph argument
#[allow(missing_docs)]
#[derive(Debug, PartialEq, Copy, Clone)]
pub enum CompositeGlyphArgument {
    U8(u8),
    I8(i8),
    U16(u16),
    I16(i16),
}

/// A scale applied to a [CompositeGlyphComponent]
#[derive(Debug, PartialEq, Copy, Clone)]
pub enum CompositeGlyphScale {
    /// Simple scale for the component
    Scale(F2Dot14),
    /// Separate X and Y scales
    XY {
        /// The X scale
        x_scale: F2Dot14,
        /// The Y scale
        y_scale: F2Dot14,
    },
    /// A 2 by 2 transformation that will be used to scale the component
    Matrix([[F2Dot14; 2]; 2]),
}

/// Wrapper for composite glyph components
pub struct CompositeGlyphs {
    /// The child components
    pub glyphs: Vec<CompositeGlyphComponent>,
    /// Flag indicating if there are hinting instructions for this glyph
    pub have_instructions: bool,
}

/// An (x, y) point
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Point(pub i16, pub i16);

/// Glyph bounding box
#[derive(Debug, PartialEq, Copy, Clone)]
pub struct BoundingBox {
    /// X minimum
    pub x_min: i16,
    /// X maximum
    pub x_max: i16,
    /// Y minimum
    pub y_min: i16,
    /// Y maximum
    pub y_max: i16,
}

impl ReadBinaryDep for GlyfTable<'_> {
    type Args<'a> = &'a LocaTable<'a>;
    type HostType<'a> = GlyfTable<'a>;

    fn read_dep<'a>(
        ctxt: &mut ReadCtxt<'a>,
        loca: Self::Args<'a>,
    ) -> Result<Self::HostType<'a>, ParseError> {
        if loca.offsets.len() < 2 {
            return Err(ParseError::BadIndex);
        }

        let glyph_records = (0..loca.offsets.len() - 1)
            .map(|i| {
                // NOTE(unwrap): Bounded by `loca.offsets.len() - 1`, both
                // indices are guaranteed in range.
                let start = loca.offsets.get(i).unwrap();
                let end = loca.offsets.get(i + 1).unwrap();
                (start, end)
            })
            .map(|(start, end)| match end.checked_sub(start) {
                Some(0) => Ok(GlyfRecord::empty()),
                Some(length) => {
                    let offset = usize::try_from(start)?;
                    let glyph_scope = ctxt.scope().offset_length(offset, usize::try_from(length)?);
                    match glyph_scope {
                        Ok(scope) => {
                            let number_of_contours = scope.read::<I16Be>()?;
                            Ok(GlyfRecord::Present {
                                number_of_contours,
                                scope,
                            })
                        }
                        Err(ParseError::BadEof) => {
                            // The length specified by `loca` is beyond the end of the `glyf`
                            // table. Try parsing the glyph without a length limit to see if it's
                            // valid. This is a workaround for a font where the last `loca` offset
                            // was incorrectly 1 byte beyond the end of the `glyf` table but the
                            // actual glyph data was valid.
                            warn!("glyph length out of bounds, trying to parse");
                            let scope = ctxt.scope().offset(offset);
                            scope.read::<Glyph>().map(GlyfRecord::Parsed)
                        }
                        Err(err) => Err(err),
                    }
                }
                None => Err(ParseError::BadOffset),
            })
            .collect::<Result<Vec<_>, _>>()?;

        Ok(GlyfTable {
            records: glyph_records,
        })
    }
}

impl<'a> WriteBinaryDep<Self> for GlyfTable<'a> {
    type Output = owned::LocaTable;
    type Args = IndexToLocFormat;

    /// Write this glyf table into `ctxt`.
    ///
    /// ## A Note About Padding
    ///
    /// On the [loca table documentation](https://docs.microsoft.com/en-us/typography/opentype/spec/loca#long-version)
    /// at the bottom it states:
    ///
    /// > Note that the local offsets should be 32-bit aligned. Offsets which are not 32-bit
    /// > aligned may seriously degrade performance of some processors.
    ///
    /// On the [Recommendations for OpenType Fonts](https://docs.microsoft.com/en-us/typography/opentype/spec/recom#loca-table)
    /// page it states:
    ///
    /// > We recommend that local offsets should be 16-bit aligned, in both the short and long
    /// > formats of this table.
    ///
    /// On [Apple's loca documentation](https://developer.apple.com/fonts/TrueType-Reference-Manual/RM06/Chap6loca.html)
    /// it says:
    ///
    /// > The glyph data is always word aligned.
    ///
    /// Elsewhere in the Apple docs they refer to long as 32-bits, so assuming word here means
    /// 16-bits.
    ///
    /// [An issue](https://github.com/MicrosoftDocs/typography-issues/issues/241) was raised against
    /// Microsoft's docs regarding this.
    /// Behdad Esfahbod [commented](https://github.com/MicrosoftDocs/typography-issues/issues/241#issuecomment-495265379):
    ///
    /// > All the requirements should be removed since 2019.
    /// >
    /// > In reality, in the short format, you are forced to do 16-bit alignment because of how
    /// > offsets are stored. In the long format, use alignment 1. We've been doing that in
    /// > fonttools for years and never ever heard a complaint whatsoever.
    ///
    /// So with this in mind we implement 16-bit alignment when `index_to_loc_format` is 0,
    /// and no alignment/padding otherwise.
    fn write_dep<C: WriteContext>(
        ctxt: &mut C,
        table: GlyfTable<'a>,
        index_to_loc_format: IndexToLocFormat,
    ) -> Result<Self::Output, WriteError> {
        let mut offsets: Vec<u32> = Vec::with_capacity(table.records.len() + 1);

        let start = ctxt.bytes_written();
        for record in table.records {
            let offset = ctxt.bytes_written();

            offsets.push(u32::try_from(ctxt.bytes_written() - start)?);

            match record {
                GlyfRecord::Present { scope, .. } => ReadScope::write(ctxt, scope)?,
                GlyfRecord::Parsed(glyph) => Glyph::write(ctxt, glyph)?,
            }

            if index_to_loc_format == IndexToLocFormat::Short {
                let length = ctxt.bytes_written() - offset;
                let padded_length = word_align(length);
                ctxt.write_zeros(padded_length - length)?;
            }
        }

        // Add the final loca entry
        offsets.push(u32::try_from(ctxt.bytes_written() - start)?);

        Ok(owned::LocaTable { offsets })
    }
}

impl Glyph {
    /// Construct a new empty glyph
    pub fn empty() -> Glyph {
        Glyph::Empty(EmptyGlyph::new())
    }

    /// The number of contours of this glyph
    ///
    /// - Zero for empty glyphs
    /// - Negative for composite glyphs
    pub fn number_of_contours(&self) -> i16 {
        match self {
            Glyph::Empty(_) => 0,
            Glyph::Simple(simple) => simple.number_of_contours(),
            Glyph::Composite(_) => -1,
        }
    }

    /// Returns the bounding box of the glyph.
    ///
    /// Returns `None` if the glyph is an [EmptyGlyph].
    pub fn bounding_box(&self) -> Option<BoundingBox> {
        match self {
            Glyph::Empty(_) => None,
            Glyph::Simple(simple) => Some(simple.bounding_box),
            Glyph::Composite(composite) => Some(composite.bounding_box),
        }
    }

    /// Returns the phantom points of the glyph.
    ///
    /// Returns `None` if the phantom points have not been assigned through glyph variation.
    pub fn phantom_points(&self) -> Option<PhantomPoints> {
        match self {
            Glyph::Empty(empty) => empty.phantom_points,
            Glyph::Simple(SimpleGlyph { phantom_points, .. })
            | Glyph::Composite(CompositeGlyph { phantom_points, .. }) => {
                phantom_points.as_deref().copied()
            }
        }
    }

    /// The number of delta adjustable points in this glyph excluding phantom points.
    fn number_of_points(&self) -> Result<u16, ParseError> {
        match self {
            Glyph::Empty(_) => Ok(0),
            Glyph::Simple(glyph) => Ok(glyph.coordinates.len().try_into()?),
            Glyph::Composite(composite) => Ok(composite.glyphs.len().try_into()?),
        }
    }
}

/// Calculate the phantom points from the glyph.
///
/// Requires that the bounding box of the glyph is accurate/up-to-date.
pub(crate) fn calculate_phantom_points(
    glyph_id: u16,
    bounding_box: Option<BoundingBox>,
    hmtx: &HmtxTable<'_>,
    vmtx: Option<&HmtxTable<'_>>,
    os2: Option<&Os2>,
    hhea: &HheaTable,
) -> Result<[Point; 4], ParseError> {
    // In a font with TrueType outlines, xMin and xMax values for each glyph are given in the
    // 'glyf' table. The advance width (“aw”) and left side bearing (“lsb”) can be derived from
    // the glyph “phantom points”.
    //
    // If pp1 and pp2 are TrueType phantom points used to control lsb and rsb, their initial
    // position in the X-direction is calculated as follows:
    //
    // pp1 = xMin - lsb
    // pp2 = pp1 + aw
    //
    // If a glyph has no contours, xMax/xMin are not defined. The left side bearing indicated
    // in the 'hmtx' table for such glyphs should be zero.
    //
    // https://learn.microsoft.com/en-us/typography/opentype/spec/hmtx
    //
    // See also notes in FreeType:
    // https://gitlab.freedesktop.org/freetype/freetype/-/blob/7d45cf2c8f219263c5b9d84763a9a101138b0ed1/src/truetype/ttgload.c#L1280-1363
    let horizonal_metrics = hmtx.metric(glyph_id)?;
    let x_min = bounding_box.map(|bbox| bbox.x_min).unwrap_or(0);
    let y_max = bounding_box.map(|bbox| bbox.y_max).unwrap_or(0);
    let pp1 = Point(x_min - horizonal_metrics.lsb, 0);
    let pp2 = Point(pp1.0 + i16::try_from(horizonal_metrics.advance_width)?, 0);

    let (advance_height, tsb) = match vmtx {
        Some(vmtx) => vmtx.metric(glyph_id).and_then(|metric| {
            i16::try_from(metric.advance_width)
                .map(|aw| (aw, metric.lsb))
                .map_err(|_| ParseError::LimitExceeded)
        })?,
        // Fall back on OS/2 table if vmtx table is not present
        None => {
            let (default_ascender, default_descender) =
                match os2.and_then(|os2| os2.version0.as_ref()) {
                    Some(os2) => (os2.s_typo_ascender, os2.s_typo_descender),
                    None => (hhea.ascender, hhea.descender),
                };
            let advance_height = default_ascender - default_descender;
            let tsb = default_ascender - y_max;
            (advance_height, tsb)
        }
    };

    let x = 0;
    let pp3 = Point(x, y_max + tsb);
    let pp4 = Point(x, pp3.1 - advance_height);

    Ok([pp1, pp2, pp3, pp4])
}

impl ReadBinary for Glyph {
    type HostType<'a> = Glyph;

    fn read<'a>(ctxt: &mut ReadCtxt<'a>) -> Result<Self::HostType<'a>, ParseError> {
        let number_of_contours = ctxt.read_i16be()?;

        if number_of_contours >= 0 {
            // Simple glyph
            // Cast is safe as we've checked value is positive above
            let glyph = ctxt.read_dep::<SimpleGlyph>(number_of_contours as u16)?;
            Ok(Glyph::Simple(glyph))
        } else {
            // Composite glyph
            let glyph = ctxt.read::<CompositeGlyph>()?;
            Ok(Glyph::Composite(glyph))
        }
    }
}

impl WriteBinary for Glyph {
    type Output = ();

    fn write<C: WriteContext>(ctxt: &mut C, glyph: Glyph) -> Result<(), WriteError> {
        match glyph {
            Glyph::Empty(_) => Ok(()),
            Glyph::Simple(simple_glyph) => SimpleGlyph::write(ctxt, simple_glyph),
            Glyph::Composite(composite) => CompositeGlyph::write(ctxt, composite),
        }
    }
}

impl SimpleGlyph {
    /// The number of contours in this glyph
    pub fn number_of_contours(&self) -> i16 {
        // TODO: Revisit this to see how we might enforce its validity
        // In theory there could be more than i16::MAX items in end_pts_of_contours
        self.end_pts_of_contours.len() as i16
    }

    /// Iterator over the contours of this glyph
    pub fn contours(&self) -> impl Iterator<Item = &[(SimpleGlyphFlag, Point)]> {
        self.end_pts_of_contours.iter().scan(0, move |i, &end| {
            let start = *i;
            let end = usize::from(end);
            *i = end + 1;
            self.coordinates.get(start..=end)
        })
    }

    /// The bounding box of this glyph
    pub fn bounding_box(&self) -> BoundingBox {
        BoundingBox::from_points(self.coordinates.iter().copied().map(|(_flag, point)| point))
    }
}

impl ReadBinaryDep for SimpleGlyph {
    type Args<'a> = u16;
    type HostType<'a> = SimpleGlyph;

    fn read_dep<'a>(
        ctxt: &mut ReadCtxt<'a>,
        number_of_contours: Self::Args<'_>,
    ) -> Result<Self::HostType<'a>, ParseError> {
        let number_of_contours = usize::from(number_of_contours);
        let bounding_box = ctxt.read::<BoundingBox>()?;
        let end_pts_of_contours = ctxt.read_array::<U16Be>(number_of_contours)?.to_vec();
        let instruction_length = ctxt.read::<U16Be>()?;
        let instructions = ctxt.read_slice(usize::from(instruction_length))?;
        // end_pts_of_contours stores the index of the end points.
        // Therefore the number of coordinates is the last index + 1
        let number_of_coordinates = end_pts_of_contours
            .last()
            .map_or(0, |&last| usize::from(last) + 1);

        // Read all the flags
        let mut coordinates = Vec::with_capacity(number_of_coordinates);
        while coordinates.len() < number_of_coordinates {
            let flag = ctxt.read::<SimpleGlyphFlag>()?;
            if flag.is_repeated() {
                let count = usize::from(ctxt.read::<U8>()?) + 1; // + 1 to include the current entry
                let repeat = std::iter::repeat_n((flag, Point::zero()), count);
                coordinates.extend(repeat)
            } else {
                coordinates.push((flag, Point::zero()));
            }
        }

        // Read the x coordinates
        for (flag, Point(x, _y)) in coordinates.iter_mut() {
            *x = if flag.x_is_short() {
                ctxt.read::<U8>()
                    .map(|val| i16::from(val) * flag.x_short_sign())?
            } else if flag.x_is_same_or_positive() {
                0
            } else {
                ctxt.read::<I16Be>()?
            }
        }

        // Read y coordinates, updating the Points in `coordinates`
        let mut prev_point = Point::zero();
        for (flag, point) in coordinates.iter_mut() {
            let y = if flag.y_is_short() {
                ctxt.read::<U8>()
                    .map(|val| i16::from(val) * flag.y_short_sign())?
            } else if flag.y_is_same_or_positive() {
                0
            } else {
                ctxt.read::<I16Be>()?
            };

            // The x and y coordinates are stored as deltas against the previous point, with the
            // first one being implicitly against (0, 0). Here we resolve these deltas into
            // absolute (x, y) values.
            prev_point = Point(prev_point.0 + point.0, prev_point.1 + y);
            *point = prev_point
        }

        Ok(SimpleGlyph {
            bounding_box,
            end_pts_of_contours,
            instructions: Box::from(instructions),
            coordinates,
            phantom_points: None,
        })
    }
}

impl WriteBinary for SimpleGlyph {
    type Output = ();

    fn write<C: WriteContext>(ctxt: &mut C, glyph: SimpleGlyph) -> Result<(), WriteError> {
        I16Be::write(ctxt, glyph.number_of_contours())?;
        BoundingBox::write(ctxt, glyph.bounding_box)?;
        ctxt.write_vec::<U16Be, _>(glyph.end_pts_of_contours)?;
        U16Be::write(ctxt, u16::try_from(glyph.instructions.len())?)?;
        ctxt.write_bytes(&glyph.instructions)?;

        // Flags and coordinates are written without any attempt to compact them using
        // smaller representation, use of REPEAT, or X/Y_IS_SAME.
        // TODO: try to compact the values written

        // flags
        let mask = SimpleGlyphFlag::ON_CURVE_POINT; // ON_CURVE_POINT is the only flag that needs to carry through
        for flag in glyph.coordinates.iter().map(|(flag, _)| *flag) {
            U8::write(ctxt, (flag & mask).bits())?;
        }

        // x coordinates
        let mut prev_x = 0;
        for (_, Point(x, _)) in &glyph.coordinates {
            let delta_x = x - prev_x;
            I16Be::write(ctxt, delta_x)?;
            prev_x = *x;
        }

        // y coordinates
        let mut prev_y = 0;
        for (_, Point(_, y)) in &glyph.coordinates {
            let delta_y = y - prev_y;
            I16Be::write(ctxt, delta_y)?;
            prev_y = *y;
        }

        Ok(())
    }
}

impl ReadFrom for SimpleGlyphFlag {
    type ReadType = U8;

    fn read_from(flag: u8) -> Self {
        SimpleGlyphFlag::from_bits_truncate(flag)
    }
}

impl ReadBinary for CompositeGlyphs {
    type HostType<'a> = Self;

    fn read(ctxt: &mut ReadCtxt<'_>) -> Result<Self, ParseError> {
        let mut have_instructions = false;
        let mut glyphs = Vec::new();
        loop {
            let flags = ctxt.read::<CompositeGlyphFlag>()?;
            let data = ctxt.read_dep::<CompositeGlyphComponent>(flags)?;

            if flags.we_have_instructions() {
                have_instructions = true;
            }

            glyphs.push(data);

            if !flags.more_components() {
                break;
            }
        }

        Ok(CompositeGlyphs {
            glyphs,
            have_instructions,
        })
    }
}

impl ReadBinary for CompositeGlyph {
    type HostType<'a> = CompositeGlyph;

    fn read<'a>(ctxt: &mut ReadCtxt<'a>) -> Result<Self::HostType<'a>, ParseError> {
        let bounding_box = ctxt.read::<BoundingBox>()?;
        let glyphs = ctxt.read::<CompositeGlyphs>()?;

        let instruction_length = if glyphs.have_instructions {
            usize::from(ctxt.read::<U16Be>()?)
        } else {
            0
        };
        let instructions = ctxt.read_slice(instruction_length)?;

        Ok(CompositeGlyph {
            bounding_box,
            glyphs: glyphs.glyphs,
            instructions: Box::from(instructions),
            phantom_points: None,
        })
    }
}

impl WriteBinary for CompositeGlyph {
    type Output = ();

    fn write<C: WriteContext>(ctxt: &mut C, composite: Self) -> Result<Self::Output, WriteError> {
        I16Be::write(ctxt, -1_i16)?; // number_of_contours
        BoundingBox::write(ctxt, composite.bounding_box)?;
        let mut has_instructions = false;
        for glyph in composite.glyphs {
            has_instructions |= glyph.flags.we_have_instructions();
            CompositeGlyphComponent::write(ctxt, glyph)?;
        }
        if has_instructions {
            U16Be::write(ctxt, u16::try_from(composite.instructions.len())?)?;
            ctxt.write_bytes(&composite.instructions)?;
        }
        Ok(())
    }
}

#[allow(missing_docs)]
impl SimpleGlyphFlag {
    pub fn is_on_curve(self) -> bool {
        self & Self::ON_CURVE_POINT == Self::ON_CURVE_POINT
    }

    pub fn x_is_short(self) -> bool {
        self & Self::X_SHORT_VECTOR == Self::X_SHORT_VECTOR
    }

    pub fn y_is_short(self) -> bool {
        self & Self::Y_SHORT_VECTOR == Self::Y_SHORT_VECTOR
    }

    pub fn is_repeated(self) -> bool {
        self & Self::REPEAT_FLAG == Self::REPEAT_FLAG
    }

    pub fn x_short_sign(self) -> i16 {
        if self.x_is_same_or_positive() {
            1
        } else {
            -1
        }
    }

    pub fn y_short_sign(self) -> i16 {
        if self.y_is_same_or_positive() {
            1
        } else {
            -1
        }
    }

    pub fn x_is_same_or_positive(self) -> bool {
        self & Self::X_IS_SAME_OR_POSITIVE_X_SHORT_VECTOR
            == Self::X_IS_SAME_OR_POSITIVE_X_SHORT_VECTOR
    }

    pub fn y_is_same_or_positive(self) -> bool {
        self & Self::Y_IS_SAME_OR_POSITIVE_Y_SHORT_VECTOR
            == Self::Y_IS_SAME_OR_POSITIVE_Y_SHORT_VECTOR
    }
}

impl ReadFrom for CompositeGlyphFlag {
    type ReadType = U16Be;

    fn read_from(flag: u16) -> Self {
        CompositeGlyphFlag::from_bits_truncate(flag)
    }
}

impl ReadBinaryDep for CompositeGlyphArgument {
    type Args<'a> = CompositeGlyphFlag;
    type HostType<'a> = Self;

    fn read_dep(ctxt: &mut ReadCtxt<'_>, flags: Self::Args<'_>) -> Result<Self, ParseError> {
        let arg = match (flags.arg_1_and_2_are_words(), flags.args_are_xy_values()) {
            (true, true) => CompositeGlyphArgument::I16(ctxt.read_i16be()?),
            (true, false) => CompositeGlyphArgument::U16(ctxt.read_u16be()?),
            (false, true) => CompositeGlyphArgument::I8(ctxt.read_i8()?),
            (false, false) => CompositeGlyphArgument::U8(ctxt.read_u8()?),
        };

        Ok(arg)
    }
}

impl WriteBinary for CompositeGlyphArgument {
    type Output = ();

    fn write<C: WriteContext>(ctxt: &mut C, arg: CompositeGlyphArgument) -> Result<(), WriteError> {
        match arg {
            CompositeGlyphArgument::U8(val) => U8::write(ctxt, val),
            CompositeGlyphArgument::I8(val) => I8::write(ctxt, val),
            CompositeGlyphArgument::U16(val) => U16Be::write(ctxt, val),
            CompositeGlyphArgument::I16(val) => I16Be::write(ctxt, val),
        }
    }
}

impl ReadBinaryDep for CompositeGlyphComponent {
    type Args<'a> = CompositeGlyphFlag;
    type HostType<'a> = Self;

    fn read_dep(ctxt: &mut ReadCtxt<'_>, flags: Self::Args<'_>) -> Result<Self, ParseError> {
        let glyph_index = ctxt.read_u16be()?;
        let argument1 = ctxt.read_dep::<CompositeGlyphArgument>(flags)?;
        let argument2 = ctxt.read_dep::<CompositeGlyphArgument>(flags)?;

        let scale = if flags.we_have_a_scale() {
            Some(CompositeGlyphScale::Scale(ctxt.read::<F2Dot14>()?))
        } else if flags.we_have_an_x_and_y_scale() {
            Some(CompositeGlyphScale::XY {
                x_scale: ctxt.read::<F2Dot14>()?,
                y_scale: ctxt.read::<F2Dot14>()?,
            })
        } else if flags.we_have_a_two_by_two() {
            Some(CompositeGlyphScale::Matrix([
                [ctxt.read::<F2Dot14>()?, ctxt.read::<F2Dot14>()?],
                [ctxt.read::<F2Dot14>()?, ctxt.read::<F2Dot14>()?],
            ]))
        } else {
            None
        };

        Ok(CompositeGlyphComponent {
            flags,
            glyph_index,
            argument1,
            argument2,
            scale,
        })
    }
}

impl WriteBinary for CompositeGlyphComponent {
    type Output = ();

    fn write<C: WriteContext>(
        ctxt: &mut C,
        glyph: CompositeGlyphComponent,
    ) -> Result<(), WriteError> {
        U16Be::write(ctxt, glyph.flags.bits())?;
        U16Be::write(ctxt, glyph.glyph_index)?;
        CompositeGlyphArgument::write(ctxt, glyph.argument1)?;
        CompositeGlyphArgument::write(ctxt, glyph.argument2)?;
        if let Some(scale) = glyph.scale {
            CompositeGlyphScale::write(ctxt, scale)?;
        }
        Ok(())
    }
}

impl WriteBinary for CompositeGlyphScale {
    type Output = ();

    fn write<C: WriteContext>(ctxt: &mut C, scale: CompositeGlyphScale) -> Result<(), WriteError> {
        match scale {
            CompositeGlyphScale::Scale(scale) => F2Dot14::write(ctxt, scale)?,
            CompositeGlyphScale::XY { x_scale, y_scale } => {
                F2Dot14::write(ctxt, x_scale)?;
                F2Dot14::write(ctxt, y_scale)?;
            }
            CompositeGlyphScale::Matrix(matrix) => {
                F2Dot14::write(ctxt, matrix[0][0])?;
                F2Dot14::write(ctxt, matrix[0][1])?;
                F2Dot14::write(ctxt, matrix[1][0])?;
                F2Dot14::write(ctxt, matrix[1][1])?;
            }
        }

        Ok(())
    }
}

impl ReadFrom for BoundingBox {
    type ReadType = ((I16Be, I16Be), (I16Be, I16Be));

    fn read_from(((x_min, y_min), (x_max, y_max)): ((i16, i16), (i16, i16))) -> Self {
        BoundingBox {
            x_min,
            y_min,
            x_max,
            y_max,
        }
    }
}

impl WriteBinary for BoundingBox {
    type Output = ();

    fn write<C: WriteContext>(ctxt: &mut C, bbox: BoundingBox) -> Result<(), WriteError> {
        I16Be::write(ctxt, bbox.x_min)?;
        I16Be::write(ctxt, bbox.y_min)?;
        I16Be::write(ctxt, bbox.x_max)?;
        I16Be::write(ctxt, bbox.y_max)?;
        Ok(())
    }
}

impl<'a> GlyfTable<'a> {
    /// Construct a glyph table from the supplied glyphs
    pub fn new(records: Vec<GlyfRecord<'a>>) -> Result<Self, ParseError> {
        if records.len() > usize::from(u16::MAX) {
            return Err(ParseError::LimitExceeded);
        }
        Ok(GlyfTable { records })
    }

    /// Returns the number of glyphs in this `glyf` table.
    ///
    /// Returns a `u16` for convenience of interacting with other parts of the code.
    pub fn num_glyphs(&self) -> u16 {
        // NOTE(cast): Safe as we check records length in `new` and `push`.
        self.records.len() as u16
    }

    /// The glyphs in this `glyf` table
    pub fn records(&self) -> &[GlyfRecord<'a>] {
        &self.records
    }

    /// Mutable access to the glyphs of this table
    pub fn records_mut(&mut self) -> &mut [GlyfRecord<'a>] {
        &mut self.records
    }

    /// Append a new glyph to this glyph table
    ///
    /// If the maximum number of glyphs is reached `ParseError::LimitExceeded` is returned.
    pub fn push(&mut self, record: GlyfRecord<'a>) -> Result<(), ParseError> {
        if self.num_glyphs() < u16::MAX {
            self.records.push(record);
            Ok(())
        } else {
            Err(ParseError::LimitExceeded)
        }
    }

    /// Returns a parsed glyph, converting [GlyfRecord::Present] into [GlyfRecord::Parsed] if
    /// necessary.
    pub fn get_parsed_glyph(&mut self, glyph_index: u16) -> Result<&Glyph, ParseError> {
        let record = self
            .records
            .get_mut(usize::from(glyph_index))
            .ok_or(ParseError::BadIndex)?;
        record.parse()?;
        match record {
            GlyfRecord::Parsed(glyph) => Ok(glyph),
            GlyfRecord::Present { .. } => unreachable!("glyph should be parsed"),
        }
    }

    /// Takes the glyph at `glyph_index` out of the table replacing it with `GlyphRecord::Empty`
    /// and returns it.
    ///
    /// Returns `None` if the index is out-of-bounds.
    pub(crate) fn take(&mut self, glyph_index: u16) -> Option<GlyfRecord<'a>> {
        let target = self.records.get_mut(usize::from(glyph_index))?;
        Some(mem::replace(target, GlyfRecord::empty()))
    }

    /// Replaces the glyph at `glyph_index` with the supplied `GlyfRecord`.
    ///
    /// Returns an error if the index is out-of-bounds.
    pub(crate) fn replace(
        &mut self,
        glyph_index: u16,
        record: GlyfRecord<'a>,
    ) -> Result<(), ParseError> {
        let target = self
            .records
            .get_mut(usize::from(glyph_index))
            .ok_or(ParseError::BadIndex)?;
        *target = record;
        Ok(())
    }
}

impl LocaGlyf {
    /// Construct an unloaded LocaGlyf structure
    ///
    /// Attempts to read glyphs when the type is in this state will fail.
    pub fn new() -> Self {
        LocaGlyf {
            loaded: false,
            loca: owned::LocaTable::new(),
            glyf: GlyfBytes::Owned(Box::default()),
            cache: FxHashMap::default(),
        }
    }

    /// Load tables from the supplied FontTableProvider
    pub fn load<F: FontTableProvider>(provider: &F) -> Result<Self, ParseError> {
        let head = ReadScope::new(&provider.read_table_data(tag::HEAD)?).read::<HeadTable>()?;
        let maxp = ReadScope::new(&provider.read_table_data(tag::MAXP)?).read::<MaxpTable>()?;
        let loca_data = provider.read_table_data(tag::LOCA)?;
        let loca = ReadScope::new(&loca_data)
            .read_dep::<LocaTable<'_>>((maxp.num_glyphs, head.index_to_loc_format))?;
        let loca = owned::LocaTable::from(&loca);
        let glyf = read_and_box_table(provider, tag::GLYF)?;
        Ok(LocaGlyf {
            loaded: true,
            loca,
            glyf: GlyfBytes::Owned(glyf),
            cache: FxHashMap::default(),
        })
    }

    /// Load like [`load`][Self::load], but keep the `glyf` table as a
    /// **zero-copy** view into `owner` instead of copying it onto the heap.
    ///
    /// `owner` must be the exact byte buffer the `provider` reads its tables
    /// from (typically an `Arc` over the mmap'd/owned font file). The `glyf`
    /// table is located within `owner` by matching the provider's borrowed
    /// slice against `owner`'s address range; if the provider returns an
    /// *owned* `glyf` (e.g. a WOFF-decompressed table) or the range can't be
    /// validated, this transparently falls back to an owned copy — so the
    /// result is always correct, only sometimes not zero-copy.
    ///
    /// Saves a per-font `glyf`-sized allocation (tens of MB for large CJK /
    /// system fonts). See scripts/RELEASE_SIZE_MEMORY_AUDIT_2026_07_04.md §3.3a.
    pub fn load_shared<F: FontTableProvider>(
        provider: &F,
        owner: Arc<dyn AsRef<[u8]> + Send + Sync>,
    ) -> Result<Self, ParseError> {
        let head = ReadScope::new(&provider.read_table_data(tag::HEAD)?).read::<HeadTable>()?;
        let maxp = ReadScope::new(&provider.read_table_data(tag::MAXP)?).read::<MaxpTable>()?;
        let loca_data = provider.read_table_data(tag::LOCA)?;
        let loca = ReadScope::new(&loca_data)
            .read_dep::<LocaTable<'_>>((maxp.num_glyphs, head.index_to_loc_format))?;
        let loca = owned::LocaTable::from(&loca);

        let glyf_cow = provider.read_table_data(tag::GLYF)?;
        let glyf = match glyf_within(&glyf_cow, (*owner).as_ref()) {
            // The provider handed back a slice that lives inside `owner` — take
            // a zero-copy view. `owner` keeps the buffer alive.
            Some((offset, len)) => GlyfBytes::Shared { owner, offset, len },
            // Owned/relocated table: fall back to a copy (still correct).
            None => GlyfBytes::Owned(Box::from(glyf_cow.into_owned())),
        };
        Ok(LocaGlyf {
            loaded: true,
            loca,
            glyf,
            cache: FxHashMap::default(),
        })
    }

    /// Construct a loaded LocaGlyf structure from the supplied `loca` and `glyf` tables.
    ///
    /// [owned::LocaTable] can be constructed by
    /// parsing a `loca` table and then converting it to the owned version with
    /// [owned::LocaTable::from][crate::tables::loca::owned::LocaTable::from].
    pub fn loaded(loca: owned::LocaTable, glyf: Box<[u8]>) -> Self {
        LocaGlyf {
            loaded: true,
            loca,
            glyf: GlyfBytes::Owned(glyf),
            cache: FxHashMap::default(),
        }
    }

    /// Returns true if this is a loaded instance.
    pub fn is_loaded(&self) -> bool {
        self.loaded
    }

    /// Look up the glyph at the supplied index
    pub fn glyph(&mut self, index: u16) -> Result<Arc<Glyph>, ParseError> {
        if let Some(glyph) = self.cache.get(&index) {
            return Ok(Arc::clone(glyph));
        }

        // Get the start and end offsets for the glyph
        let start = self
            .loca
            .offsets
            .get(usize::from(index))
            .copied()
            .ok_or(ParseError::BadOffset)
            .map(usize::safe_from)?;

        // The end is clamped to the length of the glyf table. This is a workaround for a font where
        // the last `loca` offset was incorrectly 1 byte beyond the end of the `glyf` table but the
        // actual glyph data did not extend beyond the table.
        let end = self
            .loca
            .offsets
            .get(
                index
                    .checked_add(1)
                    .ok_or(ParseError::LimitExceeded)
                    .map(usize::from)?,
            )
            .copied()
            .ok_or(ParseError::BadOffset)
            .map(usize::safe_from)?
            .min(self.glyf.as_bytes().len());

        // Fetch the slice for the glyph
        let glyph_data = self
            .glyf
            .as_bytes()
            .get(start..end)
            .ok_or(ParseError::BadOffset)?;

        // If the slice is empty, then this is a valid, but empty glyph
        let glyph = if glyph_data.is_empty() {
            Arc::new(Glyph::empty())
        } else {
            ReadScope::new(glyph_data).read::<Glyph>().map(Arc::new)?
        };
        self.cache.insert(index, Arc::clone(&glyph));
        Ok(glyph)
    }
}

impl GlyfRecord<'_> {
    /// Construct an empty glyph record
    pub fn empty() -> Self {
        GlyfRecord::Parsed(Glyph::empty())
    }

    /// The number of contours of this glyph
    ///
    /// - Zero for empty glyphs
    /// - Negative for composite glyphs
    pub fn number_of_contours(&self) -> i16 {
        match self {
            GlyfRecord::Present {
                number_of_contours, ..
            } => *number_of_contours,
            GlyfRecord::Parsed(glyph) => glyph.number_of_contours(),
        }
    }

    /// The number of delta adjustable points in this glyph record excluding phantom points.
    pub fn number_of_points(&self) -> Result<u16, ParseError> {
        // The `maxp` table contains fields:
        //
        // * maxPoints            Maximum points in a non-composite glyph.
        // * maxCompositePoints   Maximum points in a composite glyph.
        //
        // Both of which are u16 so that's what we return here.
        match self {
            GlyfRecord::Present {
                scope,
                number_of_contours,
            } => {
                let mut ctxt = scope.ctxt();
                // skip glyph header: number_of_contours and the bounding box
                let _skip = ctxt.read_slice(U16Be::SIZE + BoundingBox::SIZE)?;
                if *number_of_contours >= 0 {
                    // Simple glyph
                    let end_pts_of_contours =
                        ctxt.read_array::<U16Be>(*number_of_contours as usize)?;
                    // end_pts_of_contours stores the index of the end points.
                    // Therefore the number of coordinates is the last index + 1
                    match end_pts_of_contours.last() {
                        Some(last) => last.checked_add(1).ok_or(ParseError::LimitExceeded),
                        None => Ok(0),
                    }
                } else {
                    // Composite glyph
                    let mut count = 0;
                    loop {
                        let flags = ctxt.read::<CompositeGlyphFlag>()?;
                        let _composite_glyph = ctxt.read_dep::<CompositeGlyphComponent>(flags)?;
                        count += 1;
                        if !flags.more_components() {
                            break;
                        }
                    }
                    Ok(count)
                }
            }
            GlyfRecord::Parsed(glyph) => glyph.number_of_points(),
        }
    }

    /// True if this is a composite glyph
    pub fn is_composite(&self) -> bool {
        self.number_of_contours() < 0
    }

    /// Turn self from GlyfRecord::Present into GlyfRecord::Parsed
    pub fn parse(&mut self) -> Result<(), ParseError> {
        if let GlyfRecord::Present { scope, .. } = self {
            *self = scope.read::<Glyph>().map(GlyfRecord::Parsed)?;
        }
        Ok(())
    }
}

impl<'a> From<SimpleGlyph> for GlyfRecord<'a> {
    fn from(glyph: SimpleGlyph) -> GlyfRecord<'a> {
        GlyfRecord::Parsed(Glyph::Simple(glyph))
    }
}

impl<'a> From<CompositeGlyph> for GlyfRecord<'a> {
    fn from(glyph: CompositeGlyph) -> GlyfRecord<'a> {
        GlyfRecord::Parsed(Glyph::Composite(glyph))
    }
}

impl EmptyGlyph {
    /// Construct a new empty glyph
    pub fn new() -> Self {
        EmptyGlyph {
            phantom_points: None,
        }
    }
}

#[allow(missing_docs)]
impl CompositeGlyphFlag {
    pub fn arg_1_and_2_are_words(self) -> bool {
        self & Self::ARG_1_AND_2_ARE_WORDS == Self::ARG_1_AND_2_ARE_WORDS
    }

    pub fn args_are_xy_values(self) -> bool {
        self & Self::ARGS_ARE_XY_VALUES == Self::ARGS_ARE_XY_VALUES
    }

    pub fn we_have_a_scale(self) -> bool {
        self & Self::WE_HAVE_A_SCALE == Self::WE_HAVE_A_SCALE
    }

    pub fn we_have_an_x_and_y_scale(self) -> bool {
        self & Self::WE_HAVE_AN_X_AND_Y_SCALE == Self::WE_HAVE_AN_X_AND_Y_SCALE
    }

    pub fn we_have_a_two_by_two(self) -> bool {
        self & Self::WE_HAVE_A_TWO_BY_TWO == Self::WE_HAVE_A_TWO_BY_TWO
    }

    pub fn more_components(self) -> bool {
        self & Self::MORE_COMPONENTS == Self::MORE_COMPONENTS
    }

    pub fn we_have_instructions(self) -> bool {
        self & Self::WE_HAVE_INSTRUCTIONS == Self::WE_HAVE_INSTRUCTIONS
    }

    pub fn component_offsets(self) -> ComponentOffsets {
        // The SCALED_COMPONENT_OFFSET and UNSCALED_COMPONENT_OFFSET flags are used to determine
        // how x and y offset values are to be interpreted when the component glyph is scaled. If
        // the SCALED_COMPONENT_OFFSET flag is set, then the x and y offset values are deemed to be
        // in the component glyph’s coordinate system, and the scale transformation is applied to
        // both values.
        //
        // If the UNSCALED_COMPONENT_OFFSET flag is set, then the x and y offset values are deemed
        // to be in the current glyph’s coordinate system, and the scale transformation is not
        // applied to either value.
        //
        // If neither flag is set, then the rasterizer will apply a default behavior. On Microsoft
        // and Apple platforms, the default behavior is the same as when the
        // UNSCALED_COMPONENT_OFFSET flag is set; this behavior is recommended for all rasterizer
        // implementations. If a font has both flags set, this is invalid; the rasterizer should use
        // its default behavior for this case.
        let scaled = self & Self::SCALED_COMPONENT_OFFSET == Self::SCALED_COMPONENT_OFFSET;
        let unscaled = self & Self::UNSCALED_COMPONENT_OFFSET == Self::UNSCALED_COMPONENT_OFFSET;
        match (scaled, unscaled) {
            (true, false) => ComponentOffsets::Scaled,
            (false, true) => ComponentOffsets::Unscaled,
            // Default for neither or both set
            (true, true) | (false, false) => ComponentOffsets::Unscaled,
        }
    }
}

/// Flag indicating whether the offsets in a composite glyph component are scaled or not
#[derive(Copy, Clone, Eq, PartialEq)]
pub enum ComponentOffsets {
    /// Offsets are scaled
    Scaled,
    /// Offsets are not scaled
    Unscaled,
}

impl Point {
    /// A point at (0, 0)
    pub fn zero() -> Self {
        Point(0, 0)
    }
}

impl BoundingBox {
    /// Contruct a new, empty bounding box
    pub fn empty() -> Self {
        BoundingBox {
            x_min: 0,
            x_max: 0,
            y_min: 0,
            y_max: 0,
        }
    }

    /// Calculate xMin, xMax and yMin, yMax from a collection of `Points`
    ///
    /// Panics if `points` is empty.
    pub fn from_points(points: impl ExactSizeIterator<Item = Point>) -> Self {
        assert!(points.len() > 0);
        let mut points = points.peekable();

        // NOTE(unwrap): Safe as length is at least 1
        let &Point(initial_x, initial_y) = points.peek().unwrap();
        let initial = BoundingBox {
            x_min: initial_x,
            x_max: initial_x,
            y_min: initial_y,
            y_max: initial_y,
        };

        points.fold(initial, |mut bounding_box, point| {
            bounding_box.add(point);
            bounding_box
        })
    }

    /// Update this bounding box to contain `point`.
    pub fn add(&mut self, Point(x, y): Point) {
        self.x_min = i16::min(x, self.x_min);
        self.x_max = i16::max(x, self.x_max);
        self.y_min = i16::min(y, self.y_min);
        self.y_max = i16::max(y, self.y_max);
    }
}

impl std::ops::Add for Point {
    type Output = Self;

    fn add(self, Point(x1, y1): Point) -> Self::Output {
        let Point(x, y) = self;
        Point(x + x1, y + y1)
    }
}

impl From<CompositeGlyphArgument> for i32 {
    fn from(arg: CompositeGlyphArgument) -> Self {
        match arg {
            CompositeGlyphArgument::U8(value) => i32::from(value),
            CompositeGlyphArgument::I8(value) => i32::from(value),
            CompositeGlyphArgument::U16(value) => i32::from(value),
            CompositeGlyphArgument::I16(value) => i32::from(value),
        }
    }
}

impl TryFrom<CompositeGlyphArgument> for u16 {
    type Error = std::num::TryFromIntError;

    fn try_from(arg: CompositeGlyphArgument) -> Result<Self, Self::Error> {
        match arg {
            CompositeGlyphArgument::U8(value) => Ok(u16::from(value)),
            CompositeGlyphArgument::I8(value) => u16::try_from(value),
            CompositeGlyphArgument::U16(value) => Ok(value),
            CompositeGlyphArgument::I16(value) => u16::try_from(value),
        }
    }
}

impl From<CompositeGlyphScale> for Matrix2x2F {
    fn from(scale: CompositeGlyphScale) -> Self {
        match scale {
            CompositeGlyphScale::Scale(scale) => {
                let scale = f32::from(scale);
                Matrix2x2F::from_scale(scale)
            }
            CompositeGlyphScale::XY { x_scale, y_scale } => {
                let scale = Vector2F::new(f32::from(x_scale), f32::from(y_scale));
                Matrix2x2F::from_scale(scale)
            }
            CompositeGlyphScale::Matrix(matrix) => Matrix2x2F::row_major(
                f32::from(matrix[0][0]),
                f32::from(matrix[0][1]),
                f32::from(matrix[1][0]),
                f32::from(matrix[1][1]),
            ),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::binary::write::WriteBuffer;
    use crate::error::ReadWriteError;

    pub(super) fn simple_glyph_fixture() -> SimpleGlyph {
        SimpleGlyph {
            bounding_box: BoundingBox {
                x_min: 60,
                x_max: 915,
                y_min: -105,
                y_max: 702,
            },
            end_pts_of_contours: vec![8],
            instructions: Box::default(),
            coordinates: vec![
                (
                    SimpleGlyphFlag::ON_CURVE_POINT
                        | SimpleGlyphFlag::Y_SHORT_VECTOR
                        | SimpleGlyphFlag::Y_IS_SAME_OR_POSITIVE_Y_SHORT_VECTOR,
                    Point(433, 77),
                ),
                (
                    SimpleGlyphFlag::X_SHORT_VECTOR
                        | SimpleGlyphFlag::Y_SHORT_VECTOR
                        | SimpleGlyphFlag::X_IS_SAME_OR_POSITIVE_X_SHORT_VECTOR,
                    Point(499, 30),
                ),
                (
                    SimpleGlyphFlag::ON_CURVE_POINT
                        | SimpleGlyphFlag::X_SHORT_VECTOR
                        | SimpleGlyphFlag::Y_SHORT_VECTOR
                        | SimpleGlyphFlag::X_IS_SAME_OR_POSITIVE_X_SHORT_VECTOR,
                    Point(625, 2),
                ),
                (
                    SimpleGlyphFlag::X_SHORT_VECTOR
                        | SimpleGlyphFlag::Y_SHORT_VECTOR
                        | SimpleGlyphFlag::X_IS_SAME_OR_POSITIVE_X_SHORT_VECTOR,
                    Point(756, -27),
                ),
                (
                    SimpleGlyphFlag::ON_CURVE_POINT
                        | SimpleGlyphFlag::X_SHORT_VECTOR
                        | SimpleGlyphFlag::Y_SHORT_VECTOR
                        | SimpleGlyphFlag::X_IS_SAME_OR_POSITIVE_X_SHORT_VECTOR,
                    Point(915, -31),
                ),
                (
                    SimpleGlyphFlag::X_SHORT_VECTOR | SimpleGlyphFlag::Y_SHORT_VECTOR,
                    Point(891, -47),
                ),
                (
                    SimpleGlyphFlag::ON_CURVE_POINT
                        | SimpleGlyphFlag::X_SHORT_VECTOR
                        | SimpleGlyphFlag::Y_SHORT_VECTOR,
                    Point(862, -60),
                ),
                (
                    SimpleGlyphFlag::X_SHORT_VECTOR | SimpleGlyphFlag::Y_SHORT_VECTOR,
                    Point(832, -73),
                ),
                (
                    SimpleGlyphFlag::ON_CURVE_POINT
                        | SimpleGlyphFlag::X_SHORT_VECTOR
                        | SimpleGlyphFlag::Y_SHORT_VECTOR,
                    Point(819, -103),
                ),
            ],
            phantom_points: None,
        }
    }

    pub(super) fn composite_glyph_fixture(instructions: &'static [u8]) -> CompositeGlyph {
        CompositeGlyph {
            bounding_box: BoundingBox {
                x_min: 205,
                x_max: 4514,
                y_min: 0,
                y_max: 1434,
            },
            glyphs: vec![
                CompositeGlyphComponent {
                    flags: CompositeGlyphFlag::ARG_1_AND_2_ARE_WORDS
                        | CompositeGlyphFlag::ARGS_ARE_XY_VALUES
                        | CompositeGlyphFlag::ROUND_XY_TO_GRID
                        | CompositeGlyphFlag::MORE_COMPONENTS
                        | CompositeGlyphFlag::UNSCALED_COMPONENT_OFFSET,
                    glyph_index: 5,
                    argument1: CompositeGlyphArgument::I16(3453),
                    argument2: CompositeGlyphArgument::I16(0),
                    scale: None,
                },
                CompositeGlyphComponent {
                    flags: CompositeGlyphFlag::ARG_1_AND_2_ARE_WORDS
                        | CompositeGlyphFlag::ARGS_ARE_XY_VALUES
                        | CompositeGlyphFlag::ROUND_XY_TO_GRID
                        | CompositeGlyphFlag::MORE_COMPONENTS
                        | CompositeGlyphFlag::UNSCALED_COMPONENT_OFFSET,
                    glyph_index: 4,
                    argument1: CompositeGlyphArgument::I16(2773),
                    argument2: CompositeGlyphArgument::I16(0),
                    scale: None,
                },
                CompositeGlyphComponent {
                    flags: CompositeGlyphFlag::ARG_1_AND_2_ARE_WORDS
                        | CompositeGlyphFlag::ARGS_ARE_XY_VALUES
                        | CompositeGlyphFlag::ROUND_XY_TO_GRID
                        | CompositeGlyphFlag::MORE_COMPONENTS
                        | CompositeGlyphFlag::UNSCALED_COMPONENT_OFFSET,
                    glyph_index: 3,
                    argument1: CompositeGlyphArgument::I16(1182),
                    argument2: CompositeGlyphArgument::I16(0),
                    scale: None,
                },
                CompositeGlyphComponent {
                    flags: CompositeGlyphFlag::ARG_1_AND_2_ARE_WORDS
                        | CompositeGlyphFlag::ARGS_ARE_XY_VALUES
                        | CompositeGlyphFlag::ROUND_XY_TO_GRID
                        | CompositeGlyphFlag::UNSCALED_COMPONENT_OFFSET
                        | CompositeGlyphFlag::WE_HAVE_INSTRUCTIONS,
                    glyph_index: 2,
                    argument1: CompositeGlyphArgument::I16(205),
                    argument2: CompositeGlyphArgument::I16(0),
                    scale: None,
                },
            ],
            instructions: Box::from(instructions),
            phantom_points: None,
        }
    }

    #[test]
    fn test_point_bounding_box() {
        let points = [Point(1761, 565), Point(2007, 565), Point(1884, 1032)];

        let expected = BoundingBox {
            x_min: 1761,
            y_min: 565,
            x_max: 2007,
            y_max: 1032,
        };

        assert_eq!(BoundingBox::from_points(points.iter().copied()), expected);
    }

    #[test]
    fn write_glyf_table_loca_sanity_check() {
        let glyf = GlyfTable {
            records: vec![GlyfRecord::empty(), GlyfRecord::empty()],
        };
        let num_glyphs = glyf.records.len();
        let mut buffer = WriteBuffer::new();
        let loca = GlyfTable::write_dep(&mut buffer, glyf, IndexToLocFormat::Long).unwrap();
        assert_eq!(loca.offsets.len(), num_glyphs + 1);
    }

    #[test]
    fn write_composite_glyf_instructions() {
        let glyph = Glyph::Composite(composite_glyph_fixture(&[1, 2, 3, 4]));

        let mut buffer = WriteBuffer::new();
        Glyph::write(&mut buffer, glyph).unwrap();

        // Read it back and check the instructions are intact
        match ReadScope::new(buffer.bytes()).read::<Glyph>() {
            Ok(Glyph::Composite(CompositeGlyph { instructions, .. })) => {
                assert_eq!(&*instructions, vec![1, 2, 3, 4].as_slice())
            }
            _ => panic!("did not read back expected instructions"),
        }
    }

    #[test]
    fn read_glyph_offsets_correctly() {
        // Test for a bug in which only the length relative to current ReadCtxt offset was used
        // to read a glyph out of the `glyf` table. It should have been using `start` and `end`
        // offsets read from `loca`. The bug was discovered when reading the Baekmuk Batang font
        // in which the glyph data starts at offset 366.
        let glyph = simple_glyph_fixture();

        // Write the glyph out
        let mut buffer = WriteBuffer::new();
        buffer.write_zeros(4).unwrap(); // Add some unused data at the start
        SimpleGlyph::write(&mut buffer, glyph).unwrap();
        let glyph_data = buffer.into_inner();

        let mut buffer = WriteBuffer::new();
        let loca = owned::LocaTable {
            offsets: vec![4, 4, glyph_data.len() as u32 - 4],
        };
        owned::LocaTable::write_dep(&mut buffer, loca, IndexToLocFormat::Long)
            .expect("unable to generate loca");
        let loca_data = buffer.into_inner();

        // Parse and verify
        let num_glyphs = 2;
        let loca = ReadScope::new(&loca_data)
            .read_dep::<LocaTable<'_>>((num_glyphs, IndexToLocFormat::Long))
            .expect("unable to read loca");
        let glyf = ReadScope::new(&glyph_data)
            .read_dep::<GlyfTable<'_>>(&loca)
            .expect("unable to read glyf");
        assert_eq!(glyf.records.len(), 2);
        assert_eq!(&glyf.records[0], &GlyfRecord::empty());
        let glyph = &glyf.records[1];

        // Before the fix num_contours was read as 0
        assert_eq!(glyph.number_of_contours(), 1);
    }

    // Regarding simple glyphs the OpenType spec says:
    // This is the table information needed if numberOfContours is greater than or equal to zero
    // https://docs.microsoft.com/en-us/typography/opentype/spec/glyf#simple-glyph-description
    //
    // We previously rejected glyphs with zero contours.
    #[test]
    fn simple_glyph_with_zero_contours() {
        let glyph_data = &[
            0, 0, 0, 0, 0, 0, 0, 0, // bounding box
            0, 0, // instruction length
        ];
        let expected = SimpleGlyph {
            bounding_box: BoundingBox::empty(),
            end_pts_of_contours: vec![],
            instructions: Box::default(),
            coordinates: vec![],
            phantom_points: None,
        };

        let glyph = ReadScope::new(glyph_data)
            .read_dep::<SimpleGlyph>(0)
            .unwrap();
        assert_eq!(glyph, expected);
    }

    #[test]
    fn write_simple_glyph_with_zero_contours() {
        let glyph = SimpleGlyph {
            bounding_box: BoundingBox::empty(),
            end_pts_of_contours: vec![],
            instructions: Box::default(),
            coordinates: vec![],
            phantom_points: None,
        };

        let mut buffer = WriteBuffer::new();
        assert!(SimpleGlyph::write(&mut buffer, glyph).is_ok());
    }

    #[test]
    fn read_glyph_with_incorrect_loca_length() {
        // Write the glyph out
        let glyph = simple_glyph_fixture();
        let mut buffer = WriteBuffer::new();
        Glyph::write(&mut buffer, Glyph::Simple(glyph)).unwrap();
        let glyph_data = buffer.into_inner();

        let mut buffer = WriteBuffer::new();
        let loca = owned::LocaTable {
            offsets: vec![0, 0, glyph_data.len() as u32 + 1], // + 1 to go past end of glyf
        };
        owned::LocaTable::write_dep(&mut buffer, loca, IndexToLocFormat::Long)
            .expect("unable to generate loca");
        let loca_data = buffer.into_inner();

        // Parse and verify
        let num_glyphs = 2;
        let loca = ReadScope::new(&loca_data)
            .read_dep::<LocaTable<'_>>((num_glyphs, IndexToLocFormat::Long))
            .expect("unable to read loca");
        assert!(ReadScope::new(&glyph_data)
            .read_dep::<GlyfTable<'_>>(&loca)
            .is_ok())
    }

    // This is a test for a bug in which a composite glyph read with has_instructions = yes, but
    // instruction length 0 would be written without an instruction length field. This resulting
    // font was invalid as parsers would see the has_instructions flag and attempt to read the
    // non-existent instruction length.
    #[test]
    fn write_composite_glyph_with_empty_instructions() {
        let glyph = composite_glyph_fixture(&[]);

        let mut buffer = WriteBuffer::new();
        Glyph::write(&mut buffer, Glyph::Composite(glyph)).unwrap();

        // Ensure we can read it back. Before this fix this failed.
        match ReadScope::new(buffer.bytes()).read::<Glyph>() {
            Ok(Glyph::Composite(CompositeGlyph { instructions, .. })) => {
                assert_eq!(instructions, Box::default())
            }
            Ok(_) => panic!("did not read back expected glyph"),
            Err(_) => panic!("unable to read back glyph"),
        }
    }

    #[test]
    fn test_number_of_points_empty() {
        let glyph = GlyfRecord::empty();
        assert_eq!(glyph.number_of_points().unwrap(), 0);
    }

    #[test]
    fn test_number_of_points_simple_parsed() {
        let glyph = GlyfRecord::from(simple_glyph_fixture());
        assert_eq!(glyph.number_of_points().unwrap(), 9);
    }

    #[test]
    fn test_number_of_points_simple_present() -> Result<(), ReadWriteError> {
        // Serialize
        let glyph = GlyfRecord::from(simple_glyph_fixture());
        let glyf = GlyfTable {
            records: vec![GlyfRecord::empty(), glyph],
        };
        let num_glyphs = glyf.records.len() as u16;
        let mut buffer = WriteBuffer::new();
        let loca = GlyfTable::write_dep(&mut buffer, glyf, IndexToLocFormat::Long).unwrap();
        let mut loca_buffer = WriteBuffer::new();
        owned::LocaTable::write_dep(&mut loca_buffer, loca, IndexToLocFormat::Long)?;
        let loca_data = loca_buffer.into_inner();
        let loca = ReadScope::new(&loca_data)
            .read_dep::<LocaTable<'_>>((num_glyphs, IndexToLocFormat::Long))?;

        // Read back
        let glyf = ReadScope::new(&buffer.bytes())
            .read_dep::<GlyfTable<'_>>(&loca)
            .unwrap();
        let glyph = &glyf.records[1];
        assert!(matches!(glyph, GlyfRecord::Present { .. }));
        assert_eq!(glyph.number_of_points().unwrap(), 9);
        Ok(())
    }

    #[test]
    fn test_number_of_points_composite_parsed() {
        // Test parsed
        let glyph = GlyfRecord::from(composite_glyph_fixture(&[]));
        assert_eq!(glyph.number_of_points().unwrap(), 4);
    }

    #[test]
    fn test_number_of_points_composite_present() -> Result<(), ReadWriteError> {
        // Serialize
        let glyph = GlyfRecord::from(composite_glyph_fixture(&[]));
        let glyf = GlyfTable {
            records: vec![GlyfRecord::empty(), glyph],
        };
        let num_glyphs = glyf.records.len() as u16;
        let mut buffer = WriteBuffer::new();
        let loca = GlyfTable::write_dep(&mut buffer, glyf, IndexToLocFormat::Long).unwrap();
        let mut loca_buffer = WriteBuffer::new();
        owned::LocaTable::write_dep(&mut loca_buffer, loca, IndexToLocFormat::Long)?;
        let loca_data = loca_buffer.into_inner();
        let loca = ReadScope::new(&loca_data)
            .read_dep::<LocaTable<'_>>((num_glyphs, IndexToLocFormat::Long))?;

        // Read back
        let glyf = ReadScope::new(&buffer.bytes())
            .read_dep::<GlyfTable<'_>>(&loca)
            .unwrap();
        let glyph = &glyf.records[1];
        assert!(matches!(glyph, GlyfRecord::Present { .. }));
        assert_eq!(glyph.number_of_points().unwrap(), 4);
        Ok(())
    }
}
