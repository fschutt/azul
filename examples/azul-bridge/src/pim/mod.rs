//! CalDAV (RFC 4791) and CardDAV (RFC 6352) over AzCalendar's and AzContacts' files, for Apple
//! Calendar and Contacts, Thunderbird, DAVx5 and Evolution. They have a port of their own
//! (`pim_port`, 1181) behind the same doors as WebDAV: [`crate::dav::Dav::for_pim`] serves it
//! (the request must name this computer, must not come from a browser page, signs in with HTTP
//! Digest or Basic, and is limited the same way).
//!
//! ```text
//! /.well-known/caldav, /.well-known/carddav   301 to / (RFC 6764)
//! /                                           the current user's principal
//! /principal/                                 the one user: calendar-home-set, addressbook-home-set
//! /calendars/                                 one collection per AzCalendar calendar
//! /calendars/<default | calendar id>/<name>.ics   an event
//! /addressbooks/contacts/<name>.vcf           a contact
//! ```
//!
//! - Contacts ([`cards`]) are AzContacts' files, `contacts/<uid>.vcf` of the drive, served and
//!   written byte for byte (a program's vCard 3.0 stays 3.0; AzContacts reads 3.0 and 4.0).
//! - Events ([`events`]) are AzCalendar's JSON files (`events/<id>.json` and the calendars,
//!   `calendars/<id>.json`, of AzCalendar's data folder: `calendar/` of the drive, or the folder
//!   `serve --calendar-folder` names), read and written with AzCalendar's own code
//!   (azul-calendar-core): an event is served as the iCalendar AzCalendar's export writes and a
//!   program's iCalendar is read with AzCalendar's import, so a PUT's answer carries no ETag (the
//!   program fetches what the file became).
//! - A program names what it makes (`<UUID>.ics`); a name that cannot be the file's own (an
//!   upper-case UUID is no event id, an `@` is in no contact file name) gets a new id and is
//!   kept in the state folder's `pim-names.json` ([`Names`]), so the program finds its resource
//!   under the name it gave.
//! - ETags are the drive's version of the file (S3's entity tag; the time and size of a file on
//!   disk); a collection's CTag (`getctag`) is a hash of its members' names and versions, so a
//!   program polls one property to see that something changed.
//! - Methods: OPTIONS, PROPFIND (Depth 0 and 1), REPORT (calendar-multiget, calendar-query with
//!   its component and time range, addressbook-multiget, addressbook-query - every card: its
//!   filters are not applied), GET / HEAD, PUT (`If-Match`, `If-None-Match: *`), DELETE
//!   (`If-Match`), MKCALENDAR (a new AzCalendar calendar: its name, the nearest AzCalendar
//!   colour, under the program's path). PROPPATCH sets a calendar's name and colour; any other
//!   property is dead and refused (403, the rest of the request 424). MKCOL is refused.
//! - sync-collection (RFC 6578) on a calendar and the address book: what changed and what went
//!   since a token; the tokens of the last 32 states of each collection are kept in
//!   memory (one from before a start is refused and the program lists again). Not yet:
//!   scheduling (iTIP), tasks (VTODO).

use std::{
    collections::{BTreeMap, HashMap, HashSet},
    path::PathBuf,
    sync::{Arc, Mutex},
};

use azcal_core::{calendars::Calendar, event::Event};
use azul_storage::{Drive, DriveError, ObjectInfo};
use serde::{Deserialize, Serialize};

use crate::{
    dates,
    dav::{self, PropRequest, DAV},
    digest,
    http::{Head, Response, Status},
};

pub mod cards;
pub mod events;

/// CalDAV's namespace.
pub const CALDAV: &str = "urn:ietf:params:xml:ns:caldav";
/// CardDAV's namespace.
pub const CARDDAV: &str = "urn:ietf:params:xml:ns:carddav";
/// Apple's calendar server's namespace (`getctag`, `email-address-set`).
pub const CALSERVER: &str = "http://calendarserver.org/ns/";
/// Apple iCal's namespace (`calendar-color`).
pub const APPLE: &str = "http://apple.com/ns/ical/";
/// The principal's path.
pub const PRINCIPAL: &str = "/principal/";
/// The calendar home.
pub const CALENDAR_HOME: &str = "/calendars/";
/// The address-book home.
pub const BOOK_HOME: &str = "/addressbooks/";
/// AzCalendar's data folder in the drive (the grant azul-calendar-core's tests read it through).
pub const CALENDAR_FOLDER: &str = "calendar/";
/// The one address book's URL segment.
pub const BOOK: &str = "contacts";
/// The default calendar's URL segment (its id is empty).
pub const DEFAULT_SEGMENT: &str = "default";
/// The client names in the state folder.
pub const NAMES_FILE: &str = "pim-names.json";
/// Its `format`.
pub const NAMES_FORMAT: &str = "azul-bridge.pim-names";
/// A resource name a program gives is at most this long (in bytes).
pub const MAX_NAME: usize = 255;

const ALLOW: &str = "OPTIONS, GET, HEAD, PUT, DELETE, PROPFIND, PROPPATCH, REPORT";

/// The start of every multistatus: the namespaces the answers' prefixes stand for.
const MULTISTATUS: &str = "<?xml version=\"1.0\" encoding=\"utf-8\"?><D:multistatus xmlns:D=\"DAV:\" \
     xmlns:C=\"urn:ietf:params:xml:ns:caldav\" xmlns:CR=\"urn:ietf:params:xml:ns:carddav\" \
     xmlns:CS=\"http://calendarserver.org/ns/\" xmlns:A=\"http://apple.com/ns/ical/\">";

/// What `allprop` answers (the live properties of DAV: an item has).
const ALL_PROPS: [(&str, &str); 6] = [
    (DAV, "resourcetype"),
    (DAV, "displayname"),
    (DAV, "getetag"),
    (DAV, "getcontenttype"),
    (DAV, "getlastmodified"),
    (DAV, "getcontentlength"),
];

/// What `propname` lists (every property the bridge answers but the data itself).
const PROP_NAMES: [(&str, &str); 21] = [
    (DAV, "resourcetype"),
    (DAV, "displayname"),
    (DAV, "getetag"),
    (DAV, "getcontenttype"),
    (DAV, "getlastmodified"),
    (DAV, "getcontentlength"),
    (DAV, "current-user-principal"),
    (DAV, "principal-URL"),
    (DAV, "owner"),
    (DAV, "current-user-privilege-set"),
    (DAV, "supported-report-set"),
    (CALDAV, "calendar-home-set"),
    (CALDAV, "calendar-user-address-set"),
    (CALDAV, "supported-calendar-component-set"),
    (CALDAV, "supported-calendar-data"),
    (CARDDAV, "addressbook-home-set"),
    (CARDDAV, "supported-address-data"),
    (CALSERVER, "getctag"),
    (CALSERVER, "email-address-set"),
    (APPLE, "calendar-color"),
    (DAV, "sync-token"),
];

const READ_ONLY: &str = "<D:privilege><D:read/></D:privilege>\
     <D:privilege><D:read-current-user-privilege-set/></D:privilege>";
const READ_WRITE: &str = "<D:privilege><D:read/></D:privilege><D:privilege><D:write/></D:privilege>\
     <D:privilege><D:write-content/></D:privilege><D:privilege><D:bind/></D:privilege>\
     <D:privilege><D:unbind/></D:privilege><D:privilege><D:read-current-user-privilege-set/></D:privilege>";
const CALENDAR_REPORTS: &str = "<D:supported-report><D:report><C:calendar-multiget/></D:report></D:supported-report>\
     <D:supported-report><D:report><C:calendar-query/></D:report></D:supported-report>\
     <D:supported-report><D:report><D:sync-collection/></D:report></D:supported-report>";
const BOOK_REPORTS: &str = "<D:supported-report><D:report><CR:addressbook-multiget/></D:report></D:supported-report>\
     <D:supported-report><D:report><CR:addressbook-query/></D:report></D:supported-report>\
     <D:supported-report><D:report><D:sync-collection/></D:report></D:supported-report>";
/// The start of every sync token (RFC 6578 wants a URI); the rest is a hash of the collection's
/// members and their versions.
const SYNC_PREFIX: &str = "http://azlin-bridge.localhost/sync/";
/// Sync tokens kept per collection; a program with an older one lists the collection again.
const SYNC_KEPT: usize = 32;

/// What a sync token names: a collection's members (href to version) when it was given out.
type Snapshot = BTreeMap<String, String>;
const ADDRESS_DATA_TYPES: &str = "<CR:address-data-type content-type=\"text/vcard\" version=\"3.0\"/>\
     <CR:address-data-type content-type=\"text/vcard\" version=\"4.0\"/>";

// ---- the names programs give ----

/// Which kind of resource a name is of.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Event,
    Contact,
    /// A calendar a program made (MKCALENDAR) under a path that is no calendar id.
    Calendar,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
struct NamesFile {
    #[serde(default)]
    format: String,
    #[serde(default)]
    version: u32,
    /// An event's resource name to the event's id.
    #[serde(default)]
    events: BTreeMap<String, String>,
    /// A contact's resource name to the contact's UID (its file name).
    #[serde(default)]
    contacts: BTreeMap<String, String>,
    /// A calendar's URL segment to the calendar's id.
    #[serde(default)]
    calendars: BTreeMap<String, String>,
}

/// The names programs gave resources whose files are named otherwise, kept in the state folder
/// (or in memory): the name a program PUTs a new resource under is what it asks for later.
#[derive(Debug)]
pub struct Names {
    path: Option<PathBuf>,
    file: Mutex<NamesFile>,
}

impl Names {
    /// Names in memory only (a demo, the tests).
    #[must_use]
    pub fn in_memory() -> Names {
        Names {
            path: None,
            file: Mutex::new(NamesFile::default()),
        }
    }

    /// The names kept in `path` (a file that is not the bridge's starts empty: the programs
    /// then see the files' own names and fetch those).
    #[must_use]
    pub fn in_file(path: PathBuf) -> Names {
        let file = azcloud_kit::state::read_json::<NamesFile>(&path)
            .ok()
            .flatten()
            .filter(|file| file.format == NAMES_FORMAT)
            .unwrap_or_default();
        Names {
            path: Some(path),
            file: Mutex::new(file),
        }
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, NamesFile> {
        self.file
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    fn save(&self, file: &NamesFile) {
        let Some(path) = &self.path else {
            return;
        };
        let mut file = file.clone();
        file.format = NAMES_FORMAT.to_string();
        file.version = 1;
        // Names that cannot be kept are the files' own names next time; the programs fetch them.
        let _ = azcloud_kit::state::write_json(path, &file, false);
    }

    fn map(file: &mut NamesFile, kind: Kind) -> &mut BTreeMap<String, String> {
        match kind {
            Kind::Event => &mut file.events,
            Kind::Contact => &mut file.contacts,
            Kind::Calendar => &mut file.calendars,
        }
    }

    /// The id a program's `name` stands for.
    #[must_use]
    pub fn id_of(&self, kind: Kind, name: &str) -> Option<String> {
        Names::map(&mut self.lock(), kind).get(name).cloned()
    }

    /// The name a program gave the file `id`.
    #[must_use]
    pub fn name_of(&self, kind: Kind, id: &str) -> Option<String> {
        Names::map(&mut self.lock(), kind)
            .iter()
            .find(|(_, file_id)| file_id.as_str() == id)
            .map(|(name, _)| name.clone())
    }

    /// Keeps that `name` stands for `id`.
    pub fn set(&self, kind: Kind, name: &str, id: &str) {
        let mut file = self.lock();
        let map = Names::map(&mut file, kind);
        if map.get(name).map(String::as_str) == Some(id) {
            return;
        }
        map.insert(name.to_string(), id.to_string());
        self.save(&file);
    }

    /// Forgets `name` (its resource was deleted).
    pub fn forget(&self, kind: Kind, name: &str) {
        let mut file = self.lock();
        if Names::map(&mut file, kind).remove(name).is_some() {
            self.save(&file);
        }
    }

    /// Forgets the names whose files are not among `present` (another program deleted them).
    pub fn keep_only(&self, kind: Kind, present: &HashSet<String>) {
        let mut file = self.lock();
        let map = Names::map(&mut file, kind);
        let before = map.len();
        map.retain(|_, id| present.contains(id));
        if map.len() != before {
            self.save(&file);
        }
    }
}

// ---- what a path names ----

/// What a request's path names.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Place {
    WellKnown,
    Root,
    Principal,
    CalendarHome,
    /// A calendar, by its URL segment ([`DEFAULT_SEGMENT`] or the calendar's id).
    Calendar(String),
    /// An event: its calendar's segment and its resource name without `.ics`.
    Event(String, String),
    BookHome,
    Book,
    /// A contact: its resource name without `.vcf`.
    Card(String),
}

/// What `target` names: `Ok(None)` for a path that is nothing here.
///
/// # Errors
///
/// [`dav::key_of`]'s: a path that is not one or tries to leave (`..`, a backslash, NUL); 414 for
/// a resource name longer than [`MAX_NAME`].
pub fn place_of(target: &str) -> Result<Option<Place>, Status> {
    let (key, _) = dav::key_of(target)?;
    let levels: Vec<&str> = if key.is_empty() { Vec::new() } else { key.split('/').collect() };
    if levels.iter().any(|level| level.len() > MAX_NAME) {
        return Err(Status::URI_TOO_LONG);
    }
    let named = |file: &str, suffix: &str| -> Option<String> {
        file.strip_suffix(suffix)
            .filter(|name| !name.is_empty())
            .map(str::to_string)
    };
    Ok(match levels.as_slice() {
        [] => Some(Place::Root),
        [".well-known", "caldav" | "carddav"] => Some(Place::WellKnown),
        ["principal"] => Some(Place::Principal),
        ["calendars"] => Some(Place::CalendarHome),
        ["calendars", segment] => Some(Place::Calendar((*segment).to_string())),
        ["calendars", segment, file] => named(*file, ".ics").map(|name| Place::Event((*segment).to_string(), name)),
        ["addressbooks"] => Some(Place::BookHome),
        ["addressbooks", BOOK] => Some(Place::Book),
        ["addressbooks", BOOK, file] => named(*file, ".vcf").map(Place::Card),
        _ => None,
    })
}

// ---- what a response tells of ----

/// What one `<D:response>` tells of.
#[derive(Debug, Clone)]
pub(crate) enum Item {
    Root,
    Principal,
    CalendarHome,
    BookHome,
    /// A calendar: its URL segment, the calendar, its CTag.
    Calendar {
        segment: String,
        calendar: Calendar,
        ctag: String,
    },
    /// The address book and its CTag.
    Book { ctag: String },
    /// An event: its calendar's segment and name, its resource name, its file.
    Event {
        segment: String,
        calendar_name: String,
        name: String,
        info: ObjectInfo,
        event: Event,
    },
    /// A contact: its resource name and its file.
    Card { name: String, info: ObjectInfo },
}

impl Item {
    /// The item's href.
    fn href(&self) -> String {
        match self {
            Item::Root => String::from("/"),
            Item::Principal => String::from(PRINCIPAL),
            Item::CalendarHome => String::from(CALENDAR_HOME),
            Item::BookHome => String::from(BOOK_HOME),
            Item::Calendar { segment, .. } => format!("{CALENDAR_HOME}{}/", dav::encode_segment(segment)),
            Item::Book { .. } => format!("{BOOK_HOME}{BOOK}/"),
            Item::Event { segment, name, .. } => event_href(segment, name),
            Item::Card { name, .. } => card_href(name),
        }
    }

    fn resourcetype(&self) -> &'static str {
        match self {
            Item::Root | Item::CalendarHome | Item::BookHome => "<D:collection/>",
            Item::Principal => "<D:collection/><D:principal/>",
            Item::Calendar { .. } => "<D:collection/><C:calendar/>",
            Item::Book { .. } => "<D:collection/><CR:addressbook/>",
            Item::Event { .. } | Item::Card { .. } => "",
        }
    }

    /// The file an event or a contact is.
    fn info(&self) -> Option<&ObjectInfo> {
        match self {
            Item::Event { info, .. } | Item::Card { info, .. } => Some(info),
            _ => None,
        }
    }

    /// Whether a program may change what is in it (the address book, a calendar, their items).
    fn writable(&self) -> bool {
        matches!(
            self,
            Item::Calendar { .. } | Item::Book { .. } | Item::Event { .. } | Item::Card { .. }
        )
    }
}

/// The href of a collection `place` names (a calendar, the address book; the others' own).
fn collection_href(place: &Place) -> String {
    match place {
        Place::Calendar(segment) => format!("{CALENDAR_HOME}{}/", dav::encode_segment(segment)),
        Place::Book => format!("{BOOK_HOME}{BOOK}/"),
        Place::Principal => String::from(PRINCIPAL),
        Place::CalendarHome => String::from(CALENDAR_HOME),
        Place::BookHome => String::from(BOOK_HOME),
        Place::Event(segment, name) => event_href(segment, name),
        Place::Card(name) => card_href(name),
        Place::Root | Place::WellKnown => String::from("/"),
    }
}

/// An event's href.
#[must_use]
pub fn event_href(segment: &str, name: &str) -> String {
    format!(
        "{CALENDAR_HOME}{}/{}.ics",
        dav::encode_segment(segment),
        dav::encode_segment(name)
    )
}

/// A contact's href.
#[must_use]
pub fn card_href(name: &str) -> String {
    format!("{BOOK_HOME}{BOOK}/{}.vcf", dav::encode_segment(name))
}

/// A file's version, the ETag of what is served from it: the drive's entity tag, else (a folder
/// on disk keeps none) its time and size.
#[must_use]
pub fn version_of(info: &ObjectInfo) -> String {
    match &info.etag {
        Some(etag) => etag.trim_matches('"').to_string(),
        None => format!("{}-{}", info.modified.unwrap_or(0), info.size),
    }
}

/// A collection's CTag: a hash of `text` (its members' names and versions, and whatever else
/// shows in the collection).
fn ctag_of(text: &str) -> String {
    digest::md5_hex(text)
}

/// `<P:name>inner</P:name>`, with the prefix the multistatus declares for `namespace`.
fn element(namespace: &str, name: &str, inner: &str) -> String {
    let prefix = match namespace {
        CALDAV => "C",
        CARDDAV => "CR",
        CALSERVER => "CS",
        APPLE => "A",
        _ => "D",
    };
    if inner.is_empty() {
        format!("<{prefix}:{name}/>")
    } else {
        format!("<{prefix}:{name}>{inner}</{prefix}:{name}>")
    }
}

fn href_element(path: &str) -> String {
    format!("<D:href>{}</D:href>", dav::xml_escape(path))
}

/// One `<D:response>`: the properties found (200) and those not (404).
fn propstats(href: &str, found: &[String], missing: &[String]) -> String {
    let mut out = format!("<D:response>{}", href_element(href));
    if !found.is_empty() || missing.is_empty() {
        out.push_str("<D:propstat><D:prop>");
        out.push_str(&found.concat());
        out.push_str("</D:prop><D:status>HTTP/1.1 200 OK</D:status></D:propstat>");
    }
    if !missing.is_empty() {
        out.push_str("<D:propstat><D:prop>");
        out.push_str(&missing.concat());
        out.push_str("</D:prop><D:status>HTTP/1.1 404 Not Found</D:status></D:propstat>");
    }
    out.push_str("</D:response>");
    out
}

/// A `<D:response>` of a multiget's href that is not there.
fn missing_response(href: &str) -> String {
    format!(
        "<D:response>{}<D:status>HTTP/1.1 404 Not Found</D:status></D:response>",
        href_element(href)
    )
}

fn multistatus(mut body: String) -> Response {
    body.push_str("</D:multistatus>");
    Response::new(Status::MULTI_STATUS).with_body("application/xml; charset=utf-8", body.into_bytes())
}

/// An answer that names the precondition that failed (`<C:valid-calendar-data/>`, ...).
fn dav_error(status: Status, condition: &str) -> Response {
    let body = format!(
        "<?xml version=\"1.0\" encoding=\"utf-8\"?><D:error xmlns:D=\"DAV:\" xmlns:C=\"{CALDAV}\" \
         xmlns:CR=\"{CARDDAV}\">{condition}</D:error>"
    );
    Response::new(status).with_body("application/xml; charset=utf-8", body.into_bytes())
}

/// Whether `version` is among the entity tags of an `If-Match` / `If-None-Match` value (`*` is
/// every version).
fn listed(value: &str, version: &str) -> bool {
    value.split(',').map(str::trim).any(|tag| {
        tag == "*" || tag.trim_start_matches("W/").trim_matches('"') == version
    })
}

/// `If-None-Match` (`*`: only a new resource) and `If-Match` (only that version) against the
/// version there is (`None`: nothing yet); `Some` is the 412 to answer.
fn precondition(head: &Head, current: Option<&str>) -> Option<Response> {
    let failed = || Some(Response::text(Status::PRECONDITION_FAILED, "It changed, or it is there already."));
    if let (Some(value), Some(version)) = (head.header("If-None-Match"), current) {
        if listed(value, version) {
            return failed();
        }
    }
    if let Some(value) = head.header("If-Match") {
        if !current.is_some_and(|version| listed(value, version)) {
            return failed();
        }
    }
    None
}

// ---- REPORT ----

/// Which REPORT.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ReportKind {
    CalendarMultiget,
    CalendarQuery,
    BookMultiget,
    BookQuery,
    /// RFC 6578's sync-collection.
    Sync,
}

/// A REPORT's body.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Report {
    kind: ReportKind,
    props: PropRequest,
    hrefs: Vec<String>,
    /// calendar-query: the component asked for (`VEVENT`, `VTODO`), if one is named...
    component: Option<String>,
    /// ...and its time range (UTC; either end may be open).
    range: Option<(Option<chrono::NaiveDateTime>, Option<chrono::NaiveDateTime>)>,
    /// The vCard version address-data asks for (3.0 unless it says `version="4.0"`).
    vcard: cards::Version,
    /// sync-collection: the token the program has (empty: none yet).
    token: String,
}

/// Why a REPORT is not answered.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Refusal {
    /// Not XML, or not a REPORT.
    Bad,
    /// A report the bridge does not make (sync-collection, free-busy-query, ...).
    Unsupported,
}

fn is_caldav(node: &roxmltree::Node<'_, '_>, name: &str) -> bool {
    node.is_element() && node.tag_name().name() == name && node.tag_name().namespace() == Some(CALDAV)
}

/// `20261001T000000Z`: a time range's end.
fn utc_time(text: &str) -> Option<chrono::NaiveDateTime> {
    chrono::NaiveDateTime::parse_from_str(text.trim().trim_end_matches('Z'), "%Y%m%dT%H%M%S").ok()
}

fn prop_names(prop: &roxmltree::Node<'_, '_>) -> Vec<(String, String)> {
    prop.children()
        .filter(roxmltree::Node::is_element)
        .map(|p| {
            (
                p.tag_name().namespace().unwrap_or_default().to_string(),
                p.tag_name().name().to_string(),
            )
        })
        .collect()
}

fn report_of(body: &[u8]) -> Result<Report, Refusal> {
    let text = dav::parse_xml(body).map_err(|_| Refusal::Bad)?.ok_or(Refusal::Bad)?;
    let doc = roxmltree::Document::parse(&text).map_err(|_| Refusal::Bad)?;
    let root = doc.root_element();
    let kind = match (root.tag_name().namespace().unwrap_or_default(), root.tag_name().name()) {
        (CALDAV, "calendar-multiget") => ReportKind::CalendarMultiget,
        (CALDAV, "calendar-query") => ReportKind::CalendarQuery,
        (CARDDAV, "addressbook-multiget") => ReportKind::BookMultiget,
        (CARDDAV, "addressbook-query") => ReportKind::BookQuery,
        (DAV, "sync-collection") => ReportKind::Sync,
        _ => return Err(Refusal::Unsupported),
    };
    let mut report = Report {
        kind,
        props: PropRequest::All,
        hrefs: Vec::new(),
        component: None,
        range: None,
        vcard: cards::Version::V3,
        token: String::new(),
    };
    for child in root.children().filter(roxmltree::Node::is_element) {
        if dav::is_dav(&child, "prop") {
            report.props = PropRequest::Some(prop_names(&child));
            let asks_4 = child.children().any(|p| {
                p.is_element()
                    && p.tag_name().name() == "address-data"
                    && p.tag_name().namespace() == Some(CARDDAV)
                    && p.attribute("version").is_some_and(|v| v.trim() == "4.0")
            });
            if asks_4 {
                report.vcard = cards::Version::V4;
            }
        } else if dav::is_dav(&child, "sync-token") {
            report.token = child.text().unwrap_or_default().trim().to_string();
        } else if dav::is_dav(&child, "propname") {
            report.props = PropRequest::Names;
        } else if dav::is_dav(&child, "href") {
            if let Some(href) = child.text().map(str::trim).filter(|h| !h.is_empty()) {
                report.hrefs.push(href.to_string());
            }
        } else if is_caldav(&child, "filter") {
            // VCALENDAR > VEVENT (or VTODO ...) > time-range
            let inner = child
                .children()
                .filter(|n| is_caldav(n, "comp-filter"))
                .flat_map(|calendar| calendar.children().filter(|n| is_caldav(n, "comp-filter")))
                .next();
            if let Some(component) = inner {
                report.component = component.attribute("name").map(str::to_ascii_uppercase);
                report.range = component
                    .children()
                    .find(|n| is_caldav(n, "time-range"))
                    .map(|range| {
                        (
                            range.attribute("start").and_then(utc_time),
                            range.attribute("end").and_then(utc_time),
                        )
                    });
            }
        }
    }
    Ok(report)
}

/// One property a PROPPATCH (or MKCALENDAR) sets to `value`, or removes (`None`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Patched {
    pub namespace: String,
    pub name: String,
    pub value: Option<String>,
}

/// The properties of the `set` / `remove` elements of `root` (a propertyupdate, a mkcalendar).
fn patched_of(root: &roxmltree::Node<'_, '_>) -> Vec<Patched> {
    let mut out = Vec::new();
    for action in root.children().filter(|n| dav::is_dav(n, "set") || dav::is_dav(n, "remove")) {
        let set = dav::is_dav(&action, "set");
        for prop in action.children().filter(|n| dav::is_dav(n, "prop")) {
            for p in prop.children().filter(roxmltree::Node::is_element) {
                out.push(Patched {
                    namespace: p.tag_name().namespace().unwrap_or_default().to_string(),
                    name: p.tag_name().name().to_string(),
                    value: set.then(|| p.text().unwrap_or_default().trim().to_string()),
                });
            }
        }
    }
    out
}

/// A PROPPATCH answer's propstat: the properties `props` (empty elements) with `status`.
fn patch_propstat(props: &[&Patched], status: &str) -> String {
    let elements: String = props
        .iter()
        .map(|p| dav::empty_element(&p.namespace, &p.name))
        .collect();
    format!("<D:propstat><D:prop>{elements}</D:prop><D:status>HTTP/1.1 {status}</D:status></D:propstat>")
}

/// The properties a PROPPATCH sets or removes; `None` for a body that is no propertyupdate.
fn patched_props(body: &[u8]) -> Option<Vec<Patched>> {
    let text = dav::parse_xml(body).ok()??;
    let doc = roxmltree::Document::parse(&text).ok()?;
    let root = doc.root_element();
    dav::is_dav(&root, "propertyupdate").then(|| patched_of(&root))
}

/// The properties a MKCALENDAR body sets; nothing for no body; `None` for a body that is no
/// mkcalendar.
fn mkcalendar_props(body: &[u8]) -> Option<Vec<Patched>> {
    let Ok(text) = dav::parse_xml(body) else {
        return None;
    };
    let Some(text) = text else {
        return Some(Vec::new());
    };
    let doc = roxmltree::Document::parse(&text).ok()?;
    let root = doc.root_element();
    is_caldav(&root, "mkcalendar").then(|| patched_of(&root))
}

// ---- the server ----

/// CalDAV and CardDAV: what every connection shares.
pub struct Pim {
    /// The drive AzContacts' cards are in (`contacts/<uid>.vcf`).
    contacts: Arc<dyn Drive>,
    /// AzCalendar's data folder as a drive (`events/<id>.json`, `calendars/<id>.json`).
    calendar: Arc<dyn Drive>,
    names: Names,
    /// The account's address: the principal's name and calendar user address.
    address: String,
    /// The events read so far, by key, with the version they were read at: a listing reads a
    /// file again only when it changed.
    events: Mutex<HashMap<String, (String, Event)>>,
    /// The sync tokens given out, per collection href, with what they name.
    sync: Mutex<HashMap<String, Vec<(String, Snapshot)>>>,
}

impl std::fmt::Debug for Pim {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Pim")
            .field("names", &self.names)
            .finish_non_exhaustive()
    }
}

impl Pim {
    /// The calendars and contacts of `contacts` (the drive: `contacts/<uid>.vcf`) and
    /// `calendar` (AzCalendar's data folder: `events/`, `calendars/`), for the user `address`.
    #[must_use]
    pub fn new(contacts: Arc<dyn Drive>, calendar: Arc<dyn Drive>, names: Names, address: &str) -> Pim {
        Pim {
            contacts,
            calendar,
            names,
            address: address.trim().to_string(),
            events: Mutex::new(HashMap::new()),
            sync: Mutex::new(HashMap::new()),
        }
    }

    /// The drive the contacts are in.
    #[must_use]
    pub fn contacts_drive(&self) -> Arc<dyn Drive> {
        self.contacts.clone()
    }

    /// The answer to one request whose sign-in and body are done.
    #[must_use]
    pub fn respond(&self, head: &Head, body: &[u8]) -> Response {
        let place = match place_of(&head.target) {
            Ok(Some(place)) => place,
            Ok(None) => return Response::text(Status::NOT_FOUND, "Not there."),
            Err(status) => return Response::text(status, "Not a path of the bridge's calendars and contacts."),
        };
        let result = match head.method.as_str() {
            "OPTIONS" => Ok(Response::new(Status::OK)
                .with_header("DAV", "1, 3, calendar-access, addressbook")
                .with_header("Allow", ALLOW)),
            _ if place == Place::WellKnown => {
                Ok(Response::new(Status(301, "Moved Permanently")).with_header("Location", "/"))
            }
            "PROPFIND" => self.propfind(head, &place, body),
            "REPORT" => self.report(&place, body),
            "GET" | "HEAD" => self.get(head, &place),
            "PUT" => self.put(head, &place, body),
            "DELETE" => self.delete(head, &place),
            "PROPPATCH" => self.proppatch(&place, body),
            "MKCALENDAR" => self.mkcalendar(&place, body),
            "MKCOL" => Ok(Response::text(
                Status::FORBIDDEN,
                "Calendars are made with MKCALENDAR; the address book is the one AzContacts keeps.",
            )),
            _ => Ok(Response::text(
                Status::METHOD_NOT_ALLOWED,
                "Not a method of the bridge's calendars and contacts.",
            )
            .with_header("Allow", ALLOW)),
        };
        result.unwrap_or_else(dav::drive_answer)
    }

    /// The item `place` names, if it is there.
    fn item(&self, place: &Place) -> Result<Option<Item>, DriveError> {
        Ok(match place {
            Place::WellKnown => None,
            Place::Root => Some(Item::Root),
            Place::Principal => Some(Item::Principal),
            Place::CalendarHome => Some(Item::CalendarHome),
            Place::BookHome => Some(Item::BookHome),
            Place::Calendar(segment) => self
                .calendar_items()?
                .into_iter()
                .find(|item| matches!(item, Item::Calendar { segment: s, .. } if s == segment)),
            Place::Event(segment, name) => self.event_item(segment, name)?,
            Place::Book => Some(self.book_item()?),
            Place::Card(name) => self
                .card(name)?
                .map(|(_, info)| Item::Card { name: name.clone(), info }),
        })
    }

    /// The items one level below `place`.
    fn children(&self, place: &Place) -> Result<Vec<Item>, DriveError> {
        Ok(match place {
            Place::Root => vec![Item::Principal, Item::CalendarHome, Item::BookHome],
            Place::CalendarHome => self.calendar_items()?,
            Place::Calendar(segment) => self.event_items(segment)?,
            Place::BookHome => vec![self.book_item()?],
            Place::Book => self
                .cards()?
                .into_iter()
                .map(|card| Item::Card {
                    name: card.name,
                    info: card.info,
                })
                .collect(),
            _ => Vec::new(),
        })
    }

    fn display_name(&self, item: &Item) -> Option<String> {
        Some(match item {
            Item::Root => String::from("Azlin Bridge"),
            Item::Principal => self.address.clone(),
            Item::CalendarHome => String::from("Calendars"),
            Item::BookHome => String::from("Address books"),
            Item::Calendar { calendar, .. } => calendar.name.clone(),
            Item::Book { .. } => String::from("Contacts"),
            Item::Event { event, .. } => event.title.clone(),
            Item::Card { .. } => return None,
        })
    }

    /// The property `(namespace, name)` of `item` as its element, if the item has it.
    fn prop(&self, item: &Item, namespace: &str, name: &str, vcard: cards::Version) -> Option<String> {
        let principal = matches!(item, Item::Principal);
        let value = match (namespace, name) {
            (DAV, "resourcetype") => element(DAV, name, item.resourcetype()),
            (DAV, "displayname") => element(DAV, name, &dav::xml_escape(&self.display_name(item)?)),
            (DAV, "current-user-principal") => element(DAV, name, &href_element(PRINCIPAL)),
            (DAV, "principal-URL") if principal => element(DAV, name, &href_element(PRINCIPAL)),
            (DAV, "owner") if item.writable() => element(DAV, name, &href_element(PRINCIPAL)),
            (DAV, "current-user-privilege-set") => {
                element(DAV, name, if item.writable() { READ_WRITE } else { READ_ONLY })
            }
            (DAV, "supported-report-set") => match item {
                Item::Calendar { .. } => element(DAV, name, CALENDAR_REPORTS),
                Item::Book { .. } => element(DAV, name, BOOK_REPORTS),
                _ => return None,
            },
            (DAV, "getetag") => element(
                DAV,
                name,
                &format!("\"{}\"", dav::xml_escape(&version_of(item.info()?))),
            ),
            (DAV, "getcontenttype") => match item {
                Item::Event { .. } => element(DAV, name, events::CONTENT_TYPE),
                Item::Card { .. } => element(DAV, name, cards::CONTENT_TYPE),
                _ => return None,
            },
            (DAV, "getlastmodified") => element(
                DAV,
                name,
                &dates::http_date(i64::try_from(item.info()?.modified?).unwrap_or(0)),
            ),
            (DAV, "getcontentlength") => match item {
                Item::Card { info, .. } => element(DAV, name, &info.size.to_string()),
                _ => return None,
            },
            (CALDAV, "calendar-home-set") if principal => element(CALDAV, name, &href_element(CALENDAR_HOME)),
            (CALDAV, "calendar-user-address-set") if principal => {
                element(CALDAV, name, &href_element(&format!("mailto:{}", self.address)))
            }
            (CALDAV, "supported-calendar-component-set") if matches!(item, Item::Calendar { .. }) => {
                element(CALDAV, name, "<C:comp name=\"VEVENT\"/>")
            }
            (CALDAV, "supported-calendar-data") if matches!(item, Item::Calendar { .. }) => element(
                CALDAV,
                name,
                "<C:calendar-data content-type=\"text/calendar\" version=\"2.0\"/>",
            ),
            (CALDAV, "calendar-data") => match item {
                Item::Event {
                    calendar_name,
                    info,
                    event,
                    ..
                } => element(CALDAV, name, &dav::xml_escape(&events::ics_of(event, calendar_name, info))),
                _ => return None,
            },
            (CARDDAV, "addressbook-home-set") if principal => element(CARDDAV, name, &href_element(BOOK_HOME)),
            (CARDDAV, "supported-address-data") if matches!(item, Item::Book { .. }) => {
                element(CARDDAV, name, ADDRESS_DATA_TYPES)
            }
            (CARDDAV, "address-data") => match item {
                Item::Card { info, .. } => {
                    let uid = cards::uid_of_key(&info.key)?;
                    let bytes = cards::card_as(&self.contacts.get(&info.key).ok()?, &uid, vcard);
                    element(CARDDAV, name, &dav::xml_escape(&String::from_utf8_lossy(&bytes)))
                }
                _ => return None,
            },
            (DAV, "sync-token") => element(DAV, name, &dav::xml_escape(&self.sync_token(item)?)),
            (CALSERVER, "getctag") => match item {
                Item::Calendar { ctag, .. } | Item::Book { ctag } => element(CALSERVER, name, &dav::xml_escape(ctag)),
                _ => return None,
            },
            (CALSERVER, "email-address-set") if principal => element(
                CALSERVER,
                name,
                &format!("<CS:email-address>{}</CS:email-address>", dav::xml_escape(&self.address)),
            ),
            (APPLE, "calendar-color") => match item {
                Item::Calendar { calendar, .. } => element(APPLE, name, &events::colour_of(calendar)),
                _ => return None,
            },
            _ => return None,
        };
        Some(value)
    }

    /// One `<D:response>` of `item` with the properties `request` asks for.
    fn response_of(&self, item: &Item, request: &PropRequest, vcard: cards::Version) -> String {
        let mut found = Vec::new();
        let mut missing = Vec::new();
        match request {
            PropRequest::All => {
                for (namespace, name) in ALL_PROPS {
                    if let Some(value) = self.prop(item, namespace, name, vcard) {
                        found.push(value);
                    }
                }
            }
            PropRequest::Names => {
                for (namespace, name) in PROP_NAMES {
                    if self.prop(item, namespace, name, vcard).is_some() {
                        found.push(element(namespace, name, ""));
                    }
                }
            }
            PropRequest::Some(names) => {
                for (namespace, name) in names {
                    match self.prop(item, namespace, name, vcard) {
                        Some(value) => found.push(value),
                        None => missing.push(dav::empty_element(namespace, name)),
                    }
                }
            }
        }
        propstats(&item.href(), &found, &missing)
    }

    // ---- the methods ----

    fn propfind(&self, head: &Head, place: &Place, body: &[u8]) -> Result<Response, DriveError> {
        let depth = head.header("Depth").unwrap_or("infinity").trim().to_ascii_lowercase();
        if depth != "0" && depth != "1" {
            return Ok(dav_error(Status::FORBIDDEN, "<D:propfind-finite-depth/>"));
        }
        let request = match dav::prop_request(body) {
            Ok(request) => request,
            Err(status) => return Ok(Response::text(status, "Not a PROPFIND body the bridge reads.")),
        };
        let Some(item) = self.item(place)? else {
            return Ok(Response::text(Status::NOT_FOUND, "Not there."));
        };
        let mut out = String::from(MULTISTATUS);
        out.push_str(&self.response_of(&item, &request, cards::Version::V3));
        if depth == "1" {
            for child in self.children(place)? {
                out.push_str(&self.response_of(&child, &request, cards::Version::V3));
            }
        }
        Ok(multistatus(out))
    }

    fn report(&self, place: &Place, body: &[u8]) -> Result<Response, DriveError> {
        let report = match report_of(body) {
            Ok(report) => report,
            Err(Refusal::Unsupported) => return Ok(dav_error(Status::FORBIDDEN, "<D:supported-report/>")),
            Err(Refusal::Bad) => return Ok(Response::text(Status::BAD_REQUEST, "Not a REPORT body the bridge reads.")),
        };
        if report.kind == ReportKind::Sync {
            return self.sync_collection(place, &report);
        }
        let mut out = String::from(MULTISTATUS);
        match report.kind {
            ReportKind::Sync => {}
            ReportKind::CalendarMultiget | ReportKind::BookMultiget => {
                for href in &report.hrefs {
                    let item = match place_of(href) {
                        Ok(Some(Place::Event(segment, name))) if report.kind == ReportKind::CalendarMultiget => {
                            self.event_item(&segment, &name)?
                        }
                        Ok(Some(Place::Card(name))) if report.kind == ReportKind::BookMultiget => self
                            .card(&name)?
                            .map(|(_, info)| Item::Card { name, info }),
                        _ => None,
                    };
                    match item {
                        Some(item) => out.push_str(&self.response_of(&item, &report.props, report.vcard)),
                        None => out.push_str(&missing_response(href)),
                    }
                }
            }
            ReportKind::CalendarQuery => {
                let Place::Calendar(segment) = place else {
                    return Ok(Response::text(Status::FORBIDDEN, "calendar-query asks a calendar."));
                };
                // The bridge's calendars hold events only: a query for tasks finds none.
                if report.component.as_deref().is_none_or(|c| c == "VEVENT") {
                    let (from, to) = report.range.unwrap_or((None, None));
                    for item in self.event_items(segment)? {
                        if let Item::Event { event, .. } = &item {
                            if events::may_overlap(event, from, to) {
                                out.push_str(&self.response_of(&item, &report.props, report.vcard));
                            }
                        }
                    }
                }
            }
            ReportKind::BookQuery => {
                if *place != Place::Book {
                    return Ok(Response::text(Status::FORBIDDEN, "addressbook-query asks the address book."));
                }
                for card in self.cards()? {
                    let item = Item::Card {
                        name: card.name,
                        info: card.info,
                    };
                    out.push_str(&self.response_of(&item, &report.props, report.vcard));
                }
            }
        }
        Ok(multistatus(out))
    }

    fn get(&self, head: &Head, place: &Place) -> Result<Response, DriveError> {
        match place {
            Place::Event(segment, name) => self.get_event(segment, name),
            Place::Card(name) => self.get_card(name, head.header("Accept")),
            _ => match self.item(place)? {
                Some(_) => Ok(Response::text(
                    Status::OK,
                    "The Azlin Bridge's calendars and contacts: add this address to a calendar or \
                     contacts program as a CalDAV or CardDAV account.",
                )),
                None => Ok(Response::text(Status::NOT_FOUND, "Not there.")),
            },
        }
    }

    fn put(&self, head: &Head, place: &Place, body: &[u8]) -> Result<Response, DriveError> {
        match place {
            Place::Event(segment, name) => self.put_event(head, segment, name, body),
            Place::Card(name) => self.put_card(head, name, body),
            _ => Ok(Response::text(
                Status::METHOD_NOT_ALLOWED,
                "Events go into a calendar, contacts into the address book.",
            )),
        }
    }

    fn delete(&self, head: &Head, place: &Place) -> Result<Response, DriveError> {
        match place {
            Place::Event(segment, name) => self.delete_event(head, segment, name),
            Place::Card(name) => self.delete_card(head, name),
            _ => Ok(Response::text(
                Status::FORBIDDEN,
                "Calendars and the address book are deleted in AzCalendar and AzContacts.",
            )),
        }
    }

    // ---- sync-collection (RFC 6578) ----

    fn lock_sync(&self) -> std::sync::MutexGuard<'_, HashMap<String, Vec<(String, Snapshot)>>> {
        self.sync
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    /// The collection `place`'s members, its current sync token (a hash of their hrefs and
    /// versions) and what the token names - kept, so a program can ask what changed since.
    fn current_sync(&self, place: &Place) -> Result<(String, Vec<Item>, Snapshot), DriveError> {
        let items = self.children(place)?;
        let snapshot: Snapshot = items
            .iter()
            .filter_map(|item| Some((item.href(), version_of(item.info()?))))
            .collect();
        let text: String = snapshot.iter().map(|(href, version)| format!("{href}\n{version}\n")).collect();
        let token = format!("{SYNC_PREFIX}{}", ctag_of(&text));
        let mut sync = self.lock_sync();
        let kept = sync.entry(collection_href(place)).or_default();
        if !kept.iter().any(|(given, _)| *given == token) {
            kept.push((token.clone(), snapshot.clone()));
            if kept.len() > SYNC_KEPT {
                kept.remove(0);
            }
        }
        Ok((token, items, snapshot))
    }

    /// A calendar's or the address book's current sync token (its `sync-token` property).
    fn sync_token(&self, item: &Item) -> Option<String> {
        let place = match item {
            Item::Calendar { segment, .. } => Place::Calendar(segment.clone()),
            Item::Book { .. } => Place::Book,
            _ => return None,
        };
        self.current_sync(&place).ok().map(|(token, _, _)| token)
    }

    /// sync-collection: the members new or changed since the program's token (with the
    /// properties it asks for), the ones gone since (404), the new token. No token: every
    /// member. A token the bridge does not know (given before it started): 403 valid-sync-token,
    /// and the program lists again.
    fn sync_collection(&self, place: &Place, report: &Report) -> Result<Response, DriveError> {
        if !matches!(place, Place::Calendar(_) | Place::Book) {
            return Ok(Response::text(
                Status::FORBIDDEN,
                "sync-collection asks a calendar or the address book.",
            ));
        }
        let known = self
            .lock_sync()
            .get(&collection_href(place))
            .and_then(|kept| kept.iter().find(|(given, _)| *given == report.token))
            .map(|(_, snapshot)| snapshot.clone());
        let (token, items, now) = self.current_sync(place)?;
        let old = if report.token.is_empty() {
            Snapshot::new()
        } else if let Some(old) = known {
            old
        } else if report.token == token {
            now
        } else {
            return Ok(dav_error(Status::FORBIDDEN, "<D:valid-sync-token/>"));
        };
        let mut out = String::from(MULTISTATUS);
        let mut present = HashSet::new();
        for item in &items {
            let href = item.href();
            let version = item.info().map(version_of).unwrap_or_default();
            if old.get(&href) != Some(&version) {
                out.push_str(&self.response_of(item, &report.props, report.vcard));
            }
            present.insert(href);
        }
        for href in old.keys().filter(|href| !present.contains(*href)) {
            out.push_str(&missing_response(href));
        }
        out.push_str(&format!("<D:sync-token>{}</D:sync-token>", dav::xml_escape(&token)));
        Ok(multistatus(out))
    }

    /// A calendar's name and colour are set (AzCalendar's calendar file); every other property is
    /// dead and not kept: answered 403, and - a PROPPATCH being all or nothing - the others of
    /// the same request 424, so the program keeps its own.
    fn proppatch(&self, place: &Place, body: &[u8]) -> Result<Response, DriveError> {
        let Some(item) = self.item(place)? else {
            return Ok(Response::text(Status::NOT_FOUND, "Not there."));
        };
        let Some(props) = patched_props(body) else {
            return Ok(Response::text(Status::BAD_REQUEST, "Not a PROPPATCH body."));
        };
        let calendar = match &item {
            Item::Calendar { calendar, .. } => Some(calendar),
            _ => None,
        };
        let refused: Vec<&Patched> = props
            .iter()
            .filter(|p| calendar.is_none() || !events::patchable(p))
            .collect();
        let mut out = String::from(MULTISTATUS);
        out.push_str(&format!("<D:response>{}", href_element(&item.href())));
        if refused.is_empty() {
            if let Some(calendar) = calendar {
                self.patch_calendar(calendar, &props)?;
            }
            let all: Vec<&Patched> = props.iter().collect();
            out.push_str(&patch_propstat(&all, "200 OK"));
        } else {
            let others: Vec<&Patched> = props.iter().filter(|p| !refused.contains(p)).collect();
            out.push_str(&patch_propstat(&refused, "403 Forbidden"));
            if !others.is_empty() {
                out.push_str(&patch_propstat(&others, "424 Failed Dependency"));
            }
        }
        out.push_str("</D:response>");
        Ok(multistatus(out))
    }
}

#[cfg(test)]
mod tests;
