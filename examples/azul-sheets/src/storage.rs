//! Where workbooks live: FILES, in the per-user layout the S3 bucket will
//! have (the Azlin cloud storage split): `sheets/<uuid>.xlsx` and, beside it,
//! `sheets/<uuid>.json` - a small sidecar with what the `.xlsx` does not keep
//! (the title, the zoom, the sheet and cell the user was on). IronCalc keeps
//! column widths, row heights and frozen panes in the `.xlsx` itself.
//!
//! Every call goes through azul-storage's [`Drive`] (a `LocalDrive` rooted at
//! the user's data folder today, an `S3Drive` later), and blocks: the app
//! calls these from an azul `Thread`, never from a callback.

use std::path::PathBuf;

use azul_storage::{Drive, DriveError, ListRequest};
use serde::{Deserialize, Serialize};

/// The folder of the workbooks under the data root.
pub const DIR: &str = "sheets";

/// The environment variable that overrides the data root.
pub const DATA_VAR: &str = "AZSHEETS_DATA";

/// A new workbook id: a v4-shaped UUID, lowercase and hyphenated, from the
/// one id mint of the Azlin apps (azul's `Uuid::from_seed` over
/// azul-storage's random seed): it names a file in the drive (an S3 bucket
/// later), so no other launch or device picks it.
#[must_use]
pub fn new_id() -> String {
    azul::uuid::Uuid::from_seed(azul_storage::ids::random_seed())
        .as_str()
        .to_string()
}

/// `sheets/<id>.xlsx`.
#[must_use]
pub fn xlsx_key(id: &str) -> String {
    format!("{DIR}/{id}.xlsx")
}

/// `sheets/<id>.json`.
#[must_use]
pub fn sidecar_key(id: &str) -> String {
    format!("{DIR}/{id}.json")
}

/// The UI state of a workbook that the `.xlsx` does not hold.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Sidecar {
    /// The name the user gave the workbook ("Budget 2027").
    pub title: String,
    /// Percent; 100 = actual size.
    pub zoom: u32,
    /// The sheet that was shown.
    pub sheet: u32,
    /// The active cell (row, column), 1-based.
    pub active: (i32, i32),
    /// The top-left scrolled cell (row, column), 1-based.
    pub top_left: (i32, i32),
    /// Last saved, seconds since 1970-01-01 UTC.
    pub modified: u64,
}

impl Default for Sidecar {
    fn default() -> Self {
        Self {
            title: String::new(),
            zoom: 100,
            sheet: 0,
            active: (1, 1),
            top_left: (1, 1),
            modified: 0,
        }
    }
}

impl Sidecar {
    /// A sidecar for a workbook called `title`, everything else at rest.
    #[must_use]
    pub fn titled(title: &str) -> Self {
        Self {
            title: title.to_string(),
            ..Self::default()
        }
    }
}

/// Now, in seconds since 1970-01-01 UTC (0 if the clock is before it).
#[must_use]
pub fn now_secs() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs())
}

/// Writes the workbook and its sidecar.
pub fn save(drive: &dyn Drive, id: &str, xlsx: &[u8], sidecar: &Sidecar) -> Result<(), DriveError> {
    let json = serde_json::to_vec_pretty(sidecar)
        .map_err(|e| DriveError::Protocol(format!("the sidecar: {e}")))?;
    drive.put(&xlsx_key(id), xlsx)?;
    drive.put(&sidecar_key(id), &json)
}

/// The sidecar of `id`, or one titled by the id when it is missing or does
/// not parse (a workbook copied in by hand still opens).
fn read_sidecar(drive: &dyn Drive, id: &str) -> Result<Sidecar, DriveError> {
    match drive.get(&sidecar_key(id)) {
        Ok(bytes) => Ok(serde_json::from_slice(&bytes).unwrap_or_else(|_| Sidecar::titled(id))),
        Err(DriveError::NotFound { .. }) => Ok(Sidecar::titled(id)),
        Err(e) => Err(e),
    }
}

/// Reads the workbook and its sidecar.
pub fn load(drive: &dyn Drive, id: &str) -> Result<(Vec<u8>, Sidecar), DriveError> {
    let bytes = drive.get(&xlsx_key(id))?;
    Ok((bytes, read_sidecar(drive, id)?))
}

/// Every workbook under `sheets/`, newest first (by the sidecar's
/// `modified`, else the file's date).
pub fn list(drive: &dyn Drive) -> Result<Vec<(String, Sidecar)>, DriveError> {
    let prefix = format!("{DIR}/");
    let mut request = ListRequest::folder(&prefix);
    let mut out = Vec::new();
    loop {
        let page = drive.list(&request)?;
        for object in &page.objects {
            let Some(id) = object.name().strip_suffix(".xlsx") else {
                continue;
            };
            let mut sidecar = read_sidecar(drive, id)?;
            if sidecar.modified == 0 {
                sidecar.modified = object.modified.unwrap_or(0);
            }
            out.push((id.to_string(), sidecar));
        }
        match page.next {
            Some(next) => request = ListRequest::folder(&prefix).with_continuation(next),
            None => break,
        }
    }
    out.sort_by(|a, b| b.1.modified.cmp(&a.1.modified).then_with(|| a.0.cmp(&b.0)));
    Ok(out)
}

/// The data root: `var` (`AZSHEETS_DATA`) when set, else `Azlin` in the
/// user's data folder, else `./azsheets-data`.
#[must_use]
pub fn data_root(var: Option<&str>, user_data_dir: Option<PathBuf>) -> PathBuf {
    match var.map(str::trim).filter(|v| !v.is_empty()) {
        Some(v) => PathBuf::from(v),
        None => user_data_dir.map_or_else(|| PathBuf::from("azsheets-data"), |d| d.join("Azlin")),
    }
}

#[cfg(test)]
mod tests {
    use azul_storage::LocalDrive;

    use super::*;

    /// A LocalDrive on a fresh temporary folder, removed on drop.
    struct TempDrive {
        drive: LocalDrive,
        root: PathBuf,
    }

    impl TempDrive {
        fn new() -> Self {
            let root = std::env::temp_dir().join(format!("azsheets-storage-{}", new_id()));
            Self {
                drive: LocalDrive::new(root.clone()),
                root,
            }
        }
    }

    impl Drop for TempDrive {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.root);
        }
    }

    #[test]
    fn a_workbook_is_an_xlsx_and_a_json_sidecar_in_the_sheets_folder() {
        let id = new_id();
        assert_eq!(id.len(), 36);
        assert_eq!(id, id.to_lowercase());
        assert_eq!(xlsx_key(&id), format!("sheets/{id}.xlsx"));
        assert_eq!(sidecar_key("abc"), "sheets/abc.json");
        assert_ne!(new_id(), id);
    }

    #[test]
    fn the_sidecar_round_trips_through_json_and_fills_what_is_missing() {
        let sidecar = Sidecar {
            title: String::from("Budget 2027"),
            zoom: 125,
            sheet: 2,
            active: (7, 2),
            top_left: (3, 1),
            modified: 1_790_000_000,
        };
        let json = serde_json::to_string(&sidecar).unwrap();
        assert_eq!(serde_json::from_str::<Sidecar>(&json).unwrap(), sidecar);
        let partial: Sidecar = serde_json::from_str(r#"{"title":"Old"}"#).unwrap();
        assert_eq!(partial.zoom, 100);
        assert_eq!(partial.active, (1, 1));
    }

    #[test]
    fn the_data_root_is_the_variable_then_the_user_data_folder_then_a_local_folder() {
        assert_eq!(
            data_root(Some("/tmp/x"), Some(PathBuf::from("/home/u/.local/share"))),
            PathBuf::from("/tmp/x")
        );
        assert_eq!(
            data_root(Some("  "), Some(PathBuf::from("/data"))),
            PathBuf::from("/data/Azlin")
        );
        assert_eq!(data_root(None, None), PathBuf::from("azsheets-data"));
    }

    #[test]
    fn a_saved_workbook_loads_back_with_its_sidecar() {
        let t = TempDrive::new();
        let sidecar = Sidecar::titled("Trip");
        save(&t.drive, "w1", b"xlsx bytes", &sidecar).unwrap();
        let (bytes, back) = load(&t.drive, "w1").unwrap();
        assert_eq!(bytes, b"xlsx bytes");
        assert_eq!(back, sidecar);
        assert!(t.root.join("sheets").join("w1.xlsx").is_file());
        assert!(t.root.join("sheets").join("w1.json").is_file());
    }

    #[test]
    fn a_workbook_without_a_sidecar_is_titled_by_its_id() {
        let t = TempDrive::new();
        t.drive.put(&xlsx_key("bare"), b"x").unwrap();
        let (_, sidecar) = load(&t.drive, "bare").unwrap();
        assert_eq!(sidecar.title, "bare");
        assert!(matches!(
            load(&t.drive, "missing"),
            Err(DriveError::NotFound { .. })
        ));
    }

    #[test]
    fn the_list_has_every_workbook_newest_first_and_skips_other_files() {
        let t = TempDrive::new();
        assert!(list(&t.drive).unwrap().is_empty(), "no folder yet");
        for (id, modified) in [("old", 100), ("new", 300), ("mid", 200)] {
            let mut sidecar = Sidecar::titled(id);
            sidecar.modified = modified;
            save(&t.drive, id, b"x", &sidecar).unwrap();
        }
        t.drive.put("sheets/notes.txt", b"not a workbook").unwrap();
        let ids: Vec<String> = list(&t.drive)
            .unwrap()
            .into_iter()
            .map(|(id, _)| id)
            .collect();
        assert_eq!(ids, vec!["new", "mid", "old"]);
    }
}
