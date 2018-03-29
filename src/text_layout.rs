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
    v_advance: f32,
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
        let v_advance = v_metrics.ascent - v_metrics.descent + v_metrics.line_gap;
        let v_scale_factor = v_advance / font.units_per_em() as f32;

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
            v_advance: v_advance,
        }
    }

    /// NOTE: The glyphs are in the space of the bounds, not of the layer! 
    /// You'd need to offset them by `bounds.origin` to get the correct position
    /// 
    /// This function will only process the glyphs until the overflow.
    /// 
    /// TODO: Only process the glyphs until the screen height is filled
    pub(crate) fn get_glyphs(&mut self, text: &str, _overflow_behaviour: TextOverflowBehaviour) -> (Vec<GlyphInstance>, TextOverflow) {
        
        use unicode_normalization::UnicodeNormalization;
        use rusttype::Point;

        let text = text.nfc().collect::<String>();

        #[derive(Debug)]
        struct Word<'a> {
            // the original text
            pub text: &'a str,
            // glyphs, positions are relative to the first character of the word
            pub glyphs: Vec<GlyphInstance>,
            // the sum of the width of all the characters
            pub total_width: f32,
        }

        // normalize characters, i.e. A + ^ = Â
        // TODO: this is currently done on the whole string

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

        let mut words = Vec::new();

        // TODO: estimate how much of the text is going to fit into the rectangle

        {        
            for line in text.lines() {
                for word in line.split_whitespace() {

                    let mut caret = 0.0;
                    let mut cur_word_length = 0.0;     
                    let mut glyphs_in_this_word = Vec::new();
                    let mut last_glyph = None;

                    for c in word.chars() {
                        let g = self.font.glyph(c).scaled(self.font_size);
                        let id = g.id();
                        if let Some(last) = last_glyph {
                            caret += self.font.pair_kerning(self.font_size, last, g.id());
                        }
                        let g = g.positioned(Point { x: caret, y: 0.0 });
                        last_glyph = Some(id);        
                        let horiz_advance = g.unpositioned().h_metrics().advance_width;
                        caret += horiz_advance;
                        cur_word_length += horiz_advance;

                        glyphs_in_this_word.push(GlyphInstance {
                            index: id.0,
                            point: TypedPoint2D::new(g.position().x, g.position().y),
                        })
                    }

                    words.push(Word {
                        text: word,
                        glyphs: glyphs_in_this_word,
                        total_width: cur_word_length,
                    })
                }
            }
        }

        let mut positioned_glyphs = Vec::new();

        // do knuth-plass text layout here, determine spacing and alignment

        // position words into glyphs
        {
            let v_metrics_scaled = self.font.v_metrics(self.font_size);
            let v_advance_scaled = v_metrics_scaled.ascent - v_metrics_scaled.descent + v_metrics_scaled.line_gap;

            let mut word_caret = 0.0;
            let mut cur_line = 0;

            for word in words {
                let text_overflows_rect = word_caret + word.total_width > self.max_horizontal_width.0;
                if text_overflows_rect {
                    word_caret = 0.0;
                    cur_line += 1;
                }
                for mut glyph in word.glyphs {
                    let push_x = self.origin.x + word_caret;
                    let push_y = self.origin.y + (cur_line as f32 * v_advance_scaled) + offset_top;
                    glyph.point.x += push_x;
                    glyph.point.y += push_y;
                    positioned_glyphs.push(glyph);
                }
                
                word_caret += word.total_width + 5.0; // space between words
            }
        }

        (positioned_glyphs, TextOverflow::InBounds)
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