//! AzCalendar: a calendar like Outlook 2010's, on the public azul API, that makes AzMeet links
//! online or not.
//!
//! The window is the Office scaffold (`OfficeShell`, the same frame AzMail and the other azul
//! apps use): the title row, the ribbon (FILE opens the backstage: Info, Open & Export, Print,
//! Calendars, Options, About; HOME: New Appointment, New Meeting (with an AzMeet link), Go To
//! Today / Next 7 Days, Arrange Day / Work Week / Week / Month / Schedule View, Manage
//! Calendars, Share; VIEW: the list, the navigation pane, the To-Do bar, flat / flora, light /
//! dark), the navigation pane (the date navigator, "My calendars" with their colours, the
//! module switcher), the calendar's view, the To-Do bar (appointments and tasks) and the
//! status bar (how many items, whether the meeting links reached their server).
//!
//! The views (`views.rs`): Day, Work Week and Week are hours down the page (`timegrid.rs`, CAL2's
//! week: the whole day, scrolling, zooming around the pointer, a click or a drag makes a draft
//! with a popover); Month has "+N more" where a day is full, the Schedule View the day's hours
//! across with a row per calendar, the List the coming days (`views_ui.rs`).
//!
//! FILE > Print (`print.rs`, `print_ui.rs`) is Outlook 2010's print page: Daily, Weekly Agenda
//! or Monthly style, a range, a preview of the printout and Print, which saves it as a PDF made
//! by azul's PDF writer from a DOM laid out for paper.
//!
//! An event is edited in a window of its own (`editor_ui.rs`, `editor.rs`): title, location,
//! start and end (date and time), all day, repeat (an RRULE subset, `rrule.rs`), reminder,
//! calendar, attendees, notes and "Add AzMeet link".
//!
//! Save writes the event as one JSON file, `<data dir>/events/<uuid>.json` (see `event.rs`), at
//! once. With "Add AzMeet link" the link is made HERE (a room id drawn like the meeting server's
//! own), so a new event with a link works offline; the file keeps it `pending` until the meeting
//! server has registered the room (`POST /rooms {room, starts_at, ends_at}`, the event's day and
//! times in UTC, on an azul `Thread` so no callback waits on the network). Pending links are sent
//! at start, after every save, every `AZCAL_SYNC_SECONDS` (30) and on "Sync meeting links now";
//! the server's answer (its code, and the times it keeps the room for) is written back. An event
//! with a registered link shows "Join meeting", which starts AzMeet with `AZMEET_JOIN=<link>`
//! (or, without AzMeet, copies the link).
//!
//! Durable data is files only: events, calendars (`calendars.rs`), tasks (`tasks.rs`); .ics
//! files are imported and exported (`ics.rs`). The meeting server holds the rooms (registering
//! and joining), nothing of the calendar. The meeting server is `--worker` for one run, else the
//! one chosen in FILE > Options (`<data dir>/settings.txt`, AzMeet's settings format), else
//! `AZMEET_WORKER`, else `endpoints.meet` of the shared Azlin config (`~/.azlin/config.json`, or
//! the file `AZLIN_CONFIG` names), else the built-in one, else AzMeet's local development server:
//! there always is one. The same file keeps the zoom, the view, the hidden calendars and the
//! panes shown (`settings.rs`).
//!
//! Environment:
//! - `AZCAL_DATA`: the data folder (default: `AzCalendar` in the user's data folder).
//! - `AZMEET_WORKER`: the meeting server when none is saved, as for AzMeet (default: the shared
//!   config's `endpoints.meet`, else AzMeet's built-in one, set at build time with
//!   `AZMEET_DEFAULT_WORKER`).
//! - `AZLIN_CONFIG`: another shared Azlin config file (the local stack's profile).
//! - `AZCAL_SYNC_SECONDS`: how often pending links are sent again (default 30).
//! - `AZMEET_BIN`: the AzMeet program "Join meeting" starts (default: `AzMeet` next to AzCalendar).
//! - `AZMAIL_BIN`: the AzMail program the module switcher's Mail starts (default: next to it).
//!
//! On stdout, for scripts: `AZCAL_VIEW <view> <first day> <last day>` when the view changes,
//! `AZCAL_SAVED <file>` and `AZCAL_LINK <link>` when an event is saved, `AZCAL_DELETED <id>`,
//! `AZCAL_SYNCED <link>` when a link's room is registered, `AZCAL_EDITOR <open|closed>`,
//! `AZCAL_IMPORTED <count> <file>`, `AZCAL_EXPORTED <count> <file>`, `AZCAL_REMINDER <title>`,
//! `AZCAL_JOIN_PID <pid>` when "Join meeting" started AzMeet, `AZCAL_PRINT_PREVIEW <style>
//! <pages>` when FILE > Print's preview was made, `AZCAL_PRINTED <style> <bytes>` when a
//! printout was saved.

// The events without azul types live in azul-calendar-core, so a headless process (the Azlin
// Bridge's CalDAV) runs the same code; they keep their module names here (`crate::event`).
pub use azcal_core::{calendars, event, ics, meet_rooms};

pub mod args;
pub mod editor;
pub mod meeting;
pub mod print;
pub use azul_pim::rrule;
pub mod sample;
pub mod settings;
pub mod store;
pub mod tasks;
// A temporary folder for tests (the one the PIM apps share).
#[cfg(test)]
use azul_pim::testing as test_dir;
pub mod views;
pub mod week;

mod chrome;
mod editor_ui;
mod ids;
mod print_ui;
mod timegrid;
mod views_ui;
mod writes;
mod l10n;
#[cfg(test)]
mod l10n_tests;

/// AzMeet's invite secrets (`invite.rs`, CRYPTO.md section 4): a link made here carries one, and
/// its room is registered with the invite key, so the meeting is end-to-end encrypted.
#[path = "../../azul-meet/src/invite.rs"]
pub mod meet_invite;

use std::{
    collections::{BTreeMap, BTreeSet},
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    time::Instant,
};

use azul::{
    callbacks::CallbackType,
    css::DarkLightMode,
    dom::{ClipboardContent, DomId, VirtualKeyCode},
    error::HttpError,
    file::FilePath,
    http::{HttpGetResult, HttpMethod, HttpRequestConfig},
    menu::{Menu, MenuItem, StringMenuItem},
    option::OptionDarkLightMode,
    prelude::*,
    shells::{ShellThemeAccent, ShellThemeScope},
    str::String as AzString,
    time::SystemTimeDiff,
    vec::{StyledTextRunVec, U8Vec},
    window::WindowDecorations,
};
use azul_appkit::args::LanguagePref;
use azul_pim::write_queue::WriteQueue;
use chrono::{Datelike, NaiveDate, NaiveTime, Timelike};

use crate::{
    args::{Args, BackstagePage, Screen},
    calendars::{Calendar, Colour},
    editor::EditorForm,
    event::{Event, Meeting},
    tasks::Task,
    timegrid::{Draft, Press},
    views::{Occurrence, ViewKind},
};

/// The data folder's variable.
const DATA_VAR: &str = "AZCAL_DATA";
const HTTP_TIMEOUT_SECS: u64 = 10;
/// How often pending links are sent again, in seconds: the variable, and the default.
const SYNC_VAR: &str = "AZCAL_SYNC_SECONDS";
const SYNC_SECONDS: u64 = 30;
/// How often due reminders are looked for, in milliseconds.
const REMINDER_TICK_MS: u64 = 20_000;
/// The main window's id, and the editor window's (what a script routes a request by).
pub(crate) const MAIN_WINDOW_ID: &str = "azcalendar";
pub(crate) const EDITOR_WINDOW_ID: &str = "azcalendar-editor";
/// What an event without a title is called.
pub(crate) const UNTITLED: &str = "(No title)";

// Light and dark: the page follows the mode the window is in, like the widgets on it. Surfaces,
// text and rules are `system:` colours, resolved in whichever mode the window is in (the shell
// paints the theme's own ground around them); the few colours of the app's own (the calendars'
// tints in `calendars.rs`, today, the draft, the notice, the error red) carry a dark twin under
// `@media (prefers-color-scheme: dark)`.
//
// Under flora (`@theme(flora)`, after the flat values it outranks) the app's own colours are
// flora's: `system:` keywords where flora names a role (the engine resolves them to flora's
// tokens - the selection's soft wash and deep ink, the desk, the accent), the accent ramp
// (#2F4A85 / #1E3260 / #E0E4EE / #7A93C6: a spin recuts them) and the clay stone for danger;
// the hand is flora's Garamond (the `system:ui` role under flora).
pub(crate) const BODY: &str = "display: flex; flex-direction: column; height: 100%; margin: 0; \
                               font-family: sans-serif; font-size: 14px; color: system:text; \
                               background: system:window-background; \
                               @theme(flora) { font-family: system:ui; }";
pub(crate) const NOTICE: &str = "padding: 6px 16px; font-size: 13px; color: #2c4a7a; background: \
                                 #e6eefc; @media (prefers-color-scheme: dark) { color: #c4d7ff; \
                                 background: #1f2d45; } @theme(flora) { color: \
                                 system:selection-text; background: system:selection-background; }";
pub(crate) const LINE: &str = "system:separator";
pub(crate) const LABEL: &str = "font-size: 12px; color: system:secondary-text; margin-top: 12px; \
                                margin-bottom: 4px; @theme(flora) { font-size: 11px; font-weight: \
                                bold; text-transform: uppercase; letter-spacing: 0.1em; }";
/// A day's surface; today's is tinted with the accent.
pub(crate) const DAY_PAINT: &str = "background: system:control-background;";
pub(crate) const TODAY_PAINT: &str =
    "background: #f7faff; @media (prefers-color-scheme: dark) { background: #1b2433; } \
     @theme(flora) { background: #E0E4EE; @media (prefers-color-scheme: dark) { background: \
     #1E3260; } }";
/// A day of another month in the month grid: dimmer than the month's own (under flora the
/// desk, the recessed band behind the leaves).
pub(crate) const OTHER_MONTH_PAINT: &str =
    "background: #f3f4f6; @media (prefers-color-scheme: dark) { background: #262626; } \
     @theme(flora) { background: system:under-page-background; }";
/// The draft's box: pale, with a dashed accent edge.
pub(crate) const DRAFT_PAINT: &str = "background: #eef4ff; border: 2px dashed #2f6db0; @media \
                                      (prefers-color-scheme: dark) { background: #1a2c4d; \
                                      border: 2px dashed #6ea8ff; } @theme(flora) { background: \
                                      #E0E4EE; border: 2px dashed #2F4A85; @media \
                                      (prefers-color-scheme: dark) { background: #1E3260; \
                                      border: 2px dashed #7A93C6; } }";
/// The draft's title, in the accent.
pub(crate) const DRAFT_TITLE: &str =
    "font-weight: bold; color: #2f6db0; @media (prefers-color-scheme: dark) { color: #8dbbff; } \
     @theme(flora) { color: system:accent; }";
/// The selected event's ring.
pub(crate) const SELECTED_RING: &str = "box-shadow: 0px 0px 0px 2px #2f6db0; @media \
                                        (prefers-color-scheme: dark) { box-shadow: 0px 0px 0px \
                                        2px #8dbbff; } @theme(flora) { box-shadow: 0px 0px 0px \
                                        2px #2F4A85; @media (prefers-color-scheme: dark) { \
                                        box-shadow: 0px 0px 0px 2px #7A93C6; } }";
/// The "now" line in today's column (under flora the clay stone, its glow at night).
pub(crate) const NOW_LINE: &str = "background: #d93025; @theme(flora) { background: #7E4A42; \
                                   @media (prefers-color-scheme: dark) { background: #B3837A; } }";
/// Secondary lines: an event's time, what a form line means.
pub(crate) const SECONDARY: &str = "color: system:secondary-text;";
/// A form's error line.
pub(crate) const ERROR: &str = "font-size: 13px; color: #b3261e; margin-top: 12px; @media \
                                (prefers-color-scheme: dark) { color: #f2b8b5; } @theme(flora) { \
                                color: #7E4A42; @media (prefers-color-scheme: dark) { color: \
                                #B3837A; } }";
/// A block's title: one line, cut with an ellipsis at the block's edge.
pub(crate) const CLIPPED_TITLE: &str = "font-weight: bold; white-space: nowrap; overflow: \
                                        hidden; text-overflow: ellipsis; flex-shrink: 0;";
/// A block's other lines, the same way.
pub(crate) const CLIPPED_LINE: &str = "color: system:secondary-text; white-space: nowrap; \
                                       overflow: hidden; text-overflow: ellipsis; flex-shrink: 0;";
/// The popover's card: the whole of its window.
pub(crate) const POPOVER: &str = "display: flex; flex-direction: column; width: 320px; padding: \
                                  16px; box-sizing: border-box; background: \
                                  system:window-background; border: 1px solid system:separator; \
                                  border-radius: 8px; font-family: sans-serif; font-size: 14px; \
                                  color: system:text; @theme(flora) { border-radius: 5px; \
                                  font-family: system:ui; }";
/// A page of the backstage, and the editor window's body: padded, scrolling.
pub(crate) const PAGE: &str = "display: flex; flex-direction: column; flex-grow: 1; min-height: \
                               0; overflow-y: auto; padding: 20px 28px; color: system:text;";

/// The app.
pub(crate) struct CalState {
    pub(crate) data_dir: PathBuf,
    /// The meeting server new links are registered with (`--worker`, else Options, else
    /// `AZMEET_WORKER`, else the shared config's `endpoints.meet`, else the built-in one, else
    /// AzMeet's local one).
    pub(crate) server: String,
    pub(crate) events: Vec<Event>,
    /// The default calendar first.
    pub(crate) calendars: Vec<Calendar>,
    /// The calendars "My calendars" does not show, by id.
    pub(crate) hidden: BTreeSet<String>,
    pub(crate) tasks: Vec<Task>,
    /// The folder of the task store the To-Do bar reads and writes (`tasks::tasks_root`).
    pub(crate) tasks_root: PathBuf,
    /// The list a task typed into the To-Do bar goes to (the store's default list).
    pub(crate) task_list: String,
    /// The To-Do bar's new-task line as typed.
    pub(crate) task_text: String,
    pub(crate) today: NaiveDate,
    // ---- what the window shows ----
    pub(crate) view: ViewKind,
    /// The day the view is around (`views::days_shown`).
    pub(crate) anchor: NaiveDate,
    /// The month the date navigator shows (its arrows turn it without moving the view).
    pub(crate) nav_month: (i32, u32),
    /// The selected occurrence: the event's id and the day.
    pub(crate) selected: Option<(String, NaiveDate)>,
    pub(crate) todo_bar: bool,
    pub(crate) nav_folded: bool,
    /// The language of the words (`--language`, else Options', else the system's).
    pub(crate) language: LanguagePref,
    pub(crate) ribbon_tab: usize,
    /// FILE is open on this page.
    pub(crate) backstage: Option<BackstagePage>,
    pub(crate) notice: String,
    /// AzMeet / AzMail processes started here, until they end.
    pub(crate) launched: Vec<Child>,
    // ---- the time grid (CAL2's week) ----
    /// The height of an hour, in logical px: the zoom.
    pub(crate) hour_px: f32,
    /// The draft a click or a drag made, with its popover.
    pub(crate) draft: Option<Draft>,
    /// Drafts made so far: each draft's `serial`, so an answer for a closed one is ignored.
    pub(crate) drafts_made: u32,
    /// A press on empty time, until it is let go.
    pub(crate) press: Option<Press>,
    /// The cumulative scale of the pinch in flight at its last update (`DetectedPinch::scale`).
    pub(crate) last_pinch_scale: Option<f32>,
    /// When the popover last closed by a click outside it or Escape.
    pub(crate) popover_closed_at: Option<Instant>,
    /// A timer saves the zoom in a moment (`timegrid::queue_zoom_save`).
    pub(crate) zoom_save_queued: bool,
    // ---- meeting links ----
    /// Events whose pending link is being registered right now.
    pub(crate) syncing: BTreeSet<String>,
    /// Events whose link the meeting server refused, with what it said: not sent again until
    /// "Sync meeting links now" or a new meeting server.
    pub(crate) sync_refused: BTreeMap<String, String>,
    /// Why the last registration did not happen (empty once one did).
    pub(crate) sync_error: String,
    /// How often pending links are sent again, in milliseconds.
    pub(crate) sync_every_ms: u64,
    /// The timers that send pending links again and look for reminders run.
    pub(crate) timers_started: bool,
    // ---- FILE > Options ----
    pub(crate) server_text: String,
    pub(crate) server_error: String,
    /// The category of the Options page shown.
    pub(crate) options_category: usize,
    // ---- the editor window ----
    pub(crate) editor: Option<EditorForm>,
    /// The day of the occurrence the editor was opened on (a repeating event's).
    pub(crate) editor_occurrence: Option<NaiveDate>,
    pub(crate) editors_opened: u32,
    /// Open the editor once the window is up (`--screen editor`).
    pub(crate) editor_at_start: bool,
    /// The form as the editor window opened with it: closing the window asks "save
    /// changes?" once the form differs from it (`EditorForm::changed_since`).
    pub(crate) editor_opened: Option<EditorForm>,
    /// The editor window shows that question (its close guard is asking).
    pub(crate) editor_asking: bool,
    // ---- durable writes (`store.rs`): queued here, written on a file thread ----
    /// Writes into the calendar's data folder (events, calendars, settings, exports).
    pub(crate) data_writes: WriteQueue,
    /// Writes into the task store's folder (the To-Do bar's tasks).
    pub(crate) task_writes: WriteQueue,
    /// The batch of each queue on its way, with the lines it prints once landed.
    pub(crate) data_flight: writes::InFlight,
    pub(crate) task_flight: writes::InFlight,
    /// The settings file's text as last written (a setting replaces its line in it).
    pub(crate) settings_text: String,
    /// The main window was asked to close while writes waited: it closes once they landed.
    pub(crate) closing: bool,
    /// Writes failed when the window was to close, and the user was told: the next close
    /// passes.
    pub(crate) close_despite_failures: bool,
    /// Lines for stdout once the write of their key landed (`AZCAL_SAVED <path>`, ...).
    pub(crate) on_landing: Vec<(String, String)>,
    /// The import file being read.
    pub(crate) import_pending: Option<PathBuf>,
    // ---- FILE > Open & Export ----
    pub(crate) import_path: String,
    /// The calendar an import goes into: its index in `calendars`.
    pub(crate) import_calendar: usize,
    pub(crate) export_path: String,
    pub(crate) export_calendar: usize,
    /// What the last import or export did, and whether it failed.
    pub(crate) io_message: String,
    pub(crate) io_failed: bool,
    // ---- FILE > Calendars ----
    pub(crate) calendar_name: String,
    pub(crate) calendar_error: String,
    // ---- FILE > Print ----
    /// The print style and range the Print page shows.
    pub(crate) print: print::Settings,
    /// The preview of that printout (made on a thread, `print_ui::pump`).
    pub(crate) print_preview: print_ui::Preview,
    /// What the last Print did, and whether it failed.
    pub(crate) print_message: String,
    pub(crate) print_failed: bool,
    // ---- reminders ----
    /// Reminders shown already, by event id and day.
    pub(crate) reminded: BTreeSet<(String, NaiveDate)>,
    /// The reminder shown now.
    pub(crate) reminder: Option<(String, NaiveDate)>,
}

pub(crate) fn root_dom() -> DomId {
    DomId { inner: 0 }
}

impl CalState {
    /// The calendar `event` shows in: its own, or the default one when its calendar is gone.
    pub(crate) fn calendar_of(&self, event: &Event) -> Option<&Calendar> {
        calendars::calendar_of(&self.calendars, &event.calendar)
    }

    /// The id of the calendar `event` shows in.
    pub(crate) fn calendar_id_of(&self, event: &Event) -> String {
        self.calendar_of(event)
            .map(|c| c.id.clone())
            .unwrap_or_default()
    }

    /// `event`'s calendar is ticked in "My calendars".
    pub(crate) fn shows(&self, event: &Event) -> bool {
        !self.hidden.contains(&self.calendar_id_of(event))
    }

    /// The colour of `event`'s calendar.
    pub(crate) fn colour_of(&self, event: &Event) -> Colour {
        self.calendar_of(event).map_or(Colour::Blue, |c| c.colour)
    }

    /// The occurrences of the shown events on any day from `from` to `to`.
    pub(crate) fn occurrences(&self, from: NaiveDate, to: NaiveDate) -> Vec<Occurrence> {
        views::occurrences(&self.events, from, to, |e| self.shows(e))
    }

    /// The calendar a new event goes into: the first one shown (the default one when all are
    /// hidden).
    pub(crate) fn calendar_for_new(&self) -> String {
        self.calendars
            .iter()
            .find(|c| !self.hidden.contains(&c.id))
            .map(|c| c.id.clone())
            .unwrap_or_default()
    }

    pub(crate) fn event_index(&self, id: &str) -> Option<usize> {
        self.events.iter().position(|e| e.id == id)
    }

    /// Moves the view to `day`; the date navigator turns to its month.
    pub(crate) fn set_anchor(&mut self, day: NaiveDate) {
        self.anchor = day;
        self.nav_month = (day.year(), day.month());
        self.draft = None;
        self.press = None;
        self.announce_view();
    }

    /// Shows `view` (around the same day).
    pub(crate) fn set_view(&mut self, view: ViewKind) {
        self.view = view;
        self.backstage = None;
        self.draft = None;
        self.press = None;
        self.save_setting(&settings::line(settings::VIEW_KEY, view.name()));
        self.announce_view();
    }

    /// `AZCAL_VIEW <view> <first day> <last day>` on stdout, for scripts.
    pub(crate) fn announce_view(&self) {
        let (first, last) = views::visible_range(self.view, self.anchor);
        println!("AZCAL_VIEW {} {first} {last}", self.view.name());
    }

    /// Saves one setting line in the settings file (the others are kept): the file's text is
    /// queued for the file thread (`store.rs`).
    pub(crate) fn save_setting(&mut self, line: &str) {
        self.settings_text = settings::with_line(&self.settings_text, line);
        self.data_writes.put(
            settings::FILE_NAME.to_string(),
            self.settings_text.clone().into_bytes(),
        );
    }

    /// Puts `event` into the calendar (in place of the event with its id) and queues its file
    /// for the file thread (`store.rs`; `AZCAL_SAVED` says when it landed). Returns where the
    /// file goes.
    pub(crate) fn store_event(&mut self, event: Event) -> Result<PathBuf, String> {
        let key = event::object_key(&event.id);
        self.data_writes
            .put(key.clone(), event::to_json(&event).into_bytes());
        let path = self.data_dir.join(&key);
        self.announce_on_landing(&key, format!("AZCAL_SAVED {}", path.display()));
        if let Some(m) = &event.meeting {
            println!("AZCAL_LINK {}", m.link);
        }
        eprintln!("[azcalendar] \"{}\" goes to {key}", event.title);
        match self.event_index(&event.id) {
            Some(i) => self.events[i] = event,
            None => self.events.push(event),
        }
        Ok(path)
    }

    /// Prints `line` on stdout once the write of `key` landed (a newer write of the key
    /// replaces the waiting one, and its line goes along).
    pub(crate) fn announce_on_landing(&mut self, key: &str, line: String) {
        if !self.on_landing.iter().any(|(k, l)| k == key && *l == line) {
            self.on_landing.push((key.to_string(), line));
        }
    }

    /// The writes that did not land and wait for a retry.
    pub(crate) fn write_failures(&self) -> usize {
        self.data_writes.failures().len() + self.task_writes.failures().len()
    }

    /// Queues the removal of the event `id`'s file (what a waiting write of it would have said
    /// on landing is not said).
    pub(crate) fn remove_event_file(&mut self, id: &str) {
        let key = event::object_key(id);
        self.on_landing.retain(|(k, _)| *k != key);
        self.data_writes.delete(key);
    }

    /// Queues `calendar`'s file.
    pub(crate) fn store_calendar(&mut self, calendar: &Calendar) {
        self.data_writes.put(
            calendars::object_key(&calendar.id),
            calendars::to_json(calendar).into_bytes(),
        );
    }

    /// Queues the removal of the calendar `id`'s file.
    pub(crate) fn remove_calendar_file(&mut self, id: &str) {
        self.data_writes.delete(calendars::object_key(id));
    }

    /// Queues `task`'s file in the task store.
    pub(crate) fn store_task(&mut self, task: &Task) {
        self.task_writes
            .put(task.key(), azul_pim::task::task_to_json(task).into_bytes());
    }

    /// No write waits and none is on its way.
    pub(crate) fn writes_idle(&self) -> bool {
        self.data_writes.is_idle() && self.task_writes.is_idle()
    }

    /// The open editor's form differs from the form it opened with: closing its window asks
    /// "save changes?" first.
    pub(crate) fn editor_dirty(&self) -> bool {
        editor::close_answer(self.editor.as_ref(), self.editor_opened.as_ref())
            == editor::CloseAnswer::Ask
    }

    /// How many meeting links wait for the meeting server.
    pub(crate) fn pending_links(&self) -> usize {
        self.events
            .iter()
            .filter(|e| e.meeting.as_ref().is_some_and(|m| m.pending))
            .count()
    }
}

/// Where a new appointment goes: on a view that shows today, the next full hour after now (an
/// hour long, tomorrow at 09:00 late at night); else the view's first day (the anchor in the
/// day views) at 09:00.
pub(crate) fn new_slot(s: &CalState, now: NaiveTime) -> (NaiveDate, NaiveTime, NaiveTime) {
    let at = |h: u32| NaiveTime::from_hms_opt(h, 0, 0).unwrap_or(NaiveTime::MIN);
    let (first, last) = views::visible_range(s.view, s.anchor);
    if s.today < first || s.today > last {
        let day = match s.view {
            ViewKind::Day | ViewKind::Schedule => s.anchor,
            _ => first,
        };
        return (day, at(9), at(10));
    }
    let next = now.hour() + 1;
    if next > 22 {
        return (s.today + chrono::Duration::days(1), at(9), at(10));
    }
    (s.today, at(next), at(next + 1))
}

/// The date a date picker reports (its day held to the month's length).
pub(crate) fn picked(state: azul::widgets::DatePickerState) -> Option<NaiveDate> {
    week::picked_date(i32::try_from(state.year).ok()?, state.month, state.day)
}

/// What a text field's callback answers: the text is kept, nothing is rebuilt.
pub(crate) fn typed() -> azul::widgets::OnTextInputReturn {
    azul::widgets::OnTextInputReturn {
        update: Update::DoNothing,
        valid: azul::widgets::TextInputValid::Yes,
    }
}

/// A text field growing to its row's width, a little gap after it: the backstage pages' and the
/// editor's.
pub(crate) fn text_field(
    text: &str,
    placeholder: &str,
    name: &str,
    id: AzString,
    data: RefAny,
    cb: azul::callbacks::TextInputOnTextInputCallbackType,
) -> Dom {
    Dom::create_div()
        .with_css(
            "display: flex; flex-direction: column; flex-grow: 1; min-width: 0; margin-right: 8px;",
        )
        .with_child(
            TextInput::create()
                .with_text(text)
                // A key of the resources, or words as they are.
                .with_placeholder(azul_appkit::l10n::label(placeholder))
                .with_accessibility_name(azul_appkit::l10n::label(name))
                .with_on_text_input(data, cb)
                .dom()
                .with_id(id),
        )
}

/// A drop-down of `labels` with `selected` picked, a little gap after it: the backstage pages'
/// and the editor's.
pub(crate) fn drop_down(
    labels: Vec<String>,
    selected: usize,
    name: &str,
    id: AzString,
    data: RefAny,
    cb: azul::callbacks::DropDownOnChoiceChangeCallbackType,
) -> Dom {
    // Keys of the resources, or words as they are (a calendar's own name).
    azul::widgets::DropDown::create(azul::vec::StringVec::from(
        labels
            .iter()
            .map(|l| azul_appkit::l10n::label(l))
            .collect::<Vec<AzString>>(),
    ))
    .with_selected(selected)
    .with_accessibility_name(azul_appkit::l10n::label(name))
    .with_on_choice_change(data, cb)
    .dom()
    .with_id(id)
    .with_css("margin-right: 8px;")
}

/// `day` in `style`, as the window's language writes a date (appkit's `date_text`).
pub(crate) fn day_text(style: azul_appkit::l10n::DateStyle, day: NaiveDate) -> String {
    azul_appkit::l10n::date_text(
        style,
        day.year(),
        day.month(),
        day.day(),
        day.weekday().num_days_from_monday(),
    )
}

/// Three 64-bit draws of `azul_storage::ids::random_seed` for a room id (130 of the bits are
/// used).
pub(crate) fn room_entropy() -> [u64; 3] {
    use azul_storage::ids::random_seed;
    [random_seed(), random_seed(), random_seed()]
}

/// A new meeting link for an event: made here with an invite secret of its own (AzMeet's
/// `invite.rs`), pending until the meeting server has its room.
pub(crate) fn new_meeting(server: &str) -> Meeting {
    meeting::pending_encrypted_meeting(
        server,
        &meeting::new_room_id(room_entropy()),
        &meet_invite::secret_from(room_entropy()),
    )
}

// ==== The window ====

extern "C" fn layout(mut data: RefAny, info: LayoutCallbackInfo) -> Dom {
    // Reading the mode makes a light / dark switch rebuild the window; the layout's language
    // says the words (a switch of it too).
    let _mode = info.get_mode();
    azul_appkit::l10n::begin_layout(&info);
    let app = data.clone();
    let Some(guard) = data.downcast_ref::<CalState>() else {
        return Dom::create_body();
    };
    let s = &*guard;
    let root = Dom::create_div()
        .with_id(ids::ROOT)
        .with_css("display: flex; flex-direction: column; flex-grow: 1; min-height: 0;")
        .with_callback(
            EventFilter::Component(ComponentEventFilter::AfterMount),
            app.clone(),
            on_app_mounted,
        )
        .with_child(chrome::office_shell(s, &app, info.get_window_height()));
    Dom::create_body()
        .with_css(BODY)
        .with_menu_bar(menu_bar(&app))
        .with_callback(
            EventFilter::Window(WindowEventFilter::VirtualKeyDown),
            app.clone(),
            on_window_key,
        )
        .with_callback(
            EventFilter::Window(WindowEventFilter::CloseRequested),
            app.clone(),
            on_main_close_requested,
        )
        .with_child(
            ShellThemeScope::create(root)
                .with_accent(ShellThemeAccent::Blue)
                .dom(),
        )
}

/// The main window is asked to close (its close button, Cmd+Q's close, the app's own): it
/// waits for the writes on their way (`writes.rs` closes it once they landed), and says once
/// what did not land.
extern "C" fn on_main_close_requested(mut data: RefAny, mut info: CallbackInfo) -> Update {
    let app = data.clone();
    let Some(mut guard) = data.downcast_mut::<CalState>() else {
        return Update::DoNothing;
    };
    let s = &mut *guard;
    let failures = s.write_failures();
    match store::main_close(!s.writes_idle(), failures, s.close_despite_failures) {
        store::MainClose::Close => Update::DoNothing,
        store::MainClose::Wait => {
            eprintln!("[azcalendar] the window closes once the waiting writes landed");
            s.closing = true;
            info.prevent_window_close();
            writes::pump(s, &mut info, &app);
            Update::DoNothing
        }
        store::MainClose::Tell => {
            s.close_despite_failures = true;
            s.notice = format!(
                "{failures} change(s) could not be written. Close the window again to quit \
                 without them."
            );
            info.prevent_window_close();
            Update::RefreshDom
        }
    }
}

/// The menu bar (the native one on macOS): Calendar, View, Settings.
fn menu_bar(data: &RefAny) -> Menu {
    let item = |label: &str, callback: CallbackType| {
        MenuItem::string(StringMenuItem::create(label).with_callback(data.clone(), callback))
    };
    let menu = |label: &str, items: Vec<MenuItem>| {
        MenuItem::string(StringMenuItem::create(label).with_children(items))
    };
    Menu::create(vec![
        menu(
            "Calendar",
            vec![
                item("New Appointment", editor_ui::on_new_appointment),
                item("New Meeting", editor_ui::on_new_meeting),
                item("Open & Export\u{2026}", chrome::on_open_page),
                item("Print\u{2026}", chrome::on_print_page),
                item("Calendars\u{2026}", chrome::on_calendars_page),
            ],
        ),
        menu(
            "View",
            vec![
                item("Day", chrome::on_view_day),
                item("Work Week", chrome::on_view_work_week),
                item("Week", chrome::on_view_week),
                item("Month", chrome::on_view_month),
                item("Schedule View", chrome::on_view_schedule),
                item("List", chrome::on_view_agenda),
                item("Go To Today", chrome::on_today),
            ],
        ),
        menu(
            "Settings",
            vec![
                item("Meeting server\u{2026}", chrome::on_options_page),
                item("Sync meeting links now", on_sync_now),
            ],
        ),
    ])
}

/// The window's keys, Outlook's: Ctrl (Cmd) + Alt + 1 .. 6 the views, Ctrl + N a new
/// appointment, Ctrl + Shift + Q a new meeting, Ctrl + T today, Ctrl + P Print, Alt + Left /
/// Right the previous / next days. Keys without Ctrl, Cmd or Alt are the focused control's.
extern "C" fn on_window_key(mut data: RefAny, mut info: CallbackInfo) -> Update {
    let key = info
        .get_current_keyboard_state()
        .current_virtual_keycode
        .into_option();
    let m = info.get_key_modifiers();
    let command = m.primary_down();
    let Some(key) = key else {
        return Update::DoNothing;
    };
    if command && m.alt {
        let view = match key {
            VirtualKeyCode::Key1 => ViewKind::Day,
            VirtualKeyCode::Key2 => ViewKind::WorkWeek,
            VirtualKeyCode::Key3 => ViewKind::Week,
            VirtualKeyCode::Key4 => ViewKind::Month,
            VirtualKeyCode::Key5 => ViewKind::Schedule,
            VirtualKeyCode::Key6 => ViewKind::Agenda,
            _ => return Update::DoNothing,
        };
        let Some(mut s) = data.downcast_mut::<CalState>() else {
            return Update::DoNothing;
        };
        s.set_view(view);
        info.prevent_default();
        return Update::RefreshDom;
    }
    if command {
        return match key {
            VirtualKeyCode::N => {
                info.prevent_default();
                editor_ui::open_new(&mut data, &mut info, false)
            }
            VirtualKeyCode::Q if m.shift => {
                info.prevent_default();
                editor_ui::open_new(&mut data, &mut info, true)
            }
            VirtualKeyCode::T => {
                let Some(mut s) = data.downcast_mut::<CalState>() else {
                    return Update::DoNothing;
                };
                let today = s.today;
                s.set_anchor(today);
                info.prevent_default();
                Update::RefreshDom
            }
            VirtualKeyCode::P => {
                let Some(mut s) = data.downcast_mut::<CalState>() else {
                    return Update::DoNothing;
                };
                chrome::open_backstage(&mut s, BackstagePage::Print);
                info.prevent_default();
                Update::RefreshDom
            }
            _ => Update::DoNothing,
        };
    }
    if m.alt && matches!(key, VirtualKeyCode::Left | VirtualKeyCode::Right) {
        let Some(mut s) = data.downcast_mut::<CalState>() else {
            return Update::DoNothing;
        };
        let by = if key == VirtualKeyCode::Left { -1 } else { 1 };
        let day = views::step(s.view, s.anchor, by);
        s.set_anchor(day);
        info.prevent_default();
        return Update::RefreshDom;
    }
    Update::DoNothing
}

/// The window is up: pending meeting links start being sent (now and every
/// `AZCAL_SYNC_SECONDS`), reminders are looked for, and `--screen editor` opens the editor.
extern "C" fn on_app_mounted(mut data: RefAny, mut info: CallbackInfo) -> Update {
    // The language of the words: Options' or `--language` (the system's needs nothing).
    let language = data.downcast_ref::<CalState>().map(|s| s.language);
    if let Some(language) = language.filter(|l| *l != LanguagePref::System) {
        info.set_locale(language.tag());
    }
    start_syncing(&mut data, &mut info);
    let open_editor = data.downcast_mut::<CalState>().is_some_and(|mut s| {
        let open = s.editor_at_start;
        s.editor_at_start = false;
        open
    });
    if open_editor {
        return editor_ui::open_new(&mut data, &mut info, false);
    }
    Update::DoNothing
}

// ==== Reminders ====

/// Looks for due reminders: the first one is shown (an info bar over the view), every due one
/// is marked shown.
extern "C" fn on_reminder_tick(mut data: RefAny, _info: TimerCallbackInfo) -> TimerCallbackReturn {
    let Some(mut guard) = data.downcast_mut::<CalState>() else {
        return TimerCallbackReturn::continue_unchanged();
    };
    let s = &mut *guard;
    let now = chrono::Local::now().naive_local();
    s.today = now.date();
    let due = views::due_reminders(&s.events, now, &s.reminded, |e| s.shows(e));
    let Some(&(first, day)) = due.first() else {
        return TimerCallbackReturn::continue_unchanged();
    };
    for (index, day) in &due {
        s.reminded.insert((s.events[*index].id.clone(), *day));
    }
    let event = &s.events[first];
    println!("AZCAL_REMINDER {}", event.title);
    s.reminder = Some((event.id.clone(), day));
    TimerCallbackReturn::continue_and_refresh_dom()
}

/// What the reminder bar says: "Team sync starts at 09:00 (Room 4)".
pub(crate) fn reminder_text(s: &CalState) -> Option<String> {
    let (id, day) = s.reminder.as_ref()?;
    let e = s.events.iter().find(|e| &e.id == id)?;
    let when = if e.all_day {
        format!("is on {}", day.format("%A %-d %B"))
    } else if *day == s.today {
        format!("starts at {}", e.start.format("%H:%M"))
    } else {
        format!(
            "starts {} at {}",
            day.format("%A %-d %B"),
            e.start.format("%H:%M")
        )
    };
    let place = if e.location.is_empty() {
        String::new()
    } else {
        format!(" ({})", e.location)
    };
    Some(format!("Reminder: {} {when}{place}.", e.title))
}

pub(crate) extern "C" fn on_dismiss_reminder(mut data: RefAny, _info: CallbackInfo) -> Update {
    let Some(mut s) = data.downcast_mut::<CalState>() else {
        return Update::DoNothing;
    };
    s.reminder = None;
    Update::RefreshDom
}

// ==== Registering links: POST /rooms on an azul Thread, the answer on the UI thread ====

/// One pending link to register.
struct SyncJob {
    server: String,
    event_id: String,
    room_id: String,
    /// The `POST /rooms` body (`meeting::register_body`).
    body: String,
}

struct SyncInit {
    job: SyncJob,
    app: RefAny,
}

/// What the answer resumes with.
struct SyncReply {
    app: RefAny,
    event_id: String,
    room_id: String,
    server: String,
}

/// Sends every pending link that is not on its way already and was not refused: one thread
/// each. The event's times go along in UTC, so the server keeps the room for the meeting.
pub(crate) fn sync_links(s: &mut CalState, info: &mut CallbackInfo, app: &RefAny) {
    let jobs: Vec<SyncJob> = s
        .events
        .iter()
        .filter(|e| !s.syncing.contains(&e.id) && !s.sync_refused.contains_key(&e.id))
        .filter_map(|e| {
            let m = e.meeting.as_ref().filter(|m| m.pending)?;
            let room_id = meeting::room_id_of(m)?;
            let (starts, ends) = meeting::utc_window(&chrono::Local, e.date, e.start, e.end);
            Some(SyncJob {
                server: m.server.clone(),
                event_id: e.id.clone(),
                body: meeting::register_body_for(m, &room_id, starts, ends),
                room_id,
            })
        })
        .collect();
    for job in jobs {
        eprintln!(
            "[azcalendar] registering azlin://meet/{} with {}: {}",
            job.room_id, job.server, job.body
        );
        s.syncing.insert(job.event_id.clone());
        let init = RefAny::new(SyncInit {
            job,
            app: app.clone(),
        });
        info.add_thread(
            ThreadId::unique(),
            Thread::create(init, RefAny::new(()), sync_thread),
        );
    }
}

/// Runs `POST /rooms` on a worker thread: `http_request` blocks here, then queues its answer,
/// which the UI thread hands to `on_registered`.
extern "C" fn sync_thread(mut init: RefAny, _sender: ThreadSender, _receiver: ThreadReceiver) {
    let Some((url, body, reply)) = init.downcast_ref::<SyncInit>().map(|i| {
        (
            format!("{}/rooms", i.job.server),
            i.job.body.clone(),
            SyncReply {
                app: i.app.clone(),
                event_id: i.job.event_id.clone(),
                room_id: i.job.room_id.clone(),
                server: i.job.server.clone(),
            },
        )
    }) else {
        return;
    };
    let _request = HttpRequestConfig::create()
        .with_timeout(HTTP_TIMEOUT_SECS)
        .with_user_agent("AzCalendar/0.1")
        .with_header("accept", "application/json")
        .http_request(
            HttpMethod::Post,
            url.as_str(),
            U8Vec::from(body.into_bytes()),
            "application/json",
            RefAny::new(reply),
            on_registered,
        );
}

fn http_error_text(e: &HttpError) -> String {
    match e {
        HttpError::Timeout => String::from("timed out"),
        HttpError::InvalidUrl(s)
        | HttpError::ConnectionFailed(s)
        | HttpError::TlsError(s)
        | HttpError::IoError(s)
        | HttpError::Other(s) => s.as_str().to_string(),
        other => format!("{other:?}"),
    }
}

/// The registered meeting in the server's answer, or the answer's status (`None`: the server was
/// not reached) and what to tell the user.
fn register_outcome(
    server: &str,
    room_id: &str,
    result: RefAny,
) -> Result<Meeting, (Option<u16>, String)> {
    let Some(answer) = HttpGetResult::downcast(result).into_option() else {
        return Err((None, meeting::mint_failure(server, None, "no answer")));
    };
    match answer.result.into_result() {
        Ok(response) => {
            let body = response
                .body_as_string()
                .into_option()
                .map(|body| body.as_str().to_string())
                .unwrap_or_default();
            match response.status_code {
                status @ (200 | 201) => meeting::registered_meeting(server, room_id, &body)
                    .map_err(|message| (Some(status), message)),
                status => Err((
                    Some(status),
                    meeting::mint_failure(server, Some(status), &body),
                )),
            }
        }
        Err(HttpError::HttpStatus(e)) => Err((
            Some(e.status_code),
            meeting::mint_failure(server, Some(e.status_code), ""),
        )),
        Err(e) => Err((
            None,
            meeting::mint_failure(server, None, &http_error_text(&e)),
        )),
    }
}

/// The answer to a registration: the event's meeting becomes the registered one (its file is
/// rewritten); a failure leaves it pending, to be sent again unless the server refused it.
extern "C" fn on_registered(mut data: RefAny, _info: CallbackInfo, result: RefAny) -> Update {
    let Some((mut app, event_id, room_id, server)) = data.downcast_ref::<SyncReply>().map(|r| {
        (
            r.app.clone(),
            r.event_id.clone(),
            r.room_id.clone(),
            r.server.clone(),
        )
    }) else {
        return Update::DoNothing;
    };
    let outcome = register_outcome(&server, &room_id, result);
    let Some(mut guard) = app.downcast_mut::<CalState>() else {
        return Update::DoNothing;
    };
    let s = &mut *guard;
    s.syncing.remove(&event_id);
    match outcome {
        Ok(registered) => {
            let Some(event) = s.events.iter_mut().find(|e| e.id == event_id) else {
                return Update::DoNothing;
            };
            // Only the pending link this answer is for (a new meeting server may have taken it
            // over meanwhile); it keeps its invite secret, which the answer does not name.
            let pending = event
                .meeting
                .as_ref()
                .filter(|m| {
                    m.pending
                        && m.server == server
                        && meeting::room_id_of(m).as_deref() == Some(room_id.as_str())
                })
                .map(|m| m.link.clone());
            let Some(pending) = pending else {
                return Update::DoNothing;
            };
            let registered = meeting::with_pending_link(registered, &pending);
            let link = registered.link.clone();
            event.meeting = Some(registered);
            // The file says so once it is rewritten (`AZCAL_SYNCED` then). Should the write not
            // land, the file still says pending: the next start sends it again, and
            // registering is idempotent.
            let event = event.clone();
            let key = event::object_key(&event.id);
            eprintln!("[azcalendar] {server} registered {link}; {key} is rewritten");
            let _ = s.store_event(event);
            s.announce_on_landing(&key, format!("AZCAL_SYNCED {link}"));
            s.sync_error.clear();
        }
        Err((status, message)) => {
            eprintln!("[azcalendar] azlin://meet/{room_id} not registered: {message}");
            // A link moved to another meeting server meanwhile is not this server's to refuse.
            let still_here = s.events.iter().any(|e| {
                e.id == event_id
                    && e.meeting
                        .as_ref()
                        .is_some_and(|m| m.pending && m.server == server)
            });
            if !still_here {
                return Update::DoNothing;
            }
            if !meeting::sync_again(status) {
                s.sync_refused.insert(event_id, message.clone());
            }
            // The week looks the same (the link still waits): redraw only for the Settings
            // sheet's status line, not every retry while the user types elsewhere.
            let shown = s.backstage == Some(BackstagePage::Options) && s.sync_error != message;
            s.sync_error = message;
            if !shown {
                return Update::DoNothing;
            }
        }
    }
    Update::RefreshDom
}

/// The timer that sends pending links again.
extern "C" fn on_sync_tick(mut data: RefAny, mut info: TimerCallbackInfo) -> TimerCallbackReturn {
    let app = data.clone();
    if let Some(mut s) = data.downcast_mut::<CalState>() {
        sync_links(&mut s, &mut info.callback_info, &app);
        // Writes that did not land go again, at the links' pace (not every write tick).
        s.data_writes.retry();
        s.task_writes.retry();
    }
    TimerCallbackReturn::continue_unchanged()
}

/// Starts the timers that send pending links again and look for reminders (once), and sends
/// the pending links now.
fn start_syncing(data: &mut RefAny, info: &mut CallbackInfo) {
    let app = data.clone();
    let Some(mut guard) = data.downcast_mut::<CalState>() else {
        return;
    };
    let s = &mut *guard;
    if !s.timers_started {
        s.timers_started = true;
        let get_time = info.get_system_time_fn();
        info.add_timer(
            TimerId::unique(),
            Timer::create(app.clone(), on_sync_tick, get_time).with_interval(Duration::System(
                SystemTimeDiff::from_millis(s.sync_every_ms),
            )),
        );
        let get_time = info.get_system_time_fn();
        info.add_timer(
            TimerId::unique(),
            Timer::create(app.clone(), on_reminder_tick, get_time).with_interval(Duration::System(
                SystemTimeDiff::from_millis(REMINDER_TICK_MS),
            )),
        );
        // Every durable write: the main window starts the file threads (`writes.rs`).
        let get_time = info.get_system_time_fn();
        info.add_timer(
            TimerId::unique(),
            Timer::create(app.clone(), writes::on_write_tick, get_time).with_interval(
                Duration::System(SystemTimeDiff::from_millis(writes::WRITE_TICK_MS)),
            ),
        );
    }
    sync_links(s, info, &app);
    writes::pump(s, info, &app);
}

/// "Sync meeting links now": sends every pending link again, refused ones too.
pub(crate) extern "C" fn on_sync_now(mut data: RefAny, mut info: CallbackInfo) -> Update {
    let app = data.clone();
    let Some(mut guard) = data.downcast_mut::<CalState>() else {
        return Update::DoNothing;
    };
    let s = &mut *guard;
    s.sync_refused.clear();
    sync_links(s, &mut info, &app);
    Update::RefreshDom
}

// ==== Join meeting ====

/// An event, for a callback on its block.
pub(crate) struct EventRef {
    pub(crate) app: RefAny,
    pub(crate) id: String,
}

/// Starts AzMeet to join `meeting`. The debug-server port is this app's own, so AzMeet does not
/// inherit it.
fn launch_azmeet(program: &Path, meet: &Meeting) -> std::io::Result<Child> {
    if !program.is_file() {
        return Err(std::io::Error::new(
            std::io::ErrorKind::NotFound,
            format!("{} does not exist", program.display()),
        ));
    }
    Command::new(program)
        .args(meeting::join_args(meet))
        .envs(meeting::join_env(meet))
        .env_remove("AZ_DEBUG")
        .stdin(Stdio::null())
        .spawn()
}

pub(crate) extern "C" fn on_join_meeting(mut data: RefAny, mut info: CallbackInfo) -> Update {
    let Some((mut app, id)) = data
        .downcast_ref::<EventRef>()
        .map(|r| (r.app.clone(), r.id.clone()))
    else {
        return Update::DoNothing;
    };
    let Some(mut guard) = app.downcast_mut::<CalState>() else {
        return Update::DoNothing;
    };
    let s = &mut *guard;
    let Some((title, meet)) = s
        .events
        .iter()
        .find(|e| e.id == id)
        .and_then(|e| Some((e.title.clone(), e.meeting.clone()?)))
    else {
        return Update::DoNothing;
    };
    // Forget the AzMeet windows that were closed.
    s.launched
        .retain_mut(|child| matches!(child.try_wait(), Ok(None)));
    let program = meeting::azmeet_program(
        std::env::var(meeting::AZMEET_BIN_VAR).ok().as_deref(),
        std::env::current_exe().ok().as_deref(),
    );
    let launched = match program {
        Some(program) => launch_azmeet(&program, &meet),
        None => Err(std::io::Error::new(
            std::io::ErrorKind::NotFound,
            "the AzCalendar program's folder is unknown",
        )),
    };
    match launched {
        Ok(child) => {
            println!("AZCAL_JOIN_PID {}", child.id());
            eprintln!(
                "[azcalendar] AzMeet joins {} (pid {})",
                meet.link,
                child.id()
            );
            s.notice = format!("Opening AzMeet for \"{title}\"...");
            s.launched.push(child);
        }
        Err(e) => {
            info.set_clipboard_content(ClipboardContent {
                plain_text: AzString::from(meet.link.as_str()),
                styled_runs: StyledTextRunVec::create(),
                html: azul::option::OptionString::None,
            });
            eprintln!("[azcalendar] AzMeet not started ({e}); link copied");
            s.notice = format!(
                "AzMeet could not be started ({e}), so the meeting link was copied: {}",
                meet.link
            );
        }
    }
    Update::RefreshDom
}

// ==== Start ====

fn user_data_dir() -> Option<PathBuf> {
    FilePath::get_data_dir()
        .into_option()
        .map(|dir| PathBuf::from(dir.inner.as_str()))
}

/// The meeting server the shared Azlin config names, as AzMeet resolves it (azul-appkit's
/// `azlin_config`): `endpoints.meet` of `~/.azlin/config.json` (or of the file `AZLIN_CONFIG`
/// names), else - when none is built in (`meeting::BUILT_IN_WORKER`) - its profile's address
/// (`local`, the default: the local stack's; `AZLIN_PROFILE` picks another). `AZMEET_WORKER` is
/// AzCalendar's own layer above this one (`meeting::server_setting`).
fn shared_meeting_server() -> Option<String> {
    use azul_appkit::azlin_config::{self, AzlinConfig, Endpoint, EndpointFlags, Source};
    let home = FilePath::get_home_dir()
        .into_option()
        .map(|dir| PathBuf::from(dir.inner.as_str()));
    let path = azlin_config::config_path(
        std::env::var(azlin_config::CONFIG_VAR).ok().as_deref(),
        home.as_deref(),
    );
    let loaded = path.map(|path| {
        let (config, _problem) = AzlinConfig::load(&path);
        (path, config)
    });
    let file = loaded
        .as_ref()
        .map(|(path, config)| (path.as_path(), &config.endpoints));
    // Of the environment only the profile: the meeting server's variable is weighed above.
    let env = |var: &str| {
        if var == azlin_config::PROFILE_VAR {
            std::env::var(var).ok()
        } else {
            None
        }
    };
    let resolved = azlin_config::resolve_endpoints(file, &env, &EndpointFlags::default());
    let meet = resolved.get(Endpoint::Meet);
    match meet.source {
        Source::File(_) => meet.value.clone(),
        Source::Profile(_) | Source::BuiltIn if meeting::BUILT_IN_WORKER.is_empty() => {
            meet.value.clone()
        }
        _ => None,
    }
}

pub fn start() {
    let args = match Args::parse(std::env::args().skip(1)) {
        Ok(args) => args,
        Err(text) if text == args::HELP => {
            println!("{text}");
            return;
        }
        Err(text) => {
            eprintln!("{text}");
            std::process::exit(2);
        }
    };
    let data_dir = match &args.data {
        Some(dir) => dir.clone(),
        None => event::data_dir(std::env::var(DATA_VAR).ok().as_deref(), user_data_dir()),
    };
    // The tasks are the store every Azlin app shares: with the calendar's files when a data
    // folder was named, else in AzTasks' root (DEDUP_EDITORS B12).
    let named_dir = args.data.clone().or_else(|| {
        std::env::var(DATA_VAR)
            .ok()
            .filter(|v| !v.trim().is_empty())
            .map(|v| PathBuf::from(v.trim()))
    });
    let tasks_root = tasks::tasks_root(
        named_dir.as_deref(),
        std::env::var(tasks::TASKS_DATA_VAR).ok().as_deref(),
        std::env::var(azul_appkit::data::DATA_VAR).ok().as_deref(),
        user_data_dir(),
    );
    let today = chrono::Local::now().date_naive();
    // The data folder's drive: what the start reads and `--sample` writes through, the same
    // drive the file threads write the changes through (`writes.rs`) - a `LocalDrive` on the
    // data folder today, the user's bucket later.
    let drive = azul_storage::LocalDrive::new(&data_dir);
    if args.sample {
        match sample::write(&drive, today) {
            Ok(0) => eprintln!("[azcalendar] --sample: the calendar has events, nothing added"),
            Ok(n) => eprintln!("[azcalendar] --sample: {n} sample events written"),
            Err(e) => eprintln!("[azcalendar] --sample: {e}"),
        }
    }
    let (events, skipped) = event::load(&drive);
    for file in &skipped {
        eprintln!("[azcalendar] left out {}: {}", file.key, file.reason);
    }
    let calendars = calendars::load(&drive);
    let now = chrono::Local::now().naive_local();
    match tasks::migrate_old_folder(&data_dir, &tasks_root, now) {
        Ok(0) => {}
        Ok(n) => eprintln!(
            "[azcalendar] {n} task(s) of the old To-Do bar moved into {}",
            tasks_root.display()
        ),
        Err(e) => eprintln!("[azcalendar] the old To-Do bar's tasks could not be moved: {e}"),
    }
    let todo = tasks::load(&tasks_root);
    let saved = settings::read(&drive);
    let text = saved.as_deref().unwrap_or_default();
    let server = meeting::server_setting(
        args.worker.as_deref(),
        saved.as_deref(),
        std::env::var(meeting::WORKER_VAR).ok().as_deref(),
        shared_meeting_server().as_deref(),
        meeting::BUILT_IN_WORKER,
    );
    let hour_px = settings::hour_px(text).unwrap_or(week::DEFAULT_HOUR_PX);
    let hidden = settings::value(text, settings::HIDDEN_CALENDARS_KEY)
        .map(calendars::hidden_of)
        .unwrap_or_default();
    let mut view = settings::value(text, settings::VIEW_KEY)
        .and_then(ViewKind::from_name)
        .unwrap_or_else(views::default_view);
    let todo_bar = settings::flag(text, settings::TODO_BAR_KEY).unwrap_or(true);
    let nav_folded = settings::flag(text, settings::NAVIGATION_FOLDED_KEY).unwrap_or(false);
    let language = args.language.unwrap_or_else(|| settings::language(text));
    let mut backstage = None;
    let mut editor_at_start = false;
    match args.screen {
        Some(Screen::View(v)) => view = v,
        Some(Screen::Backstage(page)) => backstage = Some(page),
        Some(Screen::Editor) => editor_at_start = true,
        None => {}
    }
    let anchor = args.date.unwrap_or(today);
    let sync_seconds = std::env::var(SYNC_VAR)
        .ok()
        .and_then(|v| v.trim().parse::<u64>().ok())
        .filter(|&n| n > 0)
        .unwrap_or(SYNC_SECONDS);
    eprintln!(
        "[azcalendar] {} event(s), {} calendar(s) in {}; meeting server: {server}; {hour_px} px \
         an hour",
        events.len(),
        calendars.len(),
        data_dir.display(),
    );
    let state = CalState {
        data_dir,
        server_text: server.clone(),
        server,
        events,
        calendars,
        hidden,
        tasks: todo.tasks,
        tasks_root,
        task_list: todo.new_task_list,
        task_text: String::new(),
        today,
        view,
        anchor,
        nav_month: (anchor.year(), anchor.month()),
        selected: None,
        todo_bar,
        nav_folded,
        language,
        ribbon_tab: 0,
        backstage,
        notice: String::new(),
        launched: Vec::new(),
        hour_px,
        draft: None,
        drafts_made: 0,
        press: None,
        last_pinch_scale: None,
        popover_closed_at: None,
        zoom_save_queued: false,
        syncing: BTreeSet::new(),
        sync_refused: BTreeMap::new(),
        sync_error: String::new(),
        sync_every_ms: sync_seconds.saturating_mul(1000),
        timers_started: false,
        server_error: String::new(),
        options_category: 0,
        editor: None,
        editor_occurrence: None,
        editors_opened: 0,
        editor_at_start,
        editor_opened: None,
        editor_asking: false,
        data_writes: WriteQueue::new(),
        task_writes: WriteQueue::new(),
        data_flight: writes::InFlight::default(),
        task_flight: writes::InFlight::default(),
        settings_text: text.to_string(),
        closing: false,
        close_despite_failures: false,
        on_landing: Vec::new(),
        import_pending: None,
        import_path: String::new(),
        import_calendar: 0,
        export_path: String::new(),
        export_calendar: 0,
        io_message: String::new(),
        io_failed: false,
        calendar_name: String::new(),
        calendar_error: String::new(),
        print: print::Settings::for_view(view, anchor),
        print_preview: print_ui::Preview::default(),
        print_message: String::new(),
        print_failed: false,
        reminded: BTreeSet::new(),
        reminder: None,
    };
    state.announce_view();
    let mut config = AppConfig::create();
    if let Some(theme) = &args.theme {
        config = config.with_theme(theme.as_str());
    }
    if let Some(mode) = args.mode {
        config = config.with_mode(OptionDarkLightMode::Some(match mode {
            args::Mode::Light => DarkLightMode::Light,
            args::Mode::Dark => DarkLightMode::Dark,
        }));
    }
    // The kit's icons: Haiku's under flora, Material under flat.
    azul_appkit::ui::add_kit_icons(&mut config);
    // AzCalendar's words and appkit's, the engine's locale deciding.
    l10n::register(&mut config);
    let app = App::create(RefAny::new(state), config);
    let mut window = WindowCreateOptions::create(layout);
    window.window_state.size.dimensions = LogicalSize::create(1280.0, 900.0);
    window.window_state.title = AzString::from("AzCalendar");
    window.window_state.window_id = AzString::from(MAIN_WINDOW_ID);
    window.window_state.flags.decorations = WindowDecorations::NoTitle;
    app.run(window);
}

#[cfg(test)]
mod mode_tests {
    use super::*;

    /// Every `#rgb` / `#rgba` / `#rrggbb` / `#rrggbbaa` colour written into `css`.
    fn fixed_colours(css: &str) -> Vec<String> {
        let bytes = css.as_bytes();
        let mut found = Vec::new();
        let mut i = 0;
        while i < bytes.len() {
            if bytes[i] != b'#' {
                i += 1;
                continue;
            }
            let digits = bytes[i + 1..]
                .iter()
                .take_while(|c| c.is_ascii_hexdigit())
                .count();
            if matches!(digits, 3 | 4 | 6 | 8) {
                found.push(css[i..=i + digits].to_string());
            }
            i += 1 + digits;
        }
        found
    }

    /// The page's chrome and text are the desktop's palette of the mode the window is in, like
    /// the widgets on it: a fixed light surface put the dark-mode Buttons (a translucent white
    /// face, white ink) on white, where they vanished.
    #[test]
    fn the_page_chrome_and_its_text_are_system_colours_in_light_and_dark() {
        for (name, css) in [
            ("BODY", BODY),
            ("LINE", LINE),
            ("LABEL", LABEL),
            ("POPOVER", POPOVER),
            ("PAGE", PAGE),
        ] {
            let fixed = fixed_colours(css);
            assert!(
                fixed.is_empty(),
                "{name} paints {fixed:?} in both modes; it must follow the window's mode \
                 (a `system:` colour): {css}"
            );
        }
    }

    /// A colour of the app's own (an accent tint, the notice, the error red) is chosen for a
    /// light page; each carries the twin it takes on a dark one.
    #[test]
    fn every_colour_of_the_apps_own_has_a_dark_twin() {
        for (name, css) in [
            ("NOTICE", NOTICE),
            ("TODAY_PAINT", TODAY_PAINT),
            ("OTHER_MONTH_PAINT", OTHER_MONTH_PAINT),
            ("SELECTED_RING", SELECTED_RING),
            ("DRAFT_PAINT", DRAFT_PAINT),
            ("DRAFT_TITLE", DRAFT_TITLE),
            ("ERROR", ERROR),
            ("PREVIEW_SURFACE", print_ui::PREVIEW_SURFACE),
        ] {
            assert!(
                !fixed_colours(css).is_empty(),
                "{name} names a colour of its own: {css}"
            );
            assert!(
                css.contains("@media (prefers-color-scheme: dark) {"),
                "{name} has no dark twin: {css}"
            );
        }
        assert!(
            fixed_colours(DAY_PAINT).is_empty(),
            "a day is the desktop's surface"
        );
        assert!(
            fixed_colours(SECONDARY).is_empty(),
            "secondary ink is the desktop's"
        );
    }
}
