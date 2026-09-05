//! Embedded web-e2e executor.
//!
//! `AZ_BACKEND=<http(s) url> AZ_E2E=<dir|file.json> ./any-azul-app` turns the
//! app binary itself into the e2e test executor against that URL: the
//! brotli-compressed harness bundle (scripts/e2e-web, embedded at build time
//! by `bundle_e2e_web_runner` in build.rs) is extracted to a content-hashed
//! temp dir and run under the first available JS runtime (node / bun / deno,
//! `AZ_E2E_RUNTIME` overrides). Output streams through; the process exits
//! with the harness's cargo-test-style exit code — the exact semantics of
//! the desktop `AZ_BACKEND=headless AZ_E2E=…` runner, only targeting a web
//! server instead of running in-memory.
//!
//! Deliberately dependency-free (TLV + fnv, no serde) so the runner can sit
//! in every build flavor. A future step may replace the JS-runtime spawn
//! with a built-in CDP client ("injection" mode); the env contract stays.

/// FNV-1a 64 over the bundle for the extraction-dir name: same bundle →
/// same dir → second runs skip extraction; changed bundle → fresh dir.
fn fnv1a64(bytes: &[u8]) -> u64 {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for b in bytes {
        h ^= *b as u64;
        h = h.wrapping_mul(0x0000_0100_0000_01B3);
    }
    h
}

fn extract_bundle() -> Result<std::path::PathBuf, String> {
    const BUNDLE: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/e2e_web_bundle.br"));
    let mut tlv = Vec::new();
    brotli_decompressor::BrotliDecompress(&mut &BUNDLE[..], &mut tlv)
        .map_err(|e| format!("bundle decompress failed: {e:?}"))?;

    let dir = std::env::temp_dir().join(format!("azul-e2e-web-{:016x}", fnv1a64(BUNDLE)));
    let marker = dir.join(".complete");
    if marker.exists() {
        return Ok(dir);
    }
    let mut p = 0usize;
    let read_u32 = |b: &[u8], p: &mut usize| -> Result<usize, String> {
        let v = u32::from_le_bytes(
            b.get(*p..*p + 4).ok_or("bundle truncated")?.try_into().unwrap(),
        ) as usize;
        *p += 4;
        Ok(v)
    };
    while p < tlv.len() {
        let plen = read_u32(&tlv, &mut p)?;
        let rel = std::str::from_utf8(tlv.get(p..p + plen).ok_or("bundle truncated")?)
            .map_err(|_| "bundle path not utf-8".to_string())?
            .to_string();
        p += plen;
        let clen = read_u32(&tlv, &mut p)?;
        let content = tlv.get(p..p + clen).ok_or("bundle truncated")?;
        p += clen;
        if rel.contains("..") {
            return Err(format!("refusing bundle path {rel:?}"));
        }
        let dst = dir.join(&rel);
        if let Some(parent) = dst.parent() {
            std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }
        std::fs::write(&dst, content).map_err(|e| e.to_string())?;
    }
    std::fs::write(&marker, b"").map_err(|e| e.to_string())?;
    Ok(dir)
}

/// Probe order mirrors ubiquity; each candidate must answer `--version`.
fn pick_runtime() -> Option<(String, Vec<String>)> {
    if let Ok(rt) = std::env::var("AZ_E2E_RUNTIME") {
        if !rt.is_empty() {
            let extra = if rt.contains("deno") { vec!["run".into(), "-A".into()] } else { vec![] };
            return Some((rt, extra));
        }
    }
    for (cand, extra) in [
        ("node", vec![]),
        ("bun", vec![]),
        ("deno", vec!["run".to_string(), "-A".to_string()]),
    ] {
        let ok = std::process::Command::new(cand)
            .arg("--version")
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status()
            .map(|s| s.success())
            .unwrap_or(false);
        if ok {
            return Some((cand.to_string(), extra));
        }
    }
    None
}

/// If `AZ_E2E` is set and `AZ_BACKEND` is an http(s) URL, run the embedded
/// harness against that URL and EXIT with its status. Returns (does
/// nothing) in every other configuration — including the desktop
/// `AZ_BACKEND=headless` runner, which keeps its existing path.
pub fn maybe_run_and_exit() {
    let e2e = match std::env::var("AZ_E2E") {
        Ok(v) if !v.is_empty() => v,
        _ => return,
    };
    let backend = std::env::var("AZ_BACKEND").unwrap_or_default();
    if !(backend.starts_with("http://") || backend.starts_with("https://")) {
        return;
    }

    let dir = match extract_bundle() {
        Ok(d) => d,
        Err(e) => {
            eprintln!("error: web-e2e bundle extraction failed: {e}");
            std::process::exit(2);
        }
    };
    let Some((rt, extra)) = pick_runtime() else {
        eprintln!(
            "error: no JS runtime found for the web-e2e executor — install node, bun, or \
             deno (or set AZ_E2E_RUNTIME to a binary path)."
        );
        std::process::exit(2);
    };
    let run_mjs = dir.join("run.mjs");
    eprintln!(
        "[azul-e2e] executing embedded web harness via {rt}: AZ_BACKEND={backend} AZ_E2E={e2e}"
    );
    // AZ_BACKEND / AZ_E2E pass through the inherited environment; run.mjs
    // reads them (desktop-parity invocation). Node self-re-execs with
    // --experimental-websocket where needed.
    let status = std::process::Command::new(&rt)
        .args(&extra)
        .arg(&run_mjs)
        .status();
    match status {
        Ok(s) => std::process::exit(s.code().unwrap_or(2)),
        Err(e) => {
            eprintln!("error: failed to spawn {rt}: {e}");
            std::process::exit(2);
        }
    }
}
