use webrender::api::*;
use euclid::{Length, TypedRect, TypedPoint2D};
use rusttype::{Font, Scale};
use css_parser::{TextAlignment, TextOverflowBehaviour};

/// Lines is responsible for layouting the lines of the rectangle to 
struct Lines<'a> {
    align: TextAlignment,
    max_lines_before_overflow: usize,
    line_height: Length<f32, LayerPixel>,
    max_horizontal_width: Length<f32, LayerPixel>,
    font: &'a Font<'a>,
    font_size: Scale,
    origin: TypedPoint2D<f32, LayerPixel>,
    current_line: usize,
    line_writer_x: f32,
    line_writer_y: f32,
    v_scale_factor: f32,
}

pub(crate) enum TextOverflow {
    /// Text is overflowing in the vertical direction
    IsOverflowing,
    /// Text is in bounds
    InBounds,
}

impl<'a> Lines<'a> {
    pub(crate) fn from_bounds(
        bounds: &TypedRect<f32, LayerPixel>, 
        alignment: TextAlignment, 
        font: &'a Font<'a>, 
        font_size: Length<f32, LayerPixel>) 
    -> Self 
    {
        let max_lines_before_overflow = (bounds.size.height / font_size.0).floor() as usize;
        let max_horizontal_width = Length::new(bounds.size.width);
        let v_metrics = font.v_metrics_unscaled();
        let v_scale_factor = (v_metrics.ascent - v_metrics.descent + v_metrics.line_gap) / font.units_per_em() as f32;

        Self {
            align: alignment,
            max_lines_before_overflow: max_lines_before_overflow,
            line_height: font_size,
            font: font,
            origin: bounds.origin,
            max_horizontal_width: max_horizontal_width,
            font_size: Scale::uniform(font_size.0),
            current_line: 0,
            v_scale_factor: v_scale_factor,
            line_writer_x: 0.0,
            line_writer_y: 0.0,
        }
    }

    /// NOTE: The glyphs are in the space of the bounds, not of the layer! 
    /// You'd need to offset them by `bounds.origin` to get the correct position
    /// 
    /// This function will only process the glyphs until the overflow.
    /// 
    /// TODO: Only process the glyphs until the screen height is filled
    pub(crate) fn get_glyphs(&mut self, text: &str, _overflow_behaviour: TextOverflowBehaviour) -> (Vec<GlyphInstance>, TextOverflow) {
        
        // fill the rect from top to bottom with glyphs
        // let mut positioned_glyphs = Vec::new();
        
        // step 0: estimate how many lines / words are probably needed

        // step 1: collect the words

        // let words: Vec<&str> = text.split_whitespace().collect();

/*
        struct Word<'a> {
            // the original text
            text: &'a str,
            // character offsets, from the start of the word
            character_offset: Vec<f32>,
            // the sum of the width of all the characters
            total_width: f32,
        }

        let words_layouted = words.into_iter().map(|word| {

        }).collect::();
*/

/*
        println!("self.font_size: {:?}", self.font_size);

        let mut last_char = None;
        for current_char in text.chars() {

            let kerning = last_char.and_then(|last_char| {
                Some(self.font.pair_kerning(self.font_size, last_char, current_char))
            }).unwrap_or(0.0);

            // println!("kerning: ({:?} - {:?}) - {:?}", last_char, current_char, kerning);
            last_char = Some(current_char);

            let glyph = self.font.glyph(current_char);
            let idx = glyph.id().0;
            let scaled_glyph = glyph.scaled(self.font_size);
            let h_metrics = scaled_glyph.h_metrics();

            if self.line_writer_x > self.max_horizontal_width.0 {
                self.line_writer_y += self.font_size.y;
                self.current_line += 1;
                self.line_writer_x = self.origin.x;
            } else {
                self.line_writer_x += h_metrics.advance_width + kerning;
            }

            // println!("h_metrics.advance_width: {:?}, kerning: {}", h_metrics.advance_width, kerning);

            let final_x = self.origin.x + self.line_writer_x /* + kerning */;
            let final_y = self.origin.y + self.line_writer_y + self.font_size.y;

            if self.current_line <= (self.max_lines_before_overflow + 1) {
                positioned_glyphs.push(GlyphInstance {
                    index: idx,
                    point: TypedPoint2D::new(final_x, final_y),
                });
            } else {
                // do not layout text that is off-screen anyways
                break;
            }
        }
*/
        use rusttype::Point;

        let mut last_glyph = None;
        let mut caret = 0.0;

        // normalize characters, i.e. A + ^ = Â
        use unicode_normalization::UnicodeNormalization;

        // TODO: do this before hading the string to webrender?
        let text_normalized = text.nfc().collect::<String>();

        // harfbuzz pass
        /*
            use harfbuzz_rs::*;
            use harfbuzz_rs::rusttype::SetRustTypeFuncs;

            let path = "path/to/some/font_file.otf";
            let index = 0; //< face index in the font file
            let face = Face::from_file(path, index).unwrap();
            let mut font = Font::new(face);
            // Use RustType as provider for font information that harfbuzz needs.
            // You can also use a custom font implementation. For more information look
            // at the documentation for `FontFuncs`.
            font.set_rusttype_funcs();

            let output = UnicodeBuffer::new().add_str("Hello World!").shape(&font, &[]);
        */

        /*
            let positions = output.get_glyph_positions();
            let infos = output.get_glyph_infos();

            // iterate over the shaped glyphs
            for (position, info) in positions.iter().zip(infos) {
                let gid = info.codepoint;
                let cluster = info.cluster;
                let x_advance = position.x_advance;
                let x_offset = position.x_offset;
                let y_offset = position.y_offset;

                // Here you would usually draw the glyphs.
                println!("gid{:?}={:?}@{:?},{:?}+{:?}", gid, cluster, x_advance, x_offset, y_offset);
            }
        */
        // HORRIBLE WEBRENDER HACK!
        let offset_top = self.font_size.y * 3.0 / 4.0;
        
        let positioned_glyphs2 = text_normalized.chars().map(|c| {
            let g = self.font.glyph(c).scaled(self.font_size);
            if let Some(last) = last_glyph {
                caret += self.font.pair_kerning(self.font_size, last, g.id());
            }
            let g = g.positioned(Point { x: self.origin.x + caret, y: self.origin.y });
            last_glyph = Some(g.id());
            caret += g.clone().into_unpositioned().h_metrics().advance_width;
            GlyphInstance {
                index: g.id().0,
                point: TypedPoint2D::new(g.position().x, g.position().y + offset_top),
            }
        }).collect();

/*
        use rusttype::Point;

        let positioned_glyphs3 = self.font.layout(text, self.font_size, Point { x: self.origin.x, y: self.origin.y})
            .map(|g| {
                GlyphInstance {
                    index: g.id().0,
                    point: TypedPoint2D::new(g.position().x, g.position().y + self.font_size.y),
                }
            }).collect();
*/
        (positioned_glyphs2, TextOverflow::InBounds)
    }
}

#[inline]
pub(crate) fn put_text_in_bounds<'a>(
    text: &str, 
    font: &Font<'a>, 
    font_size: Length<f32, LayerPixel>, 
    alignment: TextAlignment,
    overflow_behaviour: TextOverflowBehaviour,
    bounds: &TypedRect<f32, LayerPixel>) 
-> Vec<GlyphInstance> 
{
    let mut lines = Lines::from_bounds(bounds, alignment, font, font_size);
    let (glyphs, overflow) = lines.get_glyphs(text, overflow_behaviour);
    glyphs
}