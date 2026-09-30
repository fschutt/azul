//! An S3-compatible bucket as a drive: AWS S3, Cloudflare R2, MinIO.
//!
//! Five calls of the S3 API, each signed with SigV4: ListObjectsV2 (with
//! continuation tokens), GetObject (with `Range`), PutObject, DeleteObject and
//! HeadObject. Error answers become [`ServiceError`]s that say what the
//! service said.

use std::fmt;

use crate::{
    ByteRange, Drive, DriveError, ListPage, ListRequest, ObjectInfo, ServiceError, Transport,
};

/// Where the bucket is.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct S3Config {
    /// `https://s3.eu-central-1.amazonaws.com`, `https://<account>.r2.cloudflarestorage.com`,
    /// `http://127.0.0.1:9000`. A path after the host is kept in front of the bucket.
    pub endpoint: String,
    /// `us-east-1`; R2 takes `auto`.
    pub region: String,
    pub bucket: String,
    /// `true`: `<endpoint>/<bucket>/<key>` (MinIO, local servers); `false`:
    /// `<bucket>.<endpoint host>/<key>` (virtual-host style, AWS's default).
    pub path_style: bool,
}

/// An access key. `Debug` never shows any of it.
#[derive(Clone, PartialEq, Eq)]
pub struct Credentials {
    pub access_key_id: String,
    pub secret_access_key: String,
    /// For temporary credentials (STS); sent as `x-amz-security-token`.
    pub session_token: Option<String>,
}

impl Credentials {
    #[must_use]
    pub fn new(access_key_id: &str, secret_access_key: &str) -> Self {
        Credentials {
            access_key_id: access_key_id.to_string(),
            secret_access_key: secret_access_key.to_string(),
            session_token: None,
        }
    }

    #[must_use]
    pub fn with_session_token(mut self, token: &str) -> Self {
        self.session_token = Some(token.to_string());
        self
    }

    /// The one string stored in the OS keyring for a drive (JSON).
    #[must_use]
    pub fn to_keyring_secret(&self) -> String {
        todo!("RED")
    }

    /// Reads [`Self::to_keyring_secret`] back.
    pub fn from_keyring_secret(secret: &str) -> Result<Self, DriveError> {
        let _ = secret;
        todo!("RED")
    }
}

impl fmt::Debug for Credentials {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let _ = f;
        todo!("RED")
    }
}

/// A bucket, reached through a [`Transport`].
pub struct S3Drive {
    config: S3Config,
    credentials: Credentials,
    transport: Box<dyn Transport>,
    clock: Box<dyn Fn() -> u64 + Send + Sync>,
}

impl fmt::Debug for S3Drive {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("S3Drive")
            .field("config", &self.config)
            .field("credentials", &self.credentials)
            .finish_non_exhaustive()
    }
}

impl S3Drive {
    /// Checks the endpoint and the bucket name; sends nothing.
    pub fn new(
        config: S3Config,
        credentials: Credentials,
        transport: Box<dyn Transport>,
    ) -> Result<Self, DriveError> {
        let _ = (&config, &credentials, &transport);
        todo!("RED")
    }

    /// Signs with this clock (seconds since 1970) instead of the system's.
    #[must_use]
    pub fn with_clock(mut self, clock: impl Fn() -> u64 + Send + Sync + 'static) -> Self {
        self.clock = Box::new(clock);
        self
    }

    #[must_use]
    pub fn config(&self) -> &S3Config {
        &self.config
    }
}

impl Drive for S3Drive {
    fn list(&self, request: &ListRequest) -> Result<ListPage, DriveError> {
        let _ = request;
        todo!("RED")
    }

    fn get(&self, key: &str) -> Result<Vec<u8>, DriveError> {
        let _ = key;
        todo!("RED")
    }

    fn get_range(&self, key: &str, range: ByteRange) -> Result<Vec<u8>, DriveError> {
        let _ = (key, range);
        todo!("RED")
    }

    fn put(&self, key: &str, bytes: &[u8]) -> Result<(), DriveError> {
        let _ = (key, bytes);
        todo!("RED")
    }

    fn delete(&self, key: &str) -> Result<(), DriveError> {
        let _ = key;
        todo!("RED")
    }

    fn head(&self, key: &str) -> Result<ObjectInfo, DriveError> {
        let _ = (key, ServiceError::default());
        todo!("RED")
    }
}
