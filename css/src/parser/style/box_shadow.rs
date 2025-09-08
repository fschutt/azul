use crate::{css_properties::*, parser::*};

#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
pub struct StyleBoxShadow {
    pub offset: [PixelValueNoPercent; 2],
    pub color: ColorU,
    pub blur_radius: PixelValueNoPercent,
    pub spread_radius: PixelValueNoPercent,
    pub clip_mode: BoxShadowClipMode,
}

impl StyleBoxShadow {
    pub fn scale_for_dpi(&mut self, scale_factor: f32) {
        for s in self.offset.iter_mut() {
            s.scale_for_dpi(scale_factor);
        }
        self.blur_radius.scale_for_dpi(scale_factor);
        self.spread_radius.scale_for_dpi(scale_factor);
    }
}

/// Parses a CSS box-shadow, such as "5px 10px inset"
pub fn parse_style_box_shadow<'a>(
    input: &'a str,
) -> Result<StyleBoxShadow, CssShadowParseError<'a>> {
    let mut input_iter = input.split_whitespace();
    let count = input_iter.clone().count();

    let mut box_shadow = StyleBoxShadow {
        offset: [
            PixelValueNoPercent {
                inner: PixelValue::const_px(0),
            },
            PixelValueNoPercent {
                inner: PixelValue::const_px(0),
            },
        ],
        color: ColorU {
            r: 0,
            g: 0,
            b: 0,
            a: 255,
        },
        blur_radius: PixelValueNoPercent {
            inner: PixelValue::const_px(0),
        },
        spread_radius: PixelValueNoPercent {
            inner: PixelValue::const_px(0),
        },
        clip_mode: BoxShadowClipMode::Outset,
    };

    let last_val = input_iter.clone().rev().next();
    let is_inset = last_val == Some("inset") || last_val == Some("outset");

    if count > 2 && is_inset {
        let l_val = last_val.unwrap();
        if l_val == "outset" {
            box_shadow.clip_mode = BoxShadowClipMode::Outset;
        } else if l_val == "inset" {
            box_shadow.clip_mode = BoxShadowClipMode::Inset;
        }
    }

    match count {
        2 => {
            // box-shadow: 5px 10px; (h_offset, v_offset)
            let h_offset = parse_pixel_value_no_percent(input_iter.next().unwrap())?;
            let v_offset = parse_pixel_value_no_percent(input_iter.next().unwrap())?;
            box_shadow.offset[0] = h_offset;
            box_shadow.offset[1] = v_offset;
        }
        3 => {
            // box-shadow: 5px 10px inset; (h_offset, v_offset, inset)
            let h_offset = parse_pixel_value_no_percent(input_iter.next().unwrap())?;
            let v_offset = parse_pixel_value_no_percent(input_iter.next().unwrap())?;
            box_shadow.offset[0] = h_offset;
            box_shadow.offset[1] = v_offset;

            if !is_inset {
                // box-shadow: 5px 10px #888888; (h_offset, v_offset, color)
                let color = parse_css_color(input_iter.next().unwrap())?;
                box_shadow.color = color;
            }
        }
        4 => {
            let h_offset = parse_pixel_value_no_percent(input_iter.next().unwrap())?;
            let v_offset = parse_pixel_value_no_percent(input_iter.next().unwrap())?;
            box_shadow.offset[0] = h_offset;
            box_shadow.offset[1] = v_offset;

            if !is_inset {
                let blur = parse_pixel_value_no_percent(input_iter.next().unwrap())?;
                box_shadow.blur_radius = blur.into();
            }

            let color = parse_css_color(input_iter.next().unwrap())?;
            box_shadow.color = color;
        }
        5 => {
            // box-shadow: 5px 10px 5px 10px #888888; (h_offset, v_offset, blur, spread, color)
            // box-shadow: 5px 10px 5px #888888 inset; (h_offset, v_offset, blur, color, inset)
            let h_offset = parse_pixel_value_no_percent(input_iter.next().unwrap())?;
            let v_offset = parse_pixel_value_no_percent(input_iter.next().unwrap())?;
            box_shadow.offset[0] = h_offset;
            box_shadow.offset[1] = v_offset;

            let blur = parse_pixel_value_no_percent(input_iter.next().unwrap())?;
            box_shadow.blur_radius = blur.into();

            if !is_inset {
                let spread = parse_pixel_value_no_percent(input_iter.next().unwrap())?;
                box_shadow.spread_radius = spread.into();
            }

            let color = parse_css_color(input_iter.next().unwrap())?;
            box_shadow.color = color;
        }
        6 => {
            // box-shadow: 5px 10px 5px 10px #888888 inset; (h_offset, v_offset, blur, spread,
            // color, inset)
            let h_offset = parse_pixel_value_no_percent(input_iter.next().unwrap())?;
            let v_offset = parse_pixel_value_no_percent(input_iter.next().unwrap())?;
            box_shadow.offset[0] = h_offset;
            box_shadow.offset[1] = v_offset;

            let blur = parse_pixel_value_no_percent(input_iter.next().unwrap())?;
            box_shadow.blur_radius = blur.into();

            let spread = parse_pixel_value_no_percent(input_iter.next().unwrap())?;
            box_shadow.spread_radius = spread.into();

            let color = parse_css_color(input_iter.next().unwrap())?;
            box_shadow.color = color;
        }
        _ => {
            return Err(CssShadowParseError::TooManyComponents(input));
        }
    }

    Ok(box_shadow)
}
