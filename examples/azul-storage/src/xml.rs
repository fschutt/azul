//! The two XML documents an S3-compatible service answers with that the drives
//! read: the ListObjectsV2 `ListBucketResult` and the `Error` body.

use crate::{DriveError, ListPage, ServiceError};

/// A `ListBucketResult` to a page.
pub(crate) fn parse_list(xml: &str) -> Result<ListPage, DriveError> {
    let _ = xml;
    todo!("RED")
}

/// An error answer (`<Error><Code>..</Code><Message>..</Message>..</Error>`)
/// to a [`ServiceError`]; a body that is not one still gives the status.
pub(crate) fn parse_error(status: u16, body: &str) -> ServiceError {
    let _ = (status, body);
    todo!("RED")
}
