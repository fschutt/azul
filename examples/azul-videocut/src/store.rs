//! Where a project lives: files on a Drive, in the per-user layout the S3
//! bucket will have (the azlin cloud storage split) -
//!
//! ```text
//! videocut/<uuid>/project.json     the project (media bin + sequence)
//! videocut/<uuid>/media/<name>     media copied into the project (the sample clips)
//! videocut/<uuid>/exports/<name>   exports
//! ```
//!
//! Today the Drive is a `LocalDrive` on the user's data folder; an
//! `S3Drive` replaces it with no other change. Every call blocks, so the app
//! makes them on an azul `Thread`, never in a callback.

use azul_storage::{Drive, ListRequest};

use crate::model::Project;

/// The app's folder at the root of the user's data.
pub const APP_FOLDER: &str = "videocut";

/// `name` as one key segment: no separators.
fn segment(name: &str) -> String {
    let clean: String = name
        .trim()
        .chars()
        .map(|c| if matches!(c, '/' | '\\' | ':' | '\0') { '-' } else { c })
        .collect();
    if clean.is_empty() {
        String::from("unnamed")
    } else {
        clean
    }
}

/// The key of project `id`'s `project.json`.
#[must_use]
pub fn project_key(id: &str) -> String {
    format!("{APP_FOLDER}/{}/project.json", segment(id))
}

/// The key of a media file `name` copied into project `id`.
#[must_use]
pub fn media_key(id: &str, name: &str) -> String {
    format!("{APP_FOLDER}/{}/media/{}", segment(id), segment(name))
}

/// The key of an export `name` of project `id`.
#[must_use]
pub fn export_key(id: &str, name: &str) -> String {
    format!("{APP_FOLDER}/{}/exports/{}", segment(id), segment(name))
}

/// The project id of a `project.json` key, `None` for any other key.
#[must_use]
pub fn project_id_of(key: &str) -> Option<&str> {
    let rest = key.strip_prefix(APP_FOLDER)?.strip_prefix('/')?;
    let (id, file) = rest.split_once('/')?;
    (file == "project.json" && !id.is_empty()).then_some(id)
}

/// Writes `project.json`.
pub fn save_project(drive: &dyn Drive, project: &Project) -> Result<(), String> {
    drive
        .put(&project_key(&project.id), project.to_json().as_bytes())
        .map_err(|e| format!("saving the project failed: {e}"))
}

/// Reads project `id`.
pub fn load_project(drive: &dyn Drive, id: &str) -> Result<Project, String> {
    let bytes = drive
        .get(&project_key(id))
        .map_err(|e| format!("reading the project failed: {e}"))?;
    let text = String::from_utf8(bytes).map_err(|_| String::from("project.json is not UTF-8"))?;
    Project::from_json(&text)
}

/// The ids of every project on `drive`.
pub fn list_projects(drive: &dyn Drive) -> Result<Vec<String>, String> {
    let mut ids = Vec::new();
    let mut request = ListRequest::recursive(&format!("{APP_FOLDER}/"));
    loop {
        let page = drive
            .list(&request)
            .map_err(|e| format!("listing the projects failed: {e}"))?;
        for o in &page.objects {
            if let Some(id) = project_id_of(&o.key) {
                ids.push(id.to_string());
            }
        }
        match page.next {
            Some(token) => request = request.with_continuation(token),
            None => break,
        }
    }
    Ok(ids)
}

#[cfg(test)]
#[path = "store_tests.rs"]
mod tests;
