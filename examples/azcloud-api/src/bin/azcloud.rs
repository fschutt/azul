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
    path::PathBuf,
    process::ExitCode,
    sync::Arc,
};

#[cfg(feature = "encryption")]
use std::path::Path;

use anyhow::{anyhow, bail, Context, Result};
use azcloud_api::{https::HttpsTransport, os_dirs, HTTPS_TIMEOUT};
#[cfg(feature = "encryption")]
use azcloud_kit::state::OutFile;
#[cfg(feature = "encryption")]
use azcloud_kit::sync::local::hash_file;
use azcloud_kit::{
    account::{Account, AccountFile, JoinCode},
    drive::TransportFactory,
    settings::{Flags, Settings},
    share,
    state::{write_atomic, StateDir, ACCOUNT_FILE},
    sync,
    transport::{CloudDrive, IrohDialer},
    CloudError,
};
use azul_appkit::azlin_config::{Endpoint, EndpointFlags};
use azul_storage::{footprint::Allow, Transport};
use serde_json::{json, Value};

const USAGE: &str = "\
azcloud - the Azlin cloud client (azul examples/azcloud-api)

usage: azcloud [flags] <command> [arguments]

commands:
  config                         the effective endpoints and folders, and where each value came
                                 from (a flag, the environment, the config file, a built-in default)
  signup [--tier T] [--name N] [--recovery-out F]
                                 a new drive at the token server (dev servers: POST /v1/drives),
                                 encrypted as it is made: its RECOVERY CODE shows once (or goes
                                 into F, readable by you only)
  invite [--member M] [--out F]  a join code for another device (a member token family of its own;
                                 it holds a secret: --out writes it to a file only you can read)
  join --code-file F | -         this device joins the drive of a code (- reads stdin; a code on the
                                 command line would show in ps and in the shell's history)
  status                         this device's drive, credentials and node list (no network)
  encryption                     whether the drive is encrypted and this device keeps its key
  encrypt --yes [--out F]        encrypts the drive from this device: its keys, and its RECOVERY
                                 CODE, shown once (--out writes it to a file only you can read)
  unlock                         the drive key from this device's own wrap in the bucket
  recover --code-file F | -      this device gets the drive key with the recovery code (F or
                                 stdin, never the command line)
  rotate --yes [--out F]         \"I was hacked\": locks the drive down (every other device, key and
                                 link), then a new drive key; the new RECOVERY CODE shows once
  reencrypt [--erase-old-versions --yes]
                                 after rotate: every file into a new object (recommended);
                                 with --erase-old-versions --yes the old versions - the objects the
                                 old key still opens - are erased for good once it is done, and
                                 never while a file could not be read (it has no --dry-run)
  packs [--grace-days D]         the packs small files share: uploads a crash left half done
                                 cleared up, packs under half live repacked, retired packs
                                 deleted D days after they were retired (default 30)
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
  up <file> [--key K]            uploads a file (default key uploads/<file name>) through the
                                 encryption and the drive index; an Azlin drive is encrypted, so a
                                 drive this device cannot show to be (a new one, an older one, a
                                 bucket that hides its key files) is refused: nothing is written
                                 (up, down, ls, rm)
  down <key> <file>              downloads an object (decrypted, a segment at a time)
  ls [prefix]                    lists the files under a prefix
  rm <key>                       deletes a file
  sync, share                    refused: the command line does not take an Azlin drive through
                                 the encryption for these yet (AzDrive's sync and Share do)
  lockdown --yes                 revokes every other device, key and link of the drive
  lockdown-cancel --code-file F | -
                                 cancels a pending recovery-key lockdown (the recovery code signs)
  restore <prefix> --as-of T     restores objects as they were at T (RFC 3339)
  restore-status <id>            a restore's progress
  gc <prefix> | gc --azlin [--grace-hours H] [--dry-run]
                                 deletes blobs no sync index names (older than H, default 24)

flags (every one also has an environment variable; flag > environment > config file > default):
  (azcloud as shipped talks to the live token server: --profile, --token-url and --s3-url, their
   variables and the config file's endpoints are read by a test build only, --features test-hooks;
   so are --iroh-node and --iroh-addr and their variables: a shipped build dials the nodes the
   token server's list names)
  --config F        the shared config file (AZLIN_CONFIG; default ~/.azlin/config.json)
  --profile P       local | trial | production (AZLIN_PROFILE; config endpoints.profile)
  --token-url U     the token server (AZLIN_TOKEN_URL, also AZLIN_TOKEN_SERVER)
  --s3-url U        the S3 endpoint, over the drive's own (AZLIN_S3_URL)
  --meet-url U      the meeting server (AZMEET_WORKER)
  --relay R         off | default | a relay address (AZMEET_RELAY)
  --transport T     auto | iroh | https (AZCLOUD_TRANSPORT)
  --iroh-node ID    the node to dial over iroh (AZLIN_IROH_NODE), --iroh-addr IP:PORT (AZLIN_IROH_ADDR);
                    test builds only
  --state-dir D     this device's state folder (AZCLOUD_HOME; default <OS config folder>/azcloud)
  --data-dir D      the data root of the Azlin apps (AZLIN_DATA)
  --azlin-home D    the .azlin folder (AZLIN_HOME; default ~/.azlin)
  --drive ID        which drive of the state folder (default the current one)
  --device-name N   this device's name the first time (AZCLOUD_DEVICE)
  --no-refresh      do not renew credentials first
  --allow-plain-secrets
                    this computer has no keyring: keep the drive's keys in the state folder's
                    secrets file (readable by this user only; any program of this user reads
                    it). Without it a command that would keep a key there stops and says so
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
    "--as-of",
    "--grace-hours",
    "--parallel",
    "--cloudflare-account",
    "--cloudflare-token-file",
    "--recovery-out",
    "--worker",
    "--grace-days",
];

/// Flags without a value.
const SWITCHES: &[&str] = &[
    "--json",
    "--dry-run",
    "--azlin",
    "--yes",
    "--help",
    "-h",
    "--no-refresh",
    "--allow-plain-secrets",
    "--erase-old-versions",
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
            // Every drive is encrypted as it is made: there is no plaintext one to ask for.
            if arg == "--plaintext" {
                return Err(String::from(
                    "--plaintext is gone: every drive is encrypted as it is made",
                ));
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
#[cfg(feature = "encryption")]
fn listing_time(unix: Option<u64>) -> String {
    match unix {
        Some(secs) => {
            let stamp = azul_storage::time::iso8601(secs);
            format!("{}.000Z", stamp.trim_end_matches('Z'))
        }
        None => String::new(),
    }
}

/// The state folder of this device. Its secrets file takes a drive's keys only when
/// `--allow-plain-secrets` was given (AZ-K7): the command line cannot reach the OS keyring.
fn open_state(settings: &Settings, args: &Args) -> Result<StateDir> {
    let allow = if args.on("--allow-plain-secrets") {
        Allow::PlainSecrets
    } else {
        Allow::Nothing
    };
    Ok(StateDir::open(settings.state_path()?)?.allowing_plain_secrets(allow))
}

/// The account of this device's state folder, its credentials renewed when
/// they are due (unless `--no-refresh`).
fn open_account(settings: &Settings, net: &Net, args: &Args) -> Result<Account> {
    let state = open_state(settings, args)?;
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

/// Makes the file the new recovery code is to go into ([`OutFile`]) before anything else of the
/// command happens: `None` without a path; an error when the path cannot be written - with nothing
/// done yet (X-DELTA-KEYS2-4).
#[cfg(feature = "encryption")]
fn prepare_code_file(path: Option<PathBuf>) -> Result<Option<OutFile>> {
    path.map(|path| OutFile::prepare(&path).with_context(|| format!("{}", path.display())))
        .transpose()
}

/// A new recovery code for the user, shown once: into the file `out` (readable by the user
/// only) when there is one, else into the JSON and the text. A file that cannot be written does not
/// lose the code - it exists nowhere else, and the old one is dead: it is shown in the JSON and
/// the text as without a file, with the file's error as the warning beside it.
#[cfg(feature = "encryption")]
fn give_recovery_code(
    code: &azul_storage::crypto::keys::RecoveryCode,
    out: Option<&OutFile>,
    value: &mut Value,
    text: &mut String,
) {
    let code = code.to_text();
    let show = |value: &mut Value, text: &mut String| {
        value["recovery_code"] = json!(code.as_str());
        text.push_str(&format!(
            "RECOVERY CODE (shown once, stored nowhere: write it down and keep it apart from \
             this computer; Azlin cannot reset it):\n\n    {}\n\n",
            code.as_str()
        ));
    };
    match out {
        Some(file) => match file.write(format!("{}\n", code.as_str()).as_bytes()) {
            Ok(()) => {
                value["recovery_code_file"] = json!(file.path().display().to_string());
                text.push_str(&format!(
                    "Its RECOVERY CODE is in {} (readable by you only): print it or write it down, \
                     keep it apart from this computer, then delete the file. Azlin cannot reset it.\n",
                    file.path().display()
                ));
            }
            Err(e) => {
                value["recovery_code_file_error"] =
                    json!(format!("{}: {e}", file.path().display()));
                text.push_str(&format!(
                    "warning: the recovery code could not be written to {}: {e}. It is shown here \
                     instead.\n",
                    file.path().display()
                ));
                show(value, text);
            }
        },
        None => show(value, text),
    }
}

fn cmd_signup(settings: &Settings, net: &Net, args: &Args) -> Result<Output> {
    // Before the first call of the token server (X-DELTA-KEYS2-4): a path that cannot be written
    // stops the command while no drive was made.
    #[cfg(feature = "encryption")]
    let code_file = prepare_code_file(args.path("--recovery-out"))?;
    let state = open_state(settings, args)?;
    let device = state.device(args.value("--device-name"))?;
    let tier = args.value("--tier").unwrap_or("100GB");
    let name = args.value("--name").unwrap_or("Azlin Storage");
    let token_url = settings.token_url()?;
    let token_source = settings.endpoints.get(Endpoint::Token).source.label();
    // Encrypted as it is made: the keys reach the bucket where this run's S3 endpoint says. A
    // build without encryption makes no drive (the kit refuses, saying why).
    #[cfg(feature = "encryption")]
    let (account, recovery) = {
        let secrets = state.secrets();
        let keys = azcloud_kit::encryption::NewKeys {
            keyring: &secrets,
            s3_endpoint: settings.endpoints.url(Endpoint::S3),
            kdf: azul_storage::crypto::keys::RecoveryKdf::fresh()?,
        };
        Account::signup(&state, token_url, net.transports.clone(), tier, name, keys)?
    };
    #[cfg(not(feature = "encryption"))]
    let account = Account::signup(&state, token_url, net.transports.clone(), tier, name)?;
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
        value["encrypted"] = json!(true);
        text.push_str("The drive is encrypted on this device.\n");
        give_recovery_code(&recovery, code_file.as_ref(), &mut value, &mut text);
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
                "{encoded}\n(a secret: whoever has it joins the drive once; azcloud join --code-file \
                 <file>, or azcloud join - and paste it){key_note}\n"
            );
            Ok((value, text))
        }
    }
}

/// A code from `--code-file F` or `-` (stdin), never from the command line itself: a join code
/// (it carries a drive token) and a recovery code (it opens the drive) in the arguments are
/// visible to every program that lists processes and stay in the shell's history (AZ-K16).
/// `what` names it in errors ("join", "recovery"), `command` in the usage.
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
            (None, Some(_)) => bail!(
                "the {what} code is a secret: a code on the command line shows in ps and in the \
                 shell's history. azcloud {command} --code-file <file>, or azcloud {command} - \
                 (stdin)"
            ),
            (None, None) => bail!("azcloud {command} --code-file <file> | - (stdin)"),
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
    let state = open_state(settings, args)?;
    let device = state.device(args.value("--device-name"))?;
    let token_url = settings.token_url()?;
    // Before the code is spent: a keyring that keeps no key refuses the code's key afterwards, and
    // a retry presents a token that was rotated already (X-DELTA-AZDRIVE-KEYS-25).
    #[cfg(feature = "encryption")]
    Account::check_join_key_can_be_kept(&code, &state.secrets())?;
    let account = Account::join(&state, token_url, net.transports.clone(), &code)?;
    // The drive key, when the code carries it: this device enrols itself (once).
    #[cfg(feature = "encryption")]
    let account = account.with_s3_endpoint(settings.endpoints.url(Endpoint::S3));
    #[cfg(feature = "encryption")]
    let adopted = account.adopt_join_key_reporting(&code, &account.state().secrets())?;
    #[cfg(feature = "encryption")]
    let got_key = adopted.is_some();
    // The device has the key; an invite that could not be deleted is said, not a failure.
    #[cfg(feature = "encryption")]
    let invite_left = adopted
        .and_then(|adopted| adopted.invite_left)
        .map(|why| why.to_string());
    #[cfg(not(feature = "encryption"))]
    let got_key = false;
    #[cfg(not(feature = "encryption"))]
    let invite_left: Option<String> = None;
    let key_note = match (code.key_seal.is_some(), got_key) {
        (_, true) => "the drive key came with the code: this device reads the drive's files\n",
        (true, false) => {
            "the code carries the drive key, and this azcloud was built without the feature \
             `encryption`: the key was left for a build that has it\n"
        }
        (false, false) => "",
    };
    let invite_note = invite_left
        .as_deref()
        .map(|why| {
            format!(
                "warning: the join code's key could not be deleted from the drive ({why}): the \
                 code still opens the drive key until the keys are changed (azcloud rotate)\n"
            )
        })
        .unwrap_or_default();
    let r = account.record();
    let moved =
        (code.token_url.trim_end_matches('/') != token_url.trim_end_matches('/')).then(|| {
            format!(
                "the code came from {}, this device asked {token_url}",
                code.token_url
            )
        });
    let value = json!({"ok": true, "drive": r.id, "bucket": r.bucket, "member": r.member,
                       "device": device.name, "note": moved, "drive_key": got_key,
                       "invite_left": invite_left});
    let text = format!(
        "joined drive {} (bucket {}) as member {}; this device: {}\n{}{key_note}{invite_note}",
        r.id,
        r.bucket,
        r.member,
        device.name,
        moved.map(|m| format!("note: {m}\n")).unwrap_or_default()
    );
    Ok((value, text))
}

fn cmd_status(settings: &Settings, net: &Net, args: &Args) -> Result<Output> {
    let state = open_state(settings, args)?;
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
    let state = open_state(settings, args)?;
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
    {
        // The drive is opened before the file is read: a drive that cannot be shown to be
        // encrypted is refused here, and nothing is written.
        let encrypted = encrypted_drive(settings, net, args)?;
        // Streamed through the encryption: one segment of plaintext in memory at a time.
        let blake3 = hash_file(&path).with_context(|| format!("{}", path.display()))?;
        let mut file =
            std::fs::File::open(&path).with_context(|| format!("{}", path.display()))?;
        let info = encrypted.write_from(&key, &mut file, azul_storage::encrypted::Expect::Any)?;
        let etag = info.etag.unwrap_or_default();
        let value = json!({"ok": true, "key": key, "bytes": bytes, "blake3": blake3, "etag": etag,
                           "transport": "https", "encrypted": true});
        let text = format!("up {key}: {bytes} bytes, encrypted, over https (BLAKE3 {blake3})\n");
        Ok((value, text))
    }
    #[cfg(not(feature = "encryption"))]
    {
        let _ = (settings, net, bytes);
        Err(without_encryption("up"))
    }
}

fn cmd_down(settings: &Settings, net: &Net, args: &Args) -> Result<Output> {
    let (key, file) = match args.rest() {
        [key, file, ..] => (key.clone(), PathBuf::from(file)),
        _ => bail!("azcloud down <key> <file>"),
    };
    #[cfg(feature = "encryption")]
    {
        let encrypted = encrypted_drive(settings, net, args)?;
        let bytes = save_encrypted(&encrypted, &key, &file)?;
        let blake3 = hash_file(&file).with_context(|| format!("{}", file.display()))?;
        let value = json!({"ok": true, "key": key, "bytes": bytes, "blake3": blake3,
                           "file": file.display().to_string(), "transport": "https",
                           "encrypted": true});
        let text = format!(
            "down {key}: {bytes} bytes, decrypted, over https into {} (BLAKE3 {blake3})\n",
            file.display()
        );
        Ok((value, text))
    }
    #[cfg(not(feature = "encryption"))]
    {
        let _ = (settings, net, key, file);
        Err(without_encryption("down"))
    }
}

fn cmd_ls(settings: &Settings, net: &Net, args: &Args) -> Result<Output> {
    #[cfg(feature = "encryption")]
    {
        let prefix = args.rest().first().map(String::as_str).unwrap_or("");
        // The drive index's names: the bucket holds random object ids only.
        let encrypted = encrypted_drive(settings, net, args)?;
        let objects = azul_storage::ops::list_all(&encrypted, prefix)?;
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
    #[cfg(not(feature = "encryption"))]
    {
        let _ = (settings, net, args);
        Err(without_encryption("ls"))
    }
}

fn cmd_rm(settings: &Settings, net: &Net, args: &Args) -> Result<Output> {
    let key = args
        .rest()
        .first()
        .ok_or_else(|| anyhow!("azcloud rm <key>"))?;
    #[cfg(feature = "encryption")]
    {
        use azul_storage::Drive as _;
        let encrypted = encrypted_drive(settings, net, args)?;
        match encrypted.delete(key) {
            Ok(()) | Err(azul_storage::DriveError::NotFound { .. }) => {}
            Err(e) => return Err(e.into()),
        }
        Ok((
            json!({"ok": true, "key": key, "encrypted": true}),
            format!("deleted {key}\n"),
        ))
    }
    #[cfg(not(feature = "encryption"))]
    {
        let _ = (settings, net, key);
        Err(without_encryption("rm"))
    }
}

/// `sync` and `share` of the command line: refused. Both stored plaintext in the bucket
/// (the folder's files, a presigned link to a file in the clear), and an Azlin drive is
/// encrypted, always: AzDrive's sync and Share go through the encryption, these do not yet.
fn cmd_not_through_the_encryption(command: &str) -> Result<Output> {
    bail!(
        "{command} of the command line does not go through the encryption yet (AzDrive's sync and \
         Share do): an Azlin drive is encrypted, so nothing here stores its files in the clear"
    )
}

/// What a command that needs the encryption says in a build without it.
#[cfg(not(feature = "encryption"))]
fn without_encryption(command: &str) -> anyhow::Error {
    anyhow!(
        "{command}: this azcloud was built without the feature `encryption`, and an Azlin drive \
         is encrypted: nothing is stored in the clear"
    )
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

/// The drive index of encrypted drives: the bucket's encrypted metadata repository, this
/// device's copy of it in the state folder (`<state>/drive-index`), named in its commits by the
/// device's name.
#[cfg(feature = "encryption")]
fn index_provider(
    state: &StateDir,
    device: &str,
    restore: azul_storage::meta::RestoreSource,
) -> Box<dyn azcloud_kit::encryption::IndexProvider> {
    Box::new(azul_storage::meta::MetaIndexProvider::new(
        azul_storage::meta::IndexProfile {
            device_name: device.to_string(),
            cache_root: Some(state.path().join("drive-index")),
            lazy: false,
            packing: true,
            restore,
            reclaim: azul_storage::meta::ReclaimOptions::shadow(),
        },
    ))
}

/// Where the account's drive restore announcement comes from for the drive index: the token
/// server's word as the last sign-in recorded it (`DriveRecord::restore`). The copy of the index
/// this command line keeps in the state folder refuses a drive the operator restored, for good,
/// without it (X-DELTA-AZDRIVE-SYNC-3).
#[cfg(feature = "encryption")]
fn announced_restore(
    notice: Option<azcloud_kit::sync::RestoreNotice>,
) -> azul_storage::meta::RestoreSource {
    azul_storage::meta::RestoreSource::new(move || notice.clone())
}

/// Decrypts the encrypted drive's `key` into `file` through a hidden file next to it that is
/// renamed when the whole file is there. A big file comes a segment at a time (its BLAKE3
/// checked by the reader at the end); a small one shares a pack with others and has no
/// segments of its own, so it comes whole, in one ranged GET checked against its BLAKE3. The
/// bytes written.
#[cfg(feature = "encryption")]
fn save_encrypted<D: azul_storage::Drive>(
    drive: &azul_storage::encrypted::EncryptedDrive<D>,
    key: &str,
    file: &Path,
) -> Result<u64> {
    let missing = |e: azul_storage::DriveError| match e {
        azul_storage::DriveError::NotFound { .. } => anyhow!("the drive has no {key}"),
        e => e.into(),
    };
    let entry = drive.entry(key).map_err(missing)?;
    let packed = entry.object.as_ref().is_some_and(|o| o.packed.is_some());
    let mut source: Box<dyn Read + '_> = if packed {
        let plain = azul_storage::Drive::get(drive, key).map_err(missing)?;
        Box::new(std::io::Cursor::new(plain))
    } else {
        Box::new(drive.open_reader(key).map_err(missing)?)
    };
    let partial = file.with_file_name(format!(
        ".{}.azcloud-part",
        file.file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default()
    ));
    let mut out =
        std::fs::File::create(&partial).with_context(|| format!("{}", partial.display()))?;
    let bytes = std::io::copy(&mut source, &mut out)
        .with_context(|| format!("{key} into {}", partial.display()))?;
    out.sync_all()?;
    drop(out);
    std::fs::rename(&partial, file).with_context(|| format!("{}", file.display()))?;
    Ok(bytes)
}

/// The drive's files through the encryption and the drive index (HTTPS). There is no other way to
/// the files of an Azlin drive: a drive this device cannot show to be encrypted - a new one, an
/// older one, a bucket that hides its key files, which cannot be told apart - is refused, and so is
/// one this device holds no key for.
#[cfg(feature = "encryption")]
fn encrypted_drive(
    settings: &Settings,
    net: &Net,
    args: &Args,
) -> Result<azul_storage::encrypted::EncryptedDrive<Arc<dyn azul_storage::Drive>>> {
    use azcloud_kit::encryption::OpenRefusal;
    let account = encrypted_account(settings, net, args)?;
    let keyring = account.state().secrets();
    let device = account.state().device(args.value("--device-name"))?;
    let provider = index_provider(account.state(), &device.name, announced_restore(account.record().restore.clone()));
    account
        .open_encrypted_only(&keyring, provider.as_ref())
        .map_err(|refusal| match refusal {
            OpenRefusal::Locked { drive, .. } => anyhow!(
                "drive {drive} is encrypted and this device has no key for it: a join code from a \
                 device that has it (azcloud invite there), or azcloud recover --code-file <file>"
            ),
            OpenRefusal::NotEncrypted(_) => anyhow!("{refusal} (azcloud encrypt --yes)"),
            other => anyhow::Error::new(other),
        })
}

/// The account, reaching its bucket where this run's S3 endpoint says (the keys live there).
#[cfg(feature = "encryption")]
fn encrypted_account(settings: &Settings, net: &Net, args: &Args) -> Result<Account> {
    Ok(open_account(settings, net, args)?.with_s3_endpoint(settings.endpoints.url(Endpoint::S3)))
}

#[cfg(feature = "encryption")]
fn cmd_encryption(settings: &Settings, net: &Net, args: &Args) -> Result<Output> {
    use azul_storage::trust::{EncState, Via, Why};
    let account = encrypted_account(settings, net, args)?;
    let state = account.encryption_state(&account.state().secrets())?;
    let id = account.record().id.clone();
    let (name, encrypted, here) = match state {
        EncState::Encrypted {
            via: Via::KeyInKeyring,
            ..
        } => ("encrypted", true, true),
        EncState::Encrypted { .. } => ("locked", true, false),
        EncState::SetupPending => ("setup_pending", false, false),
        EncState::Unencrypted(Why::New) => ("new", false, false),
        EncState::Unencrypted(Why::HiddenOrAbsent) => ("not_encrypted", false, false),
        EncState::Undecided => ("undecided", false, false),
    };
    let value = json!({"ok": true, "drive": id, "state": name, "encrypted": encrypted,
                       "key_here": here});
    let text = match state {
        EncState::Encrypted {
            via: Via::KeyInKeyring,
            ..
        } => format!("drive {id} is encrypted; this device keeps its key\n"),
        EncState::Encrypted { .. } => format!(
            "drive {id} is encrypted; this device has no key: azcloud unlock, a join code from a \
             device that has it, or azcloud recover --code-file <file>\n"
        ),
        EncState::SetupPending => format!(
            "drive {id}: a setup of this device stopped halfway; azcloud encrypt --yes finishes it\n"
        ),
        EncState::Unencrypted(Why::New) => {
            format!("drive {id} is new and not encrypted yet (azcloud encrypt --yes)\n")
        }
        EncState::Unencrypted(Why::HiddenOrAbsent) | EncState::Undecided => format!(
            "drive {id} is not encrypted - or its storage hides its key files, which cannot be \
             told apart from here: no command reads or writes it until it is (azcloud encrypt \
             --yes)\n"
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
             go through the encryption afterwards. Files already in the drive stay as they are \
             (AzDrive's \"Encrypt this drive\" moves them)"
        );
    }
    // Before the first call of the token server or the bucket (X-DELTA-KEYS2-4).
    let code_file = prepare_code_file(args.path("--out"))?;
    let account = encrypted_account(settings, net, args)?;
    let kdf = azul_storage::crypto::keys::RecoveryKdf::fresh()?;
    let code = account.setup_encryption(&account.state().secrets(), kdf)?;
    let id = account.record().id.clone();
    let mut value = json!({"ok": true, "drive": id, "encrypted": true});
    let mut text = format!("drive {id} is encrypted from this device.\n");
    give_recovery_code(&code, code_file.as_ref(), &mut value, &mut text);
    Ok((value, text))
}

#[cfg(feature = "encryption")]
fn cmd_unlock(settings: &Settings, net: &Net, args: &Args) -> Result<Output> {
    let account = encrypted_account(settings, net, args)?;
    let id = account.record().id.clone();
    if account.unlock_key(&account.state().secrets())?.is_none() {
        bail!(
            "this device has no key for drive {id}: a join code from a device that has it \
             (azcloud invite there), or azcloud recover --code-file <file>"
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
    // Before the lockdown and the first write of the bucket (X-DELTA-KEYS2-4): the old code is dead
    // once the new wrap is in, and a path that cannot be written must not find that out then.
    let code_file = prepare_code_file(args.path("--out"))?;
    let mut account = encrypted_account(settings, net, args)?;
    let device = account.state().device(args.value("--device-name"))?;
    let provider = index_provider(account.state(), &device.name, announced_restore(account.record().restore.clone()));
    let keyring = account.state().secrets();
    let kdf = azul_storage::crypto::keys::RecoveryKdf::fresh()?;
    let rotated = account.rotate_drive_key(&keyring, provider.as_ref(), kdf)?;
    let id = account.record().id.clone();
    let drop = rotated.drop_key.map(|k| k.to_hex());
    let mut value = json!({"ok": true, "drive": id, "rotated": true,
                           "rewrapped": rotated.rewrapped,
                           "members_removed": rotated.members_removed,
                           "codes_removed": rotated.codes_removed,
                           "shares_revoked": rotated.shares_revoked,
                           "left_over": rotated.left_over,
                           "drop_public_key": drop});
    let mut text = format!(
        "drive {id} is locked down and has a new key ({} files re-wrapped; {} other devices and \
         invites removed, {} further recovery codes removed, {} shares revoked).\n",
        rotated.rewrapped, rotated.members_removed, rotated.codes_removed, rotated.shares_revoked
    );
    text.push_str("The drive has a NEW recovery code; the old one opens nothing.\n");
    if !rotated.left_over.is_empty() {
        // The rotation is done; what it could not clean up afterwards is said, not hidden.
        text.push_str(&format!(
            "warning: the rotation is done, and {} of its leftovers could not be removed: {}\n",
            rotated.left_over.len(),
            rotated.left_over.join("; ")
        ));
    }
    give_recovery_code(&rotated.recovery_code, code_file.as_ref(), &mut value, &mut text);
    if let Some(drop) = drop {
        text.push_str(&format!(
            "Incoming mail has a new drop key: azcloud mail-drop sets it on your mail Worker \
             ({drop}).\n"
        ));
    }
    text.push_str(
        "Recommended after a compromise: azcloud reencrypt (every file into a new object, so the \
         files as they are now no longer open with the old key; the older versions and the trash \
         of a drive that keeps its history do, until they expire).\n",
    );
    Ok((value, text))
}

/// Whether `azcloud reencrypt` also erases the old versions (X-DELTA-SYNC6-3). The erase cannot be
/// taken back - the history before it is gone - so it takes both words, as every destructive command
/// here does: `--erase-old-versions` alone is refused and says what it would do.
#[cfg(feature = "encryption")]
fn erase_old_versions_asked(args: &Args) -> Result<bool> {
    match (args.on("--erase-old-versions"), args.on("--yes")) {
        (false, _) => Ok(false),
        (true, true) => Ok(true),
        (true, false) => bail!(
            "azcloud reencrypt --erase-old-versions --yes: once every file is in a new object, \
             erases the old versions too - the objects the old drive key still opens - for good; \
             they cannot be restored afterwards. Without it they stay restorable until the \
             history's retention is over"
        ),
    }
}

#[cfg(feature = "encryption")]
fn cmd_reencrypt(settings: &Settings, net: &Net, args: &Args) -> Result<Output> {
    // Before anything is read or written: the words are checked first.
    let erase = erase_old_versions_asked(args)?;
    let account = encrypted_account(settings, net, args)?;
    let device = account.state().device(args.value("--device-name"))?;
    let provider = index_provider(account.state(), &device.name, announced_restore(account.record().restore.clone()));
    let keyring = account.state().secrets();
    let on_the_account = OnTheAccount {
        account: &account,
        keyring: &keyring,
        provider: provider.as_ref(),
    };
    let (state, done, erased) =
        reencrypt_sequence(&on_the_account, erase, azul_storage::time::now_unix())?;
    Ok(reencrypt_output(&account.record().id, done, &state, erased))
}

/// What the order of `azcloud reencrypt` needs of an account: the pass, and the erase after it.
#[cfg(feature = "encryption")]
trait Reencrypting {
    /// "Re-encrypt everything" into `state`; whether every file is done.
    fn pass(&self, state: &mut azul_storage::rotation::ReencryptState) -> Result<bool>;
    /// The erase of the old versions on `state`'s word: the report, or the sentence of the refusal.
    fn erase(
        &self,
        state: &azul_storage::rotation::ReencryptState,
    ) -> std::result::Result<azul_storage::meta::reclaim::ReclaimReport, String>;
}

/// The drive of the command: its account, this device's keyring, the drive index.
#[cfg(feature = "encryption")]
struct OnTheAccount<'a, K: azul_storage::keyring::KeyringStore> {
    account: &'a Account,
    keyring: &'a K,
    provider: &'a dyn azcloud_kit::encryption::IndexProvider,
}

#[cfg(feature = "encryption")]
impl<K: azul_storage::keyring::KeyringStore> Reencrypting for OnTheAccount<'_, K> {
    fn pass(&self, state: &mut azul_storage::rotation::ReencryptState) -> Result<bool> {
        Ok(self
            .account
            .reencrypt(self.keyring, self.provider, state, &mut |_| Ok(()), &|| {
                false
            })?)
    }

    fn erase(
        &self,
        state: &azul_storage::rotation::ReencryptState,
    ) -> std::result::Result<azul_storage::meta::reclaim::ReclaimReport, String> {
        self.account
            .erase_old_versions(self.keyring, self.provider, state)
            .map_err(|refused| refused.to_string())
    }
}

/// The order of `azcloud reencrypt` (X-DELTA-SYNC7-5): the confirmation is recorded FIRST when the user
/// asked for the erase - what the pass replaces after the click is what the erase is for, and the erase
/// takes only what was confirmed - then the pass; the erase is asked only when the pass is over (a stopped
/// pass erases nothing), and the kit's one licence ([`ReencryptState::erase_licence`]: the confirmation, the
/// end of the pass, no file left that could not be read) decides whether anything is erased: what it
/// refuses is the answer, said after the pass's own line (X-DELTA-SYNC7-2). The state, whether the pass is
/// over, and what became of the erase when it was asked.
#[cfg(feature = "encryption")]
fn reencrypt_sequence(
    run: &dyn Reencrypting,
    erase: bool,
    now: u64,
) -> Result<(
    azul_storage::rotation::ReencryptState,
    bool,
    Option<std::result::Result<azul_storage::meta::reclaim::ReclaimReport, String>>,
)> {
    let mut state = azul_storage::rotation::ReencryptState::new(now);
    if erase {
        state.confirm_purge(now);
    }
    let done = run.pass(&mut state)?;
    let erased = (erase && done).then(|| run.erase(&state));
    Ok((state, done, erased))
}

/// What `azcloud reencrypt` prints, in the order it happened (X-DELTA-SYNC7-2): the pass's own line
/// first - the files in new objects and how many it could not read - and only then what became of the
/// erase of the old versions. An erase that was refused is no success: `"ok": false`, exit code 1, and
/// the sentence says nothing was erased.
#[cfg(feature = "encryption")]
fn reencrypt_output(
    id: &str,
    done: bool,
    state: &azul_storage::rotation::ReencryptState,
    erased: Option<std::result::Result<azul_storage::meta::reclaim::ReclaimReport, String>>,
) -> Output {
    let mut value = json!({"ok": done, "drive": id, "reencrypted": state.done, "failed": state.failed});
    let mut text = format!(
        "drive {id}: {} files in new objects{}\n",
        state.done,
        if state.failed > 0 {
            format!(", {} damaged ones left as they were", state.failed)
        } else {
            String::new()
        }
    );
    match erased {
        None => {}
        Some(Ok(report)) => {
            value["erased"] = json!({
                "objects": report.deleted,
                "bytes": report.deleted_bytes,
                "failed": report.failed,
                "stopped": report.stopped.as_ref().map(|why| format!("{why:?}")),
            });
            text.push_str(&format!("{}\n", report.line()));
            // A purge that stopped (the lease, the fence, the clock ...), whose deletes the bucket refused or that a
            // cap cut short has erased some of the old versions at the most: no success a script could take for
            // "the old versions are gone" - `ok: false`, exit 1, and the sentence says what is left to do
            // (X-DELTA-SYNC8-3).
            let why = match (&report.stopped, report.failed, report.capped) {
                (Some(why), _, _) => Some(format!("it stopped: {why}")),
                (None, failed, _) if failed > 0 => {
                    Some(format!("the storage refused {failed} deletes"))
                }
                (None, _, true) => Some(String::from("a cap cut the scan short")),
                _ => None,
            };
            if let Some(why) = why {
                value["ok"] = json!(false);
                value["erased"]["complete"] = json!(false);
                text.push_str(&format!(
                    "the old versions are NOT all erased from drive {id}: {why}; run the command again to go on\n"
                ));
            }
        }
        Some(Err(refused)) => {
            value["ok"] = json!(false);
            value["erased"] = json!({"refused": refused});
            text.push_str(&format!("{refused}\n"));
            // The files the refusal is about: the user restores or deletes them, the only way to lift it.
            if !state.failed_paths.is_empty() {
                value["erased"]["damaged"] = json!(state.failed_paths);
                let more = state.failed.saturating_sub(state.failed_paths.len() as u64);
                text.push_str(&format!(
                    "the files that could not be read: {}{}\n",
                    state.failed_paths.join(", "),
                    if more > 0 {
                        format!(" (and {more} more, not named here)")
                    } else {
                        String::new()
                    }
                ));
            }
        }
    }
    (value, text)
}

/// The upkeep of an encrypted drive's packs of small files: what an idle AzDrive does every six
/// hours, now.
#[cfg(feature = "encryption")]
fn cmd_packs(settings: &Settings, net: &Net, args: &Args) -> Result<Output> {
    let drive = encrypted_drive(settings, net, args)?;
    let mut options = drive.packing().cloned().unwrap_or_default();
    if let Some(days) = args.number::<u64>("--grace-days")? {
        options.retired_grace_secs = days * 86_400;
    }
    let drive = drive.with_packing(options);
    let before = drive.pack_summary()?;
    let upkeep = drive.maintain_packs(azul_storage::time::now_unix())?;
    let after = drive.pack_summary()?;
    let retired = drive.retired_packs().len();
    let repack = &upkeep.repack;
    let summary = |s: &azul_storage::packing::PackSummary| {
        json!({
            "packed_files": s.packed_files,
            "packs": s.packs,
            "own_objects": s.own_objects,
            "pack_bytes": s.pack_bytes,
            "live_bytes": s.live_bytes,
        })
    };
    let value = json!({
        "ok": true,
        "before": summary(&before),
        "after": summary(&after),
        "recovered": upkeep.recovered,
        "packs_read": repack.packs_read,
        "packs_written": repack.packs_written,
        "packs_emptied": repack.packs_emptied,
        "files_moved": repack.files_moved,
        "files_damaged": repack.files_damaged,
        "bytes_before": repack.bytes_before,
        "bytes_after": repack.bytes_after,
        "collected": upkeep.collected,
        "retired": retired,
    });
    let text = format!(
        "before: {} files in {} packs ({} of {} bytes live), {} files in objects of their own\n\
         {} stale uploads cleared up; {} packs repacked into {} ({} files moved, {} -> {} \
         bytes), {} packs with no file left; {} retired packs deleted, {} waiting\n\
         after: {} files in {} packs ({} of {} bytes live)\n",
        before.packed_files,
        before.packs,
        before.live_bytes,
        before.pack_bytes,
        before.own_objects,
        upkeep.recovered,
        repack.packs_read,
        repack.packs_written,
        repack.files_moved,
        repack.bytes_before,
        repack.bytes_after,
        repack.packs_emptied,
        upkeep.collected,
        retired,
        after.packed_files,
        after.packs,
        after.live_bytes,
        after.pack_bytes
    );
    Ok((value, text))
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

/// The commands that read `--dry-run`: they say what they would do and change nothing.
const DRY_RUN_COMMANDS: &[&str] = &["gc"];

/// `--dry-run` is a switch of the whole command line ([`SWITCHES`]), and one command reads it: a command
/// that is not in [`DRY_RUN_COMMANDS`] would take the flag and do its whole work for real - `azcloud
/// reencrypt --erase-old-versions --yes --dry-run` erased the old versions (X-DELTA-SYNC7-3). Every
/// other command refuses it, before anything is read or written, and says that it has no preview.
fn refuse_dry_run_where_it_is_not_read(command: &str, args: &Args) -> Result<()> {
    if args.on("--dry-run") && !DRY_RUN_COMMANDS.contains(&command) {
        bail!(
            "azcloud {command}: has no --dry-run - it would do its work for real, and nothing was \
             done. Only {} takes --dry-run",
            DRY_RUN_COMMANDS.join(", ")
        );
    }
    Ok(())
}

fn run(args: &Args, net: &Net) -> Result<Output> {
    let settings = Settings::from_process(&args.flags(), &os_dirs());
    let command = args
        .positional
        .first()
        .map(String::as_str)
        .unwrap_or("help");
    refuse_dry_run_where_it_is_not_read(command, args)?;
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
        "packs" => cmd_packs(&settings, net, args),
        #[cfg(feature = "encryption")]
        "git-remote" => cmd_git_remote(&settings, net, args),
        #[cfg(not(feature = "encryption"))]
        "encryption" | "encrypt" | "unlock" | "recover" | "mail-drop" | "rotate" | "reencrypt"
        | "packs" | "git-remote" => {
            bail!("{command}: this azcloud was built without the feature `encryption`")
        }
        "info" => cmd_info(&settings, net, args),
        "refresh" => cmd_refresh(&settings, net, args),
        "transport" => cmd_transport(&settings, net, args),
        "up" => cmd_up(&settings, net, args),
        "down" => cmd_down(&settings, net, args),
        "ls" => cmd_ls(&settings, net, args),
        "rm" => cmd_rm(&settings, net, args),
        "sync" | "share" => cmd_not_through_the_encryption(command),
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
        format!("{e:#}; this device must join the drive again (azcloud join --code-file <file>)")
    } else {
        format!("{e:#}")
    }
}

/// The network of this run on `runtime`: HTTPS always (one client: every bucket and token
/// server call shares its connections), iroh when this build has it.
fn net_on(runtime: &tokio::runtime::Runtime, args: &Args) -> Result<Net> {
    // The shared config's trusted roots, for HTTPS and the iroh relays alike; a root it names
    // that cannot be read stops the run rather than trust more than the user asked for.
    let settings = Settings::from_process(&args.flags(), &os_dirs());
    let roots = settings
        .trust
        .and_then(|trust| trust.certificates_pem())
        .map_err(|why| anyhow::anyhow!("the trusted roots: {why}"))?;
    let https =
        HttpsTransport::trusting(runtime.handle().clone(), HTTPS_TIMEOUT, Some(&roots))?;
    let transports: TransportFactory =
        Arc::new(move || Box::new(https.clone()) as Box<dyn Transport>);
    #[cfg(feature = "iroh")]
    let dialer: Option<Arc<dyn IrohDialer>> = Some(Arc::new(
        azcloud_api::iroh_lane::IrohLane::new(runtime.handle().clone()).trusting(&roots)?,
    ));
    #[cfg(not(feature = "iroh"))]
    let dialer: Option<Arc<dyn IrohDialer>> = None;
    Ok(Net { transports, dialer })
}

fn main() -> ExitCode {
    // Every request to the Azlin services says which app and contract sent it.
    if let Some(hello) = azcloud_kit::hello::of_app("azcloud", env!("CARGO_PKG_VERSION")) {
        azcloud_kit::hello::announce(hello);
    }
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
    let outcome = net_on(&runtime, &args).and_then(|net| run(&args, &net));
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
            // A command that says its own exit code (sync: 3 a held mass delete, 2 refused).
            match value.get("exit").and_then(Value::as_u64) {
                Some(code) => ExitCode::from(u8::try_from(code).unwrap_or(1)),
                None if ok => ExitCode::SUCCESS,
                None => ExitCode::from(1),
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

    /// Every drive is encrypted as it is made: a plaintext one is refused with why.
    #[test]
    fn a_signup_makes_an_encrypted_drive_and_refuses_a_plaintext_one() {
        assert!(Args::parse(&argv("signup --name Photos")).is_ok());
        let refused = Args::parse(&argv("signup --plaintext --name Old"))
            .err()
            .expect("no plaintext drive");
        assert!(refused.contains("encrypted"), "{refused}");
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
            "--json up ./notes.txt --key=azlin/notes --key other --dry-run",
        ))
        .unwrap();
        assert_eq!(a.positional, vec!["up", "./notes.txt"]);
        assert_eq!(a.value("--key"), Some("other"), "the last one counts");
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

    /// X-DELTA-SYNC6-3: the erase of the old versions needs both words; one alone is refused with the
    /// sentence that says what it does, and a plain `reencrypt` erases nothing.
    #[cfg(feature = "encryption")]
    #[test]
    fn x_delta_sync6_3_the_erase_of_old_versions_needs_both_words() {
        let none = Args::parse(&argv("reencrypt")).unwrap();
        assert!(!erase_old_versions_asked(&none).unwrap());
        let yes_only = Args::parse(&argv("reencrypt --yes")).unwrap();
        assert!(!erase_old_versions_asked(&yes_only).unwrap());
        let both = Args::parse(&argv("reencrypt --erase-old-versions --yes")).unwrap();
        assert!(erase_old_versions_asked(&both).unwrap());
        let alone = Args::parse(&argv("reencrypt --erase-old-versions")).unwrap();
        let refused = erase_old_versions_asked(&alone).unwrap_err().to_string();
        assert!(refused.contains("for good") && refused.contains("--yes"), "{refused}");
        assert!(USAGE.contains("--erase-old-versions"), "the usage offers it");
    }


    /// X-DELTA-SYNC7-3: `--dry-run` is a switch of the whole command line and only `gc` reads it. Every
    /// other command refuses it with a sentence - before anything is read - instead of doing its work for
    /// real: `reencrypt --erase-old-versions --yes --dry-run` erased the old versions. The commands of the
    /// usage are sorted below into the ones that change something and the ones that only read; a command
    /// the usage gains must be put into one of them, so none is forgotten.
    #[test]
    fn x_delta_sync7_3_dry_run_is_refused_by_every_command_that_does_not_read_it() {
        let changes = [
            "signup",
            "invite",
            "join",
            "encrypt",
            "unlock",
            "recover",
            "mail-drop",
            "rotate",
            "reencrypt",
            "packs",
            "up",
            "rm",
            "lockdown",
            "lockdown-cancel",
            "restore",
            "refresh",
        ];
        let reads = [
            "config",
            "status",
            "encryption",
            "info",
            "transport",
            "down",
            "ls",
            "restore-status",
            "git-remote",
            "help",
            "sync",
            "share",
        ];
        // The commands of the usage: the first word of each line of its list of commands.
        let list = USAGE
            .split("commands:\n")
            .nth(1)
            .and_then(|rest| rest.split("\nflags ").next())
            .expect("the usage lists the commands");
        let mut in_usage: Vec<&str> = list
            .lines()
            .filter(|line| line.starts_with("  ") && !line.starts_with("   "))
            .filter_map(|line| line.split_whitespace().next())
            .map(|word| word.trim_end_matches(','))
            .filter(|word| word.chars().all(|c| c.is_ascii_lowercase() || c == '-'))
            .collect();
        in_usage.push("share");
        for command in &in_usage {
            assert!(
                changes.contains(command)
                    || reads.contains(command)
                    || DRY_RUN_COMMANDS.contains(command),
                "the usage lists \"{command}\": say whether it changes something or only reads"
            );
        }
        for command in changes.iter().chain(&reads) {
            let args = Args::parse(&argv(&format!("{command} --dry-run"))).unwrap();
            let refused = refuse_dry_run_where_it_is_not_read(command, &args)
                .expect_err(command)
                .to_string();
            assert!(
                refused.contains("no --dry-run") && refused.contains("gc"),
                "{refused}"
            );
            let plain = Args::parse(&argv(command)).unwrap();
            assert!(
                refuse_dry_run_where_it_is_not_read(command, &plain).is_ok(),
                "{command}"
            );
        }
        // The one command that reads it takes it.
        let gc = Args::parse(&argv("gc --azlin --dry-run")).unwrap();
        assert!(refuse_dry_run_where_it_is_not_read("gc", &gc).is_ok());
        // The line of the finding, word for word.
        let erase = Args::parse(&argv("reencrypt --erase-old-versions --yes --dry-run")).unwrap();
        assert!(refuse_dry_run_where_it_is_not_read("reencrypt", &erase).is_err());
    }

    /// X-DELTA-SYNC7-2: what `azcloud reencrypt` prints, in the order it happened. The pass's own line comes
    /// first and says how many files could not be read; the sentence that nothing was erased comes after it;
    /// and a refused erase is a failed command (`ok: false`), not a success a script could take for erased
    /// old versions. The erase's own rules are the kit's (`erase_old_versions` refuses on `failed > 0`).
    #[cfg(feature = "encryption")]
    #[test]
    fn x_delta_sync7_2_reencrypt_says_the_damaged_files_first_and_then_that_nothing_was_erased() {
        use azul_storage::{
            meta::reclaim::{ReclaimMode, ReclaimReport},
            rotation::ReencryptState,
        };

        let mut state = ReencryptState::new(0);
        state.done = 4;
        state.failed = 1;
        let refused = String::from(
            "nothing was erased from drive d_1: 1 file could not be read in the re-encryption; its \
             older versions may be the only readable copies and are kept",
        );
        let (value, text) = reencrypt_output("d_1", true, &state, Some(Err(refused.clone())));
        let lines: Vec<&str> = text.lines().collect();
        assert_eq!(lines.len(), 2, "{text}");
        assert!(
            lines[0].contains("4 files in new objects")
                && lines[0].contains("1 damaged ones left as they were"),
            "the damaged count comes first: {text}"
        );
        assert_eq!(lines[1], refused, "then what became of the erase: {text}");
        assert_eq!(
            value["ok"],
            json!(false),
            "a refused erase fails the command"
        );
        assert_eq!(value["erased"]["refused"], json!(refused));
        assert_eq!(value["failed"], json!(1));

        // An erase that ran is the report's line, and the command went well.
        state.failed = 0;
        let report = ReclaimReport::none(ReclaimMode::Delete);
        let (value, text) = reencrypt_output("d_1", true, &state, Some(Ok(report.clone())));
        assert_eq!(value["ok"], json!(true));
        assert_eq!(value["erased"]["objects"], json!(report.deleted));
        assert!(
            text.lines().nth(1) == Some(report.line().as_str()),
            "{text}"
        );
        // No erase asked: the pass's line alone.
        let (value, text) = reencrypt_output("d_1", true, &state, None);
        assert_eq!(text.lines().count(), 1, "{text}");
        assert!(value.get("erased").is_none());
    }

    /// X-DELTA-SYNC8-3: a purge that stopped, one whose deletes the storage refused and one a cap cut short are NOT a success:
    /// `ok: false` (the exit code of the command), and the sentence says the old versions are not all erased. One that ran
    /// through is. The refusal after a damaged pass names the first files that could not be read, and the JSON lists them.
    /// RED on the old output: `ok: true` and exit 0 for all three; no file named.
    #[cfg(feature = "encryption")]
    #[test]
    fn x_delta_sync8_3_a_purge_that_stopped_or_failed_is_no_success_and_a_refusal_names_the_files()
    {
        use azul_storage::{
            meta::{
                reclaim::{ReclaimMode, ReclaimReport},
                NotYet,
            },
            rotation::ReencryptState,
        };

        let state = ReencryptState::new(0);
        let report = |shape: &dyn Fn(&mut ReclaimReport)| {
            let mut report = ReclaimReport::none(ReclaimMode::Delete);
            shape(&mut report);
            reencrypt_output("d_1", true, &state, Some(Ok(report)))
        };
        let (value, text) = report(&|r| r.stopped = Some(NotYet::Lease));
        assert_eq!(
            value["ok"],
            json!(false),
            "a purge that stopped on the lease: {text}"
        );
        assert_eq!(value["erased"]["complete"], json!(false));
        assert!(
            text.contains("NOT all erased") && text.contains("it stopped"),
            "{text}"
        );
        let (value, text) = report(&|r| r.failed = 2);
        assert_eq!(
            value["ok"],
            json!(false),
            "deletes the storage refused: {text}"
        );
        assert!(text.contains("refused 2 deletes"), "{text}");
        let (value, text) = report(&|r| r.capped = true);
        assert_eq!(value["ok"], json!(false), "a cap: {text}");
        let (value, text) = report(&|_| {});
        assert_eq!(
            value["ok"],
            json!(true),
            "a purge that ran through is a success: {text}"
        );
        assert!(!text.contains("NOT all erased"), "{text}");

        // The refusal after a damaged pass names the files (the first few, and how many more).
        let mut damaged = ReencryptState::new(0);
        damaged.failed = 3;
        damaged.failed_paths = vec![String::from("docs/a.docx"), String::from("b.bin")];
        let refused = String::from("nothing was erased from drive d_1: 3 files could not be read");
        let (value, text) = reencrypt_output("d_1", true, &damaged, Some(Err(refused)));
        assert_eq!(value["ok"], json!(false));
        assert_eq!(value["erased"]["damaged"], json!(["docs/a.docx", "b.bin"]));
        assert!(
            text.contains("docs/a.docx, b.bin") && text.contains("and 1 more"),
            "{text}"
        );
    }

    /// X-DELTA-SYNC8-8: `run` is the one place the refusal of `--dry-run` is called, and the test enters `run` - not the helper:
    /// the refusal comes before anything is read or written, so a transport that fails every call is never asked. RED when the
    /// call in `run` is taken out: the command goes on to the account, and the error is another one.
    #[test]
    fn x_delta_sync8_8_run_refuses_a_dry_run_the_command_does_not_read_before_it_asks_the_network()
    {
        use azul_storage::transport::{HttpCall, HttpReply};

        struct Down;
        impl Transport for Down {
            fn send(&self, _call: &HttpCall) -> std::result::Result<HttpReply, String> {
                Err(String::from("the network must not be asked"))
            }
        }
        let net = Net {
            transports: Arc::new(|| Box::new(Down) as Box<dyn Transport>),
            dialer: None,
        };
        let args = Args::parse(&argv("reencrypt --erase-old-versions --yes --dry-run")).unwrap();
        let refused = run(&args, &net).expect_err("refused").to_string();
        assert!(
            refused.contains("has no --dry-run") && refused.contains("nothing was done"),
            "{refused}"
        );
    }

    /// A run of `azcloud reencrypt` that writes down what it was asked and in what order.
    #[cfg(feature = "encryption")]
    struct Recording {
        log: std::cell::RefCell<Vec<String>>,
        /// Whether the pass ends (false: it was stopped).
        over: bool,
        /// How many files the pass could not read.
        damaged: u64,
    }

    #[cfg(feature = "encryption")]
    impl Reencrypting for Recording {
        fn pass(&self, state: &mut azul_storage::rotation::ReencryptState) -> Result<bool> {
            self.log.borrow_mut().push(format!(
                "pass, confirmed: {}",
                state.purge_answered_at().is_some()
            ));
            state.failed = self.damaged;
            state.packs_done = self.over;
            Ok(self.over)
        }

        fn erase(
            &self,
            state: &azul_storage::rotation::ReencryptState,
        ) -> std::result::Result<azul_storage::meta::reclaim::ReclaimReport, String> {
            self.log.borrow_mut().push(String::from("erase"));
            match state.erase_licence() {
                Ok(_) => Ok(azul_storage::meta::reclaim::ReclaimReport::none(
                    azul_storage::meta::reclaim::ReclaimMode::Delete,
                )),
                Err(why) => Err(format!("nothing was erased: {why}")),
            }
        }
    }

    /// X-DELTA-SYNC7-5: the ORDER of `azcloud reencrypt`, entered through the function the command calls. The
    /// confirmation is recorded before the pass when the user asked for the erase, and only then; the erase is
    /// asked after the pass and only when the pass is over; a pass that left damaged files is answered by the
    /// licence's refusal, which the command prints after its own line. RED on a command that erased before the
    /// pass, or after a stopped one, or confirmed afterwards: the log differs.
    #[cfg(feature = "encryption")]
    #[test]
    fn x_delta_sync7_5_reencrypt_records_the_confirmation_first_and_erases_only_after_a_finished_pass(
    ) {
        let run = |erase: bool, over: bool, damaged: u64| {
            let recording = Recording {
                log: std::cell::RefCell::new(Vec::new()),
                over,
                damaged,
            };
            let (state, done, erased) = reencrypt_sequence(&recording, erase, 1_000).unwrap();
            (recording.log.into_inner(), state, done, erased)
        };

        let (log, state, done, erased) = run(true, true, 0);
        assert_eq!(
            log,
            ["pass, confirmed: true", "erase"],
            "confirm, pass, erase"
        );
        assert!(done && matches!(erased, Some(Ok(_))));
        assert_eq!(state.purge_answered_at(), Some(1_000));

        // No words: no confirmation, no erase.
        let (log, state, _, erased) = run(false, true, 0);
        assert_eq!(log, ["pass, confirmed: false"]);
        assert!(erased.is_none() && state.purge_answered_at().is_none());

        // A pass that was stopped erases nothing.
        let (log, _, done, erased) = run(true, false, 0);
        assert_eq!(
            log,
            ["pass, confirmed: true"],
            "the erase was not asked of a stopped pass"
        );
        assert!(!done && erased.is_none());

        // A pass that left damaged files: the erase is asked, the licence refuses, and the sentence says so.
        let (log, state, _, erased) = run(true, true, 2);
        assert_eq!(log, ["pass, confirmed: true", "erase"]);
        let Some(Err(refused)) = erased.clone() else {
            panic!("the erase must be refused after damage: {erased:?}");
        };
        assert!(
            refused.starts_with("nothing was erased") && refused.contains("2 files"),
            "{refused}"
        );
        let (value, text) = reencrypt_output("d_1", true, &state, Some(Err(refused)));
        assert_eq!(value["ok"], json!(false));
        assert!(
            text.lines()
                .next()
                .is_some_and(|l| l.contains("2 damaged ones left as they were")),
            "{text}"
        );
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

    /// X-AR5b-1: `sync` and `share` of the command line store nothing in the clear any more: they
    /// are refused whatever the drive is, and the flags only they took are gone.
    #[test]
    fn x_ar5b_1_sync_and_share_are_refused_and_their_flags_are_gone() {
        for command in ["sync", "share"] {
            let refused = cmd_not_through_the_encryption(command)
                .unwrap_err()
                .to_string();
            assert!(refused.starts_with(command), "{refused}");
            assert!(
                refused.contains("does not go through the encryption"),
                "{refused}"
            );
        }
        for flag in [
            "--exclude",
            "--allow-mass-delete",
            "--allow-burst",
            "--prefix",
            "--expires",
        ] {
            assert!(
                Args::parse(&argv(&format!("sync ./notes {flag}"))).is_err(),
                "{flag}"
            );
        }
        assert!(
            !USAGE.contains("--allow-mass-delete"),
            "the usage still offers a plaintext sync"
        );
    }

    /// AZ-K16: a join or recovery code in the arguments shows in ps and in the shell's history, so
    /// it is refused (and not quoted back); a file or stdin is read.
    #[test]
    fn az_k16_a_code_in_the_arguments_is_refused_and_a_file_is_read() {
        let on_argv = Args::parse(&argv("join dt_f.0.the-secret")).unwrap();
        let refused = code_argument(&on_argv, "join", "join")
            .unwrap_err()
            .to_string();
        assert!(refused.contains("--code-file"), "{refused}");
        assert!(
            !refused.contains("the-secret"),
            "the code is not said back: {refused}"
        );
        let none = Args::parse(&argv("recover")).unwrap();
        assert!(code_argument(&none, "recovery", "recover").is_err());

        let dir = std::env::temp_dir().join(format!("azcloud-code-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("code.txt");
        std::fs::write(&file, "the code\n").unwrap();
        let from_file = Args::parse(&[
            String::from("join"),
            String::from("--code-file"),
            file.display().to_string(),
        ])
        .unwrap();
        assert_eq!(
            code_argument(&from_file, "join", "join").unwrap().trim(),
            "the code"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// The packs' upkeep takes a grace period in days, and the command line's encrypted drives
    /// pack their small files, the journal in the state folder's drive index.
    #[cfg(feature = "encryption")]
    #[test]
    fn the_packs_command_takes_a_grace_period_and_the_drives_pack_small_files() {
        let a = Args::parse(&argv("packs --grace-days 0")).unwrap();
        assert_eq!(a.number::<u64>("--grace-days").unwrap(), Some(0));
        assert!(USAGE.contains("packs [--grace-days D]"));
        let dir = std::env::temp_dir().join(format!("azcloud-packs-{}", std::process::id()));
        let state = StateDir::open(&dir).unwrap();
        let provider = index_provider(
            &state,
            "laptop",
            azul_storage::meta::RestoreSource::none("a test has no token server to ask"),
        );
        let packing = provider.packing("d_1").expect("small files are packed");
        let journal = packing.journal.expect("a journal beside the drive index");
        assert!(journal.starts_with(dir.join("drive-index")), "{journal:?}");
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// X-DELTA-AZDRIVE-SYNC-3 (AZ-M4). The command line keeps a copy of the drive index in its state
    /// folder, and left the restore announcement out: a drive the operator restored from its
    /// off-site copy was refused as a rollback for good. The provider asks the source it was given,
    /// and the account's source gives what its last sign-in recorded. RED not run: the old
    /// `index_provider(state, device)` took no source and asked nothing.
    #[cfg(feature = "encryption")]
    #[test]
    fn x_delta_azdrive_sync_3_the_command_lines_provider_asks_the_accounts_restore_announcement() {
        use std::sync::atomic::{AtomicUsize, Ordering};
        let dir = std::env::temp_dir().join(format!("azcloud-restore-{}", std::process::id()));
        let state = StateDir::open(&dir).unwrap();
        let asked = Arc::new(AtomicUsize::new(0));
        let counter = Arc::clone(&asked);
        let provider = index_provider(
            &state,
            "laptop",
            azul_storage::meta::RestoreSource::new(move || {
                counter.fetch_add(1, Ordering::SeqCst);
                None
            }),
        );
        let bucket: Arc<dyn azul_storage::Drive> = Arc::new(azul_storage::meta::MemoryBucket::new());
        let key = azul_storage::crypto::DriveKey::generate().unwrap();
        provider.open_index("d_restore", bucket, &key).unwrap();
        assert!(
            asked.load(Ordering::SeqCst) > 0,
            "the index asked where the drive's restore stands"
        );
        let notice = azcloud_kit::sync::RestoreNotice {
            epoch: 2,
            ..azcloud_kit::sync::RestoreNotice::default()
        };
        assert_eq!(
            announced_restore(Some(notice.clone())).notice(),
            Some(notice),
            "the account's last announcement"
        );
        assert!(announced_restore(None).is_set(), "a source that has announced nothing");
        assert_eq!(announced_restore(None).notice(), None);
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// `azcloud down` of an encrypted drive's file gives back its bytes: a small one (in a
    /// pack) as well as a big one (in segments of its own object).
    #[cfg(feature = "encryption")]
    #[test]
    fn down_saves_a_packed_file_and_a_segmented_one_of_an_encrypted_drive() {
        use azul_storage::{
            crypto::DriveKey,
            encrypted::{EncryptedDrive, Expect, MemoryIndex},
            local::LocalDrive,
            packing::PackOptions,
        };
        let dir = std::env::temp_dir().join(format!("azcloud-down-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("bucket")).unwrap();
        let drive = EncryptedDrive::new(
            LocalDrive::new(dir.join("bucket")),
            DriveKey::generate().unwrap(),
            Arc::new(MemoryIndex::new()),
        )
        .with_packing(PackOptions::default());
        let small: Vec<u8> = (0..64 * 1024).map(|i| (i % 251) as u8).collect();
        let big: Vec<u8> = (0..3 * 1024 * 1024).map(|i| (i % 241) as u8).collect();
        drive.write("e2e/small.bin", &small, Expect::Any).unwrap();
        drive.write("e2e/big.bin", &big, Expect::Any).unwrap();
        for (key, bytes) in [("e2e/small.bin", &small), ("e2e/big.bin", &big)] {
            let file = dir.join(key.replace('/', "-"));
            let written = save_encrypted(&drive, key, &file)
                .unwrap_or_else(|e| panic!("azcloud down {key}: {e:#}"));
            assert_eq!(written, bytes.len() as u64, "{key}");
            assert!(std::fs::read(&file).unwrap() == *bytes, "{key}: other bytes");
        }
        assert!(
            save_encrypted(&drive, "e2e/none.bin", &dir.join("none")).is_err(),
            "a file the drive does not have"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// X-DELTA-KEYS2-4: `azcloud rotate --yes --out F` locked the drive down and replaced its recovery
    /// wrap, then wrote the code to F - and a path that could not be written ended the command with
    /// the old code dead and the new one never seen. The file is made first, before the first call of
    /// the token server or the bucket: a path that cannot be written is refused with nothing done, an
    /// empty file made for a command that made no code goes again, and a file that was there stays as
    /// it is.
    #[cfg(feature = "encryption")]
    #[test]
    fn x_delta_keys2_4_an_unwritable_out_path_stops_the_command_before_its_first_call() {
        use azul_storage::testing::TempDir;
        let dir = TempDir::new("code-out");
        // Below a file: no folder can be made for it.
        let file = dir.path().join("a-file");
        std::fs::write(&file, "x").unwrap();
        assert!(
            prepare_code_file(Some(file.join("code.txt"))).is_err(),
            "refused at once"
        );
        assert!(
            prepare_code_file(None).unwrap().is_none(),
            "no path, no file"
        );

        let path = dir.path().join("code.txt");
        let made = prepare_code_file(Some(path.clone()))
            .unwrap()
            .expect("a file");
        assert_eq!(std::fs::metadata(&path).unwrap().len(), 0, "empty");
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = std::fs::metadata(&path).unwrap().permissions().mode();
            assert_eq!(mode & 0o077, 0, "this user's only: {mode:o}");
        }
        drop(made);
        assert!(
            !path.exists(),
            "the command made no code: the file goes again"
        );

        std::fs::write(&path, "older").unwrap();
        drop(prepare_code_file(Some(path.clone())).unwrap());
        assert_eq!(
            std::fs::read_to_string(&path).unwrap(),
            "older",
            "what was there stays"
        );
    }

    /// X-DELTA-KEYS2-4: the code made after the wrap is shown even when its file cannot be written
    /// any more: in the JSON and the text, with the file's error as the warning beside it - the code
    /// exists nowhere else.
    #[cfg(feature = "encryption")]
    #[test]
    fn x_delta_keys2_4_a_code_that_cannot_be_written_to_its_file_is_shown_instead() {
        use azul_storage::{crypto::keys::RecoveryCode, testing::TempDir};
        let dir = TempDir::new("code-shown");
        let path = dir.path().join("code.txt");
        let file = prepare_code_file(Some(path.clone()))
            .unwrap()
            .expect("a file");
        // The file is not writable by the time the code is there: a folder stands in its place.
        std::fs::remove_file(&path).unwrap();
        std::fs::create_dir(&path).unwrap();
        let code = RecoveryCode::generate().unwrap();
        let (mut value, mut text) = (json!({"ok": true}), String::new());
        give_recovery_code(&code, Some(&file), &mut value, &mut text);
        assert_eq!(value["recovery_code"], json!(code.to_text().as_str()));
        assert!(value["recovery_code_file_error"].is_string(), "{value}");
        assert!(value.get("recovery_code_file").is_none(), "{value}");
        assert!(
            text.contains("warning: the recovery code could not be written"),
            "{text}"
        );
        assert!(
            text.contains(code.to_text().as_str()),
            "the code is on the screen: {text}"
        );

        // The file that takes it: the code is in it and not on the screen.
        let good = dir.path().join("good.txt");
        let file = prepare_code_file(Some(good.clone()))
            .unwrap()
            .expect("a file");
        let (mut value, mut text) = (json!({"ok": true}), String::new());
        give_recovery_code(&code, Some(&file), &mut value, &mut text);
        assert_eq!(
            std::fs::read_to_string(&good).unwrap(),
            format!("{}\n", code.to_text().as_str())
        );
        assert_eq!(
            value["recovery_code_file"],
            json!(good.display().to_string())
        );
        assert!(value.get("recovery_code").is_none() && !text.contains(code.to_text().as_str()));
        drop(file);
        assert!(good.exists(), "a file with the code in it stays");
    }
}
