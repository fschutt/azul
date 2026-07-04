//! CFF CharString (glyph) processing.

use std::fmt::{self, Write};

use rustc_hash::FxHashSet;

pub use argstack::ArgumentsStack;

use crate::binary::read::{ReadCtxt, ReadScope};
use crate::binary::write::{WriteBinary, WriteBuffer, WriteContext};
use crate::binary::{I16Be, U8};
use crate::cff;
use crate::cff::cff2::BlendOperand;
use crate::error::{ParseError, WriteError};
use crate::tables::variable_fonts::{ItemVariationStore, OwnedTuple};
use crate::tables::Fixed;
use crate::GlyphId;

use super::{cff2, CFFError, CFFFont, CFFVariant, MaybeOwnedIndex, Operator};

mod argstack;

// Stack limit according to the Adobe Technical Note #5177 Appendix B and CFF2 Appended B:
//
// https://learn.microsoft.com/en-us/typography/opentype/spec/cff2charstr#appendix-b-cff2-charstring-implementation-limits
pub(crate) const STACK_LIMIT: u8 = 10;

pub(crate) const TWO_BYTE_OPERATOR_MARK: u8 = 12;

pub(crate) trait IsEven {
    fn is_even(&self) -> bool;
    fn is_odd(&self) -> bool;
}

pub(crate) struct UsedSubrs {
    pub(crate) global_subr_used: FxHashSet<usize>,
    pub(crate) local_subr_used: FxHashSet<usize>,
}

/// Context for traversing a CFF CharString.
pub struct CharStringVisitorContext<'a, 'data> {
    glyph_id: GlyphId, // Required to parse local subroutine in CID fonts.
    char_strings_index: &'a MaybeOwnedIndex<'data>,
    local_subr_index: Option<&'a MaybeOwnedIndex<'data>>,
    global_subr_index: &'a MaybeOwnedIndex<'data>,
    // Required for variable fonts
    variable: Option<VariableCharStringVisitorContext<'a, 'data>>,
    width_parsed: bool,
    stems_len: u32,
    has_endchar: bool,
    has_seac: bool,
    seen_blend: bool,
    vsindex: Option<u16>,
    scalars: Option<Vec<Option<f32>>>,
}

/// Variable font data for a [CharStringVisitorContext]. Required if the CharString to be
/// traversed is variable.
#[derive(Copy, Clone)]
pub struct VariableCharStringVisitorContext<'a, 'data> {
    pub vstore: &'a ItemVariationStore<'data>,
    pub instance: &'a OwnedTuple,
}

/// A local or global subroutine index.
#[derive(Debug, Copy, Clone, Eq, PartialEq)]
pub enum SubroutineIndex {
    Local(usize),
    Global(usize),
}

/// Flag indicating the component of an accented character.
#[derive(Debug, Copy, Clone, Eq, PartialEq)]
pub enum SeacChar {
    Base,
    Accent,
}

/// A debug implementation of [CharStringVisitor] that just prints the operators and
/// their operands.
///
/// ### Example
///
/// ```
/// use allsorts::binary::read::ReadScope;
/// use allsorts::cff::charstring::{ArgumentsStack, CharStringVisitorContext, DebugVisitor};
/// use allsorts::cff::CFFError;
/// use allsorts::cff::{self, CFFFont, CFFVariant, CFF};
/// use allsorts::font::MatchingPresentation;
/// use allsorts::font_data::FontData;
/// use allsorts::tables::{OpenTypeData, OpenTypeFont};
/// use allsorts::{tag, Font};
///
/// fn main() -> Result<(), CFFError> {
///     // Read the font
///     let buffer = std::fs::read("tests/fonts/opentype/Klei.otf").unwrap();
///     let otf = ReadScope::new(&buffer).read::<OpenTypeFont<'_>>()?;
///     let offset_table = match otf.data {
///         OpenTypeData::Single(ttf) => ttf,
///         OpenTypeData::Collection(_) => unreachable!(),
///     };
///
///     let cff_table_data = offset_table
///         .read_table(&otf.scope, tag::CFF)?
///         .expect("missing CFF table");
///     let cff = cff_table_data.read::<CFF<'_>>()?;
///
///     // Glyph to visit
///     let glyph_id = 741; // U+2116 NUMERO SIGN
///
///     // Set up CharString visitor
///     let font = &cff.fonts[0];
///     let local_subrs = match &font.data {
///         CFFVariant::CID(_) => None, // Will be resolved on request.
///         CFFVariant::Type1(type1) => type1.local_subr_index.as_ref(),
///     };
///
///     let mut visitor = DebugVisitor;
///     let variable = None;
///     let mut ctx = CharStringVisitorContext::new(
///         glyph_id,
///         &font.char_strings_index,
///         local_subrs,
///         &cff.global_subr_index,
///         variable,
///     );
///     let mut stack = ArgumentsStack {
///         data: &mut [0.0; cff::MAX_OPERANDS],
///         len: 0,
///         max_len: cff::MAX_OPERANDS,
///     };
///     ctx.visit(CFFFont::CFF(font), &mut stack, &mut visitor)?;
///     Ok(())
/// }
/// ```
pub struct DebugVisitor;

/// Trait for types that can be used to traverse a CFF CharString.
///
/// Used in conjunction with [CharStringVisitorContext]. The visitor will receive
/// a `visit` call for each operator with its operands. All methods are optional
/// as they have default no-op implementations.
pub trait CharStringVisitor<T: fmt::Debug, E: std::error::Error> {
    /// Called for each operator in the CharString, except for `callsubr` and `callgsubr`
    /// — these are handled by `enter/exit_subr`.
    fn visit(&mut self, _op: VisitOp, _stack: &ArgumentsStack<'_, T>) -> Result<(), E> {
        Ok(())
    }

    /// Called prior to entering a subroutine.
    ///
    /// The index argument indicates the type of subroutine (local/global) and holds its index.
    fn enter_subr(&mut self, _index: SubroutineIndex) -> Result<(), E> {
        Ok(())
    }

    /// Called when returning from a subroutine.
    fn exit_subr(&mut self) -> Result<(), E> {
        Ok(())
    }

    /// Called before entering a component of an accented character.
    ///
    /// The `seac` argument indicates if it's the base or accent character. `dx` and `dy`
    /// are the x and y position of the accent character. The same values will be supplied
    /// for both the base and accent.
    fn enter_seac(&mut self, _seac: SeacChar, _dx: T, _dy: T) -> Result<(), E> {
        Ok(())
    }

    /// Called when returning from a component of an accented character.
    ///
    /// `seac` indicates whether it's the base or accent.
    fn exit_seac(&mut self, _seac: SeacChar) -> Result<(), E> {
        Ok(())
    }

    /// Called with the hint data that follows the `hintmask` and `cntrmask` operators.
    ///
    /// This function will be called after a `visit` invocation for the operators.
    fn hint_data(&mut self, _op: VisitOp, _hints: &[u8]) -> Result<(), E> {
        Ok(())
    }
}

pub(crate) fn char_string_used_subrs<'a, 'data>(
    font: CFFFont<'a, 'data>,
    char_strings_index: &'a MaybeOwnedIndex<'data>,
    global_subr_index: &'a MaybeOwnedIndex<'data>,
    glyph_id: GlyphId,
) -> Result<UsedSubrs, CFFError> {
    let (local_subrs, max_len) = match font {
        CFFFont::CFF(font) => match &font.data {
            CFFVariant::CID(_) => (None, cff::MAX_OPERANDS), // local subrs will be resolved on request.
            CFFVariant::Type1(type1) => (type1.local_subr_index.as_ref(), cff::MAX_OPERANDS),
        },
        CFFFont::CFF2(cff2) => (cff2.local_subr_index.as_ref(), cff2::MAX_OPERANDS),
    };

    let mut ctx = CharStringVisitorContext::new(
        glyph_id,
        char_strings_index,
        local_subrs,
        global_subr_index,
        // This function should not be called on variable CFF2 fonts. If the CFF2 font is variable
        // it should be instanced first.
        None,
    );

    let mut used_subrs = UsedSubrs {
        global_subr_used: FxHashSet::default(),
        local_subr_used: FxHashSet::default(),
    };

    let mut stack = ArgumentsStack {
        // We use CFF2 max operands as it is the bigger of the two, and it has to be a const value
        // at compile time to init the array.
        data: &mut [0.0; cff2::MAX_OPERANDS],
        len: 0,
        max_len,
    };
    ctx.visit(font, &mut stack, &mut used_subrs)?;

    if matches!(font, CFFFont::CFF(_)) && !ctx.has_endchar {
        return Err(CFFError::MissingEndChar);
    }

    Ok(used_subrs)
}

pub(crate) fn convert_cff2_to_cff<'a, 'data>(
    font: CFFFont<'a, 'data>,
    char_strings_index: &'a MaybeOwnedIndex<'data>,
    global_subr_index: &'a MaybeOwnedIndex<'data>,
    glyph_id: GlyphId,
    width: u16,
    default_width: u16,
    nominal_width: u16,
) -> Result<Vec<u8>, CharStringConversionError> {
    let (local_subrs, max_len) = match font {
        CFFFont::CFF(font) => match &font.data {
            CFFVariant::CID(_) => (None, cff::MAX_OPERANDS), // local subrs will be resolved on request.
            CFFVariant::Type1(type1) => (type1.local_subr_index.as_ref(), cff::MAX_OPERANDS),
        },
        CFFFont::CFF2(cff2) => (cff2.local_subr_index.as_ref(), cff2::MAX_OPERANDS),
    };

    let mut ctx = CharStringVisitorContext::new(
        glyph_id,
        char_strings_index,
        local_subrs,
        global_subr_index,
        None,
    );

    // If the width is not equal to defaultWidthX then the width is stored as the difference from
    // nominalWidthX.
    let char_string_width =
        i16::try_from(i32::from(width) - i32::from(nominal_width)).map_err(ParseError::from)?;

    let mut converter = CharStringConverter {
        buffer: WriteBuffer::new(),
        width: char_string_width,
        // If the width is equal to the default width then it does not need to be written,
        // so we mark it as already written to inhibit the converter from outputting it.
        width_written: width == default_width,
    };

    let mut stack = ArgumentsStack {
        // We use CFF2 max operands as it is the bigger of the two, and it has to be a const value
        // at compile time to init the array.
        data: &mut [cff2::StackValue::Int(0); cff2::MAX_OPERANDS],
        len: 0,
        max_len,
    };
    ctx.visit(font, &mut stack, &mut converter)?;

    // CFF CharStrings must end with an endchar operator
    //
    // > A character that does not have a path (e.g. a space character) may
    // > consist of an endchar operator preceded only by a width value.
    // > Although the width must be specified in the font, it may be specified as
    // > the defaultWidthX in the CFF data, in which case it should not be
    // > specified in the charstring. Also, it may appear in the charstring as the
    // > difference from nominalWidthX. Thus the smallest legal charstring
    // > consists of a single endchar operator.
    converter.maybe_write_width()?;
    U8::write(&mut converter.buffer, operator::ENDCHAR)?;

    Ok(converter.buffer.into_inner())
}

struct CharStringConverter {
    buffer: WriteBuffer,
    width_written: bool,
    width: i16,
}

impl CharStringConverter {
    fn maybe_write_width(&mut self) -> Result<(), WriteError> {
        if !self.width_written {
            cff2::write_stack_value(cff2::StackValue::Int(self.width), &mut self.buffer)?;
            self.width_written = true;
        }
        Ok(())
    }
}

#[derive(Debug)]
pub(crate) enum CharStringConversionError {
    Write(WriteError),
    Cff(CFFError),
}

impl From<CFFError> for CharStringConversionError {
    fn from(err: CFFError) -> Self {
        CharStringConversionError::Cff(err)
    }
}

impl From<ParseError> for CharStringConversionError {
    fn from(err: ParseError) -> Self {
        CharStringConversionError::Cff(CFFError::ParseError(err))
    }
}

impl From<WriteError> for CharStringConversionError {
    fn from(err: WriteError) -> Self {
        CharStringConversionError::Write(err)
    }
}

impl fmt::Display for CharStringConversionError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            CharStringConversionError::Write(err) => {
                write!(f, "unable to convert charstring: {err}")
            }
            CharStringConversionError::Cff(err) => write!(f, "unable to convert charstring: {err}"),
        }
    }
}

impl std::error::Error for CharStringConversionError {}

impl CharStringVisitor<cff2::StackValue, CharStringConversionError> for CharStringConverter {
    fn visit(
        &mut self,
        op: VisitOp,
        stack: &ArgumentsStack<'_, cff2::StackValue>,
    ) -> Result<(), CharStringConversionError> {
        // The first stack-clearing operator, which must be one of hstem,
        // hstemhm, vstem, vstemhm, cntrmask, hintmask, hmoveto, vmoveto,
        // rmoveto, or endchar, takes an additional argument — the width (as
        // described earlier), which may be expressed as zero or one numeric
        // argument.

        // Width: If the charstring has a width other than that of
        // defaultWidthX (see Technical Note #5176, “The Compact
        // Font Format Specification”), it must be specified as the first
        // number in the charstring, and encoded as the difference
        // from nominalWidthX.
        match op {
            VisitOp::HorizontalStem
            | VisitOp::HorizontalStemHintMask
            | VisitOp::VerticalStem
            | VisitOp::VerticalStemHintMask
            | VisitOp::VerticalMoveTo
            | VisitOp::HintMask
            | VisitOp::CounterMask
            | VisitOp::MoveTo
            | VisitOp::HorizontalMoveTo => {
                self.maybe_write_width()?;
                cff2::write_stack(&mut self.buffer, stack)?;
                Ok(U8::write(&mut self.buffer, op)?)
            }

            VisitOp::Return | VisitOp::Endchar => {
                // Should not be encountered in CFF2
                Err(CFFError::InvalidOperator.into())
            }

            VisitOp::LineTo
            | VisitOp::HorizontalLineTo
            | VisitOp::VerticalLineTo
            | VisitOp::CurveTo
            | VisitOp::CurveLine
            | VisitOp::LineCurve
            | VisitOp::VvCurveTo
            | VisitOp::HhCurveTo
            | VisitOp::VhCurveTo
            | VisitOp::HvCurveTo => {
                if !self.width_written {
                    return Err(CFFError::MissingMoveTo.into());
                }
                cff2::write_stack(&mut self.buffer, stack)?;
                Ok(U8::write(&mut self.buffer, op)?)
            }

            VisitOp::Hflex | VisitOp::Flex | VisitOp::Hflex1 | VisitOp::Flex1 => {
                cff2::write_stack(&mut self.buffer, stack)?;
                U8::write(&mut self.buffer, TWO_BYTE_OPERATOR_MARK)?;
                Ok(U8::write(&mut self.buffer, op)?)
            }
            VisitOp::VsIndex | VisitOp::Blend => {
                // Should not be encountered when converting CharStrings. CharString should
                // not be variable.
                Err(CFFError::InvalidOperator.into())
            }
        }
    }

    fn enter_subr(&mut self, index: SubroutineIndex) -> Result<(), CharStringConversionError> {
        // Emit callsubr op
        match index {
            SubroutineIndex::Local(index) => {
                cff2::write_stack_value(
                    cff2::StackValue::Int(index.try_into().map_err(ParseError::from)?),
                    &mut self.buffer,
                )?;
                Ok(U8::write(
                    &mut self.buffer,
                    operator::CALL_LOCAL_SUBROUTINE,
                )?)
            }
            SubroutineIndex::Global(index) => {
                cff2::write_stack_value(
                    cff2::StackValue::Int(index.try_into().map_err(ParseError::from)?),
                    &mut self.buffer,
                )?;
                Ok(U8::write(
                    &mut self.buffer,
                    operator::CALL_GLOBAL_SUBROUTINE,
                )?)
            }
        }
    }

    fn exit_subr(&mut self) -> Result<(), CharStringConversionError> {
        Ok(U8::write(&mut self.buffer, operator::RETURN)?)
    }

    fn hint_data(&mut self, _op: VisitOp, hints: &[u8]) -> Result<(), CharStringConversionError> {
        Ok(self.buffer.write_bytes(hints)?)
    }
}

impl CharStringVisitor<f32, CFFError> for UsedSubrs {
    fn enter_subr(&mut self, index: SubroutineIndex) -> Result<(), CFFError> {
        match index {
            SubroutineIndex::Local(index) => self.local_subr_used.insert(index),
            SubroutineIndex::Global(index) => self.global_subr_used.insert(index),
        };

        Ok(())
    }
}

impl CharStringVisitor<f32, CFFError> for DebugVisitor {
    fn visit(&mut self, op: VisitOp, stack: &ArgumentsStack<'_, f32>) -> Result<(), CFFError> {
        let mut operands = String::new();
        stack.all().iter().enumerate().for_each(|(i, operand)| {
            if i > 0 {
                operands.push(' ')
            }
            write!(operands, "{}", operand).unwrap()
        });
        println!("{op} {operands}");
        Ok(())
    }

    fn enter_subr(&mut self, index: SubroutineIndex) -> Result<(), CFFError> {
        match index {
            SubroutineIndex::Local(index) => println!("callsubr {index}"),
            SubroutineIndex::Global(index) => println!("callgsubr {index}"),
        }
        Ok(())
    }
}

#[derive(Copy, Clone)]
pub enum VisitOp {
    HorizontalStem,
    VerticalStem,
    VerticalMoveTo,
    LineTo,
    HorizontalLineTo,
    VerticalLineTo,
    CurveTo,
    Return,
    Endchar,
    VsIndex,
    Blend,
    HorizontalStemHintMask,
    HintMask,
    CounterMask,
    MoveTo,
    HorizontalMoveTo,
    VerticalStemHintMask,
    CurveLine,
    LineCurve,
    VvCurveTo,
    HhCurveTo,
    VhCurveTo,
    HvCurveTo,
    Hflex,
    Flex,
    Hflex1,
    Flex1,
}

impl TryFrom<u8> for VisitOp {
    type Error = ();

    fn try_from(value: u8) -> Result<Self, Self::Error> {
        match value {
            operator::HORIZONTAL_STEM => Ok(VisitOp::HorizontalStem),
            operator::VERTICAL_STEM => Ok(VisitOp::VerticalStem),
            operator::VERTICAL_MOVE_TO => Ok(VisitOp::VerticalMoveTo),
            operator::LINE_TO => Ok(VisitOp::LineTo),
            operator::HORIZONTAL_LINE_TO => Ok(VisitOp::HorizontalLineTo),
            operator::VERTICAL_LINE_TO => Ok(VisitOp::VerticalLineTo),
            operator::CURVE_TO => Ok(VisitOp::CurveTo),
            // operator::CALL_LOCAL_SUBROUTINE not yielded as VisitOp
            operator::RETURN => Ok(VisitOp::Return),
            operator::ENDCHAR => Ok(VisitOp::Endchar),
            operator::VS_INDEX => Ok(VisitOp::VsIndex),
            operator::BLEND => Ok(VisitOp::Blend),
            operator::HORIZONTAL_STEM_HINT_MASK => Ok(VisitOp::HorizontalStemHintMask),
            operator::HINT_MASK => Ok(VisitOp::HintMask),
            operator::COUNTER_MASK => Ok(VisitOp::CounterMask),
            operator::MOVE_TO => Ok(VisitOp::MoveTo),
            operator::HORIZONTAL_MOVE_TO => Ok(VisitOp::HorizontalMoveTo),
            operator::VERTICAL_STEM_HINT_MASK => Ok(VisitOp::VerticalStemHintMask),
            operator::CURVE_LINE => Ok(VisitOp::CurveLine),
            operator::LINE_CURVE => Ok(VisitOp::LineCurve),
            operator::VV_CURVE_TO => Ok(VisitOp::VvCurveTo),
            operator::HH_CURVE_TO => Ok(VisitOp::HhCurveTo),
            // operator::SHORT_INT not yielded as VisitOp
            // operator::CALL_GLOBAL_SUBROUTINE not yielded as VisitOp
            operator::VH_CURVE_TO => Ok(VisitOp::VhCurveTo),
            operator::HV_CURVE_TO => Ok(VisitOp::HvCurveTo),
            // Flex operators are two-byte ops. This assumes the first byte has already been read
            operator::HFLEX => Ok(VisitOp::Hflex),
            operator::FLEX => Ok(VisitOp::Flex),
            operator::HFLEX1 => Ok(VisitOp::Hflex1),
            operator::FLEX1 => Ok(VisitOp::Flex1),
            // operator::FIXED_16_16 not yielded as VisitOp
            _ => Err(()),
        }
    }
}

impl From<VisitOp> for u8 {
    fn from(op: VisitOp) -> Self {
        match op {
            VisitOp::HorizontalStem => operator::HORIZONTAL_STEM,
            VisitOp::VerticalStem => operator::VERTICAL_STEM,
            VisitOp::VerticalMoveTo => operator::VERTICAL_MOVE_TO,
            VisitOp::LineTo => operator::LINE_TO,
            VisitOp::HorizontalLineTo => operator::HORIZONTAL_LINE_TO,
            VisitOp::VerticalLineTo => operator::VERTICAL_LINE_TO,
            VisitOp::CurveTo => operator::CURVE_TO,
            VisitOp::Return => operator::RETURN,
            VisitOp::Endchar => operator::ENDCHAR,
            VisitOp::VsIndex => operator::VS_INDEX,
            VisitOp::Blend => operator::BLEND,
            VisitOp::HorizontalStemHintMask => operator::HORIZONTAL_STEM_HINT_MASK,
            VisitOp::HintMask => operator::HINT_MASK,
            VisitOp::CounterMask => operator::COUNTER_MASK,
            VisitOp::MoveTo => operator::MOVE_TO,
            VisitOp::HorizontalMoveTo => operator::HORIZONTAL_MOVE_TO,
            VisitOp::VerticalStemHintMask => operator::VERTICAL_STEM_HINT_MASK,
            VisitOp::CurveLine => operator::CURVE_LINE,
            VisitOp::LineCurve => operator::LINE_CURVE,
            VisitOp::VvCurveTo => operator::VV_CURVE_TO,
            VisitOp::HhCurveTo => operator::HH_CURVE_TO,
            VisitOp::VhCurveTo => operator::VH_CURVE_TO,
            VisitOp::HvCurveTo => operator::HV_CURVE_TO,
            // Flex operators are two-byte ops. This returns the second byte
            VisitOp::Hflex => operator::HFLEX,
            VisitOp::Flex => operator::FLEX,
            VisitOp::Hflex1 => operator::HFLEX1,
            VisitOp::Flex1 => operator::FLEX1,
        }
    }
}

impl fmt::Display for VisitOp {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            VisitOp::HorizontalStem => f.write_str("hstem"),
            VisitOp::VerticalStem => f.write_str("vstem"),
            VisitOp::VerticalMoveTo => f.write_str("vmoveto"),
            VisitOp::LineTo => f.write_str("rlineto"),
            VisitOp::HorizontalLineTo => f.write_str("hlineto"),
            VisitOp::VerticalLineTo => f.write_str("vlineto"),
            VisitOp::CurveTo => f.write_str("rrcurveto"),
            VisitOp::Return => f.write_str("return"),
            VisitOp::Endchar => f.write_str("endchar"),
            VisitOp::VsIndex => f.write_str("vsindex"),
            VisitOp::Blend => f.write_str("blend"),
            VisitOp::HorizontalStemHintMask => f.write_str("hstemhm"),
            VisitOp::HintMask => f.write_str("hintmask"),
            VisitOp::CounterMask => f.write_str("cntrmask"),
            VisitOp::MoveTo => f.write_str("rmoveto"),
            VisitOp::HorizontalMoveTo => f.write_str("hmoveto"),
            VisitOp::VerticalStemHintMask => f.write_str("vstemhm"),
            VisitOp::CurveLine => f.write_str("rcurveline"),
            VisitOp::LineCurve => f.write_str("rlinecurve"),
            VisitOp::VvCurveTo => f.write_str("vvcurveto"),
            VisitOp::HhCurveTo => f.write_str("hhcurveto"),
            VisitOp::VhCurveTo => f.write_str("vhcurveto"),
            VisitOp::HvCurveTo => f.write_str("hvcurveto"),
            VisitOp::Hflex => f.write_str("hflex"),
            VisitOp::Flex => f.write_str("flex"),
            VisitOp::Hflex1 => f.write_str("hflex1"),
            VisitOp::Flex1 => f.write_str("flex1"),
        }
    }
}

impl<'a, 'data> CharStringVisitorContext<'a, 'data> {
    /// Construct a new context for traversing a CharString.
    ///
    /// Used with a `CharStringVisitor` impl supplied to `visit` to traverse the CharString.
    pub fn new(
        glyph_id: GlyphId,
        char_strings_index: &'a MaybeOwnedIndex<'data>,
        local_subr_index: Option<&'a MaybeOwnedIndex<'data>>,
        global_subr_index: &'a MaybeOwnedIndex<'data>,
        variable: Option<VariableCharStringVisitorContext<'a, 'data>>,
    ) -> CharStringVisitorContext<'a, 'data> {
        CharStringVisitorContext {
            glyph_id,
            char_strings_index,
            local_subr_index,
            global_subr_index,
            variable,
            width_parsed: false,
            stems_len: 0,
            has_endchar: false,
            has_seac: false,
            seen_blend: false,
            vsindex: None,
            scalars: None,
        }
    }

    /// Visit the operators of the CharString with a `CharStringVisitor` implementation.
    pub fn visit<S, V, E>(
        &mut self,
        font: CFFFont<'a, 'data>,
        stack: &mut ArgumentsStack<'_, S>,
        visitor: &mut V,
    ) -> Result<(), E>
    where
        V: CharStringVisitor<S, E>,
        S: BlendOperand,
        E: std::error::Error + From<CFFError> + From<ParseError>,
    {
        let char_string = self
            .char_strings_index
            .read_object(usize::from(self.glyph_id))
            .ok_or(CFFError::ParseError(ParseError::BadIndex))?;
        self.visit_impl(font, char_string, 0, stack, visitor)
    }

    fn visit_impl<S, V, E>(
        &mut self,
        font: CFFFont<'a, 'data>,
        char_string: &[u8],
        depth: u8,
        stack: &mut ArgumentsStack<'_, S>,
        visitor: &mut V,
    ) -> Result<(), E>
    where
        V: CharStringVisitor<S, E>,
        S: BlendOperand,
        E: std::error::Error + From<CFFError> + From<ParseError>,
    {
        let mut s = ReadScope::new(char_string).ctxt();
        while s.bytes_available() {
            let op = s.read::<U8>()?;
            match op {
                0 | 2 | 9 | 13 | 17 => {
                    // Reserved.
                    return Err(CFFError::InvalidOperator.into());
                }
                operator::HORIZONTAL_STEM
                | operator::VERTICAL_STEM
                | operator::HORIZONTAL_STEM_HINT_MASK
                | operator::VERTICAL_STEM_HINT_MASK => {
                    // If the stack length is uneven, then the first value is a `width`.
                    let len = if stack.len().is_odd() && !self.width_parsed {
                        self.width_parsed = true;
                        stack.len() - 1
                    } else {
                        stack.len()
                    };

                    self.stems_len += len as u32 >> 1;

                    // We are ignoring the hint operators.
                    visitor.visit(op.try_into().unwrap(), stack)?;
                    stack.clear();
                }
                operator::VERTICAL_MOVE_TO => {
                    let offset = self.handle_width(stack.len() == 2 && !self.width_parsed);
                    stack.offset(offset, |stack| visitor.visit(op.try_into().unwrap(), stack))?;
                    stack.clear();
                }
                operator::LINE_TO
                | operator::HORIZONTAL_LINE_TO
                | operator::VERTICAL_LINE_TO
                | operator::CURVE_TO => {
                    visitor.visit(op.try_into().unwrap(), stack)?;
                    stack.clear();
                }
                operator::CALL_LOCAL_SUBROUTINE => {
                    if stack.is_empty() {
                        return Err(CFFError::InvalidArgumentsStackLength.into());
                    }

                    if depth == STACK_LIMIT {
                        return Err(CFFError::NestingLimitReached.into());
                    }

                    // Parse and remember the local subroutine for the current glyph.
                    // Since it's a pretty complex task, we're doing it only when
                    // a local subroutine is actually requested by the glyphs charstring.
                    if self.local_subr_index.is_none() {
                        // Only match on this as the other variants were populated at the beginning of the function
                        if let CFFFont::CFF(super::Font {
                            data: CFFVariant::CID(ref cid),
                            ..
                        }) = font
                        {
                            // Choose the local subroutine index corresponding to the glyph/CID
                            self.local_subr_index = cid
                                .fd_select
                                .font_dict_index(self.glyph_id)
                                .and_then(|font_dict_index| {
                                    match cid.local_subr_indices.get(usize::from(font_dict_index)) {
                                        Some(Some(index)) => Some(index),
                                        _ => None,
                                    }
                                });
                        }
                    }

                    if let Some(local_subrs) = self.local_subr_index {
                        let subroutine_bias = calc_subroutine_bias(local_subrs.len());
                        let index = conv_subroutine_index(stack.pop(), subroutine_bias)?;
                        let char_string = local_subrs
                            .read_object(index)
                            .ok_or(CFFError::InvalidSubroutineIndex)?;
                        visitor.enter_subr(SubroutineIndex::Local(index))?;
                        self.visit_impl(font, char_string, depth + 1, stack, visitor)?;
                        visitor.exit_subr()?;
                    } else {
                        return Err(CFFError::NoLocalSubroutines.into());
                    }

                    if self.has_endchar && !self.has_seac {
                        if s.bytes_available() {
                            return Err(CFFError::DataAfterEndChar.into());
                        }

                        break;
                    }
                }
                operator::RETURN => {
                    match font {
                        CFFFont::CFF(_) => {
                            visitor.visit(op.try_into().unwrap(), stack)?;
                            break;
                        }
                        CFFFont::CFF2(_) => {
                            // Removed in CFF2
                            return Err(CFFError::InvalidOperator.into());
                        }
                    }
                }
                TWO_BYTE_OPERATOR_MARK => {
                    // flex
                    let op2 = s.read::<U8>()?;
                    match op2 {
                        operator::HFLEX | operator::FLEX | operator::HFLEX1 | operator::FLEX1 => {
                            visitor.visit(op2.try_into().unwrap(), stack)?;
                            stack.clear()
                        }
                        _ => return Err(CFFError::UnsupportedOperator.into()),
                    }
                }
                operator::ENDCHAR => {
                    match font {
                        CFFFont::CFF(cff) => {
                            if stack.len() == 4 || (!self.width_parsed && stack.len() == 5) {
                                // Process 'seac'.
                                let accent_char = stack
                                    .pop()
                                    .try_as_u8()
                                    .and_then(|code| cff.seac_code_to_glyph_id(code))
                                    .ok_or(CFFError::InvalidSeacCode)?;
                                let base_char = stack
                                    .pop()
                                    .try_as_u8()
                                    .and_then(|code| cff.seac_code_to_glyph_id(code))
                                    .ok_or(CFFError::InvalidSeacCode)?;
                                let dy = stack.pop();
                                let dx = stack.pop();

                                if !self.width_parsed {
                                    stack.pop();
                                    self.width_parsed = true;
                                }

                                self.has_seac = true;

                                let base_char_string = self
                                    .char_strings_index
                                    .read_object(usize::from(base_char))
                                    .ok_or(CFFError::InvalidSeacCode)?;
                                visitor.enter_seac(SeacChar::Base, dx, dy)?;
                                self.visit_impl(font, base_char_string, depth + 1, stack, visitor)?;
                                visitor.exit_seac(SeacChar::Base)?;

                                let accent_char_string = self
                                    .char_strings_index
                                    .read_object(usize::from(accent_char))
                                    .ok_or(CFFError::InvalidSeacCode)?;
                                visitor.enter_seac(SeacChar::Accent, dx, dy)?;
                                self.visit_impl(
                                    font,
                                    accent_char_string,
                                    depth + 1,
                                    stack,
                                    visitor,
                                )?;
                                visitor.exit_seac(SeacChar::Accent)?;
                            } else if stack.len() == 1 && !self.width_parsed {
                                stack.pop();
                                self.width_parsed = true;
                            }

                            if s.bytes_available() {
                                return Err(CFFError::DataAfterEndChar.into());
                            }

                            self.has_endchar = true;
                            visitor.visit(op.try_into().unwrap(), stack)?;
                            break;
                        }
                        CFFFont::CFF2(_) => {
                            // Removed in CFF2
                            return Err(CFFError::InvalidOperator.into());
                        }
                    }
                }
                operator::VS_INDEX => {
                    match font {
                        CFFFont::CFF(_) => {
                            // Added in CFF2
                            return Err(CFFError::InvalidOperator.into());
                        }
                        CFFFont::CFF2(_) => {
                            // When used, vsindex must precede the first blend operator,
                            // and may occur only once in the CharString.
                            if self.vsindex.is_some() {
                                return Err(CFFError::DuplicateVsIndex.into());
                            } else if self.seen_blend {
                                return Err(CFFError::VsIndexAfterBlend.into());
                            } else {
                                if stack.len() != 1 {
                                    return Err(CFFError::InvalidArgumentsStackLength.into());
                                }
                                visitor.visit(op.try_into().unwrap(), stack)?;
                                let item_variation_data_index = stack
                                    .pop()
                                    .try_as_u16()
                                    .ok_or(CFFError::InvalidArgumentsStackLength)?;
                                self.vsindex = Some(item_variation_data_index);
                            }
                        }
                    }
                }
                operator::BLEND => {
                    match font {
                        CFFFont::CFF(_) => {
                            // Added in CFF2
                            return Err(CFFError::InvalidOperator.into());
                        }
                        CFFFont::CFF2(font) => {
                            let Some(var) = self.variable else {
                                return Err(CFFError::MissingVariationStore.into());
                            };

                            if !stack.is_empty() {
                                visitor.visit(op.try_into().unwrap(), stack)?;

                                // Lookup the ItemVariationStore data to get the variation regions
                                let scalars = match &self.scalars {
                                    Some(scalars) => scalars,
                                    None => {
                                        let vs_index =
                                            self.vsindex.map(Ok).unwrap_or_else(|| {
                                                font.private_dict
                                                    .get_i32(Operator::VSIndex)
                                                    .ok_or(ParseError::BadValue)?
                                                    .and_then(|val| {
                                                        u16::try_from(val).map_err(ParseError::from)
                                                    })
                                            })?;

                                        self.scalars = Some(cff2::scalars(
                                            vs_index,
                                            var.vstore,
                                            var.instance,
                                        )?);
                                        self.scalars.as_ref().unwrap()
                                    }
                                };

                                cff2::blend(scalars, stack)?;
                            } else {
                                return Err(CFFError::InvalidArgumentsStackLength.into());
                            }
                        }
                    }
                }
                operator::HINT_MASK | operator::COUNTER_MASK => {
                    let mut len = stack.len();
                    let visit_op = op.try_into().unwrap();
                    visitor.visit(visit_op, stack)?;
                    stack.clear();

                    // If the stack length is uneven, then the first value is a `width`.
                    if len.is_odd() && !self.width_parsed {
                        len -= 1;
                        self.width_parsed = true;
                    }

                    self.stems_len += len as u32 >> 1;

                    // Yield the hints
                    let hints = s
                        .read_slice(
                            usize::try_from((self.stems_len + 7) >> 3)
                                .map_err(|_| ParseError::BadValue)?,
                        )
                        .map_err(|_| ParseError::BadOffset)?;
                    visitor.hint_data(visit_op, hints)?;
                }
                operator::MOVE_TO => {
                    let offset = self.handle_width(stack.len() == 3 && !self.width_parsed);
                    stack.offset(offset, |stack| visitor.visit(op.try_into().unwrap(), stack))?;
                    stack.clear();
                }
                operator::HORIZONTAL_MOVE_TO => {
                    let offset = self.handle_width(stack.len() == 2 && !self.width_parsed);
                    stack.offset(offset, |stack| visitor.visit(op.try_into().unwrap(), stack))?;
                    stack.clear();
                }
                operator::CURVE_LINE
                | operator::LINE_CURVE
                | operator::VV_CURVE_TO
                | operator::HH_CURVE_TO
                | operator::VH_CURVE_TO
                | operator::HV_CURVE_TO => {
                    visitor.visit(op.try_into().unwrap(), stack)?;
                    stack.clear();
                }
                operator::SHORT_INT => {
                    let n = s.read::<I16Be>()?;
                    stack.push(S::from(n))?;
                }
                operator::CALL_GLOBAL_SUBROUTINE => {
                    if stack.is_empty() {
                        return Err(CFFError::InvalidArgumentsStackLength.into());
                    }

                    if depth == STACK_LIMIT {
                        return Err(CFFError::NestingLimitReached.into());
                    }

                    let subroutine_bias = calc_subroutine_bias(self.global_subr_index.len());
                    let index = conv_subroutine_index(stack.pop(), subroutine_bias)?;
                    let char_string = self
                        .global_subr_index
                        .read_object(index)
                        .ok_or(CFFError::InvalidSubroutineIndex)?;
                    visitor.enter_subr(SubroutineIndex::Global(index))?;
                    self.visit_impl(font, char_string, depth + 1, stack, visitor)?;
                    visitor.exit_subr()?;

                    if self.has_endchar && !self.has_seac {
                        if s.bytes_available() {
                            return Err(CFFError::DataAfterEndChar.into());
                        }

                        break;
                    }
                }
                32..=246 => {
                    stack.push(parse_int1(op)?)?;
                }
                247..=250 => {
                    stack.push(parse_int2(op, &mut s)?)?;
                }
                251..=254 => {
                    stack.push(parse_int3(op, &mut s)?)?;
                }
                operator::FIXED_16_16 => {
                    stack.push(parse_fixed(&mut s)?)?;
                }
            }
        }

        Ok(())
    }

    fn handle_width(&mut self, cond: bool) -> usize {
        if cond {
            self.width_parsed = true;
            1
        } else {
            0
        }
    }
}

// CharString number parsing functions
fn parse_int1<S: BlendOperand>(op: u8) -> Result<S, CFFError> {
    let n = i16::from(op) - 139;
    Ok(S::from(n))
}

fn parse_int2<S: BlendOperand>(op: u8, s: &mut ReadCtxt<'_>) -> Result<S, CFFError> {
    let b1 = s.read::<U8>()?;
    let n = (i16::from(op) - 247) * 256 + i16::from(b1) + 108;
    debug_assert!((108..=1131).contains(&n));
    Ok(S::from(n))
}

fn parse_int3<S: BlendOperand>(op: u8, s: &mut ReadCtxt<'_>) -> Result<S, CFFError> {
    let b1 = s.read::<U8>()?;
    let n = -(i16::from(op) - 251) * 256 - i16::from(b1) - 108;
    debug_assert!((-1131..=-108).contains(&n));
    Ok(S::from(n))
}

fn parse_fixed<S: BlendOperand>(s: &mut ReadCtxt<'_>) -> Result<S, CFFError> {
    let n = s.read::<Fixed>()?;
    Ok(S::from(n))
}

// Conversions from biased subr index operands to unbiased value
pub(crate) fn conv_subroutine_index<S: BlendOperand>(
    index: S,
    bias: u16,
) -> Result<usize, CFFError> {
    let index = index.try_as_i32().ok_or(CFFError::InvalidSubroutineIndex)?;
    conv_subroutine_index_impl(index, bias).ok_or(CFFError::InvalidSubroutineIndex)
}

pub(crate) fn conv_subroutine_index_impl(index: i32, bias: u16) -> Option<usize> {
    let bias = i32::from(bias);

    let index = index.checked_add(bias)?;
    usize::try_from(index).ok()
}

// Adobe Technical Note #5176, Chapter 16 "Local / Global Subrs INDEXes"
pub(crate) fn calc_subroutine_bias(len: usize) -> u16 {
    if len < 1240 {
        107
    } else if len < 33900 {
        1131
    } else {
        32768
    }
}

impl IsEven for usize {
    fn is_even(&self) -> bool {
        (*self) & 1 == 0
    }

    fn is_odd(&self) -> bool {
        !self.is_even()
    }
}

impl From<ParseError> for CFFError {
    fn from(error: ParseError) -> CFFError {
        CFFError::ParseError(error)
    }
}

impl fmt::Display for CFFError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            CFFError::ParseError(parse_error) => {
                write!(f, "parse error: ")?;
                parse_error.fmt(f)
            }
            CFFError::InvalidOperator => write!(f, "an invalid operator occurred"),
            CFFError::UnsupportedOperator => write!(f, "an unsupported operator occurred"),
            CFFError::MissingEndChar => write!(f, "the 'endchar' operator is missing"),
            CFFError::DataAfterEndChar => write!(f, "unused data left after 'endchar' operator"),
            CFFError::NestingLimitReached => write!(f, "subroutines nesting limit reached"),
            CFFError::ArgumentsStackLimitReached => write!(f, "arguments stack limit reached"),
            CFFError::InvalidArgumentsStackLength => {
                write!(f, "an invalid amount of items are in an arguments stack")
            }
            CFFError::BboxOverflow => write!(f, "outline's bounding box is too large"),
            CFFError::MissingMoveTo => write!(f, "missing moveto operator"),
            CFFError::DuplicateVsIndex => write!(f, "duplicate vsindex operator"),
            CFFError::InvalidSubroutineIndex => write!(f, "an invalid subroutine index"),
            CFFError::NoLocalSubroutines => write!(f, "no local subroutines"),
            CFFError::InvalidSeacCode => write!(f, "invalid seac code"),
            CFFError::InvalidOperand => write!(f, "operand was out of range or invalid"),
            CFFError::InvalidFontIndex => write!(f, "invalid font index"),
            CFFError::VsIndexAfterBlend => write!(f, "vsindex operator encountered after blend"),
            CFFError::MissingVariationStore => write!(f, "missing variation store"),
        }
    }
}

impl std::error::Error for CFFError {}

/// Operators defined in Adobe Technical Note #5177, The Type  2 Charstring Format.
pub(crate) mod operator {
    pub const HORIZONTAL_STEM: u8 = 1;
    pub const VERTICAL_STEM: u8 = 3;
    pub const VERTICAL_MOVE_TO: u8 = 4;
    pub const LINE_TO: u8 = 5;
    pub const HORIZONTAL_LINE_TO: u8 = 6;
    pub const VERTICAL_LINE_TO: u8 = 7;
    pub const CURVE_TO: u8 = 8;
    pub const CALL_LOCAL_SUBROUTINE: u8 = 10;
    pub const RETURN: u8 = 11;
    pub const ENDCHAR: u8 = 14;
    pub const VS_INDEX: u8 = 15; // CFF2
    pub const BLEND: u8 = 16; // CFF2
    pub const HORIZONTAL_STEM_HINT_MASK: u8 = 18;
    pub const HINT_MASK: u8 = 19;
    pub const COUNTER_MASK: u8 = 20;
    pub const MOVE_TO: u8 = 21;
    pub const HORIZONTAL_MOVE_TO: u8 = 22;
    pub const VERTICAL_STEM_HINT_MASK: u8 = 23;
    pub const CURVE_LINE: u8 = 24;
    pub const LINE_CURVE: u8 = 25;
    pub const VV_CURVE_TO: u8 = 26;
    pub const HH_CURVE_TO: u8 = 27;
    pub const SHORT_INT: u8 = 28;
    pub const CALL_GLOBAL_SUBROUTINE: u8 = 29;
    pub const VH_CURVE_TO: u8 = 30;
    pub const HV_CURVE_TO: u8 = 31;
    pub const HFLEX: u8 = 34;
    pub const FLEX: u8 = 35;
    pub const HFLEX1: u8 = 36;
    pub const FLEX1: u8 = 37;
    pub const FIXED_16_16: u8 = 255;
}

#[cfg(test)]
mod tests {
    use crate::cff::cff2::{self, CFF2};
    use crate::error::ReadWriteError;
    use crate::tables::variable_fonts::avar::AvarTable;
    use crate::tables::variable_fonts::fvar::FvarTable;
    use crate::tables::{OpenTypeData, OpenTypeFont};
    use crate::tag;
    use crate::tests::read_fixture;

    use super::*;

    struct TraverseCharString;

    impl CharStringVisitor<f32, CFFError> for TraverseCharString {}

    #[test]
    fn traverse_cff2_charstring() -> Result<(), ReadWriteError> {
        let buffer = read_fixture("tests/fonts/opentype/cff2/SourceSansVariable-Roman.abc.otf");
        let otf = ReadScope::new(&buffer).read::<OpenTypeFont<'_>>().unwrap();

        let offset_table = match otf.data {
            OpenTypeData::Single(ttf) => ttf,
            OpenTypeData::Collection(_) => unreachable!(),
        };

        let cff_table_data = offset_table.read_table(&otf.scope, tag::CFF2)?.unwrap();
        let cff = cff_table_data
            .read::<CFF2<'_>>()
            .expect("error parsing CFF2 table");
        let fvar_data = offset_table.read_table(&otf.scope, tag::FVAR)?.unwrap();
        let fvar = fvar_data.read::<FvarTable<'_>>()?;
        let avar_data = offset_table.read_table(&otf.scope, tag::AVAR)?;
        let avar = avar_data
            .as_ref()
            .map(|avar_data| avar_data.read::<AvarTable<'_>>())
            .transpose()?;

        // Traverse a CharString
        let glyph_id = 1;
        let font_dict_index = cff
            .fd_select
            .map(|fd_select| fd_select.font_dict_index(glyph_id).unwrap())
            .unwrap_or(0);
        let font = &cff.fonts[usize::from(font_dict_index)];

        let user_tuple = [Fixed::from(650.0)];
        let instance = fvar
            .normalize(user_tuple.iter().copied(), avar.as_ref())
            .unwrap();

        let variable = VariableCharStringVisitorContext {
            vstore: cff.vstore.as_ref().unwrap(),
            instance: &instance,
        };
        let mut ctx = CharStringVisitorContext::new(
            1,
            &cff.char_strings_index,
            font.local_subr_index.as_ref(),
            &cff.global_subr_index,
            Some(variable),
        );
        let mut stack = ArgumentsStack {
            data: &mut [0.0; cff2::MAX_OPERANDS],
            len: 0,
            max_len: cff2::MAX_OPERANDS,
        };
        let res = ctx.visit(CFFFont::CFF2(font), &mut stack, &mut TraverseCharString);
        assert!(res.is_ok());
        Ok(())
    }
}
