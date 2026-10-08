//! The settings at work (the model is `options.rs`, the pages' look `ui.rs`): the settings'
//! list opens as a page of the media center; a category's page opens with a DRAFT of what is
//! set now; Enter or a click on one of its rows turns a check box over, takes a radio list's
//! choice, removes a library folder, adds one through the folder dialog, looks through the
//! libraries again; SAVE makes the draft AzPlayer's - the window shows it at once, a library
//! whose folders changed is looked through again - and writes it into the settings file
//! (appkit's `player/settings.json`, on its file thread); CANCEL and Back drop it.
//!
//! On stdout: `AZPLAYER_OPTION <key> <value>` (a draft's change, and every option at the
//! start), `AZPLAYER_FOLDERS <library> <n>`, `AZPLAYER_SETTINGS <saved|cancel> <category>`.

use std::path::PathBuf;

use azul::{
    dialog::{FileDialog, FileOpenResult},
    option::OptionString,
    prelude::*,
};
use azul_appkit::ui as kit;

use crate::{
    app::{self, Player},
    library::Shelf,
    nav,
    options::{self, Category, Draft, Facts, Press, Row},
    pages::Screen,
};

/// What the general and about pages say: AzPlayer's version and data folder, the look the
/// shared Azlin config gives the other apps, how many things the libraries hold.
#[must_use]
pub fn facts(s: &Player) -> Facts {
    let mut kit_ref = s.kit.clone();
    let found = kit_ref.downcast_ref::<kit::Kit>().map(|k| {
        (
            k.settings.theme.label().to_string(),
            k.settings.mode.label().to_string(),
            k.data_root.join(k.about.app_folder).display().to_string(),
        )
    });
    let (theme, mode, data) = found.unwrap_or_default();
    Facts {
        version: app::ABOUT.version.to_string(),
        theme,
        mode,
        data,
        counts: [
            s.library.music.items.len(),
            s.library.pictures.items.len(),
            s.library.videos.items.len(),
            s.library.tv.items.len(),
        ],
    }
}

/// The rows of `category`'s page, as its draft has them.
#[must_use]
pub fn page_rows(s: &Player, category: Category) -> Vec<Row> {
    let folders = s.draft.as_ref().map_or(&s.folders, |d| &d.folders);
    options::rows(category, folders, &facts(s))
}

/// What the focused part of a category's page says (`index`: a row the keys land on, or one of
/// the buttons after them).
#[must_use]
pub fn focus_label(rows: &[Row], category: Category, index: usize) -> String {
    let at = options::focusable(rows);
    match at.get(index) {
        Some(&row) => rows[row].label(),
        None => category
            .buttons()
            .get(index - at.len())
            .map_or_else(String::new, |b| (*b).to_string()),
    }
}

/// The settings (the strip's settings, Ctrl+, , `--screen settings`): their list.
pub fn open(s: &mut Player) {
    if s.place().screen != Screen::Settings {
        nav::go(s, Screen::Settings);
    }
}

/// The page of `category`: its draft starts from what is set now, the keys on its first
/// control (on about: its button).
pub fn open_category(s: &mut Player, category: Category) {
    s.draft = Some(Draft {
        category,
        options: s.options.clone(),
        folders: s.folders.clone(),
    });
    nav::go(s, Screen::SettingsPage(category));
}

/// Removes `folder` from the draft's `shelf` library (the files stay; save makes it so).
pub fn remove_folder(s: &mut Player, shelf: Shelf, folder: &std::path::Path) {
    let Some(draft) = s.draft.as_mut() else {
        return;
    };
    if draft.folders.remove(shelf, folder) {
        println!(
            "AZPLAYER_FOLDERS {} {}",
            shelf.word().replace(' ', "-"),
            draft.folders.of(shelf).len()
        );
    }
    // The keys stay on a part of the page that is there.
    let Screen::SettingsPage(category) = s.place().screen else {
        return;
    };
    let rows = page_rows(s, category);
    let last = (options::focusable(&rows).len() + category.buttons().len()).saturating_sub(1);
    let focus = &mut s.place_mut().focus;
    focus.index = focus.index.min(last);
}

/// Enter or a click on part `index` of a category's page: a check box turns over, a radio
/// list's choice is taken, a folder is removed, a button does its job; save and cancel.
pub fn press(app: &RefAny, info: &mut CallbackInfo, index: usize) -> Update {
    enum Then {
        Nothing,
        Save,
        Cancel,
        AddFolder(Shelf),
        Rescan,
    }
    let then = {
        let mut app_ref = app.clone();
        let Some(mut s) = app_ref.downcast_mut::<Player>() else {
            return Update::DoNothing;
        };
        let Screen::SettingsPage(category) = s.place().screen else {
            return Update::DoNothing;
        };
        let rows = page_rows(&s, category);
        let at = options::focusable(&rows);
        s.place_mut().focus.index = index;
        match at.get(index).map(|&row| rows[row].clone()) {
            Some(Row::Check { key, .. }) => {
                if let Some(d) = s.draft.as_mut() {
                    let on = d.options.toggle(key);
                    println!("AZPLAYER_OPTION {key} {on}");
                }
                Then::Nothing
            }
            Some(Row::Radio { key, value, .. }) => {
                if let Some(d) = s.draft.as_mut() {
                    if d.options.set(key, value) {
                        println!("AZPLAYER_OPTION {key} {value}");
                    }
                }
                Then::Nothing
            }
            Some(Row::Folder { shelf, path }) => {
                remove_folder(&mut s, shelf, &path);
                Then::Nothing
            }
            Some(Row::Button {
                press: Press::AddFolder(shelf),
                ..
            }) => Then::AddFolder(shelf),
            Some(Row::Button {
                press: Press::Rescan,
                ..
            }) => {
                s.notice("Looking through the folders again.");
                Then::Rescan
            }
            Some(Row::Heading(_) | Row::Note(_)) => Then::Nothing,
            None => match category.buttons().get(index - at.len()) {
                Some(&"save") => Then::Save,
                Some(_) => Then::Cancel,
                None => Then::Nothing,
            },
        }
    };
    match then {
        Then::Nothing => Update::RefreshDom,
        Then::Save => save(app, info),
        Then::Cancel => cancel(app),
        Then::AddFolder(shelf) => {
            pick_folder(app, shelf);
            Update::DoNothing
        }
        Then::Rescan => {
            app::scan_all(app, info);
            Update::RefreshDom
        }
    }
}

/// Save: the draft's options and folders become AzPlayer's (the window shows them at once; a
/// library whose folders changed is looked through again) and what changed goes into the
/// settings file; the settings' list comes back.
pub fn save(app: &RefAny, info: &mut CallbackInfo) -> Update {
    let rescan = {
        let mut app_ref = app.clone();
        let Some(mut s) = app_ref.downcast_mut::<Player>() else {
            return Update::DoNothing;
        };
        let Some(draft) = s.draft.take() else {
            return Update::DoNothing;
        };
        let mut changes: Vec<(String, String)> = draft
            .options
            .changes(&s.options)
            .into_iter()
            .map(|(key, value)| (key.to_string(), value))
            .collect();
        let mut rescan = false;
        for shelf in Shelf::ALL {
            if draft.folders.of(shelf) != s.folders.of(shelf) {
                changes.push((
                    options::folders_key(shelf).to_string(),
                    options::folders_text(draft.folders.of(shelf)),
                ));
                rescan = true;
            }
        }
        let category = draft.category;
        s.options = draft.options;
        s.folders = draft.folders;
        if !changes.is_empty() {
            let kit_ref = s.kit.clone();
            {
                let mut kit_mut = kit_ref.clone();
                if let Some(mut k) = kit_mut.downcast_mut::<kit::Kit>() {
                    for (key, value) in &changes {
                        k.settings.set(key, value);
                    }
                };
            }
            kit::save_settings(&kit_ref, info);
        }
        println!("AZPLAYER_SETTINGS saved {} {}", category.key(), changes.len());
        nav::back_in(&mut s);
        rescan
    };
    if rescan {
        app::scan_all(app, info);
    }
    app::request_art(app, info);
    Update::RefreshDom
}

/// Cancel (and Back on a category's page): the draft goes, the settings' list comes back.
pub fn cancel(app: &RefAny) -> Update {
    let mut app_ref = app.clone();
    if let Some(mut s) = app_ref.downcast_mut::<Player>() {
        // Back drops the draft and says so.
        nav::back_in(&mut s);
    }
    Update::RefreshDom
}

/// The folder dialog's answer goes to the library it was opened for.
struct FolderPick {
    app: RefAny,
    shelf: Shelf,
}

/// The system's folder dialog, for a folder to add to `shelf`.
fn pick_folder(app: &RefAny, shelf: Shelf) {
    let _request = FileDialog::open_directory(
        format!("Add a folder to {}", shelf.word()),
        OptionString::None,
        RefAny::new(FolderPick {
            app: app.clone(),
            shelf,
        }),
        on_folder_picked,
    );
}

/// A folder was chosen: it is the draft's (save makes it the library's).
extern "C" fn on_folder_picked(mut data: RefAny, _info: CallbackInfo, result: RefAny) -> Update {
    let Some((app, shelf)) = data
        .downcast_ref::<FolderPick>()
        .map(|p| (p.app.clone(), p.shelf))
    else {
        return Update::DoNothing;
    };
    let Some(picked) = FileOpenResult::downcast(result).into_option() else {
        return Update::DoNothing;
    };
    let Some(path) = picked.path.into_option() else {
        return Update::DoNothing;
    };
    let folder = PathBuf::from(path.inner.as_str());
    let mut app_ref = app.clone();
    let Some(mut s) = app_ref.downcast_mut::<Player>() else {
        return Update::DoNothing;
    };
    let Some(draft) = s.draft.as_mut() else {
        return Update::DoNothing;
    };
    if draft.folders.add(shelf, folder) {
        println!(
            "AZPLAYER_FOLDERS {} {}",
            shelf.word().replace(' ', "-"),
            draft.folders.of(shelf).len()
        );
    }
    Update::RefreshDom
}
