//! What loading the engine says when there is no engine: a status that names the file and the
//! system's reason, never a crash. The engine itself is exercised by the real-engine test
//! (`dll/tests/speech_builtin_engine.rs`, which needs the files `scripts/fetch_whistle.sh` fetches).

use std::path::Path;

use super::{CactusEngine, EngineError};

#[test]
fn a_path_with_no_library_is_a_status_that_names_the_path() {
    let error = CactusEngine::open(Path::new("/definitely/not/here/libcactus.so")).err().expect("no such library");
    match &error {
        EngineError::NotLoaded { path, why } => {
            assert_eq!(path, "/definitely/not/here/libcactus.so");
            assert!(!why.is_empty(), "the system's reason is kept");
        }
        other => panic!("expected NotLoaded, got {other:?}"),
    }
    let sentence = error.describe();
    assert!(sentence.contains("/definitely/not/here/libcactus.so"), "{sentence}");
    assert!(sentence.contains("could not be loaded"), "{sentence}");
}

#[test]
fn a_file_that_is_not_a_library_is_not_loaded() {
    let dir = std::env::temp_dir().join(format!("azul-speech-engine-test-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("a scratch folder");
    let file = dir.join("libcactus.so");
    std::fs::write(&file, b"this is text, not a shared library").expect("a file");
    let error = CactusEngine::open(&file).err().expect("not a library");
    assert!(matches!(error, EngineError::NotLoaded { .. }), "{error:?}");
    drop(std::fs::remove_dir_all(&dir));
}

#[test]
fn a_process_that_did_not_link_the_engine_has_no_engine() {
    let error = CactusEngine::this().err().expect("this test binary has no cactus_load");
    #[cfg(any(target_os = "linux", target_os = "windows", target_os = "android", target_os = "macos"))]
    assert_eq!(error, EngineError::Incompatible { symbol: "cactus_load" });
    #[cfg(not(any(target_os = "linux", target_os = "windows", target_os = "android", target_os = "macos")))]
    assert!(matches!(error, EngineError::NotLoaded { .. }));
}

#[test]
fn the_sentences_say_what_to_do() {
    let missing = EngineError::Incompatible { symbol: "cactus_transcribe" }.describe();
    assert!(missing.contains("cactus_transcribe") && missing.contains("not a Cactus engine"), "{missing}");
}
