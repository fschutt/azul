//! Throwaway certificate authorities and a local TLS server, for tests only.
//!
//! For the tests of what a TLS connection trusts (layout's HTTP client, the dll's iroh relays
//! and database sync, the apps' mail, storage and map clients). Nothing here is used by an app:
//! the keys are made at run time, the certificates live for the test only.
//!
//! A [`TestCa`] is an Ed25519 root (`CN=<name>`); [`TestCa::server_for_loopback`] issues a
//! certificate for 127.0.0.1 under it, and [`serve_tls`] answers TLS on a loopback port with it,
//! telling the test what each connection did: the request line it read after the handshake, or
//! why the handshake failed (the client refused the certificate). [`serve_tls_with`] speaks
//! the opening of the mail protocols instead, so an IMAP or SMTP client gets to its handshake.

use std::{
    io::{BufRead, BufReader, Write},
    net::{TcpListener, TcpStream},
    sync::{mpsc, Arc},
};

use ed25519_dalek::{Signer, SigningKey};

/// One DER element: `tag`, its length, `body`.
fn tlv(tag: u8, body: &[u8]) -> Vec<u8> {
    let mut out = vec![tag];
    let len = body.len();
    match u8::try_from(len) {
        // the short form: the length itself, below 128
        Ok(short) if short < 0x80 => out.push(short),
        _ => {
            let bytes = len.to_be_bytes();
            let first = bytes.iter().position(|b| *b != 0).unwrap_or(bytes.len() - 1);
            // 0x80 and how many length bytes follow
            let count = u8::try_from(bytes.len() - first).expect("a usize has at most 8 bytes");
            out.push(0x80 | count);
            out.extend_from_slice(&bytes[first..]);
        }
    }
    out.extend_from_slice(body);
    out
}

fn seq(parts: &[Vec<u8>]) -> Vec<u8> {
    tlv(0x30, &parts.concat())
}

/// The `AlgorithmIdentifier` of Ed25519 (1.3.101.112, no parameters).
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

/// A throwaway root certificate authority.
#[derive(Debug)]
pub struct TestCa {
    name: String,
    key: SigningKey,
    /// The root certificate, DER.
    pub cert_der: Vec<u8>,
    /// The root certificate, PEM: what an app pins.
    pub cert_pem: String,
}

/// The roots of `cas`, one PEM entry each: what a test pins or distrusts.
#[must_use]
pub fn pems(cas: &[&TestCa]) -> azul_css::StringVec {
    azul_css::StringVec::from_vec(
        cas.iter()
            .map(|ca| azul_css::AzString::from(ca.cert_pem.clone()))
            .collect(),
    )
}

/// A server's certificate chain and its private key (PKCS#8, DER).
#[derive(Debug)]
pub struct ServerCert {
    pub chain: Vec<Vec<u8>>,
    pub key_pkcs8: Vec<u8>,
}

impl TestCa {
    /// A new root named `name` (`CN=<name>`), with a key made now.
    #[must_use]
    pub fn new(name: &str) -> Self {
        let key = SigningKey::from_bytes(&fresh_bytes());
        let basic_constraints = extension(&[0x55, 0x1d, 0x13], true, &seq(&[tlv(0x01, &[0xff])]));
        let key_usage = extension(&[0x55, 0x1d, 0x0f], true, &tlv(0x03, &[0x01, 0x06]));
        let cert_der = certificate(name, &key, name, &key, &[basic_constraints, key_usage]);
        Self {
            name: name.to_string(),
            cert_pem: crate::tls::pem_certificate(&cert_der),
            cert_der,
            key,
        }
    }

    /// A certificate for the loopback address 127.0.0.1 and the name `localhost` under this
    /// root.
    #[must_use]
    pub fn server_for_loopback(&self) -> ServerCert {
        let key = SigningKey::from_bytes(&fresh_bytes());
        let names = seq(&[tlv(0x87, &[127, 0, 0, 1]), tlv(0x82, b"localhost")]);
        let san = extension(&[0x55, 0x1d, 0x11], false, &names);
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

/// What a server of [`serve_tls_with`] says around its handshake.
#[derive(Debug, Clone, Copy)]
pub enum Protocol {
    /// HTTP: the request line is read and answered `200 ok` (what [`serve_tls`] does).
    Http,
    /// A protocol whose server speaks first once TLS is up (IMAP's `* OK`, SMTP's `220` on
    /// the implicit-TLS port): the handshake is reported as `Ok("handshake")`, then this
    /// greeting is written and the connection closed.
    Greeting(&'static str),
    /// SMTP with STARTTLS: a plain-text greeting, an EHLO answer offering STARTTLS, the upgrade;
    /// the handshake is reported as `Ok("starttls")`, the EHLO after it answered, then the
    /// connection closed.
    SmtpStartTls,
}

/// A TLS server on a loopback port with `cert`: every connection completes the handshake (or
/// not) and, when it did, its first line is read and answered `200 ok`. The port, and what
/// each connection did.
#[must_use]
pub fn serve_tls(cert: ServerCert) -> (u16, mpsc::Receiver<Seen>) {
    serve_tls_with(cert, Protocol::Http)
}

/// [`serve_tls`] speaking `protocol` around the handshake.
///
/// # Panics
///
/// When the test certificate does not load or no loopback port can be bound: the test cannot
/// run.
#[must_use]
pub fn serve_tls_with(cert: ServerCert, protocol: Protocol) -> (u16, mpsc::Receiver<Seen>) {
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
            std::thread::spawn(move || answer(stream, &config, protocol, &seen));
        }
    });
    (port, saw)
}

/// One connection of [`serve_tls_with`].
fn answer(
    mut stream: TcpStream,
    config: &Arc<rustls::ServerConfig>,
    protocol: Protocol,
    seen: &mpsc::Sender<Seen>,
) {
    if matches!(protocol, Protocol::SmtpStartTls) && smtp_until_starttls(&mut stream).is_none() {
        drop(seen.send(Err("the client never asked for STARTTLS".to_string())));
        return;
    }
    let Ok(conn) = rustls::ServerConnection::new(config.clone()) else {
        return;
    };
    let mut tls = rustls::StreamOwned::new(conn, stream);
    while tls.conn.is_handshaking() {
        if let Err(e) = tls.conn.complete_io(&mut tls.sock) {
            drop(seen.send(Err(e.to_string())));
            return;
        }
    }
    match protocol {
        Protocol::Http => answer_http(&mut tls, seen),
        Protocol::Greeting(greeting) => {
            drop(seen.send(Ok("handshake".to_string())));
            drop(tls.write_all(greeting.as_bytes()));
        }
        Protocol::SmtpStartTls => {
            drop(seen.send(Ok("starttls".to_string())));
            let mut line = String::new();
            if BufReader::new(&mut tls).read_line(&mut line).unwrap_or(0) > 0 {
                drop(tls.write_all(b"250 test\r\n"));
            }
        }
    }
    drop(tls.flush());
    tls.conn.send_close_notify();
    drop(tls.conn.complete_io(&mut tls.sock));
}

/// The plain-text part of an SMTP session up to the client's STARTTLS, answered `220`;
/// `None` when the client hung up or never asked for it.
fn smtp_until_starttls(stream: &mut TcpStream) -> Option<()> {
    stream.write_all(b"220 test ESMTP\r\n").ok()?;
    // The reader is dropped before the handshake: the client sends nothing after STARTTLS
    // until it has read the 220, so no TLS byte can be in its buffer.
    let mut reader = BufReader::new(stream.try_clone().ok()?);
    let mut line = String::new();
    loop {
        line.clear();
        if reader.read_line(&mut line).ok()? == 0 {
            return None;
        }
        let command = line.trim_end().to_ascii_uppercase();
        if command.starts_with("EHLO") || command.starts_with("HELO") {
            stream.write_all(b"250-test\r\n250 STARTTLS\r\n").ok()?;
        } else if command == "STARTTLS" {
            stream.write_all(b"220 go ahead\r\n").ok()?;
            return Some(());
        } else {
            stream.write_all(b"502 not here\r\n").ok()?;
        }
    }
}

/// The request line of an HTTP request, reported, and a `200 ok` for it.
fn answer_http(
    tls: &mut rustls::StreamOwned<rustls::ServerConnection, TcpStream>,
    seen: &mpsc::Sender<Seen>,
) {
    let mut line = String::new();
    let mut reader = BufReader::new(&mut *tls);
    match reader.read_line(&mut line) {
        Ok(_) => {
            drop(seen.send(Ok(line.trim_end().to_string())));
        }
        Err(e) => {
            drop(seen.send(Err(e.to_string())));
            return;
        }
    }
    loop {
        line.clear();
        if reader.read_line(&mut line).unwrap_or(0) == 0 || line == "\r\n" {
            break;
        }
    }
    drop(tls.write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 2\r\nConnection: close\r\n\r\nok"));
}
