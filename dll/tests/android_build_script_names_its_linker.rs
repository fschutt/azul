//! `scripts/build-android.sh` names the linker of the NDK it was pointed at, on whatever machine it runs.
//!
//! The workspace's `.cargo/config.toml` names the Android linkers once, for the Homebrew layout of a Mac
//! (`/opt/homebrew/share/android-commandlinetools/ndk/.../darwin-x86_64/bin/<triple>24-clang`). On any other
//! machine cargo then says `error: linker ... not found` the moment it links the app's `cdylib`
//! (or `azul-dll` itself, which is a `cdylib` too). That is exactly what the first x86_64 emulator
//! run of 2026-11-26 died of (run 45764213148, `could not compile azul-dll (lib)`): the library step
//! (scripts/demo_lib.sh) exported the linker, the app step (this script) did not, so the library built and
//! the app could not link on a Linux runner. The arm64 build "worked" only on the one Mac that has the
//! Homebrew path. The environment wins over `config.toml`, so the script names the linker of ITS NDK.
//!
//! The tests run the real script against a fake SDK and a fake `cargo` that writes down the
//! `CARGO_TARGET_*` variables it was started with, then fails (the script stops there). They need
//! nothing built:
//!
//!     CARGO_MANIFEST_DIR=$PWD/dll rustc --edition 2021 --test dll/tests/android_build_script_names_its_linker.rs -o /tmp/t && /tmp/t

use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
};

/// A fake Android SDK, a fake workspace to build in, and a `cargo` that records its environment.
struct Fixture {
    dir: PathBuf,
}

impl Fixture {
    fn new(name: &str) -> Fixture {
        let dir = std::env::temp_dir().join(format!(
            "azul-android-script-{}-{}",
            name,
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&dir);
        let sdk = dir.join("sdk");
        for tool in ["aapt2", "zipalign", "apksigner"] {
            touch(&sdk.join("build-tools/34.0.0").join(tool));
        }
        touch(&sdk.join("platforms/android-34/android.jar"));
        fs::create_dir_all(sdk.join("ndk/27.0.12077973")).unwrap();
        fs::create_dir_all(dir.join("ws")).unwrap();
        let cargo = dir.join("bin/cargo");
        touch(&cargo);
        fs::write(
            &cargo,
            "#!/bin/sh\n\
             env | grep '^CARGO_TARGET_' | sort > \"$FAKE_CARGO_ENV\"\n\
             echo \"$@\" > \"$FAKE_CARGO_ARGS\"\n\
             exit 1\n",
        )
        .unwrap();
        make_executable(&cargo);
        Fixture { dir }
    }

    fn sdk(&self) -> PathBuf {
        self.dir.join("sdk")
    }

    /// The `bin` directory of the NDK's clang for the machine the test runs on (the script picks the
    /// prebuilt directory by `uname -s`).
    fn ndk_bin(&self) -> PathBuf {
        let host = if cfg!(target_os = "macos") {
            "darwin-x86_64"
        } else {
            "linux-x86_64"
        };
        self.sdk()
            .join("ndk/27.0.12077973/toolchains/llvm/prebuilt")
            .join(host)
            .join("bin")
    }

    /// Runs the script for `target` with `extra_env` and returns the `CARGO_TARGET_*` lines the
    /// fake cargo saw, and the arguments it was called with.
    fn run(&self, target: &str, extra_env: &[(&str, &str)]) -> (Vec<String>, String) {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("..");
        let script = root.join("scripts/build-android.sh");
        assert!(script.is_file(), "{} is missing", script.display());
        let env_file = self.dir.join("cargo.env");
        let args_file = self.dir.join("cargo.args");
        let path = format!(
            "{}:{}",
            self.dir.join("bin").display(),
            std::env::var("PATH").unwrap_or_default()
        );
        let mut cmd = Command::new("bash");
        cmd.arg(&script)
            .arg(target)
            .arg("App")
            .arg("com.test.app")
            .arg("App")
            // not the caller's linkers: the script has to name them itself
            .env_remove("CARGO_TARGET_AARCH64_LINUX_ANDROID_LINKER")
            .env_remove("CARGO_TARGET_AARCH64_LINUX_ANDROID_AR")
            .env_remove("CARGO_TARGET_X86_64_LINUX_ANDROID_LINKER")
            .env_remove("CARGO_TARGET_X86_64_LINUX_ANDROID_AR")
            .env_remove("CARGO_TARGET_ARMV7_LINUX_ANDROIDEABI_LINKER")
            .env_remove("CARGO_TARGET_ARMV7_LINUX_ANDROIDEABI_AR")
            .env_remove("CARGO_TARGET_I686_LINUX_ANDROID_LINKER")
            .env_remove("CARGO_TARGET_I686_LINUX_ANDROID_AR")
            .env("PATH", path)
            .env("ANDROID_HOME", self.sdk())
            .env_remove("ANDROID_NDK_HOME")
            .env("AZ_WORKSPACE_ROOT", self.dir.join("ws"))
            .env("FAKE_CARGO_ENV", &env_file)
            .env("FAKE_CARGO_ARGS", &args_file);
        for (k, v) in extra_env {
            cmd.env(k, v);
        }
        let out = cmd.output().expect("bash runs");
        let args = fs::read_to_string(&args_file).unwrap_or_else(|_| {
            panic!(
                "the script never reached cargo (exit {:?}):\n{}\n{}",
                out.status.code(),
                String::from_utf8_lossy(&out.stdout),
                String::from_utf8_lossy(&out.stderr)
            )
        });
        let env = fs::read_to_string(&env_file)
            .unwrap_or_default()
            .lines()
            .map(str::to_string)
            .collect();
        (env, args)
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.dir);
    }
}

fn touch(path: &Path) {
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, "").unwrap();
}

#[cfg(unix)]
fn make_executable(path: &Path) {
    use std::os::unix::fs::PermissionsExt;
    fs::set_permissions(path, fs::Permissions::from_mode(0o755)).unwrap();
}

#[cfg(not(unix))]
fn make_executable(_: &Path) {}

#[test]
fn the_x86_64_emulator_build_links_with_the_ndk_it_was_pointed_at() {
    let fx = Fixture::new("x86_64");
    let (env, args) = fx.run("x86_64-linux-android", &[]);
    assert!(
        args.contains("--target x86_64-linux-android"),
        "cargo was called with: {args}"
    );
    let want = format!(
        "CARGO_TARGET_X86_64_LINUX_ANDROID_LINKER={}",
        fx.ndk_bin().join("x86_64-linux-android24-clang").display()
    );
    assert!(
        env.contains(&want),
        "cargo must be told the linker (the workspace config names a Homebrew path that does not \
         exist on a Linux runner).\nwanted: {want}\ncargo saw: {env:#?}"
    );
}

#[test]
fn the_arm64_device_build_links_with_the_ndk_it_was_pointed_at() {
    let fx = Fixture::new("aarch64");
    let (env, _) = fx.run("aarch64-linux-android", &[]);
    let want = format!(
        "CARGO_TARGET_AARCH64_LINUX_ANDROID_LINKER={}",
        fx.ndk_bin().join("aarch64-linux-android24-clang").display()
    );
    assert!(env.contains(&want), "wanted: {want}\ncargo saw: {env:#?}");
}

#[test]
fn the_archiver_is_the_ndks_too() {
    let fx = Fixture::new("ar");
    let (env, _) = fx.run("x86_64-linux-android", &[]);
    let want = format!(
        "CARGO_TARGET_X86_64_LINUX_ANDROID_AR={}",
        fx.ndk_bin().join("llvm-ar").display()
    );
    assert!(env.contains(&want), "wanted: {want}\ncargo saw: {env:#?}");
}

#[test]
fn the_32_bit_arm_triple_uses_the_clang_the_ndk_names_armv7a() {
    // cargo's triple is armv7-linux-androideabi, the NDK's clang is armv7a-linux-androideabi24-clang
    let fx = Fixture::new("armv7");
    let (env, _) = fx.run("armv7-linux-androideabi", &[]);
    let want = format!(
        "CARGO_TARGET_ARMV7_LINUX_ANDROIDEABI_LINKER={}",
        fx.ndk_bin()
            .join("armv7a-linux-androideabi24-clang")
            .display()
    );
    assert!(env.contains(&want), "wanted: {want}\ncargo saw: {env:#?}");
}

#[test]
fn a_linker_the_caller_named_is_kept() {
    let fx = Fixture::new("caller");
    let (env, _) = fx.run(
        "x86_64-linux-android",
        &[("CARGO_TARGET_X86_64_LINUX_ANDROID_LINKER", "/my/own/clang")],
    );
    assert!(
        env.contains(&"CARGO_TARGET_X86_64_LINUX_ANDROID_LINKER=/my/own/clang".to_string()),
        "the caller's linker must win: {env:#?}"
    );
}
