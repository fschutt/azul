//! A big listing of the drive index in pages: each page is cut from one walk of the tree, not
//! from a walk per page (the sync of an encrypted folder lists the whole drive every pass, 1,000
//! entries a page: 100,000 files read every pointer a hundred times - a quiet pass took 30 s in
//! the chaos E2E).

use std::sync::Arc;

use crate::{
    crypto::DriveKey,
    encrypted::{EncryptedDrive, NameIndex},
    meta::{MemoryBucket, MetaIndex, MetaRepo, RepoOptions},
    ops, BatchPut, Drive,
};

/// A device of the drive, and the index it browses with (the test counts what the index does).
fn device(
    bucket: &MemoryBucket,
    key: &DriveKey,
) -> (
    EncryptedDrive<MemoryBucket>,
    Arc<MetaIndex<MemoryBucket, DriveKey>>,
) {
    let options = RepoOptions {
        cache_dir: None,
        lazy: false,
        ..RepoOptions::default()
    };
    let repo = match MetaRepo::open_with(bucket.clone(), key.clone(), "laptop", "Laptop", &options)
    {
        Ok(repo) => repo,
        Err(_) => MetaRepo::create_with(bucket.clone(), key.clone(), "laptop", "Laptop", &options)
            .unwrap(),
    };
    let meta = Arc::new(MetaIndex::new(repo).with_poll_every(3600));
    let index: Arc<dyn NameIndex> = meta.clone();
    (
        EncryptedDrive::new(bucket.clone(), key.clone(), index),
        meta,
    )
}

/// What it was: 4,000 files in pages of 7 and a wall clock of five seconds. The files cost minutes of
/// `put_many` in a debug build under load (the test did not finish in a quarter of an hour on a loaded
/// machine: X-SYNC4L-1), and the clock failed the test for a busy machine as readily as for a walk per
/// page. What it guards is a COUNT: the tree is walked, and every pointer read, ONCE for a listing -
/// the first page - and the pages after it are cut from the keys that walk made; a listing of N entries
/// in pages of P walked it N / P times was the chaos E2E's 30-second quiet pass. 300 files in pages of 7
/// are 43 pages; the walks of each listing are counted where they are made (`MetaIndex::listing_walks`,
/// a counter of the test build), so the bound is exact and no clock is read. A change of the tree is
/// one more walk for its next listing, and a listing of one folder level (another delimiter) is a walk
/// of its own.
#[test]
fn a_big_listing_in_small_pages_reads_the_tree_once_not_once_per_page() {
    const FILES: usize = 300;
    const PAGE: u32 = 7;
    let key = DriveKey::generate().unwrap();
    let bucket = MemoryBucket::new();
    let (drive, index) = device(&bucket, &key);
    let names: Vec<String> = (0..FILES).map(|i| format!("many/f{i:05}.txt")).collect();
    let items: Vec<BatchPut<'_>> = names.iter().map(|n| BatchPut::new(n, b"x")).collect();
    for result in drive.put_many(&items) {
        result.unwrap();
    }
    let pages = (FILES + 1).div_ceil(PAGE as usize);
    assert!(pages >= 40, "{pages} pages: the property needs many");
    let before = index.listing_walks();
    let listed = ops::list_all_paged(&drive, "many/", PAGE).unwrap();
    let walks = index.listing_walks() - before;
    assert_eq!(
        walks, 1,
        "{FILES} files in {pages} pages of {PAGE} walked the tree {walks} times: every page walked it"
    );
    // The drive index lists the folder's own marker with its files, as an S3
    // console's folder has one (`many/` first).
    assert_eq!(listed.first().map(|o| o.key.as_str()), Some("many/"));
    assert_eq!(listed.len(), FILES + 1);
    assert!(
        listed.windows(2).all(|w| w[0].key < w[1].key),
        "in key order, each once"
    );
    // A change between two listings is in the next one: the tree has a new root, one more walk.
    drive.put("many/zz-new.txt", b"y").unwrap();
    let before = index.listing_walks();
    let again = ops::list_all_paged(&drive, "many/", PAGE).unwrap();
    assert_eq!(
        index.listing_walks() - before,
        1,
        "one walk for the new tree"
    );
    assert_eq!(again.len(), FILES + 1 + 1);
    assert_eq!(
        again.last().map(|o| o.key.as_str()),
        Some("many/zz-new.txt")
    );
    // One folder level pages the same way: its own listing (another delimiter), one walk.
    let before = index.listing_walks();
    let folder = ops::list_folder_all(&drive, "many/").unwrap();
    assert_eq!(
        index.listing_walks() - before,
        1,
        "one walk for one folder level"
    );
    assert_eq!(folder.objects.len(), FILES + 1 + 1);
}
