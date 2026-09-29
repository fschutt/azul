//! The user's icon rules on disk.
//!
//! `~/.azul/icons/remap.json` is the global table, `~/.azul/icons/<theme>/remap.json`
//! one table per theme directory (`xyz/pink/` is the theme `xyz:pink`: path
//! segments on disk, `:` in names, because `:` is illegal in NTFS file
//! names). Each table maps an icon name to rules tried in order, the first
//! that applies winning:
//!
//! ```json
//! {
//!   "material/home": [
//!     { "file": "home-dark.svg", "apply-if": "theme=monokai,mode=dark", "recolor": "currentColor" },
//!     { "file": "home.svg",      "apply-if": "theme=monokai" }
//!   ],
//!   "kde:three-lines": [
//!     { "file": "menu.svg", "apply-if": "os=linux:kde", "recolor": { "light": "system:text", "dark": "#e6e6e6" } }
//!   ]
//! }
//! ```
//!
//! A rule names a `file` next to its table (an SVG, or a raster image) or an
//! `icon` spec to redirect to. `apply-if` speaks the dynamic-selector
//! vocabulary ([`azul_core::icon::parse_icon_apply_if`]) and is evaluated at
//! LOOKUP against the window's live context, so a light -> dark switch swaps
//! the artwork. A theme's rules apply only while that theme is in the theme
//! chain and rank by its place there; the global table ranks last. Remap
//! first, then the app's own fallback list.
//!
//! `recolor` (design 9.1 pitfall 10 - an explicit colour here beats the CSS
//! `color`, `currentColor` takes it): `"currentColor"`, `"mask"`, `"none"`,
//! a colour (`"#e6e6e6"`, `"system:text"`), `{ "light": c, "dark": c }`, or a
//! palette keyed by colours (`{ "#000000": "system:text" }`). `designed_for`
//! (`light` / `dark` / `any`) and `monochrome` complete the metadata; a rule
//! that recolours a file states it is recolourable ink (`monochrome`).
//!
//! The files are untrusted input (a theme exchanger unzips them): a `file`
//! must be a plain relative path that stays inside its directory, symlinks
//! included, and tables and files are size-capped.
//! (scripts/ideas/RICING_LAYERS_AND_STOPTHEMINGMYAPP_2026_09_29.md section 8)

use std::path::{Component, Path, PathBuf};

use azul_core::icon::IconProviderHandle;

/// The file name of a remap table.
pub const REMAP_TABLE: &str = "remap.json";

/// The pack the global table's files are registered in; a theme's go to
/// `user-icons/<theme path>`. Rule targets name their pack, so the packs'
/// place in the lookup order does not matter to the rules.
pub const USER_ICON_PACK: &str = "user-icons";

/// Tables larger than this are refused.
pub const MAX_REMAP_TABLE_BYTES: u64 = 1 << 20;

/// Rule files larger than this are refused (an SVG is further capped by
/// [`crate::icon::MAX_SVG_ICON_BYTES`]).
pub const MAX_RULE_FILE_BYTES: u64 = 4 << 20;

/// How deep [`walk_theme_dirs`] descends (`xyz/pink/deeper/...`).
pub const MAX_THEME_DIR_DEPTH: usize = 4;

/// How many theme directories [`walk_theme_dirs`] reports at most.
pub const MAX_THEME_DIRS: usize = 256;

/// The mode's names: never a theme, and never a segment of one (design 9.1
/// pitfall 5 - a theme called `dark` would collide with the mode).
const RESERVED_THEME_SEGMENTS: [&str; 2] = ["light", "dark"];

/// One theme directory of a user tree (`~/.azul/icons/`, `~/.azul/css/`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ThemeDir {
    /// The theme's name: the path segments below the root joined by `:`.
    pub theme: String,
    pub dir: PathBuf,
}

/// Every theme directory under `root`: parents before their spin-offs,
/// siblings by name. Dot-directories, names containing `:` and symlinked
/// directories are skipped; the walk stops at [`MAX_THEME_DIR_DEPTH`] and
/// [`MAX_THEME_DIRS`]. A missing root is an empty tree.
///
/// The one walk of a `.azul` theme tree: the CSS rice loader over
/// `~/.azul/css/<theme>/` walks the same shape.
#[must_use]
pub fn walk_theme_dirs(root: &Path) -> Vec<ThemeDir> {
    let mut found = Vec::new();
    walk_into(root, &mut Vec::new(), &mut found);
    found
}

fn walk_into(dir: &Path, segments: &mut Vec<String>, found: &mut Vec<ThemeDir>) {
    if segments.len() >= MAX_THEME_DIR_DEPTH {
        return;
    }
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    let mut children: Vec<(String, PathBuf)> = entries
        .filter_map(Result::ok)
        .filter_map(|entry| {
            // `DirEntry::file_type` does not follow symlinks: a linked
            // directory reads as a link and is not walked.
            let is_dir = entry.file_type().ok()?.is_dir();
            let name = entry.file_name().into_string().ok()?;
            (is_dir && !name.starts_with('.') && !name.contains(':'))
                .then(|| (name, entry.path()))
        })
        .collect();
    children.sort();
    for (name, path) in children {
        if found.len() >= MAX_THEME_DIRS {
            return;
        }
        segments.push(name);
        found.push(ThemeDir {
            theme: segments.join(":"),
            dir: path.clone(),
        });
        walk_into(&path, segments, found);
        segments.pop();
    }
}

/// What loading the rules did: which tables were read, how many rules were
/// added, and everything refused - for the About page's rice status and for
/// a user debugging their theme.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct IconRulesReport {
    pub tables: Vec<PathBuf>,
    pub rules: usize,
    pub warnings: Vec<String>,
}

/// Load `<root>/remap.json` and every `<root>/<theme>/remap.json` into
/// `provider`, registering each rule's file and adding the rule.
///
/// Every theme directory is loaded, not only the ones in today's chain: a
/// rule of a theme that is not in the chain contributes nothing at lookup,
/// and a runtime theme switch finds its rules already there. `app_name` is
/// what `app=` terms compare against (the executable's name,
/// [`current_app_name`]); empty leaves the provider's as it is. Nothing
/// here is fatal: what cannot be loaded is reported and skipped.
pub fn load_user_icon_rules(
    provider: &mut IconProviderHandle,
    root: &Path,
    app_name: &str,
) -> IconRulesReport {
    let mut report = IconRulesReport::default();
    if !app_name.is_empty() {
        provider.set_app_name(app_name);
    }
    if !root.is_dir() {
        return report;
    }
    let mut next_name = 0_usize;
    load_table(provider, root, None, &mut next_name, &mut report);
    for ThemeDir { theme, dir } in walk_theme_dirs(root) {
        let reserved = theme.split(':').any(|segment| {
            RESERVED_THEME_SEGMENTS
                .iter()
                .any(|r| segment.eq_ignore_ascii_case(r))
        });
        if reserved {
            if dir.join(REMAP_TABLE).is_file() {
                report.warnings.push(format!(
                    "{}: `{theme}` is not a theme - light and dark are the mode, use \
                     `apply-if: mode=...` instead",
                    dir.display()
                ));
            }
            continue;
        }
        load_table(provider, &dir, Some(&theme), &mut next_name, &mut report);
    }
    report
}

/// `~/.azul`, where user themes live (`css/<theme>/`, `icons/<theme>/`):
/// `$HOME`, else `%USERPROFILE%`. `None` when neither is set.
#[must_use]
pub fn user_azul_root() -> Option<PathBuf> {
    let home = std::env::var_os("HOME").or_else(|| std::env::var_os("USERPROFILE"))?;
    if home.is_empty() {
        return None;
    }
    Some(PathBuf::from(home).join(".azul"))
}

/// `~/.azul/icons`, the root [`load_user_icon_rules`] reads at startup.
#[must_use]
pub fn user_icons_root() -> Option<PathBuf> {
    user_azul_root().map(|root| root.join("icons"))
}

/// The running executable's file stem: the name `app=` rule terms compare
/// against (the same name the per-app user stylesheet is looked up by).
#[must_use]
pub fn current_app_name() -> String {
    std::env::current_exe()
        .ok()
        .and_then(|exe| exe.file_stem().map(|s| s.to_string_lossy().into_owned()))
        .unwrap_or_default()
}

/// The pack a table's files are registered in.
#[cfg_attr(not(feature = "json"), allow(dead_code))] // only tables register files
fn pack_for(theme: Option<&str>) -> String {
    theme.map_or_else(
        || USER_ICON_PACK.to_string(),
        |t| format!("{USER_ICON_PACK}/{}", t.replace(':', "/")),
    )
}

/// `file` inside `dir`, or why not: it must be a plain relative path (no
/// root, no `..`, no `.`), and where it really leads - symlinks resolved -
/// must still be inside `dir`.
#[cfg_attr(not(feature = "json"), allow(dead_code))] // only tables name files
fn resolve_rule_file(dir: &Path, file: &str) -> Result<PathBuf, String> {
    let relative = Path::new(file);
    let plain = !file.trim().is_empty()
        && relative
            .components()
            .all(|c| matches!(c, Component::Normal(_)));
    if !plain {
        return Err(format!(
            "`file: {file}` must be a plain relative path inside its directory"
        ));
    }
    let inside = dir
        .canonicalize()
        .map_err(|e| format!("{}: {e}", dir.display()))?;
    let target = dir
        .join(relative)
        .canonicalize()
        .map_err(|e| format!("`file: {file}`: {e}"))?;
    if !target.starts_with(&inside) {
        return Err(format!("`file: {file}` leads out of its directory"));
    }
    Ok(target)
}

#[cfg(feature = "json")]
fn load_table(
    provider: &mut IconProviderHandle,
    dir: &Path,
    theme: Option<&str>,
    next_name: &mut usize,
    report: &mut IconRulesReport,
) {
    let path = dir.join(REMAP_TABLE);
    let Ok(metadata) = std::fs::metadata(&path) else {
        return; // no table: the normal case
    };
    if !metadata.is_file() {
        return;
    }
    if metadata.len() > MAX_REMAP_TABLE_BYTES {
        report.warnings.push(format!(
            "{}: larger than {MAX_REMAP_TABLE_BYTES} bytes, not read",
            path.display()
        ));
        return;
    }
    let table: serde_json::Value = match std::fs::read_to_string(&path)
        .map_err(|e| e.to_string())
        .and_then(|text| serde_json::from_str(&text).map_err(|e| e.to_string()))
    {
        Ok(table) => table,
        Err(why) => {
            report
                .warnings
                .push(format!("{}: not read: {why}", path.display()));
            return;
        }
    };
    let Some(names) = table.as_object() else {
        report.warnings.push(format!(
            "{}: must be an object of icon names",
            path.display()
        ));
        return;
    };
    report.tables.push(path.clone());
    let pack = pack_for(theme);
    for (name, rules) in names {
        let rules: Vec<&serde_json::Value> = match rules {
            serde_json::Value::Array(list) => list.iter().collect(),
            serde_json::Value::Object(_) => vec![rules],
            _ => {
                report.warnings.push(format!(
                    "{}: `{name}`: rules are a list of objects",
                    path.display()
                ));
                continue;
            }
        };
        for rule in rules {
            match add_rule(provider, dir, theme, &pack, name, rule, next_name) {
                Ok(()) => report.rules += 1,
                Err(why) => report
                    .warnings
                    .push(format!("{}: `{name}`: {why}", path.display())),
            }
        }
    }
}

/// Without JSON support there are no rules; say so when a table exists.
#[cfg(not(feature = "json"))]
fn load_table(
    _provider: &mut IconProviderHandle,
    dir: &Path,
    _theme: Option<&str>,
    _next_name: &mut usize,
    report: &mut IconRulesReport,
) {
    let path = dir.join(REMAP_TABLE);
    if path.is_file() {
        report.warnings.push(format!(
            "{}: not read - this build has no `json` feature",
            path.display()
        ));
    }
}

/// One rule: register its file (or take its `icon` spec) and add it.
#[cfg(feature = "json")]
fn add_rule(
    provider: &mut IconProviderHandle,
    dir: &Path,
    theme: Option<&str>,
    pack: &str,
    name: &str,
    rule: &serde_json::Value,
    next_name: &mut usize,
) -> Result<(), String> {
    let rule = rule.as_object().ok_or("a rule must be an object")?;
    let text = |key: &str| rule.get(key).and_then(serde_json::Value::as_str);
    let apply_if = text("apply-if").or_else(|| text("apply_if")).unwrap_or("");
    let target = if let Some(file) = text("file") {
        let path = resolve_rule_file(dir, file)?;
        let meta = MetaOverride::from_rule(rule)?;
        let internal = format!("rule-{next_name}");
        *next_name += 1;
        register_rule_file(provider, pack, &internal, &path, &meta)?;
        format!("{pack}:{internal}")
    } else if let Some(spec) = text("icon") {
        // A redirect to a registered icon draws it with ITS metadata; a
        // `recolor` here has no file of its own to apply to.
        spec.to_string()
    } else {
        return Err("a rule needs a `file` or an `icon`".to_string());
    };
    match theme {
        None => provider.add_icon_remap_rule(name, apply_if, &target),
        Some(theme) => provider.add_theme_icon_remap_rule(theme, name, apply_if, &target),
    }
    Ok(())
}

/// The metadata a rule states for its file, over what the file implies.
#[cfg(feature = "json")]
struct MetaOverride {
    recolor: Option<azul_core::icon::IconRecolor>,
    designed_for: Option<azul_core::icon::IconDesignedFor>,
    monochrome: Option<bool>,
}

#[cfg(feature = "json")]
impl MetaOverride {
    fn from_rule(rule: &serde_json::Map<String, serde_json::Value>) -> Result<Self, String> {
        let recolor = rule.get("recolor").map(parse_recolor).transpose()?;
        let designed_for = rule
            .get("designed_for")
            .or_else(|| rule.get("designed-for"))
            .map(|v| {
                v.as_str()
                    .and_then(azul_core::icon::IconDesignedFor::from_name)
                    .ok_or_else(|| "`designed_for` is light, dark or any".to_string())
            })
            .transpose()?;
        let monochrome = rule
            .get("monochrome")
            .map(|v| {
                v.as_bool()
                    .ok_or_else(|| "`monochrome` is true or false".to_string())
            })
            .transpose()?;
        Ok(Self {
            recolor,
            designed_for,
            monochrome,
        })
    }

    fn apply(&self, mut meta: azul_core::icon::IconMeta) -> azul_core::icon::IconMeta {
        use azul_core::icon::IconRecolor;

        if let Some(recolor) = &self.recolor {
            // A rule that recolours its file with one colour states the file
            // is recolourable ink, unless it says otherwise.
            let ink = matches!(
                recolor,
                IconRecolor::CurrentColor | IconRecolor::Mask | IconRecolor::Fixed(_)
            );
            meta.recolor = recolor.clone();
            if ink {
                meta.monochrome = true;
            }
        }
        if let Some(designed_for) = self.designed_for {
            meta.designed_for = designed_for;
        }
        if let Some(monochrome) = self.monochrome {
            meta.monochrome = monochrome;
        }
        meta
    }
}

/// A rule's `recolor` value (see the module docs for the forms).
#[cfg(feature = "json")]
fn parse_recolor(value: &serde_json::Value) -> Result<azul_core::icon::IconRecolor, String> {
    use azul_core::icon::{IconColorMapping, IconColorMappingVec, IconModeColors, IconRecolor};
    use azul_css::props::basic::color::{parse_color_or_system_token, ColorU};

    let colour = |s: &str| -> Result<ColorU, String> {
        parse_color_or_system_token(s.trim()).map_err(|_| format!("`{s}` is not a colour"))
    };
    match value {
        serde_json::Value::String(s) => {
            let s = s.trim();
            if s.eq_ignore_ascii_case("currentcolor") {
                Ok(IconRecolor::CurrentColor)
            } else if s.eq_ignore_ascii_case("mask") {
                Ok(IconRecolor::Mask)
            } else if s.eq_ignore_ascii_case("none") {
                Ok(IconRecolor::None)
            } else {
                Ok(IconRecolor::Fixed(IconModeColors::same(colour(s)?)))
            }
        }
        serde_json::Value::Object(map) => {
            let is_mode = |k: &str| k.eq_ignore_ascii_case("light") || k.eq_ignore_ascii_case("dark");
            if !map.is_empty() && map.keys().all(|k| is_mode(k)) {
                let for_mode = |mode: &str| -> Result<Option<ColorU>, String> {
                    map.iter()
                        .find(|(k, _)| k.eq_ignore_ascii_case(mode))
                        .map(|(_, v)| {
                            v.as_str()
                                .ok_or_else(|| "a mode's colour is a string".to_string())
                                .and_then(colour)
                        })
                        .transpose()
                };
                match (for_mode("light")?, for_mode("dark")?) {
                    (Some(light), Some(dark)) => Ok(IconRecolor::Fixed(IconModeColors { light, dark })),
                    (Some(one), None) | (None, Some(one)) => {
                        Ok(IconRecolor::Fixed(IconModeColors::same(one)))
                    }
                    (None, None) => Err("`recolor` names no colour".to_string()),
                }
            } else {
                let mut palette = Vec::with_capacity(map.len());
                for (from, to) in map {
                    let to = to
                        .as_str()
                        .ok_or_else(|| "a palette entry's colour is a string".to_string())?;
                    palette.push(IconColorMapping {
                        from: colour(from.as_str())?,
                        to: colour(to)?,
                    });
                }
                Ok(IconRecolor::Palette(IconColorMappingVec::from_vec(palette)))
            }
        }
        _ => Err("`recolor` is a string or an object".to_string()),
    }
}

/// Register a rule's file under `pack:name`: an SVG as an SVG icon, any
/// other file as a raster image.
#[cfg(feature = "json")]
fn register_rule_file(
    provider: &mut IconProviderHandle,
    pack: &str,
    name: &str,
    path: &Path,
    meta: &MetaOverride,
) -> Result<(), String> {
    let len = std::fs::metadata(path)
        .map_err(|e| format!("{}: {e}", path.display()))?
        .len();
    if len > MAX_RULE_FILE_BYTES {
        return Err(format!(
            "{}: larger than {MAX_RULE_FILE_BYTES} bytes",
            path.display()
        ));
    }
    let bytes = std::fs::read(path).map_err(|e| format!("{}: {e}", path.display()))?;
    let is_svg = path
        .extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| e.eq_ignore_ascii_case("svg"));
    if is_svg {
        let meta = meta.apply(crate::icon::default_svg_icon_meta(&bytes));
        if crate::icon::register_svg_icon(provider, pack, name, &bytes, meta) {
            Ok(())
        } else {
            Err(format!("{}: not an SVG document", path.display()))
        }
    } else {
        register_raster(provider, pack, name, &bytes, meta, path)
    }
}

#[cfg(all(feature = "json", feature = "image_decoding"))]
fn register_raster(
    provider: &mut IconProviderHandle,
    pack: &str,
    name: &str,
    bytes: &[u8],
    meta: &MetaOverride,
    path: &Path,
) -> Result<(), String> {
    use crate::image::decode::{decode_raw_image_from_any_bytes, ResultRawImageDecodeImageError};

    let ResultRawImageDecodeImageError::Ok(raw) = decode_raw_image_from_any_bytes(bytes) else {
        return Err(format!("{}: not an image", path.display()));
    };
    let image = azul_core::resources::ImageRef::new_rawimage(raw)
        .ok_or_else(|| format!("{}: not a usable image", path.display()))?;
    crate::icon::register_image_icon_with_meta(
        provider,
        pack,
        name,
        image,
        meta.apply(azul_core::icon::IconMeta::for_image()),
    );
    Ok(())
}

#[cfg(all(feature = "json", not(feature = "image_decoding")))]
fn register_raster(
    _provider: &mut IconProviderHandle,
    _pack: &str,
    _name: &str,
    _bytes: &[u8],
    _meta: &MetaOverride,
    path: &Path,
) -> Result<(), String> {
    Err(format!(
        "{}: this build decodes no raster images (no `image_decoding` feature)",
        path.display()
    ))
}
