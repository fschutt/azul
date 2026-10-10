//! CalDAV and CardDAV requests as Apple Calendar and Contacts, Thunderbird and DAVx5 send them,
//! against drives in memory: what the bridge answers, what lands in AzCalendar's and AzContacts'
//! files.

use azcal_core::{
    calendars::{self, Colour},
    event::{self, Meeting},
};
use chrono::{NaiveDate, NaiveTime};

use super::*;
use crate::memory::MemoryDrive;

const ADDRESS: &str = "ada@example.org";
/// An event of the default calendar, one of Work, Work, a contact.
const EVENT: &str = "0b7e3c2a-4f1d-4c8e-9a6b-2d5f8e1c3a70";
const WORK_EVENT: &str = "5c1d9e2f-8a3b-4d6c-b7e4-1f2a3b4c5d6e";
const WORK: &str = "9d4c1f3a-2b5e-4c7d-8e9f-0a1b2c3d4e5f";
const CARD: &str = "3f2a1b0c-9d8e-4f7a-b6c5-d4e3f2a1b0c9";
const MEETING_EVENT: &str = "7a6b5c4d-3e2f-4a1b-9c8d-7e6f5a4b3c2d";
const ROOM: &str = "a2h859hyqkfaa11nhzxfh3gd7f";

/// A card as AzContacts writes it (vCard 4.0).
const CARD_TEXT: &str = "BEGIN:VCARD\r\nVERSION:4.0\r\nUID:3f2a1b0c-9d8e-4f7a-b6c5-d4e3f2a1b0c9\r\n\
    FN:Grace Hopper\r\nN:Hopper;Grace;;;\r\nEMAIL:grace@example.org\r\nEND:VCARD\r\n";

const PRINCIPAL_PROPS: &str = "<?xml version=\"1.0\"?><D:propfind xmlns:D=\"DAV:\" \
    xmlns:C=\"urn:ietf:params:xml:ns:caldav\" xmlns:CR=\"urn:ietf:params:xml:ns:carddav\"><D:prop>\
    <D:current-user-principal/><D:resourcetype/><D:displayname/><C:calendar-home-set/>\
    <C:calendar-user-address-set/><CR:addressbook-home-set/><C:schedule-inbox-URL/></D:prop></D:propfind>";

const CALENDAR_PROPS: &str = "<?xml version=\"1.0\"?><D:propfind xmlns:D=\"DAV:\" \
    xmlns:C=\"urn:ietf:params:xml:ns:caldav\" xmlns:CS=\"http://calendarserver.org/ns/\" \
    xmlns:A=\"http://apple.com/ns/ical/\"><D:prop><D:resourcetype/><D:displayname/><D:getetag/>\
    <CS:getctag/><A:calendar-color/><C:supported-calendar-component-set/>\
    <D:current-user-privilege-set/></D:prop></D:propfind>";

const BOOK_PROPS: &str = "<?xml version=\"1.0\"?><D:propfind xmlns:D=\"DAV:\" \
    xmlns:CS=\"http://calendarserver.org/ns/\"><D:prop><D:resourcetype/><D:getetag/><CS:getctag/>\
    </D:prop></D:propfind>";

/// Thunderbird's new event.
const DENTIST: &str = "BEGIN:VCALENDAR\r\nVERSION:2.0\r\nPRODID:-//Mozilla.org/NONSGML Mozilla Calendar V1.1//EN\r\n\
    BEGIN:VEVENT\r\nUID:dentist@thunderbird\r\nDTSTAMP:20260930T120000Z\r\nDTSTART:20261002T100000\r\n\
    DTEND:20261002T110000\r\nSUMMARY:Dentist\r\nEND:VEVENT\r\nEND:VCALENDAR\r\n";

/// Apple Calendar's weekly event with one occurrence moved a day.
const WEEKLY: &str = "BEGIN:VCALENDAR\r\nVERSION:2.0\r\nPRODID:-//Apple Inc.//macOS 15//EN\r\n\
    BEGIN:VEVENT\r\nUID:yoga@apple\r\nDTSTAMP:20260930T120000Z\r\nDTSTART:20261001T180000\r\n\
    DTEND:20261001T190000\r\nRRULE:FREQ=WEEKLY\r\nSUMMARY:Yoga\r\nEND:VEVENT\r\n\
    BEGIN:VEVENT\r\nUID:yoga@apple\r\nDTSTAMP:20260930T120000Z\r\nRECURRENCE-ID:20261008T180000\r\n\
    DTSTART:20261009T180000\r\nDTEND:20261009T190000\r\nSUMMARY:Yoga (Friday)\r\nEND:VEVENT\r\n\
    END:VCALENDAR\r\n";

fn day(year: i32, month: u32, date: u32) -> NaiveDate {
    NaiveDate::from_ymd_opt(year, month, date).unwrap()
}

fn at(hour: u32, minute: u32) -> NaiveTime {
    NaiveTime::from_hms_opt(hour, minute, 0).unwrap()
}

struct Fixture {
    contacts: Arc<MemoryDrive>,
    calendar: Arc<MemoryDrive>,
    pim: Pim,
}

/// A contact (and two files that are none), the default calendar with one event, Work with one.
fn fixture() -> Fixture {
    let contacts = Arc::new(MemoryDrive::new());
    contacts.set_now(1_790_843_400);
    contacts.put(&format!("contacts/{CARD}.vcf"), CARD_TEXT.as_bytes()).unwrap();
    contacts.put("contacts/readme.txt", b"not a card").unwrap();
    contacts.put("contacts/old/x.vcf", CARD_TEXT.as_bytes()).unwrap();
    let calendar = Arc::new(MemoryDrive::new());
    calendar.set_now(1_790_843_400);
    let standup = Event::create(EVENT, "Standup", day(2026, 10, 1), at(9, 0), at(9, 15), None).unwrap();
    calendar.put(&event::object_key(EVENT), event::to_json(&standup).as_bytes()).unwrap();
    let work = Calendar {
        id: WORK.to_string(),
        name: String::from("Work"),
        colour: Colour::Green,
    };
    calendar.put(&calendars::object_key(WORK), calendars::to_json(&work).as_bytes()).unwrap();
    let mut review = Event::create(WORK_EVENT, "Review", day(2026, 10, 2), at(14, 0), at(15, 0), None).unwrap();
    review.calendar = WORK.to_string();
    let review = review.check().unwrap();
    calendar.put(&event::object_key(WORK_EVENT), event::to_json(&review).as_bytes()).unwrap();
    let pim = Pim::new(contacts.clone(), calendar.clone(), Names::in_memory(), ADDRESS);
    Fixture { contacts, calendar, pim }
}

fn ask(pim: &Pim, method: &str, target: &str, headers: &[(&str, &str)], body: &str) -> Response {
    let head = Head {
        method: method.to_string(),
        target: target.to_string(),
        version: (1, 1),
        headers: headers
            .iter()
            .map(|(n, v)| (n.to_string(), v.to_string()))
            .collect(),
    };
    pim.respond(&head, body.as_bytes())
}

fn text(response: &Response) -> String {
    String::from_utf8_lossy(&response.body).into_owned()
}

/// The text between the first `start` and the `end` after it.
fn between(text: &str, start: &str, end: &str) -> String {
    let from = text.find(start).map(|at| at + start.len()).unwrap_or_else(|| panic!("no {start} in {text}"));
    let to = text[from..].find(end).map(|at| from + at).unwrap_or_else(|| panic!("no {end} in {text}"));
    text[from..to].to_string()
}

fn stored(drive: &MemoryDrive, id: &str) -> Event {
    let bytes = drive.get(&event::object_key(id)).unwrap();
    event::from_json(&String::from_utf8(bytes).unwrap()).unwrap()
}

#[test]
fn a_calendar_program_finds_the_principal_and_both_homes_from_the_well_known_address() {
    let f = fixture();
    let moved = ask(&f.pim, "PROPFIND", "/.well-known/caldav", &[("Depth", "0")], "");
    assert_eq!(moved.status.0, 301);
    assert_eq!(moved.header("Location"), Some("/"));
    assert_eq!(ask(&f.pim, "GET", "/.well-known/carddav", &[], "").status.0, 301);
    let root = text(&ask(&f.pim, "PROPFIND", "/", &[("Depth", "0")], PRINCIPAL_PROPS));
    assert!(
        root.contains("<D:current-user-principal><D:href>/principal/</D:href></D:current-user-principal>"),
        "{root}"
    );
    let principal = text(&ask(&f.pim, "PROPFIND", "/principal/", &[("Depth", "0")], PRINCIPAL_PROPS));
    for wanted in [
        "<D:resourcetype><D:collection/><D:principal/></D:resourcetype>",
        "<C:calendar-home-set><D:href>/calendars/</D:href></C:calendar-home-set>",
        "<CR:addressbook-home-set><D:href>/addressbooks/</D:href></CR:addressbook-home-set>",
        "<C:calendar-user-address-set><D:href>mailto:ada@example.org</D:href></C:calendar-user-address-set>",
        "<D:displayname>ada@example.org</D:displayname>",
        // What it does not have is not found, in its own namespace.
        "<x:schedule-inbox-URL xmlns:x=\"urn:ietf:params:xml:ns:caldav\"/>",
    ] {
        assert!(principal.contains(wanted), "{wanted}: {principal}");
    }
    let options = ask(&f.pim, "OPTIONS", "/", &[], "");
    assert!(
        options
            .header("DAV")
            .is_some_and(|dav| dav.contains("calendar-access") && dav.contains("addressbook")),
        "{options:?}"
    );
    let homes = text(&ask(&f.pim, "PROPFIND", "/", &[("Depth", "1")], ""));
    for href in ["/principal/", "/calendars/", "/addressbooks/"] {
        assert!(homes.contains(&format!("<D:href>{href}</D:href>")), "{href}: {homes}");
    }
}

#[test]
fn contacts_are_azcontacts_files_served_and_written_as_they_are() {
    let f = fixture();
    let listing = text(&ask(&f.pim, "PROPFIND", "/addressbooks/contacts/", &[("Depth", "1")], BOOK_PROPS));
    assert!(listing.contains("<D:resourcetype><D:collection/><CR:addressbook/></D:resourcetype>"), "{listing}");
    assert!(listing.contains(&format!("<D:href>/addressbooks/contacts/{CARD}.vcf</D:href>")), "{listing}");
    assert!(!listing.contains("readme") && !listing.contains("old/"), "{listing}");
    let ctag = between(&listing, "<CS:getctag>", "</CS:getctag>");
    let get = ask(
        &f.pim,
        "GET",
        &format!("/addressbooks/contacts/{CARD}.vcf"),
        &[("Accept", "text/vcard; version=4.0")],
        "",
    );
    assert_eq!(get.status, Status::OK);
    assert_eq!(get.body, CARD_TEXT.as_bytes(), "a 4.0 file asked for as 4.0: byte for byte");
    assert_eq!(get.header("Content-Type"), Some(cards::CONTENT_TYPE));
    assert!(get.header("ETag").is_some());

    // A program's vCard 3.0, kept as it came.
    let card = "BEGIN:VCARD\r\nVERSION:3.0\r\nUID:ABCDEF-1234\r\nFN:Ada Lovelace\r\nEND:VCARD\r\n";
    let href = "/addressbooks/contacts/ABCDEF-1234.vcf";
    let made = ask(&f.pim, "PUT", href, &[("If-None-Match", "*")], card);
    assert_eq!(made.status, Status::CREATED, "{}", text(&made));
    let etag = made.header("ETag").expect("kept as it came: its version").to_string();
    assert_eq!(f.contacts.get("contacts/ABCDEF-1234.vcf").unwrap(), card.as_bytes());
    assert_eq!(
        ask(&f.pim, "PUT", href, &[("If-None-Match", "*")], card).status,
        Status::PRECONDITION_FAILED,
        "a new card's name is taken"
    );
    let changed = card.replace("Ada Lovelace", "Ada King");
    assert_eq!(ask(&f.pim, "PUT", href, &[("If-Match", "\"stale\"")], &changed).status, Status::PRECONDITION_FAILED);
    assert_eq!(ask(&f.pim, "PUT", href, &[("If-Match", etag.as_str())], &changed).status, Status::NO_CONTENT);

    let multiget = format!(
        "<?xml version=\"1.0\"?><CR:addressbook-multiget xmlns:D=\"DAV:\" xmlns:CR=\"urn:ietf:params:xml:ns:carddav\">\
         <D:prop><D:getetag/><CR:address-data/></D:prop><D:href>{href}</D:href>\
         <D:href>/addressbooks/contacts/{CARD}.vcf</D:href><D:href>/addressbooks/contacts/nobody.vcf</D:href>\
         </CR:addressbook-multiget>"
    );
    let report = ask(&f.pim, "REPORT", "/addressbooks/contacts/", &[("Depth", "1")], &multiget);
    assert_eq!(report.status, Status::MULTI_STATUS);
    let report = text(&report);
    assert!(report.contains("FN:Ada King") && report.contains("FN:Grace Hopper"), "{report}");
    assert!(
        report.contains(
            "<D:response><D:href>/addressbooks/contacts/nobody.vcf</D:href><D:status>HTTP/1.1 404 Not Found</D:status>"
        ),
        "{report}"
    );
    let after = text(&ask(&f.pim, "PROPFIND", "/addressbooks/contacts/", &[("Depth", "0")], BOOK_PROPS));
    assert_ne!(between(&after, "<CS:getctag>", "</CS:getctag>"), ctag, "a change shows in the CTag");
    let query = "<CR:addressbook-query xmlns:D=\"DAV:\" xmlns:CR=\"urn:ietf:params:xml:ns:carddav\">\
         <D:prop><D:getetag/></D:prop></CR:addressbook-query>";
    let every = text(&ask(&f.pim, "REPORT", "/addressbooks/contacts/", &[("Depth", "1")], query));
    assert_eq!(every.matches("<D:response>").count(), 2, "{every}");

    let refused = ask(&f.pim, "PUT", "/addressbooks/contacts/x.vcf", &[], "hello");
    assert_eq!(refused.status, Status::FORBIDDEN);
    assert!(text(&refused).contains("<CR:valid-address-data/>"));
    let current = ask(&f.pim, "GET", href, &[], "").header("ETag").unwrap().to_string();
    assert_eq!(ask(&f.pim, "DELETE", href, &[("If-Match", "\"stale\"")], "").status, Status::PRECONDITION_FAILED);
    assert_eq!(ask(&f.pim, "DELETE", href, &[("If-Match", current.as_str())], "").status, Status::NO_CONTENT);
    assert!(f.contacts.head("contacts/ABCDEF-1234.vcf").is_err());
    assert_eq!(ask(&f.pim, "GET", href, &[], "").status, Status::NOT_FOUND);
}

/// AzContacts writes vCard 4.0; Apple's Contacts reads 3.0. A card is served in 3.0 unless the
/// program asks for 4.0 (`Accept`, or address-data's `version` in a REPORT), written by
/// AzContacts' own model (azul-contacts-core) with the file's UID; a 3.0 card a program put is
/// served as it came.
#[test]
fn a_vcard_4_file_is_served_as_3_0_unless_the_program_asks_for_4_0() {
    let f = fixture();
    let href = format!("/addressbooks/contacts/{CARD}.vcf");
    let plain = ask(&f.pim, "GET", &href, &[], "");
    assert_eq!(plain.status, Status::OK);
    let plain = text(&plain);
    assert!(plain.contains("VERSION:3.0") && !plain.contains("VERSION:4.0"), "{plain}");
    assert!(plain.contains("FN:Grace Hopper") && plain.contains("grace@example.org"), "{plain}");
    assert!(plain.contains(&format!("UID:{CARD}")), "{plain}");
    let four = ask(&f.pim, "GET", &href, &[("Accept", "text/vcard; version=4.0")], "");
    assert_eq!(four.body, CARD_TEXT.as_bytes(), "as the file is");

    let multiget = |version: &str| {
        format!(
            "<CR:addressbook-multiget xmlns:D=\"DAV:\" xmlns:CR=\"urn:ietf:params:xml:ns:carddav\">\
             <D:prop><D:getetag/><CR:address-data content-type=\"text/vcard\" version=\"{version}\"/></D:prop>\
             <D:href>{href}</D:href></CR:addressbook-multiget>"
        )
    };
    let three = text(&ask(&f.pim, "REPORT", "/addressbooks/contacts/", &[], &multiget("3.0")));
    assert!(three.contains("VERSION:3.0") && three.contains("FN:Grace Hopper"), "{three}");
    let four = text(&ask(&f.pim, "REPORT", "/addressbooks/contacts/", &[], &multiget("4.0")));
    assert!(four.contains("VERSION:4.0"), "{four}");

    let apple = "BEGIN:VCARD\r\nVERSION:3.0\r\nUID:A1B2-C3\r\nFN:Ada Lovelace\r\nEND:VCARD\r\n";
    assert_eq!(ask(&f.pim, "PUT", "/addressbooks/contacts/A1B2-C3.vcf", &[], apple).status, Status::CREATED);
    assert_eq!(ask(&f.pim, "GET", "/addressbooks/contacts/A1B2-C3.vcf", &[], "").body, apple.as_bytes());
}

#[test]
fn a_card_name_that_cannot_be_a_file_name_is_kept_and_found_again() {
    let dir = azul_storage::testing::TempDir::new("pim-names");
    let contacts = Arc::new(MemoryDrive::new());
    let calendar = Arc::new(MemoryDrive::new());
    let path = dir.0.join(NAMES_FILE);
    let pim = Pim::new(contacts.clone(), calendar.clone(), Names::in_file(path.clone()), ADDRESS);
    let href = "/addressbooks/contacts/grace%40navy.vcf";
    let card = "BEGIN:VCARD\r\nVERSION:3.0\r\nFN:Grace\r\nEND:VCARD\r\n";
    assert_eq!(ask(&pim, "PUT", href, &[], card).status, Status::CREATED);
    let files = azul_storage::ops::list_all(&*contacts, "contacts/").unwrap();
    assert_eq!(files.len(), 1, "{files:?}");
    let uid = cards::uid_of_key(&files[0].key).expect("a file AzContacts reads");
    let listing = text(&ask(&pim, "PROPFIND", "/addressbooks/contacts/", &[("Depth", "1")], ""));
    assert!(listing.contains(&format!("<D:href>{href}</D:href>")), "{listing}");
    assert!(!listing.contains(&uid), "the program sees its own name: {listing}");

    // The name outlives the bridge.
    let again = Pim::new(contacts.clone(), calendar, Names::in_file(path), ADDRESS);
    assert_eq!(ask(&again, "GET", href, &[], "").body, card.as_bytes());
    assert_eq!(ask(&again, "DELETE", href, &[], "").status, Status::NO_CONTENT);
    assert!(azul_storage::ops::list_all(&*contacts, "contacts/").unwrap().is_empty());
    assert_eq!(again.names.id_of(Kind::Contact, "grace@navy"), None);
}

#[test]
fn events_are_azcalendar_files_served_as_its_icalendar_export() {
    let f = fixture();
    let home = text(&ask(&f.pim, "PROPFIND", "/calendars/", &[("Depth", "1")], CALENDAR_PROPS));
    assert!(home.contains("<D:href>/calendars/default/</D:href>"), "{home}");
    assert!(home.contains(&format!("<D:href>/calendars/{WORK}/</D:href>")), "{home}");
    for wanted in [
        "<D:displayname>Calendar</D:displayname>",
        "<D:displayname>Work</D:displayname>",
        "<D:resourcetype><D:collection/><C:calendar/></D:resourcetype>",
        "<A:calendar-color>#3A8A3AFF</A:calendar-color>",
        "<C:supported-calendar-component-set><C:comp name=\"VEVENT\"/></C:supported-calendar-component-set>",
        "<D:privilege><D:write/></D:privilege>",
        "<CS:getctag>",
    ] {
        assert!(home.contains(wanted), "{wanted}: {home}");
    }
    let default = text(&ask(&f.pim, "PROPFIND", "/calendars/default/", &[("Depth", "1")], CALENDAR_PROPS));
    assert!(default.contains(&format!("<D:href>/calendars/default/{EVENT}.ics</D:href>")), "{default}");
    assert!(!default.contains(WORK_EVENT), "Work's event is in Work: {default}");

    let get = ask(&f.pim, "GET", &format!("/calendars/default/{EVENT}.ics"), &[], "");
    assert_eq!(get.status, Status::OK);
    assert_eq!(get.header("Content-Type"), Some(events::CONTENT_TYPE));
    assert!(get.header("ETag").is_some());
    let ics = text(&get);
    for wanted in [
        format!("UID:{EVENT}@azcalendar"),
        String::from("SUMMARY:Standup"),
        String::from("DTSTART:20261001T090000"),
        String::from("X-WR-CALNAME:Calendar"),
    ] {
        assert!(ics.contains(&wanted), "{wanted}: {ics}");
    }
    assert!(!ics.contains("METHOD:"), "a calendar's resource has no METHOD: {ics}");
    assert_eq!(
        ask(&f.pim, "GET", &format!("/calendars/default/{WORK_EVENT}.ics"), &[], "").status,
        Status::NOT_FOUND
    );
    assert_eq!(ask(&f.pim, "GET", &format!("/calendars/{WORK}/{WORK_EVENT}.ics"), &[], "").status, Status::OK);

    let multiget = format!(
        "<C:calendar-multiget xmlns:D=\"DAV:\" xmlns:C=\"urn:ietf:params:xml:ns:caldav\"><D:prop><D:getetag/>\
         <C:calendar-data/></D:prop><D:href>/calendars/default/{EVENT}.ics</D:href>\
         <D:href>/calendars/default/{WORK_EVENT}.ics</D:href></C:calendar-multiget>"
    );
    let report = text(&ask(&f.pim, "REPORT", "/calendars/default/", &[("Depth", "1")], &multiget));
    assert!(report.contains("<C:calendar-data>") && report.contains("SUMMARY:Standup"), "{report}");
    assert!(
        report.contains(&format!(
            "<D:href>/calendars/default/{WORK_EVENT}.ics</D:href><D:status>HTTP/1.1 404 Not Found</D:status>"
        )),
        "{report}"
    );
    let query = |component: &str, start: &str, end: &str| {
        format!(
            "<C:calendar-query xmlns:D=\"DAV:\" xmlns:C=\"urn:ietf:params:xml:ns:caldav\"><D:prop><D:getetag/></D:prop>\
             <C:filter><C:comp-filter name=\"VCALENDAR\"><C:comp-filter name=\"{component}\">\
             <C:time-range start=\"{start}\" end=\"{end}\"/></C:comp-filter></C:comp-filter></C:filter></C:calendar-query>"
        )
    };
    let found = |body: String| {
        text(&ask(&f.pim, "REPORT", "/calendars/default/", &[("Depth", "1")], &body))
            .matches("<D:response>")
            .count()
    };
    assert_eq!(found(query("VEVENT", "20260928T000000Z", "20261005T000000Z")), 1);
    assert_eq!(found(query("VEVENT", "20261101T000000Z", "20261201T000000Z")), 0, "outside the range");
    assert_eq!(found(query("VTODO", "20260928T000000Z", "20261005T000000Z")), 0, "events only");
}

#[test]
fn a_calendar_program_puts_changes_and_deletes_an_event_under_its_own_name() {
    let f = fixture();
    let href = "/calendars/default/9F3E2B1A-UPPER.ics";
    let made = ask(&f.pim, "PUT", href, &[("If-None-Match", "*"), ("Content-Type", "text/calendar")], DENTIST);
    assert_eq!(made.status, Status::CREATED, "{}", text(&made));
    assert_eq!(made.header("ETag"), None, "the file is AzCalendar's: the program fetches what it became");
    let id = f
        .pim
        .names
        .id_of(Kind::Event, "9F3E2B1A-UPPER")
        .expect("the program's name is kept");
    assert!(event::is_event_id(&id), "{id}");
    let dentist = stored(&f.calendar, &id);
    assert_eq!(
        (dentist.title.as_str(), dentist.uid.as_str(), dentist.calendar.as_str()),
        ("Dentist", "dentist@thunderbird", "")
    );
    assert_eq!((dentist.date, dentist.start, dentist.end), (day(2026, 10, 2), at(10, 0), at(11, 0)));
    let listing = text(&ask(&f.pim, "PROPFIND", "/calendars/default/", &[("Depth", "1")], CALENDAR_PROPS));
    assert!(listing.contains(&format!("<D:href>{href}</D:href>")), "{listing}");
    assert!(!listing.contains(&id), "the program sees its own name: {listing}");
    let get = ask(&f.pim, "GET", href, &[], "");
    assert!(text(&get).contains("UID:dentist@thunderbird"));
    let etag = get.header("ETag").unwrap().to_string();

    let later = DENTIST.replace("SUMMARY:Dentist", "SUMMARY:Dentist (moved)");
    assert_eq!(ask(&f.pim, "PUT", href, &[("If-Match", "\"stale\"")], &later).status, Status::PRECONDITION_FAILED);
    assert_eq!(ask(&f.pim, "PUT", href, &[("If-Match", etag.as_str())], &later).status, Status::NO_CONTENT);
    assert_eq!(stored(&f.calendar, &id).title, "Dentist (moved)");
    assert_eq!(
        ask(&f.pim, "PUT", "/calendars/9d4c1f3a-0000-4000-8000-000000000000/x.ics", &[], DENTIST).status,
        Status::CONFLICT,
        "no such calendar"
    );
    let refused = ask(&f.pim, "PUT", "/calendars/default/y.ics", &[], "not a calendar");
    assert_eq!(refused.status, Status::FORBIDDEN);
    assert!(text(&refused).contains("<C:valid-calendar-data/>"));

    assert_eq!(ask(&f.pim, "DELETE", href, &[], "").status, Status::NO_CONTENT);
    assert!(f.calendar.head(&event::object_key(&id)).is_err());
    assert_eq!(f.pim.names.id_of(Kind::Event, "9F3E2B1A-UPPER"), None);
}

#[test]
fn an_occurrence_a_program_moved_is_an_event_of_its_own_that_sending_again_replaces() {
    let f = fixture();
    let href = "/calendars/default/yoga.ics";
    assert_eq!(ask(&f.pim, "PUT", href, &[], WEEKLY).status, Status::CREATED);
    let yoga = || -> Vec<Event> {
        event::load(&*f.calendar)
            .0
            .into_iter()
            .filter(|e| e.uid.starts_with("yoga@apple"))
            .collect()
    };
    let first = yoga();
    assert_eq!(first.len(), 2, "{first:?}");
    let master = first.iter().find(|e| e.uid == "yoga@apple").expect("the weekly event");
    assert_eq!(master.except, vec![day(2026, 10, 8)]);
    let friday = first
        .iter()
        .find(|e| e.uid == "yoga@apple#20261008")
        .expect("the moved occurrence, as AzCalendar's File > Open makes it");
    assert_eq!((friday.date, friday.title.as_str()), (day(2026, 10, 9), "Yoga (Friday)"));
    assert_eq!(ask(&f.pim, "PUT", href, &[], WEEKLY).status, Status::NO_CONTENT);
    assert_eq!(yoga().len(), 2, "sending it again replaces the moved occurrence");
    assert_eq!(ask(&f.pim, "DELETE", href, &[], "").status, Status::NO_CONTENT);
    assert!(yoga().is_empty(), "the moved occurrence goes with its event");
}

#[test]
fn the_meeting_azcalendar_registered_survives_a_programs_edit() {
    let f = fixture();
    let meeting = Meeting {
        link: format!("azlin://meet/{ROOM}"),
        server: String::from("https://meet.example.com"),
        code: String::from("482913"),
        expires: String::from("2026-10-06T12:00:00Z"),
        starts_at: String::new(),
        ends_at: String::new(),
        pending: false,
    };
    let call = Event::create(MEETING_EVENT, "Planning", day(2026, 10, 5), at(11, 0), at(12, 0), Some(meeting)).unwrap();
    f.calendar
        .put(&event::object_key(MEETING_EVENT), event::to_json(&call).as_bytes())
        .unwrap();
    let href = format!("/calendars/default/{MEETING_EVENT}.ics");
    let ics = text(&ask(&f.pim, "GET", &href, &[], ""));
    assert!(ics.contains(&format!("X-AZCAL-MEETING:azlin://meet/{ROOM}")), "{ics}");
    let edited = ics.replace("SUMMARY:Planning", "SUMMARY:Planning Q4");
    assert_eq!(ask(&f.pim, "PUT", &href, &[], &edited).status, Status::NO_CONTENT);
    let planning = stored(&f.calendar, MEETING_EVENT);
    assert_eq!(planning.title, "Planning Q4");
    assert_eq!(planning.meeting.map(|m| m.code), Some(String::from("482913")));
}

/// Apple Calendar's "New Calendar" on the account: MKCALENDAR makes an AzCalendar calendar (its
/// name, the AzCalendar colour nearest the program's), found again under the program's own path;
/// PROPPATCH renames and recolours it; events go into it.
#[test]
fn mkcalendar_makes_an_azcalendar_calendar_under_the_programs_path() {
    let f = fixture();
    let body = "<?xml version=\"1.0\"?><C:mkcalendar xmlns:D=\"DAV:\" xmlns:C=\"urn:ietf:params:xml:ns:caldav\" \
         xmlns:A=\"http://apple.com/ns/ical/\"><D:set><D:prop><D:displayname>Holidays</D:displayname>\
         <A:calendar-color>#C4691AFF</A:calendar-color></D:prop></D:set></C:mkcalendar>";
    let made = ask(&f.pim, "MKCALENDAR", "/calendars/7D3A51C2-HOLIDAYS/", &[], body);
    assert_eq!(made.status, Status::CREATED, "{}", text(&made));
    let all = calendars::load(&*f.calendar);
    let holidays = all
        .iter()
        .find(|c| c.name == "Holidays")
        .expect("a calendar file AzCalendar reads")
        .clone();
    assert_eq!(holidays.colour, Colour::Orange, "the colour nearest #C4691A");
    let home = text(&ask(&f.pim, "PROPFIND", "/calendars/", &[("Depth", "1")], CALENDAR_PROPS));
    assert!(home.contains("<D:href>/calendars/7D3A51C2-HOLIDAYS/</D:href>"), "{home}");
    assert!(!home.contains(&format!("/calendars/{}/", holidays.id)), "{home}");
    assert_eq!(
        ask(&f.pim, "MKCALENDAR", "/calendars/7D3A51C2-HOLIDAYS/", &[], body).status,
        Status::METHOD_NOT_ALLOWED
    );

    assert_eq!(ask(&f.pim, "PUT", "/calendars/7D3A51C2-HOLIDAYS/beach.ics", &[], DENTIST).status, Status::CREATED);
    let id = f.pim.names.id_of(Kind::Event, "beach").expect("the program's name");
    assert_eq!(stored(&f.calendar, &id).calendar, holidays.id);

    let patch = "<D:propertyupdate xmlns:D=\"DAV:\" xmlns:A=\"http://apple.com/ns/ical/\"><D:set><D:prop>\
         <D:displayname>Vacation</D:displayname><A:calendar-color>#3A8A3AFF</A:calendar-color>\
         </D:prop></D:set></D:propertyupdate>";
    let patched = text(&ask(&f.pim, "PROPPATCH", "/calendars/7D3A51C2-HOLIDAYS/", &[], patch));
    assert!(patched.contains("HTTP/1.1 200 OK") && !patched.contains("403"), "{patched}");
    let renamed = calendars::load(&*f.calendar)
        .into_iter()
        .find(|c| c.id == holidays.id)
        .expect("the same calendar");
    assert_eq!((renamed.name.as_str(), renamed.colour), ("Vacation", Colour::Green));
}

#[test]
fn what_the_bridge_does_not_do_is_refused_plainly() {
    let f = fixture();
    let sync = "<D:sync-collection xmlns:D=\"DAV:\"><D:sync-token/><D:prop><D:getetag/></D:prop></D:sync-collection>";
    let refused = ask(&f.pim, "REPORT", "/addressbooks/contacts/", &[], sync);
    assert_eq!(refused.status, Status::FORBIDDEN);
    assert!(text(&refused).contains("<D:supported-report/>"));
    // A property no calendar keeps (a dead one) is refused; the name and the colour are not.
    let patch = "<D:propertyupdate xmlns:D=\"DAV:\" xmlns:Z=\"urn:example:dead\"><D:set><D:prop>\
         <Z:note>kept nowhere</Z:note></D:prop></D:set></D:propertyupdate>";
    let patched = text(&ask(&f.pim, "PROPPATCH", "/calendars/default/", &[], patch));
    assert!(
        patched.contains("<x:note xmlns:x=\"urn:example:dead\"/>") && patched.contains("403 Forbidden"),
        "{patched}"
    );
    assert_eq!(
        ask(&f.pim, "MKCALENDAR", "/calendars/default/", &[], "").status,
        Status::METHOD_NOT_ALLOWED,
        "a calendar is there"
    );
    assert_eq!(ask(&f.pim, "MKCOL", "/addressbooks/second/", &[], "").status, Status::FORBIDDEN);
    assert_eq!(ask(&f.pim, "PROPFIND", "/calendars/", &[], "").status, Status::FORBIDDEN, "no Depth is infinity");
    assert_eq!(ask(&f.pim, "PROPFIND", "/elsewhere/", &[("Depth", "0")], "").status, Status::NOT_FOUND);
    assert_eq!(ask(&f.pim, "DELETE", "/calendars/default/", &[], "").status, Status::FORBIDDEN);
    assert_eq!(place_of("/calendars/../principal/"), Err(Status::BAD_REQUEST));
    assert_eq!(
        place_of(&format!("/addressbooks/contacts/{}.vcf", "a".repeat(300))),
        Err(Status::URI_TOO_LONG)
    );
    assert_eq!(place_of("/addressbooks/contacts/.vcf"), Ok(None));
    assert_eq!(
        place_of("/calendars/default/x.ics"),
        Ok(Some(Place::Event(String::from("default"), String::from("x"))))
    );
}
