//! CSS parsing and styling

#[cfg(target_os="windows")]
const NATIVE_CSS_WINDOWS: &str = include_str!("../assets/native_windows.css");
#[cfg(target_os="linux")]
const NATIVE_CSS_LINUX: &str = include_str!("../assets/native_linux.css");
#[cfg(target_os="macos")]
const NATIVE_CSS_MACOS: &str = include_str!("../assets/native_macos.css");

#[derive(Debug)]
pub struct Css {
	// NOTE: Each time the rules are modified, the `dirty` flag
	// has to be set accordingly for the CSS to update!
	pub(crate) rules: Vec<CssRule>,
	pub(crate) dirty: bool,
}

#[derive(Debug)]
pub enum CssParseError {
	ParseError(::simplecss::Error),
	UnclosedBlock,
	MalformedCss,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CssRule {
	/// `div` (`*` by default)
	pub html_type: String,
	/// `#myid` (`*` by default)
	pub id: Option<String>,
	/// `.myclass .myotherclass` (vec![] by default)
	pub classes: Vec<String>,
	/// `("justify-content", "center")` (vec![] by default)
	pub declaration: (String, String),
}

impl Css {
	pub fn new() -> Self {
		Self {
			rules: Vec::new(),
			dirty: false,
		}
	}

	pub fn new_from_string(css_string: &str) -> Result<Self, CssParseError> {
		use simplecss::{Tokenizer, Token};
		use std::collections::HashSet;

		let mut tokenizer = Tokenizer::new(css_string);

		let mut block_nesting = 0_usize;
		let mut css_rules = Vec::<CssRule>::new();

		// TODO: For now, rules may not be nested, otherwise, this won't work
		// TODO: This could be more efficient. We don't even need to clone the
		// strings, but this is just a quick-n-dirty CSS parser
		// This will also use up a lot of memory, since the strings get duplicated

		let mut parser_in_block = false;
		let mut current_type = "*";
		let mut current_id = None;
		let mut current_classes = HashSet::<&str>::new();

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
							current_id = None;
							current_classes = HashSet::<&str>::new();
						},
						Token::TypeSelector(div_type) => {
							if parser_in_block {
								return Err(CssParseError::MalformedCss);
							}
							current_type = div_type;
						},
						Token::IdSelector(id) => {
							if parser_in_block {
								return Err(CssParseError::MalformedCss);
							}
							current_id = Some(id.to_string());
						}
						Token::ClassSelector(class) => {
							if parser_in_block {
								return Err(CssParseError::MalformedCss);
							}
							current_classes.insert(class);
						}
						Token::Declaration(key, val) => {
							if !parser_in_block {
								return Err(CssParseError::MalformedCss);
							}
							let mut css_rule = CssRule {
								html_type: current_type.to_string(),
								id: current_id.clone(),
								classes: current_classes.iter().map(|e| e.to_string()).collect::<Vec<String>>(),
								declaration: (key.to_string(), val.to_string()),
							};
							// IMPORTANT!
							css_rule.classes.sort();
							css_rules.push(css_rule);
						},
						_ => { }
					}
				},
				Err(e) => {
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
			dirty: true, // force layout for the first frame
		})
	}

	/// Adds a CSS rule
	pub fn add_rule(&mut self, css_rule: CssRule) {
		self.rules.push(css_rule);
		self.dirty = true;
	}

	/// Removes a rule from the current stylesheet
	pub fn remove_rule(&mut self, css_rule: &CssRule) {
		if let Some(pos) = self.rules.iter().position(|x| *x == *css_rule) {
		    self.rules.remove(pos);
		    self.dirty = true;
		};
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