use ui_state::UiState;

/// Filtered event that is currently relevant to the application
pub struct InputEvent {

}

/// Hotkeys, such as [CRTL + A], for example
pub struct Hotkeys {
	keys: Vec<Hotkey>,
}

pub struct Hotkey {
	keycode: u32,
	modifier: Option<Modifier>,
}

pub enum Modifier {
	Alt,
	Ctrl,
	Shift,
	Super,
}

impl Hotkeys {
	/// Empty hotkey list
	pub fn none() -> Self {
		Self {
			keys: Vec::new(),
		}
	}
}

pub fn hit_test_ui(ui_state: &UiState, hotkeys: &Hotkeys) -> Vec<InputEvent> {
	Vec::new()
}