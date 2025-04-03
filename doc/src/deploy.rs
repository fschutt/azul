use std::{
    env,
    fs::{self, File},
    io::{self, Write},
    path::{Path, PathBuf},
    process::Command,
};

use anyhow::{Context, Result};
use zip::write::FileOptions;

use crate::{api, codegen, docgen, license::License, utils};

pub struct Config {
    pub build_windows: bool,
    pub build_linux: bool,
    pub build_macos: bool,
    pub build_python: bool,
}

impl Config {
    pub fn from_env() -> Self {
        Self {
            build_windows: env::var("BUILD_WINDOWS").unwrap_or_else(|_| "1".to_string()) != "0",
            build_linux: env::var("BUILD_LINUX").unwrap_or_else(|_| "1".to_string()) != "0",
            build_macos: env::var("BUILD_MACOS").unwrap_or_else(|_| "1".to_string()) != "0",
            build_python: env::var("BUILD_PYTHON").unwrap_or_else(|_| "1".to_string()) != "0",
        }
    }
}

pub fn generate_license_files(version: &str, output_dir: &Path) -> Result<()> {
    println!("  Generating license files...");

    let dll_path = concat!(env!("CARGO_MANIFEST_DIR"), "/../dll");

    assert!(Path::new(dll_path).join("Xargo.toml").exists());

    let targets = &[
        ("LICENSE-WINDOWS.txt", "x86_64-pc-windows-msvc"),
        ("LICENSE-MAC.txt", "aarch64-apple-darwin"),
        ("LICENSE-LINUX.txt", "x86_64-unknown-linux-gnu"),
    ];

    for (f, target) in targets.iter() {
        // Use cargo-license to get dependency information
        let cargo_meta_cmd = cargo_metadata::MetadataCommand::new()
            .current_dir(dll_path)
            .env("CARGO_BUILD_TARGET", target)
            .clone();

        let opt = cargo_license::GetDependenciesOpt {
            avoid_dev_deps: true,
            avoid_build_deps: true,
            direct_deps_only: false,
            root_only: false,
        };

        let l = cargo_license::get_dependencies_from_cargo_lock(cargo_meta_cmd, opt)
            .unwrap_or_default()
            .into_iter()
            .map(|s| License {
                name: s.name.to_string(),
                version: s.version.to_string(),
                license_type: s.license.unwrap_or_default(),
                authors: s
                    .authors
                    .unwrap_or_default()
                    .split(",")
                    .map(|s| s.to_string())
                    .collect(),
            })
            .collect::<Vec<_>>();

        let default_license_text = vec![
            "[program] is based in part on the AZUL GUI toolkit (https://azul.rs),",
            "licensed under the MIT License (C) 2018 Felix Schütt.",
            "",
            "The AZUL GUI toolkit itself uses the following libraries:",
            "",
            "",
        ]
        .join("\r\n");

        let license_posttext = vec![
            "",
            "To generate the full text of the license for the license, please visit",
            "https://spdx.org/licenses/ and replace the license author in the source",
            "text in any given license with the name of the author listed above.",
        ]
        .join("\r\n");

        let mut s = String::new();
        s.push_str(&default_license_text);
        s.push_str(&crate::license::format_license_authors(&l));
        s.push_str(&license_posttext);
        std::fs::write(&output_dir.join(f), &s)?;
    }

    println!("  - Generated license files");
    Ok(())
}

pub fn create_examples(
    version: &str,
    output_dir: &Path,
    azul_h: &str,
    azul_hpp: &str,
) -> Result<()> {
    println!("  Creating example packages...");

    // Create a temporary directory for the examples
    let source_zip_path = output_dir.join("sourcecode.zip");
    let source_zip_file = File::create(&source_zip_path)?;

    let mut source_zip = zip::ZipWriter::new(source_zip_file);
    let options = zip::write::SimpleFileOptions::default();

    // -- c

    source_zip.start_file("hello-world.c", options)?;
    source_zip.write_all(include_bytes!("./../../examples/c/hello-world.c"))?;
    source_zip.start_file("calculator.c", options)?;
    source_zip.write_all(include_bytes!("./../../examples/c/calculator.c"))?;
    source_zip.start_file("svg.c", options)?;
    source_zip.write_all(include_bytes!("./../../examples/c/svg.c"))?;
    source_zip.start_file("table.c", options)?;
    source_zip.write_all(include_bytes!("./../../examples/c/table.c"))?;
    source_zip.start_file("xhtml.c", options)?;
    source_zip.write_all(include_bytes!("./../../examples/c/xhtml.c"))?;

    // -- cpp

    source_zip.start_file("hello-world.cpp", options)?;
    source_zip.write_all(include_bytes!("./../../examples/cpp/hello-world.cpp"))?;
    source_zip.start_file("calculator.cpp", options)?;
    source_zip.write_all(include_bytes!("./../../examples/cpp/calculator.cpp"))?;
    source_zip.start_file("svg.cpp", options)?;
    source_zip.write_all(include_bytes!("./../../examples/cpp/svg.cpp"))?;
    source_zip.start_file("table.cpp", options)?;
    source_zip.write_all(include_bytes!("./../../examples/cpp/table.cpp"))?;
    source_zip.start_file("xhtml.cpp", options)?;
    source_zip.write_all(include_bytes!("./../../examples/cpp/xhtml.cpp"))?;

    // -- rust

    source_zip.start_file("hello-world.rs", options)?;
    source_zip.write_all(include_bytes!("./../../examples/rust/hello-world.rs"))?;
    source_zip.start_file("calculator.rs", options)?;
    source_zip.write_all(include_bytes!("./../../examples/rust/calculator.rs"))?;
    source_zip.start_file("svg.rs", options)?;
    source_zip.write_all(include_bytes!("./../../examples/rust/svg.rs"))?;
    source_zip.start_file("table.rs", options)?;
    source_zip.write_all(include_bytes!("./../../examples/rust/table.rs"))?;
    source_zip.start_file("xhtml.rs", options)?;
    source_zip.write_all(include_bytes!("./../../examples/rust/xhtml.rs"))?;

    // -- python

    source_zip.start_file(&"hello-world.py", options)?;
    source_zip.write_all(include_bytes!("./../../examples/python/hello-world.py"))?;
    source_zip.start_file("calculator.py", options)?;
    source_zip.write_all(include_bytes!("./../../examples/python/calculator.py"))?;
    source_zip.start_file("svg.py", options)?;
    source_zip.write_all(include_bytes!("./../../examples/python/svg.py"))?;
    source_zip.start_file("table.py", options)?;
    source_zip.write_all(include_bytes!("./../../examples/python/table.py"))?;
    source_zip.start_file("xhtml.py", options)?;
    source_zip.write_all(include_bytes!("./../../examples/python/xhtml.py"))?;

    source_zip.start_file("include/azul.h", options)?;
    source_zip.write_all(azul_h.as_bytes())?;

    source_zip.start_file("include/azul.hpp", options)?;
    source_zip.write_all(azul_hpp.as_bytes())?;

    // Add some basic source files
    source_zip.start_file("README.md", options)?;
    source_zip.write_all(
        format!(
            "# Azul GUI Framework v{}\n\nCross-platform GUI framework for Rust, C, C++ and Python",
            version
        )
        .as_bytes(),
    )?;

    // Finalize source zip
    source_zip.finish()?;

    println!("  - Created example packages");

    Ok(())
}

pub fn create_git_repository(version: &str, output_dir: &Path, lib_rs: &str) -> Result<()> {
    println!("  Creating Git repository for version {}...", version);

    // Create repository directory
    let repo_dir = output_dir.join(format!("{}.git", version));
    fs::create_dir_all(&repo_dir)?;

    // Create basic repo structure
    fs::create_dir_all(repo_dir.join("objects/info"))?;
    fs::create_dir_all(repo_dir.join("objects/pack"))?;
    fs::create_dir_all(repo_dir.join("refs/heads"))?;
    fs::create_dir_all(repo_dir.join("refs/tags"))?;

    // Create HEAD file
    fs::write(repo_dir.join("HEAD"), "ref: refs/heads/master\n")?;

    // Create config file
    fs::write(
        repo_dir.join("config"),
        r#"[core]
    repositoryformatversion = 0
    filemode = false
    bare = true
    "#,
    )?;

    // Create description file
    fs::write(
        repo_dir.join("description"),
        format!("Azul GUI Framework v{}", version),
    )?;

    // For demonstration, create the src directory structure with lib.rs
    let src_dir = repo_dir.join("src");
    fs::create_dir_all(&src_dir)?;
    fs::write(src_dir.join("lib.rs"), lib_rs)?;

    // Create Cargo.toml
    fs::write(
        repo_dir.join("Cargo.toml"),
        format!(
            r#"[package]
        name = "azul"
        version = "{}"
        authors = ["Felix Schütt <felix.schuett@maps4print.com>"]
        license = "MIT"
        description = '''
            Azul GUI is a free, functional, reactive GUI framework
            for rapid development of desktop applications written in Rust and C,
            using the Mozilla WebRender rendering engine.
        '''
        homepage = "https://azul.rs/"
        keywords = ["gui", "GUI", "user-interface", "svg", "graphics" ]
        categories = ["gui"]
        repository = "https://github.com/fschutt/azul"
        readme = "README.md"
        exclude = ["assets/*", "doc/*", "examples/*"]
        autoexamples = false
        edition = "2021"
        build = "build.rs"
        links = "azul"

        [dependencies]
        serde = {{ version = "1", optional = true, default-features = false }}
        serde_derive = {{ version = "1", optional = true, default-features = false }}

        [features]
        default = ["link-static"]
        serde-support = ["serde_derive", "serde"]
        docs_rs = ["link-static"]
        link-dynamic = []
        link-static = []

        [package.metadata.docs.rs]
        features = ["docs_rs"]
    "#,
            version
        )
        .lines()
        .map(|s| s.trim())
        .collect::<Vec<_>>()
        .join("\r\n"),
    )?;

    // Create build.rs
    fs::write(
        repo_dir.join("build.rs"),
        r#"fn main() {
    // dynamically link azul.dll
    #[cfg(all(feature = "link-dynamic", not(feature = "link-static")))]
    {
        println!("cargo:rustc-link-search={}", env!("AZUL_LINK_PATH")); /* path to folder with azul.dll / libazul.so */
    }
}
"#,
    )?;

    println!("  - Created Git repository structure");
    Ok(())
}

pub fn generate_release_html(version: &str, releasenotes_html: &str) -> String {
    format!(
        r#"<!DOCTYPE html>
<html lang="en">
<head>
  <title>Release notes - Azul GUI v{version}</title>
  <meta charset="utf-8"/>
  <meta name="viewport" content="width=device-width, initial-scale=1">
  <meta http-equiv="Content-Type" content="text/html; charset=utf-8"/>
  <meta name="description" content="Cross-platform MIT-licensed desktop GUI framework for C and Rust using the Mozilla WebRender rendering engine">
  <meta name="keywords" content="gui, rust, user interface">

  <link rel="preload" as="font" href="../fonts/SourceSerifPro-Regular.ttf" type="font/ttf">
  <link rel="preload" as="font" href="../fonts/Morris Jenson Initialen.ttf" type="font/ttf">

  <link rel="shortcut icon" type="image/x-icon" href="https://azul.rs/favicon.ico">
  <link rel="stylesheet" type="text/css" href="https://azul.rs/main.css">
</head>

<body>
  <div class="center">
  <aside>
    <header>
      <h1 style="display:none;">Azul GUI Framework</h1>
      <a href="https://azul.rs/">
        <img src="https://azul.rs/logo.svg">
      </a>
    </header>
    <nav>
      <ul>
        <li><a href="https://azul.rs">overview</a></li>
        <li>
          <a href="https://azul.rs/releases">releases</a>
        </li>
        <li><a href="https://github.com/fschutt/azul">code</a></li>
        <li><a href="https://matrix.to/#/#azul-gui:matrix.org">chat</a></li>
        <li>
          <a href="https://azul.rs/guide">guide</a>
        </li>
        <li>
          <a href="https://azul.rs/api">api</a>
        </li>
      </ul>
    </nav>
  </aside>
  <main>
    <h1>Azul GUI v{version}</h1>
    <style>ul {{ margin-left: 20px; margin-top: 20px; list-style-type: none; }} nav ul {{ margin: 0px; }}</style>
    <div>
      
        {releasenotes_html}

        <br/>

        <strong>Links:</strong>
        <ul>
        <li><a href="https://azul.rs/api/{version}">Documentation for this release</a></li>
        <li><a href="https://azul.rs/guide/{version}">Guide for this release</a></li>
        <br/>
        <li><a href="https://github.com/fschutt/azul/releases/tag/{version}">GitHub release</a></li>
        <li><a href="https://crates.io/crates/azul/{version}">Crates.io</a></li>
        <li><a href="https://docs.rs/azul/{version}">Docs.rs</a></li>
        </ul>

        <br/>

        <strong>Files:</strong>
        <br/>
        <ul>
        <li><a href="https://azul.rs/release/{version}/azul.dll">Windows 64-bit DLL (azul.dll - 2.6Mb)</a></li>
        <li><a href="https://azul.rs/release/{version}/azul.dll">Windows 64-bit DLL (azul.dll - 2.6Mb)</a></li>
        <li><a href="https://azul.rs/release/{version}/LICENSE-WINDOWS.txt">LICENSE-WINDOWS.txt (19KB)</a></li>
        </ul>
        <ul>
        <li><a href="https://azul.rs/release/{version}/azul.dll">Linux 64-bit .so (libazul.so - 2.6Mb)</a></li>
        <li><a href="https://azul.rs/release/{version}/azul.dll">Linux 64-bit .a (libazul.a - 2.6Mb)</a></li>
        <li><a href="https://azul.rs/release/{version}/LICENSE-LINUX.txt">LICENSE-LINUX.txt (19KB)</a></li>
        </ul>
        <ul>
        <li><a href="https://azul.rs/release/{version}/azul.dll">MacOS 64-bit SO (libazul.so - 2.6Mb)</a></li>
        <li><a href="https://azul.rs/release/{version}/azul.">MacOS 64-bit .a (libazul.a - 2.6Mb)</a></li>
        <li><a href="https://azul.rs/release/{version}/LICENSE-MACOS.txt">LICENSE-MACOS.txt (19KB)</a></li>
        </ul>
        <ul>
        <li><a href="https://azul.rs/release/{version}/azul.h">C Header (azul.h - 978KB)</a></li>
        <li><a href="https://azul.rs/release/{version}/azul.hpp">CPP Header (azul.hpp - 978KB)</a></li>
        <li><a href="https://azul.rs/release/{version}/azul.pyd">Python Extension (azul.pyd - 978KB)</a></li>
        </ul>

        <br/>

        <strong>Other links:</strong>

        <br/>

        <ul>
        <li><a href="https://azul.rs/release/{version}/api.json">API Description - api.json (714KB)</a></li>
        <li><a href="./files/examples-windows.zip">Windows examples with source code (.zip - 154KB)</a></li>
        </ul>

        <br/>
        <strong>Rust dependency:</strong>
        <br/>
        <div style="padding:20px;background:rgb(236, 236, 236);margin-top: 20px;">
            <p style="color:grey;font-family:monospace;"># Cargo.toml</p>
            <p style="color:black;font-family:monospace;">[dependencies.azul]</p>
            <p style="color:black;font-family:monospace;">git = "https://azul.rs/{version}.git"</p>
            <br/>
            <p style="color:grey;font-family:monospace;"># Dynamic linking:</p>
            <p style="color:grey;font-family:monospace;"># export AZUL_LINK_PATH=/path/to/azul.dll</p>
            <p style="color:grey;font-family:monospace;"># features = ["link-dynamic"]</p>

        </pre>
    </div>
    </div>
  </main>
</div>
</body>
</html>"#
    )
}

pub fn generate_releases_index(versions: &[String]) -> String {
    let mut version_items = String::new();
    for version in versions {
        version_items.push_str(&format!(
            "<li><a href=\"https://azul.rs/release/{}\">{}</a></li>\n",
            version, version
        ));
    }

    format!(
        r#"<!DOCTYPE html>
<html lang="en">
<head>
  <title>Choose release version</title>
  <meta charset="utf-8"/>
  <meta name="viewport" content="width=device-width, initial-scale=1">
  <meta http-equiv="Content-Type" content="text/html; charset=utf-8"/>
  <meta name="description" content="Cross-platform MIT-licensed desktop GUI framework for C and Rust using the Mozilla WebRender rendering engine">
  <meta name="keywords" content="gui, rust, user interface">

  <link rel="preload" as="font" href="fonts/SourceSerifPro-Regular.ttf" type="font/ttf">
  <link rel="preload" as="font" href="fonts/Morris Jenson Initialen.ttf" type="font/ttf">

  <link rel="shortcut icon" type="image/x-icon" href="https://azul.rs/favicon.ico">
  <link rel="stylesheet" type="text/css" href="https://azul.rs/main.css">
</head>

<body>
  <div class="center">
  <aside>
    <header>
      <h1 style="display:none;">Azul GUI Framework</h1>
      <a href="https://azul.rs/">
        <img src="https://azul.rs/logo.svg">
      </a>
    </header>
    <nav>
      <ul>
        <li><a href="https://azul.rs">overview</a></li>
        <li>
          <a href="https://azul.rs/releases">releases</a>
        </li>
        <li><a href="https://github.com/fschutt/azul">code</a></li>
        <li><a href="https://matrix.to/#/#azul-gui:matrix.org">chat</a></li>
        <li>
          <a href="https://azul.rs/guide">guide</a>
        </li>
        <li>
          <a href="https://azul.rs/api">api</a>
        </li>
      </ul>
    </nav>
  </aside>
  <main>
    <h1>Choose release version</h1>
    <div>
      <ul>{}</ul>
    </div>
  </main>
  </div>
  <script async type="text/javascript" src="https://azul.rs/prism_code_highlighter.js"></script>
</body>
</html>"#,
        version_items
    )
}

pub fn copy_static_assets(output_dir: &Path) -> Result<()> {
    println!("Copying static assets...");

    // Create assets directories
    let fonts_dir = output_dir.join("fonts");
    let images_dir = output_dir.join("images");
    fs::create_dir_all(&fonts_dir)?;
    fs::create_dir_all(&images_dir)?;

    // Copy CSS file
    fs::write(
        output_dir.join("main.css"),
        include_str!("../templates/main.css"),
    )?;

    // Copy JavaScript file
    fs::write(
        output_dir.join("prism_code_highlighter.js"),
        include_str!("../templates/prism_code_highlighter.js"),
    )?;

    // Copy logo SVG
    fs::write(
        output_dir.join("logo.svg"),
        include_str!("../templates/logo.svg"),
    )?;

    // Copy fleur-de-lis SVG (for navigation)
    fs::write(
        images_dir.join("fleur-de-lis.svg"),
        include_str!("../templates/fleur-de-lis.svg"),
    )?;

    // Copy font files - in a real implementation, we'd copy actual font files
    fs::write(
        fonts_dir.join("SourceSerifPro-Regular.ttf"),
        "Font file placeholder",
    )?;
    fs::write(
        fonts_dir.join("Morris Jenson Initialen.ttf"),
        "Font file placeholder",
    )?;

    // Create favicon
    fs::write(output_dir.join("favicon.ico"), "Favicon placeholder")?;

    // Copy minimal index.html
    let index_html =
        include_str!("../templates/index.template.html").replace("$$ROOT_RELATIVE$$", "");
    fs::write(output_dir.join("index.html"), index_html)?;

    println!("Static assets copied successfully");
    Ok(())
}
