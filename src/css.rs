//! CSS parsing and styling

use simplecss;

#[cfg(target_os="windows")]
const NATIVE_CSS_WINDOWS: &str = include_str!("../assets/native_windows.css");
#[cfg(target_os="linux")]
const NATIVE_CSS_LINUX: &str = include_str!("../assets/native_linux.css");
#[cfg(target_os="macos")]
const NATIVE_CSS_MACOS: &str = include_str!("../assets/native_macos.css");

pub struct Css {

}

#[derive(Debug)]
pub struct CssParseError {

}

impl Css {
	pub fn new() -> Self {
		Self {

		}
	}

	pub fn parse_from_string(css_string: &str) -> Result<Self, CssParseError> {
		use simplecss::{Tokenizer, Token};

		println!("{:?}", css_string);

		let mut tokenizer = Tokenizer::new(css_string);

		'css_parse_loop: loop {
			let tokenize_result = tokenizer.parse_next();
			match tokenize_result {
				Ok(token) => {
					println!("got token - {:?}", token);
					if token == Token::EndOfStream {
						break 'css_parse_loop;
					}
				},
				Err(e) => {
					print_simplecss_error(e);
					return Err(CssParseError { })
				}
			}
		}

		Ok(Self {

		})
	}

	/// Returns the native style for the OS
	#[cfg(target_os="windows")]
	pub fn native() -> Self {
		Self::parse_from_string(NATIVE_CSS_WINDOWS).unwrap()
	}

	#[cfg(target_os="linux")]
	pub fn native() -> Self {
		Self::parse_from_string(NATIVE_CSS_LINUX).unwrap()
	}

	#[cfg(target_os="macos")]
	pub fn native() -> Self {
		Self::parse_from_string(NATIVE_CSS_MACOS).unwrap()
	}
}

fn print_simplecss_error(e: simplecss::Error) {
	use simplecss::Error::*;
	match e {
		UnexpectedEndOfStream(pos) => {
			error!("unexpected end of stream at position: {:?}", pos);
		},
		InvalidAdvance { expected, total, pos} => {
			error!("invalid advance: expected {:?} bytes, only got {:?}, at position {:?}",
				expected, total, pos);
		},
		UnsupportedToken(pos) => {
			error!("unsupported token at position: {:?}", pos);
		},
		UnknownToken(pos) => {
			error!("unknown token at position: {:?}", pos);
		},
	}
}