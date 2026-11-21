//! The seed store on a disk: the ONE file of the library that touches `std::fs` (rule H1 of the
//! harness; planning/architecture.md section 1.4 rule 2: a failing seed is a regression test).
//!
//! A store never overwrites: `create` fails with `StoreError::Exists` when the file is there.

use std::io::Write;
use std::path::{Path, PathBuf};

use crate::seeds::{plain_name, SeedStore, StoreError};

/// A directory of `.seed` files.
///
/// Implements planning/architecture.md section 1.4 rule 2.
///
/// Caller: tests/seeds.rs, tests/fast.rs, src/bin/simrun.rs.
#[derive(Clone, Debug)]
pub struct DirSeeds {
    dir: PathBuf,
}

impl DirSeeds {
    /// The store in a directory (it need not exist yet: `names` is then empty and `create` fails).
    pub fn open(dir: &Path) -> DirSeeds {
        DirSeeds {
            dir: dir.to_path_buf(),
        }
    }
}

fn file_name(name: &str) -> Result<String, StoreError> {
    let full = if name.ends_with(".seed") {
        name.to_string()
    } else {
        format!("{name}.seed")
    };
    if plain_name(&full) {
        Ok(full)
    } else {
        Err(StoreError::BadName)
    }
}

impl SeedStore for DirSeeds {
    fn names(&self) -> Result<Vec<String>, StoreError> {
        // gate:fs allow: the seed store is the one file of the harness that reads the disk
        let entries = match std::fs::read_dir(&self.dir) {
            Ok(entries) => entries,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
            Err(error) => return Err(StoreError::Io(error.to_string())),
        };
        let mut names = Vec::new();
        for entry in entries {
            let entry = entry.map_err(|error| StoreError::Io(error.to_string()))?;
            if let Some(name) = entry.file_name().to_str() {
                if let Some(stem) = name.strip_suffix(".seed") {
                    names.push(stem.to_string());
                }
            }
        }
        names.sort();
        Ok(names)
    }

    fn read(&self, name: &str) -> Result<String, StoreError> {
        let path = self.dir.join(file_name(name)?);
        // gate:fs allow: the seed store is the one file of the harness that reads the disk
        match std::fs::read_to_string(&path) {
            Ok(text) => Ok(text),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Err(StoreError::NotFound),
            Err(error) => Err(StoreError::Io(error.to_string())),
        }
    }

    fn create(&mut self, name: &str, text: &str) -> Result<(), StoreError> {
        let path = self.dir.join(file_name(name)?);
        // gate:fs allow: the seed store is the one file of the harness that writes the disk
        let mut file = match std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&path)
        {
            Ok(file) => file,
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
                return Err(StoreError::Exists)
            }
            Err(error) => return Err(StoreError::Io(error.to_string())),
        };
        file.write_all(text.as_bytes())
            .map_err(|error| StoreError::Io(error.to_string()))
    }
}
