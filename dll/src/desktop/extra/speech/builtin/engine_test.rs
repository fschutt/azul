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

// With `speech-engine-static` the test binary links the engine: the premise of this test is gone.
#[cfg(not(feature = "speech-engine-static"))]
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

// ---- the C ABI, over functions of a fake engine ----
//
// What the recognizer does with the four functions (how it lays out the arguments, how it reads an
// answer, what it says when the engine refuses) is the same whether they come out of a library opened
// at run time or out of the static library, so it is tested once, over functions that answer from their
// arguments alone (no state: the real engine's one global model is for the real-engine test).

#[cfg(any(
    target_os = "linux",
    target_os = "windows",
    target_os = "android",
    target_os = "macos",
    feature = "speech-engine-static"
))]
mod over_fake_functions {
    use std::ffi::{c_char, c_int, CStr};

    use super::super::abi::{Abi, MIN_ANSWER_BYTES};

    /// A model is what starts with the byte 7.
    extern "C" fn fake_load(model: *const u8, n: u64) -> c_int {
        // SAFETY: the caller passes `n` readable bytes.
        if n == 0 || unsafe { *model } != 7 {
            -1
        } else {
            c_int::try_from(n).unwrap_or(c_int::MAX)
        }
    }

    extern "C" fn fake_last_error() -> *const c_char {
        b"fake engine: not a model\0".as_ptr().cast()
    }

    extern "C" fn fake_no_reason() -> *const c_char {
        std::ptr::null()
    }

    extern "C" fn fake_set_threads(_threads: c_int) {}

    /// Writes what it was given as JSON: the number of samples, the language and the keywords (`-` for
    /// a null pointer), whether word times were asked for and the room it was given. Refuses no samples.
    extern "C" fn fake_transcribe(
        _pcm: *const f32,
        samples: c_int,
        language: *const c_char,
        keywords: *const c_char,
        word_timestamps: c_int,
        out: *mut c_char,
        out_capacity: c_int,
    ) -> c_int {
        if samples == 0 {
            return -1;
        }
        let text = |p: *const c_char| {
            if p.is_null() {
                "-".to_string()
            } else {
                // SAFETY: the recognizer passes NUL-terminated strings or null.
                unsafe { CStr::from_ptr(p) }.to_string_lossy().into_owned()
            }
        };
        let answer = format!(
            "{{\"samples\":{samples},\"language\":\"{}\",\"keywords\":\"{}\",\"times\":{word_timestamps},\"room\":{out_capacity}}}",
            text(language),
            text(keywords)
        );
        if answer.len() + 1 > usize::try_from(out_capacity).unwrap_or(0) {
            return -1;
        }
        // SAFETY: `out` has `out_capacity` bytes, and the answer and its NUL fit.
        unsafe {
            std::ptr::copy_nonoverlapping(answer.as_ptr(), out.cast::<u8>(), answer.len());
            *out.add(answer.len()) = 0;
        }
        c_int::try_from(answer.len()).unwrap_or(c_int::MAX)
    }

    fn abi() -> Abi {
        Abi { load: fake_load, last_error: fake_last_error, set_threads: fake_set_threads, transcribe: fake_transcribe }
    }

    #[test]
    fn a_model_the_engine_takes_loads_and_one_it_refuses_says_why() {
        assert_eq!(abi().load(&[7, 1, 2, 3]), Ok(()));
        assert_eq!(abi().load(&[1, 2, 3]), Err("fake engine: not a model".to_string()));
        assert_eq!(abi().load(&[]), Err("fake engine: not a model".to_string()));
    }

    #[test]
    fn an_engine_that_gives_no_reason_gets_a_sentence() {
        let abi = Abi { last_error: fake_no_reason, ..abi() };
        assert_eq!(abi.load(&[1]), Err("the engine gave no reason".to_string()));
    }

    #[test]
    fn the_audio_goes_with_its_length_and_an_empty_language_or_keywords_are_null() {
        let samples = vec![0.0f32; 16_000];
        let answer = abi().transcribe(&samples, "de", "css error\nunwrap or else").expect("answers");
        assert!(answer.contains("\"samples\":16000"), "{answer}");
        assert!(answer.contains("\"language\":\"de\""), "{answer}");
        assert!(answer.contains("\"keywords\":\"css error\nunwrap or else\""), "{answer}");
        assert!(answer.contains("\"times\":1"), "word times are always asked for: {answer}");
        let detect = abi().transcribe(&samples, "", "").expect("answers");
        assert!(detect.contains("\"language\":\"-\"") && detect.contains("\"keywords\":\"-\""), "{detect}");
    }

    #[test]
    fn a_nul_inside_a_string_is_dropped_not_a_panic() {
        let answer = abi().transcribe(&[0.0; 160], "d\0e", "a\0b").expect("answers");
        assert!(answer.contains("\"language\":\"de\"") && answer.contains("\"keywords\":\"ab\""), "{answer}");
    }

    fn room_of(answer: &str) -> usize {
        let at = answer.find("\"room\":").expect("the fake says the room") + "\"room\":".len();
        answer[at..].trim_end_matches('}').parse().expect("a number")
    }

    #[test]
    fn the_answer_gets_the_room_the_vendor_sizes_it_for() {
        // 4096 bytes for each second of audio, 256 KiB at the least.
        let short = abi().transcribe(&vec![0.0; 16_000 * 20], "en", "").expect("answers");
        assert_eq!(room_of(&short), MIN_ANSWER_BYTES);
        let long = abi().transcribe(&vec![0.0; 16_000 * 100], "en", "").expect("answers");
        assert_eq!(room_of(&long), 101 * 4096);
    }

    #[test]
    fn an_engine_that_refuses_the_audio_has_its_reason_as_the_error() {
        assert_eq!(abi().transcribe(&[], "en", ""), Err("fake engine: not a model".to_string()));
    }
}

/// The static library of the feature `speech-engine-static` is in the program and answers through the
/// same four functions: with no model loaded it refuses, in its own words, and a file that is no model
/// is refused too. (The real model is for the real-engine test; the engine's model is one for the
/// process, so nothing here loads one.)
#[cfg(feature = "speech-engine-static")]
#[test]
fn the_linked_engine_is_in_the_program_and_refuses_what_is_no_model() {
    use super::{Engine, LinkedEngine};

    let mut engine = LinkedEngine::new();
    let refused = engine.load(b"this is text, not a model").err().expect("not a model");
    assert!(!refused.is_empty(), "the engine says why");
    engine.set_threads(1);
}
