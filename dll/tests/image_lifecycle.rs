//! End-to-end proof that adding and dropping an `ImageRef` from the DOM
//! across successive frames produces the correct resource updates.
//!
//! The goal of `ImageKey` being derived from `ImageRefHash` is that:
//!   - Frame N uses an image  → `collect_image_resource_updates` yields an
//!     `AddImage` whose `ImageKey` can be round-tripped back to the source
//!     `ImageRefHash` losslessly (no folding / no truncation).
//!   - Frame N+1 no longer references the image → `scan_used_images` no
//!     longer contains that hash (input to the GC path that would emit
//!     `DeleteImage`).
//!
//! At the time of writing, the `DeleteImage` emission path on the dll side
//! has not been wired into `HeadlessWindow::regenerate_layout`; the test
//! therefore asserts the invariants that ARE reachable (AddImage generation,
//! lossless hash→key round-trip, scan_used_images diff) and documents the
//! missing GC step so that when it lands, only one assertion here flips on.
//!
//! See the sibling test `headless_lifecycle.rs` for the overall pattern of
//! driving a `HeadlessWindow` with a layout callback that returns different
//! DOMs on successive frames.

use std::cell::RefCell;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use azul_core::callbacks::{LayoutCallback, LayoutCallbackInfo};
use azul_core::dom::{Dom, NodeData};
use azul_core::icon::{IconProviderHandle, SharedIconProvider};
use azul_core::refany::RefAny;
use azul_core::resources::{
    image_ref_hash_to_image_key, AppConfig, ImageRef, ImageRefHash, RawImage, RawImageData,
    RawImageFormat,
};
use azul_layout::window_state::WindowCreateOptions;
use rust_fontconfig::FcFontCache;

use azul::desktop::shell2::headless::HeadlessWindow;
use azul::desktop::wr_translate2::collect_image_resource_updates;

#[derive(Clone)]
struct Ctx {
    /// Externally-controlled toggle. `regenerate_layout` may invoke the
    /// layout callback multiple times per frame, so we pick DOM contents
    /// off a flag rather than an auto-incrementing counter.
    include_image: Arc<AtomicBool>,
    /// Pre-built ImageRef whose hash is stable across frames.
    image: ImageRef,
}

fn make_image() -> ImageRef {
    // 2x2 fully-opaque red BGRA8 image. Exact bytes don't matter — we only
    // care that we get back a valid ImageRef that flows through the DOM.
    let pixels: Vec<u8> = vec![
        0, 0, 255, 255, 0, 0, 255, 255, 0, 0, 255, 255, 0, 0, 255, 255,
    ];
    let raw = RawImage {
        pixels: RawImageData::U8(pixels.into()),
        width: 2,
        height: 2,
        premultiplied_alpha: true,
        data_format: RawImageFormat::BGRA8,
        tag: Vec::new().into(),
    };
    ImageRef::new_rawimage(raw).expect("RawImage → ImageRef must succeed")
}

extern "C" fn layout_cb(mut data: RefAny, _info: LayoutCallbackInfo) -> Dom {
    let ctx = match data.downcast_ref::<Ctx>() {
        Some(c) => c.clone(),
        None => return Dom::create_body(),
    };

    if ctx.include_image.load(Ordering::SeqCst) {
        Dom::create_body()
            .with_child(Dom::create_from_data(NodeData::create_image(ctx.image.clone())))
    } else {
        Dom::create_body()
    }
}

fn make_window(ctx: Ctx) -> HeadlessWindow {
    let fc_cache = Arc::new(FcFontCache::default());
    let app_data = Arc::new(RefCell::new(RefAny::new(ctx)));
    let icon_provider = SharedIconProvider::from_handle(IconProviderHandle::default());

    let mut options = WindowCreateOptions::default();
    options.window_state.layout_callback = LayoutCallback {
        cb: layout_cb,
        ctx: azul_core::refany::OptionRefAny::None,
    };

    HeadlessWindow::new(
        options,
        app_data,
        azul::desktop::shell2::common::event::SharedUndoManager::new(),
        AppConfig::default(),
        icon_provider,
        fc_cache,
        None,
    )
    .expect("HeadlessWindow construction must succeed")
}

#[test]
fn image_ref_hash_round_trips_losslessly() {
    // Sanity test: image_ref_hash_to_image_key must be bijective w.r.t.
    // hash.inner <-> ImageKey.key (no folding, no truncation).
    let image = make_image();
    let hash: ImageRefHash = image.get_hash();
    let namespace = azul_core::resources::IdNamespace(7);
    let key = image_ref_hash_to_image_key(hash, namespace);

    assert_eq!(key.namespace, namespace);
    assert_eq!(
        key.key, hash.inner,
        "ImageKey.key must preserve every bit of ImageRefHash.inner \
         (both u64, no folding/truncation)"
    );
}

#[test]
fn image_lifecycle_produces_add_then_disappears_from_scan() {
    let image = make_image();
    let expected_hash = image.get_hash();
    let include_image = Arc::new(AtomicBool::new(true));
    let ctx = Ctx {
        include_image: include_image.clone(),
        image: image.clone(),
    };
    // Drop our local handle — the ImageRef will only stay alive via the
    // DOM returned from layout_cb (cloned from ctx.image on each call).
    drop(image);

    let mut window = make_window(ctx);

    // Frame 0 → DOM contains the image.
    window
        .regenerate_layout()
        .expect("frame 0 regenerate_layout");

    let layout_window = window
        .common
        .layout_window
        .as_ref()
        .expect("layout_window populated after first regenerate_layout");

    let used_frame0 = layout_window
        .scan_used_images(&azul_core::resources::ImageCache::new());
    assert!(
        used_frame0.contains(&expected_hash),
        "frame 0: the image we placed in the DOM must appear in scan_used_images \
         (hash={:?}, scanned={:?})",
        expected_hash,
        used_frame0,
    );

    let (adds_frame0, live_frame0) =
        collect_image_resource_updates(layout_window, &window.common.renderer_resources);
    assert_eq!(
        adds_frame0.len(),
        1,
        "frame 0: exactly one AddImage resource update expected (got {})",
        adds_frame0.len(),
    );
    assert!(
        live_frame0.contains(&expected_hash),
        "frame 0: the live-set returned for GC must contain the on-screen image",
    );
    let (hash0, add_msg0) = &adds_frame0[0];
    assert_eq!(*hash0, expected_hash, "AddImage hash must match");

    // ImageKey must round-trip from the hash losslessly.
    let expected_key =
        image_ref_hash_to_image_key(expected_hash, layout_window.id_namespace);
    assert_eq!(
        add_msg0.0.key, expected_key,
        "AddImage.key must equal image_ref_hash_to_image_key(hash, ns) exactly"
    );

    // Frame 1 → DOM no longer references the image.
    include_image.store(false, Ordering::SeqCst);
    window
        .regenerate_layout()
        .expect("frame 1 regenerate_layout");

    let layout_window = window
        .common
        .layout_window
        .as_ref()
        .expect("layout_window still populated after second regenerate_layout");

    let used_frame1 = layout_window
        .scan_used_images(&azul_core::resources::ImageCache::new());
    assert!(
        !used_frame1.contains(&expected_hash),
        "frame 1: image is no longer referenced by the DOM, so scan_used_images \
         must NOT contain its hash (this is the input the GC path uses to emit \
         ResourceUpdate::DeleteImage) — scanned={:?}",
        used_frame1,
    );

    let (adds_frame1, live_frame1) =
        collect_image_resource_updates(layout_window, &window.common.renderer_resources);
    assert!(
        adds_frame1.is_empty(),
        "frame 1: no image in the DOM → no AddImage updates (got {:?})",
        adds_frame1,
    );
    assert!(
        !live_frame1.contains(&expected_hash),
        "frame 1: image is off-screen, so it must NOT be in the GC live-set",
    );
}

/// The GC itself: a registered image absent from the live-set for more than
/// `IMAGE_GC_KEEP_EPOCHS` frames must produce exactly one `DeleteImage` for its
/// key and be evicted from all three registry maps — and NOT one epoch too
/// early (the retention window that avoids churning a one-frame blip).
#[test]
fn stale_image_is_deleted_after_retention_window() {
    use azul::desktop::wr_translate2::collect_stale_image_deletes;
    use azul_core::resources::{ResolvedImage, ResourceUpdate};

    let image = make_image();
    let expected_hash = image.get_hash();
    let include_image = Arc::new(AtomicBool::new(true));
    let ctx = Ctx { include_image: include_image.clone(), image: image.clone() };
    drop(image);

    let mut window = make_window(ctx);

    // Frame 0 → image on screen. Grab a real (key, descriptor) from the
    // AddImage the collector produces, then register it by hand exactly as
    // register_frame_resources would (the test harness never runs a WR txn).
    window.regenerate_layout().expect("frame 0 regenerate_layout");
    let (key, descriptor, epoch0) = {
        let lw = window.common.layout_window.as_ref().expect("layout_window");
        let (adds, live) = collect_image_resource_updates(lw, &lw.renderer_resources);
        assert!(live.contains(&expected_hash));
        let add = &adds[0].1;
        (add.0.key, add.0.descriptor, lw.epoch.into_u32())
    };
    {
        let lw = window.common.layout_window.as_mut().expect("layout_window");
        let rr = &mut lw.renderer_resources;
        rr.currently_registered_images
            .insert(expected_hash, ResolvedImage { key, descriptor });
        rr.image_key_map.insert(key, expected_hash);
        rr.image_last_seen_epoch.insert(expected_hash, epoch0);
    }

    // Image now off-screen → empty live set every frame.
    include_image.store(false, Ordering::SeqCst);
    let empty = azul_core::FastBTreeSet::new();

    // Within the retention window (≤ KEEP_EPOCHS frames absent): NO delete.
    for _ in 0..2 {
        let lw = window.common.layout_window.as_mut().expect("layout_window");
        lw.epoch.increment();
        let dels = collect_stale_image_deletes(lw, &empty);
        assert!(dels.is_empty(), "image must survive the retention window");
        assert!(
            lw.renderer_resources
                .currently_registered_images
                .contains_key(&expected_hash),
            "image must still be registered during retention",
        );
    }

    // One frame past the window → exactly one DeleteImage for its key, evicted.
    let lw = window.common.layout_window.as_mut().expect("layout_window");
    lw.epoch.increment();
    let dels = collect_stale_image_deletes(lw, &empty);
    assert_eq!(dels.len(), 1, "exactly one DeleteImage expected");
    match &dels[0] {
        ResourceUpdate::DeleteImage(k) => {
            assert_eq!(*k, key, "DeleteImage must target the image's key")
        }
        other => panic!("expected DeleteImage, got {:?}", other),
    }
    let rr = &lw.renderer_resources;
    assert!(!rr.currently_registered_images.contains_key(&expected_hash), "evicted from registry");
    assert!(!rr.image_key_map.contains_key(&key), "evicted from key map");
    assert!(!rr.image_last_seen_epoch.contains_key(&expected_hash), "evicted from gc map");
}
