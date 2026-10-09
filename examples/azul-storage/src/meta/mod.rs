//! The drive index: an encrypted metadata git repository in the bucket.
//!
//! A drive is a bucket: the file bytes as encrypted data objects with random
//! names, and ONE metadata repository that knows what they are. The repository
//! holds a pointer file per drive file (at the file's path: the data object's
//! key, the plaintext size, the mtime, the BLAKE3 of the plaintext and the file
//! key wrapped by the drive key), the folders as git trees, the drive's policy
//! (`.azlin/policy.toml`) and the members' key wraps. Devices browse the
//! repository, never the bucket: no S3 List.
//!
//! # Layout in the bucket
//!
//! The repository is stored as a write-ahead log, in the layout of walgit
//! (<https://github.com/tobi/walgit>, MIT), which implements the design of
//! Cursor's "Git at any scale". Every object is sealed with the drive key:
//!
//! ```text
//! .azlin/meta/manifest                 the linearization point: replaced only by a
//!                                      compare-and-swap (If-Match on its version)
//! .azlin/meta/log/<seq>-<attempt>      one immutable log entry per publish
//! .azlin/meta/wal/<name>.pack + .idx   immutable packs of git objects; <name> is a
//!                                      keyed hash, never the git hash
//! .azlin/meta/checkpoints/<seq>        the refs and the live packs at <seq>
//! .azlin/meta/leases/<purpose>         a lease with an expiry (compaction)
//! ```
//!
//! - **Polling** is one conditional GET of the manifest; "not modified" means nothing
//!   changed.
//! - **A publish** writes the pack, then the log entry, then swaps the manifest. When the
//!   swap loses (another device was first), the device reads the new entries, merges,
//!   and tries again.
//! - **A new device** reads the latest checkpoint plus the log entries after it.
//!
//! # Encryption
//!
//! The bucket sees that a repository exists, its size and its write times;
//! names, the tree's shape and git's hashes stay inside the ciphertext. What
//! seals is a `Sealer` (the drive key's; an authenticated cipher with
//! associated data). The associated data of every sealed object is its key in
//! the bucket, so the bucket cannot answer one object with another.
//!
//! # Why our own git objects (the dependency evaluation, 2026-10-10)
//!
//! The plan was to use the `gix` crates for objects, trees and packs, and to
//! vendor walgit's `walgit-wal` and `walgit-store` behind an encrypting store.
//!
//! - **walgit** is not published on crates.io. `walgit-wal` is a server: async (tokio),
//!   protobuf (its build script needs `protoc`), a bare repository on disk managed through
//!   the `git` binary, and it takes `walgit-store` with the AWS and Google Cloud SDKs. Its
//!   dependency closure is 514 crates, 154 of them crates this workspace has never had.
//! - **gix** for objects, hashes and pack writing is 25 new crates (one build script) on a
//!   release train with a breaking release every month. A tree merge through gix-merge is
//!   42 new crates. git's pack format (zlib, deltas, one trailing hash) cannot be read in
//!   ranges once it is sealed.
//!
//! What a drive index needs is small: git objects byte-identical to git's
//! (so `git-remote-azlin` can hand them to plain git), SHA-256 (sha2 is a
//! dependency already), a pack whose sealed chunks can be read in ranges, and
//! a three-way merge of trees whose conflicts are files to keep or rename,
//! never text to merge. That is this module, with no new crate.
