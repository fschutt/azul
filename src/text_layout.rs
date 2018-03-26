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
        
        Self {
            align: alignment,
            max_lines_before_overflow: max_lines_before_overflow,
            line_height: font_size,
            font: font,
            origin: bounds.origin,
            max_horizontal_width: max_horizontal_width,
            font_size: Scale::uniform(font_size.0),
            current_line: 0,
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
    pub(crate) fn get_glyphs(&mut self, text: &str, overflow_behaviour: TextOverflowBehaviour) -> (Vec<GlyphInstance>, TextOverflow) {
        // fill the rect from top to bottom with glyphs
        let mut char_iterator_peek = text.chars().peekable();
        let mut positioned_glyphs = Vec::new();

        for current_char in text.chars() {

            let kerning = char_iterator_peek.peek().and_then(|next_char| {
                Some(self.font.pair_kerning(self.font_size, current_char, *next_char))
            });

            let kerning = match kerning {
                Some(k) => {char_iterator_peek.next(); k},
                None => 0.0,
            };

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

            let final_x = self.line_writer_x + self.origin.x + kerning;
            let final_y = self.line_writer_y + self.origin.y + self.font_size.y;

            if self.current_line > self.max_lines_before_overflow {

            }
            positioned_glyphs.push(GlyphInstance {
                index: idx,
                point: TypedPoint2D::new(final_x, final_y),
            });
            
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