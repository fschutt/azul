//! AzBuilder projects: a folder on disk that the builder UI browses, edits
//! and saves into (the `project_*` ops).
//!
//! The builder UI (`dll/src/desktop/shell2/common/debugger/debugger-project.js`)
//! shows the folder as a Qt-Creator-style project tree with an editor; this
//! module is the only code that touches the folder. A project is:
//!
//! ```text
//! <root>/azul-project.json               manifest {format, version, name}
//! <root>/document.json                   the builder document (no uids)
//! <root>/components/<library>/<name>.json   one file per user component
//! <root>/styles/**/*.css                 stylesheets, mounted in path order
//! <root>/tests/*.json                    E2E tests, one per file
//! <root>/snapshots/*.json                app-state snapshots
//! <root>/export/<language>/…             exported code
//! ```
//!
//! **Confinement.** Every path an op takes is RELATIVE to the root. A path
//! with `..`, an absolute path, a NUL byte, and a path that — through a
//! symlink — really leads outside the (canonical) root are refused before
//! anything is read or written; a zip is checked entry by entry before its
//! first byte is written. `\` counts as a separator, so `a\..\..\x` is a `..`
//! path on every OS.
//!
//! **Live files.** Writing a file the builder uses applies it: a stylesheet
//! under `styles/` re-mounts the document with the new CSS, a component file
//! re-registers the component (every instance on screen updates), and
//! `document.json` loads the document. A file that does not apply is still
//! SAVED — it is the user's text — and the answer says why it did not apply.

use std::path::{Path, PathBuf};

use azul_core::{
    styled_dom::StyledDom,
    xml::{
        ComponentDataFieldVec, ComponentDataModel, ComponentDef, ComponentDefVec, ComponentId,
        ComponentLibrary, ComponentLibraryVec, ComponentMap, ComponentSource,
    },
};
use azul_css::AzString;
use serde_json::{json, Value};

use super::builder::{self, BuilderNode, BuilderReply, BuilderSession, Remount};

/// The manifest file at the project root.
pub const MANIFEST: &str = "azul-project.json";
/// The builder document.
pub const DOCUMENT: &str = "document.json";
const COMPONENTS: &str = "components";
const STYLES: &str = "styles";
/// The folders a new project starts with.
const SKELETON: &[&str] = &["components", "styles", "tests", "snapshots"];
/// Never listed, zipped or walked into.
const SKIP: &[&str] = &[".git", "target", "node_modules", ".DS_Store"];

const MANIFEST_FORMAT: &str = "azul-project";
const DOCUMENT_FORMAT: &str = "azul-builder-document";
const COMPONENT_FORMAT: &str = "azul-component";

/// Largest file `project_read_file` answers with.
const MAX_READ: u64 = 8 * 1024 * 1024;
/// Largest file `project_write_file` / a zip entry writes.
const MAX_WRITE: usize = 16 * 1024 * 1024;
/// Largest project `project_export_zip` packs / an import unpacks.
const MAX_ARCHIVE: u64 = 256 * 1024 * 1024;
const MAX_TREE_ENTRIES: usize = 5000;
const MAX_TREE_DEPTH: usize = 16;

// ===========================================================================
// The ops
// ===========================================================================

/// One `project_*` op, as `full.rs` hands it over.
#[derive(Debug, Clone, Copy)]
pub enum ProjectOp<'a> {
    Info,
    Open {
        path: &'a str,
        create: bool,
    },
    Close,
    List,
    Read {
        path: &'a str,
    },
    Write {
        path: &'a str,
        content: &'a str,
        encoding: Option<&'a str>,
    },
    Create {
        path: &'a str,
        directory: bool,
        content: Option<&'a str>,
    },
    Rename {
        from: &'a str,
        to: &'a str,
    },
    Delete {
        path: &'a str,
    },
    Save,
    Load,
    ExportZip,
    ImportZip {
        data: &'a str,
    },
}

/// Run one op. The answer is the JSON for the UI plus what the window must
/// show afterwards (a stylesheet, component or document that changed
/// re-mounts the builder document).
///
/// # Errors
/// No project open (except `project_info` / `project_open`), a refused path,
/// an I/O error, a malformed file or archive.
pub fn handle(
    op: ProjectOp<'_>,
    project: &mut ProjectSession,
    builder: &mut BuilderSession,
    map: &mut ComponentMap,
    live: Option<&StyledDom>,
) -> Result<BuilderReply, String> {
    let keep = |json: Value| BuilderReply {
        json,
        remount: Remount::Keep,
    };
    match op {
        ProjectOp::Info => Ok(keep(project.info_json())),
        ProjectOp::Open { path, create } => {
            let created = project.open(path, create)?;
            let remount = project.apply_stylesheets(builder, map);
            let mut json = project.info_json();
            json["created"] = json!(created);
            Ok(BuilderReply { json, remount })
        }
        ProjectOp::Close => {
            project.root = None;
            let remount = builder.set_stylesheet(map, String::new());
            Ok(BuilderReply {
                json: project.info_json(),
                remount,
            })
        }
        ProjectOp::List => {
            let root = project.root()?;
            Ok(keep(json!({
                "root": root.display().to_string(),
                "name": project.name(),
                "tree": tree(root),
            })))
        }
        ProjectOp::Read { path } => project.read_file(path).map(keep),
        ProjectOp::Write {
            path,
            content,
            encoding,
        } => {
            let (norm, size) = project.write_file(path, content, encoding)?;
            let applied = project.apply_written(&norm, content, builder, map);
            let mut json = json!({
                "path": norm,
                "size": size,
                "written": true,
                "applied": applied.what,
            });
            if let Some(e) = applied.error {
                json["apply_error"] = json!(e);
            }
            if let Some(doc) = applied.document {
                json["document"] = doc;
            }
            Ok(BuilderReply {
                json,
                remount: applied.remount,
            })
        }
        ProjectOp::Create {
            path,
            directory,
            content,
        } => {
            let json = project.create(path, directory, content)?;
            let remount = project.apply_stylesheets(builder, map);
            Ok(BuilderReply { json, remount })
        }
        ProjectOp::Rename { from, to } => {
            let json = project.rename(from, to)?;
            let remount = project.apply_stylesheets(builder, map);
            Ok(BuilderReply { json, remount })
        }
        ProjectOp::Delete { path } => {
            let json = project.delete(path)?;
            let remount = project.apply_stylesheets(builder, map);
            Ok(BuilderReply { json, remount })
        }
        ProjectOp::Save => project.save(builder, map, live).map(keep),
        ProjectOp::Load => project.load(builder, map),
        ProjectOp::ExportZip => project.export_zip().map(keep),
        ProjectOp::ImportZip { data } => {
            let json = project.import_zip(data)?;
            let remount = project.apply_stylesheets(builder, map);
            Ok(BuilderReply { json, remount })
        }
    }
}

// ===========================================================================
// The session
// ===========================================================================

/// This window's open project (one per window, like the builder document).
#[derive(Debug, Default)]
pub struct ProjectSession {
    /// The CANONICAL root folder; every resolved path must stay under it.
    root: Option<PathBuf>,
}

/// What writing a file did beyond writing it.
struct Applied {
    what: Option<&'static str>,
    error: Option<String>,
    document: Option<Value>,
    remount: Remount,
}

impl ProjectSession {
    fn root(&self) -> Result<&Path, String> {
        self.root
            .as_deref()
            .ok_or_else(|| "no project is open; open a folder first (project_open)".to_string())
    }

    /// The manifest's `name`, else the folder's name.
    fn name(&self) -> String {
        let Some(root) = self.root.as_deref() else {
            return String::new();
        };
        std::fs::read_to_string(root.join(MANIFEST))
            .ok()
            .and_then(|s| serde_json::from_str::<Value>(&s).ok())
            .and_then(|m| m.get("name").and_then(Value::as_str).map(str::to_string))
            .filter(|n| !n.trim().is_empty())
            .unwrap_or_else(|| folder_name(root))
    }

    fn info_json(&self) -> Value {
        let cwd = std::env::current_dir().unwrap_or_default();
        let mut info = json!({
            "open": self.root.is_some(),
            "cwd": cwd.display().to_string(),
            "suggested": cwd.join("AzBuilderProject").display().to_string(),
        });
        if let Some(root) = self.root.as_deref() {
            let manifest = std::fs::read_to_string(root.join(MANIFEST))
                .ok()
                .and_then(|s| serde_json::from_str::<Value>(&s).ok())
                .unwrap_or(Value::Null);
            info["root"] = json!(root.display().to_string());
            info["name"] = json!(self.name());
            info["manifest"] = manifest;
            info["tree"] = tree(root);
        }
        info
    }

    /// Open `path` (absolute, `~/…`, or relative to the working directory).
    /// `create`: make the folder if it is missing and give it the project
    /// skeleton. Answers whether the manifest was written now.
    fn open(&mut self, path: &str, create: bool) -> Result<bool, String> {
        let raw = path.trim();
        if raw.is_empty() {
            return Err("name the project folder to open".to_string());
        }
        if raw.contains('\0') {
            return Err(format!("{raw:?} contains a NUL byte"));
        }
        let mut p = expand_home(raw);
        if !p.is_absolute() {
            let cwd = std::env::current_dir()
                .map_err(|e| format!("cannot read the working directory: {e}"))?;
            p = cwd.join(p);
        }
        if !p.exists() {
            if !create {
                return Err(format!(
                    "no folder at {} (open it with \"create\": true to start a new project there)",
                    p.display()
                ));
            }
            std::fs::create_dir_all(&p)
                .map_err(|e| format!("cannot create {}: {e}", p.display()))?;
        }
        if !p.is_dir() {
            return Err(format!("{} is a file, not a folder", p.display()));
        }
        let root = p
            .canonicalize()
            .map_err(|e| format!("cannot open {}: {e}", p.display()))?;
        let mut created = false;
        if create {
            for dir in SKELETON {
                std::fs::create_dir_all(root.join(dir))
                    .map_err(|e| format!("cannot create {dir}/ in the project: {e}"))?;
            }
            if !root.join(MANIFEST).exists() {
                let manifest = json!({
                    "format": MANIFEST_FORMAT,
                    "version": 1,
                    "name": folder_name(&root),
                    "created_with": "AzBuilder",
                });
                write_json(&root.join(MANIFEST), &manifest)?;
                created = true;
            }
        }
        self.root = Some(root);
        Ok(created)
    }

    /// `rel` as a path inside the root: `(path, normalised rel)`. The empty
    /// path (also `.`) names the root itself.
    ///
    /// # Errors
    /// No project open, a `..`, an absolute path, a NUL byte, or a path that
    /// really leads outside the root (a symlink on the way).
    fn resolve(&self, rel: &str) -> Result<(PathBuf, String), String> {
        let root = self.root()?;
        let parts = rel_components(rel)?;
        let mut path = root.to_path_buf();
        for p in &parts {
            path.push(p);
        }
        ensure_inside(root, &path, rel)?;
        Ok((path, parts.join("/")))
    }

    fn read_file(&self, rel: &str) -> Result<Value, String> {
        let (path, norm) = self.resolve(rel)?;
        if norm.is_empty() {
            return Err("the project root is a folder, not a file".to_string());
        }
        let meta = std::fs::metadata(&path).map_err(|e| format!("cannot read {norm:?}: {e}"))?;
        if meta.is_dir() {
            return Err(format!("{norm:?} is a folder, not a file"));
        }
        if meta.len() > MAX_READ {
            return Err(format!(
                "{norm:?} is {} bytes; the editor opens files up to {MAX_READ} bytes",
                meta.len()
            ));
        }
        let bytes = std::fs::read(&path).map_err(|e| format!("cannot read {norm:?}: {e}"))?;
        let size = bytes.len();
        Ok(match String::from_utf8(bytes) {
            Ok(text) => json!({ "path": norm, "size": size, "binary": false, "content": text }),
            Err(e) => json!({
                "path": norm,
                "size": size,
                "binary": true,
                "encoding": "base64",
                "content": crate::callbacks::base64_encode(e.as_bytes()),
            }),
        })
    }

    /// Write `content` (UTF-8 text, or `encoding: "base64"`), creating the
    /// folders on the way. Answers `(normalised path, bytes written)`.
    fn write_file(
        &self,
        rel: &str,
        content: &str,
        encoding: Option<&str>,
    ) -> Result<(String, usize), String> {
        let (path, norm) = self.resolve(rel)?;
        if norm.is_empty() {
            return Err("the project root is a folder; name a file to write".to_string());
        }
        if path.is_dir() {
            return Err(format!("{norm:?} is a folder, not a file"));
        }
        let bytes = match encoding {
            None | Some("utf-8" | "utf8" | "text") => content.as_bytes().to_vec(),
            Some("base64") => decode_base64(content)?,
            Some(other) => {
                return Err(format!(
                    "unknown encoding {other:?} (\"utf-8\" or \"base64\")"
                ))
            }
        };
        write_bytes(&path, &norm, &bytes)?;
        Ok((norm, bytes.len()))
    }

    fn create(&self, rel: &str, directory: bool, content: Option<&str>) -> Result<Value, String> {
        let (path, norm) = self.resolve(rel)?;
        if norm.is_empty() {
            return Err("the project root already exists".to_string());
        }
        if std::fs::symlink_metadata(&path).is_ok() {
            return Err(format!("{norm:?} already exists"));
        }
        if directory {
            std::fs::create_dir_all(&path).map_err(|e| format!("cannot create {norm:?}: {e}"))?;
        } else {
            write_bytes(&path, &norm, content.unwrap_or("").as_bytes())?;
        }
        Ok(json!({ "path": norm, "kind": if directory { "dir" } else { "file" } }))
    }

    fn rename(&self, from: &str, to: &str) -> Result<Value, String> {
        let (src, from_n) = self.resolve(from)?;
        let (dst, to_n) = self.resolve(to)?;
        if from_n.is_empty() || to_n.is_empty() {
            return Err("the project root itself cannot be renamed".to_string());
        }
        if std::fs::symlink_metadata(&src).is_err() {
            return Err(format!("{from_n:?} does not exist"));
        }
        if std::fs::symlink_metadata(&dst).is_ok() {
            return Err(format!("{to_n:?} already exists"));
        }
        if to_n.starts_with(&format!("{from_n}/")) {
            return Err(format!("{from_n:?} cannot move into itself"));
        }
        if let Some(parent) = dst.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|e| format!("cannot create the folder for {to_n:?}: {e}"))?;
        }
        std::fs::rename(&src, &dst)
            .map_err(|e| format!("cannot rename {from_n:?} to {to_n:?}: {e}"))?;
        Ok(json!({ "from": from_n, "to": to_n }))
    }

    fn delete(&self, rel: &str) -> Result<Value, String> {
        let (path, norm) = self.resolve(rel)?;
        if norm.is_empty() {
            return Err("the project root itself cannot be deleted".to_string());
        }
        let meta =
            std::fs::symlink_metadata(&path).map_err(|_| format!("{norm:?} does not exist"))?;
        // `remove_dir_all` removes a symlink inside the folder, never what it
        // points at.
        let removed = if meta.is_dir() {
            std::fs::remove_dir_all(&path)
        } else {
            std::fs::remove_file(&path)
        };
        removed.map_err(|e| format!("cannot delete {norm:?}: {e}"))?;
        Ok(json!({ "path": norm, "deleted": true }))
    }

    // ── live files ──

    /// Every `styles/**/*.css`, in path order, as one stylesheet.
    fn read_stylesheets(&self) -> (String, Vec<String>) {
        let Some(root) = self.root.as_deref() else {
            return (String::new(), Vec::new());
        };
        let mut files = Vec::new();
        collect_files(&root.join(STYLES), STYLES, 0, &mut files);
        files.retain(|(rel, _)| rel.to_ascii_lowercase().ends_with(".css"));
        files.sort();
        let mut css = String::new();
        let mut names = Vec::new();
        for (rel, path) in files {
            if let Ok(text) = std::fs::read_to_string(&path) {
                css.push_str(&format!("/* {rel} */\n"));
                css.push_str(&text);
                css.push('\n');
                names.push(rel);
            }
        }
        (css, names)
    }

    /// Re-read the stylesheets and hand them to the builder (a remount if
    /// they changed and the document is on screen).
    fn apply_stylesheets(&self, builder: &mut BuilderSession, map: &ComponentMap) -> Remount {
        let (css, _) = self.read_stylesheets();
        builder.set_stylesheet(map, css)
    }

    fn apply_written(
        &self,
        norm: &str,
        content: &str,
        builder: &mut BuilderSession,
        map: &mut ComponentMap,
    ) -> Applied {
        let lower = norm.to_ascii_lowercase();
        let mut applied = Applied {
            what: None,
            error: None,
            document: None,
            remount: Remount::Keep,
        };
        if lower.starts_with("styles/") && lower.ends_with(".css") {
            applied.what = Some("stylesheet");
            applied.remount = self.apply_stylesheets(builder, map);
        } else if lower.starts_with("components/") && lower.ends_with(".json") {
            match serde_json::from_str::<Value>(content)
                .map_err(|e| format!("not JSON: {e}"))
                .and_then(|v| apply_component(map, &v, component_path_names(norm)))
            {
                Ok(_) => {
                    applied.what = Some("component");
                    applied.remount = builder
                        .remount_xml(map)
                        .map_or(Remount::Keep, Remount::Mount);
                }
                Err(e) => applied.error = Some(e),
            }
        } else if norm == DOCUMENT {
            match serde_json::from_str::<Value>(content)
                .map_err(|e| format!("not JSON: {e}"))
                .and_then(|v| document_root(&v))
            {
                Ok(node) => {
                    let reply = builder.load_document(map, node);
                    applied.what = Some("document");
                    applied.document = Some(reply.json);
                    applied.remount = reply.remount;
                }
                Err(e) => applied.error = Some(e),
            }
        }
        applied
    }

    // ── save / load ──

    fn save(
        &self,
        builder: &BuilderSession,
        map: &ComponentMap,
        live: Option<&StyledDom>,
    ) -> Result<Value, String> {
        let root = self.root()?.to_path_buf();
        let mut written: Vec<String> = Vec::new();
        let mut errors: Vec<String> = Vec::new();

        if !root.join(MANIFEST).exists() {
            let manifest = json!({
                "format": MANIFEST_FORMAT,
                "version": 1,
                "name": folder_name(&root),
                "created_with": "AzBuilder",
            });
            write_json(&root.join(MANIFEST), &manifest)?;
            written.push(MANIFEST.to_string());
        }

        let doc = document_file_json(&builder.root_for_save(live));
        let text = serde_json::to_string_pretty(&doc).map_err(|e| e.to_string())?;
        self.write_file(DOCUMENT, &text, None)?;
        written.push(DOCUMENT.to_string());

        for lib in map.libraries.as_ref() {
            if !lib.modifiable {
                continue;
            }
            let library = lib.name.as_str();
            for def in lib.components.as_ref() {
                let name = def.id.name.as_str();
                if !is_safe_segment(library) || !is_safe_segment(name) {
                    errors.push(format!(
                        "{library}:{name} was not saved: its name cannot be a file name"
                    ));
                    continue;
                }
                let rel = format!("{COMPONENTS}/{library}/{name}.json");
                let text = serde_json::to_string_pretty(&component_file_json(library, def))
                    .map_err(|e| e.to_string())?;
                match self.write_file(&rel, &text, None) {
                    Ok(_) => written.push(rel),
                    Err(e) => errors.push(format!("{rel}: {e}")),
                }
            }
        }

        Ok(json!({
            "written": written,
            "errors": errors,
            "tree": tree(&root),
        }))
    }

    /// Components first (the document uses them), then the stylesheets,
    /// then the document — which takes the window over.
    fn load(
        &self,
        builder: &mut BuilderSession,
        map: &mut ComponentMap,
    ) -> Result<BuilderReply, String> {
        let root = self.root()?.to_path_buf();
        let mut errors: Vec<String> = Vec::new();

        let mut files = Vec::new();
        collect_files(&root.join(COMPONENTS), COMPONENTS, 0, &mut files);
        files.retain(|(rel, _)| rel.to_ascii_lowercase().ends_with(".json"));
        files.sort();
        let mut components: Vec<String> = Vec::new();
        for (rel, path) in files {
            let result = std::fs::read_to_string(&path)
                .map_err(|e| format!("cannot read: {e}"))
                .and_then(|s| {
                    serde_json::from_str::<Value>(&s).map_err(|e| format!("not JSON: {e}"))
                })
                .and_then(|v| apply_component(map, &v, component_path_names(&rel)));
            match result {
                Ok(key) => components.push(key),
                Err(e) => errors.push(format!("{rel}: {e}")),
            }
        }

        let (css, stylesheets) = self.read_stylesheets();
        let mut remount = builder.set_stylesheet(map, css);

        let mut document = Value::Null;
        let doc_path = root.join(DOCUMENT);
        if doc_path.is_file() {
            let result = std::fs::read_to_string(&doc_path)
                .map_err(|e| format!("cannot read: {e}"))
                .and_then(|s| {
                    serde_json::from_str::<Value>(&s).map_err(|e| format!("not JSON: {e}"))
                })
                .and_then(|v| document_root(&v));
            match result {
                Ok(node) => {
                    let reply = builder.load_document(map, node);
                    document = reply.json;
                    remount = reply.remount;
                }
                Err(e) => errors.push(format!("{DOCUMENT}: {e}")),
            }
        }
        // Components changed under a document that stays: show them.
        if matches!(remount, Remount::Keep) && !components.is_empty() {
            if let Some(xml) = builder.remount_xml(map) {
                remount = Remount::Mount(xml);
            }
        }

        Ok(BuilderReply {
            json: json!({
                "document": document,
                "components": components,
                "stylesheets": stylesheets,
                "errors": errors,
            }),
            remount,
        })
    }

    // ── zip ──

    fn export_zip(&self) -> Result<Value, String> {
        let root = self.root()?;
        let mut files = Vec::new();
        collect_files(root, "", 0, &mut files);
        files.sort();
        let mut total: u64 = 0;
        let mut entries: Vec<(String, Vec<u8>)> = Vec::with_capacity(files.len());
        for (rel, path) in files {
            let bytes = std::fs::read(&path).map_err(|e| format!("cannot read {rel:?}: {e}"))?;
            total += bytes.len() as u64;
            if total > MAX_ARCHIVE {
                return Err(format!(
                    "the project is larger than {MAX_ARCHIVE} bytes; zip it outside AzBuilder"
                ));
            }
            entries.push((rel, bytes));
        }
        let count = entries.len();
        let bytes =
            crate::zip::zip_create_from_files(entries, &crate::zip::ZipWriteConfig::default())
                .map_err(|e| format!("cannot build the zip: {e}"))?;
        let stem: String = self
            .name()
            .chars()
            .map(|c| {
                if c.is_ascii_alphanumeric() || c == '-' || c == '_' {
                    c
                } else {
                    '-'
                }
            })
            .collect();
        Ok(json!({
            "download_url": format!(
                "data:application/zip;base64,{}",
                crate::callbacks::base64_encode(&bytes)
            ),
            "filename": format!("{}.zip", if stem.is_empty() { "project" } else { stem.as_str() }),
            "size_bytes": bytes.len(),
            "file_count": count,
        }))
    }

    /// Unpack a zip (base64 or a `data:` URI) into the project. Every entry
    /// is checked first; one refused entry refuses the archive and nothing
    /// is written. A single top-level folder holding the manifest (a project
    /// zipped with its folder) is stripped.
    fn import_zip(&self, data: &str) -> Result<Value, String> {
        let root = self.root()?.to_path_buf();
        let b64 = match data.trim().split_once(',') {
            Some((head, rest)) if head.starts_with("data:") => rest,
            _ => data.trim(),
        };
        let bytes = decode_base64(b64)?;
        // Our own entry check below is the one that counts (it also refuses
        // absolute paths); the reader's `..` check is switched off so the
        // refusal names the entry the same way every other op does.
        let config = crate::zip::ZipReadConfig::default().with_allow_path_traversal(true);
        let zip = crate::zip::ZipFile::from_bytes(&bytes, &config)
            .map_err(|e| format!("not a zip archive: {e}"))?;

        let prefix = common_project_prefix(&zip.entries);
        let mut plan: Vec<(PathBuf, String, bool, &[u8])> = Vec::new();
        let mut total: u64 = 0;
        for entry in &zip.entries {
            let name = entry.path.as_str();
            let rel = prefix
                .as_deref()
                .and_then(|p| name.strip_prefix(p))
                .unwrap_or(name);
            let (path, norm) = self
                .resolve(rel)
                .map_err(|e| format!("the archive entry {name:?} is refused: {e}"))?;
            if norm.is_empty() {
                continue;
            }
            if entry.data.len() > MAX_WRITE {
                return Err(format!(
                    "the archive entry {name:?} is larger than {MAX_WRITE} bytes"
                ));
            }
            total += entry.data.len() as u64;
            if total > MAX_ARCHIVE {
                return Err(format!(
                    "the archive unpacks to more than {MAX_ARCHIVE} bytes"
                ));
            }
            plan.push((path, norm, entry.is_directory, entry.data.as_slice()));
        }

        let mut written = Vec::new();
        for (path, norm, is_dir, data) in plan {
            if is_dir {
                std::fs::create_dir_all(&path)
                    .map_err(|e| format!("cannot create {norm:?}: {e}"))?;
            } else {
                write_bytes(&path, &norm, data)?;
                written.push(norm);
            }
        }
        Ok(json!({ "written": written, "tree": tree(&root) }))
    }
}

// ===========================================================================
// Paths
// ===========================================================================

/// The components of a project-relative path, or why it is not one.
fn rel_components(rel: &str) -> Result<Vec<String>, String> {
    if rel.contains('\0') {
        return Err(format!("{rel:?} contains a NUL byte"));
    }
    let unified = rel.trim().replace('\\', "/");
    let bytes = unified.as_bytes();
    let drive = bytes.len() >= 2 && bytes[1] == b':' && bytes[0].is_ascii_alphabetic();
    if unified.starts_with('/') || drive || Path::new(rel).is_absolute() {
        return Err(format!(
            "{rel:?} is an absolute path; project paths are relative to the project root"
        ));
    }
    let mut out = Vec::new();
    for part in unified.split('/') {
        match part {
            "" | "." => {}
            ".." => {
                return Err(format!(
                    "{rel:?} contains '..'; project paths may not leave the project root"
                ))
            }
            p => out.push(p.to_string()),
        }
    }
    Ok(out)
}

/// Where `path` really leads must be inside `root` (canonical), symlinks
/// resolved: the longest existing prefix of `path` is canonicalised and
/// checked (what does not exist yet is created under it).
fn ensure_inside(root: &Path, path: &Path, rel: &str) -> Result<(), String> {
    let mut existing = path.to_path_buf();
    while std::fs::symlink_metadata(&existing).is_err() {
        if !existing.pop() {
            break;
        }
    }
    let real = existing
        .canonicalize()
        .map_err(|e| format!("{rel:?} resolves outside the project root (cannot resolve: {e})"))?;
    if real.starts_with(root) {
        Ok(())
    } else {
        Err(format!("{rel:?} resolves outside the project root"))
    }
}

/// A library or component name that is one plain path segment.
fn is_safe_segment(s: &str) -> bool {
    !s.is_empty() && s != "." && s != ".." && !s.contains(['/', '\\', '\0', ':'])
}

fn expand_home(p: &str) -> PathBuf {
    if let Some(rest) = p.strip_prefix("~/").or_else(|| (p == "~").then_some("")) {
        if let Some(home) = std::env::var_os("HOME").or_else(|| std::env::var_os("USERPROFILE")) {
            return PathBuf::from(home).join(rest);
        }
    }
    PathBuf::from(p)
}

fn folder_name(p: &Path) -> String {
    p.file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| "project".to_string())
}

fn write_bytes(path: &Path, norm: &str, bytes: &[u8]) -> Result<(), String> {
    if bytes.len() > MAX_WRITE {
        return Err(format!("{norm:?} is larger than {MAX_WRITE} bytes"));
    }
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| format!("cannot create the folder for {norm:?}: {e}"))?;
    }
    std::fs::write(path, bytes).map_err(|e| format!("cannot write {norm:?}: {e}"))
}

fn write_json(path: &Path, v: &Value) -> Result<(), String> {
    let text = serde_json::to_string_pretty(v).map_err(|e| e.to_string())?;
    std::fs::write(path, text).map_err(|e| format!("cannot write {}: {e}", path.display()))
}

/// The project as the tree the UI renders: folders first, then files, by
/// name. Symlinks are listed (`kind: "link"`) but never followed.
fn tree(root: &Path) -> Value {
    let mut count = 0;
    json!({
        "name": folder_name(root),
        "path": "",
        "kind": "dir",
        "children": list_dir(root, "", 0, &mut count),
    })
}

fn list_dir(dir: &Path, rel: &str, depth: usize, count: &mut usize) -> Vec<Value> {
    let Ok(rd) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut entries: Vec<(u8, String, PathBuf, std::fs::Metadata)> = Vec::new();
    for e in rd.flatten() {
        let name = e.file_name().to_string_lossy().into_owned();
        if SKIP.contains(&name.as_str()) {
            continue;
        }
        let Ok(meta) = std::fs::symlink_metadata(e.path()) else {
            continue;
        };
        let order = if meta.file_type().is_symlink() {
            1
        } else if meta.is_dir() {
            0
        } else {
            1
        };
        entries.push((order, name, e.path(), meta));
    }
    entries.sort_by(|a, b| {
        (a.0, a.1.to_ascii_lowercase(), &a.1).cmp(&(b.0, b.1.to_ascii_lowercase(), &b.1))
    });
    let mut out = Vec::new();
    for (_, name, path, meta) in entries {
        *count += 1;
        if *count > MAX_TREE_ENTRIES {
            break;
        }
        let child_rel = if rel.is_empty() {
            name.clone()
        } else {
            format!("{rel}/{name}")
        };
        if meta.file_type().is_symlink() {
            out.push(json!({ "name": name, "path": child_rel, "kind": "link" }));
        } else if meta.is_dir() {
            let children = if depth < MAX_TREE_DEPTH {
                list_dir(&path, &child_rel, depth + 1, count)
            } else {
                Vec::new()
            };
            out.push(
                json!({ "name": name, "path": child_rel, "kind": "dir", "children": children }),
            );
        } else {
            out.push(
                json!({ "name": name, "path": child_rel, "kind": "file", "size": meta.len() }),
            );
        }
    }
    out
}

/// Every regular file under `dir` (symlinks and SKIP names left out) as
/// `(project-relative path, path)`.
fn collect_files(dir: &Path, rel: &str, depth: usize, out: &mut Vec<(String, PathBuf)>) {
    if depth > MAX_TREE_DEPTH || out.len() > MAX_TREE_ENTRIES {
        return;
    }
    let Ok(rd) = std::fs::read_dir(dir) else {
        return;
    };
    for e in rd.flatten() {
        let name = e.file_name().to_string_lossy().into_owned();
        if SKIP.contains(&name.as_str()) {
            continue;
        }
        let Ok(meta) = std::fs::symlink_metadata(e.path()) else {
            continue;
        };
        let child_rel = if rel.is_empty() {
            name
        } else {
            format!("{rel}/{name}")
        };
        if meta.file_type().is_symlink() {
            continue;
        }
        if meta.is_dir() {
            collect_files(&e.path(), &child_rel, depth + 1, out);
        } else {
            out.push((child_rel, e.path()));
        }
    }
}

/// `Some("X/")` when every entry lives under one folder `X` that holds the
/// manifest (a project zipped together with its folder).
fn common_project_prefix(entries: &[crate::zip::ZipFileEntry]) -> Option<String> {
    let first = entries.first()?.path.replace('\\', "/");
    let top = first.split('/').next()?.to_string();
    if top.is_empty() || top == "." || top == ".." {
        return None;
    }
    let prefix = format!("{top}/");
    let all_under = entries
        .iter()
        .all(|e| e.path.replace('\\', "/").starts_with(&prefix) || e.path == top);
    let has_manifest = entries
        .iter()
        .any(|e| e.path.replace('\\', "/") == format!("{prefix}{MANIFEST}"));
    (all_under && has_manifest).then_some(prefix)
}

/// Standard base64 (padding and whitespace tolerated).
fn decode_base64(input: &str) -> Result<Vec<u8>, String> {
    const T: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut lut = [255u8; 256];
    for (i, &c) in T.iter().enumerate() {
        lut[c as usize] = i as u8;
    }
    let mut out = Vec::with_capacity(input.len() / 4 * 3);
    let (mut acc, mut bits) = (0u32, 0u32);
    for b in input.bytes() {
        if b == b'=' || b.is_ascii_whitespace() {
            continue;
        }
        let v = lut[b as usize];
        if v == 255 {
            return Err("the data is not base64".to_string());
        }
        acc = (acc << 6) | u32::from(v);
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push((acc >> bits) as u8);
        }
    }
    Ok(out)
}

// ===========================================================================
// File formats
// ===========================================================================

/// `document.json`: the document tree without the session's uids.
fn document_file_json(root: &BuilderNode) -> Value {
    fn strip(v: &mut Value) {
        if let Value::Object(m) = v {
            m.remove("uid");
            if let Some(Value::Array(cs)) = m.get_mut("children") {
                for c in cs {
                    strip(c);
                }
            }
        }
    }
    let mut tree = root.to_json();
    strip(&mut tree);
    json!({ "format": DOCUMENT_FORMAT, "version": 1, "root": tree })
}

/// The root node of a `document.json` (also accepted: the bare tree).
fn document_root(v: &Value) -> Result<BuilderNode, String> {
    if let Some(f) = v.get("format").and_then(Value::as_str) {
        if f != DOCUMENT_FORMAT {
            return Err(format!("the format is {f:?}, not {DOCUMENT_FORMAT:?}"));
        }
    }
    let root = v.get("root").unwrap_or(v);
    BuilderNode::from_json(root)
}

/// `components/<library>/<name>.json` → `(library, name)`.
fn component_path_names(norm: &str) -> Option<(String, String)> {
    let parts: Vec<&str> = norm.split('/').collect();
    match parts.as_slice() {
        [c, lib, file] if *c == COMPONENTS => {
            let name = file
                .strip_suffix(".json")
                .or_else(|| file.strip_suffix(".JSON"))?;
            Some(((*lib).to_string(), name.to_string()))
        }
        _ => None,
    }
}

/// One component as its project file.
fn component_file_json(library: &str, def: &ComponentDef) -> Value {
    let fields: Vec<Value> = def
        .data_model
        .fields
        .as_ref()
        .iter()
        .map(|f| {
            json!({
                "name": f.name.as_str(),
                "type": super::full::field_type_to_string(&f.field_type),
                "default": super::full::default_value_to_opt_string(&f.default_value),
                "description": f.description.as_str(),
            })
        })
        .collect();
    let mut v = json!({
        "format": COMPONENT_FORMAT,
        "version": 1,
        "library": library,
        "name": def.id.name.as_str(),
        "display_name": def.display_name.as_str(),
        "description": def.description.as_str(),
        "fields": fields,
        "css": def.css.as_str(),
    });
    if let Some(t) = builder::template_of(def) {
        v["template"] = json!(t);
    }
    v
}

/// Register (or replace) the component a project file describes. `from_path`
/// is the `(library, name)` its path spells, used when the file leaves them
/// out. Answers `library:name`.
fn apply_component(
    map: &mut ComponentMap,
    v: &Value,
    from_path: Option<(String, String)>,
) -> Result<String, String> {
    let obj = v.as_object().ok_or("a component file is a JSON object")?;
    if let Some(f) = obj.get("format").and_then(Value::as_str) {
        if f != COMPONENT_FORMAT {
            return Err(format!("the format is {f:?}, not {COMPONENT_FORMAT:?}"));
        }
    }
    let (path_lib, path_name) = from_path.unwrap_or_default();
    let library = obj
        .get("library")
        .and_then(Value::as_str)
        .map(str::to_string)
        .unwrap_or(path_lib);
    let name = obj
        .get("name")
        .and_then(Value::as_str)
        .map(str::to_string)
        .unwrap_or(path_name);
    if library.is_empty() || library == "builtin" || !is_safe_segment(&library) {
        return Err(format!(
            "{library:?} is not a user library (name one in \"library\")"
        ));
    }
    if name.is_empty() || !is_safe_segment(&name) {
        return Err(format!("{name:?} is not a component name"));
    }
    let fields: Vec<super::full::ExportedDataField> = match obj.get("fields") {
        None | Some(Value::Null) => Vec::new(),
        Some(f) => serde_json::from_value(f.clone()).map_err(|e| format!("fields: {e}"))?,
    };
    let fields = super::full::validate_exported_fields(&fields)?;
    let text = |k: &str| obj.get(k).and_then(Value::as_str).unwrap_or("").to_string();
    let display = Some(text("display_name"))
        .filter(|d| !d.trim().is_empty())
        .unwrap_or_else(|| name.clone());
    let description = text("description");
    let css = text("css");

    let def = match obj.get("template").and_then(Value::as_str) {
        Some(template) => {
            let def = builder::template_component_def(
                &library,
                &name,
                &display,
                &description,
                &css,
                template,
                fields,
            );
            if !template.trim().is_empty() && builder::template_render_tree_json(&def).is_none() {
                return Err("the template does not parse as XML".to_string());
            }
            def
        }
        None => ComponentDef {
            id: ComponentId::new(&library, &name),
            display_name: AzString::from(display.as_str()),
            description: AzString::from(description.as_str()),
            css: AzString::from(css.as_str()),
            source: ComponentSource::UserDefined,
            data_model: ComponentDataModel {
                name: AzString::from(format!("{}Data", type_name(&display)).as_str()),
                description: AzString::from(description.as_str()),
                fields: ComponentDataFieldVec::from_vec(fields),
            },
            render_fn: azul_core::xml::user_defined_render_fn,
            codegen: azul_core::xml::ComponentCodegen::RenderFunction,
            render_fn_source: None.into(),
        },
    };
    upsert_component(map, &library, def)?;
    Ok(format!("{library}:{name}"))
}

/// `My Card` → `MyCard`.
fn type_name(display: &str) -> String {
    let s: String = display
        .split(|c: char| !c.is_ascii_alphanumeric())
        .filter(|w| !w.is_empty())
        .map(|w| {
            let mut cs = w.chars();
            cs.next()
                .map(|f| f.to_ascii_uppercase().to_string() + cs.as_str())
                .unwrap_or_default()
        })
        .collect();
    if s.starts_with(|c: char| c.is_ascii_digit()) || s.is_empty() {
        format!("Component{s}")
    } else {
        s
    }
}

/// Add `def` to `library` (created modifiable + exportable if missing), or
/// replace the component of the same name.
fn upsert_component(
    map: &mut ComponentMap,
    library: &str,
    def: ComponentDef,
) -> Result<(), String> {
    let empty = ComponentLibraryVec::from_const_slice(&[]);
    let mut libs = core::mem::replace(&mut map.libraries, empty).into_library_owned_vec();
    let name = def.id.name.as_str().to_string();
    let result = match libs.iter_mut().find(|l| l.name.as_str() == library) {
        Some(lib) if !lib.modifiable => Err(format!("library '{library}' is not modifiable")),
        Some(lib) => {
            let mut comps =
                core::mem::replace(&mut lib.components, Vec::new().into()).into_library_owned_vec();
            match comps.iter_mut().find(|c| c.id.name.as_str() == name) {
                Some(slot) => *slot = def,
                None => comps.push(def),
            }
            lib.components = ComponentDefVec::from_vec(comps);
            Ok(())
        }
        None => {
            libs.push(ComponentLibrary {
                name: AzString::from(library),
                version: AzString::from_const_str("0.1.0"),
                description: AzString::from_const_str("Loaded from an AzBuilder project"),
                components: ComponentDefVec::from_vec(vec![def]),
                exportable: true,
                modifiable: true,
                data_models: azul_core::xml::ComponentDataModelVec::from_const_slice(&[]),
                enum_models: azul_core::xml::ComponentEnumModelVec::from_const_slice(&[]),
            });
            Ok(())
        }
    };
    map.libraries = ComponentLibraryVec::from_vec(libs);
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_relative_path_is_split_into_plain_segments() {
        assert_eq!(
            rel_components("styles/app.css").unwrap(),
            ["styles", "app.css"]
        );
        assert_eq!(rel_components("./a//b/").unwrap(), ["a", "b"]);
        assert_eq!(rel_components("a\\b").unwrap(), ["a", "b"]);
        assert!(rel_components("").unwrap().is_empty());
        assert!(rel_components(".").unwrap().is_empty());
    }

    #[test]
    fn a_path_that_could_leave_the_root_is_refused_by_its_spelling() {
        for (p, needle) in [
            ("..", ".."),
            ("a/../../b", ".."),
            ("a\\..\\..\\b", ".."),
            ("/etc/passwd", "absolute"),
            ("\\\\server\\share", "absolute"),
            ("C:\\Windows", "absolute"),
            ("c:/x", "absolute"),
            ("a\0b", "NUL"),
        ] {
            let err = rel_components(p).expect_err(p);
            assert!(err.contains(needle), "{p:?}: expected '{needle}' in {err}");
        }
    }

    #[test]
    fn a_segment_is_safe_only_as_one_plain_name() {
        assert!(is_safe_segment("user") && is_safe_segment("my-card_2"));
        for s in ["", ".", "..", "a/b", "a\\b", "c:", "a\0"] {
            assert!(!is_safe_segment(s), "{s:?}");
        }
    }

    #[test]
    fn a_component_path_names_its_library_and_component() {
        assert_eq!(
            component_path_names("components/user/card.json"),
            Some(("user".to_string(), "card".to_string()))
        );
        assert_eq!(component_path_names("components/card.json"), None);
        assert_eq!(component_path_names("styles/user/card.json"), None);
    }

    #[test]
    fn base64_round_trips() {
        let data: Vec<u8> = (0..=255u8).collect();
        let enc = crate::callbacks::base64_encode(&data);
        assert_eq!(decode_base64(&enc).unwrap(), data);
        assert!(decode_base64("not*base64").is_err());
    }

    #[test]
    fn a_zipped_project_folder_is_unwrapped_only_when_it_holds_the_manifest() {
        let e = |p: &str| crate::zip::ZipFileEntry::file(p, Vec::new());
        assert_eq!(
            common_project_prefix(&[e("proj/azul-project.json"), e("proj/styles/a.css")]),
            Some("proj/".to_string())
        );
        assert_eq!(
            common_project_prefix(&[e("proj/styles/a.css"), e("proj/b.css")]),
            None,
            "no manifest: a folder of the project, keep it"
        );
        assert_eq!(
            common_project_prefix(&[e("proj/azul-project.json"), e("other/x")]),
            None
        );
    }

    #[test]
    fn a_component_file_round_trips_through_the_component_map() {
        let mut map = ComponentMap::default();
        let file = json!({
            "format": "azul-component", "version": 1,
            "library": "lib", "name": "badge", "display_name": "Badge",
            "fields": [ { "name": "text", "type": "String", "default": "New" } ],
            "css": ".badge { color: red; }",
            "template": "<span class=\"badge\">{text}</span>",
        });
        assert_eq!(apply_component(&mut map, &file, None).unwrap(), "lib:badge");
        let def = map.get("lib", "badge").expect("registered");
        let back = component_file_json("lib", def);
        assert_eq!(back["template"], "<span class=\"badge\">{text}</span>");
        assert_eq!(back["fields"][0]["name"], "text");
        assert_eq!(back["fields"][0]["default"], "New");
        assert_eq!(back["css"], ".badge { color: red; }");
        // Applying it again replaces, not duplicates.
        apply_component(&mut map, &file, None).unwrap();
        let lib = map
            .libraries
            .as_ref()
            .iter()
            .find(|l| l.name.as_str() == "lib")
            .expect("lib");
        assert_eq!(lib.components.as_ref().len(), 1);
    }

    #[test]
    fn a_component_file_names_its_library_or_its_path_does() {
        let mut map = ComponentMap::default();
        let bare = json!({ "fields": [], "template": "<div/>" });
        assert_eq!(
            apply_component(&mut map, &bare, Some(("user".into(), "box".into()))).unwrap(),
            "user:box"
        );
        let err = apply_component(&mut map, &bare, None).expect_err("no library");
        assert!(err.contains("library"), "{err}");
        let builtin = json!({ "library": "builtin", "name": "div" });
        assert!(apply_component(&mut map, &builtin, None).is_err());
    }
}
