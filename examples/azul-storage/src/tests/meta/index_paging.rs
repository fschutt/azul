//! A big listing of the drive index in pages: each page is cut from one walk of the tree, not
//! from a walk per page (the sync of an encrypted folder lists the whole drive every pass, 1,000
//! entries a page: 100,000 files read every pointer a hundred times - a quiet pass took 30 s in
//! the chaos E2E).

use std::{
    sync::Arc,
    time::{Duration, Instant},
};

use crate::{
    crypto::DriveKey,
    encrypted::{EncryptedDrive, NameIndex},
    meta::{MemoryBucket, MetaIndex, MetaRepo, RepoOptions},
    ops, BatchPut, Drive,
};

fn device(bucket: &MemoryBucket, key: &DriveKey) -> EncryptedDrive<MemoryBucket> {
    let options = RepoOptions {
        cache_dir: None,
        lazy: false,
    };
    let repo = match MetaRepo::open_with(bucket.clone(), key.clone(), "laptop", "Laptop", &options)
    {
        Ok(repo) => repo,
        Err(_) => MetaRepo::create_with(bucket.clone(), key.clone(), "laptop", "Laptop", &options)
            .unwrap(),
    };
    let index: Arc<dyn NameIndex> = Arc::new(MetaIndex::new(repo).with_poll_every(3600));
    EncryptedDrive::new(bucket.clone(), key.clone(), index)
}

#[test]
fn a_big_listing_in_small_pages_reads_the_tree_once_not_once_per_page() {
    let key = DriveKey::generate().unwrap();
    let bucket = MemoryBucket::new();
    let drive = device(&bucket, &key);
    let names: Vec<String> = (0..4000).map(|i| format!("many/f{i:05}.txt")).collect();
    let items: Vec<BatchPut<'_>> = names.iter().map(|n| BatchPut::new(n, b"x")).collect();
    for result in drive.put_many(&items) {
        result.unwrap();
    }
    let started = Instant::now();
    let listed = ops::list_all_paged(&drive, "many/", 7).unwrap();
    let took = started.elapsed();
    // The drive index lists the folder's own marker with its files, as an S3
    // console's folder has one (`many/` first).
    assert_eq!(listed.first().map(|o| o.key.as_str()), Some("many/"));
    assert_eq!(listed.len(), 4000 + 1);
    assert!(
        listed.windows(2).all(|w| w[0].key < w[1].key),
        "in key order, each once"
    );
    assert!(
        took < Duration::from_secs(5),
        "4,000 files in pages of 7 took {took:?}: every page walked the whole tree"
    );
    // A change between two listings is in the next one.
    drive.put("many/zz-new.txt", b"y").unwrap();
    let again = ops::list_all_paged(&drive, "many/", 7).unwrap();
    assert_eq!(again.len(), 4001 + 1);
    assert_eq!(
        again.last().map(|o| o.key.as_str()),
        Some("many/zz-new.txt")
    );
    // One folder level pages the same way.
    let folder = ops::list_folder_all(&drive, "many/").unwrap();
    assert_eq!(folder.objects.len(), 4001 + 1);
}
