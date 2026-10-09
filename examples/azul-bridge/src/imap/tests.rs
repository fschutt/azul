//! Scripted IMAP sessions over a real socket against the drive's mailbox in memory: what a
//! mail program sends, what the bridge answers, what lands in the drive.

use std::{
    io::Write,
    net::TcpStream,
    sync::Arc,
    time::Duration,
};

use azmail_core::azlin;
use azul_storage::Drive;

use super::*;
use crate::{
    auth::{Credentials, FailureGate},
    limits::Limits,
    memory::MemoryDrive,
    net::{bind_loopback, Input},
    sent::SentRegistry,
    store::DriveMailStore,
    uids::UidMaps,
};

const USER: &str = "ada@example.org";
const PASSWORD: &str = "k7m2p-9qxat-4ds8w-hb3zn-e6r1v";
/// 2026-10-01T08:30:00Z
const OCT_1: u64 = 1_790_843_400;

fn mail(subject: &str, body: &str) -> Vec<u8> {
    format!(
        "From: Ben <ben@example.net>\r\nTo: Ada <ada@example.org>\r\nSubject: {subject}\r\n\
         Message-ID: <{subject}@example.net>\r\nDate: Thu, 01 Oct 2026 08:00:00 +0000\r\n\r\n{body}\r\n"
    )
    .into_bytes()
}

/// A drive with two messages in the Inbox (the first one read), one in Junk, one in a
/// nested folder and a folder with a non-ASCII name.
fn seeded() -> Arc<MemoryDrive> {
    let drive = Arc::new(MemoryDrive::new());
    drive.set_now(OCT_1);
    let put = |folder: &str, bytes: &[u8], stamp: u64| {
        let name = azlin::object_name(bytes, stamp);
        drive.put(&azlin::message_key(folder, &name), bytes).unwrap();
        name.trim_end_matches(".eml").to_string()
    };
    let first = put("Inbox", &mail("lunch", "See you at noon."), OCT_1);
    put("Inbox", &mail("plans", "The garden plan is attached."), OCT_1 + 60);
    put("Junk", &mail("prize", "You won!"), OCT_1 + 120);
    put("Work/Projects", &mail("kickoff", "Monday 9:00."), OCT_1 + 180);
    put("Entwürfe", &mail("draft", "Half written."), OCT_1 + 240);
    drive.put(&azlin::marker_key(&first, azlin::SEEN), &[]).unwrap();
    drive
}

fn bridge_with(drive: Arc<MemoryDrive>, limits: Limits) -> Arc<Imap> {
    Arc::new(Imap::new(
        Arc::new(DriveMailStore::new(drive)),
        Arc::new(UidMaps::in_memory().with_clock(|| 1_700_000_000)),
        Credentials::new(USER, PASSWORD),
        Arc::new(FailureGate::new(100, Duration::from_secs(60), Duration::ZERO)),
        limits,
        Arc::new(SentRegistry::new()),
    ))
}

fn bridge(drive: Arc<MemoryDrive>) -> Arc<Imap> {
    bridge_with(drive, Limits::default())
}

/// A mail program's end of one connection.
struct Client {
    stream: TcpStream,
    input: Input,
    greeting: String,
}

impl Client {
    fn connect(imap: Arc<Imap>) -> Client {
        let listener = bind_loopback(0).unwrap();
        let addr = listener.local_addr().unwrap();
        std::thread::spawn(move || {
            if let Ok((stream, _)) = listener.accept() {
                imap.handle(stream);
            }
        });
        let stream = TcpStream::connect(addr).unwrap();
        stream
            .set_read_timeout(Some(Duration::from_secs(10)))
            .unwrap();
        let mut client = Client {
            stream,
            input: Input::new(),
            greeting: String::new(),
        };
        client.greeting = client.response();
        client
    }

    fn signed_in(imap: Arc<Imap>) -> Client {
        let mut client = Client::connect(imap);
        let answer = client.run("L", &format!("LOGIN {USER} \"{PASSWORD}\""));
        assert!(answer.last().unwrap().starts_with("L OK"), "{answer:?}");
        client
    }

    fn send(&mut self, bytes: &[u8]) {
        self.stream.write_all(bytes).unwrap();
    }

    /// One response line, the literals inside it read in.
    fn response(&mut self) -> String {
        let mut text = String::new();
        loop {
            let line = self.input.read_line(&mut self.stream, 1 << 20).expect("a line");
            let line = String::from_utf8_lossy(&line).into_owned();
            text.push_str(&line);
            match crate::imap::parse::literal_at_end(line.as_bytes()) {
                Some((n, _)) => {
                    let bytes = self.input.read_bytes(&mut self.stream, n).unwrap();
                    text.push_str(&String::from_utf8_lossy(&bytes));
                }
                None => return text,
            }
        }
    }

    /// Sends `tag command` and reads until the tagged answer; every response, in order.
    fn run(&mut self, tag: &str, command: &str) -> Vec<String> {
        self.send(format!("{tag} {command}\r\n").as_bytes());
        self.until(tag)
    }

    fn until(&mut self, tag: &str) -> Vec<String> {
        let mut lines = Vec::new();
        loop {
            let line = self.response();
            let done = line.starts_with(&format!("{tag} "));
            lines.push(line);
            if done {
                return lines;
            }
        }
    }

    fn ok(&mut self, tag: &str, command: &str) -> Vec<String> {
        let lines = self.run(tag, command);
        assert!(
            lines.last().unwrap().starts_with(&format!("{tag} OK")),
            "{command}: {lines:?}"
        );
        lines
    }
}

fn has(lines: &[String], needle: &str) -> bool {
    lines.iter().any(|l| l.contains(needle))
}

#[test]
fn the_greeting_offers_sign_in_and_nothing_works_before_it() {
    let mut client = Client::connect(bridge(seeded()));
    assert!(client.greeting.starts_with("* OK [CAPABILITY IMAP4rev1"), "{}", client.greeting);
    assert!(client.greeting.contains("AUTH=PLAIN"));
    let caps = client.ok("a1", "CAPABILITY");
    assert!(has(&caps, "IDLE") && has(&caps, "UIDPLUS") && has(&caps, "MOVE"));
    assert!(client.run("a2", "SELECT INBOX").last().unwrap().starts_with("a2 BAD"));
    assert!(client.run("a3", "STARTTLS").last().unwrap().starts_with("a3 NO"));
    let wrong = client.run("a4", "LOGIN ada@example.org wrong");
    assert!(wrong.last().unwrap().starts_with("a4 NO [AUTHENTICATIONFAILED]"), "{wrong:?}");
    let right = client.run("a5", &format!("LOGIN ADA@example.org {PASSWORD}"));
    assert!(right.last().unwrap().starts_with("a5 OK [CAPABILITY"), "{right:?}");
    assert!(!right.last().unwrap().contains("AUTH=PLAIN"));
    assert!(client.run("a6", "LOGIN x y").last().unwrap().starts_with("a6 BAD"));
    let bye = client.run("a7", "LOGOUT");
    assert!(has(&bye, "* BYE") && bye.last().unwrap().starts_with("a7 OK"));
}

#[test]
fn authenticate_plain_and_login_sign_in_with_or_without_an_initial_response() {
    let imap = bridge(seeded());
    let plain = crate::auth::encode_base64(format!("\0{USER}\0{PASSWORD}").as_bytes());
    let mut client = Client::connect(imap.clone());
    client.ok("p1", &format!("AUTHENTICATE PLAIN {plain}"));

    let mut client = Client::connect(imap.clone());
    client.send(b"p2 AUTHENTICATE PLAIN\r\n");
    assert_eq!(client.response(), "+ ");
    client.send(format!("{plain}\r\n").as_bytes());
    assert!(client.until("p2").last().unwrap().starts_with("p2 OK"));

    let mut client = Client::connect(imap.clone());
    client.send(b"p3 AUTHENTICATE LOGIN\r\n");
    assert_eq!(client.response(), "+ VXNlcm5hbWU6");
    client.send(format!("{}\r\n", crate::auth::encode_base64(USER.as_bytes())).as_bytes());
    assert_eq!(client.response(), "+ UGFzc3dvcmQ6");
    client.send(format!("{}\r\n", crate::auth::encode_base64(PASSWORD.as_bytes())).as_bytes());
    assert!(client.until("p3").last().unwrap().starts_with("p3 OK"));

    let mut client = Client::connect(imap);
    client.send(b"p4 AUTHENTICATE PLAIN\r\n");
    client.response();
    client.send(b"*\r\n");
    assert!(client.until("p4").last().unwrap().starts_with("p4 BAD"));
}

#[test]
fn three_wrong_passwords_end_the_connection() {
    let mut client = Client::connect(bridge(seeded()));
    client.run("w1", "LOGIN ada@example.org a");
    client.run("w2", "LOGIN ada@example.org b");
    let last = client.run("w3", "LOGIN ada@example.org c");
    assert!(last.last().unwrap().starts_with("w3 NO"));
    assert!(client.response().starts_with("* BYE"));
}

#[test]
fn list_shows_the_inbox_the_special_use_folders_and_the_users_own() {
    let mut client = Client::signed_in(bridge(seeded()));
    let lines = client.ok("l1", "LIST \"\" \"*\"");
    assert!(has(&lines, "(\\HasNoChildren) \"/\" \"INBOX\""), "{lines:?}");
    assert!(has(&lines, "\\Junk) \"/\" \"Junk\""), "{lines:?}");
    assert!(has(&lines, "\\Sent) \"/\" \"Sent\""), "{lines:?}");
    assert!(has(&lines, "\\Trash) \"/\" \"Trash\""), "{lines:?}");
    assert!(has(&lines, "(\\HasChildren) \"/\" \"Work\""), "{lines:?}");
    assert!(has(&lines, "\"Work/Projects\""), "{lines:?}");
    assert!(has(&lines, "\"Entw&APw-rfe\""), "{lines:?}");
    let top = client.ok("l2", "LIST \"\" %");
    assert!(!has(&top, "Work/Projects") && has(&top, "\"Work\""), "{top:?}");
    let root = client.ok("l3", "LIST \"\" \"\"");
    assert!(has(&root, "* LIST (\\Noselect) \"/\" \"\""), "{root:?}");
    let inbox = client.ok("l4", "LSUB \"\" inbox");
    assert!(has(&inbox, "* LSUB") && has(&inbox, "\"INBOX\""), "{inbox:?}");
}

#[test]
fn select_reports_the_messages_their_uids_and_the_first_unseen() {
    let mut client = Client::signed_in(bridge(seeded()));
    let lines = client.ok("s1", "SELECT INBOX");
    assert!(has(&lines, "* 2 EXISTS"), "{lines:?}");
    assert!(has(&lines, "* OK [UNSEEN 2]"), "{lines:?}");
    assert!(has(&lines, "* OK [UIDVALIDITY 1700000000]"), "{lines:?}");
    assert!(has(&lines, "* OK [UIDNEXT 3]"), "{lines:?}");
    assert!(lines.last().unwrap().contains("[READ-WRITE]"));
    let fetched = client.ok("s2", "FETCH 1:* (UID FLAGS RFC822.SIZE INTERNALDATE)");
    assert!(has(&fetched, "* 1 FETCH (UID 1 FLAGS (\\Seen) RFC822.SIZE "), "{fetched:?}");
    assert!(has(&fetched, "INTERNALDATE \" 1-Oct-2026 08:30:00 +0000\""), "{fetched:?}");
    assert!(has(&fetched, "* 2 FETCH (UID 2 FLAGS () RFC822.SIZE"), "{fetched:?}");
    let entwurf = client.ok("s3", "EXAMINE \"Entw&APw-rfe\"");
    assert!(has(&entwurf, "* 1 EXISTS") && entwurf.last().unwrap().contains("[READ-ONLY]"));
    let missing = client.run("s4", "SELECT Nowhere");
    assert!(missing.last().unwrap().starts_with("s4 NO [NONEXISTENT]"));
    // A failed SELECT leaves nothing selected.
    assert!(client.run("s5", "FETCH 1 FLAGS").last().unwrap().starts_with("s5 BAD"));
}

#[test]
fn fetch_gives_envelopes_structures_and_sections_and_only_body_marks_read() {
    let drive = seeded();
    let mut client = Client::signed_in(bridge(drive.clone()));
    client.ok("f0", "SELECT INBOX");
    let lines = client.ok("f1", "UID FETCH 2 (ENVELOPE BODYSTRUCTURE BODY.PEEK[HEADER.FIELDS (SUBJECT)])");
    let text = lines.join("\n");
    assert!(text.contains("* 2 FETCH (UID 2 ENVELOPE (\"Thu, 01 Oct 2026 08:00:00 +0000\" \"plans\""), "{text}");
    assert!(text.contains("((\"Ben\" NIL \"ben\" \"example.net\"))"), "{text}");
    assert!(text.contains("BODYSTRUCTURE (\"TEXT\" \"PLAIN\""), "{text}");
    // (The client helper strings a literal's bytes right after its `{n}`.)
    assert!(text.contains("BODY[HEADER.FIELDS (SUBJECT)] {18}Subject: plans\r\n\r\n"), "{text}");
    assert!(!text.contains("FLAGS"), "a PEEK changes no flag: {text}");
    let seen_markers = |drive: &MemoryDrive| {
        drive
            .keys()
            .into_iter()
            .filter(|k| k.starts_with(azlin::STATE_PREFIX) && k.ends_with("/seen"))
            .count()
    };
    assert_eq!(seen_markers(&drive), 1, "{:?}", drive.keys());
    let read = client.ok("f2", "FETCH 2 (BODY[TEXT] BODY[]<0.4>)");
    let text = read.join("\n");
    assert!(text.contains("FLAGS (\\Seen)"), "{text}");
    assert!(text.contains("BODY[TEXT] {30}The garden plan is attached.\r\n"), "{text}");
    assert!(text.contains("BODY[]<0> {4}From"), "{text}");
    assert_eq!(seen_markers(&drive), 2, "{:?}", drive.keys());
    let missing = client.ok("f3", "FETCH 1 BODY[3]");
    assert!(has(&missing, "BODY[3] {0}"), "{missing:?}");
}

#[test]
fn store_writes_the_shared_marks_and_keeps_deleted_until_expunge() {
    let drive = seeded();
    let mut client = Client::signed_in(bridge(drive.clone()));
    client.ok("t0", "SELECT INBOX");
    let lines = client.ok("t1", "STORE 2 +FLAGS (\\Flagged \\Answered $Work)");
    assert!(has(&lines, "* 2 FETCH (FLAGS (\\Answered \\Flagged $Work))"), "{lines:?}");
    assert_eq!(
        drive.keys().iter().filter(|k| k.ends_with("/flagged") || k.ends_with("/answered")).count(),
        2
    );
    let lines = client.ok("t2", "UID STORE 1 -FLAGS.SILENT (\\Seen)");
    assert!(!has(&lines, "FETCH"), "{lines:?}");
    assert!(!drive.keys().iter().any(|k| k.ends_with("/seen")));
    client.ok("t3", "STORE 1 +FLAGS (\\Deleted)");
    assert_eq!(drive.keys().iter().filter(|k| k.starts_with("mail/Inbox/")).count(), 2);
    let gone = client.ok("t4", "EXPUNGE");
    assert!(has(&gone, "* 1 EXPUNGE"), "{gone:?}");
    assert_eq!(drive.keys().iter().filter(|k| k.starts_with("mail/Inbox/")).count(), 1);
    let left = client.ok("t5", "FETCH 1:* (UID FLAGS)");
    assert!(has(&left, "* 1 FETCH (UID 2 FLAGS (\\Answered \\Flagged $Work))"), "{left:?}");
    client.ok("t6", "EXAMINE INBOX");
    let refused = client.run("t7", "STORE 1 +FLAGS (\\Seen)");
    assert!(refused.last().unwrap().starts_with("t7 NO [READ-ONLY]"));
}

#[test]
fn search_finds_by_flags_headers_text_dates_and_uids() {
    let mut client = Client::signed_in(bridge(seeded()));
    client.ok("q0", "SELECT INBOX");
    assert!(has(&client.ok("q1", "SEARCH UNSEEN"), "* SEARCH 2"));
    assert!(has(&client.ok("q2", "UID SEARCH SEEN"), "* SEARCH 1"));
    assert!(has(&client.ok("q3", "SEARCH SUBJECT LUNCH"), "* SEARCH 1"));
    assert!(has(&client.ok("q4", "SEARCH BODY garden"), "* SEARCH 2"));
    assert!(has(&client.ok("q5", "SEARCH FROM ben SINCE 1-Oct-2026"), "* SEARCH 1 2"));
    let none = client.ok("q6", "SEARCH BEFORE 1-Oct-2026");
    assert!(none.iter().any(|l| l == "* SEARCH"), "{none:?}");
    assert!(has(&client.ok("q8", "UID SEARCH UID 2:*"), "* SEARCH 2"));
    assert!(has(&client.ok("q9", "SEARCH OR SUBJECT lunch SUBJECT plans NOT DELETED"), "* SEARCH 1 2"));
    let charset = client.run("q10", "SEARCH CHARSET ISO-8859-1 ALL");
    assert!(charset.last().unwrap().contains("[BADCHARSET"), "{charset:?}");
}

#[test]
fn copy_and_move_answer_with_copyuid_and_a_move_expunges_here() {
    let drive = seeded();
    let mut client = Client::signed_in(bridge(drive.clone()));
    client.ok("c0", "SELECT INBOX");
    let copied = client.ok("c1", "COPY 1 Archive");
    assert!(copied.last().unwrap().contains("[COPYUID 1700000000 1 1]"), "{copied:?}");
    assert_eq!(drive.keys().iter().filter(|k| k.starts_with("mail/Archive/")).count(), 1);
    let moved = client.ok("c2", "UID MOVE 2 Trash");
    assert!(has(&moved, "* OK [COPYUID 1700000000 2 1] Moved"), "{moved:?}");
    assert!(has(&moved, "* 2 EXPUNGE"), "{moved:?}");
    assert_eq!(drive.keys().iter().filter(|k| k.starts_with("mail/Inbox/")).count(), 1);
    assert_eq!(drive.keys().iter().filter(|k| k.starts_with("mail/Trash/")).count(), 1);
    let nowhere = client.run("c3", "COPY 1 Nowhere");
    assert!(nowhere.last().unwrap().starts_with("c3 NO [TRYCREATE]"));
    let status = client.ok("c4", "STATUS Trash (MESSAGES UNSEEN UIDNEXT)");
    assert!(has(&status, "* STATUS \"Trash\" (MESSAGES 1 UNSEEN 1 UIDNEXT 2)"), "{status:?}");
}

#[test]
fn append_files_under_an_azlin_name_and_a_copy_the_bridge_sent_is_not_filed_twice() {
    let drive = seeded();
    let imap = bridge(drive.clone());
    let mut client = Client::signed_in(imap.clone());
    let message = mail("report", "Numbers.");
    client.send(
        format!(
            "a1 APPEND Drafts (\\Seen \\Draft) \" 1-Oct-2026 09:00:00 +0000\" {{{}}}\r\n",
            message.len()
        )
        .as_bytes(),
    );
    assert!(client.response().starts_with("+ "));
    client.send(&message);
    client.send(b"\r\n");
    let done = client.until("a1");
    assert!(done.last().unwrap().contains("[APPENDUID 1700000000 1]"), "{done:?}");
    let name = azlin::object_name(&message, OCT_1 + 30 * 60);
    assert!(drive.keys().contains(&format!("mail/Drafts/{name}")), "{:?}", drive.keys());
    assert!(drive
        .keys()
        .contains(&format!("mail/.state/{}/seen", name.trim_end_matches(".eml"))));
    client.ok("a2", "SELECT Drafts");
    assert!(has(&client.ok("a3", "FETCH 1 FLAGS"), "FLAGS (\\Seen \\Draft)"));
    // The submission port filed the sent copy: the program's own APPEND to Sent is answered
    // with it.
    let sent = imap
        .store
        .append("Sent", &mail("sent-one", "Out."), OCT_1, crate::store::Marks::default())
        .unwrap();
    imap.sent.remember("sent-one@example.net", "Sent", &sent.name);
    let copy = mail("sent-one", "Out, as the program kept it.");
    client.send(format!("a4 APPEND Sent {{{}+}}\r\n", copy.len()).as_bytes());
    client.send(&copy);
    client.send(b"\r\n");
    let answer = client.until("a4");
    assert!(answer.last().unwrap().contains("Already filed"), "{answer:?}");
    assert_eq!(drive.keys().iter().filter(|k| k.starts_with("mail/Sent/")).count(), 1);
    let nowhere = client.run("a5", "APPEND Nowhere {1+}\r\nx");
    assert!(nowhere.last().unwrap().starts_with("a5 NO [TRYCREATE]"));
}

#[test]
fn mailboxes_are_created_renamed_and_deleted_and_bad_names_refused() {
    let drive = seeded();
    let mut client = Client::signed_in(bridge(drive.clone()));
    client.ok("m1", "CREATE \"Receipts/2026\"");
    assert!(drive.keys().contains(&"mail/Receipts/2026/".to_string()), "{:?}", drive.keys());
    let again = client.run("m2", "CREATE \"Receipts/2026\"");
    assert!(again.last().unwrap().starts_with("m2 NO [ALREADYEXISTS]"), "{again:?}");
    client.ok("m3", "RENAME \"Receipts/2026\" \"Receipts/Old\"");
    assert!(has(&client.ok("m4", "LIST \"\" \"Receipts/*\""), "\"Receipts/Old\""));
    client.ok("m5", "DELETE \"Receipts/Old\"");
    for bad in ["\"../up\"", "\".hidden\"", "\"a//b\"", "INBOX"] {
        let refused = client.run("m6", &format!("CREATE {bad}"));
        assert!(refused.last().unwrap().starts_with("m6 NO"), "{bad}: {refused:?}");
    }
    assert!(client.run("m7", "DELETE INBOX").last().unwrap().starts_with("m7 NO"));
}

#[test]
fn idle_tells_about_new_mail_and_ends_with_done() {
    let drive = seeded();
    let limits = Limits {
        idle_poll: Duration::from_millis(100),
        ..Limits::default()
    };
    let mut client = Client::signed_in(bridge_with(drive.clone(), limits));
    client.ok("i0", "SELECT INBOX");
    client.send(b"i1 IDLE\r\n");
    assert_eq!(client.response(), "+ idling");
    let message = mail("late", "Arrived during IDLE.");
    drive
        .put(&azlin::message_key("Inbox", &azlin::object_name(&message, OCT_1 + 600)), &message)
        .unwrap();
    assert_eq!(client.response(), "* 3 EXISTS");
    assert_eq!(client.response(), "* 0 RECENT");
    client.send(b"DONE\r\n");
    assert!(client.until("i1").last().unwrap().starts_with("i1 OK"));
    let uid = client.ok("i2", "FETCH 3 UID");
    assert!(has(&uid, "* 3 FETCH (UID 3)"), "{uid:?}");
}

#[test]
fn noop_reports_what_another_device_changed() {
    let drive = seeded();
    let mut client = Client::signed_in(bridge(drive.clone()));
    client.ok("n0", "SELECT INBOX");
    let keys: Vec<String> = drive
        .keys()
        .into_iter()
        .filter(|k| k.starts_with("mail/Inbox/"))
        .collect();
    // Another device archives the first message and flags the second.
    let second_id = azlin::message_id(&keys[1]).unwrap().to_string();
    drive.delete(&keys[0]).unwrap();
    drive.put(&azlin::marker_key(&second_id, azlin::FLAGGED), &[]).unwrap();
    std::thread::sleep(Duration::from_millis(2100));
    let lines = client.ok("n1", "NOOP");
    assert!(has(&lines, "* 1 EXPUNGE"), "{lines:?}");
    assert!(has(&lines, "* 1 FETCH (UID 2 FLAGS (\\Flagged))"), "{lines:?}");
}

#[test]
fn oversized_literals_are_refused_and_an_http_request_is_hung_up_on() {
    let limits = Limits {
        literal_bytes: 16,
        ..Limits::default()
    };
    let mut client = Client::connect(bridge_with(seeded(), limits));
    client.send(b"x1 LOGIN {999}\r\n");
    assert!(client.response().starts_with("x1 NO [TOOBIG]"));
    // Still in step: the next command works.
    assert!(client.run("x2", "NOOP").last().unwrap().starts_with("x2 OK"));
    client.send(b"x3 LOGIN {999+}\r\n");
    assert!(client.response().starts_with("* BYE [TOOBIG]"));

    let mut client = Client::connect(bridge(seeded()));
    client.send(b"POST /x HTTP/1.1\r\nHost: 127.0.0.1\r\n\r\n");
    let started = std::time::Instant::now();
    let mut rest = Vec::new();
    // Closed unanswered (a reset, when the request's other lines were never read).
    let _ = std::io::Read::read_to_end(&mut client.stream, &mut rest);
    assert!(rest.is_empty(), "{rest:?}");
    assert!(started.elapsed() < Duration::from_secs(5), "the bridge kept the connection");
}

#[test]
fn a_command_the_grammar_does_not_know_is_bad_and_the_session_goes_on() {
    let mut client = Client::signed_in(bridge(seeded()));
    let answer = client.run("b1", "FROBNICATE now");
    assert!(answer.last().unwrap().starts_with("b1 BAD"));
    let answer = client.run("b2", "FETCH 1 (FLAGS");
    assert!(answer.last().unwrap().starts_with("b2 BAD"));
    assert!(client.run("b3", "NOOP").last().unwrap().starts_with("b3 OK"));
    let namespace = client.ok("b4", "NAMESPACE");
    assert!(has(&namespace, "* NAMESPACE ((\"\" \"/\")) NIL NIL"));
    let id = client.ok("b5", "ID (\"name\" \"Mail\")");
    assert!(has(&id, "* ID (\"name\" \"Azlin Bridge\""));
}
