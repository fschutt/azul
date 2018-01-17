pub struct AppState {

}

pub struct UiState {

}

pub struct InputEvent {

}

pub struct UiDescription {

}

pub struct Hotkeys {

}

pub fn render_loop() {
	let mut app_state = AppState { };
	let mut ui_state = app_state_to_ui_state(&app_state);
	let mut hotkeys = app_state_get_hotkeys(&app_state);
	let mut ui_description = ui_state_to_ui_description(&ui_state);

	update(&mut app_state, &mut ui_state, &mut hotkeys, &mut ui_description);
	render(&ui_description);
}

pub fn update(app_state: &mut AppState, ui_state: &mut UiState, hotkeys: &mut Hotkeys, ui_description: &mut UiDescription) {
	let frame_events = hit_test_ui(&ui_state, &hotkeys);
	if frame_events.is_empty() { return; }
	update_application_state(app_state, &frame_events);
	*ui_state = app_state_to_ui_state(&app_state);
	*ui_description = ui_state_to_ui_description(&ui_state);
}

pub fn render(ui_description: &UiDescription) {

}

fn hit_test_ui(ui_state: &UiState, hotkeys: &Hotkeys) -> Vec<InputEvent> {
	Vec::new()
}

fn update_application_state(app_state: &mut AppState, input: &[InputEvent]) {

}

fn app_state_to_ui_state(app_state: &AppState) -> UiState {
	UiState {

	}
}

fn app_state_get_hotkeys(app_state: &AppState) -> Hotkeys {
	Hotkeys {

	}
}

fn ui_state_to_ui_description(ui_state: &UiState) -> UiDescription {
	UiDescription {

	}
}
