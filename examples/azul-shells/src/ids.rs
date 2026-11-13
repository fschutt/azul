//! AzShells' own DOM ids, each name defined ONCE with the app's `__azshells_`
//! prefix (user ruling 2026-10-02). The shells' slot ids (`shell-title`,
//! `shell-list`, ...) are the shell widgets' own, not the app's.

use azul::str::String as AzString;

/// The picker row's id, as text (the shell table and the scripts name it).
pub const PICKER_NAME: &str = "__azshells_picker";
/// The area under the picker that holds the chosen shell, as text.
pub const AREA_NAME: &str = "__azshells_area";
/// The button that opens the app's settings page.
pub const SETTINGS_BUTTON_NAME: &str = "__azshells_settings";

pub const PICKER: AzString = AzString::from_const_str(PICKER_NAME);
pub const AREA: AzString = AzString::from_const_str(AREA_NAME);
pub const SETTINGS_BUTTON: AzString = AzString::from_const_str(SETTINGS_BUTTON_NAME);
