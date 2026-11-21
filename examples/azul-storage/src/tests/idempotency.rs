//! Every request says whether it may be sent twice: a router re-sends the safe ones and the
//! conditional ones, and checks what an unsafe one did instead of sending it blind again. A
//! completion of a multipart upload whose answer was lost is found by its ETag (the MD5 of the
//! parts' MD5s and their count), so an upload that is done is never reported as failed.

use std::sync::Mutex;

use md5::{Digest, Md5};

use super::s3::Fake;
use crate::{
    s3::{multipart_etag, Idempotency, Routed, Router, Verify},
    Credentials, Drive, DriveError, HttpReply, Method, Precondition, S3Config, S3Drive, Transport,
};

/// md5("a") and md5("b"), as S3 names single parts.
const PART_A: &str = "0cc175b9c0f1b6a831c399e269772661";
const PART_B: &str = "92eb5ffee6ae2fec3ad71c777531578f";

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

fn unhex(text: &str) -> Vec<u8> {
    (0..text.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&text[i..i + 2], 16).unwrap())
        .collect()
}

/// What the node names an upload of parts a and b once it is complete.
fn completed_ab() -> String {
    let mut both = unhex(PART_A);
    both.extend(unhex(PART_B));
    format!("{}-2", hex(&Md5::digest(&both)))
}

fn parts_ab() -> Vec<(usize, String)> {
    vec![(1, format!("\"{PART_A}\"")), (2, PART_B.to_string())]
}

fn drive(fake: &Fake) -> S3Drive {
    S3Drive::new(
        S3Config {
            endpoint: String::from("http://127.0.0.1:9000"),
            region: String::from("us-east-1"),
            bucket: String::from("azdrive"),
            path_style: true,
        },
        Credentials::new("AKIDTEST", "test-secret"),
        Box::new(fake.clone()),
    )
    .expect("a valid configuration")
}

#[test]
fn a_multipart_etag_is_the_md5_of_the_part_md5s_and_their_count() {
    assert_eq!(multipart_etag(&parts_ab()), Some(completed_ab()));
    assert_eq!(
        multipart_etag(&[(1, String::from("\"part-1\""))]),
        None,
        "a part ETag that is no MD5 says nothing about the whole"
    );
    assert_eq!(multipart_etag(&[]), None);
}

#[test]
fn a_completion_whose_upload_is_gone_but_whose_object_is_there_succeeded() {
    // The first completion committed, its answer was lost; asked again the node knows the
    // upload no more - the object with the upload's ETag says it is done.
    let fake = Fake::default();
    fake.answer(
        404,
        &[],
        "<Error><Code>NoSuchUpload</Code><Message>no such upload</Message></Error>",
    );
    let etag = format!("\"{}\"", completed_ab());
    fake.answer(200, &[("etag", etag.as_str())], "");
    let done = drive(&fake).complete_multipart("big.bin", "up-1", &parts_ab(), None);
    assert_eq!(done.unwrap(), Some(completed_ab()));
    let calls = fake.calls();
    assert_eq!(calls.len(), 2);
    assert_eq!(calls[1].method, Method::Head);
}

#[test]
fn a_conditional_completion_refused_after_it_committed_is_done_and_another_writers_is_a_conflict(
) {
    let fake = Fake::default();
    let etag = format!("\"{}\"", completed_ab());
    fake.answer(412, &[], "<Error><Code>PreconditionFailed</Code></Error>");
    fake.answer(200, &[("etag", etag.as_str())], "");
    let free = Precondition::Absent;
    let done = drive(&fake).complete_multipart("big.bin", "up-1", &parts_ab(), Some(&free));
    assert_eq!(done.unwrap(), Some(completed_ab()), "the 412 was our own write");

    let fake = Fake::default();
    fake.answer(412, &[], "<Error><Code>PreconditionFailed</Code></Error>");
    fake.answer(200, &[("etag", "\"v7\"")], "");
    let lost = drive(&fake).complete_multipart("big.bin", "up-1", &parts_ab(), Some(&free));
    assert!(
        matches!(lost, Err(DriveError::Conflict { .. })),
        "another writer's version: {lost:?}"
    );
}

/// Answers every request with 200 and keeps what each said about being sent twice.
struct Records {
    seen: Mutex<Vec<(Method, Idempotency)>>,
}

impl Router for Records {
    fn send(
        &self,
        request: &Routed<'_>,
        _transport: &dyn Transport,
    ) -> Result<HttpReply, DriveError> {
        self.seen
            .lock()
            .unwrap()
            .push((request.method(), request.idempotency().clone()));
        Ok(HttpReply {
            status: 200,
            headers: vec![(String::from("etag"), String::from("\"v1\""))],
            body: b"<InitiateMultipartUploadResult><UploadId>up-1</UploadId>\
                    </InitiateMultipartUploadResult>"
                .to_vec(),
        })
    }
}

#[test]
fn every_request_says_whether_it_may_be_sent_twice() {
    let records = std::sync::Arc::new(Records {
        seen: Mutex::new(Vec::new()),
    });
    let drive = drive(&Fake::default()).with_router(records.clone());
    let _ = drive.get("a.txt");
    let _ = drive.put_if("a.txt", b"x", &Precondition::Absent);
    let _ = drive.start_multipart("big.bin");
    let _ = drive.complete_multipart("big.bin", "up-1", &parts_ab(), None);
    let _ = drive.delete("a.txt");
    let seen = records.seen.lock().unwrap().clone();
    assert_eq!(
        seen,
        vec![
            (Method::Get, Idempotency::Safe),
            (Method::Put, Idempotency::Conditional),
            (Method::Post, Idempotency::Unsafe(Verify::LeavesEmptyUpload)),
            (
                Method::Post,
                Idempotency::Unsafe(Verify::Completion {
                    etag: Some(completed_ab())
                })
            ),
            (Method::Delete, Idempotency::Safe),
        ]
    );
}
