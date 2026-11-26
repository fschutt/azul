//! The built-in recognizer's model fetched from the real server: the redirect of huggingface.co, the
//! windows of the ranged download, a stop that keeps what came and a resume from it, and the check of the
//! pinned size and SHA-256 before the file gets its name.
//!
//! `cargo test -p azul-dll --test speech_model_download -- --ignored --nocapture`. Ignored by default: it
//! fetches 16.9 MB from the network. What it proves that the unit tests (over a fake transport) and
//! azul-layout's (over a local server) cannot: the pinned URL, size and checksum are the real ones, and
//! the server answers ranged requests the way the downloader believes.

use std::{
    sync::Arc,
    time::{Duration, Instant},
};

use azul::desktop::extra::speech::builtin::{DownloadPolicy, LayoutFetch, ModelPhase, ModelStore, WHISTLE};

fn wait(store: &ModelStore, what: &str, done: impl Fn(&azul::desktop::extra::speech::builtin::ModelState) -> bool) {
    let began = Instant::now();
    loop {
        let state = store.state();
        if done(&state) {
            return;
        }
        assert!(began.elapsed() < Duration::from_secs(600), "waited for {what}: {state:?}");
        assert_ne!(state.phase, ModelPhase::Failed, "{what}: {}", state.message);
        std::thread::sleep(Duration::from_millis(50));
    }
}

#[test]
#[ignore = "needs the network: it fetches the 16.9 MB model from huggingface.co (run by hand)"]
fn the_real_model_is_fetched_stopped_resumed_and_checked_against_its_pin() {
    let dir = std::env::temp_dir().join(format!("azul-speech-model-real-{}", std::process::id()));
    drop(std::fs::remove_dir_all(&dir));
    let store = ModelStore::new(&WHISTLE, Arc::new(LayoutFetch), Some(dir.clone()));
    store.set_policy(DownloadPolicy::Automatic);
    assert_eq!(store.state().phase, ModelPhase::Missing);

    // The first need starts it; stop it once a few windows are on disk.
    store.request();
    wait(&store, "the first megabytes", |s| s.bytes_done >= 3 * 1024 * 1024 || s.phase == ModelPhase::Present);
    store.cancel();
    wait(&store, "the stop", |s| s.phase != ModelPhase::Downloading);
    // The download ends at its next window (a megabyte): give it the time, so that the next one is a resume.
    std::thread::sleep(Duration::from_secs(5));
    let partial = dir.join(format!("{}.partial", WHISTLE.file_name));
    let kept = std::fs::metadata(&partial).map(|m| m.len()).unwrap_or(0);
    println!("stopped with {kept} bytes on disk");
    assert!(kept >= 3 * 1024 * 1024 || dir.join(WHISTLE.file_name).exists(), "what came stays for the resume");

    // The user says yes again: it goes on from there and ends with the pinned file.
    store.allow();
    wait(&store, "the model", |s| s.phase == ModelPhase::Present);
    let state = store.state();
    let file = state.path.expect("the model's path");
    assert_eq!(std::fs::metadata(&file).expect("the model").len(), WHISTLE.bytes);
    assert!(!partial.exists(), "the partial file became the model");
    println!("the model is {} ({} bytes)", file.display(), WHISTLE.bytes);
    drop(std::fs::remove_dir_all(&dir));
}
