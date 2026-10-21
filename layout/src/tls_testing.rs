//! Throwaway certificate authorities and a local TLS server, for the tests of what a TLS
//! connection trusts (layout's HTTP client, the dll's iroh relays). Nothing here is used by an
//! app: the keys are made at run time, the certificates live for the test only.
//!
//! A [`TestCa`] is an Ed25519 root (`CN=<name>`); [`TestCa::server_for_loopback`] issues a
//! certificate for 127.0.0.1 under it, and [`serve_tls`] answers TLS on a loopback port with it,
//! telling the test what each connection did: the request line it read after the handshake, or
//! why the handshake failed (the client refused the certificate).

use std::{
    io::{BufRead, BufReader, Write},
    net::TcpListener,
    sync::{mpsc, Arc},
};

use ed25519_dalek::{Signer, SigningKey};

/// One DER element: `tag`, its length, `body`.
fn tlv(tag: u8, body: &[u8]) -> Vec<u8> {
    let mut out = vec![tag];
    let len = body.len();
    if len < 0x80 {
        out.push(len as u8);
    } else {
        let bytes = len.to_be_bytes();
        let first = bytes.iter().position(|b| *b != 0).unwrap_or(bytes.len() - 1);
        out.push(0x80 | (bytes.len() - first) as u8);
        out.extend_from_slice(&bytes[first..]);
    }
    out.extend_from_slice(body);
    out
}

fn seq(parts: &[Vec<u8>]) -> Vec<u8> {
    tlv(0x30, &parts.concat())
}

/// AlgorithmIdentifier of Ed25519 (1.3.101.112, no parameters).
fn ed25519_algorithm() -> Vec<u8> {
    seq(&[tlv(0x06, &[0x2b, 0x65, 0x70])])
}

/// `CN=<name>`.
fn name(common_name: &str) -> Vec<u8> {
    let attribute = seq(&[tlv(0x06, &[0x55, 0x04, 0x03]), tlv(0x0c, common_name.as_bytes())]);
    seq(&[tlv(0x31, &attribute)])
}

/// One extension: its OID (the body of the OID element), whether it is critical, its value.
fn extension(oid: &[u8], critical: bool, value: &[u8]) -> Vec<u8> {
    let mut parts = vec![tlv(0x06, oid)];
    if critical {
        parts.push(tlv(0x01, &[0xff]));
    }
    parts.push(tlv(0x04, value));
    seq(&parts)
}

/// 32 bytes no other run of the tests shares (no crate of randomness needed for a test key).
fn fresh_bytes() -> [u8; 32] {
    use std::hash::{BuildHasher, Hasher};
    let mut out = [0u8; 32];
    for chunk in out.chunks_mut(8) {
        let mut hasher = std::collections::hash_map::RandomState::new().build_hasher();
        hasher.write_u128(
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map_or(0, |d| d.as_nanos()),
        );
        chunk.copy_from_slice(&hasher.finish().to_le_bytes());
    }
    out
}

/// A certificate signed with `issuer_key`: `subject` named, `key` its public key, `extensions`.
fn certificate(
    issuer: &str,
    issuer_key: &SigningKey,
    subject: &str,
    key: &SigningKey,
    extensions: &[Vec<u8>],
) -> Vec<u8> {
    let mut serial = fresh_bytes()[..8].to_vec();
    serial[0] &= 0x7f;
    serial[0] |= 0x01;
    let validity = seq(&[
        tlv(0x17, b"200101000000Z"),
        tlv(0x17, b"491231235959Z"),
    ]);
    let mut public_key = vec![0u8];
    public_key.extend_from_slice(key.verifying_key().as_bytes());
    let spki = seq(&[ed25519_algorithm(), tlv(0x03, &public_key)]);
    let tbs = seq(&[
        tlv(0xa0, &tlv(0x02, &[0x02])),
        tlv(0x02, &serial),
        ed25519_algorithm(),
        name(issuer),
        validity,
        name(subject),
        spki,
        tlv(0xa3, &seq(extensions)),
    ]);
    let signature = issuer_key.sign(&tbs).to_bytes();
    let mut bits = vec![0u8];
    bits.extend_from_slice(&signature);
    seq(&[tbs, ed25519_algorithm(), tlv(0x03, &bits)])
}

/// `der` as PEM text of `label`.
fn pem(label: &str, der: &[u8]) -> String {
    const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut text = String::new();
    for chunk in der.chunks(3) {
        let n = chunk.iter().enumerate().fold(0u32, |n, (i, b)| n | u32::from(*b) << (16 - 8 * i));
        for i in 0..4 {
            text.push(if i <= chunk.len() {
                ALPHABET[(n >> (18 - 6 * i) & 63) as usize] as char
            } else {
                '='
            });
        }
    }
    let mut out = format!("-----BEGIN {label}-----\n");
    for line in text.as_bytes().chunks(64) {
        out.push_str(core::str::from_utf8(line).unwrap_or_default());
        out.push('\n');
    }
    out.push_str(&format!("-----END {label}-----\n"));
    out
}

/// A throwaway root certificate authority.
pub struct TestCa {
    name: String,
    key: SigningKey,
    /// The root certificate, DER.
    pub cert_der: Vec<u8>,
    /// The root certificate, PEM: what an app pins.
    pub cert_pem: String,
}

/// A server's certificate chain and its private key (PKCS#8, DER).
pub struct ServerCert {
    pub chain: Vec<Vec<u8>>,
    pub key_pkcs8: Vec<u8>,
}

impl TestCa {
    /// A new root named `name` (`CN=<name>`), with a key made now.
    #[must_use]
    pub fn new(name: &str) -> TestCa {
        let key = SigningKey::from_bytes(&fresh_bytes());
        let basic_constraints = extension(&[0x55, 0x1d, 0x13], true, &seq(&[tlv(0x01, &[0xff])]));
        let key_usage = extension(&[0x55, 0x1d, 0x0f], true, &tlv(0x03, &[0x01, 0x06]));
        let cert_der = certificate(name, &key, name, &key, &[basic_constraints, key_usage]);
        TestCa {
            name: name.to_string(),
            cert_pem: pem("CERTIFICATE", &cert_der),
            cert_der,
            key,
        }
    }

    /// A certificate for the loopback address 127.0.0.1 under this root.
    #[must_use]
    pub fn server_for_loopback(&self) -> ServerCert {
        let key = SigningKey::from_bytes(&fresh_bytes());
        let san = extension(&[0x55, 0x1d, 0x11], false, &seq(&[tlv(0x87, &[127, 0, 0, 1])]));
        let server_auth = [0x2b, 0x06, 0x01, 0x05, 0x05, 0x07, 0x03, 0x01];
        let eku = extension(&[0x55, 0x1d, 0x25], false, &seq(&[tlv(0x06, &server_auth)]));
        let leaf = certificate(&self.name, &self.key, "127.0.0.1", &key, &[san, eku]);
        // PKCS#8 v1 of an Ed25519 key: the fixed prefix, then the 32-byte seed.
        let mut key_pkcs8 = vec![
            0x30, 0x2e, 0x02, 0x01, 0x00, 0x30, 0x05, 0x06, 0x03, 0x2b, 0x65, 0x70, 0x04, 0x22,
            0x04, 0x20,
        ];
        key_pkcs8.extend_from_slice(&key.to_bytes());
        ServerCert {
            chain: vec![leaf, self.cert_der.clone()],
            key_pkcs8,
        }
    }
}

/// What a connection to [`serve_tls`] did: the request line read after the handshake, or why
/// the handshake failed.
pub type Seen = Result<String, String>;

/// A TLS server on a loopback port with `cert`: every connection completes the handshake (or
/// not) and, when it did, its first line is read and answered `200 ok`. The port, and what
/// each connection did.
#[must_use]
pub fn serve_tls(cert: ServerCert) -> (u16, mpsc::Receiver<Seen>) {
    let provider = Arc::new(rustls_rustcrypto::provider());
    let chain = cert
        .chain
        .into_iter()
        .map(rustls::pki_types::CertificateDer::from)
        .collect();
    let key = rustls::pki_types::PrivateKeyDer::Pkcs8(cert.key_pkcs8.into());
    let config = rustls::ServerConfig::builder_with_provider(provider)
        .with_safe_default_protocol_versions()
        .expect("the provider's protocol versions")
        .with_no_client_auth()
        .with_single_cert(chain, key)
        .expect("the test certificate loads");
    let config = Arc::new(config);
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind");
    let port = listener.local_addr().expect("addr").port();
    let (seen, saw) = mpsc::channel();
    std::thread::spawn(move || {
        for stream in listener.incoming() {
            let Ok(stream) = stream else { return };
            let config = config.clone();
            let seen = seen.clone();
            std::thread::spawn(move || {
                let Ok(conn) = rustls::ServerConnection::new(config) else {
                    return;
                };
                let mut tls = rustls::StreamOwned::new(conn, stream);
                while tls.conn.is_handshaking() {
                    if let Err(e) = tls.conn.complete_io(&mut tls.sock) {
                        let _ = seen.send(Err(e.to_string()));
                        return;
                    }
                }
                let mut line = String::new();
                let mut reader = BufReader::new(&mut tls);
                match reader.read_line(&mut line) {
                    Ok(_) => {
                        let _ = seen.send(Ok(line.trim_end().to_string()));
                    }
                    Err(e) => {
                        let _ = seen.send(Err(e.to_string()));
                        return;
                    }
                }
                loop {
                    line.clear();
                    if reader.read_line(&mut line).unwrap_or(0) == 0 || line == "\r\n" {
                        break;
                    }
                }
                let _ = tls.write_all(
                    b"HTTP/1.1 200 OK\r\nContent-Length: 2\r\nConnection: close\r\n\r\nok",
                );
                let _ = tls.flush();
                tls.conn.send_close_notify();
                let _ = tls.conn.complete_io(&mut tls.sock);
            });
        }
    });
    (port, saw)
}
