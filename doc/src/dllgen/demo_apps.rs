//! The demo apps of the release page, read from `scripts/demo_apps.toml`.
//!
//! That file is the ONE list of the Az apps. The plan job of rust.yml builds from it
//! (`scripts/demo_apps.py`), and the Demos section of the release page is generated from it here: the
//! manifest is compiled in, so the page can only list what the manifest names. The tests in
//! `demo_apps_test.rs` hold it against the Az packages of the workspace, against the names
//! `scripts/demo_apps.py` gives the built files and against the routing of `deploy::asset_url`.
//!
//! [`staged_name`], [`release_name`] and [`columns_of`] are the functions of the same names in
//! `scripts/demo_apps.py`, to the byte: a build stages `<Name>-linux`, the release carries
//! `<Name>-linux.tar.gz`, and the link of the page is the third place that has to spell it the same.
//!
//! The page makes no claim about what CI published. Every link of the manifest is written, and the
//! deploy (`scripts/prune_dead_release_links.py`) turns the ones whose file does not exist into a
//! visible "not available". What the page does say by itself is what the manifest says: a dash for a
//! platform the app is not built for, and "not available" with the reason for a build the manifest
//! lists in `known_failing`.

use std::collections::{BTreeMap, BTreeSet};

use serde::Deserialize;

/// `scripts/demo_apps.toml`, compiled in.
pub const MANIFEST_TOML: &str = include_str!("../../../scripts/demo_apps.toml");

/// The `schema` of the manifest this reader knows.
pub const SCHEMA: u32 = 1;

/// The longest `description` of an app (the limit `scripts/demo_apps.py check` holds it to).
pub const MAX_DESCRIPTION: usize = 110;

// ---------------------------------------------------------------------------------------------
// The manifest
// ---------------------------------------------------------------------------------------------

/// What an app is built for: the `platforms` of an app.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Deserialize)]
pub enum Platform {
    #[serde(rename = "linux")]
    Linux,
    /// Apple Silicon, macOS 11 and later.
    #[serde(rename = "macos-arm64")]
    MacosArm64,
    /// x86_64, macOS 10.12 and later (built on an Apple Silicon runner).
    #[serde(rename = "macos-intel")]
    MacosIntel,
    #[serde(rename = "windows")]
    Windows,
    /// An arm64 APK and an x86_64 emulator APK.
    #[serde(rename = "android")]
    Android,
    /// An unsigned device .app and a Simulator .app, both zipped.
    #[serde(rename = "ios")]
    Ios,
}

impl Platform {
    /// The spelling of the manifest (`platforms = [...]`, and the platform keys of `known_failing`).
    pub fn id(self) -> &'static str {
        match self {
            Platform::Linux => "linux",
            Platform::MacosArm64 => "macos-arm64",
            Platform::MacosIntel => "macos-intel",
            Platform::Windows => "windows",
            Platform::Android => "android",
            Platform::Ios => "ios",
        }
    }

    pub fn from_id(id: &str) -> Option<Platform> {
        PLATFORMS.iter().copied().find(|p| p.id() == id)
    }
}

/// Every platform, in the order of `PLATFORMS` in `scripts/demo_apps.py`.
pub static PLATFORMS: [Platform; 6] = [
    Platform::Linux,
    Platform::MacosArm64,
    Platform::MacosIntel,
    Platform::Windows,
    Platform::Android,
    Platform::Ios,
];

/// One built file of an app, which is one column of the table. A desktop platform is one file, a
/// phone platform two (the order of `ARTIFACT_COLUMNS` in `scripts/demo_apps.py`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Column {
    Linux,
    MacosArm64,
    MacosIntel,
    Windows,
    AndroidArm64,
    AndroidX86_64,
    IosDevice,
    IosSim,
}

/// Every column, in the order of `ARTIFACT_COLUMNS` in `scripts/demo_apps.py`.
pub static COLUMNS: [Column; 8] = [
    Column::Linux,
    Column::MacosArm64,
    Column::MacosIntel,
    Column::Windows,
    Column::AndroidArm64,
    Column::AndroidX86_64,
    Column::IosDevice,
    Column::IosSim,
];

impl Column {
    /// The spelling of the manifest and of `demo-status.json`: `linux`, `macos-arm64`, `macos-intel`,
    /// `windows`, `android-arm64`, `android-x86_64`, `ios-device`, `ios-sim`.
    pub fn id(self) -> &'static str {
        match self {
            Column::Linux => "linux",
            Column::MacosArm64 => "macos-arm64",
            Column::MacosIntel => "macos-intel",
            Column::Windows => "windows",
            Column::AndroidArm64 => "android-arm64",
            Column::AndroidX86_64 => "android-x86_64",
            Column::IosDevice => "ios-device",
            Column::IosSim => "ios-sim",
        }
    }

    pub fn from_id(id: &str) -> Option<Column> {
        COLUMNS.iter().copied().find(|c| c.id() == id)
    }

    /// The platform that builds this file: `android` builds both Android columns, `ios` both iOS ones.
    pub fn platform(self) -> Platform {
        match self {
            Column::Linux => Platform::Linux,
            Column::MacosArm64 => Platform::MacosArm64,
            Column::MacosIntel => Platform::MacosIntel,
            Column::Windows => Platform::Windows,
            Column::AndroidArm64 | Column::AndroidX86_64 => Platform::Android,
            Column::IosDevice | Column::IosSim => Platform::Ios,
        }
    }

    /// A desktop file is staged under `demos/` and shipped as a `.tar.gz`; a phone file is staged
    /// under `mobile-apps/` and shipped as it is.
    pub fn is_desktop(self) -> bool {
        matches!(
            self.platform(),
            Platform::Linux | Platform::MacosArm64 | Platform::MacosIntel | Platform::Windows
        )
    }

    /// The heading of the column on the page.
    pub fn heading(self) -> &'static str {
        match self {
            Column::Linux => "Linux",
            Column::MacosArm64 => "macOS Apple Silicon",
            Column::MacosIntel => "macOS Intel",
            Column::Windows => "Windows",
            Column::AndroidArm64 => "Android arm64 APK",
            Column::AndroidX86_64 => "Android emulator APK",
            Column::IosDevice => "iOS device",
            Column::IosSim => "iOS Simulator .app",
        }
    }

    /// The small line under the heading: what the file is.
    pub fn note(self) -> &'static str {
        match self {
            Column::Linux => "x86-64",
            Column::MacosArm64 => "macOS 11+",
            Column::MacosIntel => "macOS 10.12+",
            Column::Windows => "x86-64",
            Column::AndroidArm64 => "sideload it",
            Column::AndroidX86_64 => "x86_64",
            Column::IosDevice => "unsigned .app",
            Column::IosSim => "runs unsigned",
        }
    }

    /// The sentence behind the heading (its `title`): what the file is and how to use it.
    pub fn hint(self) -> &'static str {
        match self {
            Column::Linux => "a binary for Linux, x86-64",
            Column::MacosArm64 => "a binary for Apple Silicon Macs, macOS 11 and later",
            Column::MacosIntel => "a binary for Intel Macs, macOS 10.12 and later",
            Column::Windows => "a binary for Windows, x86-64",
            Column::AndroidArm64 => {
                "an APK for arm64 phones: sideload it (adb install, or open the file on the phone)"
            }
            Column::AndroidX86_64 => "an x86_64 APK for the Android emulator",
            Column::IosDevice => "an unsigned .app (zipped): AzReview signs it for your phone",
            Column::IosSim => "a .app (zipped) for the iOS Simulator, which runs it unsigned",
        }
    }

    /// What the link of a cell says: the kind of file it downloads.
    pub fn file_kind(self) -> &'static str {
        match self {
            Column::Linux | Column::MacosArm64 | Column::MacosIntel | Column::Windows => ".tar.gz",
            Column::AndroidArm64 | Column::AndroidX86_64 => ".apk",
            Column::IosDevice | Column::IosSim => ".app.zip",
        }
    }
}

/// One app of `scripts/demo_apps.toml`. The keys the page does not need (`weight`'s balancing, the
/// Android permissions, the iOS usage texts, ...) are not read; a key this reader does not know is
/// ignored, never an error.
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct App {
    /// The cargo package. Package, `[[bin]]` and the stem of every asset are this one string.
    pub package: String,
    /// The crate's directory, relative to the repository root.
    pub dir: String,
    /// What the page calls the app (the package when the manifest names none).
    #[serde(default)]
    pub name: Option<String>,
    /// One line, for the row of the table.
    pub description: String,
    pub platforms: Vec<Platform>,
    /// 1 (light) .. 3 (heavy): what the plan job balances its build groups by.
    #[serde(default = "default_weight")]
    pub weight: u32,
    /// The app has a Dockerfile (a web row on the page).
    #[serde(default)]
    pub docker: bool,
    /// What the app needs to show anything: a note under its name.
    #[serde(default)]
    pub needs: Vec<String>,
    /// A column (`android-arm64`) or a platform (`android`, `ios`) whose build is known to fail, with
    /// the reason: the page links nothing there and says why.
    #[serde(default)]
    pub known_failing: BTreeMap<String, String>,
    /// Which libazul of `[lib.*]` a phone build links.
    #[serde(default)]
    pub mobile_lib: Option<String>,
}

fn default_weight() -> u32 {
    1
}

impl App {
    /// The name the page shows for the app.
    pub fn display_name(&self) -> &str {
        self.name.as_deref().unwrap_or(&self.package)
    }
}

/// `scripts/demo_apps.toml`: the apps, in the order the page lists them.
#[derive(Debug, Clone, Deserialize)]
pub struct Manifest {
    #[serde(default)]
    pub schema: u32,
    #[serde(default, rename = "app")]
    pub apps: Vec<App>,
}

impl Manifest {
    pub fn parse(text: &str) -> Result<Manifest, String> {
        toml::from_str(text).map_err(|e| e.to_string())
    }

    /// The manifest compiled into this binary. A test parses it, so a failure here is a manifest that
    /// was committed without running the tests.
    pub fn embedded() -> Manifest {
        Manifest::parse(MANIFEST_TOML)
            .unwrap_or_else(|e| panic!("scripts/demo_apps.toml does not parse: {e}"))
    }

    pub fn app(&self, package: &str) -> Option<&App> {
        self.apps.iter().find(|a| a.package == package)
    }

    /// Everything in the manifest that the page could not render honestly: the half of
    /// `scripts/demo_apps.py check` that needs no other file.
    pub fn problems(&self) -> Vec<String> {
        let mut out = Vec::new();
        if self.schema != SCHEMA {
            out.push(format!(
                "schema is {}, this reader knows {SCHEMA}",
                self.schema
            ));
        }
        if self.apps.is_empty() {
            out.push("the manifest names no app".to_string());
        }
        let mut seen = BTreeSet::new();
        for app in &self.apps {
            let p = &app.package;
            if !seen.insert(p.as_str()) {
                out.push(format!("{p} is listed twice"));
            }
            if !is_az_name(p) {
                out.push(format!(
                    "{p}: breaks the AzXxx convention (package, binary and asset stem are one string)"
                ));
            }
            let d = &app.description;
            if d.trim().is_empty() || d.chars().count() > MAX_DESCRIPTION || d.contains('\n') {
                out.push(format!(
                    "{p}: the description is empty, longer than {MAX_DESCRIPTION} characters or has a line break"
                ));
            }
            if app.dir.trim().is_empty() {
                out.push(format!("{p}: names no dir"));
            }
            if app.platforms.is_empty() {
                out.push(format!("{p}: builds for no platform"));
            }
            for (key, why) in &app.known_failing {
                if Column::from_id(key).is_none() && Platform::from_id(key).is_none() {
                    out.push(format!(
                        "{p}: known_failing names {key:?}, which is no column or platform"
                    ));
                }
                if why.trim().is_empty() {
                    out.push(format!("{p}: known_failing.{key} needs the reason"));
                }
            }
        }
        out
    }
}

/// `AzXxx`: "Az", an upper-case letter, then letters and digits.
pub fn is_az_name(name: &str) -> bool {
    let mut chars = name.chars();
    chars.next() == Some('A')
        && chars.next() == Some('z')
        && chars.next().is_some_and(|c| c.is_ascii_uppercase())
        && chars.all(|c| c.is_ascii_alphanumeric())
}

// ---------------------------------------------------------------------------------------------
// The names of the built files (the same functions as scripts/demo_apps.py's)
// ---------------------------------------------------------------------------------------------

/// The built files of an app, in column order: `columns_of` of `scripts/demo_apps.py`.
pub fn columns_of(app: &App) -> Vec<Column> {
    COLUMNS
        .iter()
        .copied()
        .filter(|c| app.platforms.contains(&c.platform()))
        .collect()
}

/// The file a build produces for `column`: `staged_name` of `scripts/demo_apps.py`.
pub fn staged_name(app: &App, column: Column) -> String {
    let p = &app.package;
    match column {
        Column::Linux => format!("{p}-linux"),
        Column::MacosArm64 => format!("{p}-macos"),
        Column::MacosIntel => format!("{p}-macos-intel"),
        Column::Windows => format!("{p}-windows.exe"),
        Column::AndroidArm64 => format!("{p}-android-arm64.apk"),
        Column::AndroidX86_64 => format!("{p}-android-x86_64.apk"),
        Column::IosDevice => format!("{p}-ios-device.app.zip"),
        Column::IosSim => format!("{p}-ios-sim.app.zip"),
    }
}

/// The asset name on the GitHub release: `release_name` of `scripts/demo_apps.py`. The desktop
/// binaries are tarred, the phone files are not.
pub fn release_name(app: &App, column: Column) -> String {
    let name = staged_name(app, column);
    if column.is_desktop() {
        format!("{name}.tar.gz")
    } else {
        name
    }
}

/// The name the page hands to `deploy::asset_url` for a built file: `demos/<staged name>` or
/// `mobile-apps/<staged name>`. Nothing is stored under such a path; `asset_url` routes both prefixes
/// to the release asset of the file, flattened, and a desktop binary to its `.tar.gz`
/// ([`release_name`]).
pub fn asset_path(app: &App, column: Column) -> String {
    let dir = if column.is_desktop() {
        "demos"
    } else {
        "mobile-apps"
    };
    format!("{dir}/{}", staged_name(app, column))
}

/// The reason the manifest gives for a build that is known to fail: `known_failing` names the column
/// (`android-arm64`) or its platform (`android`), the column first. A blank reason names nothing.
pub fn known_failing(app: &App, column: Column) -> Option<&str> {
    [column.id(), column.platform().id()]
        .into_iter()
        .filter_map(|key| app.known_failing.get(key))
        .map(String::as_str)
        .find(|why| !why.trim().is_empty())
}

/// What one cell of the table holds.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Cell<'a> {
    /// The app is not built for this platform at all: a dash.
    NotBuilt,
    /// A build exists in principle: a link to the file (`path` goes to `deploy::asset_url`).
    Download { path: String },
    /// The manifest lists the build as failing: no link, the reason instead.
    Unavailable { reason: &'a str },
}

pub fn cell<'a>(app: &'a App, column: Column) -> Cell<'a> {
    if !columns_of(app).contains(&column) {
        return Cell::NotBuilt;
    }
    match known_failing(app, column) {
        Some(reason) => Cell::Unavailable { reason },
        None => Cell::Download {
            path: asset_path(app, column),
        },
    }
}

// ---------------------------------------------------------------------------------------------
// The numbers
// ---------------------------------------------------------------------------------------------

/// What the Demos section holds, counted from the manifest.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Numbers {
    /// The rows of the table.
    pub apps: usize,
    /// The cells that are no dash: files the apps are built as.
    pub built_files: usize,
    /// The built files that are not known to fail: a link each.
    pub downloads: usize,
    /// The cells that say "not available" and why.
    pub unavailable: usize,
    /// The cells that are a dash.
    pub dashes: usize,
    /// The web rows: the apps with `docker = true`, a link to the Dockerfile each.
    pub web_rows: usize,
}

impl Numbers {
    /// Every `<a>` of the section.
    pub fn links(&self) -> usize {
        self.downloads + self.web_rows
    }
}

impl std::fmt::Display for Numbers {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{} apps x {} columns: {} built files = {} downloads + {} not available; {} dashes; \
             {} web rows; {} links in all",
            self.apps,
            COLUMNS.len(),
            self.built_files,
            self.downloads,
            self.unavailable,
            self.dashes,
            self.web_rows,
            self.links()
        )
    }
}

pub fn numbers(manifest: &Manifest) -> Numbers {
    let mut n = Numbers {
        apps: manifest.apps.len(),
        built_files: 0,
        downloads: 0,
        unavailable: 0,
        dashes: 0,
        web_rows: 0,
    };
    for app in &manifest.apps {
        for column in COLUMNS.iter().copied() {
            match cell(app, column) {
                Cell::NotBuilt => n.dashes += 1,
                Cell::Download { .. } => {
                    n.built_files += 1;
                    n.downloads += 1;
                }
                Cell::Unavailable { .. } => {
                    n.built_files += 1;
                    n.unavailable += 1;
                }
            }
        }
        if app.docker {
            n.web_rows += 1;
        }
    }
    n
}

// ---------------------------------------------------------------------------------------------
// The web (Docker) rows
// ---------------------------------------------------------------------------------------------

/// The name the Dockerfile of a web app is published under, next to the other small assets of the
/// release (`release/<version>/<Package>.Dockerfile`).
pub fn dockerfile_name(app: &App) -> String {
    format!("{}.Dockerfile", app.package)
}

/// What the deploy copies for the web rows: the Dockerfile of every app with `docker = true`
/// (relative to the repository root) and the name it is published under.
pub fn dockerfile_copies(manifest: &Manifest) -> Vec<(String, String)> {
    manifest
        .apps
        .iter()
        .filter(|a| a.docker)
        .map(|a| {
            (
                format!("{}/Dockerfile", a.dir.trim_end_matches('/')),
                dockerfile_name(a),
            )
        })
        .collect()
}

// ---------------------------------------------------------------------------------------------
// The Demos section of the release page
// ---------------------------------------------------------------------------------------------

/// `asset_url(version, path)` of `deploy.rs`: where the page links the file at `path` of the release.
/// It is a parameter so that the generator knows nothing of where the release is hosted.
pub type AssetUrl = fn(&str, &str) -> String;

/// Text for an HTML text node or a single-quoted attribute.
fn esc(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '\'' => out.push_str("&#39;"),
            '"' => out.push_str("&quot;"),
            c => out.push(c),
        }
    }
    out
}

/// Text of the manifest for an HTML text node: escaped, and a `code` span of the markdown
/// (`` `meet` ``) is a `<code>` element.
fn inline(s: &str) -> String {
    let parts: Vec<&str> = s.split('`').collect();
    if parts.len() % 2 == 0 {
        // an odd number of backticks: one is not closed, so none of them is code
        return esc(s);
    }
    let mut out = String::new();
    for (i, part) in parts.iter().enumerate() {
        if i % 2 == 1 {
            out.push_str(&format!("<code>{}</code>", esc(part)));
        } else {
            out.push_str(&esc(part));
        }
    }
    out
}

/// One cell of the table.
fn cell_html(version: &str, app: &App, column: Column, asset_url: AssetUrl) -> String {
    let id = column.id();
    match cell(app, column) {
        Cell::NotBuilt => format!(
            "<td class='demo-none' data-column='{id}' title='not built for this platform'>-</td>"
        ),
        Cell::Unavailable { reason } => format!(
            "<td class='demo-cell is-missing' data-column='{id}' title='not available: {title}'>\
             <span class='demo-na'>not available</span><small class='demo-reason'>{text}</small></td>",
            title = esc(reason),
            text = inline(reason),
        ),
        Cell::Download { path } => format!(
            "<td class='demo-cell' data-column='{id}'><a href='{url}' title='{file}' \
             aria-label='{label}'>{kind}</a></td>",
            url = esc(&asset_url(version, &path)),
            file = esc(&release_name(app, column)),
            label = esc(&format!("{} for {}", app.display_name(), column.heading())),
            kind = column.file_kind(),
        ),
    }
}

/// One row of the table: the app, then one cell per column.
fn row_html(version: &str, app: &App, asset_url: AssetUrl) -> String {
    let mut head = format!(
        "<strong>{}</strong><span class='demo-desc'>{}</span>",
        esc(app.display_name()),
        inline(&app.description)
    );
    if !app.needs.is_empty() {
        let needs: Vec<String> = app.needs.iter().map(|n| inline(n)).collect();
        head.push_str(&format!(
            "<span class='demo-needs'>Needs {}</span>",
            needs.join("; ")
        ));
    }
    let mut row = format!(
        "<tr id='demo-{}'>\n<th scope='row' class='demo-app'>{head}</th>\n",
        esc(&app.package)
    );
    for column in COLUMNS.iter().copied() {
        row.push_str(&cell_html(version, app, column, asset_url));
        row.push('\n');
    }
    row.push_str("</tr>");
    row
}

/// The table of the downloads: a row per app of the manifest, a column per built file, in a box that
/// scrolls on a screen too narrow for it.
pub fn downloads_table_html(version: &str, manifest: &Manifest, asset_url: AssetUrl) -> String {
    let mut html = String::new();
    html.push_str(
        "<div class='release-demo-table' role='region' aria-label='Demo downloads' tabindex='0'>\n",
    );
    html.push_str("<table>\n<thead>\n<tr><th scope='col'>App</th>");
    for column in COLUMNS.iter().copied() {
        html.push_str(&format!(
            "<th scope='col' data-column='{id}' title='{hint}'>{heading}<small>{note}</small></th>",
            id = column.id(),
            hint = esc(column.hint()),
            heading = esc(column.heading()),
            note = esc(column.note()),
        ));
    }
    html.push_str("</tr>\n</thead>\n<tbody>\n");
    for app in &manifest.apps {
        html.push_str(&row_html(version, app, asset_url));
        html.push('\n');
    }
    html.push_str("</tbody>\n</table>\n</div>");
    html
}

/// The web rows: for each app with a Dockerfile, the Dockerfile and the two commands that run it.
/// The image reuses the Linux x86-64 binary of the app and serves it in the browser.
pub fn web_html(version: &str, manifest: &Manifest, asset_url: AssetUrl) -> String {
    let rows: Vec<String> = manifest
        .apps
        .iter()
        .filter(|a| a.docker)
        .map(|app| {
            let url = esc(&asset_url(version, &dockerfile_name(app)));
            // the image tag is the app's own name, lower-cased: the command a reader copies names the
            // thing they just clicked on
            let tag = esc(&app.package.to_lowercase());
            format!(
                "<li id='demo-web-{package}'><strong>{name}</strong> <a href='{url}'>Dockerfile</a>\n\
                 <pre><code class='language-bash'>docker build {url} -t {tag}\n\
                 docker run -p 8080:8080 {tag}</code></pre></li>",
                package = esc(&app.package),
                name = esc(app.display_name()),
            )
        })
        .collect();
    if rows.is_empty() {
        return String::new();
    }
    format!(
        "<h3 id='demos-web'>Web (Docker, experimental)</h3>\n\
         <p class='release-note'>Run a demo as a web app with one command: the image reuses the Linux \
         x86-64 binary of the app and serves it in your browser.</p>\n\
         <ul class='release-demos-web'>\n{}\n</ul>",
        rows.join("\n")
    )
}

/// What the page says before the table, once for every row.
pub fn intro_html() -> &'static str {
    "<p class='release-note'>One row per demo app, one column per file. A desktop download is a \
     .tar.gz holding a self-contained binary: the app is linked statically against the prebuilt \
     libazul, so nothing else needs to be installed. The iOS device .app is unsigned: AzReview signs \
     it for your phone. A dash means the app is not built for that platform; \"not available\" says \
     why a build is missing.</p>"
}

/// The whole of the Demos section below the AzBuilder text: the intro, the table and the web rows.
pub fn demos_section_html(version: &str, manifest: &Manifest, asset_url: AssetUrl) -> String {
    let mut parts = vec![
        intro_html().to_string(),
        downloads_table_html(version, manifest, asset_url),
    ];
    let web = web_html(version, manifest, asset_url);
    if !web.is_empty() {
        parts.push(web);
    }
    parts.join("\n")
}

#[cfg(test)]
#[path = "demo_apps_test.rs"]
mod tests;
