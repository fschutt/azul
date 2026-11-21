//! `azul-bridge`'s command line.
//!
//! ```text
//! azul-bridge [--state-dir DIR] [--token-url URL] [--s3-url URL] [--keyring os|file] <command> [options]
//!
//!   init --address ADDR [--account ID] [--alias ADDR]... [--sending FILE]
//!        [--imap-port N] [--smtp-port N] [--dav-port N] [--pim-port N]
//!        The settings (bridge.json) and, the first time, the bridge's password - printed once
//!        (`AZUL_BRIDGE_PASSWORD <password>`), then kept in the secret store.
//!   password   A new password, printed once; the old one stops working.
//!   signup [--name NAME] [--tier TIER] [--recovery-out FILE]
//!        A development drive of the bridge's own (development token servers only), encrypted as
//!        it is made: its recovery code is printed once (`AZUL_BRIDGE_RECOVERY_CODE <code>`), the
//!        keys kept in the secret store. `--recovery-out` makes FILE (readable by you only) before
//!        the drive is made and puts the code into it instead (`AZUL_BRIDGE_RECOVERY_CODE_FILE
//!        <path>`); a file that cannot be written then does not lose the code: it is printed.
//!   join --code-file FILE
//!        Joins a drive with a code from `azcloud invite` (a token family of the bridge's own).
//!   serve [--imap-port N] [--smtp-port N] [--dav-port N] [--pim-port N] [--folder DIR | --memory]
//!         [--calendar-folder DIR] [--idle-poll SECS]
//!        Serves IMAP, SMTP submission, WebDAV and CalDAV / CardDAV on 127.0.0.1 (port 0: any free
//!        one) and prints `AZUL_BRIDGE_READY imap=<port> smtp=<port> dav=<port> pim=<port>`.
//!        `--folder` serves a folder of this computer as the drive, `--memory` an empty drive in
//!        memory (development). The calendars are AzCalendar's files in the drive's `calendar/`,
//!        or in the folder `--calendar-folder` names (AzCalendar's data folder on this computer).
//!   status     What the bridge is set up with (no secret).
//!   autostart enable|disable|status
//!        The login item that starts `serve` at every login ([`crate::autostart`]).
//! ```
//!
//! The state folder: `--state-dir`, else `$AZUL_BRIDGE_HOME`, else `<OS config folder>/azul-bridge`
//! ([`crate::config`]). The token server: a shipped bridge talks to the live one
//! ([`azul_appkit::anchors::TOKEN_SERVER`]) and ignores `--token-url`, `--s3-url`,
//! `$AZLIN_TOKEN_URL` and the shared config's endpoints; only a TEST build (cargo feature
//! `test-hooks`, the E2E stacks) takes the address from them, in that order (azcloud-kit's
//! settings, as the `azcloud` command line resolves it). The secrets
//! (`--keyring`): `os`, the OS keyring (a build with the `os-keyring` feature, the default
//! there), or `file`, the state folder's 0600 file; `init` records the choice in `bridge.json`.

use std::{
    io::Write,
    path::{Path, PathBuf},
    sync::Arc,
    time::Duration,
};

use azcloud_kit::{drive::TransportFactory, Account, JoinCode, StateDir};
use azul_appkit::azlin_config::{Endpoint, EndpointFlags};
use azul_storage::{footprint::Allow, Drive, LocalDrive, ScopedDrive, Transport};

use crate::{
    account::AccountDrive,
    autostart,
    auth::{self, Credentials, FailureGate},
    config::{self, BridgeConfig},
    dav::{self, Dav, SwapRecords},
    imap::{self, Imap},
    limits::Limits,
    memory::MemoryDrive,
    net,
    pim::{self, Names, Pim},
    secrets::{self, FileKeyring, KeyringChoice, KeyringStore, PASSWORD_ENTRY},
    sender::AzMailSubmitter,
    sent::SentRegistry,
    smtp::{self, Smtp},
    store::{DriveMailStore, MailStore},
    uids::{UidMaps, UIDS_DIR},
};

/// How often queued mail is tried again.
pub const RETRY_EVERY: Duration = Duration::from_secs(300);

/// What the command line said.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Options {
    pub command: String,
    pub state_dir: Option<PathBuf>,
    pub token_url: Option<String>,
    pub s3_url: Option<String>,
    pub address: Option<String>,
    pub account: Option<String>,
    pub aliases: Vec<String>,
    pub sending: Option<PathBuf>,
    pub code_file: Option<PathBuf>,
    /// Where `signup` puts the new recovery code (made before the drive is).
    pub recovery_out: Option<PathBuf>,
    pub name: Option<String>,
    pub tier: Option<String>,
    pub imap_port: Option<u16>,
    pub smtp_port: Option<u16>,
    pub dav_port: Option<u16>,
    pub pim_port: Option<u16>,
    pub folder: Option<PathBuf>,
    /// AzCalendar's data folder on this computer, instead of the drive's `calendar/`.
    pub calendar_folder: Option<PathBuf>,
    pub memory: bool,
    /// Seconds between IMAP IDLE's looks at the drive (the default: 30).
    pub idle_poll: Option<u64>,
    /// `os` (the OS keyring; a build with the os-keyring feature) or `file`.
    pub keyring: Option<String>,
    /// The user's word that the secrets file may hold the drive's keys (`--allow-plain-secrets`;
    /// `init` writes it into bridge.json, so the later runs need not repeat it).
    pub allow_plain_secrets: bool,
    /// `autostart`'s `enable` / `disable` / `status`.
    pub action: Option<String>,
}

/// The usage text.
pub const USAGE: &str = "usage: azul-bridge [--state-dir DIR] [--token-url URL] [--s3-url URL] [--keyring os|file] \
     [--allow-plain-secrets] <init --address ADDR [--account ID] [--alias ADDR]... [--sending FILE] [--imap-port N] \
     [--smtp-port N] [--dav-port N] [--pim-port N] | password | signup [--name NAME] [--tier TIER] \
     [--recovery-out FILE] | \
     join --code-file FILE | autostart enable|disable|status | serve [--imap-port N] [--smtp-port N] [--dav-port N] \
     [--pim-port N] [--folder DIR | --memory] [--calendar-folder DIR] [--idle-poll SECS] | status>";

/// Reads the arguments (without the program's name).
///
/// # Errors
///
/// What was wrong, for the usage message.
pub fn parse_args(args: &[String]) -> Result<Options, String> {
    let mut options = Options::default();
    let mut i = 0;
    let value = |i: &mut usize, flag: &str| -> Result<String, String> {
        *i += 1;
        args.get(*i)
            .cloned()
            .ok_or_else(|| format!("{flag} needs a value"))
    };
    let port = |text: String, flag: &str| -> Result<u16, String> {
        text.parse::<u16>()
            .map_err(|_| format!("{flag} takes a port number (0 to 65535)"))
    };
    while i < args.len() {
        let arg = args[i].as_str();
        match arg {
            "--state-dir" => options.state_dir = Some(PathBuf::from(value(&mut i, arg)?)),
            "--token-url" => options.token_url = Some(value(&mut i, arg)?),
            "--s3-url" => options.s3_url = Some(value(&mut i, arg)?),
            "--address" => options.address = Some(value(&mut i, arg)?),
            "--account" => options.account = Some(value(&mut i, arg)?),
            "--alias" => options.aliases.push(value(&mut i, arg)?),
            "--sending" => options.sending = Some(PathBuf::from(value(&mut i, arg)?)),
            "--code-file" => options.code_file = Some(PathBuf::from(value(&mut i, arg)?)),
            "--recovery-out" => options.recovery_out = Some(PathBuf::from(value(&mut i, arg)?)),
            "--name" => options.name = Some(value(&mut i, arg)?),
            "--tier" => options.tier = Some(value(&mut i, arg)?),
            "--imap-port" => options.imap_port = Some(port(value(&mut i, arg)?, arg)?),
            "--smtp-port" => options.smtp_port = Some(port(value(&mut i, arg)?, arg)?),
            "--dav-port" => options.dav_port = Some(port(value(&mut i, arg)?, arg)?),
            "--pim-port" => options.pim_port = Some(port(value(&mut i, arg)?, arg)?),
            "--folder" => options.folder = Some(PathBuf::from(value(&mut i, arg)?)),
            "--calendar-folder" => options.calendar_folder = Some(PathBuf::from(value(&mut i, arg)?)),
            "--memory" => options.memory = true,
            // Every drive is encrypted as it is made: there is no plaintext one to ask for.
            "--plaintext" => {
                return Err(String::from(
                    "--plaintext is gone: every drive is encrypted as it is made",
                ))
            }
            "--keyring" => options.keyring = Some(value(&mut i, arg)?),
            "--allow-plain-secrets" => options.allow_plain_secrets = true,
            "--idle-poll" => {
                let text = value(&mut i, arg)?;
                let secs = text
                    .parse::<u64>()
                    .ok()
                    .filter(|s| (1..=3600).contains(s))
                    .ok_or_else(|| format!("{arg} takes seconds from 1 to 3600"))?;
                options.idle_poll = Some(secs);
            }
            "-h" | "--help" => return Err(String::from("help")),
            other if other.starts_with('-') => return Err(format!("unknown option {other}")),
            word if options.command == "autostart" && options.action.is_none() => {
                if !matches!(word, "enable" | "disable" | "status") {
                    return Err(format!("autostart takes enable, disable or status, not {word}"));
                }
                options.action = Some(word.to_string());
            }
            command => {
                if !options.command.is_empty() {
                    return Err(format!("one command at a time ({} and {command})", options.command));
                }
                if !matches!(
                    command,
                    "init" | "password" | "signup" | "join" | "serve" | "status" | "autostart"
                ) {
                    return Err(format!("unknown command {command}"));
                }
                options.command = command.to_string();
            }
        }
        i += 1;
    }
    if options.command.is_empty() {
        return Err(String::from("a command is needed"));
    }
    if options.command == "init" && options.address.is_none() {
        return Err(String::from("init needs --address (the address the mail programs sign in with)"));
    }
    if options.command == "autostart" && options.action.is_none() {
        return Err(String::from("autostart needs enable, disable or status"));
    }
    if options.command == "join" && options.code_file.is_none() {
        return Err(String::from("join needs --code-file (the join code, in a file only you can read)"));
    }
    if options.memory && options.folder.is_some() {
        return Err(String::from("--folder or --memory, not both"));
    }
    Ok(options)
}

/// Every transport of the run, trusting `roots` (the shared config's set).
fn transports(roots: &ureq::tls::RootCerts) -> TransportFactory {
    let roots = roots.clone();
    Arc::new(move || {
        Box::new(crate::transport::UreqTransport::with_root_certs(roots.clone()))
            as Box<dyn Transport>
    })
}

/// The roots every connection of the run trusts, PEM: the shared config's `trust` section
/// (the public roots when it names none).
///
/// # Errors
///
/// A root the section names that cannot be read or does not parse: the run stops rather
/// than trust more than the user asked for.
fn trusted_roots(options: &Options) -> Result<Vec<String>, String> {
    settings(options).trust?.certificates_pem()
}

/// [`trusted_roots`] as ureq's root store.
fn root_certs(options: &Options) -> Result<ureq::tls::RootCerts, String> {
    crate::transport::root_certs(&trusted_roots(options)?)
}

/// The settings of this run, as azcloud-kit resolves them.
fn settings(options: &Options) -> azcloud_kit::Settings {
    let flags = azcloud_kit::Flags {
        endpoints: EndpointFlags {
            token: options.token_url.clone(),
            s3: options.s3_url.clone(),
            ..EndpointFlags::default()
        },
        ..azcloud_kit::Flags::default()
    };
    let os = azcloud_kit::OsDirs {
        home: dirs::home_dir(),
        config: dirs::config_dir(),
        data: dirs::data_dir(),
    };
    azcloud_kit::Settings::from_process(&flags, &os)
}

/// The token server and an S3 override, as azcloud-kit's settings resolve them.
fn endpoints(options: &Options) -> (Option<String>, Option<String>) {
    let settings = settings(options);
    (
        settings.token_url().ok().map(str::to_string),
        settings.endpoints.url(Endpoint::S3).map(str::to_string),
    )
}

/// The drive's iroh dialer: azul's iroh endpoint when the bridge links libazul (`os-keyring`,
/// `tray`); none otherwise - HTTPS with the failover, connect-by-IP included.
/// `AZCLOUD_TRANSPORT=https` keeps to HTTPS.
fn iroh_dialer() -> Option<Arc<dyn azcloud_kit::IrohDialer>> {
    let asked = std::env::var("AZCLOUD_TRANSPORT").ok();
    if asked.as_deref().and_then(azcloud_kit::TransportPref::parse)
        == Some(azcloud_kit::TransportPref::Https)
    {
        return None;
    }
    #[cfg(feature = "os-keyring")]
    let dialer: Option<Arc<dyn azcloud_kit::IrohDialer>> = Some(Arc::new(
        |target: &azcloud_kit::IrohTarget, relay: Option<&str>| {
            azul_storage::azul_iroh::dial(&target.id, &target.addrs, relay)
        },
    ));
    #[cfg(not(feature = "os-keyring"))]
    let dialer: Option<Arc<dyn azcloud_kit::IrohDialer>> = None;
    dialer
}

/// The new recovery code to the user, once: into the file `--recovery-out` named (made before the drive
/// was) or, without one - or when it cannot be written, which does not lose the code: it exists
/// nowhere else - as the line `AZUL_BRIDGE_RECOVERY_CODE <code>`.
#[cfg(feature = "encryption")]
fn give_recovery_code(
    code: &azul_storage::crypto::keys::RecoveryCode,
    out: Option<&azcloud_kit::state::OutFile>,
) {
    let text = code.to_text();
    if let Some(file) = out {
        match file.write(format!("{}\n", text.as_str()).as_bytes()) {
            Ok(()) => {
                say(&format!(
                    "AZUL_BRIDGE_RECOVERY_CODE_FILE {}",
                    file.path().display()
                ));
                return;
            }
            Err(e) => say(&format!(
                "AZUL_BRIDGE_RECOVERY_CODE_FILE_FAILED {}: {e}",
                file.path().display()
            )),
        }
    }
    say(&format!("AZUL_BRIDGE_RECOVERY_CODE {}", text.as_str()));
}

fn say(line: &str) {
    let mut out = std::io::stdout();
    let _ = writeln!(out, "{line}");
    let _ = out.flush();
}

/// What every run opens first: the state folder, its device state, the keyring chosen and its
/// secrets (`azul-bridge-tray` opens the same before it starts `serve`).
pub struct Opened {
    pub state: PathBuf,
    pub state_dir: StateDir,
    pub choice: KeyringChoice,
    pub secrets: Arc<dyn KeyringStore>,
}

/// The state folder (`--state-dir`, `$AZUL_BRIDGE_HOME`, the OS config folder) and the secrets:
/// the keyring of the flag, else the one the bridge was set up with, else this build's.
///
/// # Errors
///
/// No state folder, a state folder that cannot be opened, a keyring that is not there.
pub fn open_state(options: &Options) -> Result<Opened, String> {
    let state = config::state_dir(
        options.state_dir.clone(),
        std::env::var(config::HOME_VAR).ok(),
        dirs::config_dir(),
    )
    .ok_or("no state folder: pass --state-dir or set AZUL_BRIDGE_HOME")?;
    let state_dir = StateDir::open(&state).map_err(|e| e.to_string())?;
    // A settings file that cannot be read is said and does not stop the run (`init` writes it
    // anew): the keyring is the build's and the plain file takes no key until the flag says so.
    let config = match BridgeConfig::load(&state) {
        Ok(config) => config,
        Err(e) => {
            say(&format!("AZUL_BRIDGE_CONFIG_UNREADABLE {e}"));
            None
        }
    };
    let choice = match &options.keyring {
        Some(text) => KeyringChoice::parse(text).ok_or("--keyring takes os or file")?,
        None => config
            .as_ref()
            .and_then(|config| KeyringChoice::parse(&config.keyring))
            .unwrap_or_else(KeyringChoice::default_for_build),
    };
    // The secrets file keeps the drive's keys only when the user said it may (AZ-K7): the flag of
    // this run, or the setting `init --allow-plain-secrets` wrote down.
    let allow = if options.allow_plain_secrets || config.as_ref().is_some_and(|c| c.plain_secrets)
    {
        Allow::PlainSecrets
    } else {
        Allow::Nothing
    };
    let state_dir = state_dir.allowing_plain_secrets(allow);
    let secrets: Arc<dyn KeyringStore> = secrets::open(choice, state_dir.secrets())?;
    Ok(Opened {
        state,
        state_dir,
        choice,
        secrets,
    })
}

/// Runs the command.
///
/// # Errors
///
/// What went wrong, as a sentence (never a secret).
pub fn run(options: &Options) -> Result<(), String> {
    let Opened {
        state,
        state_dir,
        choice,
        secrets,
    } = open_state(options)?;
    match options.command.as_str() {
        "init" => init(options, &state, &*secrets, choice),
        "password" => new_password(&*secrets),
        "signup" => {
            let (token_url, _) = endpoints(options);
            let token_url = token_url.ok_or("no token server: pass --token-url or set AZLIN_TOKEN_URL")?;
            let tier = options
                .tier
                .clone()
                .unwrap_or_else(|| azcloud_kit::token::DEFAULT_TIER.to_string());
            let name = options.name.clone().unwrap_or_else(|| String::from("Azlin Bridge"));
            let transports = transports(&root_certs(options)?);
            // Encrypted as it is made: the keys go into the bucket at this run's S3 endpoint,
            // the drive key and the bridge's member key into its secret store. A build without
            // encryption makes no drive (the kit refuses, saying why).
            #[cfg(feature = "encryption")]
            {
                // Before the first call of the token server (X-DELTA-KEYS2-4): a path that cannot
                // be written stops the command while no drive was made.
                let out_file = options
                    .recovery_out
                    .as_deref()
                    .map(|path| {
                        azcloud_kit::state::OutFile::prepare(path)
                            .map_err(|e| format!("{}: {e}", path.display()))
                    })
                    .transpose()?;
                let s3_url = endpoints(options).1;
                let keys = azcloud_kit::encryption::NewKeys {
                    keyring: &*secrets,
                    s3_endpoint: s3_url.as_deref(),
                    kdf: azul_storage::crypto::keys::RecoveryKdf::fresh()
                        .map_err(|e| e.to_string())?,
                };
                let (account, code) =
                    Account::signup(&state_dir, &token_url, transports, &tier, &name, keys)
                        .map_err(|e| e.to_string())?;
                say(&format!("AZUL_BRIDGE_DRIVE {}", account.record().id));
                give_recovery_code(&code, out_file.as_ref());
            }
            #[cfg(not(feature = "encryption"))]
            {
                let account = Account::signup(&state_dir, &token_url, transports, &tier, &name)
                    .map_err(|e| e.to_string())?;
                say(&format!("AZUL_BRIDGE_DRIVE {}", account.record().id));
            }
            Ok(())
        }
        "join" => {
            let (token_url, _) = endpoints(options);
            let token_url = token_url.ok_or("no token server: pass --token-url or set AZLIN_TOKEN_URL")?;
            let path = options.code_file.clone().unwrap_or_default();
            let text = std::fs::read_to_string(&path)
                .map_err(|e| format!("{}: {e}", path.display()))?;
            let code = JoinCode::decode(text.trim()).map_err(|e| e.to_string())?;
            let transports = transports(&root_certs(options)?);
            // Before the code is spent: a secret store that keeps no key refuses the code's key
            // afterwards (X-DELTA-AZDRIVE-KEYS-25).
            #[cfg(feature = "encryption")]
            Account::check_join_key_can_be_kept(&code, &*secrets).map_err(|e| e.to_string())?;
            let account = Account::join(&state_dir, &token_url, transports, &code)
                .map_err(|e| e.to_string())?;
            say(&format!("AZUL_BRIDGE_DRIVE {}", account.record().id));
            // An encrypted drive's key rides in the code (sealed to a one-time key): the bridge
            // keeps it in its keyring and enrols itself, as every other device does.
            #[cfg(feature = "encryption")]
            if let Some(adopted) = account
                .adopt_join_key_reporting(&code, &*secrets)
                .map_err(|e| e.to_string())?
            {
                say("AZUL_BRIDGE_DRIVE_KEY kept");
                // The bridge has the key; a join code whose key file could not be deleted from
                // the drive still opens the drive key, and that is said (X-AR5b-9).
                if let Some(why) = &adopted.invite_left {
                    say(&format!(
                        "AZUL_BRIDGE_DRIVE_INVITE_LEFT the join code's key could not be deleted \
                         from the drive ({why}): the code still opens the drive key until the \
                         keys are replaced"
                    ));
                }
            }
            Ok(())
        }
        "autostart" => autostart(options.action.as_deref().unwrap_or("status"), &state),
        "status" => {
            let config = BridgeConfig::load(&state).map_err(|e| e.to_string())?;
            match config {
                Some(config) => say(&format!(
                    "address={} account={} imap={} smtp={} dav={} pim={} state={}",
                    config.address,
                    config.account,
                    config.imap_port,
                    config.smtp_port,
                    config.dav_port,
                    config.pim_port,
                    state.display()
                )),
                None => say("not set up: azul-bridge init --address <your address>"),
            }
            let has_password = secrets.get(PASSWORD_ENTRY).map_err(|e| e.to_string())?.is_some();
            say(&format!("password={}", if has_password { "set" } else { "none" }));
            match crate::single::running(&state) {
                Some(running) => say(&format!(
                    "running=process {} imap={} smtp={} dav={} pim={}",
                    running.pid, running.imap, running.smtp, running.dav, running.pim
                )),
                None => say("running=no"),
            }
            Ok(())
        }
        "serve" => serve(options, &state, &state_dir, secrets),
        other => Err(format!("unknown command {other}")),
    }
}

/// `autostart enable|disable|status`: the login item of this system ([`crate::autostart`]).
fn autostart(action: &str, state: &Path) -> Result<(), String> {
    let system = autostart::System::current().ok_or("this system has no login items the bridge knows")?;
    let home = dirs::home_dir().ok_or("no home folder")?;
    let config = dirs::config_dir().ok_or("no config folder")?;
    match action {
        "enable" => {
            let binary = std::env::current_exe().map_err(|e| format!("where this program is: {e}"))?;
            let path = autostart::enable(system, &home, &config, &binary, state)
                .map_err(|e| e.to_string())?;
            say(&format!("AZUL_BRIDGE_AUTOSTART on {}", path.display()));
            say("The bridge starts at the next login (azul-bridge serve starts it now).");
        }
        "disable" => {
            let removed = autostart::disable(system, &home, &config).map_err(|e| e.to_string())?;
            say(if removed { "AZUL_BRIDGE_AUTOSTART off" } else { "AZUL_BRIDGE_AUTOSTART off (it was not on)" });
        }
        _ => say(if autostart::is_enabled(system, &home, &config) {
            "AZUL_BRIDGE_AUTOSTART on"
        } else {
            "AZUL_BRIDGE_AUTOSTART off"
        }),
    }
    Ok(())
}

fn print_password(password: &str, config: Option<&BridgeConfig>) {
    say(&format!("AZUL_BRIDGE_PASSWORD {password}"));
    say("The bridge's password is shown this once. Mail programs and file managers sign in with it:");
    if let Some(config) = config {
        say(&format!(
            "  IMAP 127.0.0.1 port {}, SMTP 127.0.0.1 port {}, no SSL / TLS, user {}",
            config.imap_port, config.smtp_port, config.address
        ));
        say(&format!(
            "  WebDAV http://127.0.0.1:{}/ (Finder: Go > Connect to Server), user {}",
            config.dav_port, config.address
        ));
        say(&format!(
            "  CalDAV / CardDAV http://127.0.0.1:{}/ (a calendar or contacts account), user {}",
            config.pim_port, config.address
        ));
    }
    say("A new one: azul-bridge password");
}

fn init(
    options: &Options,
    state: &Path,
    secrets: &dyn KeyringStore,
    keyring: KeyringChoice,
) -> Result<(), String> {
    let address = options.address.clone().unwrap_or_default();
    if !address.contains('@') {
        return Err(format!("{address} is not an address"));
    }
    let mut config = BridgeConfig::load(state)
        .map_err(|e| e.to_string())?
        .unwrap_or_else(|| BridgeConfig::new(&address));
    config.address = address.trim().to_string();
    config.account = options
        .account
        .clone()
        .unwrap_or_else(|| config.address.clone());
    if !options.aliases.is_empty() {
        config.aliases = options.aliases.clone();
    }
    if let Some(sending) = &options.sending {
        let full = std::fs::canonicalize(sending).map_err(|e| format!("{}: {e}", sending.display()))?;
        config.sending_file = Some(full);
    }
    if let Some(port) = options.imap_port {
        config.imap_port = port;
    }
    if let Some(port) = options.smtp_port {
        config.smtp_port = port;
    }
    if let Some(port) = options.dav_port {
        config.dav_port = port;
    }
    if let Some(port) = options.pim_port {
        config.pim_port = port;
    }
    config.keyring = keyring.name().to_string();
    if options.allow_plain_secrets {
        config.plain_secrets = true;
    }
    config.save(state).map_err(|e| e.to_string())?;
    if secrets.get(PASSWORD_ENTRY).map_err(|e| e.to_string())?.is_none() {
        let password = auth::new_password()?;
        secrets.set(PASSWORD_ENTRY, &password).map_err(|e| e.to_string())?;
        print_password(&password, Some(&config));
    } else {
        say("The bridge has a password already (azul-bridge password makes a new one).");
    }
    say(&format!("AZUL_BRIDGE_CONFIG {}", state.join(config::CONFIG_FILE).display()));
    Ok(())
}

fn new_password(secrets: &dyn KeyringStore) -> Result<(), String> {
    let password = auth::new_password()?;
    secrets.set(PASSWORD_ENTRY, &password).map_err(|e| e.to_string())?;
    print_password(&password, None);
    Ok(())
}

/// What the bridge serves: an Azlin drive - its account's bytes, a `PlainBucket` (no drive: nothing reads or writes a
/// file through it) over which no gate has been made yet - or a drive of this computer. The two were one `Arc<dyn Drive>`
/// in a tuple beside an `Option` id and an `Option` restore source, and the arm `_ => (drive, None)` of the match that made
/// the gate returned the bare account drive for any pair that was not `(Some, Some)` (X-DELTA-STRUCT4-6): an Azlin drive
/// without its restore source would have been served in the clear. An `Azlin` has only one way on - the encryption's gate
/// (or, in a build without it, the mail's drive that refuses every call) - and a `Local` is no Azlin drive.
enum Backing {
    /// An Azlin drive: the account's bucket, its id (the keyring's and the swap records' key) and where its restore
    /// announcement comes from.
    Azlin {
        bucket: azmail_core::azlin::PlainBucket,
        id: String,
        restore: azul_storage::meta::RestoreSource,
    },
    /// A folder on this computer, or the demo's memory: no Azlin drive, no encryption.
    Local(Arc<dyn Drive>),
}

/// The drive the bridge serves for `backing`, and the hook that files the drops an Azlin drive's mail Worker left (none for a
/// drive of this computer). An `Azlin` has ONE way on: the encryption's gate (the mail's `wrap_auto`) - or, in a build without
/// the encryption, the mail's drive that refuses every call, because the bare account drive would serve the mail and the
/// WebDAV files in the clear (X-DELTA-AZDRIVE-KEYS-23). There is no arm that hands the account's bytes out as they are
/// (X-DELTA-STRUCT4-6).
fn served_drive(
    backing: Backing,
    secrets: &Arc<dyn KeyringStore>,
) -> (Arc<dyn Drive>, Option<crate::store::Incoming>) {
    #[cfg(feature = "encryption")]
    {
        match backing {
            Backing::Azlin {
                bucket,
                id,
                restore,
            } => {
                let auto =
                    azmail_core::mail_drive::wrap_auto(bucket, &id, secrets.clone(), restore);
                let incoming = crate::store::drops_incoming(Arc::clone(&auto), secrets.clone());
                (auto as Arc<dyn Drive>, Some(incoming))
            }
            Backing::Local(drive) => (drive, None),
        }
    }
    #[cfg(not(feature = "encryption"))]
    {
        match backing {
            Backing::Azlin {
                bucket,
                id,
                restore,
            } => (
                azmail_core::mail_drive::wrap(bucket, &id, secrets.clone(), restore),
                None,
            ),
            Backing::Local(drive) => (drive, None),
        }
    }
}

/// Where `serve` keeps the records of the replacing COPYs and MOVEs of the drive it serves ([`SwapRecords`]): the ONE
/// function that decides it, so a test enters it for each kind of drive. `--memory` is a drive that is gone with the
/// process, and so are its records; `--folder DIR` is a folder of this computer, keyed by its canonical path; an
/// Azlin account's drive (`drive_id`) is keyed by its id. Each of the last two has a folder of its own below
/// `<state>/dav-swaps/`, so a record is never settled on another drive than the one it was made on - and a process that
/// starts again finds the records of the one that was killed. An account's drive without an id has no place to keep
/// them: an error, never records in memory (the swap's intent would be lost with the process, the strays and the
/// half-swapped destination of X-DELTA-APPS3-14 would be back, and nothing would say so). The records are made under
/// `instance`, the lock of the state folder ([`crate::single::claim`]): the `Dav` that gets them knows no other bridge
/// serves the folder, so it settles a record of another process at its first request ([`SwapRecords::guarded_by`]).
///
/// # Errors
///
/// The drive is an account's and `drive_id` is `None`.
fn swap_records_for(
    options: &Options,
    state: &Path,
    drive_id: Option<&str>,
    instance: &crate::single::Instance,
) -> Result<SwapRecords, String> {
    let records = if options.memory {
        SwapRecords::of_drive(state, None)
    } else if let Some(folder) = &options.folder {
        let whole = std::fs::canonicalize(folder).unwrap_or_else(|_| folder.clone());
        SwapRecords::of_drive(state, Some(&format!("folder:{}", whole.display())))
    } else {
        let id = drive_id.ok_or_else(|| {
            String::from(
                "the drive has no id: the records of its replacing copies and moves cannot be kept",
            )
        })?;
        SwapRecords::of_drive(state, Some(&format!("azlin:{id}")))
    };
    Ok(records.guarded_by(instance))
}

fn serve(
    options: &Options,
    state: &Path,
    state_dir: &StateDir,
    secrets: Arc<dyn KeyringStore>,
) -> Result<(), String> {
    let config = BridgeConfig::load(state)
        .map_err(|e| e.to_string())?
        .ok_or("not set up: azul-bridge init --address <your address>")?;
    let password = secrets
        .get(PASSWORD_ENTRY)
        .map_err(|e| e.to_string())?
        .ok_or("no password yet: azul-bridge init")?;
    let credentials = Credentials::new(&config.address, &password);
    let mut limits = Limits::default();
    if let Some(secs) = options.idle_poll {
        limits.idle_poll = Duration::from_secs(secs);
    }
    // The Azlin drive (its id and where its restore announcement comes from: the device account's last refresh, asked at
    // every poll of the drive index, X-DELTA-AZDRIVE-SYNC-3) or a drive of this computer. (The records of its replacing
    // COPYs and MOVEs are made below: `swap_records_for`.)
    let (backing, uids): (Backing, UidMaps) = if options.memory {
        (
            Backing::Local(Arc::new(MemoryDrive::new())),
            UidMaps::in_memory(),
        )
    } else if let Some(folder) = &options.folder {
        std::fs::create_dir_all(folder).map_err(|e| format!("{}: {e}", folder.display()))?;
        (
            Backing::Local(Arc::new(LocalDrive::without_manifest(folder.clone()))),
            UidMaps::in_folder(state.join(UIDS_DIR)),
        )
    } else {
        let (token_url, s3_url) = endpoints(options);
        let token_url = token_url.ok_or("no token server: pass --token-url or set AZLIN_TOKEN_URL")?;
        let drive = AccountDrive::open(state_dir, &token_url, transports(&root_certs(options)?), s3_url)
            .map_err(|e| e.to_string())?;
        // iroh to the nodes first when this build dials it, HTTPS with the failover after.
        let drive = match iroh_dialer() {
            Some(dialer) => drive.with_iroh(dialer, settings(options).relay()),
            None => drive,
        };
        let id = drive.drive_id();
        say(&format!("AZUL_BRIDGE_DRIVE {id}"));
        let bucket = Arc::new(drive);
        let announcer = Arc::clone(&bucket);
        (
            Backing::Azlin {
                bucket: azmail_core::azlin::PlainBucket::from(bucket),
                id,
                restore: azul_storage::meta::RestoreSource::new(move || announcer.restore_notice()),
            },
            UidMaps::in_folder(state.join(UIDS_DIR)),
        )
    };
    // The id of the account's drive: the key of its swap records, below (a `Backing::Local` has none).
    let drive_id: Option<String> = match &backing {
        Backing::Azlin { id, .. } => Some(id.clone()),
        Backing::Local(_) => None,
    };
    // An Azlin drive goes through the encryption, as AzMail's does (azul-mail-core's mail_drive:
    // AutoEncrypted with the drive index, its copy in the state folder; the drive key from the
    // bridge's keyring; a drive that is not encrypted is neither read nor written), and the drops
    // the mail Worker leaves are filed before a folder is listed. IMAP, SMTP, WebDAV, CalDAV and
    // CardDAV all use this drive. The settings of its index are the process's and are made before
    // the gate is (`served_drive`: no arm of it returns the account's bytes - an `Azlin` is wrapped
    // or refused, whatever else it holds: X-DELTA-STRUCT4-6).
    #[cfg(feature = "encryption")]
    if matches!(backing, Backing::Azlin { .. }) {
        azmail_core::mail_drive::set_device_name("Azlin Bridge");
        azmail_core::mail_drive::set_index_cache_root(Some(state.join(config::INDEX_CACHE_DIR)));
        // The poison pill of the analysis of a file and of the decode of a stored transform:
        // the bridge serves the JPEGs, PDFs and archives any member of the drive wrote over
        // WebDAV, and a crafted stream that ended it three times is stored as it is from then
        // on (X-DELTA-AZDRIVE-KEYS-44; AzDrive names its folder the same way).
        azul_storage::crypto::codec::set_analysis_marker_dir(Some(state.join("analysis")));
    }
    let (drive, incoming) = served_drive(backing, &secrets);
    let mut mail_store = DriveMailStore::new(drive.clone());
    if let Some(incoming) = incoming {
        mail_store = mail_store.with_incoming(incoming);
    }
    let store: Arc<dyn MailStore> = Arc::new(mail_store);
    let gate = Arc::new(FailureGate::new(
        limits.auth_failures_free,
        limits.auth_window,
        limits.auth_delay,
    ));
    let sent = Arc::new(SentRegistry::new());
    let submitter = Arc::new(
        AzMailSubmitter::new(
            state.join(config::SPOOL_DIR),
            &config.account,
            config.sending_file.clone(),
            secrets,
            store.clone(),
            sent.clone(),
        )
        .trusting(trusted_roots(options)?),
    );
    let bind = |port: u16, what: &str| {
        net::bind_loopback(port).map_err(|e| format!("{what} on 127.0.0.1:{port}: {e}"))
    };
    // One bridge per user: refused here, before a port is taken, naming the one that serves.
    let instance = crate::single::claim(state)?;
    // The records of a replacing COPY or MOVE (WebDAV): files in the state folder beside the UID maps - never in the
    // drive, which refuses its own bookkeeping's names (X-DELTA-APPS4-1) - one folder for each drive, made under the
    // lock just taken: no other bridge serves this state folder, so a record of another process is a dead process's
    // and the first request settles it (X-DELTA-APPS5-5). ONE function makes them (X-DELTA-APPS5-4: nothing pinned that
    // `serve` kept them there, and records handed to the wrong arm lose the swap's intent with the process, without
    // a word).
    let swaps = swap_records_for(options, state, drive_id.as_deref(), &instance)?;
    let imap_listener = bind(options.imap_port.unwrap_or(config.imap_port), "IMAP")?;
    let smtp_listener = bind(options.smtp_port.unwrap_or(config.smtp_port), "SMTP")?;
    let dav_listener = bind(options.dav_port.unwrap_or(config.dav_port), "WebDAV")?;
    let pim_listener = bind(options.pim_port.unwrap_or(config.pim_port), "CalDAV / CardDAV")?;
    let port_of = |listener: &std::net::TcpListener| listener.local_addr().map(|a| a.port()).unwrap_or(0);
    let (imap_port, smtp_port, dav_port, pim_port) = (
        port_of(&imap_listener),
        port_of(&smtp_listener),
        port_of(&dav_listener),
        port_of(&pim_listener),
    );
    let imap = Arc::new(Imap::new(
        store,
        Arc::new(uids),
        credentials.clone(),
        gate.clone(),
        limits.clone(),
        sent,
    ));
    let smtp = Arc::new(Smtp {
        submitter: submitter.clone(),
        credentials: credentials.clone(),
        gate: gate.clone(),
        limits: limits.clone(),
        senders: config.senders(),
    });
    // AzCalendar's data folder: the drive's `calendar/`, or a folder of this computer.
    let calendar: Arc<dyn Drive> = match &options.calendar_folder {
        Some(folder) => Arc::new(LocalDrive::new(folder.clone())) as Arc<dyn Drive>,
        None => Arc::new(
            ScopedDrive::new(drive.clone(), pim::CALENDAR_FOLDER, true).map_err(|e| e.to_string())?,
        ) as Arc<dyn Drive>,
    };
    let names = if options.memory {
        Names::in_memory()
    } else {
        Names::in_file(state.join(pim::NAMES_FILE))
    };
    let pim = Arc::new(Dav::for_pim(
        Pim::new(drive.clone(), calendar, names, &config.address),
        credentials.clone(),
        gate.clone(),
        limits.clone(),
        pim_port,
    ));
    let dav = Arc::new(Dav::new(drive, credentials, gate, limits, dav_port, swaps));
    let mut threads = Vec::new();
    threads.push(std::thread::spawn(move || imap::serve(imap, imap_listener)));
    threads.push(std::thread::spawn(move || smtp::serve(smtp, smtp_listener)));
    threads.push(std::thread::spawn(move || dav::serve(dav, dav_listener)));
    threads.push(std::thread::spawn(move || dav::serve(pim, pim_listener)));
    std::thread::spawn(move || loop {
        std::thread::sleep(RETRY_EVERY);
        submitter.retry();
    });
    instance.announce(&crate::single::Running {
        pid: std::process::id(),
        imap: imap_port,
        smtp: smtp_port,
        dav: dav_port,
        pim: pim_port,
        started: azul_storage::time::now_unix(),
    });
    say(&format!("AZUL_BRIDGE_READY imap={imap_port} smtp={smtp_port} dav={dav_port} pim={pim_port}"));
    for thread in threads {
        let _ = thread.join();
    }
    Err(String::from("a listener stopped"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(text: &str) -> Vec<String> {
        text.split_whitespace().map(str::to_string).collect()
    }

    /// Every drive is encrypted as it is made: the bridge's own development drive too, and a
    /// plaintext one is refused with why.
    /// X-DELTA-KEYS2-4: `signup --recovery-out F` names the file the code goes into; the file is made
    /// before the drive is (see azcloud-kit's `OutFile`).
    #[test]
    fn x_delta_keys2_4_signup_takes_a_file_for_the_recovery_code() {
        let options = parse_args(&args("signup --recovery-out /tmp/code.txt")).unwrap();
        assert_eq!(
            options.recovery_out.as_deref(),
            Some(Path::new("/tmp/code.txt"))
        );
        assert!(
            parse_args(&args("signup --recovery-out")).is_err(),
            "a value is missing"
        );
        assert_eq!(parse_args(&args("signup")).unwrap().recovery_out, None);
    }

    /// The folder a drive's records are in: `<state>/dav-swaps/` and the first 32 hex digits of the SHA-256 of the drive's
    /// label - written here a second time on purpose, so that a change of the scheme (which would strand the records of a
    /// bridge that was killed before the upgrade) is a red test and not a silent move.
    fn swaps_of(state: &Path, label: &str) -> PathBuf {
        let hash = azul_storage::sigv4::sha256_hex(label.as_bytes());
        state.join("dav-swaps").join(&hash[..32])
    }

    /// X-DELTA-APPS5-4, RED on the code before it was pinned: nothing entered the line of `serve` that gives the swap its
    /// records. The mutant that hands `SwapRecords::in_memory()` to the folder arm or to the Azlin arm kept all 196 tests of
    /// the bridge green, because every swap test makes its own records - and in production the intent of a replacing COPY
    /// or MOVE then died with the process (the strays and the half-swapped destination of X-DELTA-APPS3-14 are back, with
    /// no word). Each arm of the function `serve` calls, by the options it is given: `--memory` keeps them in memory (no
    /// file anywhere); `--folder DIR` a file in a folder of its own below `<state>/dav-swaps/`, keyed by the canonical
    /// path; an account's drive another folder, keyed by its id; and an account's drive with no id is refused - it is never
    /// the memory arm. X-DELTA-APPS5-5: the records of the two arms that are files are made under the lock of the state
    /// folder (`Instance`), which is what lets the `Dav` settle a record of another process at once; memory has none.
    #[test]
    fn x_delta_apps5_4_serve_keeps_the_records_of_a_swap_in_the_state_folder_by_the_drive_it_serves(
    ) {
        let state = azul_storage::testing::TempDir::new("bridge-cli-swaps-state");
        let documents = azul_storage::testing::TempDir::new("bridge-cli-swaps-documents");
        let options = |line: &str| parse_args(&args(line)).unwrap();
        let instance = crate::single::claim(&state.0).unwrap();

        // `--memory`: records that die with the process, and not one file below the state folder.
        let memory =
            swap_records_for(&options("serve --memory"), &state.0, None, &instance).unwrap();
        memory.put("1000-4242-0", b"{}").unwrap();
        assert_eq!(memory.folder(), None, "memory");
        assert_eq!(memory.stamps().unwrap(), vec![String::from("1000-4242-0")]);
        assert!(!state.0.join("dav-swaps").exists(), "no file for --memory");
        assert!(!memory.guarded(), "no other process can see them");

        // `--folder DIR`: a file in the folder of that directory's records, keyed by its canonical path.
        let line = format!("serve --folder {}", documents.0.display());
        let folder = swap_records_for(&options(&line), &state.0, None, &instance).unwrap();
        folder.put("1000-4242-0", b"{}").unwrap();
        let whole = std::fs::canonicalize(&documents.0).unwrap();
        let place = swaps_of(&state.0, &format!("folder:{}", whole.display()));
        assert_eq!(folder.folder(), Some(place.as_path()), "folder");
        assert!(place.join("1000-4242-0.json").is_file(), "{place:?}");
        assert!(folder.guarded(), "made under the lock of the state folder");

        // An account's drive: a folder of its own, keyed by its id.
        let account =
            swap_records_for(&options("serve"), &state.0, Some("d_42"), &instance).unwrap();
        account.put("1000-4242-0", b"{}").unwrap();
        let place = swaps_of(&state.0, "azlin:d_42");
        assert_eq!(account.folder(), Some(place.as_path()), "account");
        assert!(place.join("1000-4242-0.json").is_file(), "{place:?}");
        assert!(account.guarded(), "made under the lock of the state folder");
        assert_ne!(
            account.folder(),
            folder.folder(),
            "one folder for each drive"
        );
        let other = swap_records_for(&options("serve"), &state.0, Some("d_43"), &instance).unwrap();
        assert_eq!(
            other.folder(),
            Some(swaps_of(&state.0, "azlin:d_43").as_path()),
            "another account: another folder"
        );
        assert!(
            other.stamps().unwrap().is_empty(),
            "its records are not d_42's"
        );

        // The same account in a process that starts again finds the record of the one that was killed.
        let again = swap_records_for(&options("serve"), &state.0, Some("d_42"), &instance).unwrap();
        assert_eq!(again.stamps().unwrap(), vec![String::from("1000-4242-0")]);

        // An account's drive that has no id has no place to keep them: refused, never the memory arm.
        let none = swap_records_for(&options("serve"), &state.0, None, &instance).unwrap_err();
        assert!(none.contains("has no id"), "{none}");
    }

    /// X-DELTA-STRUCT4-6. The bridge serves an Azlin drive through the encryption's gate and no other way (`served_drive`, the
    /// step of `serve` that makes the drive): a backing over a bucket nobody encrypted takes no write and the bucket is as it was;
    /// one set up under its key serves its files as objects of AZL1 and none of them in the clear, and gets the hook that files
    /// the drops; a folder of this computer is served as it is, with no hook.
    #[cfg(feature = "encryption")]
    #[test]
    fn x_delta_struct4_6_an_azlin_backing_is_served_through_the_gate_and_never_as_the_plain_bucket()
    {
        use azmail_core::azlin::PlainBucket;
        use azul_storage::{
            crypto::{device, keys::RecoveryKdf},
            keyring::MemoryKeyring,
            meta::{MemoryBucket, RestoreSource},
            DriveError,
        };

        let secrets: Arc<dyn KeyringStore> = Arc::new(MemoryKeyring::new());
        let none = || RestoreSource::none("a test has no token server to ask");
        // A drive nobody encrypted: nothing is read or written through the gate, and nothing reached the bucket.
        let plain = Arc::new(MemoryBucket::new());
        let (drive, incoming) = served_drive(
            Backing::Azlin {
                bucket: PlainBucket::from(plain.clone()),
                id: String::from("d_new"),
                restore: none(),
            },
            &secrets,
        );
        assert!(incoming.is_some(), "the drops of an Azlin drive are filed");
        assert!(
            matches!(
                drive.put("mail/Inbox/1.eml", b"hello plain text"),
                Err(DriveError::Denied { .. })
            ),
            "a drive that is not encrypted takes no write"
        );
        assert!(plain.objects().is_empty(), "nothing reached the bucket");
        // A drive set up under its key: its files are objects of AZL1, none holds the mail in the clear.
        let sealed = Arc::new(MemoryBucket::new());
        device::setup_new_drive(
            sealed.as_ref(),
            secrets.as_ref(),
            "d_enc",
            RecoveryKdf::with_cost(64, 1, 1).unwrap(),
        )
        .unwrap();
        let (drive, incoming) = served_drive(
            Backing::Azlin {
                bucket: PlainBucket::from(sealed.clone()),
                id: String::from("d_enc"),
                restore: none(),
            },
            &secrets,
        );
        assert!(incoming.is_some());
        drive.put("mail/Inbox/1.eml", b"hello plain text").unwrap();
        assert_eq!(drive.get("mail/Inbox/1.eml").unwrap(), b"hello plain text");
        for (name, bytes) in sealed.objects() {
            assert!(
                !String::from_utf8_lossy(&bytes).contains("hello plain text"),
                "{name} holds the mail in the clear"
            );
        }
        // A folder of this computer is served as it is.
        let folder = Arc::new(MemoryBucket::new());
        let (drive, incoming) = served_drive(Backing::Local(folder.clone()), &secrets);
        assert!(incoming.is_none(), "no drops in a folder");
        drive.put("a.txt", b"as it is").unwrap();
        assert_eq!(folder.get("a.txt").unwrap(), b"as it is");
    }

    #[test]
    fn a_signup_makes_an_encrypted_drive_and_refuses_a_plaintext_one() {
        assert_eq!(parse_args(&args("signup --name Bridge")).unwrap().command, "signup");
        let refused = parse_args(&args("signup --plaintext")).expect_err("no plaintext drive");
        assert!(refused.contains("encrypted"), "{refused}");
    }

    #[test]
    fn the_command_line_reads_commands_flags_and_ports() {
        let options = parse_args(&args(
            "--state-dir /tmp/b init --address ada@example.org --alias info@example.org --imap-port 0",
        ))
        .unwrap();
        assert_eq!(options.command, "init");
        assert_eq!(options.state_dir, Some(PathBuf::from("/tmp/b")));
        assert_eq!(options.address.as_deref(), Some("ada@example.org"));
        assert_eq!(options.aliases, vec!["info@example.org"]);
        assert_eq!(options.imap_port, Some(0));
        let on = parse_args(&args("autostart enable")).unwrap();
        assert_eq!((on.command.as_str(), on.action.as_deref()), ("autostart", Some("enable")));
        assert!(parse_args(&args("autostart")).is_err());
        assert!(parse_args(&args("autostart sometimes")).is_err());
        let serve = parse_args(&args("serve --memory --dav-port 8080")).unwrap();
        assert!(serve.memory && serve.dav_port == Some(8080));
        let pim = parse_args(&args("serve --pim-port 0 --calendar-folder /data/AzCalendar")).unwrap();
        assert_eq!(pim.pim_port, Some(0));
        assert_eq!(pim.calendar_folder, Some(PathBuf::from("/data/AzCalendar")));
        for bad in [
            "",
            "init",
            "join",
            "serve --imap-port 70000",
            "serve --folder /x --memory",
            "serve status",
            "frobnicate",
            "serve --nope",
            "serve --state-dir",
        ] {
            assert!(parse_args(&args(bad)).is_err(), "{bad}");
        }
    }

    #[test]
    fn init_makes_the_password_once_and_keeps_it_in_the_secret_store() {
        let dir = azul_storage::testing::TempDir::new("bridge-cli");
        let state = dir.0.join("state");
        let options = parse_args(&[
            String::from("--state-dir"),
            state.display().to_string(),
            String::from("init"),
            String::from("--address"),
            String::from("ada@example.org"),
            // Never the OS keyring in a test, whatever the build links.
            String::from("--keyring"),
            String::from("file"),
        ])
        .unwrap();
        run(&options).unwrap();
        let secrets = FileKeyring(StateDir::open(&state).unwrap().secrets());
        let first = secrets.get(PASSWORD_ENTRY).unwrap().unwrap();
        assert_eq!(first.len(), 29);
        run(&options).unwrap();
        assert_eq!(secrets.get(PASSWORD_ENTRY).unwrap().unwrap(), first, "init keeps it");
        let config = BridgeConfig::load(&state).unwrap().unwrap();
        assert_eq!((config.address.as_str(), config.imap_port), ("ada@example.org", config::IMAP_PORT));
        assert_eq!(config.keyring, "file");
        let text = std::fs::read_to_string(state.join(config::CONFIG_FILE)).unwrap();
        assert!(!text.contains(&first), "the password is not in bridge.json");
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = std::fs::metadata(state_dir_secrets(&state)).unwrap().permissions().mode();
            assert_eq!(mode & 0o077, 0, "the secrets file is this user's only: {mode:o}");
        }
    }

    /// AZ-K7: the secrets file is a plain file, and the bridge keeps a drive's key in it only when
    /// the user said so - `init --allow-plain-secrets` writes the word into bridge.json, so a later
    /// run needs no flag - and a run that was not told refuses the key and says why. (A drive's key
    /// exists only in a build with the encryption: without it this test cannot even be compiled.)
    #[cfg(feature = "encryption")]
    #[test]
    fn az_k7_the_secrets_file_keeps_a_drives_key_only_when_the_user_said_it_may() {
        use azul_storage::{
            crypto::{device::plan_drive_key, DriveKey},
            footprint::Protection,
        };
        let dir = azul_storage::testing::TempDir::new("bridge-plain-secrets");
        let state = dir.0.join("state");
        let line = |rest: &str| format!("--state-dir {} --keyring file {rest}", state.display());
        let key = DriveKey::generate().unwrap();

        // Set up without the word: the file says it is plain, and takes no key.
        let init = parse_args(&args(&line("init --address ada@example.org"))).unwrap();
        run(&init).unwrap();
        assert!(!BridgeConfig::load(&state).unwrap().unwrap().plain_secrets);
        let opened = open_state(&init).unwrap();
        assert_eq!(
            opened.secrets.protection(),
            Protection::PlainFile { allow: Allow::Nothing }
        );
        let refused = plan_drive_key(&*opened.secrets, "d_1", &key).unwrap_err().to_string();
        assert!(refused.contains("plain file"), "{refused}");
        assert!(refused.contains("--allow-plain-secrets"), "{refused}");

        // The flag for one run.
        let once = parse_args(&args(&line("--allow-plain-secrets status"))).unwrap();
        let opened = open_state(&once).unwrap();
        plan_drive_key(&*opened.secrets, "d_1", &key).unwrap();

        // The word at init is written down; the later runs have it without the flag.
        let init = parse_args(&args(&line("--allow-plain-secrets init --address ada@example.org")))
            .unwrap();
        run(&init).unwrap();
        assert!(BridgeConfig::load(&state).unwrap().unwrap().plain_secrets);
        let text = std::fs::read_to_string(state.join(config::CONFIG_FILE)).unwrap();
        assert!(text.contains("\"plain_secrets\": true"), "{text}");
        let later = parse_args(&args(&line("status"))).unwrap();
        assert_eq!(
            open_state(&later).unwrap().secrets.protection(),
            Protection::PlainFile { allow: Allow::PlainSecrets }
        );
    }

    #[cfg(unix)]
    fn state_dir_secrets(state: &Path) -> PathBuf {
        StateDir::open(state).unwrap().secrets().path().to_path_buf()
    }
}
