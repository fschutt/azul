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