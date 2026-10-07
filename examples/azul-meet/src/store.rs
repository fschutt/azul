//! AzMeet's files in the Azlin data tree (azul-appkit's data root: `--data-dir`, `$AZLIN_DATA`,
//! else `<data dir>/Azlin`), keyed as the user's S3 bucket will be (user ruling: meeting data =
//! files in a per-meeting folder):
//!
//! - `meet/settings.json`: azul-appkit's settings file, which the kit (`azul_appkit::ui::Kit`)
//!   reads at start and writes - the app theme and mode, and AzMeet's values ([`Prefs`]): the
//!   meeting server (`server`), the name others see (`name`), the video quality (`quality`), the
//!   devices (`microphone`, `speaker`, `camera`), `mirror`, `join_muted`, `join_camera_off`;
//! - `meet/<meeting>/meeting.json`: the meeting - its key, link, meeting server, when this side
//!   joined, who was there;
//! - `meet/<meeting>/chat.jsonl`: the call's chat, one JSON object per line.
//!
//! The settings are read once at start (before the window exists) and written by the kit; every
//! other write runs on an azul Thread through azul-storage's `LocalDrive` (an `S3Drive` later),
//! never in a callback.

use std::{
    path::{Path, PathBuf},
    sync::{Mutex, MutexGuard, PoisonError},
};

use azul::{
    callbacks::WriteBackCallbackType,
    file::FilePath,
    prelude::*,
    task::{Thread, ThreadId, ThreadReceiveMsg, ThreadReceiver, ThreadSender, ThreadWriteBackMsg},
};
use azul_appkit::{
    files::{run_jobs, FileJob, FileOutcome},
    settings::AppSettings,
};
use azul_storage::{Drive, LocalDrive};
use serde::{Deserialize, Serialize};

use crate::chat::ChatMessage;

/// AzMeet's folder in the data tree.
pub const APP_FOLDER: &str = "meet";
/// The settings values AzMeet keeps besides the theme and the mode.
pub const SERVER: &str = "server";
pub const NAME: &str = "name";
pub const QUALITY: &str = "quality";
pub const MICROPHONE: &str = "microphone";
pub const SPEAKER: &str = "speaker";
pub const CAMERA: &str = "camera";
pub const MIRROR: &str = "mirror";
pub const JOIN_MUTED: &str = "join_muted";
pub const JOIN_CAMERA_OFF: &str = "join_camera_off";
/// The longest meeting folder name.
const MAX_FOLDER: usize = 64;
/// The video qualities as the settings file names them, in the settings' order (automatic up to
/// 720p, data saver up to 360p, low up to 180p).
pub const QUALITY_NAMES: [&str; 3] = ["automatic", "data-saver", "low"];
/// The cameras as the settings file names them, in the settings' order (by facing: there is no
/// camera list).
pub const CAMERA_NAMES: [&str; 3] = ["front", "back", "external"];

/// What AzMeet remembers besides the app theme and the mode.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Prefs {
    /// The meeting server that answered last.
    pub server: Option<String>,
    /// The name others see, as typed in the waiting room or the settings.
    pub name: Option<String>,
    /// The video quality: an index into [`QUALITY_NAMES`].
    pub quality: usize,
    /// The microphone picked, by name; `None`: the system's default.
    pub microphone: Option<String>,
    /// The speaker picked, by name; `None`: the system's default.
    pub speaker: Option<String>,
    /// The camera picked: an index into [`CAMERA_NAMES`].
    pub camera: usize,
    /// This side's own picture is shown mirrored, as in a mirror (what the others get never is).
    pub mirror: bool,
    /// A meeting is joined with the microphone off: the waiting room's switch starts off.
    pub join_muted: bool,
    /// A meeting is joined with the camera off: the waiting room's switch starts off.
    pub join_camera_off: bool,
}

impl Default for Prefs {
    /// Nothing remembered: automatic quality, the system's devices, the front camera, the own
    /// picture mirrored, the microphone and the camera on in the waiting room.
    fn default() -> Self {
        Prefs {
            server: None,
            name: None,
            quality: 0,
            microphone: None,
            speaker: None,
            camera: 0,
            mirror: true,
            join_muted: false,
            join_camera_off: false,
        }
    }
}

/// The index of `value` in `names` (case-insensitive), else 0.
fn index_of(value: Option<&str>, names: &[&str]) -> usize {
    value
        .and_then(|v| names.iter().position(|n| n.eq_ignore_ascii_case(v.trim())))
        .unwrap_or(0)
}

impl Prefs {
    /// What `settings` remember; an unknown quality or camera is the first, an empty name or
    /// device none, a switch that does not read is its default.
    #[must_use]
    pub fn read(settings: &AppSettings) -> Prefs {
        let text = |key: &str| {
            settings
                .get(key)
                .map(str::trim)
                .filter(|v| !v.is_empty())
                .map(str::to_string)
        };
        let defaults = Prefs::default();
        Prefs {
            server: text(SERVER),
            name: text(NAME),
            quality: index_of(settings.get(QUALITY), &QUALITY_NAMES),
            microphone: text(MICROPHONE),
            speaker: text(SPEAKER),
            camera: index_of(settings.get(CAMERA), &CAMERA_NAMES),
            mirror: settings.get_bool(MIRROR, defaults.mirror),
            join_muted: settings.get_bool(JOIN_MUTED, defaults.join_muted),
            join_camera_off: settings.get_bool(JOIN_CAMERA_OFF, defaults.join_camera_off),
        }
    }

    /// Writes these into `settings` (a missing server, name or device is removed).
    pub fn write(&self, settings: &mut AppSettings) {
        for (key, value) in [
            (SERVER, &self.server),
            (NAME, &self.name),
            (MICROPHONE, &self.microphone),
            (SPEAKER, &self.speaker),
        ] {
            match value {
                Some(value) => settings.set(key, value),
                None => {
                    settings.values.remove(key);
                }
            }
        }
        settings.set(QUALITY, QUALITY_NAMES[self.quality.min(QUALITY_NAMES.len() - 1)]);
        settings.set(CAMERA, CAMERA_NAMES[self.camera.min(CAMERA_NAMES.len() - 1)]);
        settings.set_bool(MIRROR, self.mirror);
        settings.set_bool(JOIN_MUTED, self.join_muted);
        settings.set_bool(JOIN_CAMERA_OFF, self.join_camera_off);
    }
}

/// `meet/settings.json`.
#[must_use]
pub fn settings_key() -> String {
    azul_appkit::data::app_key(APP_FOLDER, azul_appkit::settings::SETTINGS_FILE)
}

/// The folder of meeting `meeting` (its room key, or the demo's code): `meet/<meeting>/`, every
/// character but a letter, a digit, `-` and `_` an underscore, at most 64 of them; `None` for a
/// meeting without a name.
#[must_use]
pub fn meeting_folder(meeting: &str) -> Option<String> {
    let meeting = meeting.trim();
    if meeting.is_empty() {
        return None;
    }
    let safe: String = meeting
        .chars()
        .take(MAX_FOLDER)
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' || c == '_' {
                c
            } else {
                '_'
            }
        })
        .collect();
    Some(azul_appkit::data::app_prefix(&format!("{APP_FOLDER}/{safe}")))
}

/// `meet/<meeting>/chat.jsonl`.
#[must_use]
pub fn chat_key(meeting: &str) -> Option<String> {
    meeting_folder(meeting).map(|folder| format!("{folder}chat.jsonl"))
}

/// `meet/<meeting>/meeting.json`.
#[must_use]
pub fn meeting_key(meeting: &str) -> Option<String> {
    meeting_folder(meeting).map(|folder| format!("{folder}meeting.json"))
}

/// One line of `chat.jsonl`.
#[derive(Serialize)]
struct ChatLine<'a> {
    name: &'a str,
    text: &'a str,
    mine: bool,
}

/// The chat as `chat.jsonl`: one `{"name", "text", "mine"}` object per message, oldest first,
/// every line ending in a newline.
#[must_use]
pub fn chat_lines(messages: &[ChatMessage]) -> String {
    let mut out = String::new();
    for m in messages {
        let line = ChatLine {
            name: &m.name,
            text: &m.text,
            mine: m.mine,
        };
        if let Ok(json) = serde_json::to_string(&line) {
            out.push_str(&json);
            out.push('\n');
        }
    }
    out
}

/// The messages of a `chat.jsonl` (see [`chat_lines`]), oldest first: this side's (`mine`) sent
/// by `me`, the others' by nobody known now (`from` 0, they are only listed). A line that is no
/// such object is skipped.
#[must_use]
pub fn parse_chat(text: &str, me: u64) -> Vec<ChatMessage> {
    #[derive(Deserialize)]
    struct Line {
        name: String,
        text: String,
        #[serde(default)]
        mine: bool,
    }
    text.lines()
        .filter_map(|line| serde_json::from_str::<Line>(line.trim()).ok())
        .map(|l| ChatMessage {
            from: if l.mine { me } else { 0 },
            mine: l.mine,
            name: l.name,
            text: l.text,
        })
        .collect()
}

/// The people an earlier `meeting.json` lists (none for a file that does not read).
#[must_use]
pub fn record_people(text: &str) -> Vec<String> {
    #[derive(Deserialize)]
    struct People {
        #[serde(default)]
        people: Vec<String>,
    }
    serde_json::from_str::<People>(text)
        .map(|p| p.people)
        .unwrap_or_default()
}

/// What `meeting.json` says about a meeting.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct MeetingRecord {
    /// The room key (or the demo's code).
    pub meeting: String,
    /// The link others join with ("" in the demo).
    pub link: String,
    /// The meeting server ("" in the demo).
    pub server: String,
    /// When this side joined, seconds since 1970.
    pub joined: u64,
    /// Everyone this side met in the call, this side first.
    pub people: Vec<String>,
}

impl MeetingRecord {
    /// The record as pretty JSON with a trailing newline.
    #[must_use]
    pub fn to_json(&self) -> String {
        let mut text = serde_json::to_string_pretty(self).unwrap_or_else(|_| String::from("{}"));
        text.push('\n');
        text
    }

    /// Adds `name` to the people met, once.
    pub fn met(&mut self, name: &str) -> bool {
        let name = name.trim();
        if name.is_empty() || self.people.iter().any(|p| p == name) {
            return false;
        }
        self.people.push(name.to_string());
        true
    }
}

// ==== Reading at start, writing on a Thread ====

/// The data root: `--data-dir`, `$AZLIN_DATA`, else `<data dir>/Azlin` (azul-appkit's rule).
#[must_use]
pub fn data_root(flag: Option<&Path>) -> PathBuf {
    let os_dir = FilePath::get_data_dir()
        .into_option()
        .map(|dir| PathBuf::from(dir.inner.as_str()))
        .filter(|p| !p.as_os_str().is_empty());
    azul_appkit::data::data_root(
        flag,
        std::env::var(azul_appkit::data::DATA_VAR).ok().as_deref(),
        os_dir,
    )
}

/// The settings saved last time (the defaults without a file), read once before the window
/// opens - no callback waits on it.
#[must_use]
pub fn load_settings(root: &Path) -> AppSettings {
    let key = settings_key();
    match LocalDrive::new(root.to_path_buf()).get(&key) {
        Ok(bytes) => {
            let (settings, problem) = AppSettings::parse(&String::from_utf8_lossy(&bytes));
            if let Some(problem) = problem {
                eprintln!("[azmeet] {key}: {problem}");
            }
            settings
        }
        Err(_) => AppSettings::default(),
    }
}

/// The files waiting for the save thread, and whether one runs. ONE thread writes at a time, so
/// an older save never lands over a newer one.
struct Queue {
    files: Vec<Pending>,
    busy: bool,
}

static QUEUE: Mutex<Queue> = Mutex::new(Queue {
    files: Vec::new(),
    busy: false,
});

fn queue() -> MutexGuard<'static, Queue> {
    QUEUE.lock().unwrap_or_else(PoisonError::into_inner)
}

/// What a save hands back to the UI thread.
struct Saved {
    outcomes: Vec<FileOutcome>,
}

/// Runs on the worker thread: takes what waits, writes it into the data tree, hands the outcomes
/// back, until nothing waits (then the next save starts a new thread).
extern "C" fn save_thread(_init: RefAny, mut sender: ThreadSender, _receiver: ThreadReceiver) {
    loop {
        let files = {
            let mut q = queue();
            if q.files.is_empty() {
                q.busy = false;
                return;
            }
            std::mem::take(&mut q.files)
        };
        let mut outcomes = Vec::with_capacity(files.len());
        for (root, key, bytes) in files {
            let drive = LocalDrive::new(root);
            outcomes.extend(run_jobs(&drive, vec![FileJob::Put { key, bytes }]));
        }
        let _sent = sender.send(ThreadReceiveMsg::WriteBack(ThreadWriteBackMsg::create(
            on_saved,
            RefAny::new(Saved { outcomes }),
        )));
    }
}

/// On the UI thread: `AZMEET_SAVED <key>` per file written (for scripts), errors on stderr.
extern "C" fn on_saved(_data: RefAny, mut msg: RefAny, _info: CallbackInfo) -> Update {
    let Some(saved) = msg.downcast_ref::<Saved>() else {
        return Update::DoNothing;
    };
    for outcome in &saved.outcomes {
        match (outcome, outcome.error()) {
            (FileOutcome::Put { key, .. }, None) => println!("AZMEET_SAVED {key}"),
            (_, Some(e)) => eprintln!("[azmeet] not saved: {e}"),
            _ => {}
        }
    }
    Update::DoNothing
}

/// A file waiting to be written: the data root, the key, the bytes.
type Pending = (PathBuf, String, Vec<u8>);

/// Queues `files` under `root` behind what waits already; a file queued again replaces its older
/// bytes (one write, the newer bytes, in the newer place).
fn enqueue(queue: &mut Vec<Pending>, root: &Path, files: Vec<(String, Vec<u8>)>) {
    for (key, bytes) in files {
        queue.retain(|(r, k, _)| !(r == root && *k == key));
        queue.push((root.to_path_buf(), key, bytes));
    }
}

/// Writes `files` (key, bytes) into the data tree at `root` on an azul Thread: queued, and a
/// thread started when none runs.
pub fn save(info: &mut CallbackInfo, root: &Path, files: Vec<(String, Vec<u8>)>) {
    if files.is_empty() {
        return;
    }
    let start = {
        let mut q = queue();
        enqueue(&mut q.files, root, files);
        !std::mem::replace(&mut q.busy, true)
    };
    if start {
        info.add_thread(
            ThreadId::unique(),
            Thread::create(RefAny::new(()), RefAny::new(()), save_thread),
        );
    }
}

/// What a meeting's files held when this side came back to it, read on an azul Thread: the
/// meeting (its folder name), its `chat.jsonl` and its `meeting.json` (`None`: not there).
pub struct Earlier {
    pub meeting: String,
    pub chat: Option<String>,
    pub record: Option<String>,
}

/// What the read thread needs: where, which meeting, whom to tell.
struct ReadInit {
    root: PathBuf,
    meeting: String,
    on_read: WriteBackCallbackType,
}

/// Runs on the worker thread: reads the meeting's two files from the data tree and hands them
/// back as an [`Earlier`].
extern "C" fn read_thread(mut init: RefAny, mut sender: ThreadSender, _receiver: ThreadReceiver) {
    let Some((root, meeting, on_read)) = init
        .downcast_ref::<ReadInit>()
        .map(|i| (i.root.clone(), i.meeting.clone(), i.on_read))
    else {
        return;
    };
    let drive = LocalDrive::new(root);
    let read = |key: Option<String>| {
        key.and_then(|k| drive.get(&k).ok())
            .map(|bytes| String::from_utf8_lossy(&bytes).into_owned())
    };
    let earlier = Earlier {
        chat: read(chat_key(&meeting)),
        record: read(meeting_key(&meeting)),
        meeting,
    };
    let _sent = sender.send(ThreadReceiveMsg::WriteBack(ThreadWriteBackMsg::create(
        on_read,
        RefAny::new(earlier),
    )));
}

/// Reads meeting `meeting`'s files from the data tree at `root` on an azul Thread (never here);
/// `on_read(reply_to, Earlier, info)` gets them on the UI thread.
pub fn read_meeting(
    info: &mut CallbackInfo,
    root: &Path,
    meeting: &str,
    reply_to: RefAny,
    on_read: WriteBackCallbackType,
) {
    let init = RefAny::new(ReadInit {
        root: root.to_path_buf(),
        meeting: meeting.to_string(),
        on_read,
    });
    info.add_thread(ThreadId::unique(), Thread::create(init, reply_to, read_thread));
}

#[cfg(test)]
mod tests {
    use super::*;

    fn message(name: &str, text: &str, mine: bool) -> ChatMessage {
        ChatMessage {
            from: 1,
            mine,
            name: name.to_string(),
            text: text.to_string(),
        }
    }

    #[test]
    fn the_settings_and_a_meeting_live_in_the_meet_folder() {
        assert_eq!(settings_key(), "meet/settings.json");
        assert_eq!(meeting_folder("abc-defg-hij").as_deref(), Some("meet/abc-defg-hij/"));
        assert_eq!(chat_key("abc-defg-hij").as_deref(), Some("meet/abc-defg-hij/chat.jsonl"));
        assert_eq!(
            meeting_key("abc-defg-hij").as_deref(),
            Some("meet/abc-defg-hij/meeting.json")
        );
    }

    #[test]
    fn a_meeting_folder_is_one_safe_segment_or_none() {
        assert_eq!(meeting_folder("../../etc").as_deref(), Some("meet/______etc/"));
        assert_eq!(meeting_folder("a b/c").as_deref(), Some("meet/a_b_c/"));
        assert_eq!(meeting_folder("   "), None);
        assert_eq!(meeting_folder(""), None);
        let long = "x".repeat(200);
        assert_eq!(meeting_folder(&long).map(|f| f.len()), Some("meet/".len() + 64 + 1));
    }

    #[test]
    fn the_chat_is_one_json_object_per_line_oldest_first() {
        let lines = chat_lines(&[
            message("Ada", "Hello \"Ben\"", true),
            message("Ben", "line one\nline two", false),
        ]);
        let parsed: Vec<serde_json::Value> = lines
            .lines()
            .map(|l| serde_json::from_str(l).expect("a JSON object per line"))
            .collect();
        assert_eq!(parsed.len(), 2);
        assert_eq!(parsed[0]["name"], "Ada");
        assert_eq!(parsed[0]["text"], "Hello \"Ben\"");
        assert_eq!(parsed[0]["mine"], true);
        assert_eq!(parsed[1]["text"], "line one\nline two", "a newline stays inside its line");
        assert!(lines.ends_with('\n'));
        assert_eq!(chat_lines(&[]), "");
    }

    #[test]
    fn a_chat_file_reads_back_as_the_messages_it_holds() {
        let messages = vec![
            message("Ada", "Hello \"Ben\"", true),
            message("Ben", "line one\nline two", false),
        ];
        let text = format!("{}not json\n\n{{\"name\": 3}}\n", chat_lines(&messages));
        let back = parse_chat(&text, 7);
        assert_eq!(back.len(), 2, "the broken lines are skipped: {back:?}");
        assert_eq!((back[0].from, back[0].mine, back[0].name.as_str()), (7, true, "Ada"));
        assert_eq!(back[0].text, "Hello \"Ben\"");
        assert_eq!((back[1].from, back[1].mine, back[1].text.as_str()), (0, false, "line one\nline two"));
    }

    #[test]
    fn an_earlier_record_says_who_was_there() {
        let mut record = MeetingRecord {
            meeting: String::from("abc"),
            people: vec![String::from("Ada")],
            ..MeetingRecord::default()
        };
        record.met("Ben");
        assert_eq!(record_people(&record.to_json()), vec!["Ada", "Ben"]);
        assert!(record_people("{").is_empty());
    }

    #[test]
    fn the_server_the_name_and_the_quality_are_remembered_in_the_settings_file() {
        let mut settings = AppSettings::default();
        assert_eq!(Prefs::read(&settings), Prefs::default());
        let prefs = Prefs {
            server: Some(String::from("https://meet.example.com")),
            name: Some(String::from("Ada")),
            quality: 2,
            ..Prefs::default()
        };
        prefs.write(&mut settings);
        assert_eq!(settings.get(SERVER), Some("https://meet.example.com"));
        assert_eq!(settings.get(NAME), Some("Ada"));
        assert_eq!(settings.get(QUALITY), Some("low"));
        let (back, problem) = AppSettings::parse(&settings.to_json());
        assert_eq!(problem, None);
        assert_eq!(Prefs::read(&back), prefs, "through the file and back");
        Prefs::default().write(&mut settings);
        assert_eq!(settings.get(SERVER), None, "no server, no line");
        assert_eq!(settings.get(QUALITY), Some("automatic"));
    }

    #[test]
    fn the_devices_the_mirror_and_how_to_join_are_remembered_in_the_settings_file() {
        let fresh = Prefs::read(&AppSettings::default());
        assert!(fresh.mirror, "the own picture is mirrored until the user says otherwise");
        assert!(!fresh.join_muted && !fresh.join_camera_off, "the waiting room starts live");
        assert_eq!((fresh.microphone.as_deref(), fresh.camera), (None, 0));
        let prefs = Prefs {
            microphone: Some(String::from("USB Microphone")),
            speaker: Some(String::from("Headphones")),
            camera: 2,
            mirror: false,
            join_muted: true,
            join_camera_off: true,
            ..Prefs::default()
        };
        let mut settings = AppSettings::default();
        prefs.write(&mut settings);
        assert_eq!(settings.get(CAMERA), Some("external"));
        assert_eq!(settings.get(MIRROR), Some("false"));
        assert_eq!(settings.get(JOIN_MUTED), Some("true"));
        let (back, _) = AppSettings::parse(&settings.to_json());
        assert_eq!(Prefs::read(&back), prefs, "through the file and back");
        // Back to the system's devices: the lines go.
        Prefs::default().write(&mut settings);
        assert_eq!(settings.get(MICROPHONE), None);
        assert_eq!(settings.get(SPEAKER), None);
        settings.set(CAMERA, "periscope");
        settings.set(MIRROR, "maybe");
        let odd = Prefs::read(&settings);
        assert_eq!((odd.camera, odd.mirror), (0, true), "what does not read is the default");
    }

    #[test]
    fn an_unknown_quality_or_a_blank_name_reads_as_the_default() {
        let mut settings = AppSettings::default();
        settings.set(QUALITY, "ultra");
        settings.set(NAME, "   ");
        settings.set(SERVER, "https://meet.example.com");
        let prefs = Prefs::read(&settings);
        assert_eq!(prefs.quality, 0);
        assert_eq!(prefs.name, None);
        assert_eq!(prefs.server.as_deref(), Some("https://meet.example.com"));
        settings.set(QUALITY, "data-saver");
        assert_eq!(Prefs::read(&settings).quality, 1);
    }

    #[test]
    fn a_file_saved_again_before_the_thread_writes_it_is_written_once_with_the_newer_bytes() {
        let root = PathBuf::from("/data");
        let mut queue = Vec::new();
        enqueue(&mut queue, &root, vec![(String::from("meet/a/chat.jsonl"), b"1".to_vec())]);
        enqueue(&mut queue, &root, vec![(String::from("meet/settings.json"), b"s".to_vec())]);
        enqueue(&mut queue, &root, vec![(String::from("meet/a/chat.jsonl"), b"12".to_vec())]);
        let keys: Vec<(&str, &[u8])> =
            queue.iter().map(|(_, k, b)| (k.as_str(), b.as_slice())).collect();
        assert_eq!(
            keys,
            vec![("meet/settings.json", &b"s"[..]), ("meet/a/chat.jsonl", &b"12"[..])],
            "the older bytes are dropped, the newer go last"
        );
        enqueue(&mut queue, &PathBuf::from("/other"), vec![(String::from("meet/settings.json"), b"o".to_vec())]);
        assert_eq!(queue.len(), 3, "the same key under another root is another file");
    }

    #[test]
    fn a_meeting_record_lists_everyone_met_once() {
        let mut record = MeetingRecord {
            meeting: String::from("abc-defg-hij"),
            people: vec![String::from("Ada")],
            ..MeetingRecord::default()
        };
        assert!(record.met("Ben"));
        assert!(!record.met("Ben"), "once");
        assert!(!record.met("Ada"));
        assert_eq!(record.people, vec!["Ada", "Ben"]);
        let json: serde_json::Value = serde_json::from_str(&record.to_json()).unwrap();
        assert_eq!(json["meeting"], "abc-defg-hij");
        assert_eq!(json["people"][1], "Ben");
    }
}
