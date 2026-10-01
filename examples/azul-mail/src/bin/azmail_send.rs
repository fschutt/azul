//! Sends one mail through AzMail's send path (`azmail::send`), or retries the outbox, from the
//! command line: what `scripts/azmail_send_test.py` drives against `scripts/azmail_smtp_sink.py`.
//!
//! ```text
//! azmail-send --data <AzMail folder> --account <id> --from <addr> --to <addr> [--to ...]
//!     [--cc <addr>]... [--bcc <addr>]... [--subject <text>] [--text <text>] [--html <html>]
//!     [--attach <file>[=<mime type>]]... [--in-reply-to <id>] [--reference <id>]...
//!     [--smtp <host:port> | --direct] [--direct-port <port>] [--tls opportunistic|required|off]
//!     [--ca <pem file>] [--helo <name>] [--ignore-policy]
//!     [--dkim-domain <d> --dkim-selector <s> --dkim-key <pem file>] [--save-settings]
//! azmail-send --data <dir> --account <id> --retry [--force] [route options as above]
//! ```
//!
//! Without route options the account's `sending.json` is used. Prints one line per mail:
//! `AZMAIL_SEND sent <message-id>`, `AZMAIL_SEND queued <reason>` or
//! `AZMAIL_SEND failed <reason>` (with `--retry`: `AZMAIL_SEND <id> <status> ...`). Exit code 0
//! when everything was sent, 2 when something is queued, 1 when something failed, 3 for a usage
//! error.

use std::path::PathBuf;

use azmail::send::{
    retry_outbox, send_mail, Attachment, DkimSettings, OutgoingMail, SendRoute, SendSettings,
    SendStatus, TlsPolicy,
};

fn usage(why: &str) -> ! {
    eprintln!("azmail-send: {why}");
    eprintln!(
        "usage: azmail-send --data <dir> --account <id> --from <addr> --to <addr> [--cc ..] \
         [--bcc ..] [--subject ..] [--text ..] [--html ..] [--attach file[=mime]] \
         [--in-reply-to id] [--reference id] [--smtp host:port | --direct] [--direct-port n] \
         [--tls opportunistic|required|off] [--ca pem] [--helo name] [--ignore-policy] \
         [--dkim-domain d --dkim-selector s --dkim-key pem] [--save-settings] | --retry [--force]"
    );
    std::process::exit(3);
}

fn main() {
    let mut args = std::env::args().skip(1);
    let mut data: Option<PathBuf> = None;
    let mut account: Option<String> = None;
    let mut mail = OutgoingMail::default();
    let mut route: Option<SendRoute> = None;
    let mut tls: Option<TlsPolicy> = None;
    let mut ca: Option<PathBuf> = None;
    let mut helo: Option<String> = None;
    let mut direct_port: Option<u16> = None;
    let mut ignore_policy = false;
    let mut dkim = DkimSettings::default();
    let mut retry = false;
    let mut force = false;
    let mut save = false;

    while let Some(arg) = args.next() {
        let mut value = |name: &str| -> String {
            args.next()
                .unwrap_or_else(|| usage(&format!("{name} needs a value")))
        };
        match arg.as_str() {
            "--data" => data = Some(PathBuf::from(value("--data"))),
            "--account" => account = Some(value("--account")),
            "--from" => mail.from = value("--from"),
            "--to" => mail.to.push(value("--to")),
            "--cc" => mail.cc.push(value("--cc")),
            "--bcc" => mail.bcc.push(value("--bcc")),
            "--subject" => mail.subject = value("--subject"),
            "--text" => mail.text_body = value("--text"),
            "--html" => mail.html_body = Some(value("--html")),
            "--in-reply-to" => mail.in_reply_to = Some(value("--in-reply-to")),
            "--reference" => mail.references.push(value("--reference")),
            "--attach" => {
                let spec = value("--attach");
                let (path, mime) = match spec.rsplit_once('=') {
                    Some((path, mime)) if mime.contains('/') => {
                        (path.to_string(), mime.to_string())
                    }
                    _ => (spec.clone(), "application/octet-stream".to_string()),
                };
                let bytes = std::fs::read(&path)
                    .unwrap_or_else(|e| usage(&format!("cannot read {path}: {e}")));
                let file_name = std::path::Path::new(&path)
                    .file_name()
                    .map(|n| n.to_string_lossy().into_owned())
                    .unwrap_or_else(|| "attachment".to_string());
                mail.attachments.push(Attachment {
                    file_name,
                    mime_type: mime,
                    bytes,
                });
            }
            "--smtp" => {
                let spec = value("--smtp");
                let Some((host, port)) = spec.rsplit_once(':') else {
                    usage("--smtp wants host:port");
                };
                let port: u16 = port
                    .parse()
                    .unwrap_or_else(|_| usage("--smtp wants a port number"));
                route = Some(SendRoute::Smtp {
                    host: host
                        .trim_start_matches('[')
                        .trim_end_matches(']')
                        .to_string(),
                    port,
                });
            }
            "--direct" => route = Some(SendRoute::Direct),
            "--direct-port" => {
                direct_port = Some(
                    value("--direct-port")
                        .parse()
                        .unwrap_or_else(|_| usage("--direct-port wants a port number")),
                )
            }
            "--tls" => {
                tls = Some(match value("--tls").as_str() {
                    "opportunistic" => TlsPolicy::Opportunistic,
                    "required" => TlsPolicy::Required,
                    "off" => TlsPolicy::Off,
                    other => usage(&format!("--tls {other}: opportunistic, required or off")),
                })
            }
            "--ca" => ca = Some(PathBuf::from(value("--ca"))),
            "--helo" => helo = Some(value("--helo")),
            "--ignore-policy" => ignore_policy = true,
            "--dkim-domain" => dkim.domain = value("--dkim-domain"),
            "--dkim-selector" => dkim.selector = value("--dkim-selector"),
            "--dkim-key" => dkim.key_file = Some(PathBuf::from(value("--dkim-key"))),
            "--retry" => retry = true,
            "--force" => force = true,
            "--save-settings" => save = true,
            "-h" | "--help" => usage("help"),
            other => usage(&format!("unknown argument {other}")),
        }
    }

    let data = data.unwrap_or_else(|| usage("--data is missing"));
    let account = account.unwrap_or_else(|| usage("--account is missing"));
    let mut settings = SendSettings::load(&data, &account);
    if let Some(route) = route {
        settings.route = route;
    }
    if let Some(tls) = tls {
        settings.tls = tls;
    }
    if let Some(ca) = ca {
        settings.extra_ca_file = Some(ca);
    }
    if let Some(helo) = helo {
        settings.helo_name = helo;
    }
    if let Some(port) = direct_port {
        settings.direct_port = port;
    }
    if ignore_policy {
        settings.ignore_policy = true;
    }
    if !dkim.domain.is_empty() || dkim.key_file.is_some() {
        if dkim.selector.is_empty() {
            dkim.selector = "azmail".to_string();
        }
        settings.dkim = Some(dkim);
    }
    if save {
        match settings.save(&data, &account) {
            Ok(path) => println!("AZMAIL_SETTINGS_SAVED {}", path.display()),
            Err(e) => {
                eprintln!("azmail-send: cannot save the settings: {e}");
                std::process::exit(1);
            }
        }
    }

    let statuses: Vec<(Option<String>, SendStatus)> = if retry {
        retry_outbox(&data, &account, &settings, force)
            .into_iter()
            .map(|(id, status)| (Some(id), status))
            .collect()
    } else {
        if mail.from.is_empty() || (mail.to.is_empty() && mail.cc.is_empty() && mail.bcc.is_empty())
        {
            usage("--from and at least one --to, --cc or --bcc are needed");
        }
        vec![(None, send_mail(&data, &account, &settings, &mail))]
    };

    let mut code = 0;
    for (id, status) in statuses {
        let prefix = match id {
            Some(id) => format!("AZMAIL_SEND {id}"),
            None => "AZMAIL_SEND".to_string(),
        };
        match status {
            SendStatus::Sent { message_id } => println!("{prefix} sent {message_id}"),
            SendStatus::Queued { reason } => {
                println!("{prefix} queued {reason}");
                if code == 0 {
                    code = 2;
                }
            }
            SendStatus::Failed { reason } => {
                println!("{prefix} failed {reason}");
                code = 1;
            }
        }
    }
    std::process::exit(code);
}
