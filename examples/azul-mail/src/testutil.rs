//! Test helpers shared by the modules' tests.

use std::{
    path::PathBuf,
    sync::atomic::{AtomicU32, Ordering},
};

/// A folder of its own under the system's temporary folder, removed when dropped.
pub struct TempDir(pub PathBuf);

impl TempDir {
    pub fn new(what: &str) -> TempDir {
        static NEXT: AtomicU32 = AtomicU32::new(0);
        let path = std::env::temp_dir().join(format!(
            "azmail-{what}-test-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        let _ = std::fs::remove_dir_all(&path);
        std::fs::create_dir_all(&path).unwrap();
        TempDir(path)
    }

    /// The folder as the root of a drive of its own (no manifest): the AzMail folder or an
    /// account's folder the tests write through `MailStore`.
    pub fn folder(&self) -> crate::store::DriveFolder {
        crate::store::DriveFolder::outside(self.0.clone())
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

// ==== a one-connection SMTP server on this computer, for the clients' tests ====

/// What the test sink offers and answers. `Default`: the name and nothing else after EHLO,
/// every command taken.
#[derive(Debug, Clone, Default)]
pub struct SinkScript {
    /// The EHLO lines after the server's name (`8BITMIME`, `AUTH PLAIN LOGIN`, `STARTTLS`).
    pub ehlo: Vec<String>,
    /// The user name and the password / token a sign-in must give; `None`: every sign-in is
    /// refused.
    pub accept: Option<(String, String)>,
    /// The answer to `RCPT TO:<address>` by address (lower case); others get 250.
    pub rcpt: Vec<(String, String)>,
    /// The answer to the end of the data; `None`: `250 2.0.0 queued`.
    pub data_reply: Option<String>,
}

/// What the sink saw.
#[derive(Debug, Default)]
pub struct SinkSession {
    /// Every command line as it came (an AUTH's continuation lines included).
    pub commands: Vec<String>,
    /// The message, the dot-stuffing undone.
    pub message: String,
    /// Each sign-in: the mechanism, the user name and the secret it carried.
    pub sign_ins: Vec<(String, String, String)>,
}

/// Starts the sink on a free port of 127.0.0.1 for one connection; the session comes through
/// the channel when the client has gone.
pub fn spawn_smtp_sink(script: SinkScript) -> (u16, std::sync::mpsc::Receiver<SinkSession>) {
    use std::io::{BufRead, BufReader, Write};

    use base64::Engine;

    /// One line without its line end, recorded; `None` when the client has gone.
    fn read_line(
        reader: &mut BufReader<std::net::TcpStream>,
        session: &mut SinkSession,
    ) -> Option<String> {
        let mut line = String::new();
        if reader.read_line(&mut line).unwrap_or(0) == 0 {
            return None;
        }
        let line = line.trim_end_matches(['\r', '\n']).to_string();
        session.commands.push(line.clone());
        Some(line)
    }

    fn decode(text: &str) -> String {
        base64::engine::general_purpose::STANDARD
            .decode(text.trim())
            .map(|bytes| String::from_utf8_lossy(&bytes).into_owned())
            .unwrap_or_default()
    }

    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let Ok((stream, _)) = listener.accept() else {
            return;
        };
        let _ = stream.set_read_timeout(Some(std::time::Duration::from_secs(10)));
        let mut out = stream.try_clone().unwrap();
        let mut reader = BufReader::new(stream);
        let mut session = SinkSession::default();
        let _ = out.write_all(b"220 azmail-test-sink ESMTP\r\n");
        while let Some(line) = read_line(&mut reader, &mut session) {
            let upper = line.to_ascii_uppercase();
            let answer: String = if upper.starts_with("EHLO") {
                let mut lines = vec![String::from("azmail-test-sink")];
                lines.extend(script.ehlo.iter().cloned());
                let last = lines.len() - 1;
                lines
                    .iter()
                    .enumerate()
                    .map(|(i, l)| format!("250{}{l}\r\n", if i == last { ' ' } else { '-' }))
                    .collect()
            } else if upper.starts_with("AUTH ") {
                let mut words = line.split_whitespace().skip(1);
                let mechanism = words.next().unwrap_or("").to_ascii_uppercase();
                let initial = words.next().map(str::to_string);
                let (user, secret) = match mechanism.as_str() {
                    "PLAIN" => {
                        let response = match initial {
                            Some(response) => response,
                            None => {
                                let _ = out.write_all(b"334 \r\n");
                                read_line(&mut reader, &mut session).unwrap_or_default()
                            }
                        };
                        let decoded = decode(&response);
                        let mut parts = decoded.split('\0').skip(1);
                        (
                            parts.next().unwrap_or("").to_string(),
                            parts.next().unwrap_or("").to_string(),
                        )
                    }
                    "LOGIN" => {
                        let _ = out.write_all(b"334 VXNlcm5hbWU6\r\n");
                        let user =
                            decode(&read_line(&mut reader, &mut session).unwrap_or_default());
                        let _ = out.write_all(b"334 UGFzc3dvcmQ6\r\n");
                        let secret =
                            decode(&read_line(&mut reader, &mut session).unwrap_or_default());
                        (user, secret)
                    }
                    "XOAUTH2" => {
                        let decoded = decode(&initial.unwrap_or_default());
                        let field = |name: &str| {
                            decoded
                                .split('\x01')
                                .find_map(|f| f.strip_prefix(name))
                                .unwrap_or("")
                                .to_string()
                        };
                        (field("user="), field("auth=Bearer "))
                    }
                    _ => (String::new(), String::new()),
                };
                let taken = script
                    .accept
                    .as_ref()
                    .is_some_and(|(u, s)| *u == user && *s == secret);
                session.sign_ins.push((mechanism.clone(), user, secret));
                if taken {
                    String::from("235 2.7.0 Authentication successful\r\n")
                } else if mechanism == "XOAUTH2" {
                    // Google's way: an error report as a challenge, then 535 after the
                    // client's empty line.
                    let _ = out.write_all(b"334 eyJzdGF0dXMiOiI0MDEifQ==\r\n");
                    let _ = read_line(&mut reader, &mut session);
                    String::from("535 5.7.8 Username and Password not accepted\r\n")
                } else {
                    String::from("535 5.7.8 Username and Password not accepted\r\n")
                }
            } else if upper.starts_with("RCPT TO:") {
                let address = line[8..]
                    .trim()
                    .trim_start_matches('<')
                    .split('>')
                    .next()
                    .unwrap_or("")
                    .to_ascii_lowercase();
                match script.rcpt.iter().find(|(a, _)| *a == address) {
                    Some((_, reply)) => format!("{reply}\r\n"),
                    None => String::from("250 2.1.5 OK\r\n"),
                }
            } else if upper == "DATA" {
                let _ = out.write_all(b"354 go ahead\r\n");
                loop {
                    let mut l = String::new();
                    if reader.read_line(&mut l).unwrap_or(0) == 0 || l == ".\r\n" {
                        break;
                    }
                    session.message.push_str(l.strip_prefix('.').unwrap_or(&l));
                }
                match &script.data_reply {
                    Some(reply) => format!("{reply}\r\n"),
                    None => String::from("250 2.0.0 queued\r\n"),
                }
            } else if upper == "QUIT" {
                let _ = out.write_all(b"221 2.0.0 bye\r\n");
                break;
            } else {
                String::from("250 2.0.0 OK\r\n")
            };
            let _ = out.write_all(answer.as_bytes());
        }
        let _ = tx.send(session);
    });
    (port, rx)
}
