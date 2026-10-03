//! The real mail server: IMAP over TLS (rustls with the pure-Rust RustCrypto provider and the
//! Mozilla roots, as azul's own HTTP client has it) or, to a test server on this computer, over
//! plain TCP. The `imap` crate speaks the protocol over that stream. Everything here blocks: it
//! runs on the sync's azul Thread, never in a callback.
//!
//! Folders are opened with `EXAMINE` (read-only `SELECT`) and bodies fetched with
//! `BODY.PEEK[]`, so syncing changes nothing on the server: no flag, no `\Recent`.

use std::{
    io::{Read, Write},
    net::{TcpStream, ToSocketAddrs},
    path::Path,
    sync::Arc,
    time::Duration,
};

use crate::{
    account::{Account, Secret, Security},
    auth::{self, AuthMethod, ServerCaps},
    folders::ServerMailbox,
    sync::{uid_set, MailSource, MessageMeta, Selected, SyncError},
};

/// How long connecting may take.
const CONNECT_TIMEOUT: Duration = Duration::from_secs(20);
/// How long one read or write may take (a big batch of bodies streams in the meantime).
const IO_TIMEOUT: Duration = Duration::from_secs(120);

/// The connection under the IMAP session.
pub enum Transport {
    Tls(Box<rustls::StreamOwned<rustls::ClientConnection, TcpStream>>),
    Plain(TcpStream),
}

impl Read for Transport {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        match self {
            Transport::Tls(stream) => stream.read(buf),
            Transport::Plain(stream) => stream.read(buf),
        }
    }
}

impl Write for Transport {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        match self {
            Transport::Tls(stream) => stream.write(buf),
            Transport::Plain(stream) => stream.write(buf),
        }
    }

    fn flush(&mut self) -> std::io::Result<()> {
        match self {
            Transport::Tls(stream) => stream.flush(),
            Transport::Plain(stream) => stream.flush(),
        }
    }
}

/// A SASL response (`AUTHENTICATE PLAIN` / `XOAUTH2`) for the `imap` crate. It holds the
/// secret: no `Debug`, and its bytes are overwritten when it is dropped.
struct Sasl(Vec<u8>);

impl imap::Authenticator for Sasl {
    type Response = Vec<u8>;

    fn process(&self, challenge: &[u8]) -> Vec<u8> {
        auth::answer(challenge, &self.0)
    }
}

impl Drop for Sasl {
    fn drop(&mut self) {
        self.0.fill(0);
        std::hint::black_box(&self.0);
    }
}

/// A signed-in IMAP session.
pub struct ImapSource {
    session: imap::Session<Transport>,
}

impl ImapSource {
    /// Connects to the account's IMAP server and signs in. `extra_ca` is a PEM certificate to
    /// trust besides the Mozilla roots (the test server's; headless runs only).
    pub fn connect(
        account: &Account,
        secret: &Secret,
        extra_ca: Option<&Path>,
    ) -> Result<ImapSource, SyncError> {
        let host = account
            .imap
            .host
            .trim()
            .trim_start_matches('[')
            .trim_end_matches(']');
        let tcp = connect_tcp(host, account.imap.port)?;
        let transport = match account.security {
            Security::Tls => Transport::Tls(Box::new(tls_stream(host, tcp, extra_ca)?)),
            Security::Plain => Transport::Plain(tcp),
        };
        let mut client = imap::Client::new(transport);
        // With TLS, the handshake happens here, on the first read.
        client
            .read_greeting()
            .map_err(|e| SyncError::Connect(format!("{host}:{}: {e}", account.imap.port)))?;
        let capabilities = client
            .capabilities()
            .map_err(|e| SyncError::Protocol(format!("CAPABILITY: {e}")))?;
        let caps = ServerCaps {
            auth_plain: capabilities.has_str("AUTH=PLAIN"),
            auth_xoauth2: capabilities.has_str("AUTH=XOAUTH2"),
            login_disabled: capabilities.has_str("LOGINDISABLED"),
        };
        drop(capabilities);
        let user = account.username.as_str();
        let session = match auth::choose(account.auth, caps).map_err(SyncError::Auth)? {
            AuthMethod::Plain => {
                client.authenticate("PLAIN", &Sasl(auth::plain_response(user, secret.expose())))
            }
            AuthMethod::Xoauth2 => client.authenticate(
                "XOAUTH2",
                &Sasl(auth::xoauth2_response(user, secret.expose())),
            ),
            AuthMethod::Login => client.login(user, secret.expose()),
        }
        .map_err(|(e, _client)| SyncError::Auth(e.to_string()))?;
        Ok(ImapSource { session })
    }

    /// Says goodbye to the server (errors do not matter any more).
    pub fn logout(mut self) {
        let _ = self.session.logout();
    }
}

/// A TCP connection to `host:port`, trying each address in turn.
fn connect_tcp(host: &str, port: u16) -> Result<TcpStream, SyncError> {
    let addresses = (host, port)
        .to_socket_addrs()
        .map_err(|e| SyncError::Connect(format!("{host}: {e}")))?;
    let mut last = None;
    for address in addresses {
        match TcpStream::connect_timeout(&address, CONNECT_TIMEOUT) {
            Ok(tcp) => {
                let _ = tcp.set_read_timeout(Some(IO_TIMEOUT));
                let _ = tcp.set_write_timeout(Some(IO_TIMEOUT));
                return Ok(tcp);
            }
            Err(e) => last = Some(e),
        }
    }
    Err(SyncError::Connect(match last {
        Some(e) => format!("{host}:{port}: {e}"),
        None => format!("{host}: no address"),
    }))
}

/// The TLS stream to `host` over `tcp`, verified against the Mozilla roots (and `extra_ca`).
fn tls_stream(
    host: &str,
    tcp: TcpStream,
    extra_ca: Option<&Path>,
) -> Result<rustls::StreamOwned<rustls::ClientConnection, TcpStream>, SyncError> {
    let tls_error = |e: String| SyncError::Connect(format!("TLS to {host}: {e}"));
    let mut roots = rustls::RootCertStore::empty();
    roots.extend(webpki_roots::TLS_SERVER_ROOTS.iter().cloned());
    if let Some(path) = extra_ca {
        use rustls::pki_types::{pem::PemObject, CertificateDer};
        let cert = CertificateDer::from_pem_file(path)
            .map_err(|e| tls_error(format!("{}: {e}", path.display())))?;
        roots
            .add(cert)
            .map_err(|e| tls_error(format!("{}: {e}", path.display())))?;
    }
    let config =
        rustls::ClientConfig::builder_with_provider(Arc::new(rustls_rustcrypto::provider()))
            .with_safe_default_protocol_versions()
            .map_err(|e| tls_error(e.to_string()))?
            .with_root_certificates(roots)
            .with_no_client_auth();
    let name = rustls::pki_types::ServerName::try_from(host.to_string())
        .map_err(|e| tls_error(e.to_string()))?;
    let connection = rustls::ClientConnection::new(Arc::new(config), name)
        .map_err(|e| tls_error(e.to_string()))?;
    Ok(rustls::StreamOwned::new(connection, tcp))
}

/// A LIST attribute as the server wrote it (`\Junk`, `\HasChildren`, ...).
fn attribute_text(attribute: &imap_proto::NameAttribute<'_>) -> String {
    use imap_proto::NameAttribute as A;
    match attribute {
        A::NoInferiors => String::from("\\Noinferiors"),
        A::NoSelect => String::from("\\Noselect"),
        A::Marked => String::from("\\Marked"),
        A::Unmarked => String::from("\\Unmarked"),
        A::All => String::from("\\All"),
        A::Archive => String::from("\\Archive"),
        A::Drafts => String::from("\\Drafts"),
        A::Flagged => String::from("\\Flagged"),
        A::Junk => String::from("\\Junk"),
        A::Sent => String::from("\\Sent"),
        A::Trash => String::from("\\Trash"),
        A::Extension(name) => name.to_string(),
        _ => String::new(),
    }
}

fn protocol(command: &'static str) -> impl Fn(imap::Error) -> SyncError {
    move |e| SyncError::Protocol(format!("{command}: {e}"))
}

impl MailSource for ImapSource {
    fn list(&mut self) -> Result<Vec<ServerMailbox>, SyncError> {
        let names = self
            .session
            .list(Some(""), Some("*"))
            .map_err(protocol("LIST"))?;
        Ok(names
            .iter()
            .map(|name| ServerMailbox {
                name: name.name().to_string(),
                delimiter: name.delimiter().map(str::to_string),
                attributes: name.attributes().iter().map(attribute_text).collect(),
            })
            .collect())
    }

    fn select(&mut self, server_name: &str) -> Result<Selected, SyncError> {
        let mailbox = self
            .session
            .examine(server_name)
            .map_err(protocol("EXAMINE"))?;
        Ok(Selected {
            uidvalidity: mailbox.uid_validity.unwrap_or(0),
            uid_next: mailbox.uid_next,
            exists: mailbox.exists,
        })
    }

    fn search_from(&mut self, first: u32) -> Result<Vec<u32>, SyncError> {
        let found = self
            .session
            .uid_search(format!("UID {first}:*"))
            .map_err(protocol("UID SEARCH"))?;
        Ok(found.into_iter().collect())
    }

    fn fetch_meta(&mut self, uids: &[u32]) -> Result<Vec<MessageMeta>, SyncError> {
        if uids.is_empty() {
            return Ok(Vec::new());
        }
        let fetches = self
            .session
            .uid_fetch(uid_set(uids), "(UID FLAGS RFC822.SIZE INTERNALDATE)")
            .map_err(protocol("UID FETCH"))?;
        Ok(fetches
            .iter()
            .filter_map(|fetch| {
                Some(MessageMeta {
                    uid: fetch.uid?,
                    size: fetch.size.map(u64::from),
                    flags: fetch.flags().iter().map(|flag| flag.to_string()).collect(),
                    internal_date: fetch.internal_date().map(|date| date.timestamp()),
                })
            })
            .collect())
    }

    fn fetch_bodies(&mut self, uids: &[u32]) -> Result<Vec<(u32, Vec<u8>)>, SyncError> {
        if uids.is_empty() {
            return Ok(Vec::new());
        }
        let fetches = self
            .session
            .uid_fetch(uid_set(uids), "(UID BODY.PEEK[])")
            .map_err(protocol("UID FETCH"))?;
        Ok(fetches
            .iter()
            .filter_map(|fetch| Some((fetch.uid?, fetch.body()?.to_vec())))
            .collect())
    }
}
