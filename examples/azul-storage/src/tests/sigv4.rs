//! SigV4 against AWS's published vectors: the aws-sig-v4-test-suite (service
//! "service", 2015-08-30, credentials AKIDEXAMPLE) and the worked examples of
//! the S3 API reference, "Signature Calculations for the Authorization Header"
//! (bucket examplebucket, 2013-05-24, credentials AKIAIOSFODNN7EXAMPLE).

use crate::sigv4::{self, Signed, SigningParams};

const SUITE_ACCESS_KEY: &str = "AKIDEXAMPLE";
const SUITE_SECRET: &str = "wJalrXUtnFEMI/K7MDENG+bPxRfiCYEXAMPLEKEY";
const SUITE_DATE: &str = "20150830T123600Z";

const S3_ACCESS_KEY: &str = "AKIAIOSFODNN7EXAMPLE";
const S3_SECRET: &str = "wJalrXUtnFEMI/K7MDENG/bPxRfiCYEXAMPLEKEY";
const S3_DATE: &str = "20130524T000000Z";
const S3_HOST: &str = "examplebucket.s3.amazonaws.com";

fn pairs(list: &[(&str, &str)]) -> Vec<(String, String)> {
    list.iter()
        .map(|(k, v)| (k.to_string(), v.to_string()))
        .collect()
}

/// One request of the test suite: host and x-amz-date plus `headers`.
fn suite(method: &str, uri: &str, query: &[(&str, &str)], headers: &[(&str, &str)]) -> Signed {
    let mut all = pairs(&[
        ("Host", "example.amazonaws.com"),
        ("X-Amz-Date", SUITE_DATE),
    ]);
    all.extend(pairs(headers));
    sigv4::sign(
        &SigningParams {
            access_key_id: SUITE_ACCESS_KEY,
            secret_access_key: SUITE_SECRET,
            region: "us-east-1",
            service: "service",
            amz_date: SUITE_DATE,
        },
        method,
        uri,
        &pairs(query),
        &all,
        sigv4::EMPTY_SHA256,
    )
}

/// One request of the S3 reference examples.
fn s3_example(
    method: &str,
    uri: &str,
    query: &[(&str, &str)],
    headers: &[(&str, &str)],
    payload_hash: &str,
) -> Signed {
    sigv4::sign(
        &SigningParams {
            access_key_id: S3_ACCESS_KEY,
            secret_access_key: S3_SECRET,
            region: "us-east-1",
            service: "s3",
            amz_date: S3_DATE,
        },
        method,
        uri,
        &pairs(query),
        &pairs(headers),
        payload_hash,
    )
}

#[test]
fn sha256_of_nothing_is_the_empty_payload_hash() {
    assert_eq!(sigv4::sha256_hex(b""), sigv4::EMPTY_SHA256);
    assert_eq!(
        sigv4::EMPTY_SHA256,
        "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
    );
}

#[test]
fn get_vanilla_has_the_suites_canonical_request() {
    assert_eq!(
        suite("GET", "/", &[], &[]).canonical_request,
        "GET\n/\n\nhost:example.amazonaws.com\nx-amz-date:20150830T123600Z\n\nhost;x-amz-date\n\
         e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
    );
}

#[test]
fn get_vanilla_has_the_suites_string_to_sign() {
    assert_eq!(
        suite("GET", "/", &[], &[]).string_to_sign,
        "AWS4-HMAC-SHA256\n20150830T123600Z\n20150830/us-east-1/service/aws4_request\n\
         bb579772317eb040ac9ed261061d46c1f17a8133879d6129b6e1c25292927e63"
    );
}

#[test]
fn get_vanilla_has_the_suites_signature_and_authorization_header() {
    let signed = suite("GET", "/", &[], &[]);
    assert_eq!(
        signed.signature,
        "5fa00fa31553b73ebf1942676e86291e8372ff2a2260956d9b8aae1d763fbf31"
    );
    assert_eq!(
        signed.authorization,
        "AWS4-HMAC-SHA256 Credential=AKIDEXAMPLE/20150830/us-east-1/service/aws4_request, \
         SignedHeaders=host;x-amz-date, \
         Signature=5fa00fa31553b73ebf1942676e86291e8372ff2a2260956d9b8aae1d763fbf31"
    );
}

#[test]
fn post_vanilla_signs_like_the_suite() {
    assert_eq!(
        suite("POST", "/", &[], &[]).signature,
        "5da7c1a2acd57cee7505fc6676e4e544621c30862966e37dddb68e92efbe5d6b"
    );
}

#[test]
fn query_parameters_are_sorted_by_key_before_signing() {
    let signed = suite(
        "GET",
        "/",
        &[("Param2", "value2"), ("Param1", "value1")],
        &[],
    );
    assert!(signed
        .canonical_request
        .starts_with("GET\n/\nParam1=value1&Param2=value2\n"));
    assert_eq!(
        signed.signature,
        "b97d918cfa904a5beff61c982a1b6f458b799221646efd99d3219ec94cdf2500"
    );
}

#[test]
fn a_single_query_parameter_signs_like_the_suite() {
    assert_eq!(
        suite("GET", "/", &[("Param1", "value1")], &[]).signature,
        "a67d582fa61cc504c4bae71f336f98b97f1ea3c7a6bfe1b6e45aec72011b9aeb"
    );
}

#[test]
fn unreserved_characters_stay_unencoded_in_the_path() {
    let path = "/-._~0123456789ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz";
    assert_eq!(sigv4::uri_encode(path, false), path);
    assert_eq!(
        suite("GET", path, &[], &[]).signature,
        "07ef7494c76fa4850883e2b006601f940f8a34d404d0cfa977f52a65bbf5f24f"
    );
}

#[test]
fn a_utf8_path_is_percent_encoded_byte_by_byte() {
    let path = sigv4::uri_encode("/\u{1234}", false);
    assert_eq!(path, "/%E1%88%B4");
    assert_eq!(
        suite("GET", &path, &[], &[]).signature,
        "8318018e0b0f223aa2bbf98705b62bb787dc9c0e678f255a891fd03141be5d85"
    );
}

#[test]
fn a_space_in_the_path_is_percent_20() {
    let path = sigv4::uri_encode("/example space/", false);
    assert_eq!(path, "/example%20space/");
    assert_eq!(
        suite("GET", &path, &[], &[]).signature,
        "652487583200325589f1fba4c7e578f72c47cb61beeca81406b39ddec1366741"
    );
}

#[test]
fn header_values_are_trimmed_and_inner_spaces_collapsed() {
    let signed = suite(
        "GET",
        "/",
        &[],
        &[("My-Header1", " value1 "), ("My-Header2", "\"a   b   c\"")],
    );
    assert!(signed
        .canonical_request
        .contains("my-header1:value1\nmy-header2:\"a b c\"\n"));
    assert_eq!(
        signed.signature,
        "acc3ed3afb60bb290fc8d2dd0098b9911fcaa05412b367055dee359757a9c736"
    );
}

#[test]
fn uri_encode_keeps_or_encodes_the_slash_as_asked() {
    assert_eq!(sigv4::uri_encode("a/b c+d=e", false), "a/b%20c%2Bd%3De");
    assert_eq!(sigv4::uri_encode("a/b c+d=e", true), "a%2Fb%20c%2Bd%3De");
    assert_eq!(sigv4::uri_encode("$", false), "%24");
}

#[test]
fn the_s3_get_object_example_with_a_range_signs_like_the_s3_reference() {
    let signed = s3_example(
        "GET",
        "/test.txt",
        &[],
        &[
            ("Host", S3_HOST),
            ("Range", "bytes=0-9"),
            ("x-amz-content-sha256", sigv4::EMPTY_SHA256),
            ("x-amz-date", S3_DATE),
        ],
        sigv4::EMPTY_SHA256,
    );
    assert_eq!(
        signed.string_to_sign,
        "AWS4-HMAC-SHA256\n20130524T000000Z\n20130524/us-east-1/s3/aws4_request\n\
         7344ae5b7ee6c3e7e6b0fe0640412a37625d1fbfff95c48bbb2dc43964946972"
    );
    assert_eq!(
        signed.signature,
        "f0e8bdb87c964420e857bd35b5d6ed310bd44f0170aba48dd91039c6036bdb41"
    );
}

#[test]
fn the_s3_put_object_example_signs_like_the_s3_reference() {
    let body = b"Welcome to Amazon S3.";
    let payload = sigv4::sha256_hex(body);
    assert_eq!(
        payload,
        "44ce7dd67c959e0d3524ffac1771dfbba87d2b6b4b4e99e42034a8b803f8b072"
    );
    let signed = s3_example(
        "PUT",
        &sigv4::uri_encode("/test$file.text", false),
        &[],
        &[
            ("Date", "Fri, 24 May 2013 00:00:00 GMT"),
            ("Host", S3_HOST),
            ("x-amz-content-sha256", &payload),
            ("x-amz-date", S3_DATE),
            ("x-amz-storage-class", "REDUCED_REDUNDANCY"),
        ],
        &payload,
    );
    assert_eq!(
        signed.signature,
        "98ad721746da40c64f1a55b78f14c238d841ea1380cd77a1b5971af0ece108bd"
    );
}

#[test]
fn the_s3_list_objects_example_signs_like_the_s3_reference() {
    let signed = s3_example(
        "GET",
        "/",
        &[("max-keys", "2"), ("prefix", "J")],
        &[
            ("Host", S3_HOST),
            ("x-amz-content-sha256", sigv4::EMPTY_SHA256),
            ("x-amz-date", S3_DATE),
        ],
        sigv4::EMPTY_SHA256,
    );
    assert_eq!(
        signed.signature,
        "34b48302e7b5fa45bde8084f4b7868a86f0a534bc59db6670ed5711ef69dc6f7"
    );
}

#[test]
fn the_s3_get_lifecycle_example_signs_a_valueless_query_parameter() {
    let signed = s3_example(
        "GET",
        "/",
        &[("lifecycle", "")],
        &[
            ("Host", S3_HOST),
            ("x-amz-content-sha256", sigv4::EMPTY_SHA256),
            ("x-amz-date", S3_DATE),
        ],
        sigv4::EMPTY_SHA256,
    );
    assert!(signed.canonical_request.starts_with("GET\n/\nlifecycle=\n"));
    assert_eq!(
        signed.signature,
        "fea454ca298b7da1c68078a5d1bdbfbbe0d65c699e0f91ac7a200a0136783543"
    );
}
