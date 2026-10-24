//! Shares of an encrypted drive's files (feature `encryption`): a link that opens chosen files
//! or folders and nothing else, for anyone who has it, without an account.
//!
//! - [`share`] makes a SHARE KEY, wraps each shared file's key with it (the drive key stays out
//!   of the share), seals the list - the [`ShareManifest`], names relative to the share - with a
//!   key derived from the share key and puts it at `.azlin/shares/<id>.azs`.
//! - **Presigned links (the default):** with a [`Presigner`] (the drive's `S3Drive`), every
//!   entry carries a presigned GET of its object and the link carries a presigned GET of the
//!   manifest, both for at most seven days (S3's limit, [`MAX_PRESIGNED_SECS`]). The link is
//!   `<viewer>#m=<the manifest's URL>&k=<the share key>` ([`share_link`]): the fragment never
//!   reaches a server, so the bucket serves ciphertext to whoever has the link and learns
//!   neither names nor contents. A presigned URL lasts as long as the credentials that signed it
//!   too: credentials that expire sooner (temporary ones) end the link sooner.
//! - **Longer links (later): the token server's share endpoint**, which hands out fresh
//!   presigned URLs per visit. Its contract:
//!
//! ```text
//!   POST   /v1/drives/{drive}/shares          drive token
//!          {"share": "<id, 32 hex>", "objects": ["<object id, 32 hex>", ...],
//!           "expires_at": "<RFC 3339>" | null}
//!          -> 201 {"share": "<id>", "path": "/s/<id>"}
//!          The server keeps id -> drive, the object ids and the expiry: never a name or a key.
//!   GET    /v1/shares/{id}                    no authentication; rate-limited, abuse caps
//!          -> 200 {"manifest_url": "<presigned GET of .azlin/shares/<id>.azs, <= 1 h>",
//!                  "objects": {"<object id>": "<presigned GET, <= 1 h>", ...},
//!                  "expires_at": "<RFC 3339>" | null}
//!          -> 404 when revoked or expired, 429 when over the caps
//!   DELETE /v1/drives/{drive}/shares/{id}     drive token -> 204 (new visits fail at once)
//!   link:  https://<share host>/s/<id>#k=<64 hex share key>
//! ```
//!
//!   Such a share's entries carry no `url`: the viewer asks the endpoint for them.
//! - [`revoke_share`] deletes the manifest: new visits of a presigned link fail at once, but
//!   object URLs fetched before stay valid until they expire, and a file key read once stays
//!   known - re-encrypting the files is the strong revocation.
//! - A share is a snapshot: files added to a shared folder later are not in it (live folder
//!   shares need folder keys in the drive index).

use crate::{
    crypto::{
        azl1,
        share::{
            manifest_key, share_key_from_text, share_key_text, share_of_manifest_key,
            ShareEntry, ShareManifest, SHARES_PREFIX,
        },
        CryptoError, KeyId, ShareKey, Zeroizing,
    },
    encrypted::EncryptedDrive,
    key::check_path_key,
    ops::list_all,
    sigv4::uri_encode,
    time::now_unix,
    Drive, DriveError, S3Drive,
};

/// What S3 takes as a presigned URL's lifetime at most: seven days.
pub const MAX_PRESIGNED_SECS: u64 = 7 * 24 * 3600;

/// Signs links that download one object of the bucket without credentials.
pub trait Presigner {
    /// A GET of `key` that works for `expires_secs` seconds.
    fn presigned_get(&self, key: &str, expires_secs: u64) -> Result<String, DriveError>;
}

impl Presigner for S3Drive {
    fn presigned_get(&self, key: &str, expires_secs: u64) -> Result<String, DriveError> {
        self.presigned_get_url(key, expires_secs)
    }
}

/// A new share.
pub struct NewShare {
    /// The key that opens it: goes into the link, kept nowhere else.
    pub key: ShareKey,
    /// Its id (the share key's public id): the manifest's name, what revokes it.
    pub id: KeyId,
    pub manifest: ShareManifest,
    /// A presigned GET of the manifest (with a presigner).
    pub manifest_url: Option<String>,
}

impl std::fmt::Debug for NewShare {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("NewShare")
            .field("id", &self.id)
            .field("files", &self.manifest.entries.len())
            .finish_non_exhaustive()
    }
}

/// The last part of a path (`a/b/c.txt` -> `c.txt`, `a/b/` -> `b`).
fn last_part(path: &str) -> &str {
    path.trim_end_matches('/')
        .rsplit('/')
        .next()
        .unwrap_or(path)
}

/// Shares `paths` of `drive` - files, and folders (ending in `/`) with everything in them - as
/// one share named `title`, its links working until `expires` (seconds since 1970). A file is
/// named by its own name in the share, a folder's files by the folder's name and their path in
/// it. With `presign` the links are presigned (at most [`MAX_PRESIGNED_SECS`] from now, so an
/// `expires` is required and must be within them). Two shared files of the same name are
/// refused (`InvalidKey`).
pub fn share<D: Drive>(
    drive: &EncryptedDrive<D>,
    paths: &[&str],
    title: Option<&str>,
    expires: Option<u64>,
    presign: Option<&dyn Presigner>,
) -> Result<NewShare, DriveError> {
    let now = now_unix();
    let lifetime = match (presign.is_some(), expires) {
        (false, _) => None,
        (true, Some(at)) if at > now && at - now <= MAX_PRESIGNED_SECS => Some(at - now),
        (true, _) => {
            return Err(DriveError::InvalidConfig(String::from(
                "presigned links last seven days at most and need an end within them; longer \
                 links come with the token server's share endpoint",
            )))
        }
    };
    let mut files: Vec<(String, String)> = Vec::new(); // (path, name in the share)
    for &path in paths {
        if path.ends_with('/') {
            let folder = last_part(path);
            for (inside, entry) in drive.all_entries(path)? {
                if entry.object.is_some() {
                    files.push((inside.clone(), format!("{folder}/{}", &inside[path.len()..])));
                }
            }
        } else {
            check_path_key(path)?;
            files.push((path.to_string(), last_part(path).to_string()));
        }
    }
    let key = ShareKey::generate().map_err(|e| e.for_key("a new share"))?;
    let id = key.id();
    let mut entries: Vec<ShareEntry> = Vec::with_capacity(files.len());
    for (path, name) in files {
        if entries.iter().any(|e| e.name == name) {
            return Err(DriveError::InvalidKey {
                key: name,
                reason: "two shared files have this name",
            });
        }
        let index_entry = drive.entry(&path)?;
        let shared = drive.share_file(&path, &key)?;
        let url = match (presign, lifetime) {
            (Some(presigner), Some(secs)) => {
                Some(presigner.presigned_get(&shared.object.bucket_key(), secs)?)
            }
            _ => None,
        };
        let blake3 = index_entry
            .object
            .as_ref()
            .map(|o| o.blake3)
            .unwrap_or_default();
        entries.push(ShareEntry {
            name,
            object: shared.object,
            stored_size: shared.stored_size,
            size: index_entry.size,
            blake3,
            wrapped_key: shared.wrapped_key,
            url,
        });
    }
    let manifest = ShareManifest {
        created: now,
        expires,
        title: title.map(str::to_string),
        entries,
    };
    let at = manifest_key(&id);
    let sealed = manifest.seal(&key).map_err(|e| e.for_key(&at))?;
    drive.inner().put(&at, &sealed)?;
    let manifest_url = match (presign, lifetime) {
        (Some(presigner), Some(secs)) => Some(presigner.presigned_get(&at, secs)?),
        _ => None,
    };
    Ok(NewShare {
        key,
        id,
        manifest,
        manifest_url,
    })
}

/// A presigned share's link: `<viewer>#m=<manifest URL, percent-encoded>&k=<share key>`. A
/// secret (whoever has it reads the share).
#[must_use]
pub fn share_link(viewer: &str, manifest_url: &str, key: &ShareKey) -> Zeroizing<String> {
    Zeroizing::new(format!(
        "{}#m={}&k={}",
        viewer.trim_end_matches('#'),
        uri_encode(manifest_url, true),
        share_key_text(key).as_str()
    ))
}

fn percent_decode(text: &str) -> Option<String> {
    let bytes = text.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' {
            let hex = text.get(i + 1..i + 3)?;
            out.push(u8::from_str_radix(hex, 16).ok()?);
            i += 3;
        } else {
            out.push(bytes[i]);
            i += 1;
        }
    }
    String::from_utf8(out).ok()
}

/// The manifest URL and the share key of a [`share_link`].
#[must_use]
pub fn parse_share_link(link: &str) -> Option<(String, ShareKey)> {
    let (_, fragment) = link.trim().split_once('#')?;
    let (mut manifest, mut key) = (None, None);
    for pair in fragment.split('&') {
        match pair.split_once('=') {
            Some(("m", value)) => manifest = percent_decode(value),
            Some(("k", value)) => key = share_key_from_text(value),
            _ => {}
        }
    }
    Some((manifest?, key?))
}

/// The plaintext of a shared file from its object's bytes (from the bucket or its URL),
/// checked against the manifest's entry: `Corrupt` when the object is not the one the share
/// names.
pub fn open_shared_entry(
    object_bytes: &[u8],
    entry: &ShareEntry,
    key: &ShareKey,
) -> Result<Vec<u8>, DriveError> {
    let name = entry.name.as_str();
    let file_key = key
        .unwrap_file_key(&entry.wrapped_key, &entry.object)
        .map_err(|e| e.for_key(name))?;
    let plain = azl1::decrypt(object_bytes, &entry.object, &file_key).map_err(|e| e.for_key(name))?;
    if plain.len() as u64 != entry.size || *blake3::hash(&plain).as_bytes() != entry.blake3 {
        return Err(CryptoError::Damaged(String::from(
            "the object is not the file the share names",
        ))
        .for_key(name));
    }
    Ok(plain)
}

/// The shared file `entry` read from `bucket` (the drive's bucket, or one that reaches it).
pub fn read_shared_entry(
    bucket: &dyn Drive,
    entry: &ShareEntry,
    key: &ShareKey,
) -> Result<Vec<u8>, DriveError> {
    open_shared_entry(&bucket.get(&entry.object.bucket_key())?, entry, key)
}

/// The manifest of the share `key` opens, from `bucket`.
pub fn open_share(bucket: &dyn Drive, key: &ShareKey) -> Result<ShareManifest, DriveError> {
    let at = manifest_key(&key.id());
    ShareManifest::open(&bucket.get(&at)?, key).map_err(|e| e.for_key(&at))
}

/// The ids of the drive's shares (their manifests in `bucket`).
pub fn list_shares(bucket: &dyn Drive) -> Result<Vec<KeyId>, DriveError> {
    Ok(list_all(bucket, SHARES_PREFIX)?
        .iter()
        .filter_map(|object| share_of_manifest_key(&object.key))
        .collect())
}

/// Revokes the share `id`: its manifest leaves the bucket (see the module documentation for
/// what that does and does not take back).
pub fn revoke_share(bucket: &dyn Drive, id: &KeyId) -> Result<(), DriveError> {
    match bucket.delete(&manifest_key(id)) {
        Ok(()) | Err(DriveError::NotFound { .. }) => Ok(()),
        Err(e) => Err(e),
    }
}

/// Revokes every share of the drive (the key rotation after a compromise). Returns how many.
pub fn revoke_all_shares(bucket: &dyn Drive) -> Result<usize, DriveError> {
    let shares = list_shares(bucket)?;
    for id in &shares {
        revoke_share(bucket, id)?;
    }
    Ok(shares.len())
}
