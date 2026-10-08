//! The kit's bucket against an S3 service in memory: one winner per ETag, a big object in parts
//! and ranges, listings page by page, the endpoint failover.

use std::sync::Arc;

use azul_storage::{Credentials, DriveError, HttpReply, S3Config, Transport};

use super::{
    fake_s3::{FakeS3, S3Bucket, BUCKET},
    Fake, Shared, S3,
};
use crate::{
    bucket::Bucket,
    drive::TransportFactory,
    store::{Conditional, RemoteStore},
    CloudError,
};

#[test]
fn a_bucket_has_one_winner_per_etag_like_the_real_one() {
    let s = S3Bucket::new();
    let first = s.bucket.put_if("i", b"1", None).unwrap();
    assert!(first.is_some(), "the first create wins");
    assert!(s.bucket.put_if("i", b"x", None).unwrap().is_none());
    let etag = first.unwrap();
    let second = s.bucket.put_if("i", b"2", Some(&etag)).unwrap();
    assert!(second.is_some());
    assert!(
        s.bucket.put_if("i", b"3", Some(&etag)).unwrap().is_none(),
        "a stale ETag loses"
    );
    match s.bucket.get_unless("i", second.as_deref()).unwrap() {
        Conditional::NotModified => {}
        other => panic!("{other:?}"),
    }
    s.before_next_cas(|s| {
        s.write("i", b"from another device".to_vec());
    });
    assert!(
        s.bucket
            .put_if("i", b"4", second.as_deref())
            .unwrap()
            .is_none(),
        "the write that landed in between wins"
    );
    assert_eq!(s.read("i").unwrap(), b"from another device");
    match s.bucket.get_unless("i", second.as_deref()).unwrap() {
        Conditional::Found { body, etag } => {
            assert_eq!(body, b"from another device");
            assert!(etag.is_some_and(|e| e.starts_with('"')), "the ETag as sent");
        }
        other => panic!("{other:?}"),
    }
    assert_eq!(
        s.bucket.get_unless("nothing", None).unwrap(),
        Conditional::NotFound
    );
}

#[test]
fn a_big_object_goes_up_in_parts_and_comes_down_in_ranges() {
    let s3 = FakeS3::new();
    let mut bucket = s3.bucket();
    bucket.set_part_size(4);
    let data: Vec<u8> = (0..23u8).collect();
    let etag = bucket.put("big.bin", &data).unwrap();
    assert!(!etag.is_empty());
    assert_eq!(s3.read("big.bin").unwrap(), data);
    assert_eq!(
        s3.count("POST"),
        2,
        "started and completed: {:?}",
        s3.log.lock().unwrap()
    );
    assert_eq!(s3.count("PUT"), 6, "23 bytes in parts of 4");
    s3.clear_log();
    assert_eq!(bucket.get_big("big.bin").unwrap().unwrap(), data);
    assert_eq!(s3.count("HEAD"), 1);
    assert_eq!(s3.count("GET"), 6, "six ranges");
    assert_eq!(bucket.get_big("nothing").unwrap(), None);
    let small = vec![1u8, 2, 3];
    bucket.put("small.bin", &small).unwrap();
    assert_eq!(
        RemoteStore::fetch(&bucket, "small.bin", 3).unwrap().unwrap(),
        small
    );
    assert_eq!(RemoteStore::head(&bucket, "small.bin").unwrap(), Some(3));
    assert_eq!(RemoteStore::head(&bucket, "nothing").unwrap(), None);
}

#[test]
fn a_listing_comes_page_by_page_and_deleting_nothing_is_done() {
    let s = S3Bucket::new();
    for i in 0..1005 {
        s.write(&format!("p/{i:04}"), vec![1]);
    }
    s.write("q/other", vec![2, 2]);
    let listed = s.bucket.list_all("p/").unwrap();
    assert_eq!(listed.len(), 1005);
    assert_eq!(s.count("LIST"), 2, "two pages of at most 1000");
    assert_eq!(listed[0].key, "p/0000");
    assert_eq!(listed[0].size, 1);
    assert!(listed[0].modified.is_some());
    s.bucket.delete("p/0000").unwrap();
    s.bucket.delete("p/0000").unwrap();
    assert!(s.read("p/0000").is_none());
    let objects = RemoteStore::list(&s, "q/").unwrap();
    assert_eq!(objects.len(), 1);
    assert_eq!(objects[0].key, "q/other");
    assert_eq!(objects[0].size, 2);
}

fn bucket_on(fake: &Arc<Fake>) -> Bucket {
    let fake = fake.clone();
    let factory: TransportFactory =
        Arc::new(move || Box::new(Shared(fake.clone())) as Box<dyn Transport>);
    Bucket::new(
        S3Config {
            endpoint: S3.to_string(),
            region: String::from("us-east-1"),
            bucket: BUCKET.to_string(),
            path_style: true,
        },
        Credentials::new("AKID1", "secret-of-AKID1"),
        factory,
    )
    .unwrap()
}

#[test]
fn a_block_endpoint_without_an_answer_hands_the_request_to_the_next_endpoint() {
    let s3 = FakeS3::new();
    s3.write("a.txt", b"alpha".to_vec());
    let service = s3.clone();
    let fake = Fake::new(move |call, _| {
        if call.url.starts_with(S3) {
            return Err(String::from("connection refused"));
        }
        if call.url.starts_with("http://127.0.0.1:19003") {
            return Ok(HttpReply {
                status: 503,
                headers: Vec::new(),
                body: Vec::new(),
            });
        }
        let mut answer = service.answer(call);
        answer.headers.push((
            String::from("x-azlin-alt-endpoints"),
            String::from("http://127.0.0.1:19004, http://127.0.0.1:19002"),
        ));
        Ok(answer)
    });
    let bucket = bucket_on(&fake).with_alternatives(vec![
        String::from("http://127.0.0.1:19003"),
        String::from("http://127.0.0.1:19002/"),
    ]);
    assert_eq!(bucket.get("a.txt").unwrap().unwrap(), b"alpha");
    let asked: Vec<String> = fake.calls().iter().map(|c| c.url.clone()).collect();
    assert_eq!(asked.len(), 3, "{asked:?}");
    assert!(asked[0].starts_with(S3), "{asked:?}");
    assert!(asked[1].starts_with("http://127.0.0.1:19003/"), "{asked:?}");
    assert!(asked[2].starts_with("http://127.0.0.1:19002/"), "{asked:?}");
    assert_eq!(
        bucket.alternatives(),
        vec![
            "http://127.0.0.1:19003",
            "http://127.0.0.1:19002",
            "http://127.0.0.1:19004"
        ],
        "what an answer names is asked next time too, once"
    );
}

#[test]
fn a_bucket_no_endpoint_answers_says_so_and_a_503_of_the_last_one_is_the_services_answer() {
    let dead = Fake::new(|_, _| Err(String::from("connection refused")));
    match bucket_on(&dead).get("a.txt") {
        Err(CloudError::Drive(DriveError::Transport(why))) => {
            assert!(why.contains(S3) && why.contains("refused"), "{why}")
        }
        other => panic!("not a connection error: {other:?}"),
    }
    let busy = Fake::new(|_, _| {
        Ok(HttpReply {
            status: 503,
            headers: Vec::new(),
            body: b"<Error><Code>SlowDown</Code><Message>later</Message></Error>".to_vec(),
        })
    });
    match bucket_on(&busy).put("a.txt", b"x") {
        Err(CloudError::Drive(DriveError::Service(e))) => assert_eq!(e.code, "SlowDown"),
        other => panic!("not the service's refusal: {other:?}"),
    }
    assert_eq!(busy.calls().len(), 1, "no alternatives, one request");
    assert!(
        bucket_on(&busy).probe(".azlin/probe").is_ok(),
        "any answer means the pipe works"
    );
    assert!(bucket_on(&dead).probe(".azlin/probe").is_err());
}
