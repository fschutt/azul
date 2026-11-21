//! A text that names a URL without the secret in it (AZ-CN3): pCloud's password and WebHDFS'
//! delegation token in the query, `user:password@` in an address, a token in a fragment - the
//! failure text of a request names the URL it tried, and the text is shown, printed and logged.

use crate::scrub::{scrub_url, HIDDEN};

/// The review's example, word for word: the red line under the Add drive form read
/// "Test failed: no answer from the storage service: .. https://api.pcloud.com/listfolder?path=/
/// &username=ann&password=hunter2: the name did not resolve".
#[test]
fn az_cn3_a_pcloud_url_loses_its_query_and_the_sentence_goes_on() {
    let said = "no answer from the storage service: .. \
                https://api.pcloud.com/listfolder?path=/&username=ann&password=hunter2: \
                the name did not resolve";
    let clean = scrub_url(said);
    assert_eq!(
        clean,
        "no answer from the storage service: .. \
         https://api.pcloud.com/listfolder?<hidden>: the name did not resolve"
    );
    assert!(
        !clean.contains("hunter2") && !clean.contains("ann"),
        "{clean}"
    );
}

#[test]
fn az_cn3_a_webhdfs_url_loses_its_delegation_token() {
    let said =
        "Connection failed: http://namenode:9870/webhdfs/v1/data?op=LISTSTATUS&user.name=ann\
                &delegation=HAAAABBCCDD";
    assert_eq!(
        scrub_url(said),
        "Connection failed: http://namenode:9870/webhdfs/v1/data?<hidden>"
    );
}

#[test]
fn az_cn3_the_user_and_password_before_the_at_sign_go_and_the_host_stays() {
    assert_eq!(
        scrub_url("cannot open https://ann:pw@cloud.example/dav/files"),
        "cannot open https://cloud.example/dav/files"
    );
    // A password may hold an '@' itself: the host is what follows the LAST one.
    assert_eq!(
        scrub_url("https://ann:p@ss@cloud.example:8443/dav"),
        "https://cloud.example:8443/dav"
    );
    // An IPv6 host and a port stay.
    assert_eq!(
        scrub_url("ftp://ann:pw@[::1]:2121/pub"),
        "ftp://[::1]:2121/pub"
    );
    // A user name alone is user info too.
    assert_eq!(scrub_url("http://ann@nas.local/x"), "http://nas.local/x");
}

#[test]
fn az_cn3_user_info_query_and_fragment_together_and_a_fragment_token() {
    assert_eq!(
        scrub_url("https://ann:pw@h.example/a/b?token=1&x=2#access_token=3"),
        "https://h.example/a/b?<hidden>#<hidden>"
    );
}

/// What the sentence around a URL says stays: quotes, brackets and the punctuation after it.
#[test]
fn az_cn3_the_words_around_a_url_stay() {
    assert_eq!(
        scrub_url(r#"uri: "https://x.example/p?password=1", status: 403"#),
        r#"uri: "https://x.example/p?<hidden>", status: 403"#
    );
    assert_eq!(
        scrub_url("(see https://u:p@x.example/a?k=v) and https://y.example/b."),
        "(see https://x.example/a?<hidden>) and https://y.example/b."
    );
    assert_eq!(
        scrub_url("two: https://a.example/?x=1 and https://b.example/y?z=2"),
        "two: https://a.example/?<hidden> and https://b.example/y?<hidden>"
    );
}

/// A secret written as a pair outside a URL loses its value; a pair that is no secret stays.
#[test]
fn az_cn3_a_secret_pair_outside_a_url_loses_its_value() {
    assert_eq!(
        scrub_url("rejected: password=hunter2, user=ann"),
        "rejected: password=<hidden>, user=ann"
    );
    assert_eq!(
        scrub_url("client_secret=abc&Access_Token=def refresh_token=ghi"),
        "client_secret=<hidden>&Access_Token=<hidden> refresh_token=<hidden>"
    );
    assert_eq!(
        scrub_url("X-Amz-Signature=0123abcd bypass=1 key=docs/a.txt monkey=2"),
        "X-Amz-Signature=<hidden> bypass=1 key=docs/a.txt monkey=2"
    );
    // The name before the equals sign, not a word that ends in one of the secret names by chance.
    assert_eq!(
        scrub_url("pass=1 pwd=2 sig=3"),
        "pass=<hidden> pwd=<hidden> sig=<hidden>"
    );
}

/// Text that holds no URL and no secret pair is returned as it is, and a scrubbed text scrubs to
/// itself.
#[test]
fn az_cn3_a_text_without_a_secret_is_returned_as_it_is_and_scrubbing_is_idempotent() {
    for plain in [
        "",
        "the name did not resolve",
        "timed out",
        "\"docs/a=b.txt\" is not a file",
        "Ränder: größer als ½ – über uns",
        "http:// is no address, nor is :// or ftp:/x",
    ] {
        assert_eq!(scrub_url(plain), plain);
    }
    for dirty in [
        "https://ann:pw@h.example/a?password=1#t=2",
        "password=1 https://h.example/?a=b: failed",
        "see https://api.pcloud.com/listfolder?path=/&password=hunter2",
    ] {
        let once = scrub_url(dirty);
        assert_eq!(scrub_url(&once), once, "{dirty}");
        assert!(once.contains(HIDDEN), "{once}");
    }
}

/// Multi-byte text before and after a URL does not break the cut.
#[test]
fn az_cn3_non_ascii_text_around_a_url_is_kept() {
    assert_eq!(
        scrub_url("Fehler bei https://münchen.example/ü?passwort=1&password=2 – nochmal"),
        "Fehler bei https://münchen.example/ü?<hidden> – nochmal"
    );
}

/// A request's Debug output goes to logs and failure messages: its URL is scrubbed like any text.
#[test]
fn az_cn3_the_debug_output_of_a_request_hides_the_query_of_its_url() {
    let call = crate::HttpCall {
        method: crate::Method::Get,
        url: String::from("https://api.pcloud.com/listfolder?path=/&username=ann&password=hunter2"),
        headers: Vec::new(),
        body: Vec::new(),
        content_type: String::new(),
        limits: crate::CallLimits::json(),
    };
    let debug = format!("{call:?}");
    assert!(!debug.contains("hunter2"), "{debug}");
    assert!(
        debug.contains("https://api.pcloud.com/listfolder?<hidden>"),
        "{debug}"
    );
    // The request itself is untouched: it is what is sent.
    assert!(call.url.contains("password=hunter2"));
}
