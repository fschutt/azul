//! Haiku's icons for azul: the flora theme's icon pack.
//!
//! 73 icons of the Haiku project's icon set (MIT, see `LICENSE`) in HVIF,
//! drawn for the Material names azul's apps ask for ([`MATERIAL_TO_HAIKU`]:
//! `inbox` is Haiku's mail folder, `delete` its trash can, `print` its
//! printer). [`register`] puts them into a pack ([`PACK`]) that is searched
//! before the Material icons while the app theme is flora - flora's spins
//! (`flora:green`) included - and takes no part under any other theme
//! ([`CONDITION`], `IconProviderHandle::set_pack_condition`): Material stays
//! the fallback, for a name without a Haiku icon and for every name under
//! flat. The condition is evaluated at every lookup against the window's
//! context, so a theme switch at runtime switches the icons with the next
//! frame. azul-appkit registers the pack for every Azlin app.
//!
//! The files are Haiku's own, `data/artwork/icons` at the commit
//! `icons/UPSTREAM.txt` names, converted from Icon-O-Matic's documents to HVIF
//! by `tools/iom2hvif.py` (Icon-O-Matic's HVIF export, ported); 23 of them
//! are the very bytes Haiku ships in its `.rdef` resources.
//! `python3 tools/fetch.py` fetches and converts them again. They are
//! embedded with `include_bytes!` - 41517 bytes for all 73 - and azul draws
//! each at the size it is shown at, with the level of detail and the pixel
//! hinting the icon has for that size. HVIF artwork is full colour: it does
//! not follow the text colour as a Material glyph does.

/// The pack the icons are registered in.
pub const PACK: &str = "haiku";

/// When the pack takes part in a lookup: while the app theme is flora.
pub const CONDITION: &str = "theme=flora";

/// The credit an app's About shows for the icons: (what, its licence).
pub const CREDIT: (&str, &str) = ("Haiku's icon set (Haiku, Inc.)", "MIT");

/// The Material names drawn as a Haiku icon under flora: (the Material name,
/// the Haiku icon's name in [`ICONS`]). Every other Material name an app asks
/// for stays Material - there is no Haiku icon for it, or none that reads as
/// it (calendars, refresh, settings, the text-formatting glyphs, the media
/// controls, chevrons and other small UI glyphs).
pub const MATERIAL_TO_HAIKU: &[(&str, &str)] = &[
    // mail
    ("mail", "Mail_MarkUnread"),
    ("email", "File_New_Mail"),
    ("mark_email_unread", "Mail_MarkUnread"),
    ("mark_email_read", "Mail_MarkRead"),
    ("drafts", "Mail_MarkRead"),
    ("reply", "Mail_Reply"),
    ("forward", "Mail_Forward"),
    ("send", "Mail_Send"),
    ("report", "Mail_Junk"),
    ("inbox", "Folder_mail"),
    ("all_inbox", "Folder_mail"),
    // people
    ("contacts", "App_People"),
    ("group", "Folder_people"),
    ("person", "File_Person"),
    ("account_circle", "File_Person"),
    ("person_add", "Website_Register"),
    ("manage_accounts", "Website_Edit_Profile"),
    // folders and files
    ("folder", "Folder_generic"),
    ("folder_open", "Folder_generic"),
    ("home", "Action_GoHome"),
    ("insert_drive_file", "File_Generic"),
    ("description", "File_Text"),
    ("article", "File_Text"),
    ("notes", "File_Text"),
    ("picture_as_pdf", "File_PDF"),
    ("image", "File_Image_3"),
    ("photo", "File_Image_3"),
    ("photo_library", "File_Image_3"),
    ("movie", "File_Video"),
    ("video_library", "File_Video"),
    ("music_note", "File_Audio"),
    ("library_music", "File_Audio_2"),
    ("album", "Device_CD"),
    ("queue_music", "File_Playlist"),
    ("playlist_play", "File_Playlist"),
    ("archive", "File_Archive"),
    ("table_chart", "File_Spreadsheet_Open"),
    ("pie_chart", "File_Chart"),
    ("insert_chart", "File_Chart"),
    ("html", "File_HTML"),
    ("rss_feed", "File_RSS_Feed"),
    ("saved_search", "File_Query"),
    ("extension", "File_Plugin"),
    ("task_alt", "File_Task"),
    ("sticky_note_2", "File_Task"),
    // what is done to files
    ("file_copy", "Tracker_copy"),
    ("drive_file_move", "Tracker_move"),
    ("delete", "Trash_Empty"),
    ("delete_forever", "Trash_Empty"),
    ("delete_sweep", "Trash_Full"),
    ("save", "Document_Save"),
    ("content_paste", "Device_Clipboard"),
    ("print", "Prefs_Printer"),
    ("search", "Action_Search"),
    // going places
    ("arrow_back", "Action_GoBack_3_Large"),
    ("navigate_before", "Action_GoBack_3_Large"),
    ("arrow_forward", "Action_GoForward_3_Large"),
    ("navigate_next", "Action_GoForward_3_Large"),
    ("arrow_upward", "Action_GoUp_3_Large"),
    ("arrow_downward", "Action_GoDown_3"),
    // devices and the world
    ("storage", "Device_Harddisk"),
    ("computer", "Prefs_Screen"),
    ("keyboard", "Prefs_Keyboard"),
    ("public", "Prefs_Locale"),
    ("language", "Prefs_Locale"),
    ("settings_input_antenna", "Misc_Antenna"),
    ("live_tv", "App_TV"),
    ("slideshow", "App_ShowImage"),
    // alerts and states
    ("info", "Alert_Info"),
    ("warning", "Alert_Warning"),
    ("report_problem", "Alert_Warning"),
    ("error", "Alert_Stop"),
    ("cancel", "Action_Stop"),
    ("done", "HaikuDepot_Installed"),
    ("done_all", "HaikuDepot_Installed"),
    ("check_circle", "HaikuDepot_Installed"),
    ("star", "HaikuDepot_StarBlue"),
    ("star_border", "HaikuDepot_StarGray"),
    ("lock", "Action_Logout"),
    ("lock_open", "Action_Login"),
    ("volume_up", "Misc_Speaker"),
    ("volume_down", "Misc_Speaker"),
    ("volume_off", "Misc_Speaker_Muted"),
    ("volume_mute", "Misc_Speaker_Muted"),
    // tools
    ("calculate", "App_Calculator"),
    ("terminal", "App_Terminal"),
    ("alarm", "App_Clock"),
    ("schedule", "App_Clock"),
    ("monitor_heart", "App_Pulse"),
    ("install_desktop", "App_PackageInstaller"),
    ("font_download", "Prefs_Fonts"),
    ("book", "Misc_Book"),
    ("menu_book", "Misc_Book"),
    ("comment", "Website_Comment"),
    ("highlight", "Misc_Marker"),
    ("auto_fix_high", "Misc_Magic_Wand"),
    ("science", "Misc_Erlenmeyer"),
];

/// The Haiku icons, by their file name upstream (`data/artwork/icons/<name>`),
/// sorted: (the name, the HVIF bytes).
pub const ICONS: &[(&str, &[u8])] = &[
    ("Action_GoBack_3_Large", include_bytes!("../icons/Action_GoBack_3_Large.hvif")),
    ("Action_GoDown_3", include_bytes!("../icons/Action_GoDown_3.hvif")),
    ("Action_GoForward_3_Large", include_bytes!("../icons/Action_GoForward_3_Large.hvif")),
    ("Action_GoHome", include_bytes!("../icons/Action_GoHome.hvif")),
    ("Action_GoUp_3_Large", include_bytes!("../icons/Action_GoUp_3_Large.hvif")),
    ("Action_Login", include_bytes!("../icons/Action_Login.hvif")),
    ("Action_Logout", include_bytes!("../icons/Action_Logout.hvif")),
    ("Action_Search", include_bytes!("../icons/Action_Search.hvif")),
    ("Action_Stop", include_bytes!("../icons/Action_Stop.hvif")),
    ("Alert_Info", include_bytes!("../icons/Alert_Info.hvif")),
    ("Alert_Stop", include_bytes!("../icons/Alert_Stop.hvif")),
    ("Alert_Warning", include_bytes!("../icons/Alert_Warning.hvif")),
    ("App_Calculator", include_bytes!("../icons/App_Calculator.hvif")),
    ("App_Clock", include_bytes!("../icons/App_Clock.hvif")),
    ("App_PackageInstaller", include_bytes!("../icons/App_PackageInstaller.hvif")),
    ("App_People", include_bytes!("../icons/App_People.hvif")),
    ("App_Pulse", include_bytes!("../icons/App_Pulse.hvif")),
    ("App_ShowImage", include_bytes!("../icons/App_ShowImage.hvif")),
    ("App_TV", include_bytes!("../icons/App_TV.hvif")),
    ("App_Terminal", include_bytes!("../icons/App_Terminal.hvif")),
    ("Device_CD", include_bytes!("../icons/Device_CD.hvif")),
    ("Device_Clipboard", include_bytes!("../icons/Device_Clipboard.hvif")),
    ("Device_Harddisk", include_bytes!("../icons/Device_Harddisk.hvif")),
    ("Document_Save", include_bytes!("../icons/Document_Save.hvif")),
    ("File_Archive", include_bytes!("../icons/File_Archive.hvif")),
    ("File_Audio", include_bytes!("../icons/File_Audio.hvif")),
    ("File_Audio_2", include_bytes!("../icons/File_Audio_2.hvif")),
    ("File_Chart", include_bytes!("../icons/File_Chart.hvif")),
    ("File_Generic", include_bytes!("../icons/File_Generic.hvif")),
    ("File_HTML", include_bytes!("../icons/File_HTML.hvif")),
    ("File_Image_3", include_bytes!("../icons/File_Image_3.hvif")),
    ("File_New_Mail", include_bytes!("../icons/File_New_Mail.hvif")),
    ("File_PDF", include_bytes!("../icons/File_PDF.hvif")),
    ("File_Person", include_bytes!("../icons/File_Person.hvif")),
    ("File_Playlist", include_bytes!("../icons/File_Playlist.hvif")),
    ("File_Plugin", include_bytes!("../icons/File_Plugin.hvif")),
    ("File_Query", include_bytes!("../icons/File_Query.hvif")),
    ("File_RSS_Feed", include_bytes!("../icons/File_RSS_Feed.hvif")),
    ("File_Spreadsheet_Open", include_bytes!("../icons/File_Spreadsheet_Open.hvif")),
    ("File_Task", include_bytes!("../icons/File_Task.hvif")),
    ("File_Text", include_bytes!("../icons/File_Text.hvif")),
    ("File_Video", include_bytes!("../icons/File_Video.hvif")),
    ("Folder_generic", include_bytes!("../icons/Folder_generic.hvif")),
    ("Folder_mail", include_bytes!("../icons/Folder_mail.hvif")),
    ("Folder_people", include_bytes!("../icons/Folder_people.hvif")),
    ("HaikuDepot_Installed", include_bytes!("../icons/HaikuDepot_Installed.hvif")),
    ("HaikuDepot_StarBlue", include_bytes!("../icons/HaikuDepot_StarBlue.hvif")),
    ("HaikuDepot_StarGray", include_bytes!("../icons/HaikuDepot_StarGray.hvif")),
    ("Mail_Forward", include_bytes!("../icons/Mail_Forward.hvif")),
    ("Mail_Junk", include_bytes!("../icons/Mail_Junk.hvif")),
    ("Mail_MarkRead", include_bytes!("../icons/Mail_MarkRead.hvif")),
    ("Mail_MarkUnread", include_bytes!("../icons/Mail_MarkUnread.hvif")),
    ("Mail_Reply", include_bytes!("../icons/Mail_Reply.hvif")),
    ("Mail_Send", include_bytes!("../icons/Mail_Send.hvif")),
    ("Misc_Antenna", include_bytes!("../icons/Misc_Antenna.hvif")),
    ("Misc_Book", include_bytes!("../icons/Misc_Book.hvif")),
    ("Misc_Erlenmeyer", include_bytes!("../icons/Misc_Erlenmeyer.hvif")),
    ("Misc_Magic_Wand", include_bytes!("../icons/Misc_Magic_Wand.hvif")),
    ("Misc_Marker", include_bytes!("../icons/Misc_Marker.hvif")),
    ("Misc_Speaker", include_bytes!("../icons/Misc_Speaker.hvif")),
    ("Misc_Speaker_Muted", include_bytes!("../icons/Misc_Speaker_Muted.hvif")),
    ("Prefs_Fonts", include_bytes!("../icons/Prefs_Fonts.hvif")),
    ("Prefs_Keyboard", include_bytes!("../icons/Prefs_Keyboard.hvif")),
    ("Prefs_Locale", include_bytes!("../icons/Prefs_Locale.hvif")),
    ("Prefs_Printer", include_bytes!("../icons/Prefs_Printer.hvif")),
    ("Prefs_Screen", include_bytes!("../icons/Prefs_Screen.hvif")),
    ("Tracker_copy", include_bytes!("../icons/Tracker_copy.hvif")),
    ("Tracker_move", include_bytes!("../icons/Tracker_move.hvif")),
    ("Trash_Empty", include_bytes!("../icons/Trash_Empty.hvif")),
    ("Trash_Full", include_bytes!("../icons/Trash_Full.hvif")),
    ("Website_Comment", include_bytes!("../icons/Website_Comment.hvif")),
    ("Website_Edit_Profile", include_bytes!("../icons/Website_Edit_Profile.hvif")),
    ("Website_Register", include_bytes!("../icons/Website_Register.hvif")),
];

/// The HVIF bytes of the Haiku icon `name` (its file name upstream).
#[must_use]
pub fn hvif(name: &str) -> Option<&'static [u8]> {
    ICONS
        .binary_search_by(|(n, _)| (*n).cmp(name))
        .ok()
        .map(|i| ICONS[i].1)
}

/// The Haiku icon the Material name `material` is drawn as under flora, if
/// it has one.
#[must_use]
pub fn haiku_icon_for(material: &str) -> Option<&'static str> {
    MATERIAL_TO_HAIKU
        .iter()
        .find(|(m, _)| *m == material)
        .map(|(_, haiku)| *haiku)
}

/// Registers every icon of [`MATERIAL_TO_HAIKU`] under its Material name in
/// [`PACK`] - full-colour artwork, never recoloured - ranks the pack first
/// and makes it the flora theme's ([`CONDITION`]): searched before the
/// Material icons under flora, passed by under any other theme. Returns how
/// many registered (all of them; a file azul could not read is reported).
#[cfg(feature = "azul")]
pub fn register(provider: &mut azul::window::IconProviderHandle) -> usize {
    use azul::{vec::U8VecRef, window::IconMeta};

    let mut registered = 0;
    for (material, haiku) in MATERIAL_TO_HAIKU {
        let Some(bytes) = hvif(haiku) else {
            eprintln!("[azul-icons-haiku] no icon `{haiku}` for `{material}`");
            continue;
        };
        if provider.register_hvif_icon(
            PACK,
            *material,
            U8VecRef::from(bytes),
            IconMeta::create_for_image(),
        ) {
            registered += 1;
        } else {
            eprintln!("[azul-icons-haiku] `{haiku}` ({material}) did not register");
        }
    }
    provider.set_pack_rank(PACK, 0);
    provider.set_pack_condition(PACK, CONDITION);
    registered
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_icons_are_sorted_by_name_and_each_is_there_once() {
        let names: Vec<&str> = ICONS.iter().map(|(n, _)| *n).collect();
        let mut sorted = names.clone();
        sorted.sort_unstable();
        sorted.dedup();
        assert_eq!(names, sorted, "sorted, no name twice (hvif() searches them)");
    }

    #[test]
    fn every_material_name_is_drawn_by_an_icon_this_crate_ships() {
        for (material, haiku) in MATERIAL_TO_HAIKU {
            assert!(hvif(haiku).is_some(), "`{material}`: no icon `{haiku}`");
        }
        assert_eq!(haiku_icon_for("inbox"), Some("Folder_mail"));
        assert_eq!(haiku_icon_for("settings"), None, "settings stays Material");
    }

    #[test]
    fn every_icon_this_crate_ships_draws_a_material_name() {
        for (name, _) in ICONS {
            assert!(
                MATERIAL_TO_HAIKU.iter().any(|(_, haiku)| haiku == name),
                "`{name}` is shipped but drawn for no name"
            );
        }
    }

    #[test]
    fn every_name_is_a_material_icon_azul_has_and_is_mapped_once() {
        let material: std::collections::BTreeSet<&str> = material_icons::ALL_ICONS
            .iter()
            .map(material_icons::icon_to_html_name)
            .collect();
        let mut seen = std::collections::BTreeSet::new();
        for (name, _) in MATERIAL_TO_HAIKU {
            assert!(material.contains(name), "`{name}` is no Material icon azul registers");
            assert!(seen.insert(*name), "`{name}` mapped twice");
        }
    }

    #[test]
    fn every_icon_is_an_hvif_file_azul_reads_and_draws_at_16_px() {
        let mut total = 0;
        for (name, bytes) in ICONS {
            assert!(bytes.starts_with(b"ncif"), "{name} is no HVIF file");
            let icon = azul_core::hvif::Hvif::parse(bytes)
                .unwrap_or_else(|e| panic!("{name}: azul cannot read it: {e:?}"));
            // A shape whose level of detail starts above 16 px draws nothing
            // there (Haiku's Misc_Hand starts at 33 px): every icon has to
            // show at the size of a small ribbon button.
            assert!(
                icon.shapes.iter().any(|s| s.visible_at(16.0 / azul_core::hvif::GRID)),
                "{name} draws nothing at 16 px"
            );
            total += bytes.len();
        }
        assert!(total < 64 * 1024, "{total} bytes");
    }
}
