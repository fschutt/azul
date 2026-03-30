//! Minimal HTTPS test using pure-Rust TLS (no ring, no C code).
//!
//! Uses: ureq + rustls + rustls-rustcrypto + webpki-roots
//! Cross-compilable to Linux: `cargo build -p https-test --target x86_64-unknown-linux-gnu`

use std::sync::Arc;
use std::time::Duration;

fn main() {
    let url = std::env::args().nth(1).unwrap_or_else(|| "https://www.google.com".into());
    println!("[https-test] Fetching: {url}");

    let tls_config = ureq::tls::TlsConfig::builder()
        .provider(ureq::tls::TlsProvider::Rustls)
        .unversioned_rustls_crypto_provider(Arc::new(rustls_rustcrypto::provider()))
        .root_certs(ureq::tls::RootCerts::WebPki)
        .build();

    let agent = ureq::Agent::config_builder()
        .tls_config(tls_config)
        .timeout_global(Some(Duration::from_secs(30)))
        .http_status_as_error(false)
        .build()
        .new_agent();

    match agent.get(&url).call() {
        Ok(resp) => {
            let status = resp.status().as_u16();
            let body = resp.into_body().read_to_string()
                .unwrap_or_else(|e| format!("<read error: {e}>"));
            println!("[https-test] HTTP {status}, body {} bytes", body.len());
            println!("[https-test] Preview:\n{}", &body[..body.len().min(300)]);
        }
        Err(e) => {
            eprintln!("[https-test] FAILED: {e}");
            eprintln!("[https-test] Debug: {e:?}");
            std::process::exit(1);
        }
    }
}
