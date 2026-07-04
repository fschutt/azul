//! Glyph outline generation for CFF.

// Portions of this file derived from ttf-parser, licenced under Apache-2.0.
// https://github.com/RazrFalcon/ttf-parser/tree/439aaaebd50eb8aed66302e3c1b51fae047f85b2

use pathfinder_geometry::line_segment::LineSegment2F;
use pathfinder_geometry::vector::vec2f;

use cff2::CFF2;
use charstring::CharStringParser;

use crate::cff;
use crate::error::ParseError;
use crate::outline::{BoundingBoxSink, OutlineBuilder, OutlineSink};
use crate::tables::glyf::BoundingBox;
use crate::tables::variable_fonts::OwnedTuple;

use super::charstring::{
    ArgumentsStack, CharStringVisitor, CharStringVisitorContext, SeacChar,
    VariableCharStringVisitorContext, VisitOp,
};
use super::{cff2, CFFError, CFFFont, CFFVariant, CFF};

mod charstring;

pub(crate) struct Builder<'a, B>
where
    B: OutlineSink,
{
    builder: &'a mut B,
    bbox: BoundingBoxSink,
}

pub struct CFFOutlines<'a, 'data> {
    pub table: &'a CFF<'data>,
}

pub struct CFF2Outlines<'a, 'data> {
    pub table: &'a CFF2<'data>,
}

impl<B> Builder<'_, B>
where
    B: OutlineSink,
{
    fn move_to(&mut self, x: f32, y: f32) {
        let point = vec2f(x, y);
        self.bbox.move_to(point);
        self.builder.move_to(point);
    }

    fn line_to(&mut self, x: f32, y: f32) {
        let point = vec2f(x, y);
        self.bbox.line_to(point);
        self.builder.line_to(point);
    }

    fn curve_to(&mut self, x1: f32, y1: f32, x2: f32, y2: f32, x: f32, y: f32) {
        let ctrl = LineSegment2F::new(vec2f(x1, y1), vec2f(x2, y2));
        let to = vec2f(x, y);
        self.bbox.cubic_curve_to(ctrl, to);
        self.builder.cubic_curve_to(ctrl, to);
    }

    fn close(&mut self) {
        self.bbox.close();
        self.builder.close();
    }
}

impl OutlineBuilder for CFFOutlines<'_, '_> {
    type Error = CFFError;
    type Output = Option<BoundingBox>;

    fn visit<S: OutlineSink>(
        &mut self,
        glyph_index: u16,
        _tuple: Option<&OwnedTuple>,
        sink: &mut S,
    ) -> Result<Self::Output, Self::Error> {
        let font = self.table.fonts.first().ok_or(ParseError::MissingValue)?;
        let local_subrs = match &font.data {
            CFFVariant::CID(_) => None, // local subrs will be resolved on request.
            CFFVariant::Type1(type1) => type1.local_subr_index.as_ref(),
        };

        let ctx = CharStringVisitorContext::new(
            glyph_index,
            &font.char_strings_index,
            local_subrs,
            &self.table.global_subr_index,
            None,
        );
        let mut stack = ArgumentsStack {
            data: &mut [0.0; cff::MAX_OPERANDS],
            len: 0,
            max_len: cff::MAX_OPERANDS,
        };

        parse_char_string(CFFFont::CFF(font), ctx, &mut stack, sink)
    }
}

impl OutlineBuilder for CFF2Outlines<'_, '_> {
    type Error = CFFError;
    type Output = Option<BoundingBox>;

    fn visit<S: OutlineSink>(
        &mut self,
        glyph_index: u16,
        tuple: Option<&OwnedTuple>,
        sink: &mut S,
    ) -> Result<Self::Output, Self::Error> {
        let font = self.table.fonts.first().ok_or(ParseError::MissingValue)?;

        let variable = tuple
            .map(|tuple| {
                let vstore = self
                    .table
                    .vstore
                    .as_ref()
                    .ok_or(CFFError::MissingVariationStore)?;
                Ok::<_, CFFError>(VariableCharStringVisitorContext {
                    vstore,
                    instance: tuple,
                })
            })
            .transpose()?;

        let ctx = CharStringVisitorContext::new(
            glyph_index,
            &self.table.char_strings_index,
            font.local_subr_index.as_ref(),
            &self.table.global_subr_index,
            variable,
        );

        let mut stack = ArgumentsStack {
            data: &mut [0.0; cff2::MAX_OPERANDS],
            len: 0,
            max_len: cff2::MAX_OPERANDS,
        };

        parse_char_string(CFFFont::CFF2(font), ctx, &mut stack, sink)
    }
}

fn parse_char_string<'a, 'data, B: OutlineSink>(
    font: CFFFont<'a, 'data>,
    mut context: CharStringVisitorContext<'a, 'data>,
    stack: &mut ArgumentsStack<'a, f32>,
    builder: &mut B,
) -> Result<Option<BoundingBox>, CFFError> {
    let mut inner_builder = Builder {
        builder,
        bbox: BoundingBoxSink::new(),
    };

    let mut parser = CharStringParser {
        builder: &mut inner_builder,
        x: 0.0,
        y: 0.0,
        has_move_to: false,
        is_first_move_to: true,
        temp: [0.0; cff::MAX_OPERANDS],
    };

    context.visit(font, stack, &mut parser)?;

    if font.is_cff2() {
        // > CFF2 CharStrings differ from Type 2 CharStrings in that there is no operator for
        // > finishing a CharString outline definition.The end of the CharString byte string
        // > implies the end of the last subpath, and serves the same purpose as the Type 2
        // > endchar operator.
        parser.builder.close();
    }

    let bbox = parser.builder.bbox.bbox();
    if bbox.is_default() {
        return Ok(None);
    }

    bbox.to_bounding_box()
        .ok_or(CFFError::BboxOverflow)
        .map(Some)
}

impl<B: OutlineSink> CharStringVisitor<f32, CFFError> for CharStringParser<'_, B> {
    fn visit(&mut self, op: VisitOp, stack: &ArgumentsStack<'_, f32>) -> Result<(), CFFError> {
        match op {
            VisitOp::HorizontalStem
            | VisitOp::VerticalStem
            | VisitOp::HorizontalStemHintMask
            | VisitOp::VerticalStemHintMask => {
                // We are ignoring the hint operators.
                Ok(())
            }
            VisitOp::VerticalMoveTo => self.parse_vertical_move_to(stack),
            VisitOp::LineTo => self.parse_line_to(stack),
            VisitOp::HorizontalLineTo => self.parse_horizontal_line_to(stack),
            VisitOp::VerticalLineTo => self.parse_vertical_line_to(stack),
            VisitOp::CurveTo => self.parse_curve_to(stack),
            VisitOp::Return => Ok(()),
            VisitOp::Endchar => {
                if !self.is_first_move_to {
                    self.is_first_move_to = true;
                    self.builder.close();
                }

                Ok(())
            }
            VisitOp::HintMask | VisitOp::CounterMask => Ok(()),
            VisitOp::MoveTo => self.parse_move_to(stack),
            VisitOp::HorizontalMoveTo => self.parse_horizontal_move_to(stack),
            VisitOp::CurveLine => self.parse_curve_line(stack),
            VisitOp::LineCurve => self.parse_line_curve(stack),
            VisitOp::VvCurveTo => self.parse_vv_curve_to(stack),
            VisitOp::HhCurveTo => self.parse_hh_curve_to(stack),
            VisitOp::VhCurveTo => self.parse_vh_curve_to(stack),
            VisitOp::HvCurveTo => self.parse_hv_curve_to(stack),
            VisitOp::VsIndex | VisitOp::Blend => {
                // Handled by the CharStringVisitor
                Ok(())
            }
            VisitOp::Hflex => self.parse_hflex(stack),
            VisitOp::Flex => self.parse_flex(stack),
            VisitOp::Hflex1 => self.parse_hflex1(stack),
            VisitOp::Flex1 => self.parse_flex1(stack),
        }
    }

    fn enter_seac(&mut self, seac: SeacChar, dx: f32, dy: f32) -> Result<(), CFFError> {
        if seac == SeacChar::Accent {
            self.x = dx;
            self.y = dy;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use crate::binary::read::ReadScope;
    use pathfinder_geometry::rect::RectI;
    use pathfinder_geometry::vector::{vec2i, Vector2F, Vector2I};
    use std::fmt::Write;
    use std::marker::PhantomData;

    use crate::binary::write::{WriteBinary, WriteBuffer};
    use crate::cff::charstring::operator;
    use crate::cff::{
        Charset, Encoding, Header, Index, MaybeOwnedIndex, Operand, Operator, PrivateDict, TopDict,
        Type1Data,
    };
    use crate::tests::writer::{self, TtfType::*};

    use super::*;

    struct Builder(String);

    impl OutlineSink for Builder {
        fn move_to(&mut self, to: Vector2F) {
            write!(&mut self.0, "M {} {} ", to.x(), to.y()).unwrap();
        }

        fn line_to(&mut self, to: Vector2F) {
            write!(&mut self.0, "L {} {} ", to.x(), to.y()).unwrap();
        }

        fn quadratic_curve_to(&mut self, ctrl: Vector2F, to: Vector2F) {
            write!(
                &mut self.0,
                "Q {} {} {} {} ",
                ctrl.x(),
                ctrl.y(),
                to.x(),
                to.y()
            )
            .unwrap();
        }

        fn cubic_curve_to(&mut self, ctrl: LineSegment2F, to: Vector2F) {
            write!(
                &mut self.0,
                "C {} {} {} {} {} {} ",
                ctrl.from().x(),
                ctrl.from().y(),
                ctrl.to().x(),
                ctrl.to().y(),
                to.x(),
                to.y()
            )
            .unwrap();
        }

        fn close(&mut self) {
            write!(&mut self.0, "Z ").unwrap();
        }
    }

    fn gen_cff(
        global_subrs: &[&[writer::TtfType]],
        local_subrs: &[&[writer::TtfType]],
        chars: &[writer::TtfType],
    ) -> Vec<u8> {
        fn gen_subrs(subrs: &[&[writer::TtfType]]) -> Vec<u8> {
            let mut w = writer::Writer::new();
            for v1 in subrs {
                for v2 in v1.iter() {
                    w.write(*v2);
                }
            }
            w.data
        }

        // TODO: support multiple subrs
        assert!(global_subrs.len() <= 1);
        assert!(local_subrs.len() <= 1);

        let global_subrs_data = gen_subrs(global_subrs);
        let local_subrs_data = gen_subrs(local_subrs);
        let chars_data = writer::convert(chars);

        // FIXME: Explain xxx
        assert!(global_subrs_data.len() < 255);
        assert!(local_subrs_data.len() < 255);
        assert!(chars_data.len() < 255);

        // Header
        let header = Header {
            major: 1,
            minor: 0,
            hdr_size: 4,
            off_size: 1,
        };

        let font_name = b"Test Font";
        let name_index = Index {
            count: 1,
            off_size: 1,
            offset_array: &[1, font_name.len() as u8 + 1],
            data_array: font_name,
        };

        let top_dict = TopDict {
            dict: vec![
                (Operator::CharStrings, vec![Operand::Offset(0)]), // offset filled in when writing
                (
                    Operator::Private,
                    vec![Operand::Offset(0), Operand::Offset(0)],
                ), // offsets are filled in when writing
            ],
            default: PhantomData,
        };

        let string_index = MaybeOwnedIndex::Borrowed(Index {
            count: 0,
            off_size: 0,
            offset_array: &[],
            data_array: &[],
        });

        let global_subr_index = Index {
            count: if global_subrs_data.is_empty() { 0 } else { 1 },
            off_size: 1,
            offset_array: &[1, global_subrs_data.len() as u8 + 1],
            data_array: &global_subrs_data,
        };

        let char_strings_index = Index {
            count: 1,
            off_size: 1,
            offset_array: &[1, chars_data.len() as u8 + 1],
            data_array: &chars_data,
        };

        let local_subrs_index = Index {
            count: if local_subrs_data.is_empty() { 0 } else { 1 },
            off_size: 1,
            offset_array: &[1, local_subrs_data.len() as u8 + 1],
            data_array: &local_subrs_data,
        };

        let (private_dict_data, local_subr_index) = if !local_subrs_data.is_empty() {
            (
                vec![
                    (Operator::Subrs, vec![Operand::Offset(0)]), // offset filled in when writing
                ],
                Some(MaybeOwnedIndex::Borrowed(local_subrs_index)),
            )
        } else {
            (Vec::new(), None)
        };

        let private_dict = PrivateDict {
            dict: private_dict_data,
            default: PhantomData,
        };

        let cff = CFF {
            header,
            name_index: MaybeOwnedIndex::Borrowed(name_index),
            string_index,
            global_subr_index: MaybeOwnedIndex::Borrowed(global_subr_index),
            fonts: vec![cff::Font {
                top_dict,
                char_strings_index: MaybeOwnedIndex::Borrowed(char_strings_index),
                charset: Charset::ISOAdobe,
                data: CFFVariant::Type1(Type1Data {
                    encoding: Encoding::Standard,
                    private_dict,
                    local_subr_index,
                }),
            }],
        };

        let mut w = WriteBuffer::new();
        CFF::write(&mut w, &cff).unwrap();
        w.into_inner()
    }

    fn rect(x_min: i16, y_min: i16, x_max: i16, y_max: i16) -> RectI {
        RectI::from_points(
            vec2i(i32::from(x_min), i32::from(y_min)),
            vec2i(i32::from(x_max), i32::from(y_max)),
        )
    }

    // Helper for the test that parses the char string for glyph 0 and returns the result and
    // glyph path.
    fn parse_char_string0(data: &[u8]) -> (Result<RectI, CFFError>, String) {
        let glyph_id = 0;
        let metadata = ReadScope::new(data).read::<CFF<'_>>().unwrap();
        let mut builder = Builder(String::new());
        let font = metadata.fonts.first().unwrap();
        let local_subrs = match &font.data {
            CFFVariant::CID(_) => None, // local subrs will be resolved on request.
            CFFVariant::Type1(type1) => type1.local_subr_index.as_ref(),
        };

        let ctx = CharStringVisitorContext::new(
            glyph_id,
            &font.char_strings_index,
            local_subrs,
            &metadata.global_subr_index,
            None,
        );
        let mut stack = ArgumentsStack {
            data: &mut [0.0; cff::MAX_OPERANDS],
            len: 0,
            max_len: cff::MAX_OPERANDS,
        };

        let res = parse_char_string(CFFFont::CFF(font), ctx, &mut stack, &mut builder);
        (
            res.map(|opt_bbox| {
                opt_bbox
                    .map(|bbox| {
                        RectI::from_points(
                            vec2i(i32::from(bbox.x_min), i32::from(bbox.y_min)),
                            vec2i(i32::from(bbox.x_max), i32::from(bbox.y_max)),
                        )
                    })
                    .unwrap_or_else(|| RectI::from_points(Vector2I::zero(), Vector2I::zero()))
            }),
            builder.0,
        )
    }

    macro_rules! test_cs_with_subrs {
        ($name:ident, $glob:expr, $loc:expr, $values:expr, $path:expr, $rect_res:expr) => {
            #[test]
            fn $name() {
                let data = gen_cff($glob, $loc, $values);
                let (res, path) = parse_char_string0(&data);
                let rect = res.unwrap();

                assert_eq!(path, $path);
                assert_eq!(rect, $rect_res);
            }
        };
    }

    macro_rules! test_cs {
        ($name:ident, $values:expr, $path:expr, $rect_res:expr) => {
            test_cs_with_subrs!($name, &[], &[], $values, $path, $rect_res);
        };
    }

    macro_rules! test_cs_err {
        ($name:ident, $values:expr, $err:expr) => {
            #[test]
            fn $name() {
                let data = gen_cff(&[], &[], $values);
                let (res, _path) = parse_char_string0(&data);
                assert_eq!(res.unwrap_err(), $err);
            }
        };
    }

    test_cs!(
        move_to,
        &[
            CFFInt(10),
            CFFInt(20),
            UInt8(operator::MOVE_TO),
            UInt8(operator::ENDCHAR),
        ],
        "M 10 20 Z ",
        rect(10, 20, 10, 20)
    );

    test_cs!(
        move_to_with_width,
        &[
            CFFInt(5),
            CFFInt(10),
            CFFInt(20),
            UInt8(operator::MOVE_TO),
            UInt8(operator::ENDCHAR),
        ],
        "M 10 20 Z ",
        rect(10, 20, 10, 20)
    );

    test_cs!(
        hmove_to,
        &[
            CFFInt(10),
            UInt8(operator::HORIZONTAL_MOVE_TO),
            UInt8(operator::ENDCHAR),
        ],
        "M 10 0 Z ",
        rect(10, 0, 10, 0)
    );

    test_cs!(
        hmove_to_with_width,
        &[
            CFFInt(10),
            CFFInt(20),
            UInt8(operator::HORIZONTAL_MOVE_TO),
            UInt8(operator::ENDCHAR),
        ],
        "M 20 0 Z ",
        rect(20, 0, 20, 0)
    );

    test_cs!(
        vmove_to,
        &[
            CFFInt(10),
            UInt8(operator::VERTICAL_MOVE_TO),
            UInt8(operator::ENDCHAR),
        ],
        "M 0 10 Z ",
        rect(0, 10, 0, 10)
    );

    test_cs!(
        vmove_to_with_width,
        &[
            CFFInt(10),
            CFFInt(20),
            UInt8(operator::VERTICAL_MOVE_TO),
            UInt8(operator::ENDCHAR),
        ],
        "M 0 20 Z ",
        rect(0, 20, 0, 20)
    );

    test_cs!(
        line_to,
        &[
            CFFInt(10),
            CFFInt(20),
            UInt8(operator::MOVE_TO),
            CFFInt(30),
            CFFInt(40),
            UInt8(operator::LINE_TO),
            UInt8(operator::ENDCHAR),
        ],
        "M 10 20 L 40 60 Z ",
        rect(10, 20, 40, 60)
    );

    test_cs!(
        line_to_with_multiple_pairs,
        &[
            CFFInt(10),
            CFFInt(20),
            UInt8(operator::MOVE_TO),
            CFFInt(30),
            CFFInt(40),
            CFFInt(50),
            CFFInt(60),
            UInt8(operator::LINE_TO),
            UInt8(operator::ENDCHAR),
        ],
        "M 10 20 L 40 60 L 90 120 Z ",
        rect(10, 20, 90, 120)
    );

    test_cs!(
        hline_to,
        &[
            CFFInt(10),
            CFFInt(20),
            UInt8(operator::MOVE_TO),
            CFFInt(30),
            UInt8(operator::HORIZONTAL_LINE_TO),
            UInt8(operator::ENDCHAR),
        ],
        "M 10 20 L 40 20 Z ",
        rect(10, 20, 40, 20)
    );

    test_cs!(
        hline_to_with_two_coords,
        &[
            CFFInt(10),
            CFFInt(20),
            UInt8(operator::MOVE_TO),
            CFFInt(30),
            CFFInt(40),
            UInt8(operator::HORIZONTAL_LINE_TO),
            UInt8(operator::ENDCHAR),
        ],
        "M 10 20 L 40 20 L 40 60 Z ",
        rect(10, 20, 40, 60)
    );

    test_cs!(
        hline_to_with_three_coords,
        &[
            CFFInt(10),
            CFFInt(20),
            UInt8(operator::MOVE_TO),
            CFFInt(30),
            CFFInt(40),
            CFFInt(50),
            UInt8(operator::HORIZONTAL_LINE_TO),
            UInt8(operator::ENDCHAR),
        ],
        "M 10 20 L 40 20 L 40 60 L 90 60 Z ",
        rect(10, 20, 90, 60)
    );

    test_cs!(
        vline_to,
        &[
            CFFInt(10),
            CFFInt(20),
            UInt8(operator::MOVE_TO),
            CFFInt(30),
            UInt8(operator::VERTICAL_LINE_TO),
            UInt8(operator::ENDCHAR),
        ],
        "M 10 20 L 10 50 Z ",
        rect(10, 20, 10, 50)
    );

    test_cs!(
        vline_to_with_two_coords,
        &[
            CFFInt(10),
            CFFInt(20),
            UInt8(operator::MOVE_TO),
            CFFInt(30),
            CFFInt(40),
            UInt8(operator::VERTICAL_LINE_TO),
            UInt8(operator::ENDCHAR),
        ],
        "M 10 20 L 10 50 L 50 50 Z ",
        rect(10, 20, 50, 50)
    );

    test_cs!(
        vline_to_with_three_coords,
        &[
            CFFInt(10),
            CFFInt(20),
            UInt8(operator::MOVE_TO),
            CFFInt(30),
            CFFInt(40),
            CFFInt(50),
            UInt8(operator::VERTICAL_LINE_TO),
            UInt8(operator::ENDCHAR),
        ],
        "M 10 20 L 10 50 L 50 50 L 50 100 Z ",
        rect(10, 20, 50, 100)
    );

    test_cs!(
        curve_to,
        &[
            CFFInt(10),
            CFFInt(20),
            UInt8(operator::MOVE_TO),
            CFFInt(30),
            CFFInt(40),
            CFFInt(50),
            CFFInt(60),
            CFFInt(70),
            CFFInt(80),
            UInt8(operator::CURVE_TO),
            UInt8(operator::ENDCHAR),
        ],
        "M 10 20 C 40 60 90 120 160 200 Z ",
        rect(10, 20, 160, 200)
    );

    test_cs!(
        curve_to_with_two_sets_of_coords,
        &[
            CFFInt(10),
            CFFInt(20),
            UInt8(operator::MOVE_TO),
            CFFInt(30),
            CFFInt(40),
            CFFInt(50),
            CFFInt(60),
            CFFInt(70),
            CFFInt(80),
            CFFInt(90),
            CFFInt(100),
            CFFInt(110),
            CFFInt(120),
            CFFInt(130),
            CFFInt(140),
            UInt8(operator::CURVE_TO),
            UInt8(operator::ENDCHAR),
        ],
        "M 10 20 C 40 60 90 120 160 200 C 250 300 360 420 490 560 Z ",
        rect(10, 20, 490, 560)
    );

    test_cs!(
        hh_curve_to,
        &[
            CFFInt(10),
            CFFInt(20),
            UInt8(operator::MOVE_TO),
            CFFInt(30),
            CFFInt(40),
            CFFInt(50),
            CFFInt(60),
            UInt8(operator::HH_CURVE_TO),
            UInt8(operator::ENDCHAR),
        ],
        "M 10 20 C 40 20 80 70 140 70 Z ",
        rect(10, 20, 140, 70)
    );

    test_cs!(
        hh_curve_to_with_y,
        &[
            CFFInt(10),
            CFFInt(20),
            UInt8(operator::MOVE_TO),
            CFFInt(30),
            CFFInt(40),
            CFFInt(50),
            CFFInt(60),
            CFFInt(70),
            UInt8(operator::HH_CURVE_TO),
            UInt8(operator::ENDCHAR),
        ],
        "M 10 20 C 50 50 100 110 170 110 Z ",
        rect(10, 20, 170, 110)
    );

    test_cs!(
        vv_curve_to,
        &[
            CFFInt(10),
            CFFInt(20),
            UInt8(operator::MOVE_TO),
            CFFInt(30),
            CFFInt(40),
            CFFInt(50),
            CFFInt(60),
            UInt8(operator::VV_CURVE_TO),
            UInt8(operator::ENDCHAR),
        ],
        "M 10 20 C 10 50 50 100 50 160 Z ",
        rect(10, 20, 50, 160)
    );

    test_cs!(
        vv_curve_to_with_x,
        &[
            CFFInt(10),
            CFFInt(20),
            UInt8(operator::MOVE_TO),
            CFFInt(30),
            CFFInt(40),
            CFFInt(50),
            CFFInt(60),
            CFFInt(70),
            UInt8(operator::VV_CURVE_TO),
            UInt8(operator::ENDCHAR),
        ],
        "M 10 20 C 40 60 90 120 90 190 Z ",
        rect(10, 20, 90, 190)
    );

    // #[test]
    // fn only_endchar() {
    //     let data = gen_cff(&[], &[], &[UInt8(operator::ENDCHAR)]);
    //     let metadata = parse_metadata(&data).unwrap();
    //     let mut builder = Builder(String::new());
    //     let char_str = metadata.char_strings.get(0).unwrap();
    //     assert!(parse_char_string(char_str, &metadata, GlyphId(0), &mut builder).is_err());
    // }

    test_cs_with_subrs!(
        local_subr,
        &[],
        &[&[
            CFFInt(30),
            CFFInt(40),
            UInt8(operator::LINE_TO),
            UInt8(operator::RETURN),
        ]],
        &[
            CFFInt(10),
            UInt8(operator::HORIZONTAL_MOVE_TO),
            CFFInt(0 - 107), // subr index - subr bias
            UInt8(operator::CALL_LOCAL_SUBROUTINE),
            UInt8(operator::ENDCHAR),
        ],
        "M 10 0 L 40 40 Z ",
        rect(10, 0, 40, 40)
    );

    test_cs_with_subrs!(
        endchar_in_subr,
        &[],
        &[&[
            CFFInt(30),
            CFFInt(40),
            UInt8(operator::LINE_TO),
            UInt8(operator::ENDCHAR),
        ]],
        &[
            CFFInt(10),
            UInt8(operator::HORIZONTAL_MOVE_TO),
            CFFInt(0 - 107), // subr index - subr bias
            UInt8(operator::CALL_LOCAL_SUBROUTINE),
        ],
        "M 10 0 L 40 40 Z ",
        rect(10, 0, 40, 40)
    );

    test_cs_with_subrs!(
        global_subr,
        &[&[
            CFFInt(30),
            CFFInt(40),
            UInt8(operator::LINE_TO),
            UInt8(operator::RETURN),
        ]],
        &[],
        &[
            CFFInt(10),
            UInt8(operator::HORIZONTAL_MOVE_TO),
            CFFInt(0 - 107), // subr index - subr bias
            UInt8(operator::CALL_GLOBAL_SUBROUTINE),
            UInt8(operator::ENDCHAR),
        ],
        "M 10 0 L 40 40 Z ",
        rect(10, 0, 40, 40)
    );

    test_cs_err!(
        reserved_operator,
        &[CFFInt(10), UInt8(2), UInt8(operator::ENDCHAR),],
        CFFError::InvalidOperator
    );

    test_cs_err!(
        line_to_without_move_to,
        &[
            CFFInt(10),
            CFFInt(20),
            UInt8(operator::LINE_TO),
            UInt8(operator::ENDCHAR),
        ],
        CFFError::MissingMoveTo
    );

    // Width must be set only once.
    test_cs_err!(
        two_vmove_to_with_width,
        &[
            CFFInt(10),
            CFFInt(20),
            UInt8(operator::VERTICAL_MOVE_TO),
            CFFInt(10),
            CFFInt(20),
            UInt8(operator::VERTICAL_MOVE_TO),
            UInt8(operator::ENDCHAR),
        ],
        CFFError::InvalidArgumentsStackLength
    );

    test_cs_err!(
        move_to_with_too_many_coords,
        &[
            CFFInt(10),
            CFFInt(10),
            CFFInt(10),
            CFFInt(20),
            UInt8(operator::MOVE_TO),
            UInt8(operator::ENDCHAR),
        ],
        CFFError::InvalidArgumentsStackLength
    );

    test_cs_err!(
        move_to_with_not_enought_coords,
        &[
            CFFInt(10),
            UInt8(operator::MOVE_TO),
            UInt8(operator::ENDCHAR),
        ],
        CFFError::InvalidArgumentsStackLength
    );

    test_cs_err!(
        hmove_to_with_too_many_coords,
        &[
            CFFInt(10),
            CFFInt(10),
            CFFInt(10),
            UInt8(operator::HORIZONTAL_MOVE_TO),
            UInt8(operator::ENDCHAR),
        ],
        CFFError::InvalidArgumentsStackLength
    );

    test_cs_err!(
        hmove_to_with_not_enought_coords,
        &[
            UInt8(operator::HORIZONTAL_MOVE_TO),
            UInt8(operator::ENDCHAR),
        ],
        CFFError::InvalidArgumentsStackLength
    );

    test_cs_err!(
        vmove_to_with_too_many_coords,
        &[
            CFFInt(10),
            CFFInt(10),
            CFFInt(10),
            UInt8(operator::VERTICAL_MOVE_TO),
            UInt8(operator::ENDCHAR),
        ],
        CFFError::InvalidArgumentsStackLength
    );

    test_cs_err!(
        vmove_to_with_not_enought_coords,
        &[UInt8(operator::VERTICAL_MOVE_TO), UInt8(operator::ENDCHAR),],
        CFFError::InvalidArgumentsStackLength
    );

    test_cs_err!(
        line_to_with_single_coord,
        &[
            CFFInt(10),
            CFFInt(20),
            UInt8(operator::MOVE_TO),
            CFFInt(30),
            UInt8(operator::LINE_TO),
            UInt8(operator::ENDCHAR),
        ],
        CFFError::InvalidArgumentsStackLength
    );

    test_cs_err!(
        line_to_with_odd_number_of_coord,
        &[
            CFFInt(10),
            CFFInt(20),
            UInt8(operator::MOVE_TO),
            CFFInt(30),
            CFFInt(40),
            CFFInt(50),
            UInt8(operator::LINE_TO),
            UInt8(operator::ENDCHAR),
        ],
        CFFError::InvalidArgumentsStackLength
    );

    test_cs_err!(
        hline_to_without_coords,
        &[
            CFFInt(10),
            CFFInt(20),
            UInt8(operator::MOVE_TO),
            UInt8(operator::HORIZONTAL_LINE_TO),
            UInt8(operator::ENDCHAR),
        ],
        CFFError::InvalidArgumentsStackLength
    );

    test_cs_err!(
        vline_to_without_coords,
        &[
            CFFInt(10),
            CFFInt(20),
            UInt8(operator::MOVE_TO),
            UInt8(operator::VERTICAL_LINE_TO),
            UInt8(operator::ENDCHAR),
        ],
        CFFError::InvalidArgumentsStackLength
    );

    test_cs_err!(
        curve_to_with_invalid_num_of_coords_1,
        &[
            CFFInt(10),
            CFFInt(20),
            UInt8(operator::MOVE_TO),
            CFFInt(30),
            CFFInt(40),
            CFFInt(50),
            CFFInt(60),
            UInt8(operator::CURVE_TO),
            UInt8(operator::ENDCHAR),
        ],
        CFFError::InvalidArgumentsStackLength
    );

    test_cs_err!(
        curve_to_with_invalid_num_of_coords_2,
        &[
            CFFInt(10),
            CFFInt(20),
            UInt8(operator::MOVE_TO),
            CFFInt(30),
            CFFInt(40),
            CFFInt(50),
            CFFInt(60),
            CFFInt(70),
            CFFInt(80),
            CFFInt(90),
            UInt8(operator::CURVE_TO),
            UInt8(operator::ENDCHAR),
        ],
        CFFError::InvalidArgumentsStackLength
    );

    test_cs_err!(
        hh_curve_to_with_not_enought_coords,
        &[
            CFFInt(10),
            CFFInt(20),
            UInt8(operator::MOVE_TO),
            CFFInt(30),
            CFFInt(40),
            CFFInt(50),
            UInt8(operator::HH_CURVE_TO),
            UInt8(operator::ENDCHAR),
        ],
        CFFError::InvalidArgumentsStackLength
    );

    test_cs_err!(
        hh_curve_to_with_too_many_coords,
        &[
            CFFInt(10),
            CFFInt(20),
            UInt8(operator::MOVE_TO),
            CFFInt(30),
            CFFInt(40),
            CFFInt(50),
            CFFInt(30),
            CFFInt(40),
            CFFInt(50),
            UInt8(operator::HH_CURVE_TO),
            UInt8(operator::ENDCHAR),
        ],
        CFFError::InvalidArgumentsStackLength
    );

    test_cs_err!(
        vv_curve_to_with_not_enought_coords,
        &[
            CFFInt(10),
            CFFInt(20),
            UInt8(operator::MOVE_TO),
            CFFInt(30),
            CFFInt(40),
            CFFInt(50),
            UInt8(operator::VV_CURVE_TO),
            UInt8(operator::ENDCHAR),
        ],
        CFFError::InvalidArgumentsStackLength
    );

    test_cs_err!(
        vv_curve_to_with_too_many_coords,
        &[
            CFFInt(10),
            CFFInt(20),
            UInt8(operator::MOVE_TO),
            CFFInt(30),
            CFFInt(40),
            CFFInt(50),
            CFFInt(30),
            CFFInt(40),
            CFFInt(50),
            UInt8(operator::VV_CURVE_TO),
            UInt8(operator::ENDCHAR),
        ],
        CFFError::InvalidArgumentsStackLength
    );

    test_cs_err!(
        multiple_endchar,
        &[UInt8(operator::ENDCHAR), UInt8(operator::ENDCHAR),],
        CFFError::DataAfterEndChar
    );

    test_cs_err!(
        operands_overflow,
        &[
            CFFInt(0),
            CFFInt(1),
            CFFInt(2),
            CFFInt(3),
            CFFInt(4),
            CFFInt(5),
            CFFInt(6),
            CFFInt(7),
            CFFInt(8),
            CFFInt(9),
            CFFInt(0),
            CFFInt(1),
            CFFInt(2),
            CFFInt(3),
            CFFInt(4),
            CFFInt(5),
            CFFInt(6),
            CFFInt(7),
            CFFInt(8),
            CFFInt(9),
            CFFInt(0),
            CFFInt(1),
            CFFInt(2),
            CFFInt(3),
            CFFInt(4),
            CFFInt(5),
            CFFInt(6),
            CFFInt(7),
            CFFInt(8),
            CFFInt(9),
            CFFInt(0),
            CFFInt(1),
            CFFInt(2),
            CFFInt(3),
            CFFInt(4),
            CFFInt(5),
            CFFInt(6),
            CFFInt(7),
            CFFInt(8),
            CFFInt(9),
            CFFInt(0),
            CFFInt(1),
            CFFInt(2),
            CFFInt(3),
            CFFInt(4),
            CFFInt(5),
            CFFInt(6),
            CFFInt(7),
            CFFInt(8),
            CFFInt(9),
        ],
        CFFError::ArgumentsStackLimitReached
    );

    test_cs_err!(
        operands_overflow_with_4_byte_ints,
        &[
            CFFInt(30000),
            CFFInt(30000),
            CFFInt(30000),
            CFFInt(30000),
            CFFInt(30000),
            CFFInt(30000),
            CFFInt(30000),
            CFFInt(30000),
            CFFInt(30000),
            CFFInt(30000),
            CFFInt(30000),
            CFFInt(30000),
            CFFInt(30000),
            CFFInt(30000),
            CFFInt(30000),
            CFFInt(30000),
            CFFInt(30000),
            CFFInt(30000),
            CFFInt(30000),
            CFFInt(30000),
            CFFInt(30000),
            CFFInt(30000),
            CFFInt(30000),
            CFFInt(30000),
            CFFInt(30000),
            CFFInt(30000),
            CFFInt(30000),
            CFFInt(30000),
            CFFInt(30000),
            CFFInt(30000),
            CFFInt(30000),
            CFFInt(30000),
            CFFInt(30000),
            CFFInt(30000),
            CFFInt(30000),
            CFFInt(30000),
            CFFInt(30000),
            CFFInt(30000),
            CFFInt(30000),
            CFFInt(30000),
            CFFInt(30000),
            CFFInt(30000),
            CFFInt(30000),
            CFFInt(30000),
            CFFInt(30000),
            CFFInt(30000),
            CFFInt(30000),
            CFFInt(30000),
            CFFInt(30000),
            CFFInt(30000),
        ],
        CFFError::ArgumentsStackLimitReached
    );

    test_cs_err!(
        bbox_overflow,
        &[
            CFFInt(32767),
            UInt8(operator::HORIZONTAL_MOVE_TO),
            CFFInt(32767),
            UInt8(operator::HORIZONTAL_LINE_TO),
            UInt8(operator::ENDCHAR),
        ],
        CFFError::BboxOverflow
    );

    #[test]
    fn endchar_in_subr_with_extra_data_1() {
        let data = gen_cff(
            &[],
            &[&[
                CFFInt(30),
                CFFInt(40),
                UInt8(operator::LINE_TO),
                UInt8(operator::ENDCHAR),
            ]],
            &[
                CFFInt(10),
                UInt8(operator::HORIZONTAL_MOVE_TO),
                CFFInt(0 - 107), // subr index - subr bias
                UInt8(operator::CALL_LOCAL_SUBROUTINE),
                CFFInt(30),
                CFFInt(40),
                UInt8(operator::LINE_TO),
            ],
        );

        let (res, _path) = parse_char_string0(&data);
        assert_eq!(res.unwrap_err(), CFFError::DataAfterEndChar);
    }

    #[test]
    fn endchar_in_subr_with_extra_data_2() {
        let data = gen_cff(
            &[],
            &[&[
                CFFInt(30),
                CFFInt(40),
                UInt8(operator::LINE_TO),
                UInt8(operator::ENDCHAR),
                CFFInt(30),
                CFFInt(40),
                UInt8(operator::LINE_TO),
            ]],
            &[
                CFFInt(10),
                UInt8(operator::HORIZONTAL_MOVE_TO),
                CFFInt(0 - 107), // subr index - subr bias
                UInt8(operator::CALL_LOCAL_SUBROUTINE),
            ],
        );

        let (res, _path) = parse_char_string0(&data);
        assert_eq!(res.unwrap_err(), CFFError::DataAfterEndChar);
    }

    #[test]
    fn subr_without_return() {
        let data = gen_cff(
            &[],
            &[&[
                CFFInt(30),
                CFFInt(40),
                UInt8(operator::LINE_TO),
                UInt8(operator::ENDCHAR),
                CFFInt(30),
                CFFInt(40),
                UInt8(operator::LINE_TO),
            ]],
            &[
                CFFInt(10),
                UInt8(operator::HORIZONTAL_MOVE_TO),
                CFFInt(0 - 107), // subr index - subr bias
                UInt8(operator::CALL_LOCAL_SUBROUTINE),
            ],
        );

        let (res, _path) = parse_char_string0(&data);
        assert_eq!(res.unwrap_err(), CFFError::DataAfterEndChar);
    }

    #[test]
    fn recursive_local_subr() {
        let data = gen_cff(
            &[],
            &[&[
                CFFInt(0 - 107), // subr index - subr bias
                UInt8(operator::CALL_LOCAL_SUBROUTINE),
            ]],
            &[
                CFFInt(10),
                UInt8(operator::HORIZONTAL_MOVE_TO),
                CFFInt(0 - 107), // subr index - subr bias
                UInt8(operator::CALL_LOCAL_SUBROUTINE),
            ],
        );

        let (res, _path) = parse_char_string0(&data);
        assert_eq!(res.unwrap_err(), CFFError::NestingLimitReached);
    }

    #[test]
    fn recursive_global_subr() {
        let data = gen_cff(
            &[&[
                CFFInt(0 - 107), // subr index - subr bias
                UInt8(operator::CALL_GLOBAL_SUBROUTINE),
            ]],
            &[],
            &[
                CFFInt(10),
                UInt8(operator::HORIZONTAL_MOVE_TO),
                CFFInt(0 - 107), // subr index - subr bias
                UInt8(operator::CALL_GLOBAL_SUBROUTINE),
            ],
        );

        let (res, _path) = parse_char_string0(&data);
        assert_eq!(res.unwrap_err(), CFFError::NestingLimitReached);
    }

    #[test]
    fn recursive_mixed_subr() {
        let data = gen_cff(
            &[&[
                CFFInt(0 - 107), // subr index - subr bias
                UInt8(operator::CALL_LOCAL_SUBROUTINE),
            ]],
            &[&[
                CFFInt(0 - 107), // subr index - subr bias
                UInt8(operator::CALL_GLOBAL_SUBROUTINE),
            ]],
            &[
                CFFInt(10),
                UInt8(operator::HORIZONTAL_MOVE_TO),
                CFFInt(0 - 107), // subr index - subr bias
                UInt8(operator::CALL_GLOBAL_SUBROUTINE),
            ],
        );

        let (res, _path) = parse_char_string0(&data);
        assert_eq!(res.unwrap_err(), CFFError::NestingLimitReached);
    }

    // TODO: return from main
    // TODO: return without endchar
    // TODO: data after return
    // TODO: recursive subr
    // TODO: HORIZONTAL_STEM
    // TODO: VERTICAL_STEM
    // TODO: HORIZONTAL_STEM_HINT_MASK
    // TODO: HINT_MASK
    // TODO: COUNTER_MASK
    // TODO: VERTICAL_STEM_HINT_MASK
    // TODO: CURVE_LINE
    // TODO: LINE_CURVE
    // TODO: VH_CURVE_TO
    // TODO: HFLEX
    // TODO: FLEX
    // TODO: HFLEX1
    // TODO: FLEX1
}
