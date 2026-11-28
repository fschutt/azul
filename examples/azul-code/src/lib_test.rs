use super::SHORTCUTS;

/// The keys `commands::handle_key` takes (Mod+O, Mod+P, Mod+B included: the welcome page
/// promises them); the F1 list must show every one of them.
#[test]
fn the_shortcut_list_names_every_key_the_window_takes() {
    let window_keys = [
        "Mod+K Mod+O", "Mod+O", "Mod+K F", "Mod+P", "Mod+Shift+P", "Mod+B", "Mod+Shift+E", "Mod+S",
        "Mod+W", "Mod+F", "Mod+Shift+F", "Mod+H", "Mod+G", "F3", "Escape", "Ctrl+`", "Mod+J",
        "Ctrl+Shift+`", "Mod+Shift+G", "Mod+Shift+D", "Mod+Shift+M", "Mod+Shift+U",
    ];
    for key in window_keys {
        assert!(
            SHORTCUTS
                .iter()
                .any(|s| s.keys.split(" / ").any(|k| k == key)),
            "the F1 list does not name {key}"
        );
    }
}

/// Every command with keys names keys the F1 list shows too.
#[test]
fn every_command_with_keys_is_in_the_shortcut_list() {
    for &action in &crate::actions::ACTIONS {
        let keys = action.keys();
        if keys.is_empty() || keys == "Mod+," || keys == "F1" || keys == "Mod+Z" || keys == "Mod+Shift+Z" {
            continue;
        }
        assert!(
            SHORTCUTS.iter().any(|s| s.keys.split(" / ").any(|k| k == keys)),
            "{action:?}: {keys} is not in the F1 list"
        );
    }
}

/// AzCode is dark by default, as VSCode is: only a settings file that names a mode
/// (the settings page's Mode writes it) or a `--mode` switch picks another.
#[test]
fn azcode_is_dark_until_the_settings_file_names_a_mode() {
    let dir = std::env::temp_dir().join(format!("azcode-mode-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("a temporary folder");
    let file = dir.join("settings.json");
    assert!(!super::names_a_mode(&file), "no file names no mode");
    for (text, named) in [
        ("{\"theme\": \"flat\", \"mode\": \"light\"}", true),
        ("{\"theme\": \"flat\", \"mode\": \"system\"}", true),
        ("{\"theme\": \"flat\", \"mode\": \"\"}", false),
        ("{\"theme\": \"flora\"}", false),
        ("not json", false),
    ] {
        std::fs::write(&file, text).expect("the settings file");
        assert_eq!(super::names_a_mode(&file), named, "{text}");
    }
    let _ = std::fs::remove_dir_all(&dir);
}
