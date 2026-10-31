//! What a request may take and what its answer may weigh. An upload's time grows with its body,
//! so a big file reaches the bucket over a slow uplink; a call that reads a document (a listing,
//! a token endpoint, an error) caps its answer, a ranged read at its range. The parts of an
//! upload follow the link: smaller and one at a time on a slow one.

use std::{
    io::Cursor,
    sync::{Arc, Mutex},
    time::Duration,
};

use super::{
    fake_bucket::{FakeBucket, BUCKET},
    s3::Fake,
};
use crate::{
    multipart::{plan_parts, LinkEstimate, MIN_PART_SIZE},
    oauth::{refresh, OAuthClient},
    transport::{
        BytesPerSec, CallLimits, ExpectedBody, RequestTimeout, ANSWER_FLOOR, JSON_MAX,
        LISTING_MAX,
    },
    s3::{PARALLEL_PARTS, PART_SIZE},
    ByteRange, Credentials, Drive, HttpCall, HttpReply, ListRequest, S3Config, S3Drive,
    Transport,
};

const MIB: u64 = 1024 * 1024;

/// A link of `rate` bytes a second that `sharing` requests share, in virtual time: a request
/// whose body would take longer than the time its call allows gets "timed out", as azul's
/// client says. The bucket behind it is the multipart fake.
struct SlowLink {
    bucket: Arc<FakeBucket>,
    rate: u64,
    sharing: u64,
    /// The body sizes that went through.
    sent: Mutex<Vec<usize>>,
}

impl Transport for SlowLink {
    fn send(&self, call: &HttpCall) -> Result<HttpReply, String> {
        let budget = call.budget();
        let body = call.body.len() as u64;
        let takes = Duration::from_secs_f64((body * self.sharing) as f64 / self.rate as f64);
        let allowed = budget.send_body.map_or(budget.overall, |send| send.min(budget.overall));
        if takes > allowed {
            return Err(String::from("timed out"));
        }
        self.sent.lock().unwrap().push(call.body.len());
        self.bucket.answer(call)
    }
}

struct Link(Arc<SlowLink>);

impl Transport for Link {
    fn send(&self, call: &HttpCall) -> Result<HttpReply, String> {
        self.0.send(call)
    }
}

fn drive_over(link: &Arc<SlowLink>) -> S3Drive {
    S3Drive::new(
        S3Config {
            endpoint: String::from("http://127.0.0.1:9000"),
            region: String::from("us-east-1"),
            bucket: BUCKET.to_string(),
            path_style: true,
        },
        Credentials::new("AKIDTEST", "test-secret"),
        Box::new(Link(link.clone())),
    )
    .unwrap()
    .with_clock(|| 1_791_590_400)
}

#[test]
fn a_64_mib_file_reaches_the_bucket_over_a_link_of_one_mib_a_second() {
    let link = Arc::new(SlowLink {
        bucket: FakeBucket::new(),
        rate: MIB,
        sharing: PARALLEL_PARTS as u64,
        sent: Mutex::new(Vec::new()),
    });
    let drive = drive_over(&link);
    let bytes: Vec<u8> = (0..64 * MIB + 1024).map(|i| (i % 251) as u8).collect();
    let written = drive
        .put_from("videos/holiday.mov", &mut Cursor::new(bytes.clone()))
        .expect("every part fits the time its size allows");
    assert_eq!(written, bytes.len() as u64);
    assert_eq!(link.bucket.object("videos/holiday.mov"), Some(bytes));
    let parts = link.sent.lock().unwrap().iter().filter(|n| **n >= PART_SIZE).count();
    assert_eq!(parts, 4, "four parts of 16 MiB, each 64 s on the shared link");
}

#[test]
fn one_minute_for_the_whole_request_cannot_carry_a_part_that_shares_the_link() {
    // What every request had before: 60 s, whatever its body. A 16 MiB part sharing a link of
    // 1 MiB/s with three others needs 64 s.
    let minute = CallLimits {
        timeout: RequestTimeout::Overall(Duration::from_secs(60)),
        answer: ExpectedBody::Json { max: JSON_MAX },
    };
    assert!(minute.budget(16 * MIB).overall < Duration::from_secs(64));
    let upload = CallLimits::upload().budget(16 * MIB);
    assert!(upload.send_body.unwrap() >= Duration::from_secs(64));
    assert!(upload.overall >= upload.send_body.unwrap());
}

#[test]
fn an_uploads_time_grows_with_its_body() {
    let limits = CallLimits::upload();
    let RequestTimeout::Throughput { base, min_rate } = limits.timeout else {
        panic!("an upload's time follows its body: {:?}", limits.timeout);
    };
    assert_eq!(base, Duration::from_secs(30));
    assert_eq!(min_rate, BytesPerSec(32 * 1024));
    let small = limits.budget(0);
    let part = limits.budget(16 * MIB);
    assert_eq!(small.send_body, Some(Duration::from_secs(30)));
    assert_eq!(part.send_body, Some(Duration::from_secs(30 + 512)));
    assert_eq!(
        part.recv_body,
        Some(Duration::from_secs(30 + 32)),
        "the answer of an upload is a small document"
    );
    assert_eq!(part.overall, Duration::from_secs(542 + 62));
    assert_eq!(part.max_answer, JSON_MAX);
}

#[test]
fn a_ranged_reads_time_and_answer_follow_its_range() {
    let chunk = CallLimits::range(ByteRange::new(0, Some(16 * MIB - 1))).budget(0);
    assert_eq!(chunk.max_answer, 16 * MIB);
    assert_eq!(chunk.recv_body, Some(Duration::from_secs(30 + 512)));
    // A few bytes may still come back as an error document.
    let few = CallLimits::range(ByteRange::new(100, Some(109))).budget(0);
    assert_eq!(few.max_answer, ANSWER_FLOOR);
    // A range to the end has no known length: it reads like a whole object.
    assert_eq!(
        CallLimits::range(ByteRange::new(5, None)),
        CallLimits::whole_object()
    );
}

#[test]
fn a_document_call_has_a_minute_and_a_capped_answer() {
    let json = CallLimits::json().budget(0);
    assert_eq!(json.overall, Duration::from_secs(60));
    assert_eq!((json.send_body, json.recv_body), (None, None));
    assert_eq!(json.max_answer, MIB);
    assert_eq!(CallLimits::listing().budget(0).max_answer, 16 * MIB);
    assert_eq!(JSON_MAX, MIB);
    assert_eq!(LISTING_MAX, 16 * MIB);
}

/// The calls a drive makes, with the limits each one carries.
fn limits_of(fake: &Fake) -> Vec<(crate::Method, CallLimits)> {
    fake.calls()
        .into_iter()
        .map(|call| (call.method, call.limits))
        .collect()
}

fn drive_on(fake: &Fake) -> S3Drive {
    S3Drive::new(
        S3Config {
            endpoint: String::from("http://127.0.0.1:9000"),
            region: String::from("us-east-1"),
            bucket: BUCKET.to_string(),
            path_style: true,
        },
        Credentials::new("AKIDTEST", "test-secret"),
        Box::new(fake.clone()),
    )
    .unwrap()
    .with_clock(|| 1_791_590_400)
}

#[test]
fn every_call_of_a_bucket_says_what_it_may_take_and_what_its_answer_may_weigh() {
    let fake = Fake::default();
    fake.answer(
        200,
        &[],
        "<ListBucketResult><IsTruncated>false</IsTruncated></ListBucketResult>",
    )
    .answer(200, &[("Content-Length", "3")], "")
    .answer(206, &[], "abc")
    .answer(200, &[("ETag", "\"e\"")], "")
    .answer(200, &[], "whole");
    let drive = drive_on(&fake);
    drive.list(&ListRequest::default()).unwrap();
    drive.head("a.txt").unwrap();
    drive.get_range("a.txt", ByteRange::new(0, Some(2))).unwrap();
    drive.put("a.txt", b"abc").unwrap();
    drive.get("a.txt").unwrap();
    assert_eq!(
        limits_of(&fake),
        vec![
            (crate::Method::Get, CallLimits::listing()),
            (crate::Method::Head, CallLimits::json()),
            (
                crate::Method::Get,
                CallLimits::range(ByteRange::new(0, Some(2)))
            ),
            (crate::Method::Put, CallLimits::upload()),
            (crate::Method::Get, CallLimits::whole_object()),
        ]
    );
}

#[test]
fn a_token_endpoints_answer_is_capped_as_a_document() {
    let fake = Fake::default();
    fake.answer(
        200,
        &[("Content-Type", "application/json")],
        r#"{"access_token":"a","token_type":"Bearer"}"#,
    );
    refresh(
        &fake,
        "https://oauth2.example/token",
        &OAuthClient::public("id"),
        "1//refresh",
    )
    .unwrap();
    assert_eq!(fake.last().limits, CallLimits::json());
}

#[test]
fn the_parts_of_an_upload_follow_the_link() {
    let ceiling = PART_SIZE;
    // Nothing measured yet: the drive's own part size and parallel parts.
    assert_eq!(plan_parts(None, ceiling, 4), (PART_SIZE, 4));
    // 1 MiB/s shared by four parts: each gets 256 KiB/s, a minute of it is 15 MiB.
    assert_eq!(
        plan_parts(Some(BytesPerSec(MIB)), ceiling, 4),
        (15 * MIB as usize, 4)
    );
    // Below 2 Mbit/s one part at a time, sized to a minute of the whole link.
    assert_eq!(
        plan_parts(Some(BytesPerSec(187_500)), ceiling, 4),
        (10 * MIB as usize, 1)
    );
    // A very slow link still sends parts of at least 5 MiB (S3's smallest).
    assert_eq!(
        plan_parts(Some(BytesPerSec(62_500)), ceiling, 4),
        (MIN_PART_SIZE, 1)
    );
    assert_eq!(MIN_PART_SIZE, 5 * MIB as usize);
    // A drive configured with small parts keeps them.
    assert_eq!(plan_parts(Some(BytesPerSec(62_500)), 1024, 4), (1024, 1));
}

#[test]
fn the_link_is_estimated_from_the_uploads_that_moved_a_mebibyte_or_more() {
    let link = LinkEstimate::default();
    assert_eq!(link.rate(), None);
    link.sample(1024, Duration::from_millis(500), 1);
    assert_eq!(link.rate(), None, "a small body measures the latency, not the link");
    // Four parts of 4 MiB at once, each in 16 s: the link moves 1 MiB/s.
    link.sample(4 * MIB as usize, Duration::from_secs(16), 4);
    assert_eq!(link.rate(), Some(BytesPerSec(MIB)));
    // Later samples move the estimate towards them.
    link.sample(4 * MIB as usize, Duration::from_secs(1), 1);
    let rate = link.rate().unwrap().0;
    assert!(rate > MIB && rate < 4 * MIB, "{rate}");
}
