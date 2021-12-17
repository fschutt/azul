extern crate tar;

use std::fs;
use std::io;
use std::env;
use std::process::{Command, Stdio};
use std::path::{Path, PathBuf};
use std::ffi::OsStr;
use std::fs::DirEntry;
use std::io::Write;
use std::io::Read;
use std::io::{BufReader, BufRead};

const INSTALL_DIRECTORY_VAR: &str = "AZUL_LINK_PATH";

fn main() {

    let cargo_home = match env::var("CARGO_HOME") {
        Ok(val) => val,
        Err(e) => {
            println!("WARNING (azul/azul-dll:build.rs): could not find environment variable CARGO_HOME: rustup not installed?: {}", e); 
            // don't error, installation can still continue without CARGO_HOME, 
            // usually when building locally or in a VM
            return; 
        },
    };

    // 0. find the source crate "azul-dll-0.1.0.crate" in ~/.cargo/registry/cache/
    let crate_file_path = find_crate(
        &format!("{}/registry/cache/", cargo_home), 
        concat!(env!("CARGO_PKG_NAME"), "-", env!("CARGO_PKG_VERSION"), ".crate")
    );

    let crate_file_path = match crate_file_path {
        None => {
            println!("error: could not find crate file in /registry/cache!");
            return;
        },
        Some(s) => s,
    };

    // 1. re-create the output path for the DLL, i.e "~/.cargo/lib/azul-dll-0.1.0"
    let dll_output_path = match env::var(INSTALL_DIRECTORY_VAR).ok() {
        Some(s) => s,
        None => format!("{}/lib/{}-{}", cargo_home, env!("CARGO_PKG_NAME"), env!("CARGO_PKG_VERSION")),
    };

    env::set_var(INSTALL_DIRECTORY_VAR, dll_output_path.clone());

    if Path::new(&dll_output_path).exists() {
        fs::remove_dir_all(&dll_output_path).unwrap();
    }

    fs::create_dir_all(&dll_output_path).unwrap();

    // 2. unzip it into ~/.cargo/lib/azul-dll-0.1.0
    unzip_file_into_dir(crate_file_path.as_path(), &format!("{}/lib/", cargo_home)).unwrap();

    // 3. remove the 'build = "build.rs"' from the Cargo.toml file to prevent a infinite build loop
    let file = fs::read_to_string(&format!("{}/Cargo.toml", dll_output_path)).unwrap();
    let new_file = file.replace("build = \"build.rs\"", "");
    fs::write(&format!("{}/Cargo.toml", dll_output_path), &new_file).unwrap();

    // if Cargo.toml.orig exists, remove it
    let _ = fs::remove_file(&format!("{}/Cargo.toml.orig", dll_output_path));
    let _ = fs::remove_file(&format!("{}/build.rs", dll_output_path));

    // 4. run "cargo build --release" in the dll_output_path
    let child = Command::new(env!("CARGO"))
        .current_dir(&dll_output_path)
        .stdout(Stdio::piped())
        .args(&["build", "--release", "--all-features", "--lib", "--package", env!("CARGO_PKG_NAME")])
        .spawn()
        .expect("cargo build --release --all-features failed to start");

    let child_stdout = child.stdout.expect("cannot access stdout");

    let reader = BufReader::new(child_stdout);

    reader
        .lines()
        .filter_map(|line| line.ok())
        .for_each(|line| println!("{}", line));


    // finished, the output file is now built in AZUL_INSTALL_DIR (= "~/.cargo/lib/azul-dll-0.1.0/target/release/libazul.so")
}

// returns the file path to the "azul-dll-0.1.0.crate"
fn find_crate(path: &str, crate_name: &str) -> Option<PathBuf> {
    let path = PathBuf::from(path);
    let path = path.as_path();
    let mut found_file_path = None;
    visit_dirs(path, &mut |file_path: &DirEntry| {
        if file_path.file_name() == OsStr::new(crate_name) {
            found_file_path = Some(file_path.path());
        }
        found_file_path.is_some()
    }).unwrap();
    found_file_path
}

fn visit_dirs(dir: &Path, cb: &mut dyn FnMut(&DirEntry) -> bool) -> io::Result<()> {
    if dir.is_dir() {
        for entry in fs::read_dir(dir)? {
            let entry = entry?;
            let path = entry.path();
            if path.is_dir() {
                visit_dirs(&path, cb)?;
            } else {
                if cb(&entry) { return Ok(()) };
            }
        }
    }

    Ok(())
}


fn unzip_file_into_dir(zip_file_path: &Path, target_path: &str) -> Option<()> {

    use tar::Archive;
    use flate2::read::GzDecoder;

    // NOTE: Currently cargo uses the tarball format, see
    // https://github.com/rust-lang/cargo/blob/534ce68621ce4feec0b7e8627cfd3b077d4f3900/src/cargo/ops/cargo_package.rs#L185

    let file = fs::File::open(zip_file_path).unwrap();
    let file = GzDecoder::new(file);
    let mut a = Archive::new(file);

    for file in a.entries().unwrap() {
        // Make sure there wasn't an I/O error
        let mut file = file.unwrap();

        let tar_path = file.path().unwrap();
        let mut outpath = PathBuf::from(target_path);
        outpath.push(tar_path.clone());

        if tar_path.is_dir() {
            fs::create_dir_all(&outpath).unwrap();
        } else {
            if let Some(p) = outpath.parent() {
                if !p.exists() {
                    fs::create_dir_all(&p).unwrap();
                }
            }

            let mut s = String::new();
            let mut outfile = fs::File::create(&outpath).unwrap();
            file.read_to_string(&mut s).unwrap();
            outfile.write(s.as_bytes()).unwrap();
        }

        // Get and Set permissions
        #[cfg(unix)] {
            use std::os::unix::fs::PermissionsExt;

            if let Ok(mode) = file.header().mode() {
                fs::set_permissions(&outpath, fs::Permissions::from_mode(mode)).unwrap();
            }
        }
    }

    Some(())
}