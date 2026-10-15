//! The danger, warning and success inks are the theme's: `system:danger`, `system:warning` and
//! `system:success`, mapped for flat, flora and the system palette by day and by night. An app
//! that paints a problem with a literal red keeps that red in every theme and mode (the flat
//! red read at 2.8:1 on flora's night leaf).

use std::{fs, path::Path};

/// The reds the apps painted problems with before the theme had a danger ink.
const LITERAL_REDS: [&str; 3] = ["#7e4a42", "#b3261e", "#c42b1c"];

/// AzMusic and AzPlayer keep their own look; AzMaps and AzMeet are not swept yet.
const LEFT_OUT: [&str; 4] = ["azul-music", "azul-player", "azul-maps", "azul-meet"];

fn rust_files(dir: &Path, out: &mut Vec<std::path::PathBuf>) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            rust_files(&path, out);
        } else if path.extension().is_some_and(|e| e == "rs") {
            out.push(path);
        }
    }
}

#[test]
fn no_app_paints_a_problem_with_a_literal_red() {
    let examples = Path::new(env!("CARGO_MANIFEST_DIR")).join("..");
    let mut found = Vec::new();
    for app in fs::read_dir(&examples).expect("the examples folder").flatten() {
        let name = app.file_name().to_string_lossy().into_owned();
        if !name.starts_with("azul-") || LEFT_OUT.contains(&name.as_str()) {
            continue;
        }
        let mut files = Vec::new();
        rust_files(&app.path().join("src"), &mut files);
        for file in files {
            let text = fs::read_to_string(&file).unwrap_or_default().to_lowercase();
            for red in LITERAL_REDS {
                if text.contains(red) {
                    found.push(format!("{} has {red}", file.display()));
                }
            }
        }
    }
    assert!(found.is_empty(), "use system:danger / -warning / -success: {found:#?}");
}
