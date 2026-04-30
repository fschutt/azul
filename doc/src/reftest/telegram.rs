//! Telegram bot bridge for routing apply-midlevel prompts to a phone.
//!
//! The bot acts as a remote stdin/stdout for the interactive prompts in
//! `apply_midlevel.rs`. Messages sent via [`TelegramBridge::send_message`]
//! show up in the user's Telegram chat; replies are long-polled via
//! `getUpdates` and merged into a single [`InputChannel`] that also reads
//! local stdin. Whichever input arrives first wins.
//!
//! Configuration: `~/.config/azul-doc/telegram.toml` (mode 0600), populated
//! by the `telegram-setup` subcommand. Env vars `AZUL_DOC_TG_TOKEN` /
//! `AZUL_DOC_TG_CHAT_ID` (and the more standard `TELEGRAM_BOT_TOKEN` /
//! `TELEGRAM_CHAT_ID`) override the file.

use std::fs;
use std::io::{self, BufRead, Write};
use std::path::PathBuf;
use std::sync::{
    mpsc::{self, Receiver},
    Arc,
};
use std::thread;
use std::time::Duration;

use serde_derive::{Deserialize, Serialize};
use ureq::Agent;

use super::make_https_agent;

// ── Config ────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TelegramConfig {
    pub token: String,
    pub chat_id: i64,
}

impl TelegramConfig {
    pub fn from_env() -> Option<Self> {
        let token = std::env::var("AZUL_DOC_TG_TOKEN")
            .or_else(|_| std::env::var("TELEGRAM_BOT_TOKEN"))
            .ok()?;
        let chat_id = std::env::var("AZUL_DOC_TG_CHAT_ID")
            .or_else(|_| std::env::var("TELEGRAM_CHAT_ID"))
            .ok()?;
        let chat_id = chat_id.parse::<i64>().ok()?;
        Some(Self { token, chat_id })
    }

    pub fn load_from_file() -> Result<Option<Self>, String> {
        let path = config_path();
        if !path.exists() {
            return Ok(None);
        }
        let raw = fs::read_to_string(&path)
            .map_err(|e| format!("read {}: {}", path.display(), e))?;
        let cfg: Self = toml::from_str(&raw)
            .map_err(|e| format!("parse {}: {}", path.display(), e))?;
        Ok(Some(cfg))
    }

    pub fn save_to_file(&self) -> Result<PathBuf, String> {
        let path = config_path();
        let dir = path.parent().ok_or("no parent dir for config_path")?;
        fs::create_dir_all(dir)
            .map_err(|e| format!("mkdir {}: {}", dir.display(), e))?;
        let toml_str = toml::to_string_pretty(self)
            .map_err(|e| format!("toml serialize: {}", e))?;
        fs::write(&path, toml_str)
            .map_err(|e| format!("write {}: {}", path.display(), e))?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mut perms = fs::metadata(&path)
                .map_err(|e| format!("metadata: {}", e))?
                .permissions();
            perms.set_mode(0o600);
            fs::set_permissions(&path, perms)
                .map_err(|e| format!("chmod 600: {}", e))?;
        }
        Ok(path)
    }
}

fn config_path() -> PathBuf {
    let home = std::env::var("HOME").unwrap_or_default();
    PathBuf::from(home)
        .join(".config")
        .join("azul-doc")
        .join("telegram.toml")
}

// ── Bridge (HTTP + state) ─────────────────────────────────────────────────

pub struct TelegramBridge {
    pub token: String,
    pub chat_id: i64,
    agent: Agent,
}

impl TelegramBridge {
    pub fn from_config(cfg: TelegramConfig) -> Self {
        Self {
            token: cfg.token,
            chat_id: cfg.chat_id,
            agent: make_https_agent(),
        }
    }

    /// Returns `Some(Ok(...))` if a token + chat_id are available (env or
    /// file). `None` if no configuration exists. `Some(Err)` only on
    /// malformed config.
    pub fn from_env_or_config() -> Option<Result<Self, String>> {
        if let Some(cfg) = TelegramConfig::from_env() {
            return Some(Ok(Self::from_config(cfg)));
        }
        match TelegramConfig::load_from_file() {
            Ok(Some(cfg)) => Some(Ok(Self::from_config(cfg))),
            Ok(None) => None,
            Err(e) => Some(Err(e)),
        }
    }

    /// POST sendMessage. `keyboard` is rows of button labels — Telegram
    /// shows a custom reply keyboard so the user can tap instead of type.
    /// Pass `None` to clear any existing keyboard (free-form replies).
    ///
    /// Long messages are split into 4000-char chunks (Telegram caps at
    /// 4096); only the first chunk carries the keyboard.
    pub fn send_message(
        &self,
        text: &str,
        keyboard: Option<&[&[&str]]>,
    ) -> Result<(), String> {
        let url = format!("https://api.telegram.org/bot{}/sendMessage", self.token);

        let chunks = chunk_chars(text, 4000);
        let last_idx = chunks.len().saturating_sub(1);
        for (i, chunk) in chunks.iter().enumerate() {
            let mut body = serde_json::json!({
                "chat_id": self.chat_id,
                "text": chunk,
                "disable_web_page_preview": true,
            });
            // Only attach keyboard once, on the LAST chunk so it stays
            // visible after the user scrolls down to read.
            if i == last_idx {
                if let Some(rows) = keyboard {
                    body["reply_markup"] = serde_json::json!({
                        "keyboard": rows.iter().map(|row| {
                            row.iter().map(|t| serde_json::json!({"text": t})).collect::<Vec<_>>()
                        }).collect::<Vec<_>>(),
                        "resize_keyboard": true,
                        "is_persistent": true,
                    });
                } else {
                    body["reply_markup"] = serde_json::json!({"remove_keyboard": true});
                }
            }
            self.agent
                .post(&url)
                .header("Content-Type", "application/json")
                .send_json(&body)
                .map_err(|e| format!("telegram sendMessage: {}", e))?;
        }
        Ok(())
    }

    /// Long-poll `getUpdates`. Returns `(reply, new_offset)`. `reply` is the
    /// most recent message text from the configured chat_id, or `None` if
    /// the timeout elapsed without a relevant message.
    fn poll_once(
        &self,
        offset: i64,
        timeout_secs: u32,
    ) -> Result<(Option<String>, i64), String> {
        let url = format!(
            "https://api.telegram.org/bot{}/getUpdates?offset={}&timeout={}&allowed_updates=%5B%22message%22%5D",
            self.token, offset, timeout_secs,
        );
        let parsed: serde_json::Value = self
            .agent
            .get(&url)
            .call()
            .map_err(|e| format!("telegram getUpdates: {}", e))?
            .into_body()
            .read_json()
            .map_err(|e| format!("telegram getUpdates parse: {}", e))?;

        if !parsed.get("ok").and_then(|v| v.as_bool()).unwrap_or(false) {
            return Err(format!("telegram api not ok: {}", parsed));
        }
        let updates = parsed
            .get("result")
            .and_then(|r| r.as_array())
            .cloned()
            .unwrap_or_default();

        let mut new_offset = offset;
        let mut found_text: Option<String> = None;
        for upd in &updates {
            let update_id = upd.get("update_id").and_then(|v| v.as_i64()).unwrap_or(0);
            new_offset = new_offset.max(update_id + 1);

            let chat_id = upd.pointer("/message/chat/id").and_then(|v| v.as_i64());
            if chat_id != Some(self.chat_id) {
                continue;
            }
            if let Some(t) = upd.pointer("/message/text").and_then(|v| v.as_str()) {
                // If multiple messages arrived in one batch, take the LAST
                // (most recent user intent).
                found_text = Some(t.to_string());
            }
        }
        Ok((found_text, new_offset))
    }
}

// ── Char-aware chunking (Telegram counts UTF-16 code units, but staying
//    under 4000 *Rust chars* is comfortably below the 4096 limit) ─────────

fn chunk_chars(s: &str, max_chars: usize) -> Vec<String> {
    if s.chars().count() <= max_chars {
        return vec![s.to_string()];
    }
    let mut out = Vec::new();
    let mut buf = String::new();
    let mut count = 0usize;
    for c in s.chars() {
        if count == max_chars {
            out.push(std::mem::take(&mut buf));
            count = 0;
        }
        buf.push(c);
        count += 1;
    }
    if !buf.is_empty() {
        out.push(buf);
    }
    out
}

// ── Combined input channel (stdin + telegram) ─────────────────────────────

pub enum UserInput {
    Local(String),
    Remote(String),
}

impl UserInput {
    pub fn into_text(self) -> String {
        match self {
            UserInput::Local(s) | UserInput::Remote(s) => s,
        }
    }

    pub fn source_label(&self) -> &'static str {
        match self {
            UserInput::Local(_) => "stdin",
            UserInput::Remote(_) => "telegram",
        }
    }
}

pub struct InputChannel {
    pub bridge: Option<Arc<TelegramBridge>>,
    rx: Receiver<UserInput>,
}

impl InputChannel {
    /// Spawn one stdin reader and (if `bridge` is `Some`) one Telegram
    /// long-poll worker. Both feed the same channel; whichever produces
    /// input first wins.
    pub fn start(bridge: Option<Arc<TelegramBridge>>) -> Self {
        let (tx, rx) = mpsc::channel::<UserInput>();

        let tx_stdin = tx.clone();
        thread::spawn(move || {
            let stdin = io::stdin();
            for line in stdin.lock().lines() {
                let Ok(l) = line else { return };
                if tx_stdin.send(UserInput::Local(l)).is_err() {
                    return;
                }
            }
        });

        if let Some(ref b) = bridge {
            let bridge_clone = Arc::clone(b);
            let tx_tg = tx;
            thread::spawn(move || {
                let mut offset = 0i64;
                loop {
                    match bridge_clone.poll_once(offset, 30) {
                        Ok((maybe, new_offset)) => {
                            offset = new_offset;
                            if let Some(text) = maybe {
                                if tx_tg.send(UserInput::Remote(text)).is_err() {
                                    return;
                                }
                            }
                        }
                        Err(e) => {
                            eprintln!("[telegram poll] {}", e);
                            thread::sleep(Duration::from_secs(5));
                        }
                    }
                }
            });
        }

        Self { bridge, rx }
    }

    /// Drain anything currently buffered. Use at the start of each new
    /// prompt so input typed before the prompt was shown is discarded.
    pub fn drain_stale(&self) {
        while self.rx.try_recv().is_ok() {}
    }

    pub fn recv(&self) -> Result<UserInput, String> {
        self.rx
            .recv()
            .map_err(|e| format!("input channel closed: {}", e))
    }
}

// ── Setup wizard (telegram-setup subcommand) ──────────────────────────────

pub fn setup_interactive() -> Result<(), String> {
    println!("azul-doc Telegram setup");
    println!("=======================");
    println!();
    println!("This connects azul-doc to a Telegram bot so you can answer");
    println!("apply-midlevel (and other interactive) prompts from your phone.");
    println!();

    let existing = TelegramConfig::load_from_file().ok().flatten();
    if let Some(ref c) = existing {
        let preview: String = c.token.chars().take(10).collect();
        println!(
            "Existing config found: chat_id={}, token={}...",
            c.chat_id, preview
        );
        println!("Press Enter at the token prompt to keep the existing token.");
        println!();
    }

    println!("Step 1: Create a Telegram bot");
    println!("------------------------------");
    println!("  1. Open Telegram and message @BotFather");
    println!("  2. Send /newbot, follow the prompts (any name + handle ending in 'bot')");
    println!("  3. BotFather replies with a token like:");
    println!("       1234567890:ABCdef-GhI_JklM-NoPq...");
    println!();
    print!("  Paste token: ");
    io::stdout().flush().ok();
    let mut token_in = String::new();
    io::stdin()
        .read_line(&mut token_in)
        .map_err(|e| format!("stdin: {}", e))?;
    let token_in = token_in.trim().to_string();
    let token = if token_in.is_empty() {
        existing
            .as_ref()
            .map(|c| c.token.clone())
            .ok_or_else(|| "no existing token; must paste one".to_string())?
    } else {
        token_in
    };

    let agent = make_https_agent();
    let me_url = format!("https://api.telegram.org/bot{}/getMe", token);
    let me: serde_json::Value = agent
        .get(&me_url)
        .call()
        .map_err(|e| format!("getMe: {} (token wrong?)", e))?
        .into_body()
        .read_json()
        .map_err(|e| format!("getMe parse: {}", e))?;
    if !me.get("ok").and_then(|v| v.as_bool()).unwrap_or(false) {
        return Err(format!("getMe response not ok: {}", me));
    }
    let username = me
        .pointer("/result/username")
        .and_then(|v| v.as_str())
        .unwrap_or("?");
    let bot_id = me.pointer("/result/id").and_then(|v| v.as_i64()).unwrap_or(0);
    println!("  Token works: bot @{} (id {})", username, bot_id);
    println!();

    println!("Step 2: Pair with your account");
    println!("------------------------------");
    println!(
        "  Open Telegram on your phone, find @{}, send /start.",
        username
    );
    println!("  (If you've messaged this bot before, send any new message.)");
    println!();
    print!("  Waiting for first message");
    io::stdout().flush().ok();

    let mut offset = 0i64;
    let chat_id = loop {
        let url = format!(
            "https://api.telegram.org/bot{}/getUpdates?offset={}&timeout=15",
            token, offset,
        );
        let parsed: serde_json::Value = agent
            .get(&url)
            .call()
            .map_err(|e| format!("getUpdates: {}", e))?
            .into_body()
            .read_json()
            .map_err(|e| format!("getUpdates parse: {}", e))?;
        let updates = parsed
            .get("result")
            .and_then(|r| r.as_array())
            .cloned()
            .unwrap_or_default();

        let mut new_chat: Option<(i64, String)> = None;
        for upd in &updates {
            let id = upd.get("update_id").and_then(|v| v.as_i64()).unwrap_or(0);
            offset = offset.max(id + 1);
            if let Some(c) = upd.pointer("/message/chat/id").and_then(|v| v.as_i64()) {
                let name = upd
                    .pointer("/message/from/first_name")
                    .and_then(|v| v.as_str())
                    .unwrap_or("?")
                    .to_string();
                new_chat = Some((c, name));
            }
        }
        if let Some((cid, name)) = new_chat {
            println!();
            println!("  Got message from chat_id {} (\"{}\")", cid, name);
            break cid;
        } else {
            print!(".");
            io::stdout().flush().ok();
        }
    };

    let cfg = TelegramConfig {
        token: token.clone(),
        chat_id,
    };
    let path = cfg.save_to_file()?;
    println!();
    println!("Step 3: Save");
    println!("------------");
    println!("  Saved {} (mode 0600)", path.display());

    let bridge = TelegramBridge::from_config(cfg);
    bridge.send_message(
        "azul-doc paired.\n\nYou'll see prompts here when running:\n  azul-doc autoreview apply-midlevel ...",
        None,
    )?;
    println!("  Sent a test message — check your Telegram.");
    println!();
    println!("Done. Disable for any single run with --no-telegram.");
    Ok(())
}
