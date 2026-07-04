//! Binary reading of the `morx` table.
use std::convert::TryInto;

use bitflags::bitflags;

use crate::binary::read::{
    ReadArray, ReadBinary, ReadBinaryDep, ReadCtxt, ReadFrom, ReadUnchecked,
};
use crate::binary::{U16Be, U32Be, U64Be, U8};
use crate::error::ParseError;
use crate::size;
use crate::SafeFrom;

use super::aat::VecTable;

/// The extended glyph metamorphosis table.
///
/// <https://developer.apple.com/fonts/TrueType-Reference-Manual/RM06/Chap6morx.html>
#[derive(Debug)]
pub struct MorxTable<'a> {
    pub version: u16,
    pub chains: Vec<Chain<'a>>,
}

impl ReadBinaryDep for MorxTable<'_> {
    type HostType<'a> = MorxTable<'a>;
    type Args<'a> = u16;

    fn read_dep<'a>(
        ctxt: &mut ReadCtxt<'a>,
        n_glyphs: u16,
    ) -> Result<Self::HostType<'a>, ParseError> {
        let version = ctxt.read_u16be()?;
        // TODO: handle this:
        // If the 'morx' table version is 3 or greater, then the last subtable in the chain is
        // followed by a subtableGlyphCoverageArray.
        //
        // Presumably a version 1 morx table used a State Table instead of Extended State Table
        // (STXHeader)
        ctxt.check_version(version == 2 || version == 3)?;
        let _unused = ctxt.read_u16be()?;
        let n_chains = ctxt.read_u32be()?;
        let mut chains = Vec::with_capacity(usize::safe_from(n_chains));

        for _i in 0..n_chains {
            // Read the chain header to get the chain length
            let scope_hdr = ctxt.scope();
            let chain_header = scope_hdr.read::<ChainHeader>()?;
            let chain_length = usize::safe_from(chain_header.chain_length);

            // Get a scope of length "chain_length" to read the chain and advance to the correct
            // position in the buffer for reading the next chain, regardless whether the "Subtable
            // Glyph Coverage table" is present at the end of the chain.
            let chain_scope = ctxt.read_scope(chain_length)?;
            let chain = chain_scope.read_dep::<Chain<'a>>(n_glyphs)?;
            chains.push(chain);
        }

        Ok(MorxTable { version, chains })
    }
}

#[derive(Debug)]
pub struct ChainHeader {
    pub default_flags: u32,
    chain_length: u32,
    n_feature_entries: u32,
    n_subtables: u32,
}

impl ReadFrom for ChainHeader {
    type ReadType = (U32Be, U32Be, U32Be, U32Be);

    fn read_from(
        (default_flags, chain_length, n_feature_entries, n_subtables): (u32, u32, u32, u32),
    ) -> Self {
        ChainHeader {
            default_flags,
            chain_length,
            n_feature_entries,
            n_subtables,
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct Feature {
    pub feature_type: u16,
    pub feature_setting: u16,
    pub enable_flags: u32,
    pub disable_flags: u32,
}

impl ReadFrom for Feature {
    type ReadType = (U16Be, U16Be, U32Be, U32Be);

    fn read_from(
        (feature_type, feature_setting, enable_flags, disable_flags): (u16, u16, u32, u32),
    ) -> Self {
        Feature {
            feature_type,
            feature_setting,
            enable_flags,
            disable_flags,
        }
    }
}

#[derive(Debug)]
pub struct Chain<'a> {
    pub chain_header: ChainHeader,
    pub feature_array: ReadArray<'a, Feature>,
    pub subtables: Vec<Subtable<'a>>,
}

impl ReadBinaryDep for Chain<'_> {
    type HostType<'a> = Chain<'a>;
    type Args<'a> = u16;

    fn read_dep<'a>(
        ctxt: &mut ReadCtxt<'a>,
        n_glyphs: u16,
    ) -> Result<Self::HostType<'a>, ParseError> {
        let chain_header = ctxt.read::<ChainHeader>()?;
        let feature_array =
            ctxt.read_array::<Feature>(usize::safe_from(chain_header.n_feature_entries))?;
        let subtables = (0..chain_header.n_subtables)
            .map(|_i| ctxt.read_dep::<Subtable<'a>>(n_glyphs))
            .collect::<Result<Vec<_>, _>>()?;

        Ok(Chain {
            chain_header,
            feature_array,
            subtables,
        })
    }
}

#[derive(Debug)]
pub struct Coverage(u32);

impl Coverage {
    /// If set, this subtable will only be applied to vertical text. If clear, this subtable will
    /// only be applied to horizontal text.
    pub fn vertical_text(&self) -> bool {
        self.0 & 0x80000000 != 0
    }

    /// If set, this subtable will process glyphs in descending order. If clear, it will process
    /// the glyphs in ascending order.
    pub fn descending_order(&self) -> bool {
        self.0 & 0x40000000 != 0
    }

    /// If set, this subtable will be applied to both horizontal and vertical text (i.e. the state
    /// of bit 0x80000000 is ignored).
    pub fn all_text(&self) -> bool {
        self.0 & 0x20000000 != 0
    }

    /// If set, this subtable will process glyphs in logical order (or reverse logical order,
    /// depending on the value of bit 0x80000000).
    pub fn logical_order(&self) -> bool {
        self.0 & 0x10000000 != 0
    }

    /// Subtable type.
    fn subtable_type(&self) -> u32 {
        self.0 & 0x000000FF
    }
}

#[derive(Debug)]
pub struct SubtableHeader {
    length: u32,
    pub coverage: Coverage,
    pub sub_feature_flags: u32,
}

impl ReadFrom for SubtableHeader {
    type ReadType = (U32Be, U32Be, U32Be);

    fn read_from((length, coverage, sub_feature_flags): (u32, u32, u32)) -> Self {
        SubtableHeader {
            length,
            coverage: Coverage(coverage),
            sub_feature_flags,
        }
    }
}

#[derive(Debug)]
pub struct Subtable<'a> {
    pub subtable_header: SubtableHeader,
    pub subtable_body: SubtableType<'a>,
}

impl ReadBinaryDep for Subtable<'_> {
    type HostType<'a> = Subtable<'a>;
    type Args<'a> = u16;

    fn read_dep<'a>(
        ctxt: &mut ReadCtxt<'a>,
        n_glyphs: u16,
    ) -> Result<Self::HostType<'a>, ParseError> {
        let subtable_header = ctxt.read::<SubtableHeader>()?;

        // 12 is the length of the subtable header that needs to be skipped.
        let subtable_body_length = subtable_header
            .length
            .checked_sub(12)
            .map(usize::safe_from)
            .ok_or(ParseError::BadEof)?;

        // Get a shorter scope from the ReadCtxt to read the subtable
        let subtable_scope = ctxt.read_scope(subtable_body_length)?;

        let subtable_body = match subtable_header.coverage.subtable_type() {
            0 => SubtableType::Rearrangement(
                subtable_scope.read_dep::<RearrangementSubtable<'a>>(n_glyphs)?,
            ),
            1 => SubtableType::Contextual(
                subtable_scope.read_dep::<ContextualSubtable<'a>>(n_glyphs)?,
            ),
            2 => SubtableType::Ligature(subtable_scope.read_dep::<LigatureSubtable<'a>>(n_glyphs)?),
            4 => SubtableType::NonContextual(
                subtable_scope.read_dep::<NonContextualSubtable<'a>>(n_glyphs)?,
            ),
            // The insertion subtable is not yet supported.
            5 => {
                SubtableType::Insertion(subtable_scope.read_dep::<InsertionSubtable<'a>>(n_glyphs)?)
            }
            _ => return Err(ParseError::BadValue),
        };

        Ok(Subtable {
            subtable_header,
            subtable_body,
        })
    }
}

#[derive(Debug)]
pub enum SubtableType<'a> {
    Rearrangement(RearrangementSubtable<'a>),
    Contextual(ContextualSubtable<'a>),
    Ligature(LigatureSubtable<'a>),
    NonContextual(NonContextualSubtable<'a>),
    Insertion(InsertionSubtable<'a>),
}

/// Extended State Table
///
/// > Historically the class table had been a tight array of 8-bit values. However, in certain cases
/// > (such as Asian fonts) the potential wide separation between glyph indices covered by the same
/// > class table has led to much wasted space in the table. Therefore, the class tables in extended
/// > state tables are now simply LookupTables, where the looked-up value is a 16-bit class value.
/// > Note that a format 8 LookupTable (trimmed array) yields the same results as class array defined
/// > in the original state table format.
#[derive(Debug)]
struct StxHeader {
    n_classes: u32,
    class_table_offset: u32,
    state_array_offset: u32,
    entry_table_offset: u32,
}

impl ReadFrom for StxHeader {
    type ReadType = (U32Be, U32Be, U32Be, U32Be);

    fn read_from(
        (n_classes, class_table_offset, state_array_offset, entry_table_offset): (
            u32,
            u32,
            u32,
            u32,
        ),
    ) -> Self {
        StxHeader {
            n_classes,
            class_table_offset,
            state_array_offset,
            entry_table_offset,
        }
    }
}

pub trait StxTable<'a, T> {
    fn class_table(&self) -> &ClassLookupTable<'a>;

    fn state_array(&self) -> &StateArray<'a>;

    fn entry_table(&self) -> &VecTable<T>;
}

macro_rules! stx_table {
    ($entry: ident, $struct: ident) => {
        impl<'a> StxTable<'a, $entry> for $struct<'a> {
            fn class_table(&self) -> &ClassLookupTable<'a> {
                &self.class_table
            }

            fn state_array(&self) -> &StateArray<'a> {
                &self.state_array
            }

            fn entry_table(&self) -> &VecTable<$entry> {
                &self.entry_table
            }
        }
    };
}

stx_table!(RearrangementEntry, RearrangementSubtable);
stx_table!(ContextualEntry, ContextualSubtable);
stx_table!(LigatureEntry, LigatureSubtable);
stx_table!(InsertionEntry, InsertionSubtable);

#[derive(Debug)]
pub struct RearrangementSubtable<'a> {
    class_table: ClassLookupTable<'a>,
    state_array: StateArray<'a>,
    entry_table: VecTable<RearrangementEntry>,
}

impl ReadBinaryDep for RearrangementSubtable<'_> {
    type HostType<'a> = RearrangementSubtable<'a>;
    type Args<'a> = u16;

    fn read_dep<'a>(
        ctxt: &mut ReadCtxt<'a>,
        n_glyphs: u16,
    ) -> Result<Self::HostType<'a>, ParseError> {
        let subtable = ctxt.scope();

        let stx_header = ctxt.read::<StxHeader>()?;

        let class_table = subtable
            .offset(usize::safe_from(stx_header.class_table_offset))
            .read_dep::<ClassLookupTable<'a>>(n_glyphs)?;

        let state_array = subtable
            .offset(usize::safe_from(stx_header.state_array_offset))
            .read_dep::<StateArray<'a>>(NClasses(stx_header.n_classes))?;

        let entry_table = subtable
            .offset(usize::safe_from(stx_header.entry_table_offset))
            .read::<VecTable<RearrangementEntry>>()?;

        Ok(RearrangementSubtable {
            class_table,
            state_array,
            entry_table,
        })
    }
}

#[derive(Debug)]
pub struct RearrangementEntry {
    pub next_state: u16,
    flags: u16,
}

impl RearrangementEntry {
    /// If set, make the current glyph the first glyph to be rearranged.
    pub fn mark_first(&self) -> bool {
        self.flags & 0x8000 != 0
    }

    /// If set, don't advance to the next glyph before going to the new state. This means that the
    /// glyph index doesn't change, even if the glyph at that index has changed.
    pub fn dont_advance(&self) -> bool {
        self.flags & 0x4000 != 0
    }

    /// If set, make the current glyph the last glyph to be rearranged.
    pub fn mark_last(&self) -> bool {
        self.flags & 0x2000 != 0
    }

    /// The type of rearrangement specified.
    pub fn verb(&self) -> RearrangementVerb {
        use RearrangementVerb::*;
        match self.flags & 0x000F {
            0 => Verb0,
            1 => Verb1,
            2 => Verb2,
            3 => Verb3,
            4 => Verb4,
            5 => Verb5,
            6 => Verb6,
            7 => Verb7,
            8 => Verb8,
            9 => Verb9,
            10 => Verb10,
            11 => Verb11,
            12 => Verb12,
            13 => Verb13,
            14 => Verb14,
            15 => Verb15,
            _ => unreachable!(),
        }
    }
}

#[derive(Debug)]
pub enum RearrangementVerb {
    Verb0,  // No change
    Verb1,  // Ax => xA
    Verb2,  // xD => Dx
    Verb3,  // AxD => DxA
    Verb4,  // ABx => xAB
    Verb5,  // ABx => xBA
    Verb6,  // xCD => CDx
    Verb7,  // xCD => DCx
    Verb8,  // AxCD => CDxA
    Verb9,  // AxCD => DCxA
    Verb10, // ABxD => DxAB
    Verb11, // ABxD => DxBA
    Verb12, // ABxCD => CDxAB
    Verb13, // ABxCD => CDxBA
    Verb14, // ABxCD => DCxAB
    Verb15, // ABxCD => DCxBA
}

impl ReadFrom for RearrangementEntry {
    type ReadType = (U16Be, U16Be);

    fn read_from((next_state, flags): (u16, u16)) -> Self {
        RearrangementEntry { next_state, flags }
    }
}

/// Contextual Glyph Substitution Subtable
#[derive(Debug)]
pub struct ContextualSubtable<'a> {
    class_table: ClassLookupTable<'a>,
    state_array: StateArray<'a>,
    entry_table: VecTable<ContextualEntry>,
    pub substitution_subtables: Vec<ClassLookupTable<'a>>,
}

impl ReadBinaryDep for ContextualSubtable<'_> {
    type HostType<'a> = ContextualSubtable<'a>;
    type Args<'a> = u16;

    fn read_dep<'a>(
        ctxt: &mut ReadCtxt<'a>,
        n_glyphs: u16,
    ) -> Result<Self::HostType<'a>, ParseError> {
        let subtable = ctxt.scope();

        let stx_header = ctxt.read::<StxHeader>()?;
        let substitution_subtables_offset = ctxt.read_u32be()?;

        let class_table = subtable
            .offset(usize::safe_from(stx_header.class_table_offset))
            .read_dep::<ClassLookupTable<'a>>(n_glyphs)?;

        let state_array = subtable
            .offset(usize::safe_from(stx_header.state_array_offset))
            .read_dep::<StateArray<'a>>(NClasses(stx_header.n_classes))?;

        let entry_table = subtable
            .offset(usize::safe_from(stx_header.entry_table_offset))
            .read::<VecTable<ContextualEntry>>()?;

        let first_offset_to_subst_tables = subtable
            .offset(usize::safe_from(substitution_subtables_offset))
            .ctxt()
            .read_u32be()?;

        // This assumes the offsets are in order, which they may not be
        let offset_array_len = first_offset_to_subst_tables / 4;
        let mut subst_tables_ctxt = subtable
            .offset(usize::safe_from(substitution_subtables_offset))
            .ctxt();

        // The spec notes:
        //
        // > Note that nowhere is there specified the number of LookupTables. Since this number is an
        // > artifact of the font production process, and is not needed by the runtime metamorphosis
        // > software, there was no need to include it explicitly.
        //
        // We attempt to read them all up-front, which is fragile but works for the set of fonts
        // tested.
        let mut substitution_subtables: Vec<ClassLookupTable<'a>> = Vec::new();
        for _i in 0..offset_array_len {
            let offset = match subst_tables_ctxt.read_u32be() {
                Ok(offset) => usize::safe_from(offset),
                Err(_err) => break,
            };

            let subst_subtable = match subtable
                .offset(usize::safe_from(substitution_subtables_offset))
                .offset(offset)
                .read_dep::<ClassLookupTable<'a>>(n_glyphs)
            {
                Ok(val) => val,
                Err(_err) => break,
            };
            substitution_subtables.push(subst_subtable);
        }

        Ok(ContextualSubtable {
            class_table,
            state_array,
            entry_table,
            substitution_subtables,
        })
    }
}

#[derive(Debug)]
pub struct ContextualEntry {
    pub next_state: u16,
    pub flags: ContextualEntryFlags,
    pub mark_index: u16,
    pub current_index: u16,
}

bitflags! {
    #[derive(Debug, Clone, Copy, Eq, PartialEq)]
    pub struct ContextualEntryFlags: u16 {
        /// If set, make the current glyph the marked glyph.
        const SET_MARK = 0x8000;
        /// If set, don't advance to the next glyph before going to the new state.
        const DONT_ADVANCE = 0x4000;
        // 0x3FFF 	reserved 	These bits are reserved and should be set to 0.
    }
}

impl ReadFrom for ContextualEntry {
    type ReadType = (U16Be, U16Be, U16Be, U16Be);

    fn read_from((next_state, flags, mark_index, current_index): (u16, u16, u16, u16)) -> Self {
        ContextualEntry {
            next_state,
            flags: ContextualEntryFlags::from_bits_truncate(flags),
            mark_index,
            current_index,
        }
    }
}

/// Noncontextual Glyph Substitution Subtable
#[derive(Debug)]
pub struct NonContextualSubtable<'a> {
    pub lookup_table: ClassLookupTable<'a>,
}

impl ReadBinaryDep for NonContextualSubtable<'_> {
    type HostType<'a> = NonContextualSubtable<'a>;
    type Args<'a> = u16;

    fn read_dep<'a>(
        ctxt: &mut ReadCtxt<'a>,
        n_glyphs: u16,
    ) -> Result<Self::HostType<'a>, ParseError> {
        let lookup_table = ctxt.read_dep::<ClassLookupTable<'a>>(n_glyphs)?;

        Ok(NonContextualSubtable { lookup_table })
    }
}

/// Ligature subtable
#[derive(Debug)]
pub struct LigatureSubtable<'a> {
    class_table: ClassLookupTable<'a>,
    state_array: StateArray<'a>,
    entry_table: VecTable<LigatureEntry>,
    pub action_table: VecTable<LigatureAction>,
    pub component_table: ComponentTable<'a>,
    pub ligature_list: LigatureList<'a>,
}

impl ReadBinaryDep for LigatureSubtable<'_> {
    type HostType<'a> = LigatureSubtable<'a>;
    type Args<'a> = u16;

    fn read_dep<'a>(
        ctxt: &mut ReadCtxt<'a>,
        n_glyphs: u16,
    ) -> Result<Self::HostType<'a>, ParseError> {
        let subtable = ctxt.scope();

        let stx_header = ctxt.read::<StxHeader>()?;

        let lig_action_offset = ctxt.read_u32be()?;

        let component_offset = ctxt.read_u32be()?;

        let ligature_list_offset = ctxt.read_u32be()?;

        let class_table = subtable
            .offset(usize::safe_from(stx_header.class_table_offset))
            .read_dep::<ClassLookupTable<'a>>(n_glyphs)?;

        let state_array = subtable
            .offset(usize::safe_from(stx_header.state_array_offset))
            .read_dep::<StateArray<'a>>(NClasses(stx_header.n_classes))?;

        let entry_table = subtable
            .offset(usize::safe_from(stx_header.entry_table_offset))
            .read::<VecTable<LigatureEntry>>()?;

        let action_table = subtable
            .offset(usize::safe_from(lig_action_offset))
            .read::<VecTable<LigatureAction>>()?;

        let component_table = subtable
            .offset(usize::safe_from(component_offset))
            .read::<ComponentTable<'a>>()?;

        let ligature_list = subtable
            .offset(usize::safe_from(ligature_list_offset))
            .read::<LigatureList<'a>>()?;

        Ok(LigatureSubtable {
            class_table,
            state_array,
            entry_table,
            action_table,
            component_table,
            ligature_list,
        })
    }
}

#[derive(Debug)]
pub struct LigatureEntry {
    pub next_state_index: u16,
    pub flags: LigatureEntryFlags,
    pub lig_action_index: u16,
}

bitflags! {
    #[derive(Debug, Clone, Copy, Eq, PartialEq)]
    pub struct LigatureEntryFlags: u16 {
        /// Push this glyph onto the component stack for eventual processing.
        const SET_COMPONENT = 0x8000;
        /// Leave the glyph pointer at this glyph for the next iteration.
        const DONT_ADVANCE = 0x4000;
        /// Use the ligActionIndex to process a ligature group.
        const PERFORM_ACTION = 0x2000;
        // 0x1FFF   RESERVED    Reserved; set to zero.
    }
}

impl ReadFrom for LigatureEntry {
    type ReadType = (U16Be, U16Be, U16Be);

    fn read_from((next_state_index, flags, lig_action_index): (u16, u16, u16)) -> Self {
        LigatureEntry {
            next_state_index,
            flags: LigatureEntryFlags::from_bits_truncate(flags),
            lig_action_index,
        }
    }
}

#[derive(Debug)]
pub struct LigatureAction(u32);

impl LigatureAction {
    /// This is the last action in the list. This also implies storage.
    pub fn last(&self) -> bool {
        self.0 & 0x80000000 != 0
    }

    /// Store the ligature at the current cumulated index in the ligature table in place of the
    /// marked (i.e. currently-popped) glyph.
    pub fn store(&self) -> bool {
        self.0 & 0x40000000 != 0
    }

    /// A 30-bit value which is sign-extended to 32-bits and added to the glyph ID, resulting in an
    /// index into the component table.
    pub fn offset(&self) -> i32 {
        let mut offset = self.0 & 0x3FFFFFFF; // Take 30 bits.
        if offset & 0x20000000 != 0 {
            offset |= 0xC0000000; // Sign-extend it to 32 bits.
        }
        offset as i32 // Cast is safe due to masking.
    }
}

impl ReadFrom for LigatureAction {
    type ReadType = U32Be;

    fn read_from(action: u32) -> Self {
        LigatureAction(action)
    }
}

#[derive(Debug)]
pub struct InsertionSubtable<'a> {
    class_table: ClassLookupTable<'a>,
    state_array: StateArray<'a>,
    entry_table: VecTable<InsertionEntry>,
    pub action_table: VecTable<InsertionAction>,
}

impl ReadBinaryDep for InsertionSubtable<'_> {
    type HostType<'a> = InsertionSubtable<'a>;
    type Args<'a> = u16;

    fn read_dep<'a>(
        ctxt: &mut ReadCtxt<'a>,
        n_glyphs: u16,
    ) -> Result<Self::HostType<'a>, ParseError> {
        let subtable = ctxt.scope();

        let stx_header = ctxt.read::<StxHeader>()?;
        let insertion_action_offset = ctxt.read_u32be()?;

        let class_table = subtable
            .offset(usize::safe_from(stx_header.class_table_offset))
            .read_dep::<ClassLookupTable<'a>>(n_glyphs)?;

        let state_array = subtable
            .offset(usize::safe_from(stx_header.state_array_offset))
            .read_dep::<StateArray<'a>>(NClasses(stx_header.n_classes))?;

        let entry_table = subtable
            .offset(usize::safe_from(stx_header.entry_table_offset))
            .read::<VecTable<InsertionEntry>>()?;

        let action_table = subtable
            .offset(usize::safe_from(insertion_action_offset))
            .read::<VecTable<InsertionAction>>()?;

        Ok(InsertionSubtable {
            class_table,
            state_array,
            entry_table,
            action_table,
        })
    }
}

#[derive(Debug)]
pub struct InsertionEntry {
    pub next_state: u16,
    flags: u16,
    pub current_insert_index: u16,
    pub marked_insert_index: u16,
}

impl InsertionEntry {
    /// If set, mark the current glyph.
    pub fn set_mark(&self) -> bool {
        self.flags & 0x8000 != 0
    }

    /// If set, don't update the glyph index before going to the new state. This does not mean that
    /// the glyph pointed to is the same one as before. If you've made insertions immediately
    /// downstream of the current glyph, the next glyph processed would in fact be the first one
    /// inserted.
    pub fn dont_advance(&self) -> bool {
        self.flags & 0x4000 != 0
    }

    /// If set, and the currentInsertList is nonzero, then the specified glyph list will be inserted
    /// as a kashida-like insertion, either before or after the current glyph (depending on the
    /// state of the currentInsertBefore flag). If clear, and the currentInsertList is nonzero, then
    /// the specified glyph list will be inserted as a split-vowel-like insertion, either before or
    /// after the current glyph (depending on the state of the currentInsertBefore flag).
    pub fn current_is_kashida_like(&self) -> bool {
        self.flags & 0x2000 != 0
    }

    /// If set, and the markedInsertList is nonzero, then the specified glyph list will be inserted
    /// as a kashida-like insertion, either before or after the marked glyph (depending on the state
    /// of the markedInsertBefore flag). If clear, and the markedInsertList is nonzero, then the
    /// specified glyph list will be inserted as a split-vowel-like insertion, either before or
    /// after the marked glyph (depending on the state of the markedInsertBefore flag).
    pub fn marked_is_kashida_like(&self) -> bool {
        self.flags & 0x1000 != 0
    }

    /// If set, specifies that insertions are to be made to the left of the current glyph. If clear,
    /// they're made to the right of the current glyph.
    pub fn current_insert_before(&self) -> bool {
        self.flags & 0x0800 != 0
    }

    /// If set, specifies that insertions are to be made to the left of the marked glyph. If clear,
    /// they're made to the right of the marked glyph.
    pub fn marked_insert_before(&self) -> bool {
        self.flags & 0x0400 != 0
    }

    /// This 5-bit field is treated as a count of the number of glyphs to insert at the current
    /// position. Since zero means no insertions, the largest number of insertions at any given
    /// current location is 31 glyphs.
    pub fn current_insert_count(&self) -> usize {
        usize::from((self.flags & 0x03E0) >> 5)
    }

    /// This 5-bit field is treated as a count of the number of glyphs to insert at the marked
    /// position. Since zero means no insertions, the largest number of insertions at any given
    /// marked location is 31 glyphs.
    pub fn marked_insert_count(&self) -> usize {
        usize::from(self.flags & 0x001F)
    }
}

impl ReadFrom for InsertionEntry {
    type ReadType = (U16Be, U16Be, U16Be, U16Be);

    fn read_from(
        (next_state, flags, current_insert_index, marked_insert_index): (u16, u16, u16, u16),
    ) -> Self {
        InsertionEntry {
            next_state,
            flags,
            current_insert_index,
            marked_insert_index,
        }
    }
}

#[derive(Debug)]
pub struct InsertionAction(pub u16);

impl ReadFrom for InsertionAction {
    type ReadType = U16Be;

    fn read_from(action: u16) -> Self {
        InsertionAction(action)
    }
}

#[derive(Debug, Clone, Copy)]
pub struct NClasses(u32);

#[derive(Debug)]
pub struct StateArray<'a>(pub Vec<ReadArray<'a, U16Be>>);

impl ReadBinaryDep for StateArray<'_> {
    type Args<'a> = NClasses;
    type HostType<'a> = StateArray<'a>;

    fn read_dep<'a>(
        ctxt: &mut ReadCtxt<'a>,
        NClasses(n_classes): NClasses,
    ) -> Result<Self::HostType<'a>, ParseError> {
        let mut state_array: Vec<ReadArray<'a, U16Be>> = Vec::new();
        let state_row_len = usize::safe_from(n_classes);

        loop {
            let state_row = match ctxt.read_array::<U16Be>(state_row_len) {
                Ok(array) => array,
                Err(ParseError::BadEof) => break,
                Err(err) => return Err(err),
            };

            state_array.push(state_row);
        }

        Ok(StateArray(state_array))
    }
}

#[derive(Debug)]
pub struct ComponentTable<'a> {
    pub component_array: ReadArray<'a, U16Be>,
}

impl ReadBinary for ComponentTable<'_> {
    type HostType<'a> = ComponentTable<'a>;

    fn read<'a>(ctxt: &mut ReadCtxt<'a>) -> Result<Self::HostType<'a>, ParseError> {
        let len_remaining = ctxt.scope().data().len();
        let component_array = ctxt.read_array::<U16Be>(len_remaining / size::U16)?;

        Ok(ComponentTable { component_array })
    }
}

#[derive(Debug)]
pub struct LigatureList<'a>(pub ReadArray<'a, U16Be>);

impl LigatureList<'_> {
    pub fn get(&self, index: u16) -> Option<u16> {
        let index = usize::from(index);
        self.0.get_item(index)
    }
}

impl ReadBinary for LigatureList<'_> {
    type HostType<'a> = LigatureList<'a>;

    fn read<'a>(ctxt: &mut ReadCtxt<'a>) -> Result<Self::HostType<'a>, ParseError> {
        let len_remaining = ctxt.scope().data().len();
        let ligature_list = ctxt.read_array::<U16Be>(len_remaining / size::U16)?;

        Ok(LigatureList(ligature_list))
    }
}

#[derive(Debug)]
pub struct LookupTableHeader {
    pub format: u16,
    bin_srch_header: Option<BinSrchHeader>,
}

impl ReadBinary for LookupTableHeader {
    type HostType<'a> = Self;

    fn read(ctxt: &mut ReadCtxt<'_>) -> Result<Self, ParseError> {
        let format = ctxt.read_u16be()?;

        let bin_srch_header = match format {
            2 | 4 | 6 => Some(ctxt.read::<BinSrchHeader>()?),
            0 | 8 | 10 => None,
            _ => return Err(ParseError::BadValue),
        };

        Ok(LookupTableHeader {
            format,
            bin_srch_header,
        })
    }
}

#[derive(Debug, Clone, Copy)]
pub struct BinSrchHeader {
    unit_size: u16,
    n_units: u16,
}

impl ReadBinary for BinSrchHeader {
    type HostType<'a> = Self;

    fn read(ctxt: &mut ReadCtxt<'_>) -> Result<Self, ParseError> {
        let unit_size = ctxt.read_u16be()?;
        let n_units = ctxt.read_u16be()?;

        // Note that the searchRange, entrySelector, and rangeShift fields are redundant. Binary
        // search tables were designed to work efficiently with the processors available in the late
        // 1980s; the inclusion of these three fields allowed the use of a very efficient lookup
        // algorithm on such processors. This optimization is not needed on modern processors and
        // these three fields are no longer used.
        let _search_range = ctxt.read_u16be()?;
        let _entry_selector = ctxt.read_u16be()?;
        let _range_shift = ctxt.read_u16be()?;

        Ok(BinSrchHeader { unit_size, n_units })
    }
}

#[derive(Debug)]
pub enum LookupTable<'a> {
    /// Simple Array format 0
    Format0(ReadArray<'a, U16Be>),
    /// Segment Single format 2
    Format2(ReadArray<'a, LookupSegmentFmt2>),
    /// Segment Array format 4
    Format4(Vec<LookupValuesFmt4<'a>>),
    /// Single Table format 6
    Format6(ReadArray<'a, LookupSingleFmt6>),
    /// Trimmed Array format 8
    Format8(LookupTableFormat8<'a>),
    /// Trimmed Array format 10
    Format10(LookupTableFormat10<'a>),
}

#[derive(Debug)]
pub struct LookupTableFormat8<'a> {
    first_glyph: u16,
    lookup_values: ReadArray<'a, U16Be>,
}

impl<'a> LookupTableFormat8<'a> {
    pub fn new(
        first_glyph: u16,
        lookup_values: ReadArray<'a, U16Be>,
    ) -> Option<LookupTableFormat8<'a>> {
        // Validate arguments
        let len = lookup_values.len().try_into().ok()?;
        let _last = first_glyph.checked_add(len)?;
        Some(LookupTableFormat8 {
            first_glyph,
            lookup_values,
        })
    }
    pub fn contains(&self, glyph: u16) -> bool {
        // NOTE(cast): Safe due to validation in new
        let end = self.first_glyph + self.lookup_values.len() as u16;
        (self.first_glyph..end).contains(&glyph)
    }

    pub fn lookup(&self, glyph: u16) -> Option<u16> {
        if self.contains(glyph) {
            // NOTE(sub): Won't underflow due to contains check
            self.lookup_values
                .get_item(usize::from(glyph - self.first_glyph))
        } else {
            None
        }
    }
}

// TODO: Format8 is basically this with a unit size of 2
#[derive(Debug)]
pub struct LookupTableFormat10<'a> {
    first_glyph: u16,
    lookup_values: UnitSize<'a>,
}

impl<'a> LookupTableFormat10<'a> {
    pub fn new(first_glyph: u16, lookup_values: UnitSize<'a>) -> Option<Self> {
        // Validate arguments
        let len = lookup_values.len().try_into().ok()?;
        let _last = first_glyph.checked_add(len)?;
        Some(LookupTableFormat10 {
            first_glyph,
            lookup_values,
        })
    }

    pub fn contains(&self, glyph: u16) -> bool {
        // NOTE(cast): Safe due to validation in new
        let end = self.first_glyph + self.lookup_values.len() as u16;
        (self.first_glyph..end).contains(&glyph)
    }

    pub fn lookup(&self, glyph: u16) -> Option<u16> {
        if self.contains(glyph) {
            // NOTE(sub): Won't underflow due to contains check
            let index = glyph - self.first_glyph;
            match &self.lookup_values {
                UnitSize::OneByte(one_byte_values) => {
                    one_byte_values.get_item(usize::from(index)).map(u16::from)
                }
                UnitSize::TwoByte(two_byte_values) => two_byte_values.get_item(usize::from(index)),
                // Note: ignore 4-byte and 8-byte lookup values for now
                UnitSize::FourByte { .. } | UnitSize::EightByte { .. } => {
                    todo!("handle 4 and 8-bit lookup values")
                }
            }
        } else {
            None
        }
    }
}

#[derive(Debug)]
pub enum UnitSize<'a> {
    OneByte(ReadArray<'a, U8>),
    TwoByte(ReadArray<'a, U16Be>),
    FourByte(ReadArray<'a, U32Be>),
    EightByte(ReadArray<'a, U64Be>),
}

impl UnitSize<'_> {
    pub fn len(&self) -> usize {
        match self {
            UnitSize::OneByte(array) => array.len(),
            UnitSize::TwoByte(array) => array.len(),
            UnitSize::FourByte(array) => array.len(),
            UnitSize::EightByte(array) => array.len(),
        }
    }
}

#[derive(Debug, Copy, Clone)]
pub struct LookupSegmentFmt2 {
    pub last_glyph: u16,
    pub first_glyph: u16,
    // Assumption: lookup values are commonly u16. If this is not the case
    // an error will be returned when reading the segment.
    pub lookup_value: u16,
}

impl LookupSegmentFmt2 {
    pub fn contains(&self, glyph: u16) -> bool {
        (self.first_glyph..=self.last_glyph).contains(&glyph)
    }
}

impl ReadFrom for LookupSegmentFmt2 {
    type ReadType = (U16Be, U16Be, U16Be);

    fn read_from((last_glyph, first_glyph, lookup_value): (u16, u16, u16)) -> Self {
        LookupSegmentFmt2 {
            last_glyph,
            first_glyph,
            lookup_value,
        }
    }
}

#[derive(Debug)]
pub struct LookupSegmentFmt4 {
    last_glyph: u16,
    first_glyph: u16,
    offset: u16,
}

impl ReadFrom for LookupSegmentFmt4 {
    type ReadType = (U16Be, U16Be, U16Be);

    fn read_from((last_glyph, first_glyph, offset): (u16, u16, u16)) -> Self {
        LookupSegmentFmt4 {
            last_glyph,
            first_glyph,
            offset,
        }
    }
}

#[derive(Debug)]
pub struct LookupValuesFmt4<'a> {
    pub last_glyph: u16,
    pub first_glyph: u16,
    pub lookup_values: ReadArray<'a, U16Be>,
}

impl LookupValuesFmt4<'_> {
    pub fn contains(&self, glyph: u16) -> bool {
        (self.first_glyph..=self.last_glyph).contains(&glyph)
    }
}

#[derive(Debug, Copy, Clone)]
pub struct LookupSingleFmt6 {
    pub glyph: u16,
    // Assumption: lookup values are commonly u16. If this is not the case
    // an error will be returned when reading the segment.
    pub lookup_value: u16,
}

impl ReadFrom for LookupSingleFmt6 {
    type ReadType = (U16Be, U16Be);

    fn read_from((glyph, lookup_value): (u16, u16)) -> Self {
        LookupSingleFmt6 {
            glyph,
            lookup_value,
        }
    }
}

#[derive(Debug)]
pub struct ClassLookupTable<'a> {
    pub lookup_table: LookupTable<'a>,
}

impl ReadBinaryDep for ClassLookupTable<'_> {
    type HostType<'a> = ClassLookupTable<'a>;
    type Args<'a> = u16;

    fn read_dep<'a>(
        ctxt: &mut ReadCtxt<'a>,
        n_glyphs: u16,
    ) -> Result<Self::HostType<'a>, ParseError> {
        let class_table = ctxt.scope();

        let lookup_header = ctxt.read::<LookupTableHeader>()?;
        match (lookup_header.format, lookup_header.bin_srch_header) {
            // Format 0 lookup table presents an array of lookup values, indexed by glyph index.
            (0, None) => {
                let lookup_values = ctxt.read_array(usize::from(n_glyphs))?;
                let lookup_table = LookupTable::Format0(lookup_values);

                Ok(ClassLookupTable { lookup_table })
            }
            (2, Some(b_sch_header)) => {
                // FIXME: 6 is a minimum
                // The units for this binary search are of type LookupSegment, and always have a minimum length of 6.
                if usize::from(b_sch_header.unit_size) != LookupSegmentFmt2::SIZE {
                    return Err(ParseError::BadValue);
                }

                let lookup_segments =
                    ctxt.read_array::<LookupSegmentFmt2>(usize::from(b_sch_header.n_units))?;
                let lookup_table = LookupTable::Format2(lookup_segments);

                Ok(ClassLookupTable { lookup_table })
            }
            (4, Some(b_sch_header)) => {
                // FIXME: 6 is a minimum
                // The units for this binary search are of type LookupSegment and always have a minimum length of 6.
                if usize::from(b_sch_header.unit_size) != LookupSegmentFmt4::SIZE {
                    return Err(ParseError::BadValue);
                }

                let mut lookup_segments: Vec<LookupValuesFmt4<'_>> =
                    Vec::with_capacity(usize::from(b_sch_header.n_units));

                for _i in 0..b_sch_header.n_units {
                    let segment = ctxt.read::<LookupSegmentFmt4>()?;

                    // To guarantee that a binary search terminates, you must include one or more
                    // special "end of search table" values at the end of the data to be searched.
                    // The number of termination values that need to be included is table-specific.
                    // The value that indicates binary search termination is 0xFFFF.
                    if (segment.first_glyph == 0xFFFF) && (segment.last_glyph == 0xFFFF) {
                        break;
                    }

                    let mut read_ctxt = class_table.offset(usize::from(segment.offset)).ctxt();

                    let num_lookup_values = segment
                        .last_glyph
                        .checked_sub(segment.first_glyph)
                        .ok_or(ParseError::BadValue)?
                        .checked_add(1)
                        .ok_or(ParseError::BadValue)?;
                    let lookup_values =
                        read_ctxt.read_array::<U16Be>(usize::from(num_lookup_values))?;

                    let lookup_segment = LookupValuesFmt4 {
                        last_glyph: segment.last_glyph,
                        first_glyph: segment.first_glyph,
                        lookup_values,
                    };

                    lookup_segments.push(lookup_segment);
                }

                let lookup_table = LookupTable::Format4(lookup_segments);

                Ok(ClassLookupTable { lookup_table })
            }
            (6, Some(b_sch_header)) => {
                // FIXME: 4 is a minimum
                // The units for this binary search are of type LookupSingle and always have a minimum length of 4.
                if usize::from(b_sch_header.unit_size) != LookupSingleFmt6::SIZE {
                    return Err(ParseError::BadValue);
                }

                let lookup_entries =
                    ctxt.read_array::<LookupSingleFmt6>(usize::from(b_sch_header.n_units))?;

                let lookup_table = LookupTable::Format6(lookup_entries);

                Ok(ClassLookupTable { lookup_table })
            }
            (8, None) => {
                let first_glyph = ctxt.read_u16be()?;
                let glyph_count = ctxt.read_u16be()?;
                let lookup_values = ctxt.read_array::<U16Be>(usize::from(glyph_count))?;
                let lookup_table = LookupTableFormat8::new(first_glyph, lookup_values)
                    .ok_or(ParseError::BadValue)?;

                Ok(ClassLookupTable {
                    lookup_table: LookupTable::Format8(lookup_table),
                })
            }
            (10, None) => {
                // Size of a lookup unit for this lookup table in bytes. Allowed values are 1, 2, 4, and 8.
                let unit_size = ctxt.read_u16be()?;
                let first_glyph = ctxt.read_u16be()?;
                let glyph_count = ctxt.read_u16be().map(usize::from)?;

                let lookup_values = match unit_size {
                    1 => {
                        let lookup_values = ctxt.read_array::<U8>(glyph_count)?;
                        UnitSize::OneByte(lookup_values)
                    }
                    2 => {
                        let lookup_values = ctxt.read_array::<U16Be>(glyph_count)?;
                        UnitSize::TwoByte(lookup_values)
                    }
                    4 => {
                        let lookup_values = ctxt.read_array::<U32Be>(glyph_count)?;
                        UnitSize::FourByte(lookup_values)
                    }
                    8 => {
                        let lookup_values = ctxt.read_array::<U64Be>(glyph_count)?;
                        UnitSize::EightByte(lookup_values)
                    }
                    _ => return Err(ParseError::BadValue),
                };

                let lookup_table = LookupTableFormat10::new(first_glyph, lookup_values)
                    .ok_or(ParseError::BadValue)?;

                Ok(ClassLookupTable {
                    lookup_table: LookupTable::Format10(lookup_table),
                })
            }
            _ => Err(ParseError::BadVersion),
        }
    }
}
