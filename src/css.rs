//! CSS parsing and styling

use simplecss;

#[cfg(target_os="windows")]
const NATIVE_CSS_WINDOWS: &str = include_str!("../assets/native_windows.css");
#[cfg(target_os="linux")]
const NATIVE_CSS_LINUX: &str = include_str!("../assets/native_linux.css");
#[cfg(target_os="macos")]
const NATIVE_CSS_MACOS: &str = include_str!("../assets/native_macos.css");

#[derive(Debug)]
pub struct Css {
	pub rules: Vec<CssRule>,
}

#[derive(Debug)]
pub enum CssParseError {
	ParseError(::simplecss::Error),
	UnclosedBlock,
	MalformedCss,
}

#[derive(Debug)]
pub struct CssRule {
	/// `div` (`*` by default)
	pub html_type: String,
	/// `#myid` (`*` by default)
	pub id: String,
	/// `.myclass .myotherclass` (vec![] by default)
	pub classes: String,
	/// `("justify-content", "center")` (vec![] by default)
	pub declaration: (String, String),
}

impl Css {
	pub fn new() -> Self {
		Self {
			rules: Vec::new(),
		}
	}

	pub fn new_from_string(css_string: &str) -> Result<Self, CssParseError> {
		use simplecss::{Tokenizer, Token};

		let mut tokenizer = Tokenizer::new(css_string);

		let mut block_nesting = 0_usize;
		let mut css_rules = Vec::<CssRule>::new();

		// TODO: For now, rules may not be nested, otherwise, this won't work
		// TODO: This could be more efficient. We don't even need to clone the
		// strings, but this is just a quick-n-dirty CSS parser
		// This will also use up a lot of memory, since the strings get duplicated

		let mut parser_in_block = false;
		let mut current_type = "*";
		let mut current_id = "*";
		let mut current_classes = Vec::<String>::new();

		'css_parse_loop: loop {
			let tokenize_result = tokenizer.parse_next();
			match tokenize_result {
				Ok(token) => {
					match token {
						Token::EndOfStream => {
							break 'css_parse_loop;
						},
						Token::BlockStart => {
							parser_in_block = true;
							block_nesting += 1;
						},
						Token::BlockEnd => {
							block_nesting -= 1;
							parser_in_block = false;
							current_type = "*";
							current_id = "*";
							current_classes = Vec::<String>::new();
						},
						Token::TypeSelector(id) => {
							if parser_in_block {
								return Err(CssParseError::MalformedCss);
							}
							current_type = id;
						},
						Token::ClassSelector(class) => {
							if parser_in_block {
								return Err(CssParseError::MalformedCss);
							}
							current_classes.push(class.to_string());
						}
						Token::Declaration(key, val) => {
							if !parser_in_block {
								return Err(CssParseError::MalformedCss);
							}
							css_rules.push(CssRule {
								html_type: current_type.to_string(),
								id: current_id.to_string(),
								classes: current_classes.drain(..).collect(),
								declaration: (key.to_string(), val.to_string()),
							})
						},
						_ => { }
					}
				},
				Err(e) => {
					print_simplecss_error(e);
					return Err(CssParseError::ParseError(e));
				}
			}
		}

		// non-even number of blocks
		if block_nesting != 0 {
			return Err(CssParseError::UnclosedBlock);
		}

		Ok(Self {
			rules: css_rules,
		})
	}

	/// Returns the native style for the OS
	#[cfg(target_os="windows")]
	pub fn native() -> Self {
		Self::new_from_string(NATIVE_CSS_WINDOWS).unwrap()
	}

	#[cfg(target_os="linux")]
	pub fn native() -> Self {
		Self::new_from_string(NATIVE_CSS_LINUX).unwrap()
	}

	#[cfg(target_os="macos")]
	pub fn native() -> Self {
		Self::new_from_string(NATIVE_CSS_MACOS).unwrap()
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