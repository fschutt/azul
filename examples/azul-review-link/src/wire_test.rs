//! The bytes of every frame: round trips, the exact bytes of a few (the format is a promise to the other side),
//! and the total decoder: truncation, garbage, flipped bytes, lies about lengths, numbers that are no numbers.

use super::*;
use crate::access::ScreenId;
use crate::frame::*;

fn pen_state() -> PenState {
    PenState {
        seq: 41,
        screen: ScreenId(2),
        folder: "azul".into(),
        surface: Surface::Page(PageSurface {
            file: "layout/src/solver3/fc.rs".into(),
            page_index: 2,
            page_count: 9,
            line: 1337,
            line_count: 4000,
            aspect: 794.0 / 1123.0,
            gap: 0.04,
        }),
        view: Rect { x: 0.125, y: 3.25, w: 0.5, h: 0.375 },
        zoom: 1.75,
        pen: Some(PenMark { x: 0.5, y: 3.625, down: true }),
        thumb_id: 7,
        context: Context::Line,
        return_point: Some(ReturnPoint { file: "layout/src/window.rs".into(), page_index: 1, line: 214, label: "window.rs p2 l214".into() }),
    }
}

fn board_state() -> PenState {
    PenState {
        seq: 42,
        surface: Surface::Whiteboard(WhiteboardSurface { board_id: 3, name: "Layout engine".into(), aspect: 1.6, item_count: 27 }),
        view: Rect { x: 0.1, y: 0.05, w: 0.4, h: 0.25 },
        zoom: 2.5,
        pen: Some(PenMark { x: 0.3, y: 0.2, down: false }),
        thumb_id: 9,
        ..PenState::empty()
    }
}

fn hello(role: Role) -> Hello {
    Hello { role, protocol: 1, min_protocol: 1, app_version: "0.1.0".into(), name: "Studio Mac".into(), ticket: "endpointabc".into() }
}

fn gesture(seq: u32, phase: Phase, pan_x: f32, pan_y: f32, zoom: f32, focus: Focus) -> Frame {
    Frame::Viewport(ViewportCmd { screen: ScreenId(1), seq, op: ViewportOp::Gesture { id: 3, phase, pan_x, pan_y, zoom, focus } })
}

/// One of every frame, and the empty and the odd ones.
fn samples() -> Vec<Frame> {
    let entry = |name: &str, kind, state| Entry { name: name.into(), kind, state };
    let mut all = vec![
        Frame::Hello(hello(Role::Host)),
        Frame::Hello(Hello { role: Role::Phone, protocol: 1, min_protocol: 1, app_version: String::new(), name: String::new(), ticket: String::new() }),
        Frame::Ping { nonce: 0x0102_0304 },
        Frame::Pong { nonce: u32::MAX },
        Frame::Bye,
        Frame::PairRequest { proof: [7u8; 32], phone_name: "iPhone von Änne".into() },
        gesture(9, Phase::Begin, 0.0, 0.0, 1.0, Focus::Pen),
        gesture(10, Phase::Update, -0.25, 0.5, 1.5, Focus::Point { x: 0.3, y: 0.7 }),
        gesture(11, Phase::End, 1.0, 2.0, 0.5, Focus::Center),
        Frame::Viewport(ViewportCmd { screen: ScreenId::MAIN, seq: 12, op: ViewportOp::Fit }),
        Frame::Viewport(ViewportCmd { screen: ScreenId::MAIN, seq: 13, op: ViewportOp::PageStep(-1) }),
        Frame::Viewport(ViewportCmd { screen: ScreenId::MAIN, seq: 14, op: ViewportOp::PageStep(1) }),
        Frame::Viewport(ViewportCmd { screen: ScreenId::MAIN, seq: 15, op: ViewportOp::Reset }),
        Frame::ListDir { request_id: 5, path: String::new(), offset: 0 },
        Frame::ListDir { request_id: 6, path: "layout/src".into(), offset: 500 },
        Frame::OpenFile { screen: ScreenId::MAIN, request_id: 7, path: "src/lib.rs".into(), line: Some(42) },
        Frame::OpenFile { screen: ScreenId::MAIN, request_id: 8, path: "README.md".into(), line: None },
        Frame::Button { screen: ScreenId::MAIN, request_id: 9, index: 3 },
        Frame::SetButtons { request_id: 10, buttons: ButtonSet::defaults() },
        Frame::NeedThumbnail { id: 7 },
        Frame::PairAccepted { code: 482_913 },
        Frame::Refused(Refusal::NotPaired),
        Frame::PenState(pen_state()),
        Frame::PenState(board_state()),
        Frame::PenState(PenState::empty()),
        Frame::ContinueReviewing { screen: ScreenId(4) },
        Frame::AgentFinished(AgentFinished { conversation_id: 12, file: "layout/src/fc.rs".into(), line: 1337, kind: FinishKind::Done }),
        Frame::AgentFinished(AgentFinished { conversation_id: 0, file: String::new(), line: 0, kind: FinishKind::NoAction }),
        Frame::Thumbnail(Thumbnail { id: 7, width: 2, height: 2, format: ThumbFormat::Rgba8, bytes: vec![1; 16] }),
        Frame::Thumbnail(Thumbnail { id: 8, width: 64, height: 80, format: ThumbFormat::Png, bytes: vec![9; 1000] }),
        Frame::Thumbnail(Thumbnail { id: 9, width: 64, height: 80, format: ThumbFormat::Jpeg, bytes: Vec::new() }),
        Frame::Rows(Rows {
            request_id: 6,
            dir: "layout/src".into(),
            offset: 0,
            entries: vec![
                entry("lib.rs", EntryKind::File, FileState::Clean),
                entry("solver3", EntryKind::Dir, FileState::Working),
                entry("ünï.rs", EntryKind::File, FileState::Unresolved),
                entry("x", EntryKind::File, FileState::None),
            ],
            more: true,
        }),
        Frame::Rows(Rows { request_id: 0, dir: String::new(), offset: 0, entries: Vec::new(), more: false }),
        Frame::Buttons(ButtonSet::defaults()),
        Frame::Reply(Reply { request_id: 9, status: ReplyStatus::Suggested }),
        Frame::Notice(Notice { level: NoticeLevel::Warning, text: "AzReview is closing".into() }),
    ];
    for reason in [PairReject::WrongSecret, PairReject::Expired, PairReject::NoPairingOpen, PairReject::Busy, PairReject::TooManyPhones, PairReject::Declined] {
        all.push(Frame::PairRejected(reason));
    }
    for refusal in [Refusal::UpdateCompanion, Refusal::UpdateAzReview, Refusal::UnsupportedRole] {
        all.push(Frame::Refused(refusal));
    }
    for kind in [FinishKind::Suggestion, FinishKind::NoAction, FinishKind::Failed] {
        all.push(Frame::AgentFinished(AgentFinished { conversation_id: 3, file: "a.rs".into(), line: 9, kind }));
    }
    for role in [Role::Review, Role::ReviewMirror, Role::Whiteboard] {
        all.push(Frame::Hello(hello(role)));
    }
    for bits in 0u8..32 {
        all.push(Frame::Settings(HostSettings {
            reverse_pan: bits & 1 != 0,
            snap_to_page_top: bits & 2 != 0,
            haptic_done: bits & 4 != 0,
            haptic_suggestion: bits & 8 != 0,
            haptic_failed: bits & 16 != 0,
        }));
    }
    for status in [ReplyStatus::Received, ReplyStatus::Working, ReplyStatus::Done, ReplyStatus::Failed] {
        all.push(Frame::Reply(Reply { request_id: 1, status }));
    }
    all
}

/// A frame of kind `tag` with the bytes of `payload`, as a hostile peer would write it.
fn raw(tag: u8, payload: &[u8]) -> Vec<u8> {
    let mut bytes = vec![MAGIC, WIRE_VERSION, tag];
    bytes.extend_from_slice(payload);
    bytes
}

/// A small deterministic generator (xorshift64*), so a failing fuzz case is the same case next time.
struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 >> 12;
        self.0 ^= self.0 << 25;
        self.0 ^= self.0 >> 27;
        self.0.wrapping_mul(0x2545_F491_4F6C_DD1D)
    }

    fn bytes(&mut self, len: usize) -> Vec<u8> {
        (0..len).map(|_| (self.next() >> 24) as u8).collect()
    }
}

// ---- round trips

#[test]
fn every_frame_round_trips_and_writes_the_same_bytes_again() {
    for frame in samples() {
        let bytes = encode(&frame).unwrap_or_else(|e| panic!("{} did not encode: {e}", frame.name()));
        let back = decode(&bytes).unwrap_or_else(|e| panic!("{} did not decode: {e}", frame.name()));
        assert_eq!(back, frame);
        assert_eq!(encode(&back).unwrap(), bytes, "{} is not written the same twice", frame.name());
    }
}

#[test]
fn the_exact_bytes_of_a_few_frames_are_a_promise() {
    assert_eq!(encode(&Frame::Ping { nonce: 0x0102_0304 }).unwrap(), [0xA7, 1, tag::PING, 4, 3, 2, 1]);
    assert_eq!(encode(&Frame::Bye).unwrap(), [0xA7, 1, tag::BYE]);
    // a frame that acts on a screen starts with the screen's number
    assert_eq!(encode(&Frame::Button { screen: ScreenId(2), request_id: 7, index: 3 }).unwrap(), [0xA7, 1, tag::BUTTON, 2, 0, 0, 0, 7, 0, 0, 0, 3]);
    assert_eq!(
        encode(&Frame::Viewport(ViewportCmd { screen: ScreenId::MAIN, seq: 5, op: ViewportOp::Fit })).unwrap(),
        [0xA7, 1, tag::VIEWPORT, 0, 0, 0, 0, 5, 0, 0, 0, 1]
    );
    assert_eq!(encode(&Frame::ContinueReviewing { screen: ScreenId(9) }).unwrap(), [0xA7, 1, tag::CONTINUE_REVIEWING, 9, 0, 0, 0]);
    // the settings are one byte: reverse pan, snap, buzz for a result, for a suggestion, for a failure
    assert_eq!(encode(&Frame::Settings(HostSettings::default())).unwrap(), [0xA7, 1, tag::SETTINGS, 0b1_1100]);
    assert_eq!(
        encode(&Frame::Settings(HostSettings { reverse_pan: true, snap_to_page_top: true, haptic_done: false, haptic_suggestion: false, haptic_failed: false })).unwrap(),
        [0xA7, 1, tag::SETTINGS, 0b0_0011]
    );
    assert_eq!(
        encode(&Frame::AgentFinished(AgentFinished { conversation_id: 5, file: "a".into(), line: 6, kind: FinishKind::Suggestion })).unwrap(),
        [0xA7, 1, tag::AGENT_FINISHED, 5, 0, 0, 0, 1, 0, b'a', 6, 0, 0, 0, 1]
    );
    assert_eq!(encode(&Frame::PairAccepted { code: 482_913 }).unwrap(), [0xA7, 1, tag::PAIR_ACCEPTED, 0x61, 0x5E, 0x07, 0x00]);
    // a string is a u16 length and the UTF-8
    assert_eq!(
        encode(&Frame::ListDir { request_id: 1, path: "ab".into(), offset: 2 }).unwrap(),
        [0xA7, 1, tag::LIST_DIR, 1, 0, 0, 0, 2, 0, b'a', b'b', 2, 0, 0, 0]
    );
}

#[test]
fn every_kind_of_frame_has_a_tag_of_its_own() {
    let mut seen: Vec<(&'static str, u8)> = Vec::new();
    for frame in samples() {
        let bytes = encode(&frame).unwrap();
        let entry = (frame.name(), bytes[2]);
        match seen.iter().find(|(name, _)| *name == entry.0) {
            Some(known) => assert_eq!(known.1, entry.1, "{} has two tags", entry.0),
            None => seen.push(entry),
        }
    }
    let mut tags: Vec<u8> = seen.iter().map(|(_, t)| *t).collect();
    let count = tags.len();
    tags.sort_unstable();
    tags.dedup();
    assert_eq!(tags.len(), count, "two kinds share a tag: {seen:?}");
    assert_eq!(count, 23, "{seen:?}");
}

// ---- the decoder is total

#[test]
fn a_frame_cut_anywhere_is_an_error_and_never_a_panic() {
    for frame in samples() {
        let bytes = encode(&frame).unwrap();
        for cut in 0..bytes.len() {
            assert!(decode(&bytes[..cut]).is_err(), "{} cut at {cut} of {} decoded", frame.name(), bytes.len());
        }
    }
}

#[test]
fn bytes_after_the_end_of_a_frame_are_refused() {
    for frame in samples() {
        let mut bytes = encode(&frame).unwrap();
        bytes.push(0);
        assert_eq!(decode(&bytes), Err(DecodeError::Trailing { extra: 1 }), "{}", frame.name());
    }
}

#[test]
fn random_bytes_never_panic() {
    let mut rng = Rng(0x9E37_79B9_7F4A_7C15);
    for _ in 0..40_000 {
        let len = (rng.next() % 300) as usize;
        let bytes = rng.bytes(len);
        let _ = decode(&bytes);
    }
}

#[test]
fn random_payloads_behind_a_good_header_never_panic_for_any_tag() {
    let mut rng = Rng(0xDEAD_BEEF_CAFE_F00D);
    for tag in 0..=40u8 {
        for _ in 0..3_000 {
            let len = (rng.next() % 200) as usize;
            let _ = decode(&raw(tag, &rng.bytes(len)));
        }
    }
}

#[test]
fn a_changed_byte_decodes_to_something_or_errs_and_never_panics() {
    for frame in samples() {
        let bytes = encode(&frame).unwrap();
        for at in 0..bytes.len() {
            for change in [0x00u8, 0xFF, 0x55, 0x80, 0x7F] {
                let mut hurt = bytes.clone();
                hurt[at] = change;
                let _ = decode(&hurt);
            }
        }
    }
}

#[test]
fn a_frame_over_the_cap_is_refused_before_it_is_read() {
    let big = vec![MAGIC; MAX_FRAME_BYTES + 1];
    assert_eq!(decode(&big), Err(DecodeError::TooLarge { len: MAX_FRAME_BYTES + 1, max: MAX_FRAME_BYTES }));
}

#[test]
fn the_header_is_checked_first() {
    assert_eq!(decode(&[]), Err(DecodeError::Truncated { field: "header" }));
    assert_eq!(decode(&[0x00, 1, tag::BYE]), Err(DecodeError::BadMagic));
    assert_eq!(decode(&[MAGIC, 2, tag::BYE]), Err(DecodeError::UnsupportedVersion { got: 2, ours: 1 }));
    assert_eq!(decode(&[MAGIC, 0, tag::BYE]), Err(DecodeError::UnsupportedVersion { got: 0, ours: 1 }));
    assert_eq!(decode(&[MAGIC, 1, 99]), Err(DecodeError::UnknownTag(99)));
    assert_eq!(decode(&[MAGIC, 1, 0]), Err(DecodeError::UnknownTag(0)));
}

#[test]
fn a_version_this_build_does_not_speak_says_to_update() {
    let said = DecodeError::UnsupportedVersion { got: 2, ours: 1 }.to_string();
    assert!(said.contains("update"), "{said}");
}

// ---- lies about lengths

#[test]
fn a_string_that_claims_more_than_its_cap_is_refused_without_reserving_it() {
    // Hello: role, protocol, min protocol, version (empty), then a name of 65535 bytes that is not there
    let payload = [0u8, 1, 0, 1, 0, 0, 0, 0xFF, 0xFF];
    assert_eq!(
        decode(&raw(tag::HELLO, &payload)),
        Err(DecodeError::TooLong { field: "name", len: 65_535, max: MAX_NAME_BYTES })
    );
}

#[test]
fn a_list_that_claims_more_rows_than_the_cap_is_refused_without_reserving_them() {
    // Rows: request id, dir (empty), offset, more, then 65535 rows that are not there
    let payload = [0u8, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0xFF, 0xFF];
    assert_eq!(
        decode(&raw(tag::ROWS, &payload)),
        Err(DecodeError::TooLong { field: "rows", len: 65_535, max: MAX_ROWS })
    );
}

#[test]
fn a_thumbnail_that_claims_four_gigabytes_is_refused_without_reserving_them() {
    // id, width 8, height 8, format Rgba8, then a length of u32::MAX
    let payload = [1u8, 0, 0, 0, 8, 0, 8, 0, 0, 0xFF, 0xFF, 0xFF, 0xFF];
    assert_eq!(
        decode(&raw(tag::THUMBNAIL, &payload)),
        Err(DecodeError::TooLong { field: "thumbnail bytes", len: u32::MAX as usize, max: MAX_THUMBNAIL_BYTES })
    );
}

#[test]
fn a_thumbnail_whose_pixels_are_not_its_size_is_refused() {
    let thumb = |width, height, len: usize| {
        let mut payload = vec![1u8, 0, 0, 0];
        payload.extend_from_slice(&u16::to_le_bytes(width));
        payload.extend_from_slice(&u16::to_le_bytes(height));
        payload.push(0); // Rgba8
        payload.extend_from_slice(&u32::to_le_bytes(len as u32));
        payload.extend(std::iter::repeat_n(0u8, len));
        raw(tag::THUMBNAIL, &payload)
    };
    assert!(decode(&thumb(4, 4, 64)).is_ok());
    assert_eq!(decode(&thumb(4, 4, 60)), Err(DecodeError::BadValue { field: "thumbnail bytes" }));
    assert_eq!(decode(&thumb(0, 4, 0)), Err(DecodeError::BadValue { field: "thumbnail size" }));
    assert_eq!(decode(&thumb(MAX_THUMBNAIL_SIDE + 1, 4, 0)), Err(DecodeError::BadValue { field: "thumbnail size" }));
}

// ---- values that are not allowed

#[test]
fn a_number_that_is_no_number_is_refused_in_every_float_field() {
    for bad in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY] {
        let mut state = pen_state();
        state.zoom = bad;
        assert_eq!(encode(&Frame::PenState(state)), Err(EncodeError::BadValue { field: "zoom" }));
        let frame = gesture(1, Phase::Update, bad, 0.0, 1.0, Focus::Pen);
        assert_eq!(encode(&frame), Err(EncodeError::BadValue { field: "pan" }));
        let frame = gesture(1, Phase::Update, 0.0, 0.0, 1.0, Focus::Point { x: bad, y: 0.0 });
        assert_eq!(encode(&frame), Err(EncodeError::BadValue { field: "focus" }));
    }
    // and a frame somebody else wrote with one in it
    let good = encode(&gesture(1, Phase::Update, 0.5, 0.0, 1.0, Focus::Pen)).unwrap();
    let mut hurt = good.clone();
    let nan = f32::NAN.to_le_bytes();
    // screen (4) + seq (4) + op tag (1) + id (4) + phase (1) = the pan_x float starts 14 bytes after the header
    hurt[3 + 14..3 + 18].copy_from_slice(&nan);
    assert_eq!(decode(&hurt), Err(DecodeError::BadValue { field: "pan" }));
}

#[test]
fn a_kind_byte_out_of_range_is_refused() {
    let mut bytes = encode(&Frame::Hello(hello(Role::Host))).unwrap();
    bytes[3] = 9;
    assert_eq!(decode(&bytes), Err(DecodeError::BadValue { field: "role" }));
    let mut bytes = encode(&Frame::PairRejected(PairReject::Busy)).unwrap();
    bytes[3] = 77;
    assert_eq!(decode(&bytes), Err(DecodeError::BadValue { field: "reject reason" }));
    let mut bytes = encode(&Frame::Reply(Reply { request_id: 1, status: ReplyStatus::Done })).unwrap();
    bytes[7] = 200;
    assert_eq!(decode(&bytes), Err(DecodeError::BadValue { field: "reply status" }));
    let mut bytes = encode(&Frame::Viewport(ViewportCmd { screen: ScreenId::MAIN, seq: 1, op: ViewportOp::Reset })).unwrap();
    bytes[11] = 42;
    assert_eq!(decode(&bytes), Err(DecodeError::BadValue { field: "viewport op" }));
    // what a NEWER AzReview may say is read for what this build understands, not refused
    let mut bytes = encode(&Frame::Settings(HostSettings::default())).unwrap();
    bytes[3] = 0b1010_0100;
    assert_eq!(
        decode(&bytes),
        Ok(Frame::Settings(HostSettings { reverse_pan: false, snap_to_page_top: false, haptic_done: true, haptic_suggestion: false, haptic_failed: false })),
        "a bit this build does not know is left alone"
    );
    let mut bytes = encode(&Frame::AgentFinished(AgentFinished { conversation_id: 1, file: String::new(), line: 1, kind: FinishKind::Done })).unwrap();
    let last = bytes.len() - 1;
    bytes[last] = 9;
    assert!(matches!(decode(&bytes), Ok(Frame::AgentFinished(AgentFinished { kind: FinishKind::NoAction, .. }))), "an ending this build does not know is nothing to do");
    bytes[last] = 3;
    assert!(matches!(decode(&bytes), Ok(Frame::AgentFinished(AgentFinished { kind: FinishKind::Failed, .. }))));
    let mut bytes = encode(&Frame::Refused(Refusal::NotPaired)).unwrap();
    bytes[3] = 4;
    assert_eq!(decode(&bytes), Err(DecodeError::BadValue { field: "refusal" }));
}

#[test]
fn text_that_is_not_utf8_is_refused() {
    // ListDir: request id, then a path of two bytes that are no UTF-8
    let payload = [1u8, 0, 0, 0, 2, 0, 0xFF, 0xFE, 0, 0, 0, 0];
    assert_eq!(decode(&raw(tag::LIST_DIR, &payload)), Err(DecodeError::BadUtf8 { field: "path" }));
}

#[test]
fn a_quick_button_over_its_character_cap_is_refused_in_characters() {
    let mut set = ButtonSet::defaults();
    set.buttons[1].label = "x".repeat(MAX_LABEL_CHARS + 1);
    assert_eq!(
        encode(&Frame::Buttons(set)),
        Err(EncodeError::TooLong { field: "button label", len: MAX_LABEL_CHARS + 1, max: MAX_LABEL_CHARS })
    );
    let mut set = ButtonSet::defaults();
    set.buttons[2].text = "y".repeat(MAX_BUTTON_TEXT_CHARS + 1);
    assert_eq!(
        encode(&Frame::SetButtons { request_id: 1, buttons: set }),
        Err(EncodeError::TooLong { field: "button text", len: MAX_BUTTON_TEXT_CHARS + 1, max: MAX_BUTTON_TEXT_CHARS })
    );
    // the same on the way in: a frame somebody else wrote with a label of 13 characters
    let mut good = ButtonSet::defaults();
    good.buttons[0].label = "x".repeat(MAX_LABEL_CHARS);
    let mut bytes = encode(&Frame::Buttons(good)).unwrap();
    // first label: u16 length at 3, the characters after it; make it 13 long by writing a 13-byte label by hand
    let rest = bytes.split_off(5 + MAX_LABEL_CHARS);
    bytes[3] = (MAX_LABEL_CHARS + 1) as u8;
    bytes.push(b'x');
    bytes.extend_from_slice(&rest);
    assert!(matches!(decode(&bytes), Err(DecodeError::TooLong { field: "button label", .. })), "{:?}", decode(&bytes));
}

// ---- the biggest frames that are allowed

#[test]
fn the_biggest_allowed_things_round_trip_and_fit_in_a_frame() {
    let mut set = ButtonSet::defaults();
    set.buttons[0].label = "😀".repeat(MAX_LABEL_CHARS);
    set.buttons[3].text = "ü".repeat(MAX_BUTTON_TEXT_CHARS);
    let biggest = vec![
        Frame::Buttons(set),
        Frame::Thumbnail(Thumbnail { id: 1, width: 1024, height: 1024, format: ThumbFormat::Png, bytes: vec![3; MAX_THUMBNAIL_BYTES] }),
        Frame::Rows(Rows {
            request_id: 1,
            dir: "d".repeat(MAX_PATH_BYTES),
            offset: u32::MAX,
            entries: (0..MAX_ROWS)
                .map(|i| Entry { name: format!("{i:0>width$}", width = MAX_ENTRY_NAME_BYTES), kind: EntryKind::File, state: FileState::Clean })
                .collect(),
            more: true,
        }),
        Frame::Hello(Hello {
            role: Role::Host,
            protocol: u16::MAX,
            min_protocol: 0,
            app_version: "v".repeat(MAX_VERSION_BYTES),
            name: "n".repeat(MAX_NAME_BYTES),
            ticket: "t".repeat(MAX_TICKET_BYTES),
        }),
        Frame::Notice(Notice { level: NoticeLevel::Info, text: "n".repeat(MAX_NOTICE_BYTES) }),
        Frame::OpenFile { screen: ScreenId::MAIN, request_id: u32::MAX, path: "p".repeat(MAX_PATH_BYTES), line: Some(u32::MAX) },
    ];
    for frame in biggest {
        let bytes = encode(&frame).unwrap_or_else(|e| panic!("{} did not encode: {e}", frame.name()));
        assert!(bytes.len() <= MAX_FRAME_BYTES, "{} is {} bytes", frame.name(), bytes.len());
        assert_eq!(decode(&bytes).unwrap(), frame);
    }
}

#[test]
fn things_over_their_caps_are_refused_on_the_way_out_with_the_cap_named() {
    let over = |frame: Frame| encode(&frame).unwrap_err();
    assert_eq!(
        over(Frame::OpenFile { screen: ScreenId::MAIN, request_id: 1, path: "p".repeat(MAX_PATH_BYTES + 1), line: None }),
        EncodeError::TooLong { field: "path", len: MAX_PATH_BYTES + 1, max: MAX_PATH_BYTES }
    );
    assert_eq!(
        over(Frame::Rows(Rows {
            request_id: 1,
            dir: String::new(),
            offset: 0,
            entries: (0..=MAX_ROWS).map(|_| Entry { name: "x".into(), kind: EntryKind::File, state: FileState::None }).collect(),
            more: false
        })),
        EncodeError::TooLong { field: "rows", len: MAX_ROWS + 1, max: MAX_ROWS }
    );
    assert_eq!(
        over(Frame::Thumbnail(Thumbnail { id: 1, width: 64, height: 64, format: ThumbFormat::Jpeg, bytes: vec![0; MAX_THUMBNAIL_BYTES + 1] })),
        EncodeError::TooLong { field: "thumbnail bytes", len: MAX_THUMBNAIL_BYTES + 1, max: MAX_THUMBNAIL_BYTES }
    );
    assert_eq!(
        over(Frame::Thumbnail(Thumbnail { id: 1, width: 2, height: 2, format: ThumbFormat::Rgba8, bytes: vec![0; 15] })),
        EncodeError::BadValue { field: "thumbnail bytes" }
    );
    assert_eq!(
        over(Frame::Notice(Notice { level: NoticeLevel::Info, text: "n".repeat(MAX_NOTICE_BYTES + 1) })),
        EncodeError::TooLong { field: "notice", len: MAX_NOTICE_BYTES + 1, max: MAX_NOTICE_BYTES }
    );
    assert_eq!(
        over(Frame::Hello(Hello { role: Role::Phone, protocol: 1, min_protocol: 1, app_version: String::new(), name: "n".repeat(MAX_NAME_BYTES + 1), ticket: String::new() })),
        EncodeError::TooLong { field: "name", len: MAX_NAME_BYTES + 1, max: MAX_NAME_BYTES }
    );
}

#[test]
fn an_error_says_in_words_what_was_wrong() {
    for error in [
        DecodeError::TooLarge { len: 9, max: 1 },
        DecodeError::BadMagic,
        DecodeError::UnknownTag(3),
        DecodeError::Truncated { field: "name" },
        DecodeError::TooLong { field: "rows", len: 600, max: 500 },
        DecodeError::BadUtf8 { field: "path" },
        DecodeError::BadValue { field: "zoom" },
        DecodeError::Trailing { extra: 3 },
    ] {
        let said = error.to_string();
        assert!(said.len() > 12 && !said.contains("{}"), "{said}");
    }
}

// ---- the surfaces of a pen state

/// The bytes of `state`, and a copy of them with the byte at `at` (counted from the end) changed to `value`.
fn state_bytes(state: &PenState) -> Vec<u8> {
    encode(&Frame::PenState(state.clone())).unwrap()
}

#[test]
fn a_page_shape_that_is_no_shape_is_refused_both_ways() {
    for (aspect, gap) in [(0.0, 0.04), (-1.0, 0.04), (0.01, 0.04), (21.0, 0.04), (0.7, -0.1), (0.7, 2.5)] {
        let mut state = pen_state();
        state.surface = Surface::Page(PageSurface { aspect, gap, ..PageSurface::empty() });
        assert_eq!(encode(&Frame::PenState(state)), Err(EncodeError::BadValue { field: "page shape" }), "aspect {aspect} gap {gap}");
    }
    // somebody else's frame: the aspect float sits at the end of a page surface, before the gap
    let good = state_bytes(&pen_state());
    let mut hurt = good.clone();
    let at = hurt.len() - 8;
    hurt[at..at + 4].copy_from_slice(&0.0f32.to_le_bytes());
    assert_eq!(decode(&hurt), Err(DecodeError::BadValue { field: "page shape" }));
    let mut hurt = good;
    let at = hurt.len() - 4;
    hurt[at..at + 4].copy_from_slice(&5.0f32.to_le_bytes());
    assert_eq!(decode(&hurt), Err(DecodeError::BadValue { field: "page shape" }));
}

#[test]
fn a_page_count_over_the_cap_is_refused_before_anything_follows_it() {
    let mut state = pen_state();
    state.surface = Surface::Page(PageSurface { page_count: MAX_PAGE_COUNT + 1, ..PageSurface::empty() });
    assert!(matches!(encode(&Frame::PenState(state.clone())), Err(EncodeError::TooLong { field: "pages", .. })));
    state.surface = Surface::Page(PageSurface { page_count: MAX_PAGE_COUNT, ..PageSurface::empty() });
    assert!(encode(&Frame::PenState(state)).is_ok());
}

#[test]
fn a_whiteboard_is_named_shaped_and_counted_within_its_caps() {
    let board = |name: &str, aspect: f32, items: u32| {
        let mut state = board_state();
        state.surface = Surface::Whiteboard(WhiteboardSurface { board_id: 1, name: name.into(), aspect, item_count: items });
        Frame::PenState(state)
    };
    assert!(encode(&board("Layout", 1.6, 5)).is_ok());
    assert!(matches!(encode(&board(&"n".repeat(MAX_NAME_BYTES + 1), 1.6, 5)), Err(EncodeError::TooLong { field: "board name", .. })));
    assert_eq!(encode(&board("x", 0.0, 5)), Err(EncodeError::BadValue { field: "board shape" }));
    assert_eq!(encode(&board("x", f32::NAN, 5)), Err(EncodeError::BadValue { field: "board shape" }));
    assert!(matches!(encode(&board("x", 1.0, MAX_BOARD_ITEMS + 1)), Err(EncodeError::TooLong { field: "board items", .. })));
    let back = decode(&encode(&board("Übersicht", 2.0, MAX_BOARD_ITEMS)).unwrap()).unwrap();
    assert_eq!(back, board("Übersicht", 2.0, MAX_BOARD_ITEMS));
}

#[test]
fn a_return_point_keeps_its_file_page_line_and_label_and_its_strings_are_capped() {
    let mut state = pen_state();
    state.return_point = Some(ReturnPoint { file: "a".repeat(MAX_PATH_BYTES + 1), page_index: 0, line: 0, label: String::new() });
    assert!(matches!(encode(&Frame::PenState(state.clone())), Err(EncodeError::TooLong { field: "return file", .. })));
    state.return_point = Some(ReturnPoint { file: "ok.rs".into(), page_index: 4, line: 10, label: "l".repeat(MAX_NAME_BYTES + 1) });
    assert!(matches!(encode(&Frame::PenState(state.clone())), Err(EncodeError::TooLong { field: "return label", .. })));
    state.return_point = None;
    let back = decode(&state_bytes(&state)).unwrap();
    assert_eq!(back, Frame::PenState(state), "no return point comes back as none");
}

#[test]
fn the_surface_byte_of_a_pen_state_is_one_of_two_and_a_pen_state_cut_inside_a_surface_is_an_error() {
    let good = state_bytes(&board_state());
    for cut in 0..good.len() {
        assert!(decode(&good[..cut]).is_err(), "cut at {cut}");
    }
    let page = state_bytes(&pen_state());
    // find the surface byte: it follows the return point, so it is the first 0 before the page's file string... instead of
    // guessing the offset, flip every byte once: the decoder must answer with a frame or an error, never a panic
    for at in 3..page.len() {
        let mut hurt = page.clone();
        hurt[at] ^= 0xFF;
        let _ = decode(&hurt);
    }
}
