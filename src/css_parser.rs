//! Contains utilities to convert strings (CSS strings) to servo types

use webrender::api::BorderRadius;
use webrender::api::LayoutSize;

use std::num::ParseFloatError;

pub const EM_HEIGHT: f32 = 16.0;

#[derive(Debug, PartialEq, Eq)]
pub enum CssBorderRadiusParseError<'a> {
	TooManyValues(&'a str),
	InvalidComponent(&'a str),
	ValueParseErr(ParseFloatError),
}

pub fn parse_border_radius<'a>(input: &'a str)
-> Result<BorderRadius, CssBorderRadiusParseError<'a>>
{
	let mut components = input.split_whitespace();
	let len = components.clone().count();

	match len {
		1 => {
			// One value - border-radius: 15px;
			// (the value applies to all four corners, which are rounded equally:

			let uniform_radius = parse_single_css_value(components.next().unwrap())?.to_pixels();
			Ok(BorderRadius::uniform(uniform_radius))
		},
		2 => {
			// Two values - border-radius: 15px 50px;
			// (first value applies to top-left and bottom-right corners,
			// and the second value applies to top-right and bottom-left corners):

			let top_left_bottom_right = parse_single_css_value(components.next().unwrap())?.to_pixels();
			let top_right_bottom_left = parse_single_css_value(components.next().unwrap())?.to_pixels();

			Ok(BorderRadius{
				top_left: LayoutSize::new(top_left_bottom_right, top_left_bottom_right),
				bottom_right: LayoutSize::new(top_left_bottom_right, top_left_bottom_right),
				top_right: LayoutSize::new(top_right_bottom_left, top_right_bottom_left),
				bottom_left: LayoutSize::new(top_right_bottom_left, top_right_bottom_left),
			})
		},
		3 => {
			// Three values - border-radius: 15px 50px 30px;
			// (first value applies to top-left corner,
			// second value applies to top-right and bottom-left corners,
			// and third value applies to bottom-right corner):
			let top_left = parse_single_css_value(components.next().unwrap())?.to_pixels();
			let top_right_bottom_left = parse_single_css_value(components.next().unwrap())?.to_pixels();
			let bottom_right = parse_single_css_value(components.next().unwrap())?.to_pixels();

			Ok(BorderRadius{
				top_left: LayoutSize::new(top_left, top_left),
				bottom_right: LayoutSize::new(bottom_right, bottom_right),
				top_right: LayoutSize::new(top_right_bottom_left, top_right_bottom_left),
				bottom_left: LayoutSize::new(top_right_bottom_left, top_right_bottom_left),
			})
		}
		4 => {
			// Four values - border-radius: 15px 50px 30px 5px;
			// (first value applies to top-left corner,
			//  second value applies to top-right corner,
			//  third value applies to bottom-right corner,
			//  fourth value applies to bottom-left corner)
			let top_left = parse_single_css_value(components.next().unwrap())?.to_pixels();
			let top_right = parse_single_css_value(components.next().unwrap())?.to_pixels();
			let bottom_right = parse_single_css_value(components.next().unwrap())?.to_pixels();
			let bottom_left = parse_single_css_value(components.next().unwrap())?.to_pixels();

			Ok(BorderRadius{
				top_left: LayoutSize::new(top_left, top_left),
				bottom_right: LayoutSize::new(bottom_right, bottom_right),
				top_right: LayoutSize::new(top_right, top_right),
				bottom_left: LayoutSize::new(bottom_left, bottom_left),
			})
		},
		_ => {
			Err(CssBorderRadiusParseError::TooManyValues(input))
		}
	}
}

#[derive(Debug, PartialEq)]
pub struct PixelValue {
	metric: CssMetric,
	number: f32,
}

#[derive(Debug, PartialEq)]
pub enum CssMetric {
	Px,
	Em,
}

impl PixelValue {
	pub fn to_pixels(&self) -> f32 {
		match self.metric {
			CssMetric::Px => { self.number },
			CssMetric::Em => { self.number * EM_HEIGHT },
		}
	}
}

/// parse a single value such as "15px"
fn parse_single_css_value<'a>(input: &'a str)
-> Result<PixelValue, CssBorderRadiusParseError<'a>>
{
	let mut split_pos = 0;
	for (idx, ch) in input.char_indices() {
		if ch.is_numeric() || ch == '.' {
			split_pos = idx;
		}
	}

	split_pos += 1;

    let unit = &input[split_pos..];
    let unit = match unit {
    	"px" => CssMetric::Px,
    	"em" => CssMetric::Em,
    	_ => { return Err(CssBorderRadiusParseError::InvalidComponent(&input[(split_pos - 1)..])); }
    };

    let number = input[..split_pos].parse::<f32>().map_err(|e| CssBorderRadiusParseError::ValueParseErr(e))?;

    Ok(PixelValue {
    	metric: unit,
    	number: number,
	})
}

#[test]
fn test_parse_single_css_value() {
	assert_eq!(parse_single_css_value("15px"), Ok(PixelValue { metric: CssMetric::Px, number: 15.0 }));
	assert_eq!(parse_single_css_value("1.2em"), Ok(PixelValue { metric: CssMetric::Em, number: 1.2 }));
	assert_eq!(parse_single_css_value("aslkfdjasdflk"), Err(CssBorderRadiusParseError::InvalidComponent("aslkfdjasdflk")));
}

#[test]
fn test_parse_border_radius() {
	assert_eq!(parse_border_radius("15px"), Ok(BorderRadius::uniform(15.0)));
	assert_eq!(parse_border_radius("15px 50px"), Ok(BorderRadius {
		top_left: LayoutSize::new(15.0, 15.0),
		bottom_right: LayoutSize::new(15.0, 15.0),
		top_right: LayoutSize::new(50.0, 50.0),
		bottom_left: LayoutSize::new(50.0, 50.0),
	}));
	assert_eq!(parse_border_radius("15px 50px 30px"), Ok(BorderRadius {
		top_left: LayoutSize::new(15.0, 15.0),
		bottom_right: LayoutSize::new(30.0, 30.0),
		top_right: LayoutSize::new(50.0, 50.0),
		bottom_left: LayoutSize::new(50.0, 50.0),
	}));
	assert_eq!(parse_border_radius("15px 50px 30px 5px"), Ok(BorderRadius {
		top_left: LayoutSize::new(15.0, 15.0),
		bottom_right: LayoutSize::new(30.0, 30.0),
		top_right: LayoutSize::new(50.0, 50.0),
		bottom_left: LayoutSize::new(5.0, 5.0),
	}));
}