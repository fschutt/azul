//! The two XML documents an S3-compatible service answers with that the drives
//! read: the ListObjectsV2 `ListBucketResult` and the `Error` body.
//! Elements are matched by local name, so the S3 namespace does not matter.

use roxmltree::{Document, Node};

use crate::{time::parse_iso8601, DriveError, ListPage, ObjectInfo, ServiceError};

/// The text of the first child element called `name` (empty for `<Name/>`).
fn child_text(node: Node<'_, '_>, name: &str) -> Option<String> {
    node.children()
        .find(|c| c.is_element() && c.tag_name().name() == name)
        .map(|c| c.text().unwrap_or("").to_string())
}

/// The text of the first element called `name` anywhere in `xml` (any namespace), trimmed;
/// `None` when `xml` is no XML or has no such element.
pub(crate) fn first_text(xml: &str, name: &str) -> Option<String> {
    let doc = Document::parse(xml.trim()).ok()?;
    doc.descendants()
        .find(|n| n.is_element() && n.tag_name().name() == name)
        .map(|n| n.text().unwrap_or("").trim().to_string())
}

/// An ETag without its quotes.
pub(crate) fn strip_quotes(etag: &str) -> String {
    etag.trim().trim_matches('"').to_string()
}

/// A `ListBucketResult` to a page.
pub(crate) fn parse_list(xml: &str) -> Result<ListPage, DriveError> {
    let doc = Document::parse(xml.trim())
        .map_err(|e| DriveError::Protocol(format!("the listing is not XML ({e})")))?;
    let root = doc.root_element();
    if root.tag_name().name() != "ListBucketResult" {
        return Err(DriveError::Protocol(format!(
            "expected a ListBucketResult, got <{}>",
            root.tag_name().name()
        )));
    }
    let mut page = ListPage::default();
    for node in root.children().filter(|n| n.is_element()) {
        match node.tag_name().name() {
            "Contents" => {
                let key = child_text(node, "Key").ok_or_else(|| {
                    DriveError::Protocol(String::from("the listing has an object without a Key"))
                })?;
                page.objects.push(ObjectInfo {
                    key,
                    size: child_text(node, "Size")
                        .and_then(|s| s.trim().parse::<u64>().ok())
                        .unwrap_or(0),
                    modified: child_text(node, "LastModified").and_then(|s| parse_iso8601(&s)),
                    etag: child_text(node, "ETag")
                        .map(|e| strip_quotes(&e))
                        .filter(|e| !e.is_empty()),
                });
            }
            "CommonPrefixes" => {
                if let Some(prefix) = child_text(node, "Prefix").filter(|p| !p.is_empty()) {
                    page.folders.push(prefix);
                }
            }
            _ => {}
        }
    }
    let truncated = child_text(root, "IsTruncated")
        .map(|t| t.trim().eq_ignore_ascii_case("true"))
        .unwrap_or(false);
    if truncated {
        match child_text(root, "NextContinuationToken").filter(|t| !t.is_empty()) {
            Some(token) => page.next = Some(token),
            None => {
                return Err(DriveError::Protocol(String::from(
                    "the listing is truncated but has no NextContinuationToken",
                )))
            }
        }
    }
    Ok(page)
}

/// A name for an HTTP status, for error answers without a body (HEAD).
fn status_code_name(status: u16) -> &'static str {
    match status {
        301 => "PermanentRedirect",
        307 => "TemporaryRedirect",
        400 => "BadRequest",
        401 => "Unauthorized",
        403 => "Forbidden",
        404 => "NotFound",
        405 => "MethodNotAllowed",
        409 => "Conflict",
        412 => "PreconditionFailed",
        416 => "InvalidRange",
        500 => "InternalError",
        501 => "NotImplemented",
        503 => "ServiceUnavailable",
        _ => "HttpError",
    }
}

/// At most `max` characters of `text`, whitespace collapsed.
fn snippet(text: &str, max: usize) -> String {
    let collapsed = text.split_whitespace().collect::<Vec<_>>().join(" ");
    if collapsed.chars().count() <= max {
        collapsed
    } else {
        let cut: String = collapsed.chars().take(max).collect();
        format!("{cut}...")
    }
}

/// An error answer (`<Error><Code>..</Code><Message>..</Message>..</Error>`)
/// to a [`ServiceError`]; a body that is not one still gives the status.
pub(crate) fn parse_error(status: u16, body: &str) -> ServiceError {
    let mut error = ServiceError {
        status,
        ..ServiceError::default()
    };
    let parsed = Document::parse(body.trim()).ok();
    match parsed.as_ref().map(|doc| doc.root_element()) {
        Some(root) if root.tag_name().name() == "Error" => {
            error.code = child_text(root, "Code").unwrap_or_default();
            error.message = snippet(&child_text(root, "Message").unwrap_or_default(), 400);
            error.resource = child_text(root, "Resource").or_else(|| child_text(root, "Key"));
            error.request_id = child_text(root, "RequestId");
            error.region = child_text(root, "Region");
            error.endpoint = child_text(root, "Endpoint");
        }
        _ => {
            let text = snippet(body, 160);
            if !text.is_empty() && !text.starts_with('<') {
                error.message = text;
            }
        }
    }
    if error.code.is_empty() {
        error.code = status_code_name(status).to_string();
    }
    if error.message.is_empty() {
        error.message = format!("the service answered HTTP {status} without an S3 error body");
    }
    error
}
