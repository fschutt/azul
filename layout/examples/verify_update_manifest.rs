//! Verifies an update manifest + artifact exactly as a CLIENT will.
//!
//! `scripts/sign-release.sh` runs this on its own output before you publish
//! anything, because the ways a signature can be technically valid and still
//! be rejected are not obvious — the client requires PREHASHED minisign
//! signatures (`ED`), and a `minisign` invocation without `-H` produces
//! legacy ones (`Ed`) that verify fine with the `minisign` CLI and fail in
//! the app. A release that only gets checked by the tool that made it is not
//! checked at all.
//!
//! ```text
//! cargo run -p azul-layout --features updater --example verify_update_manifest -- \
//!     manifest.json ./azul-app-2.0.0.bin RWQ...rootpubkey
//!
//! # or, with the root key as a minisign .pub file:
//! cargo run -p azul-layout --features updater --example verify_update_manifest -- \
//!     manifest.json ./azul-app-2.0.0.bin --root-pub-file root.pub
//! ```
//!
//! `--selftest` mints a throwaway key hierarchy, signs a throwaway artifact
//! and verifies the result end to end — a way to check the chain code and to
//! see the exact byte formats without touching real keys.
//!
//! Exit status is the point: 0 = a client will accept this release, non-zero
//! = it will not, with the failing link named.

use std::{path::Path, process::ExitCode};

use azul_layout::updater::{
    parse_manifest_v1, parse_signing_key_statement, verify_digest, verify_release_signature,
    ReleaseInfo, UpdateState,
};

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.first().map(String::as_str) == Some("--selftest") {
        return selftest();
    }
    if args.len() < 3 {
        eprintln!(
            "usage: verify_update_manifest <manifest.json> <artifact> <root-pubkey-base64>\n\
             \x20      verify_update_manifest <manifest.json> <artifact> --root-pub-file <root.pub>\n\
             \x20      verify_update_manifest --selftest"
        );
        return ExitCode::from(2);
    }

    let manifest_path = &args[0];
    let artifact_path = &args[1];
    let root_key = if args[2] == "--root-pub-file" {
        let Some(path) = args.get(3) else {
            eprintln!("--root-pub-file needs a path");
            return ExitCode::from(2);
        };
        match read_minisign_pub(path) {
            Ok(k) => k,
            Err(e) => {
                eprintln!("root key: {e}");
                return ExitCode::from(2);
            }
        }
    } else {
        args[2].clone()
    };

    let manifest = match std::fs::read_to_string(manifest_path) {
        Ok(t) => t,
        Err(e) => {
            eprintln!("cannot read {manifest_path}: {e}");
            return ExitCode::from(2);
        }
    };
    let (release, _rollout) = match parse_manifest_v1(&manifest) {
        Ok(r) => r,
        Err(e) => {
            eprintln!("MANIFEST REJECTED: {e}");
            return ExitCode::FAILURE;
        }
    };

    match check(&release, Path::new(artifact_path), &root_key) {
        Ok(report) => {
            println!("{report}");
            println!("OK: a client with this root key will accept this release.");
            ExitCode::SUCCESS
        }
        Err(e) => {
            eprintln!("REJECTED: {e}");
            eprintln!("A client would refuse this update. Do not publish it.");
            ExitCode::FAILURE
        }
    }
}

/// The client's own checks, in the client's order.
fn check(release: &ReleaseInfo, artifact: &Path, root_key: &str) -> Result<String, String> {
    use std::fmt::Write as _;
    let mut out = String::new();
    let _ = writeln!(out, "version         {}", release.version.as_str());
    let _ = writeln!(out, "artifact        {}", artifact.display());

    verify_digest(artifact, release.digest.as_str())?;
    let _ = writeln!(
        out,
        "digest          {}",
        if release.digest.as_str().trim().is_empty() {
            "(none pinned)"
        } else {
            "OK"
        }
    );

    if root_key.trim().is_empty() {
        let _ = writeln!(
            out,
            "signature       (no root key given - chain not checked)"
        );
        return Ok(out);
    }

    let statement = parse_signing_key_statement(release.signing_key_statement.as_str())?;
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs());
    let mut state = UpdateState::default();
    verify_release_signature(artifact, release, root_key, &mut state, now)?;

    let _ = writeln!(out, "signing key     {}", statement.pubkey_b64);
    let _ = writeln!(out, "generation      {}", statement.generation);
    let remaining = statement.expires_unix.saturating_sub(now);
    let _ = writeln!(
        out,
        "statement       valid, expires in {} days ({})",
        remaining / 86_400,
        statement.expires_unix
    );
    let _ = writeln!(out, "signature       OK (prehashed, root-delegated)");
    Ok(out)
}

/// Reads the base64 key line out of a minisign `.pub` file (line 1 is an
/// untrusted comment).
fn read_minisign_pub(path: &str) -> Result<String, String> {
    let text = std::fs::read_to_string(path).map_err(|e| format!("{path}: {e}"))?;
    text.lines()
        .map(str::trim)
        .find(|l| !l.is_empty() && !l.starts_with("untrusted comment:"))
        .map(str::to_owned)
        .ok_or_else(|| format!("{path}: no key line found"))
}

/// Mints a whole key hierarchy and walks the chain, so the format is
/// demonstrated by something that runs rather than by prose.
fn selftest() -> ExitCode {
    let dir = std::env::temp_dir().join(format!("azul-sign-selftest-{}", std::process::id()));
    if let Err(e) = std::fs::create_dir_all(&dir) {
        eprintln!("cannot create {}: {e}", dir.display());
        return ExitCode::from(2);
    }

    let root = minisign::KeyPair::generate_unencrypted_keypair().expect("root keypair");
    let signing = minisign::KeyPair::generate_unencrypted_keypair().expect("signing keypair");
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs());
    let expires = now + 365 * 86_400;

    // The statement is signed as EXACTLY these bytes — no trailing newline.
    let statement = format!(
        "azul-signing-key-v1|pubkey={}|expires={expires}|generation=1",
        signing.pk.to_base64()
    );
    let statement_sig = minisign::sign(
        Some(&root.pk),
        &root.sk,
        std::io::Cursor::new(statement.as_bytes()),
        Some("azul signing-key statement"),
        None,
    )
    .expect("sign statement")
    .to_string();

    let artifact_path = dir.join("azul-app-2.0.0.bin");
    let artifact_bytes = b"pretend this is a release binary".to_vec();
    std::fs::write(&artifact_path, &artifact_bytes).expect("write artifact");
    let artifact_sig = minisign::sign(
        Some(&signing.pk),
        &signing.sk,
        std::io::Cursor::new(artifact_bytes.as_slice()),
        Some("azul release 2.0.0"),
        None,
    )
    .expect("sign artifact")
    .to_string();

    let digest = sha256_hex(&artifact_bytes);
    let manifest = serde_json::json!({
        "latest": {
            "version": "2.0.0",
            "download_url": "https://example.invalid/azul-app-2.0.0.bin",
            "changelog_md": "https://example.invalid/CHANGELOG.md",
            "digest": format!("sha256:{digest}"),
            "signature": artifact_sig,
            "signing_key_statement": statement,
            "signing_key_statement_sig": statement_sig,
        }
    })
    .to_string();

    println!("root public key : {}", root.pk.to_base64());
    println!("statement       : {statement}");
    println!(
        "manifest        : {}\n",
        dir.join("manifest.json").display()
    );
    drop(std::fs::write(dir.join("manifest.json"), &manifest));

    let (release, _) = match parse_manifest_v1(&manifest) {
        Ok(r) => r,
        Err(e) => {
            eprintln!("selftest manifest did not parse: {e}");
            return ExitCode::FAILURE;
        }
    };
    match check(&release, &artifact_path, &root.pk.to_base64()) {
        Ok(report) => {
            print!("{report}");
            println!("\nselftest OK - the chain this recipe produces verifies.");
        }
        Err(e) => {
            eprintln!("selftest FAILED: {e}");
            return ExitCode::FAILURE;
        }
    }

    // And the negative half: a tampered artifact must be refused, or the
    // "OK" above means nothing.
    std::fs::write(&artifact_path, b"tampered").expect("tamper");
    match check(&release, &artifact_path, &root.pk.to_base64()) {
        Ok(_) => {
            eprintln!("selftest FAILED: a tampered artifact was ACCEPTED");
            return ExitCode::FAILURE;
        }
        Err(e) => println!("tampered artifact correctly refused: {e}"),
    }

    drop(std::fs::remove_dir_all(&dir));
    ExitCode::SUCCESS
}

fn sha256_hex(bytes: &[u8]) -> String {
    use sha2::Digest as _;
    use std::fmt::Write as _;
    sha2::Sha256::digest(bytes)
        .iter()
        .fold(String::with_capacity(64), |mut out, b| {
            let _ = write!(out, "{b:02x}");
            out
        })
}
