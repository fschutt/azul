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
//!   signup [--name NAME] [--tier TIER] [--plaintext]
//!        A development drive of the bridge's own (development token servers only), encrypted as
//!        it is made: its recovery code is printed once (`AZUL_BRIDGE_RECOVERY_CODE <code>`), the
//!        keys kept in the secret store. `--plaintext` makes a drive like the ones made before
//!        encryption (tests of those).
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
//! ([`crate::config`]). The token server: `--token-url`, `$AZLIN_TOKEN_URL`, the shared Azlin
//! config (azcloud-kit's settings, as the `azcloud` command line resolves it). The secrets
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
use azul_storage::{Drive, LocalDrive, ScopedDrive, Transport};

use crate::{
    account::AccountDrive,
    autostart,
    auth::{self, Credentials, FailureGate},
    config::{self, BridgeConfig},
    dav::{self, Dav},
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
    /// `autostart`'s `enable` / `disable` / `status`.
    pub action: Option<String>,
    /// `signup --plaintext`: a drive like the ones made before encryption (tests of those).
    pub plaintext: bool,
}

/// Whether `signup` makes the new drive encrypted: always ("we always encrypt"), but for
/// `--plaintext`.
#[must_use]
pub fn signup_encrypts(options: &Options) -> bool {
    !options.plaintext
}

/// The usage text.
pub const USAGE: &str = "usage: azul-bridge [--state-dir DIR] [--token-url URL] [--s3-url URL] [--keyring os|file] \
     <init --address ADDR [--account ID] [--alias ADDR]... [--sending FILE] [--imap-port N] \
     [--smtp-port N] [--dav-port N] [--pim-port N] | password | signup [--name NAME] [--tier TIER] [--plaintext] | \
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
            "--name" => options.name = Some(value(&mut i, arg)?),
            "--tier" => options.tier = Some(value(&mut i, arg)?),
            "--imap-port" => options.imap_port = Some(port(value(&mut i, arg)?, arg)?),
            "--smtp-port" => options.smtp_port = Some(port(value(&mut i, arg)?, arg)?),
            "--dav-port" => options.dav_port = Some(port(value(&mut i, arg)?, arg)?),
            "--pim-port" => options.pim_port = Some(port(value(&mut i, arg)?, arg)?),
            "--folder" => options.folder = Some(PathBuf::from(value(&mut i, arg)?)),
            "--calendar-folder" => options.calendar_folder = Some(PathBuf::from(value(&mut i, arg)?)),
            "--memory" => options.memory = true,
            "--plaintext" => options.plaintext = true,
            "--keyring" => options.keyring = Some(value(&mut i, arg)?),
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

fn transports() -> TransportFactory {
    Arc::new(|| Box::new(crate::transport::UreqTransport::new()) as Box<dyn Transport>)
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
    let choice = match &options.keyring {
        Some(text) => KeyringChoice::parse(text).ok_or("--keyring takes os or file")?,
        None => BridgeConfig::load(&state)
            .ok()
            .flatten()
            .and_then(|config| KeyringChoice::parse(&config.keyring))
            .unwrap_or_else(KeyringChoice::default_for_build),
    };
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
            let account = Account::signup(&state_dir, &token_url, transports(), &tier, &name)
                .map_err(|e| e.to_string())?;
            say(&format!("AZUL_BRIDGE_DRIVE {}", account.record().id));
            // "We always encrypt": the keys as the drive is made (into the bucket at this run's
            // S3 endpoint, the drive key and the bridge's member key into its secret store).
            #[cfg(feature = "encryption")]
            if signup_encrypts(options) {
                let account = account.with_s3_endpoint(endpoints(options).1.as_deref());
                let kdf = azul_storage::crypto::keys::RecoveryKdf::fresh()
                    .map_err(|e| e.to_string())?;
                let code = account
                    .setup_encryption(&*secrets, kdf)
                    .map_err(|e| e.to_string())?;
                say(&format!("AZUL_BRIDGE_RECOVERY_CODE {}", code.to_text().as_str()));
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
            let account = Account::join(&state_dir, &token_url, transports(), &code)
                .map_err(|e| e.to_string())?;
            say(&format!("AZUL_BRIDGE_DRIVE {}", account.record().id));
            // An encrypted drive's key rides in the code (sealed to a one-time key): the bridge
            // keeps it in its keyring and enrols itself, as every other device does.
            #[cfg(feature = "encryption")]
            if account
                .adopt_join_key(&code, &*secrets)
                .map_err(|e| e.to_string())?
                .is_some()
            {
                say("AZUL_BRIDGE_DRIVE_KEY kept");
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
    // The Azlin drive's id (`None`: a development drive of this computer).
    let (drive, uids, drive_id): (Arc<dyn Drive>, UidMaps, Option<String>) = if options.memory {
        (
            Arc::new(MemoryDrive::new()) as Arc<dyn Drive>,
            UidMaps::in_memory(),
            None,
        )
    } else if let Some(folder) = &options.folder {
        std::fs::create_dir_all(folder).map_err(|e| format!("{}: {e}", folder.display()))?;
        (
            Arc::new(LocalDrive::without_manifest(folder.clone())) as Arc<dyn Drive>,
            UidMaps::in_folder(state.join(UIDS_DIR)),
            None,
        )
    } else {
        let (token_url, s3_url) = endpoints(options);
        let token_url = token_url.ok_or("no token server: pass --token-url or set AZLIN_TOKEN_URL")?;
        let drive = AccountDrive::open(state_dir, &token_url, transports(), s3_url)
            .map_err(|e| e.to_string())?;
        // iroh to the nodes first when this build dials it, HTTPS with the failover after.
        let drive = match iroh_dialer() {
            Some(dialer) => drive.with_iroh(dialer, settings(options).relay()),
            None => drive,
        };
        let id = drive.drive_id();
        say(&format!("AZUL_BRIDGE_DRIVE {id}"));
        (
            Arc::new(drive) as Arc<dyn Drive>,
            UidMaps::in_folder(state.join(UIDS_DIR)),
            Some(id),
        )
    };
    // An Azlin drive goes through the encryption when it is encrypted, as AzMail's does
    // (azul-mail-core's mail_drive: AutoEncrypted with the drive index, its copy in the state
    // folder; the drive key from the bridge's keyring), and the drops the mail Worker leaves are
    // filed before a folder is listed. IMAP, SMTP, WebDAV, CalDAV and CardDAV all use this drive.
    #[cfg(feature = "encryption")]
    let (drive, incoming) = match &drive_id {
        Some(id) => {
            azmail_core::mail_drive::set_device_name("Azlin Bridge");
            azmail_core::mail_drive::set_index_cache_root(Some(state.join(config::INDEX_CACHE_DIR)));
            let auto = azmail_core::mail_drive::wrap_auto(drive, id, secrets.clone());
            let incoming = crate::store::drops_incoming(Arc::clone(&auto), secrets.clone());
            (auto as Arc<dyn Drive>, Some(incoming))
        }
        None => (drive, None),
    };
    #[cfg(not(feature = "encryption"))]
    let incoming: Option<crate::store::Incoming> = {
        let _ = &drive_id;
        None
    };
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
    let submitter = Arc::new(AzMailSubmitter::new(
        state.join(config::SPOOL_DIR),
        &config.account,
        config.sending_file.clone(),
        secrets,
        store.clone(),
        sent.clone(),
    ));
    let bind = |port: u16, what: &str| {
        net::bind_loopback(port).map_err(|e| format!("{what} on 127.0.0.1:{port}: {e}"))
    };
    // One bridge per user: refused here, before a port is taken, naming the one that serves.
    let instance = crate::single::claim(state)?;
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
    let dav = Arc::new(Dav::new(drive, credentials, gate, limits, dav_port));
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

    /// "We always encrypt": the bridge's own development drive is encrypted as it is made;
    /// `--plaintext` makes one like the drives made before encryption.
    #[test]
    fn a_signup_makes_an_encrypted_drive_unless_asked_for_a_plaintext_one() {
        assert!(signup_encrypts(&parse_args(&args("signup --name Bridge")).unwrap()));
        assert!(!signup_encrypts(
            &parse_args(&args("signup --plaintext")).unwrap()
        ));
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

    #[cfg(unix)]
    fn state_dir_secrets(state: &Path) -> PathBuf {
        StateDir::open(state).unwrap().secrets().path().to_path_buf()
    }
}
