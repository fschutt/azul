//! The bridge's drive: the user's Azlin drive through a device account of the bridge's own.
//!
//! The bridge is a device like any other (azcloud-kit's [`Account`] in its state folder): it
//! signs up (a development server) or joins a drive with a code from `azcloud invite` / AzDrive,
//! and so holds its OWN token family - sharing AzMail's or AzDrive's drive token would make each
//! refresh spend the other's token, and the token server revokes a family when a spent token
//! comes back. The credentials are renewed under the state folder's lock six hours before they
//! run out ([`Account::ensure_fresh`]), and once more when the bucket refuses them.

use std::sync::{Arc, Mutex};

use azcloud_kit::{
    drive::TransportFactory,
    error::{CloudError, CloudResult},
    Account, StateDir,
};
use azul_storage::{
    ByteRange, Drive, DriveError, ListPage, ListRequest, ObjectInfo, Precondition, S3Config,
    S3Drive,
};

/// The S3 error codes that mean "these credentials no longer work".
const REFUSED_CREDENTIALS: &[&str] = &[
    "ExpiredToken",
    "InvalidToken",
    "TokenRefreshRequired",
    "InvalidAccessKeyId",
];

fn drive_error(e: &CloudError) -> DriveError {
    if e.is_sign_in() {
        return DriveError::Denied {
            message: e.to_string(),
        };
    }
    match e.root() {
        CloudError::Drive(inner) => inner.clone(),
        _ => DriveError::Transport(e.to_string()),
    }
}

/// The drive of the bridge's device account, refreshing its credentials by itself.
pub struct AccountDrive {
    account: Mutex<Account>,
    transports: TransportFactory,
    /// An S3 endpoint to use instead of the token server's (`--s3-url`).
    s3_override: Option<String>,
    /// The bucket of the current credentials, by their access key.
    bucket: Mutex<Option<(String, Arc<S3Drive>)>>,
}

impl std::fmt::Debug for AccountDrive {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AccountDrive").finish_non_exhaustive()
    }
}

impl AccountDrive {
    /// The drive this device holds in `state` (its current one), refreshed at `token_url`.
    /// Sends nothing.
    ///
    /// # Errors
    ///
    /// When the state folder holds no drive (sign up or join first).
    pub fn open(
        state: &StateDir,
        token_url: &str,
        transports: TransportFactory,
        s3_override: Option<String>,
    ) -> CloudResult<AccountDrive> {
        let account = Account::open(state, token_url, transports.clone(), None)?;
        Ok(AccountDrive {
            account: Mutex::new(account),
            transports,
            s3_override: s3_override.filter(|s| !s.trim().is_empty()),
            bucket: Mutex::new(None),
        })
    }

    /// The drive's id (for people).
    #[must_use]
    pub fn drive_id(&self) -> String {
        self.account
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .record()
            .id
            .clone()
    }

    /// The bucket with credentials that work now; `force` renews them first.
    fn current(&self, force: bool) -> Result<(Arc<S3Drive>, String), DriveError> {
        let mut account = self
            .account
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if force {
            account.refresh().map_err(|e| drive_error(&e))?;
        } else {
            account.ensure_fresh().map_err(|e| drive_error(&e))?;
        }
        let credentials = account.credentials().map_err(|e| drive_error(&e))?;
        let key = credentials.access_key_id.clone();
        let mut bucket = self
            .bucket
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if let Some((cached_key, drive)) = bucket.as_ref() {
            if *cached_key == key {
                return Ok((drive.clone(), key));
            }
        }
        let record = account.record();
        let config = S3Config {
            endpoint: self
                .s3_override
                .clone()
                .unwrap_or_else(|| record.endpoint.clone()),
            region: if record.region.trim().is_empty() {
                String::from("us-east-1")
            } else {
                record.region.clone()
            },
            bucket: record.bucket.clone(),
            path_style: record.path_style,
        };
        let drive = Arc::new(S3Drive::new(config, credentials, (self.transports)())?);
        *bucket = Some((key.clone(), drive.clone()));
        Ok((drive, key))
    }

    /// Runs `call` on the bucket; when the bucket refuses the credentials, renews them and
    /// runs it once more.
    fn with_bucket<T>(&self, call: impl Fn(&S3Drive) -> Result<T, DriveError>) -> Result<T, DriveError> {
        let (drive, _) = self.current(false)?;
        match call(&drive) {
            Err(DriveError::Service(service))
                if REFUSED_CREDENTIALS.contains(&service.code.as_str()) =>
            {
                let (drive, _) = self.current(true)?;
                call(&drive)
            }
            other => other,
        }
    }
}

impl Drive for AccountDrive {
    fn list(&self, request: &ListRequest) -> Result<ListPage, DriveError> {
        self.with_bucket(|b| b.list(request))
    }
    fn get(&self, key: &str) -> Result<Vec<u8>, DriveError> {
        self.with_bucket(|b| b.get(key))
    }
    fn get_range(&self, key: &str, range: ByteRange) -> Result<Vec<u8>, DriveError> {
        self.with_bucket(|b| b.get_range(key, range))
    }
    fn put(&self, key: &str, bytes: &[u8]) -> Result<(), DriveError> {
        self.with_bucket(|b| b.put(key, bytes))
    }
    // The bucket's conditional write (S3 If-None-Match / If-Match): what an encrypted drive's
    // index commits with; the default of the trait would refuse it.
    fn put_if(
        &self,
        key: &str,
        bytes: &[u8],
        condition: &Precondition,
    ) -> Result<Option<String>, DriveError> {
        self.with_bucket(|b| b.put_if(key, bytes, condition))
    }
    // The bucket's streamed write (parts for a big file). Once: the body cannot be read again for
    // a retry, so the credentials are made fresh before it starts.
    fn put_from(&self, key: &str, body: &mut dyn std::io::Read) -> Result<u64, DriveError> {
        let (drive, _) = self.current(false)?;
        drive.put_from(key, body)
    }
    fn delete(&self, key: &str) -> Result<(), DriveError> {
        self.with_bucket(|b| b.delete(key))
    }
    fn head(&self, key: &str) -> Result<ObjectInfo, DriveError> {
        self.with_bucket(|b| b.head(key))
    }
    fn copy(&self, from: &str, to: &str) -> Result<(), DriveError> {
        self.with_bucket(|b| b.copy(from, to))
    }
    fn create_folder(&self, prefix: &str) -> Result<(), DriveError> {
        self.with_bucket(|b| b.create_folder(prefix))
    }
    fn rename(&self, from: &str, to: &str) -> Result<(), DriveError> {
        self.with_bucket(|b| b.rename(from, to))
    }
    fn delete_folder(&self, prefix: &str) -> Result<(), DriveError> {
        self.with_bucket(|b| b.delete_folder(prefix))
    }
    fn metadata(&self, key: &str) -> Result<Vec<(String, String)>, DriveError> {
        self.with_bucket(|b| b.metadata(key))
    }
}
