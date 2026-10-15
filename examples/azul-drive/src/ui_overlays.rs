//! What shows over the window on a grey scrim: Quick Look (Space: the focused item's preview,
//! large; the arrows step through the files, Space or Escape closes it) and the command
//! palette (Cmd+K: a box and what it finds - folders, files, commands, a search of every drive;
//! the arrows and Return pick, Cmd+Return shows a file in its folder, Escape closes). Both are
//! part of the window's own DOM, so the window's keyboard reaches them; a press on the scrim
//! closes them.
//!
//! On stdout, for scripts: `AZDRIVE_QUICK_LOOK open <key>|closed`, `AZDRIVE_PALETTE
//! open|closed`, `AZDRIVE_PALETTE_RUN <action>`.

use azul::{
    callbacks::{TextInputOnTextInputCallbackType, TextInputOnVirtualKeyDownCallbackType},
    dom::{FocusTarget, VirtualKeyCode},
    prelude::*,
    str::String as AzString,
    widgets::{OnTextInputReturn, TextInputState, TextInputValid},
};
use azul_appkit::l10n::{label, t, t_args, Arg};

use crate::{
    actions::{self, Action, Toggle},
    browse::{self, Place},
    ids, look,
    palette::{self, Candidates, Row, Section},
    rank, ui_find, ui_panes, with_state, DriveState,
};

/// The palette while it is open: what was typed, the hot row.
#[derive(Debug, Clone, Default)]
pub(crate) struct PaletteState {
    pub query: String,
    pub hot: usize,
}

/// A scrim over the window; a press on it (not on what lies on it) runs `close`.
fn scrim(app: &RefAny, id: AzString, close: Action) -> Dom {
    Dom::create_div()
        .with_id(id)
        .with_css(look::SCRIM)
        .with_callback(
            EventFilter::Hover(HoverEventFilter::Click),
            actions::action_ref(app, close),
            actions::on_action,
        )
}

/// What lies on a scrim: a press on it does not reach the scrim.
fn panel(css: &str) -> Dom {
    Dom::create_div()
        .with_css(css)
        .with_callback(
            EventFilter::Hover(HoverEventFilter::Click),
            RefAny::new(()),
            on_panel_click,
        )
}

extern "C" fn on_panel_click(_data: RefAny, mut info: CallbackInfo) -> Update {
    info.stop_propagation();
    Update::DoNothing
}

// ==== Quick Look ====

/// Space: Quick Look opens on the focused item (selecting it when nothing is), or closes.
pub(crate) fn toggle_quick_look(info: &mut CallbackInfo, app: &RefAny, s: &mut DriveState) {
    if s.quick_look {
        close_quick_look(s);
        return;
    }
    if s.selection.is_empty() {
        if let Some(focus) = s.selection.focus().map(str::to_string) {
            s.selection.click(&focus);
        }
    }
    let Some(key) = s.selection.single().map(str::to_string) else {
        return;
    };
    s.quick_look = true;
    actions::request_preview(info, app, s);
    println!("AZDRIVE_QUICK_LOOK open {key}");
}

pub(crate) fn close_quick_look(s: &mut DriveState) {
    if s.quick_look {
        s.quick_look = false;
        println!("AZDRIVE_QUICK_LOOK closed");
    }
}

/// Quick Look over the window while it is open: the preview of the selected item, large.
pub(crate) fn quick_look(s: &DriveState, app: &RefAny, dark: bool) -> Option<Dom> {
    if !s.quick_look {
        return None;
    }
    let name = s
        .single_selected()
        .map(|e| e.display_name(s.settings.show_extensions))
        .unwrap_or_default();
    Some(
        scrim(app, ids::QUICK_LOOK, Action::CloseQuickLook).with_child(
            panel(look::QUICK_LOOK)
                .with_child(
                    Dom::create_div()
                        .with_css(look::QUICK_LOOK_HEAD)
                        .with_child(Dom::create_span_with_text(AzString::from(name))),
                )
                .with_child(ui_panes::preview_pane(s, app, dark)),
        ),
    )
}

// ==== The command palette ====

/// Cmd+K: the palette opens (empty), or closes.
pub(crate) fn toggle_palette(s: &mut DriveState) {
    if s.palette.is_some() {
        close_palette(s);
    } else {
        s.palette = Some(PaletteState::default());
        println!("AZDRIVE_PALETTE open");
    }
}

pub(crate) fn close_palette(s: &mut DriveState) {
    if s.palette.take().is_some() {
        println!("AZDRIVE_PALETTE closed");
    }
}

/// The commands the palette offers: their words, their keys, what they run.
fn commands() -> Vec<(String, String, Action)> {
    let mut out = vec![
        ("azdrive-ribbon-new-folder", "Ctrl+Shift+N", Action::NewFolder),
        ("azdrive-loctab-new", "Ctrl+T", Action::NewTab),
        ("azdrive-ribbon-copy", "", Action::Copy),
        ("azdrive-ribbon-paste", "", Action::Paste),
        ("azdrive-ribbon-rename", "F2", Action::Rename),
        ("azdrive-ribbon-delete", "", Action::Delete),
        ("azdrive-ribbon-select-all", "", Action::SelectAll),
        ("azdrive-ribbon-preview-pane", "", Action::Toggle(Toggle::PreviewPane)),
        ("azdrive-ribbon-navigation-pane", "", Action::Toggle(Toggle::NavigationPane)),
        ("azdrive-ribbon-properties", "", Action::Properties),
        ("azdrive-ribbon-copy-link", "", Action::Share),
        ("azdrive-ribbon-upload", "", Action::Upload),
        ("azdrive-ribbon-zip", "", Action::Zip),
        ("azdrive-ribbon-email", "", Action::Email),
        ("azdrive-ribbon-add-drive", "", Action::AddDrive),
        ("azdrive-ribbon-refresh", "F5", Action::Refresh),
        ("azdrive-ribbon-options", "", Action::Options),
    ]
    .into_iter()
    .map(|(key, keys, action)| (t(key), keys.to_string(), action))
    .collect::<Vec<_>>();
    out.push((t("azdrive-file-shortcuts"), String::from("F1"), Action::Shortcuts));
    out.push((t("azdrive-file-about"), String::new(), Action::About));
    out
}

/// The folders above `place` from its drive (`Home › Documents`), a palette row's detail.
fn where_of(s: &DriveState, place: &Place) -> String {
    let drive_name = s.drive_name(place);
    let crumbs = browse::crumbs_of(place, &drive_name);
    let names: Vec<String> = crumbs.into_iter().map(|(name, _)| name).collect();
    match names.split_last() {
        Some((_, above)) => above.join(" \u{203A} "),
        None => String::new(),
    }
}

/// The palette's rows for what was typed: the window's recent searches and places, the folders
/// the window knows (the source list's listings, the open folder's), the open folder's files
/// (or the search's results), the commands.
pub(crate) fn rows(s: &DriveState) -> Vec<Row<Action>> {
    let query = s.palette.as_ref().map_or("", |p| p.query.as_str());
    let recent_searches: Vec<(String, Action)> = s
        .settings
        .recent_searches
        .iter()
        .map(|text| (text.clone(), Action::SearchFor(text.clone())))
        .collect();
    let recent_places: Vec<(String, String, Action)> = s
        .recent
        .iter()
        .filter(|place| **place != s.place)
        .map(|place| (s.place_title(place), where_of(s, place), Action::Go(place.clone())))
        .collect();
    let mut folders: Vec<(String, String, Action)> = Vec::new();
    for ((drive, _), children) in &s.tree.loaded {
        for child in children {
            let place = Place::folder(drive, child);
            if folders.iter().any(|(_, _, a)| *a == Action::Go(place.clone())) {
                continue;
            }
            let name = azul_storage::key::last_segment(child).to_string();
            folders.push((name, where_of(s, &place), Action::Go(place)));
        }
    }
    let mut files: Vec<(String, String, Action)> = Vec::new();
    if let Some(drive) = s.current_drive_id() {
        for entry in s.visible_entries() {
            let place = Place::folder(&drive, &entry.key);
            if entry.is_folder {
                if !folders.iter().any(|(_, _, a)| *a == Action::Go(place.clone())) {
                    folders.push((entry.name.clone(), where_of(s, &place), Action::Go(place)));
                }
            } else {
                let folder = Place::folder(&drive, &crate::fileops::parent_of(&entry.key));
                files.push((
                    entry.display_name(s.settings.show_extensions),
                    s.place_title(&folder),
                    Action::RevealFile {
                        place: folder,
                        key: entry.key.clone(),
                    },
                ));
            }
        }
    }
    let commands = commands();
    palette::rows(
        query,
        &Candidates {
            recent_searches: &recent_searches,
            recent_places: &recent_places,
            folders: &folders,
            files: &files,
            commands: &commands,
        },
        |q| Action::SearchAllDrives(q.to_string()),
    )
}

/// A section's title over its rows.
fn section_title(section: Section) -> &'static str {
    match section {
        Section::RecentSearches => "azdrive-palette-recent-searches",
        Section::RecentPlaces => "azdrive-palette-recent-places",
        Section::Folders => "azdrive-palette-folders",
        Section::Files => "azdrive-palette-files",
        Section::Commands => "azdrive-palette-commands",
        Section::SearchAll => "azdrive-palette-search",
    }
}

/// A row's icon.
fn row_icon(row: &Row<Action>) -> &'static str {
    match row.section {
        Section::RecentSearches => "history",
        Section::RecentPlaces | Section::Folders => "folder",
        Section::Files => "description",
        Section::Commands => "bolt",
        Section::SearchAll => "travel_explore",
    }
}

/// The palette over the window while it is open: its box, its rows under their titles, the
/// keys at its foot.
pub(crate) fn palette(s: &DriveState, app: &RefAny) -> Option<Dom> {
    let state = s.palette.as_ref()?;
    let rows = rows(s);
    let hot = state.hot.min(rows.len().saturating_sub(1));
    let input = TextInput::create()
        .with_text(AzString::from(state.query.as_str()))
        .with_placeholder(label("azdrive-palette-placeholder"))
        .with_on_text_input(app.clone(), on_palette_text as TextInputOnTextInputCallbackType)
        .with_on_virtual_key_down(app.clone(), on_palette_key as TextInputOnVirtualKeyDownCallbackType)
        .with_accessibility_name(label("azdrive-palette-placeholder"))
        .dom()
        .with_id(ids::PALETTE_INPUT)
        .with_css("flex-grow: 1;")
        .with_callback(
            EventFilter::Component(ComponentEventFilter::AfterMount),
            RefAny::new(()),
            on_palette_mounted,
        );
    let mut list = Dom::create_div().with_css(look::PALETTE_LIST);
    let mut last: Option<Section> = None;
    for (i, row) in rows.iter().enumerate() {
        if last != Some(row.section) {
            list.add_child(
                Dom::create_div()
                    .with_css(look::PALETTE_SECTION)
                    .with_child(Dom::create_span_with_text(label(section_title(row.section)))),
            );
            last = Some(row.section);
        }
        let text = match row.section {
            Section::SearchAll => t_args(
                "azdrive-palette-search-all",
                &[("text", Arg::from(row.label.as_str()))],
            ),
            _ => row.label.clone(),
        };
        let marks = rank::marks(&text, &state.query);
        let mut item = Dom::create_div()
            .with_class(ids::PALETTE_ROW_CLASS)
            .with_css(format!(
                "{} {}",
                look::PALETTE_ROW,
                if i == hot { look::PALETTE_ROW_HOT } else { "" }
            ))
            .with_accessibility_name(AzString::from(text.as_str()))
            .with_child(
                Dom::create_icon(AzString::from(row_icon(row)))
                    .with_css("font-size: 18px; margin-right: 8px; flex-shrink: 0;"),
            )
            .with_child(ui_find::marked(&text, &marks, look::CLIP));
        if !row.detail.is_empty() {
            item.add_child(
                Dom::create_div()
                    .with_css(look::PALETTE_DETAIL)
                    .with_child(Dom::create_span_with_text(AzString::from(row.detail.as_str()))),
            );
        }
        list.add_child(item.with_callback(
            EventFilter::Hover(HoverEventFilter::Click),
            RefAny::new(PaletteRowRef {
                app: app.clone(),
                index: i,
            }),
            on_palette_row,
        ));
    }
    if rows.is_empty() {
        list.add_child(
            Dom::create_div()
                .with_css(look::PALETTE_SECTION)
                .with_child(Dom::create_span_with_text(label("azdrive-find-none"))),
        );
    }
    Some(
        scrim(app, ids::PALETTE, Action::ClosePalette).with_css(look::SCRIM_TOP).with_child(
            panel(look::PALETTE_PANEL)
                .with_child(
                    Dom::create_div()
                        .with_css(look::PALETTE_INPUT_ROW)
                        .with_child(
                            Dom::create_icon(AzString::from("search"))
                                .with_css("font-size: 18px; margin-right: 8px;"),
                        )
                        .with_child(input),
                )
                .with_child(list)
                .with_child(
                    Dom::create_div()
                        .with_css(look::PALETTE_FOOT)
                        .with_child(Dom::create_span_with_text(label("azdrive-palette-keys"))),
                ),
        ),
    )
}

/// What a palette row carries: the app and its position.
struct PaletteRowRef {
    app: RefAny,
    index: usize,
}

/// Runs the palette's row `index` (`reveal`: Cmd+Return, a file shown in its folder without
/// Quick Look), closing the palette first.
fn run_row(info: &mut CallbackInfo, app: &RefAny, s: &mut DriveState, index: usize, reveal: bool) {
    let Some(row) = rows(s).into_iter().nth(index) else {
        return;
    };
    close_palette(s);
    println!("AZDRIVE_PALETTE_RUN {:?}", row.target);
    match row.target {
        // Return shows the file in Quick Look in its folder, Cmd+Return only in its folder.
        Action::RevealFile { place, key } => actions::reveal_file(info, app, s, place, key, !reveal),
        action => actions::run_action(info, app, s, action),
    }
}

extern "C" fn on_palette_row(mut data: RefAny, mut info: CallbackInfo) -> Update {
    let Some((mut app, index)) = data
        .downcast_ref::<PaletteRowRef>()
        .map(|r| (r.app.clone(), r.index))
    else {
        return Update::DoNothing;
    };
    info.stop_propagation();
    let reveal = info.get_key_modifiers().primary_down();
    with_state(&mut app, &mut info, |info, app, s| run_row(info, app, s, index, reveal))
}

extern "C" fn on_palette_text(
    mut data: RefAny,
    mut info: CallbackInfo,
    state: TextInputState,
) -> OnTextInputReturn {
    let text = state.get_text().as_str().to_string();
    let update = with_state(&mut data, &mut info, |_info, _app, s| {
        if let Some(palette) = s.palette.as_mut() {
            palette.query = text;
            palette.hot = 0;
        }
    });
    OnTextInputReturn {
        update,
        valid: TextInputValid::Yes,
    }
}

extern "C" fn on_palette_key(
    mut data: RefAny,
    mut info: CallbackInfo,
    _state: TextInputState,
) -> OnTextInputReturn {
    let key = info
        .get_current_keyboard_state()
        .current_virtual_keycode
        .into_option();
    let reveal = info.get_key_modifiers().primary_down();
    let update = match key {
        Some(VirtualKeyCode::Down | VirtualKeyCode::Up) => {
            info.stop_propagation();
            let delta = if key == Some(VirtualKeyCode::Down) { 1 } else { -1 };
            with_state(&mut data, &mut info, |_info, _app, s| {
                let count = rows(s).len();
                if let Some(palette) = s.palette.as_mut() {
                    palette.hot = palette::step(palette.hot, delta, count);
                }
            })
        }
        Some(VirtualKeyCode::Return | VirtualKeyCode::NumpadEnter) => {
            info.stop_propagation();
            with_state(&mut data, &mut info, |info, app, s| {
                let hot = s.palette.as_ref().map_or(0, |p| p.hot);
                run_row(info, app, s, hot, reveal);
            })
        }
        Some(VirtualKeyCode::Escape) => {
            info.stop_propagation();
            with_state(&mut data, &mut info, |_info, _app, s| close_palette(s))
        }
        _ => Update::DoNothing,
    };
    OnTextInputReturn {
        update,
        valid: TextInputValid::Yes,
    }
}

/// The palette's box appeared: it takes the keyboard.
extern "C" fn on_palette_mounted(_data: RefAny, mut info: CallbackInfo) -> Update {
    let node = info.get_hit_node();
    info.set_focus(FocusTarget::Id(node));
    Update::DoNothing
}
