use crate::xml::{parse_error, parse_list};

const PAGE: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<ListBucketResult xmlns="http://s3.amazonaws.com/doc/2006-03-01/">
  <Name>azdrive</Name>
  <Prefix>mail/</Prefix>
  <KeyCount>3</KeyCount>
  <MaxKeys>3</MaxKeys>
  <Delimiter>/</Delimiter>
  <IsTruncated>true</IsTruncated>
  <Contents>
    <Key>mail/a&amp;b.eml</Key>
    <LastModified>2009-10-12T17:50:30.000Z</LastModified>
    <ETag>&quot;fba9dede5f27731c9771645a39863328&quot;</ETag>
    <Size>434234</Size>
    <StorageClass>STANDARD</StorageClass>
  </Contents>
  <Contents>
    <Key>mail/b.eml</Key>
    <LastModified>2009-10-12T17:50:31Z</LastModified>
    <ETag>"0123"</ETag>
    <Size>5</Size>
  </Contents>
  <CommonPrefixes>
    <Prefix>mail/inbox/</Prefix>
  </CommonPrefixes>
  <NextContinuationToken>1ueGcxLPRx1Tr/XYExHnhbYLgveDs2J/wm36Hy4vbOwM=</NextContinuationToken>
</ListBucketResult>"#;

#[test]
fn a_list_objects_v2_answer_parses_into_folders_objects_and_the_next_token() {
    let page = parse_list(PAGE).unwrap();
    assert_eq!(page.folders, vec!["mail/inbox/".to_string()]);
    assert_eq!(page.objects.len(), 2);
    let first = &page.objects[0];
    assert_eq!(first.key, "mail/a&b.eml");
    assert_eq!(first.size, 434_234);
    assert_eq!(first.modified, Some(1_255_369_830));
    assert_eq!(
        first.etag.as_deref(),
        Some("fba9dede5f27731c9771645a39863328")
    );
    assert_eq!(page.objects[1].etag.as_deref(), Some("0123"));
    assert_eq!(
        page.next.as_deref(),
        Some("1ueGcxLPRx1Tr/XYExHnhbYLgveDs2J/wm36Hy4vbOwM=")
    );
}

#[test]
fn a_final_page_has_no_next_token() {
    let xml = r#"<ListBucketResult xmlns="http://s3.amazonaws.com/doc/2006-03-01/">
        <Name>azdrive</Name><Prefix></Prefix><KeyCount>0</KeyCount><MaxKeys>1000</MaxKeys>
        <IsTruncated>false</IsTruncated></ListBucketResult>"#;
    let page = parse_list(xml).unwrap();
    assert!(page.objects.is_empty() && page.folders.is_empty());
    assert_eq!(page.next, None);
}

#[test]
fn an_answer_that_is_not_a_listing_is_a_protocol_error() {
    assert!(parse_list("<html>proxy login</html>").is_err());
    assert!(parse_list("not xml at all").is_err());
}

#[test]
fn an_s3_error_body_becomes_a_readable_error() {
    let body =
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<Error><Code>SignatureDoesNotMatch</Code>\
                <Message>The request signature we calculated does not match the signature you \
                provided. Check your key and signing method.</Message>\
                <RequestId>4442587FB7D0A2F9</RequestId></Error>";
    let error = parse_error(403, body);
    assert_eq!(error.status, 403);
    assert_eq!(error.code, "SignatureDoesNotMatch");
    assert_eq!(error.request_id.as_deref(), Some("4442587FB7D0A2F9"));
    let text = error.to_string();
    assert!(text.contains("SignatureDoesNotMatch"), "{text}");
    assert!(text.contains("does not match the signature"), "{text}");
    assert!(text.contains("403"), "{text}");
    assert!(
        text.contains("secret key"),
        "a hint names what to check: {text}"
    );
}

#[test]
fn a_wrong_region_error_names_the_region_it_wants() {
    let body = "<Error><Code>AuthorizationHeaderMalformed</Code><Message>The authorization \
                header is malformed; the region 'us-east-1' is wrong; expecting 'eu-west-1'\
                </Message><Region>eu-west-1</Region></Error>";
    let error = parse_error(400, body);
    assert_eq!(error.region.as_deref(), Some("eu-west-1"));
    assert!(error.to_string().contains("eu-west-1"));
}

#[test]
fn a_non_xml_error_body_still_names_the_status() {
    let empty = parse_error(500, "");
    assert_eq!(empty.status, 500);
    assert!(empty.to_string().contains("500"));
    let html = parse_error(502, "<html><body>Bad gateway</body></html>");
    assert!(html.to_string().contains("502"));
}
