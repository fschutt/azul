//! AzReview's DOM ids, each name defined ONCE with the app's `__azreview_`
//! prefix (user ruling 2026-10-02).

use azul::str::String as AzString;

/// The sheet strip (the virtual view the pages scroll in), as text: the
/// page rail looks it up by its id attribute.
pub const STRIP_NAME: &str = "__azreview_sheet-strip";

pub const STRIP: AzString = AzString::from_const_str(STRIP_NAME);
/// The file list (in the flyout).
pub const FILES: AzString = AzString::from_const_str("__azreview_files");
/// The flyout the files show in, over the pages.
pub const FLYOUT: AzString = AzString::from_const_str("__azreview_flyout");
/// The title row's switch that flies the files out and folds them away.
pub const SIDEBAR_TOGGLE: AzString = AzString::from_const_str("__azreview_sidebar-toggle");
/// The page numbers' row, as text: the strip's scroll looks it up to keep
/// the lit number in view.
pub const PAGES_NAME: &str = "__azreview_pages";

pub const PAGES: AzString = AzString::from_const_str(PAGES_NAME);
/// The page rail's record, save and settings buttons.
pub const RECORD: AzString = AzString::from_const_str("__azreview_record");
pub const SAVE: AzString = AzString::from_const_str("__azreview_save");
pub const SETTINGS: AzString = AzString::from_const_str("__azreview_settings");
/// The side column of notes, its title row's switch, its hint when empty, its "listening" pill, its
/// stop button.
pub const NOTES: AzString = AzString::from_const_str("__azreview_notes");
pub const NOTES_TOGGLE: AzString = AzString::from_const_str("__azreview_notes-toggle");
pub const NOTES_HINT: AzString = AzString::from_const_str("__azreview_notes-hint");
pub const LISTENING: AzString = AzString::from_const_str("__azreview_listening");
pub const DICTATION_STOP: AzString = AzString::from_const_str("__azreview_dictation-stop");
/// The settings page's dictation rows.
pub const SETTING_INPUT: AzString = AzString::from_const_str("__azreview_setting-input");
pub const SETTING_REST: AzString = AzString::from_const_str("__azreview_setting-rest");
pub const SETTING_QUIET: AzString = AzString::from_const_str("__azreview_setting-quiet");
pub const SETTING_STANDBY: AzString = AzString::from_const_str("__azreview_setting-standby");
pub const SETTING_STANDBY_IDLE: AzString = AzString::from_const_str("__azreview_setting-standby-idle");
/// The flyout's "Open folder" button.
pub const OPEN_FOLDER: AzString = AzString::from_const_str("__azreview_open-folder");
/// The empty page's "Open folder" button, and the microphone widget that records a clip.
pub const OPEN_FOLDER_EMPTY: AzString = AzString::from_const_str("__azreview_open-folder-empty");
pub const MICROPHONE: AzString = AzString::from_const_str("__azreview_microphone");
/// The title row's labeled "Open folder..." button, and the chip that names the folder under review.
pub const OPEN_FOLDER_BAR: AzString = AzString::from_const_str("__azreview_open-folder-bar");
pub const FOLDER_CHIP: AzString = AzString::from_const_str("__azreview_folder-chip");
/// The file tree (the whole window's height, it pushes the pages) and the grip on its right edge.
pub const TREE: AzString = AzString::from_const_str("__azreview_tree");
pub const TREE_GRIP: AzString = AzString::from_const_str("__azreview_tree-grip");
/// The right bar, its key, the panel it opens (the whiteboard's place) and the grip on the panel's left edge.
pub const RIGHT_BAR: AzString = AzString::from_const_str("__azreview_right-bar");
pub const RIGHT_TOGGLE: AzString = AzString::from_const_str("__azreview_right-toggle");
pub const RIGHT_PANEL: AzString = AzString::from_const_str("__azreview_right-panel");
pub const RIGHT_GRIP: AzString = AzString::from_const_str("__azreview_right-grip");
/// The settings page's list of the recognisers and their state.
pub const SPEECH_STATUS: AzString = AzString::from_const_str("__azreview_speech-status");
/// The sentence at the foot of the tree about how much of the workspace is reviewed.
pub const REVIEWED: AzString = AzString::from_const_str("__azreview_reviewed");
/// The settings page's line about the microphone's state.
pub const MIC_STATUS: AzString = AzString::from_const_str("__azreview_mic-status");
/// The bottom bar of the agents at work.
pub const BOTTOM_BAR: AzString = AzString::from_const_str("__azreview_agents-bar");
/// What the tree shows of a file's conversations: an agent at work (the spinner), a suggestion waiting, all done.
pub const FILE_MARK_WORKING: AzString = AzString::from_const_str("__azreview_file-mark-working");
pub const FILE_MARK_SUGGESTED: AzString = AzString::from_const_str("__azreview_file-mark-suggested");
pub const FILE_MARK_DONE: AzString = AzString::from_const_str("__azreview_file-mark-done");
