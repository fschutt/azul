//! `azcloud`: the Azlin cloud client on the command line - the end-to-end test of
//! azcloud-kit when it runs against a local stack, and the tool that shows where
//! every endpoint came from (`azcloud config`). See [`USAGE`]. With `--json` every
//! command prints one JSON object (`"ok": false` and the error when it failed);
//! exit code 0 ok, 1 failed, 2 usage.
//!
//! Everything but the transports is azcloud-kit's: the account, the state folder,
//! the settings, the bucket, the sync, the links. This binary brings reqwest for
//! HTTPS and, with the `iroh` feature, the dialer of "S3 over iroh"; their futures
//! run on one tokio runtime while the kit's blocking calls wait on the main thread.

use std::{
    collections::{BTreeMap, BTreeSet},
    io::Read,
    path::{Path, PathBuf},
    process::ExitCode,
    sync::Arc,
};

use anyhow::{anyhow, bail, Context, Result};
use azcloud_api::{https::HttpsTransport, os_dirs, HTTPS_TIMEOUT};
use azcloud_kit::{
    account::{Account, AccountFile, JoinCode},
    drive::TransportFactory,
    settings::{Flags, Settings},
    share,
    state::{write_atomic, StateDir, ACCOUNT_FILE},
    sync::{self, local::hash_file, LocalRoot, SyncOptions},
    transport::{CloudDrive, IrohDialer},
    CloudError,
};
use azul_appkit::azlin_config::{Endpoint, EndpointFlags};
use azul_storage::Transport;
use serde_json::{json, Value};

const USAGE: &str = "\
azcloud - the Azlin cloud client (azul examples/azcloud-api)

usage: azcloud [flags] <command> [arguments]

commands:
  config                         the effective endpoints and folders, and where each value came
                                 from (a flag, the environment, the config file, a built-in default)
  signup [--tier T] [--name N] [--recovery-out F] [--plaintext]
                                 a new drive at the token server (dev servers: POST /v1/drives),
                                 encrypted as it is made: its RECOVERY CODE shows once (or goes
                                 into F, readable by you only); --plaintext makes one like the
                                 drives made before encryption (tests of those)
  invite [--member M] [--out F]  a join code for another device (a member token family of its own;
                                 it holds a secret: --out writes it to a file only you can read)
  join <code> | --code-file F | -   this device joins the drive of a code (- reads stdin)
  status                         this device's drive, credentials and node list (no network)
  encryption                     whether the drive is encrypted and this device keeps its key
  encrypt --yes [--out F]        encrypts the drive from this device: its keys, and its RECOVERY
                                 CODE, shown once (--out writes it to a file only you can read)
  unlock                         the drive key from this device's own wrap in the bucket
  recover <code> | --code-file F | -   this device gets the drive key with the recovery code
  rotate --yes [--out F]         \"I was hacked\": locks the drive down (every other device, key and
                                 link), then a new drive key; the new RECOVERY CODE shows once
  reencrypt                      after rotate: every file into a new object (recommended)
  mail-drop [--cloudflare-account ID --cloudflare-token-file F] [--worker W]
                                 incoming mail for the encrypted drive: its drop key; with your
                                 Cloudflare account and API token, set on your mail Worker
                                 (default azlin-mail-worker; the token goes to Cloudflare only)
  git-remote <remote> <url>      git's remote helper over the drive index (git-remote-azlin
                                 runs it for `git clone azlin::drive://<drive id>`)
                                 (encryption .. git-remote: builds with the feature `encryption`)
  info                           the token server's view of the drive
  refresh                        new credentials and node list now
  transport                      which transport requests take (iroh or https) and why
  up <file> [--key K]            uploads a file (default key uploads/<file name>); an encrypted
                                 drive's files go through the encryption and the drive index
                                 (up, down, ls, rm; sync and share refuse an encrypted drive)
  down <key> <file>              downloads an object (parallel ranges when it is big)
  ls [prefix]                    lists the objects under a prefix
  rm <key>                       deletes an object
  sync <dir> [--prefix P]        syncs a folder with a drive prefix (default sync/<folder name>/)
  sync --azlin                   the Azlin tree: the data root -> azlin/data/,
                                 the .azlin folder -> azlin/config/ (config.json without endpoints)
       [--dry-run] [--allow-mass-delete] [--allow-burst] [--exclude GLOB]... [--max-file-mb N]
       [--parallel N]            (--allow-burst: the paused burst of changes is yours, send it)
  share <key> [--expires S]      a presigned link (with --prefix P: the synced file <key> of P)
  lockdown --yes                 revokes every other device, key and link of the drive
  lockdown-cancel <code> | --code-file F | -
                                 cancels a pending recovery-key lockdown (the recovery code signs)
  restore <prefix> --as-of T     restores objects as they were at T (RFC 3339)
  restore-status <id>            a restore's progress
  gc <prefix> | gc --azlin [--grace-hours H] [--dry-run]
                                 deletes blobs no sync index names (older than H, default 24)

flags (every one also has an environment variable; flag > environment > config file > default):
  --config F        the shared config file (AZLIN_CONFIG; default ~/.azlin/config.json)
  --profile P       local | trial | production (AZLIN_PROFILE; config endpoints.profile)
  --token-url U     the token server (AZLIN_TOKEN_URL, also AZLIN_TOKEN_SERVER)
  --s3-url U        the S3 endpoint, over the drive's own (AZLIN_S3_URL)
  --meet-url U      the meeting server (AZMEET_WORKER)
  --relay R         off | default | a relay address (AZMEET_RELAY)
  --transport T     auto | iroh | https (AZCLOUD_TRANSPORT)
  --iroh-node ID    the node to dial over iroh (AZLIN_IROH_NODE), --iroh-addr IP:PORT (AZLIN_IROH_ADDR)
  --state-dir D     this device's state folder (AZCLOUD_HOME; default <OS config folder>/azcloud)
  --data-dir D      the data root of the Azlin apps (AZLIN_DATA)
  --azlin-home D    the .azlin folder (AZLIN_HOME; default ~/.azlin)
  --drive ID        which drive of the state folder (default the current one)
  --device-name N   this device's name the first time (AZCLOUD_DEVICE)
  --no-refresh      do not renew credentials first
  --json            one JSON object on stdout
";

/// Flags that take a value.
const VALUE_FLAGS: &[&str] = &[
    "--config",
    "--state-dir",
    "--data-dir",
    "--azlin-home",
    "--profile",
    "--token-url",
    "--s3-url",
    "--meet-url",
    "--relay",
    "--transport",
    "--iroh-node",
    "--iroh-addr",
    "--drive",
    "--tier",
    "--name",
    "--device-name",
    "--code-file",
    "--member",
    "--out",
    "--key",
    "--prefix",
    "--exclude",
    "--expires",
    "--as-of",
    "--grace-hours",
    "--max-file-mb",
    "--parallel",
    "--cloudflare-account",
    "--cloudflare-token-file",
    "--recovery-out",
    "--worker",
];

/// Flags without a value.
const SWITCHES: &[&str] = &[
    "--json",
    "--dry-run",
    "--allow-mass-delete",
    "--allow-burst",
    "--azlin",
    "--yes",
    "--plaintext",
    "--help",
    "-h",
    "--no-refresh",
];

/// The command line, parsed.
#[derive(Default)]
struct Args {
    values: BTreeMap<String, Vec<String>>,
    switches: BTreeSet<String>,
    positional: Vec<String>,
}

impl Args {
    /// `--flag value`, `--flag=value` and switches anywhere; `--` ends the
    /// flags; `-` alone is a positional argument (stdin).
    fn parse(argv: &[String]) -> Result<Args, String> {
        let mut args = Args::default();
        let mut i = 0;
        let mut only_positional = false;
        while i < argv.len() {
            let arg = argv[i].clone();
            i += 1;
            if only_positional || !arg.starts_with('-') || arg == "-" {
                args.positional.push(arg);
                continue;
            }
            if arg == "--" {
                only_positional = true;
                continue;
            }
            let (name, inline) = match arg.split_once('=') {
                Some((name, value)) if name.starts_with("--") => {
                    (name.to_string(), Some(value.to_string()))
                }
                _ => (arg.clone(), None),
            };
            if SWITCHES.contains(&name.as_str()) {
                if inline.is_some() {
                    return Err(format!("{name} takes no value"));
                }
                args.switches.insert(name);
                continue;
            }
            if !VALUE_FLAGS.contains(&name.as_str()) {
                return Err(format!("unknown flag {name}"));
            }
            let value = match inline {
                Some(value) => value,
                None => {
                    let value = argv
                        .get(i)
                        .cloned()
                        .ok_or_else(|| format!("{name} needs a value"))?;
                    i += 1;
                    value
                }
            };
            args.values.entry(name).or_default().push(value);
        }
        Ok(args)
    }

    fn value(&self, name: &str) -> Option<&str> {
        self.values
            .get(name)
            .and_then(|v| v.last())
            .map(String::as_str)
    }

    fn all(&self, name: &str) -> Vec<&str> {
        self.values
            .get(name)
            .map(|v| v.iter().map(String::as_str).collect())
            .unwrap_or_default()
    }

    fn on(&self, name: &str) -> bool {
        self.switches.contains(name)
    }

    fn path(&self, name: &str) -> Option<PathBuf> {
        self.value(name).map(PathBuf::from)
    }

    fn string(&self, name: &str) -> Option<String> {
        self.value(name).map(String::from)
    }

    /// The settings' command-line layer.
    fn flags(&self) -> Flags {
        Flags {
            endpoints: EndpointFlags {
                profile: self.string("--profile"),
                token: self.string("--token-url"),
                s3: self.string("--s3-url"),
                meet: self.string("--meet-url"),
                relay: self.string("--relay"),
            },
            config: self.path("--config"),
            state_dir: self.path("--state-dir"),
            data_dir: self.path("--data-dir"),
            azlin_home: self.path("--azlin-home"),
            transport: self.string("--transport"),
            iroh_node: self.string("--iroh-node"),
            iroh_addr: self.string("--iroh-addr"),
        }
    }

    /// The command's own arguments (after the command name).
    fn rest(&self) -> &[String] {
        self.positional.get(1..).unwrap_or(&[])
    }

    fn number<T: std::str::FromStr>(&self, name: &str) -> Result<Option<T>> {
        match self.value(name) {
            Some(raw) => raw
                .trim()
                .parse::<T>()
                .map(Some)
                .map_err(|_| anyhow!("{name} {raw:?} is not a number")),
            None => Ok(None),
        }
    }
}

/// What a command prints: its JSON and its text.
type Output = (Value, String);

/// How the commands reach the network: HTTPS for the token server and the bucket, and the
/// dialer of S3 over iroh when this build has one.
struct Net {
    transports: TransportFactory,
    dialer: Option<Arc<dyn IrohDialer>>,
}

fn rfc3339(unix: i64) -> String {
    if unix <= 0 {
        String::from("never (a long-lived key)")
    } else {
        azcloud_kit::rfc3339(unix)
    }
}

/// A listing's time as S3 writes it (`2026-10-08T09:15:00.000Z`).
fn listing_time(unix: Option<u64>) -> String {
    match unix {
        Some(secs) => {
            let stamp = azul_storage::time::iso8601(secs);
            format!("{}.000Z", stamp.trim_end_matches('Z'))
        }
        None => String::new(),
    }
}

fn open_state(settings: &Settings) -> Result<StateDir> {
    Ok(StateDir::open(settings.state_path()?)?)
}

/// The account of this device's state folder, its credentials renewed when
/// they are due (unless `--no-refresh`).
fn open_account(settings: &Settings, net: &Net, args: &Args) -> Result<Account> {
    let state = open_state(settings)?;
    let mut account = Account::open(
        &state,
        settings.token_url()?,
        net.transports.clone(),
        args.value("--drive"),
    )?;
    if !args.on("--no-refresh") {
        account.ensure_fresh()?;
    }
    Ok(account)
}

fn open_drive(settings: &Settings, net: &Net, account: &Account, args: &Args) -> Result<CloudDrive> {
    let drive = CloudDrive::open(account, settings, net.dialer.clone())?;
    if let Some(parallel) = args.number::<usize>("--parallel")? {
        drive.set_parallel(parallel);
    }
    Ok(drive)
}

fn token_note(account: &Account) -> Option<String> {
    account.token_url_moved().map(|(drive, now)| {
        format!("this drive came from the token server {drive}; this run is configured with {now}")
    })
}

fn cmd_config(settings: &Settings) -> Result<Output> {
    // Read-only: no state folder is made for a question.
    let record = settings
        .state_dir
        .path
        .as_deref()
        .and_then(|dir| AccountFile::load(&dir.join(ACCOUNT_FILE)).ok())
        .and_then(|file| file.get(None).cloned());
    let (mut value, mut text) = settings.report(record.as_ref().map(|r| r.endpoint.as_str()));
    if let Some(r) = &record {
        value["drive"] = json!({"id": r.id, "bucket": r.bucket, "endpoint": r.endpoint,
                                "token_url": r.token_url});
        text.push_str(&format!(
            "drive      {} (bucket {}) from {}\n",
            r.id, r.bucket, r.token_url
        ));
    }
    Ok((value, text))
}

/// Whether `signup` makes the new drive encrypted: always ("we always encrypt"), but for
/// `--plaintext` - a drive like the ones made before encryption, for tests of those.
#[cfg(feature = "encryption")]
fn signup_encrypts(args: &Args) -> bool {
    !args.on("--plaintext")
}

/// A new recovery code for the user, shown once: into the file `out` (readable by the user
/// only) when there is one, else into the JSON and the text.
#[cfg(feature = "encryption")]
fn give_recovery_code(
    code: &azul_storage::crypto::keys::RecoveryCode,
    out: Option<PathBuf>,
    value: &mut Value,
    text: &mut String,
) -> Result<()> {
    let code = code.to_text();
    match out {
        Some(path) => {
            write_atomic(&path, format!("{}\n", code.as_str()).as_bytes(), true)
                .with_context(|| format!("{}", path.display()))?;
            value["recovery_code_file"] = json!(path.display().to_string());
            text.push_str(&format!(
                "Its RECOVERY CODE is in {} (readable by you only): print it or write it down, \
                 keep it apart from this computer, then delete the file. Azlin cannot reset it.\n",
                path.display()
            ));
        }
        None => {
            value["recovery_code"] = json!(code.as_str());
            text.push_str(&format!(
                "RECOVERY CODE (shown once, stored nowhere: write it down and keep it apart from \
                 this computer; Azlin cannot reset it):\n\n    {}\n\n",
                code.as_str()
            ));
        }
    }
    Ok(())
}

fn cmd_signup(settings: &Settings, net: &Net, args: &Args) -> Result<Output> {
    let state = open_state(settings)?;
    let device = state.device(args.value("--device-name"))?;
    let tier = args.value("--tier").unwrap_or("100GB");
    let name = args.value("--name").unwrap_or("Azlin Storage");
    let token_url = settings.token_url()?;
    let token_source = settings.endpoints.get(Endpoint::Token).source.label();
    let account = Account::signup(&state, token_url, net.transports.clone(), tier, name)?;
    // "We always encrypt": the keys are made as the drive is (they reach the bucket where this
    // run's S3 endpoint says).
    #[cfg(feature = "encryption")]
    let account = account.with_s3_endpoint(settings.endpoints.url(Endpoint::S3));
    #[cfg(feature = "encryption")]
    let recovery = if signup_encrypts(args) {
        let kdf = azul_storage::crypto::keys::RecoveryKdf::fresh()?;
        Some(account.setup_encryption(&state.secrets(), kdf)?)
    } else {
        None
    };
    let r = account.record();
    #[allow(unused_mut)]
    let mut value = json!({
        "ok": true, "drive": r.id, "bucket": r.bucket, "endpoint": r.endpoint, "tier": r.tier,
        "token_url": token_url, "token_url_source": token_source,
        "expires_at": rfc3339(r.expires_at), "nodes": r.nodes.len(), "device": device.name,
        "state_dir": state.path().display().to_string(),
    });
    #[allow(unused_mut)]
    let mut text = format!(
        "signed up: drive {} (bucket {}, {}) at {}\ntoken server {} ({token_source}); \
         credentials until {}; {} nodes\nthis device: {}; state in {}\n",
        r.id,
        r.bucket,
        r.tier.as_deref().unwrap_or("?"),
        r.endpoint,
        token_url,
        rfc3339(r.expires_at),
        r.nodes.len(),
        device.name,
        state.path().display()
    );
    #[cfg(feature = "encryption")]
    {
        value["encrypted"] = json!(recovery.is_some());
        match &recovery {
            Some(code) => {
                text.push_str("The drive is encrypted on this device.\n");
                give_recovery_code(code, args.path("--recovery-out"), &mut value, &mut text)?;
            }
            None => text.push_str("The drive is NOT encrypted (--plaintext).\n"),
        }
    }
    Ok((value, text))
}

fn cmd_invite(settings: &Settings, net: &Net, args: &Args) -> Result<Output> {
    let account = open_account(settings, net, args)?;
    // An encrypted drive's code carries its key, sealed for the joining device (once).
    #[cfg(feature = "encryption")]
    let code = {
        let account = account.with_s3_endpoint(settings.endpoints.url(Endpoint::S3));
        account.invite_with_key(args.value("--member"), &account.state().secrets())?
    };
    #[cfg(not(feature = "encryption"))]
    let code = account.invite(args.value("--member"))?;
    let carries_key = code.key_seal.is_some();
    let key_note = if carries_key {
        "\nit also carries the drive key: the other device reads the drive's files"
    } else {
        ""
    };
    let encoded = code.encode();
    match args.path("--out") {
        Some(path) => {
            write_atomic(&path, format!("{encoded}\n").as_bytes(), true)
                .with_context(|| format!("{}", path.display()))?;
            let value = json!({"ok": true, "member": code.member, "drive": code.drive_id,
                               "code_file": path.display().to_string(),
                               "carries_key": carries_key});
            let text = format!(
                "a join code for member {} is in {} (readable by you only): on the other device, \
                 azcloud join --code-file <it>{key_note}\n",
                code.member,
                path.display()
            );
            Ok((value, text))
        }
        None => {
            let value = json!({"ok": true, "member": code.member, "drive": code.drive_id,
                               "code": encoded, "carries_key": carries_key});
            let text = format!(
                "{encoded}\n(a secret: whoever has it joins the drive once; azcloud join <it>)\
                 {key_note}\n"
            );
            Ok((value, text))
        }
    }
}

/// A code from the command line: `<code>`, `--code-file F` or `-` (stdin). `what` names it in
/// errors ("join", "recovery"), `command` in the usage.
fn code_argument(args: &Args, what: &str, command: &str) -> Result<String> {
    Ok(
        match (
            args.path("--code-file"),
            args.rest().first().map(String::as_str),
        ) {
            (Some(path), _) => std::fs::read_to_string(&path)
                .with_context(|| format!("the {what} code file {}", path.display()))?,
            (None, Some("-")) => {
                let mut text = String::new();
                std::io::stdin().read_to_string(&mut text)?;
                text
            }
            (None, Some(code)) => code.to_string(),
            (None, None) => bail!("azcloud {command} <code> | --code-file <file> | - (stdin)"),
        },
    )
}

/// The recovery code from the command line ([`code_argument`]).
#[cfg(feature = "encryption")]
fn recovery_code_argument(
    args: &Args,
    command: &str,
) -> Result<azul_storage::crypto::keys::RecoveryCode> {
    let text = azul_storage::crypto::Zeroizing::new(code_argument(args, "recovery", command)?);
    azul_storage::crypto::keys::RecoveryCode::parse(&text)
        .ok_or_else(|| anyhow!("that is not a recovery code (26 letters and digits, in groups)"))
}

fn cmd_join(settings: &Settings, net: &Net, args: &Args) -> Result<Output> {
    let text = code_argument(args, "join", "join")?;
    let code = JoinCode::decode(&text)?;
    let state = open_state(settings)?;
    let device = state.device(args.value("--device-name"))?;
    let token_url = settings.token_url()?;
    let account = Account::join(&state, token_url, net.transports.clone(), &code)?;
    // The drive key, when the code carries it: this device enrols itself (once).
    #[cfg(feature = "encryption")]
    let account = account.with_s3_endpoint(settings.endpoints.url(Endpoint::S3));
    #[cfg(feature = "encryption")]
    let got_key = account
        .adopt_join_key(&code, &account.state().secrets())?
        .is_some();
    #[cfg(not(feature = "encryption"))]
    let got_key = false;
    let key_note = match (code.key_seal.is_some(), got_key) {
        (_, true) => "the drive key came with the code: this device reads the drive's files\n",
        (true, false) => {
            "the code carries the drive key, and this azcloud was built without the feature \
             `encryption`: the key was left for a build that has it\n"
        }
        (false, false) => "",
    };
    let r = account.record();
    let moved =
        (code.token_url.trim_end_matches('/') != token_url.trim_end_matches('/')).then(|| {
            format!(
                "the code came from {}, this device asked {token_url}",
                code.token_url
            )
        });
    let value = json!({"ok": true, "drive": r.id, "bucket": r.bucket, "member": r.member,
                       "device": device.name, "note": moved, "drive_key": got_key});
    let text = format!(
        "joined drive {} (bucket {}) as member {}; this device: {}\n{}{key_note}",
        r.id,
        r.bucket,
        r.member,
        device.name,
        moved.map(|m| format!("note: {m}\n")).unwrap_or_default()
    );
    Ok((value, text))
}

fn cmd_status(settings: &Settings, net: &Net, args: &Args) -> Result<Output> {
    let state = open_state(settings)?;
    let account = Account::open(
        &state,
        settings.token_url()?,
        net.transports.clone(),
        args.value("--drive"),
    )?;
    let r = account.record();
    let s3 = match settings.endpoints.url(Endpoint::S3) {
        Some(url) => format!(
            "{url} ({})",
            settings.endpoints.get(Endpoint::S3).source.label()
        ),
        None => format!("{} (the drive's grant)", r.endpoint),
    };
    let due = account.needs_refresh(azcloud_kit::now());
    let note = token_note(&account);
    let value = json!({
        "ok": true, "drive": r.id, "name": r.name, "bucket": r.bucket, "member": r.member,
        "s3": s3, "token_url": account.token_url(), "drive_token_url": r.token_url,
        "expires_at": rfc3339(r.expires_at), "refresh_due": due,
        "refreshed_at": rfc3339(r.refreshed_at), "nodes": r.nodes, "failover": r.failover,
        "read_only": r.read_only, "quota_bytes": r.quota_bytes, "note": note,
    });
    let mut text = format!(
        "drive {} \"{}\" (bucket {}), member {}\nS3 {s3}\ncredentials until {}{}\nnodes:\n",
        r.id,
        r.name,
        r.bucket,
        r.member,
        rfc3339(r.expires_at),
        if due {
            " (renewed at the next command)"
        } else {
            ""
        }
    );
    for node in &r.nodes {
        text.push_str(&format!(
            "  {} {} ready={}\n",
            node["name"].as_str().unwrap_or("?"),
            node["url"].as_str().unwrap_or("?"),
            node["ready"]
        ));
    }
    if let Some(note) = note {
        text.push_str(&format!("note: {note}\n"));
    }
    Ok((value, text))
}

fn cmd_info(settings: &Settings, net: &Net, args: &Args) -> Result<Output> {
    let account = open_account(settings, net, args)?;
    let mut info = account.info()?;
    let text = format!("{}\n", serde_json::to_string_pretty(&info)?);
    info["ok"] = json!(true);
    Ok((info, text))
}

fn cmd_refresh(settings: &Settings, net: &Net, args: &Args) -> Result<Output> {
    let state = open_state(settings)?;
    let mut account = Account::open(
        &state,
        settings.token_url()?,
        net.transports.clone(),
        args.value("--drive"),
    )?;
    account.refresh()?;
    let r = account.record();
    let value = json!({"ok": true, "drive": r.id, "expires_at": rfc3339(r.expires_at),
                       "nodes": r.nodes.len(), "failover": r.failover});
    let text = format!(
        "new credentials for {} until {}; {} nodes\n",
        r.id,
        rfc3339(r.expires_at),
        r.nodes.len()
    );
    Ok((value, text))
}

fn cmd_transport(settings: &Settings, net: &Net, args: &Args) -> Result<Output> {
    let account = open_account(settings, net, args)?;
    let drive = open_drive(settings, net, &account, args)?;
    let report = drive.transport();
    drive.close();
    let mut value = serde_json::to_value(&report)?;
    value["ok"] = json!(true);
    let target = report
        .iroh_target
        .as_ref()
        .map(|t| {
            format!(
                "{}{} ({})",
                t.id,
                if t.addrs.is_empty() {
                    String::new()
                } else {
                    format!(" at {}", t.addrs.join(", "))
                },
                t.source
            )
        })
        .unwrap_or_else(|| String::from("none"));
    let text = format!(
        "transport {:?} (asked: {})\nwhy: {}\niroh in this build: {}\niroh node: {target}\nrelay: \
         {}\nS3 endpoint: {} ({})\n",
        report.lane,
        report.preference.name(),
        report.reason,
        report.iroh_built,
        report.relay.as_deref().unwrap_or("-"),
        report.endpoint,
        report.endpoint_source
    );
    Ok((value, text))
}

fn lane_name(drive: &CloudDrive) -> String {
    drive.lane().name().to_string()
}

fn cmd_up(settings: &Settings, net: &Net, args: &Args) -> Result<Output> {
    let file = args
        .rest()
        .first()
        .ok_or_else(|| anyhow!("azcloud up <file> [--key K]"))?;
    let path = PathBuf::from(file);
    // Streamed: the file is never read whole; a big one goes up in parts, resumably.
    let bytes = std::fs::metadata(&path)
        .with_context(|| format!("{}", path.display()))?
        .len();
    let key = match args.value("--key") {
        Some(key) => key.to_string(),
        None => format!(
            "uploads/{}",
            path.file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .ok_or_else(|| anyhow!("{} has no file name; pass --key", path.display()))?
        ),
    };
    sync::remote::check_key(&key)?;
    #[cfg(feature = "encryption")]
    if let Some(encrypted) = encrypted_files(settings, net, args)? {
        // Streamed through the encryption: one segment of plaintext in memory at a time.
        let blake3 = hash_file(&path).with_context(|| format!("{}", path.display()))?;
        let mut file =
            std::fs::File::open(&path).with_context(|| format!("{}", path.display()))?;
        let info = encrypted.write_from(&key, &mut file, azul_storage::encrypted::Expect::Any)?;
        let etag = info.etag.unwrap_or_default();
        let value = json!({"ok": true, "key": key, "bytes": bytes, "blake3": blake3, "etag": etag,
                           "transport": "https", "encrypted": true});
        let text = format!("up {key}: {bytes} bytes, encrypted, over https (BLAKE3 {blake3})\n");
        return Ok((value, text));
    }
    let account = open_account(settings, net, args)?;
    refuse_plaintext(&account)?;
    let drive = open_drive(settings, net, &account, args)?;
    let blake3 = hash_file(&path).with_context(|| format!("{}", path.display()))?;
    drive.put_file(&key, &path, &|_| {})?;
    let etag = drive.head(&key)?.map(|(_, etag)| etag).unwrap_or_default();
    let lane = lane_name(&drive);
    drive.close();
    let value = json!({"ok": true, "key": key, "bytes": bytes, "blake3": blake3, "etag": etag,
                       "transport": lane});
    let text = format!("up {key}: {bytes} bytes over {lane} (BLAKE3 {blake3})\n");
    Ok((value, text))
}

fn cmd_down(settings: &Settings, net: &Net, args: &Args) -> Result<Output> {
    let (key, file) = match args.rest() {
        [key, file, ..] => (key.clone(), PathBuf::from(file)),
        _ => bail!("azcloud down <key> <file>"),
    };
    #[cfg(feature = "encryption")]
    if let Some(encrypted) = encrypted_files(settings, net, args)? {
        let mut reader = match encrypted.open_reader(&key) {
            Ok(reader) => reader,
            Err(azul_storage::DriveError::NotFound { .. }) => {
                return Err(anyhow!("the drive has no {key}"))
            }
            Err(e) => return Err(e.into()),
        };
        // Into a hidden file next to it, renamed when the whole file is there (its BLAKE3
        // checked by the reader at the end).
        let partial = file.with_file_name(format!(
            ".{}.azcloud-part",
            file.file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_default()
        ));
        let mut out =
            std::fs::File::create(&partial).with_context(|| format!("{}", partial.display()))?;
        let bytes = std::io::copy(&mut reader, &mut out)
            .with_context(|| format!("{key} into {}", partial.display()))?;
        out.sync_all()?;
        drop(out);
        std::fs::rename(&partial, &file).with_context(|| format!("{}", file.display()))?;
        let blake3 = hash_file(&file).with_context(|| format!("{}", file.display()))?;
        let value = json!({"ok": true, "key": key, "bytes": bytes, "blake3": blake3,
                           "file": file.display().to_string(), "transport": "https",
                           "encrypted": true});
        let text = format!(
            "down {key}: {bytes} bytes, decrypted, over https into {} (BLAKE3 {blake3})\n",
            file.display()
        );
        return Ok((value, text));
    }
    let account = open_account(settings, net, args)?;
    let drive = open_drive(settings, net, &account, args)?;
    if drive.head(&key)?.is_none() {
        return Err(anyhow!("the drive has no {key}"));
    }
    // Into the file in ranges, several at once (never whole in memory), resumably.
    let bytes = drive.download_to(&key, &file, &mut |_| {})?;
    let lane = lane_name(&drive);
    drive.close();
    let blake3 = hash_file(&file).with_context(|| format!("{}", file.display()))?;
    let value = json!({"ok": true, "key": key, "bytes": bytes, "blake3": blake3,
                       "file": file.display().to_string(), "transport": lane});
    let text = format!(
        "down {key}: {bytes} bytes over {lane} into {} (BLAKE3 {blake3})\n",
        file.display()
    );
    Ok((value, text))
}

fn cmd_ls(settings: &Settings, net: &Net, args: &Args) -> Result<Output> {
    let prefix = args.rest().first().map(String::as_str).unwrap_or("");
    let objects = match encrypted_files(settings, net, args)? {
        // The drive index's names: the bucket holds random object ids only.
        #[cfg(feature = "encryption")]
        Some(encrypted) => azul_storage::ops::list_all(&encrypted, prefix)?,
        #[cfg(not(feature = "encryption"))]
        Some(never) => match never {},
        None => {
            let account = open_account(settings, net, args)?;
            let drive = open_drive(settings, net, &account, args)?;
            let objects = drive.list_all(prefix)?;
            drive.close();
            objects
        }
    };
    let mut text = String::new();
    for o in &objects {
        text.push_str(&format!(
            "{:>12} {} {}\n",
            o.size,
            listing_time(o.modified),
            o.key
        ));
    }
    text.push_str(&format!("{} objects under {prefix:?}\n", objects.len()));
    let value = json!({"ok": true, "prefix": prefix, "objects": objects.iter().map(|o| json!({
        "key": o.key, "size": o.size, "etag": o.etag.clone().unwrap_or_default(),
        "last_modified": listing_time(o.modified),
    })).collect::<Vec<_>>()});
    Ok((value, text))
}

fn cmd_rm(settings: &Settings, net: &Net, args: &Args) -> Result<Output> {
    let key = args
        .rest()
        .first()
        .ok_or_else(|| anyhow!("azcloud rm <key>"))?;
    #[cfg(feature = "encryption")]
    if let Some(encrypted) = encrypted_files(settings, net, args)? {
        use azul_storage::Drive as _;
        match encrypted.delete(key) {
            Ok(()) | Err(azul_storage::DriveError::NotFound { .. }) => {}
            Err(e) => return Err(e.into()),
        }
        return Ok((json!({"ok": true, "key": key, "encrypted": true}), format!("deleted {key}\n")));
    }
    let account = open_account(settings, net, args)?;
    let drive = open_drive(settings, net, &account, args)?;
    drive.delete(key)?;
    drive.close();
    Ok((json!({"ok": true, "key": key}), format!("deleted {key}\n")))
}

/// The prefix of `sync <dir>` without `--prefix`: `sync/<folder name>/`.
fn default_prefix(dir: &Path) -> String {
    let abs = std::path::absolute(dir).unwrap_or_else(|_| dir.to_path_buf());
    let name = abs
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .filter(|n| !n.is_empty())
        .unwrap_or_else(|| String::from("root"));
    format!("sync/{}/", name.replace(['/', '\\'], "-"))
}

fn cmd_sync(settings: &Settings, net: &Net, args: &Args) -> Result<Output> {
    let roots: Vec<(LocalRoot, String)> = if args.on("--azlin") {
        if !args.rest().is_empty() || args.value("--prefix").is_some() {
            bail!("sync --azlin takes no folder and no prefix: it syncs the data root and the .azlin folder");
        }
        let data = settings
            .data_root
            .path
            .clone()
            .ok_or_else(|| anyhow!("no data root (AZLIN_DATA, --data-dir)"))?;
        let home = settings
            .azlin_home
            .path
            .clone()
            .ok_or_else(|| anyhow!("no .azlin folder (AZLIN_HOME, --azlin-home)"))?;
        sync::azlin_roots(&data, &home)
            .into_iter()
            .map(|(root, prefix)| (root, prefix.to_string()))
            .collect()
    } else {
        let dir = args
            .rest()
            .first()
            .ok_or_else(|| anyhow!("azcloud sync <dir> [--prefix P] | azcloud sync --azlin"))?;
        let dir = PathBuf::from(dir);
        let prefix = args
            .string("--prefix")
            .unwrap_or_else(|| default_prefix(&dir));
        vec![(LocalRoot::folder(&dir), prefix)]
    };
    let account = open_account(settings, net, args)?;
    refuse_plaintext(&account)?;
    refuse_encrypted(&account, "sync")?;
    let state = account.state().clone();
    let device = state.device(args.value("--device-name"))?;
    for (root, _) in &roots {
        if state.overlaps(&root.path) {
            bail!(
                "{} and this device's state folder {} overlap: a sync would upload its secrets. \
                 Move the state folder (AZCLOUD_HOME, --state-dir) or sync another folder",
                root.path.display(),
                state.path().display()
            );
        }
    }
    let drive = open_drive(settings, net, &account, args)?;
    let mut reports = Vec::new();
    let mut text = String::new();
    for (mut root, prefix) in roots {
        for pattern in args.all("--exclude") {
            root.rules.add(pattern, "--exclude");
        }
        let mut opts = SyncOptions::new(&prefix, &account.record().bucket, &device.name)?;
        opts.dry_run = args.on("--dry-run");
        opts.allow_mass_delete = args.on("--allow-mass-delete");
        opts.allow_burst = args.on("--allow-burst");
        if let Some(mb) = args.number::<u64>("--max-file-mb")? {
            opts.max_file_bytes = mb.saturating_mul(1024 * 1024);
        }
        if let Some(parallel) = args.number::<usize>("--parallel")? {
            opts.parallel = parallel.max(1);
        }
        let index_path =
            sync::local::index_path(&state.sync_dir(), &opts.bucket, &opts.prefix, &root.path);
        let report = match sync::sync_folder(&drive, &root, &index_path, &opts) {
            Ok(report) => report,
            Err(e) => {
                let what = format!("sync {} <-> {prefix}", root.path.display());
                drive.close();
                return Err(e.context(what).into());
            }
        };
        text.push_str(&report.summary());
        text.push('\n');
        for line in report
            .conflicts
            .iter()
            .map(|c| format!("  conflict: {c}"))
            .chain(report.errors.iter().map(|e| format!("  error: {e}")))
            .chain(report.notes.iter().map(|n| format!("  note: {n}")))
            .chain(report.planned.iter().map(|p| format!("  would: {p}")))
            .chain(
                report
                    .changed_during_sync
                    .iter()
                    .map(|c| format!("  later: {c}")),
            )
        {
            text.push_str(&line);
            text.push('\n');
        }
        reports.push(report);
    }
    let lane = lane_name(&drive);
    let transport = drive.transport();
    drive.close();
    let ok = reports.iter().all(|r| r.errors.is_empty());
    let value = json!({"ok": ok, "transport": lane, "transport_reason": transport.reason,
                       "device": device.name, "reports": reports});
    Ok((value, text))
}

fn cmd_share(settings: &Settings, net: &Net, args: &Args) -> Result<Output> {
    let key = args
        .rest()
        .first()
        .ok_or_else(|| anyhow!("azcloud share <key> [--expires S] [--prefix P]"))?;
    let expires = args.number::<i64>("--expires")?.unwrap_or(3600);
    let account = open_account(settings, net, args)?;
    refuse_encrypted(&account, "share")?;
    let drive = open_drive(settings, net, &account, args)?;
    let endpoint = drive.https_bucket().config().endpoint.clone();
    let now = azcloud_kit::now();
    let link = match args.value("--prefix") {
        Some(prefix) => share::synced_link(&drive, &account, &endpoint, prefix, key, expires, now),
        None => share::public_link(&account, &endpoint, key, expires, now),
    };
    drive.close();
    let link = link?;
    let mut value = serde_json::to_value(&link)?;
    value["ok"] = json!(true);
    let text = format!(
        "{}\nvalid until {}{}\n",
        link.url,
        rfc3339(link.expires_at),
        link.note
            .as_deref()
            .map(|n| format!("\nnote: {n}"))
            .unwrap_or_default()
    );
    Ok((value, text))
}

fn cmd_lockdown(settings: &Settings, net: &Net, args: &Args) -> Result<Output> {
    if !args.on("--yes") {
        bail!(
            "a lockdown revokes every other device, key and public link of the drive at once; \
             run again with --yes"
        );
    }
    let mut account = open_account(settings, net, args)?;
    let value = share::lockdown(&mut account)?;
    let text = format!(
        "locked down: {}; this device continues with new credentials, every other one must join \
         again\n",
        account.record().id
    );
    Ok((value, text))
}

/// F12, "the recovery code always wins": a recovery-key lockdown is cancelled with the
/// recovery code (its key signs; this device's token alone cancels nothing).
#[cfg(feature = "encryption")]
fn cmd_lockdown_cancel(settings: &Settings, net: &Net, args: &Args) -> Result<Output> {
    let code = recovery_code_argument(args, "lockdown-cancel")?;
    let account = open_account(settings, net, args)?;
    let mut value = share::lockdown_cancel(&account, code.as_bytes())?;
    if !value.is_object() {
        value = json!({});
    }
    value["ok"] = json!(true);
    Ok((value, String::from("the pending lockdown was cancelled\n")))
}

#[cfg(not(feature = "encryption"))]
fn cmd_lockdown_cancel(_settings: &Settings, _net: &Net, _args: &Args) -> Result<Output> {
    bail!(
        "a lockdown is cancelled with the drive's recovery code, which this azcloud (built \
         without the encryption feature) cannot read"
    )
}

fn cmd_restore(settings: &Settings, net: &Net, args: &Args) -> Result<Output> {
    let prefix = args
        .rest()
        .first()
        .ok_or_else(|| anyhow!("azcloud restore <prefix> --as-of <RFC 3339>"))?;
    let as_of = args
        .value("--as-of")
        .ok_or_else(|| anyhow!("azcloud restore <prefix> --as-of <RFC 3339>"))?;
    let account = open_account(settings, net, args)?;
    let mut value = share::restore(&account, prefix, as_of)?;
    let text = format!(
        "restore of {prefix:?} as of {as_of} queued: {} (azcloud restore-status <it>)\n",
        value["request_id"].as_str().unwrap_or("?")
    );
    value["ok"] = json!(true);
    Ok((value, text))
}

fn cmd_restore_status(settings: &Settings, net: &Net, args: &Args) -> Result<Output> {
    let id = args
        .rest()
        .first()
        .ok_or_else(|| anyhow!("azcloud restore-status <request id>"))?;
    let account = open_account(settings, net, args)?;
    let mut value = share::restore_status(&account, id)?;
    let text = format!("{}\n", serde_json::to_string_pretty(&value)?);
    value["ok"] = json!(true);
    Ok((value, text))
}

fn cmd_gc(settings: &Settings, net: &Net, args: &Args) -> Result<Output> {
    let prefixes: Vec<String> = if args.on("--azlin") {
        vec![sync::DATA_PREFIX.to_string(), sync::HOME_PREFIX.to_string()]
    } else {
        vec![args
            .rest()
            .first()
            .cloned()
            .ok_or_else(|| anyhow!("azcloud gc <prefix> | gc --azlin"))?]
    };
    let grace = args.number::<i64>("--grace-hours")?.unwrap_or(24) * 3600;
    let account = open_account(settings, net, args)?;
    let drive = open_drive(settings, net, &account, args)?;
    let mut reports = Vec::new();
    let mut text = String::new();
    for prefix in prefixes {
        let report = match sync::collect_garbage(&drive, &prefix, grace, args.on("--dry-run")) {
            Ok(report) => report,
            Err(e) => {
                drive.close();
                return Err(e.into());
            }
        };
        text.push_str(&format!(
            "{}: {} blobs, {} named by the index, {} too young, {} {} ({} bytes)\n",
            report.prefix,
            report.blobs,
            report.referenced,
            report.kept_young,
            if report.dry_run {
                "would be deleted"
            } else {
                "deleted"
            },
            report.deleted,
            report.bytes_freed
        ));
        reports.push(report);
    }
    drive.close();
    Ok((json!({"ok": true, "reports": reports}), text))
}

/// Refuses a command that writes plaintext into the drive when this device keeps the drive's
/// key (the drive is encrypted): a look into the secrets file, no network.
#[cfg(feature = "encryption")]
fn refuse_plaintext(account: &Account) -> Result<()> {
    if account.holds_drive_key(&account.state().secrets())? {
        bail!(
            "drive {} is encrypted: this command would store plaintext in it. Files go through \
             the encryption once the drive index (the encrypted metadata repository) is in",
            account.record().id
        );
    }
    Ok(())
}

/// Without the feature nothing here knows the drive's key: the commands run as before.
#[cfg(not(feature = "encryption"))]
fn refuse_plaintext(_account: &Account) -> Result<()> {
    Ok(())
}

/// The drive index of encrypted drives: the bucket's encrypted metadata repository, this
/// device's copy of it in the state folder (`<state>/drive-index`), named in its commits by the
/// device's name.
#[cfg(feature = "encryption")]
fn index_provider(state: &StateDir, device: &str) -> Box<dyn azcloud_kit::encryption::IndexProvider> {
    Box::new(
        azul_storage::meta::MetaIndexProvider::new(device)
            .with_cache_root(Some(state.path().join("drive-index"))),
    )
}

/// An encrypted drive's files through the encryption and the drive index (HTTPS); `None` for a
/// plaintext drive (its bucket as it is). Refused when the drive is encrypted and this device
/// has no key for it.
#[cfg(feature = "encryption")]
fn encrypted_files(
    settings: &Settings,
    net: &Net,
    args: &Args,
) -> Result<Option<azul_storage::encrypted::EncryptedDrive<Arc<dyn azul_storage::Drive>>>> {
    let account = encrypted_account(settings, net, args)?;
    let keyring = account.state().secrets();
    let (encrypted, _) = account.encryption_status(&keyring)?;
    if !encrypted {
        return Ok(None);
    }
    let id = account.record().id.clone();
    if account.unlock_key(&keyring)?.is_none() {
        bail!(
            "drive {id} is encrypted and this device has no key for it: a join code from a device \
             that has it (azcloud invite there), or azcloud recover <code>"
        );
    }
    let device = account.state().device(args.value("--device-name"))?;
    let provider = index_provider(account.state(), &device.name);
    Ok(Some(account.open_encrypted(&keyring, provider.as_ref())?))
}

/// Without the feature every drive is its bucket as it is.
#[cfg(not(feature = "encryption"))]
fn encrypted_files(
    _settings: &Settings,
    _net: &Net,
    _args: &Args,
) -> Result<Option<std::convert::Infallible>> {
    Ok(None)
}

/// Refuses a command that cannot go through the encryption yet (sync, share) for an encrypted
/// drive - whether or not this device keeps its key: it would store plaintext in it, or hand
/// out ciphertext.
#[cfg(feature = "encryption")]
fn refuse_encrypted(account: &Account, what: &str) -> Result<()> {
    let (encrypted, _) = account.encryption_status(&account.state().secrets())?;
    if encrypted {
        bail!(
            "drive {} is encrypted: {what} of the command line does not go through the \
             encryption yet (AzDrive's sync and Share do)",
            account.record().id
        );
    }
    Ok(())
}

#[cfg(not(feature = "encryption"))]
fn refuse_encrypted(_account: &Account, _what: &str) -> Result<()> {
    Ok(())
}

/// The account, reaching its bucket where this run's S3 endpoint says (the keys live there).
#[cfg(feature = "encryption")]
fn encrypted_account(settings: &Settings, net: &Net, args: &Args) -> Result<Account> {
    Ok(open_account(settings, net, args)?.with_s3_endpoint(settings.endpoints.url(Endpoint::S3)))
}

#[cfg(feature = "encryption")]
fn cmd_encryption(settings: &Settings, net: &Net, args: &Args) -> Result<Output> {
    let account = encrypted_account(settings, net, args)?;
    let (encrypted, here) = account.encryption_status(&account.state().secrets())?;
    let id = account.record().id.clone();
    let value = json!({"ok": true, "drive": id, "encrypted": encrypted, "key_here": here});
    let text = match (encrypted, here) {
        (false, _) => format!("drive {id} is not encrypted (azcloud encrypt --yes)\n"),
        (true, true) => format!("drive {id} is encrypted; this device keeps its key\n"),
        (true, false) => format!(
            "drive {id} is encrypted; this device has no key: azcloud unlock, a join code from a \
             device that has it, or azcloud recover <code>\n"
        ),
    };
    Ok((value, text))
}

#[cfg(feature = "encryption")]
fn cmd_encrypt(settings: &Settings, net: &Net, args: &Args) -> Result<Output> {
    if !args.on("--yes") {
        bail!(
            "azcloud encrypt --yes [--out F]: the drive's keys are made here and its recovery \
             code is shown ONCE (Azlin cannot reset it). The files commands (up, down, ls, rm) \
             go through the encryption afterwards; sync and share refuse an encrypted drive. \
             Files already in the drive stay as they are (AzDrive's \"Encrypt this drive\" \
             moves them)"
        );
    }
    let account = encrypted_account(settings, net, args)?;
    let kdf = azul_storage::crypto::keys::RecoveryKdf::fresh()?;
    let code = account.setup_encryption(&account.state().secrets(), kdf)?;
    let id = account.record().id.clone();
    let mut value = json!({"ok": true, "drive": id, "encrypted": true});
    let mut text = format!("drive {id} is encrypted from this device.\n");
    give_recovery_code(&code, args.path("--out"), &mut value, &mut text)?;
    Ok((value, text))
}

#[cfg(feature = "encryption")]
fn cmd_unlock(settings: &Settings, net: &Net, args: &Args) -> Result<Output> {
    let account = encrypted_account(settings, net, args)?;
    let id = account.record().id.clone();
    if account.unlock_key(&account.state().secrets())?.is_none() {
        bail!(
            "this device has no key for drive {id}: a join code from a device that has it \
             (azcloud invite there), or azcloud recover <code>"
        );
    }
    let value = json!({"ok": true, "drive": id, "key_here": true});
    Ok((value, format!("the key of drive {id} is on this device\n")))
}

#[cfg(feature = "encryption")]
fn cmd_recover(settings: &Settings, net: &Net, args: &Args) -> Result<Output> {
    let code = recovery_code_argument(args, "recover")?;
    let account = encrypted_account(settings, net, args)?;
    account.recover_key(&code, &account.state().secrets())?;
    let id = account.record().id.clone();
    let value = json!({"ok": true, "drive": id, "key_here": true});
    Ok((
        value,
        format!("the recovery code opened drive {id}: its key is on this device\n"),
    ))
}

/// git's remote helper for this account's drive: `git clone azlin::drive://<drive id>` runs
/// azul-storage's `git-remote-azlin`, which hands the URL here as `azcloud --drive <id>
/// git-remote <remote> <url>`. The drive index in the drive's bucket is served over this
/// run's signed S3 requests, with the key this device keeps. stdin and stdout belong to git's
/// protocol, so the command prints nothing else (errors go to stderr).
#[cfg(feature = "encryption")]
fn cmd_git_remote(settings: &Settings, net: &Net, args: &Args) -> Result<Output> {
    let account = encrypted_account(settings, net, args)?;
    let stdin = std::io::stdin();
    let stdout = std::io::stdout();
    account.serve_git_remote(
        &account.state().secrets(),
        stdin.lock(),
        stdout.lock(),
        &mut azul_storage::meta::git::index_pack,
    )?;
    Ok((json!({"ok": true}), String::new()))
}

#[cfg(feature = "encryption")]
fn cmd_rotate(settings: &Settings, net: &Net, args: &Args) -> Result<Output> {
    if !args.on("--yes") {
        bail!(
            "azcloud rotate --yes [--out F]: locks the drive down (every other device, key and \
             link loses access at once; the other devices join again with new codes), then makes \
             a new drive key and a new RECOVERY CODE, shown once. Then run azcloud reencrypt"
        );
    }
    let mut account = encrypted_account(settings, net, args)?;
    let device = account.state().device(args.value("--device-name"))?;
    let provider = index_provider(account.state(), &device.name);
    let keyring = account.state().secrets();
    let kdf = azul_storage::crypto::keys::RecoveryKdf::fresh()?;
    let rotated = account.rotate_drive_key(&keyring, provider.as_ref(), kdf)?;
    let id = account.record().id.clone();
    let drop = rotated.drop_key.map(|k| k.to_hex());
    let mut value = json!({"ok": true, "drive": id, "rotated": true,
                           "rewrapped": rotated.rewrapped,
                           "members_removed": rotated.members_removed,
                           "shares_revoked": rotated.shares_revoked,
                           "drop_public_key": drop});
    let mut text = format!(
        "drive {id} is locked down and has a new key ({} files re-wrapped; {} other devices and \
         invites removed, {} shares revoked).\n",
        rotated.rewrapped, rotated.members_removed, rotated.shares_revoked
    );
    text.push_str("The drive has a NEW recovery code; the old one opens nothing.\n");
    give_recovery_code(&rotated.recovery_code, args.path("--out"), &mut value, &mut text)?;
    if let Some(drop) = drop {
        text.push_str(&format!(
            "Incoming mail has a new drop key: azcloud mail-drop sets it on your mail Worker \
             ({drop}).\n"
        ));
    }
    text.push_str(
        "Recommended after a compromise: azcloud reencrypt (every file into a new object, so \
         nothing in the bucket opens with the old key).\n",
    );
    Ok((value, text))
}

#[cfg(feature = "encryption")]
fn cmd_reencrypt(settings: &Settings, net: &Net, args: &Args) -> Result<Output> {
    let account = encrypted_account(settings, net, args)?;
    let device = account.state().device(args.value("--device-name"))?;
    let provider = index_provider(account.state(), &device.name);
    let keyring = account.state().secrets();
    let mut state =
        azul_storage::rotation::ReencryptState::new(azul_storage::time::now_unix());
    let done = account.reencrypt(&keyring, provider.as_ref(), &mut state, &mut |_| Ok(()), &|| false)?;
    let id = account.record().id.clone();
    let value = json!({"ok": done, "drive": id, "reencrypted": state.done, "failed": state.failed});
    Ok((
        value,
        format!(
            "drive {id}: {} files in new objects{}\n",
            state.done,
            if state.failed > 0 {
                format!(", {} damaged ones left as they were", state.failed)
            } else {
                String::new()
            }
        ),
    ))
}

#[cfg(feature = "encryption")]
fn cmd_mail_drop(settings: &Settings, net: &Net, args: &Args) -> Result<Output> {
    use azcloud_kit::cloudflare::{Cloudflare, DEFAULT_WORKER, DROP_KEY_VARIABLE};
    let worker = args
        .string("--worker")
        .unwrap_or_else(|| DEFAULT_WORKER.to_string());
    let cloudflare = match (
        args.string("--cloudflare-account"),
        args.path("--cloudflare-token-file"),
    ) {
        (Some(account_id), Some(token_file)) => {
            let token = azul_storage::crypto::Zeroizing::new(
                std::fs::read_to_string(&token_file)
                    .with_context(|| format!("the Cloudflare token file {}", token_file.display()))?,
            );
            Some(Cloudflare::new((net.transports)(), &account_id, &token)?)
        }
        (None, None) => None,
        _ => bail!("--cloudflare-account and --cloudflare-token-file go together"),
    };
    let account = encrypted_account(settings, net, args)?;
    let public = account.enable_mail_drop(&account.state().secrets())?.to_hex();
    let id = account.record().id.clone();
    if let Some(cloudflare) = &cloudflare {
        cloudflare.set_worker_secret(&worker, DROP_KEY_VARIABLE, &public)?;
    }
    let value = json!({"ok": true, "drive": id, "drop_public_key": public, "worker": worker,
                       "worker_updated": cloudflare.is_some()});
    let text = if cloudflare.is_some() {
        format!(
            "incoming mail of drive {id} is sealed to its drop key; the Worker {worker} has the \
             key now\n"
        )
    } else {
        format!(
            "incoming mail of drive {id} is sealed to its drop key. Give your mail Worker the \
             public half:\n\n    npx wrangler secret put {DROP_KEY_VARIABLE}    # {public}\n\nor \
             run this again with --cloudflare-account ID --cloudflare-token-file F\n"
        )
    };
    Ok((value, text))
}

fn run(args: &Args, net: &Net) -> Result<Output> {
    let settings = Settings::from_process(&args.flags(), &os_dirs());
    let command = args
        .positional
        .first()
        .map(String::as_str)
        .unwrap_or("help");
    match command {
        "config" => cmd_config(&settings),
        "signup" => cmd_signup(&settings, net, args),
        "invite" => cmd_invite(&settings, net, args),
        "join" => cmd_join(&settings, net, args),
        "status" => cmd_status(&settings, net, args),
        #[cfg(feature = "encryption")]
        "encryption" => cmd_encryption(&settings, net, args),
        #[cfg(feature = "encryption")]
        "encrypt" => cmd_encrypt(&settings, net, args),
        #[cfg(feature = "encryption")]
        "unlock" => cmd_unlock(&settings, net, args),
        #[cfg(feature = "encryption")]
        "recover" => cmd_recover(&settings, net, args),
        #[cfg(feature = "encryption")]
        "mail-drop" => cmd_mail_drop(&settings, net, args),
        #[cfg(feature = "encryption")]
        "rotate" => cmd_rotate(&settings, net, args),
        #[cfg(feature = "encryption")]
        "reencrypt" => cmd_reencrypt(&settings, net, args),
        #[cfg(feature = "encryption")]
        "git-remote" => cmd_git_remote(&settings, net, args),
        #[cfg(not(feature = "encryption"))]
        "encryption" | "encrypt" | "unlock" | "recover" | "mail-drop" | "rotate" | "reencrypt"
        | "git-remote" => {
            bail!("{command}: this azcloud was built without the feature `encryption`")
        }
        "info" => cmd_info(&settings, net, args),
        "refresh" => cmd_refresh(&settings, net, args),
        "transport" => cmd_transport(&settings, net, args),
        "up" => cmd_up(&settings, net, args),
        "down" => cmd_down(&settings, net, args),
        "ls" => cmd_ls(&settings, net, args),
        "rm" => cmd_rm(&settings, net, args),
        "sync" => cmd_sync(&settings, net, args),
        "share" => cmd_share(&settings, net, args),
        "lockdown" => cmd_lockdown(&settings, net, args),
        "lockdown-cancel" => cmd_lockdown_cancel(&settings, net, args),
        "restore" => cmd_restore(&settings, net, args),
        "restore-status" => cmd_restore_status(&settings, net, args),
        "gc" => cmd_gc(&settings, net, args),
        "help" => Ok((json!({"ok": true, "usage": USAGE}), USAGE.to_string())),
        other => bail!("unknown command {other:?} (azcloud help)"),
    }
}

/// The error as printed: its chain, and what to do when the token server refused this
/// device's drive token.
fn error_text(e: &anyhow::Error) -> String {
    let signed_out = e
        .chain()
        .any(|cause| cause.downcast_ref::<CloudError>().is_some_and(CloudError::is_sign_in));
    if signed_out {
        format!("{e:#}; this device must join the drive again (azcloud join <code>)")
    } else {
        format!("{e:#}")
    }
}

/// The network of this run on `runtime`: HTTPS always (one client: every bucket and token
/// server call shares its connections), iroh when this build has it.
fn net_on(runtime: &tokio::runtime::Runtime) -> Result<Net> {
    let https = HttpsTransport::new(runtime.handle().clone(), HTTPS_TIMEOUT)?;
    let transports: TransportFactory =
        Arc::new(move || Box::new(https.clone()) as Box<dyn Transport>);
    #[cfg(feature = "iroh")]
    let dialer: Option<Arc<dyn IrohDialer>> = Some(Arc::new(
        azcloud_api::iroh_lane::IrohLane::new(runtime.handle().clone()),
    ));
    #[cfg(not(feature = "iroh"))]
    let dialer: Option<Arc<dyn IrohDialer>> = None;
    Ok(Net { transports, dialer })
}

fn main() -> ExitCode {
    let argv: Vec<String> = std::env::args().skip(1).collect();
    let args = match Args::parse(&argv) {
        Ok(args) => args,
        Err(e) => {
            eprintln!("azcloud: {e}\n\n{USAGE}");
            return ExitCode::from(2);
        }
    };
    if args.on("--help") || args.on("-h") || args.positional.is_empty() {
        print!("{USAGE}");
        return ExitCode::SUCCESS;
    }
    let json_mode = args.on("--json");
    let runtime = match tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
    {
        Ok(runtime) => runtime,
        Err(e) => {
            eprintln!("azcloud: no async runtime: {e}");
            return ExitCode::from(1);
        }
    };
    // The kit blocks this thread while the runtime's threads do the network.
    let outcome = net_on(&runtime).and_then(|net| run(&args, &net));
    let code = match outcome {
        Ok((value, text)) => {
            let ok = value.get("ok").and_then(Value::as_bool).unwrap_or(true);
            if json_mode {
                println!(
                    "{}",
                    serde_json::to_string_pretty(&value).unwrap_or_default()
                );
            } else {
                print!("{text}");
            }
            if ok {
                ExitCode::SUCCESS
            } else {
                ExitCode::from(1)
            }
        }
        Err(e) => {
            let text = error_text(&e);
            if json_mode {
                println!("{}", json!({"ok": false, "error": text}));
            }
            eprintln!("azcloud: {text}");
            ExitCode::from(1)
        }
    };
    runtime.shutdown_background();
    code
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A new drive is encrypted as it is made; `--plaintext` makes one like the drives made
    /// before encryption (tests of those).
    #[cfg(feature = "encryption")]
    #[test]
    fn a_signup_makes_an_encrypted_drive_unless_asked_for_a_plaintext_one() {
        assert!(signup_encrypts(&Args::parse(&argv("signup --name Photos")).unwrap()));
        assert!(!signup_encrypts(
            &Args::parse(&argv("signup --plaintext --name Old")).unwrap()
        ));
    }

    /// "We always encrypt and compress": the command line's default build makes, opens and
    /// writes encrypted drives.
    #[test]
    fn a_default_build_of_azcloud_speaks_encrypted_drives() {
        assert!(
            cfg!(feature = "encryption"),
            "azcloud's default features take `encryption`"
        );
    }

    fn argv(s: &str) -> Vec<String> {
        s.split_whitespace().map(String::from).collect()
    }

    #[test]
    fn flags_go_anywhere_in_both_forms_and_an_unknown_one_is_refused() {
        let a = Args::parse(&argv(
            "--json sync ./notes --prefix=azlin/notes --exclude *.bak --exclude tmp/ --dry-run",
        ))
        .unwrap();
        assert_eq!(a.positional, vec!["sync", "./notes"]);
        assert_eq!(a.value("--prefix"), Some("azlin/notes"));
        assert_eq!(a.all("--exclude"), vec!["*.bak", "tmp/"]);
        assert!(a.on("--json") && a.on("--dry-run"));
        assert!(Args::parse(&argv("ls --bogus")).is_err());
        assert!(
            Args::parse(&argv("up --key")).is_err(),
            "a value is missing"
        );
        assert!(Args::parse(&argv("ls --json=1")).is_err());
        let b = Args::parse(&argv("join - -- --not-a-flag")).unwrap();
        assert_eq!(b.positional, vec!["join", "-", "--not-a-flag"]);
    }

    #[test]
    fn the_endpoint_flags_reach_the_settings_layer() {
        let a = Args::parse(&argv(
            "--token-url http://t:1 --profile trial --relay off --transport https config",
        ))
        .unwrap();
        let f = a.flags();
        assert_eq!(f.endpoints.token.as_deref(), Some("http://t:1"));
        assert_eq!(f.endpoints.profile.as_deref(), Some("trial"));
        assert_eq!(f.endpoints.relay.as_deref(), Some("off"));
        assert_eq!(f.transport.as_deref(), Some("https"));
    }

    #[test]
    fn a_folder_without_a_prefix_syncs_to_sync_and_its_name() {
        assert_eq!(
            default_prefix(Path::new("/home/a/My Notes")),
            "sync/My Notes/"
        );
        assert_eq!(default_prefix(Path::new("/")), "sync/root/");
    }
}
