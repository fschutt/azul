//! Which renderer a phone window gets: the request, the failure plan of a test run, the decision and the line
//! the QA harness reads. No window, no GL, no environment.

use super::*;

fn info(renderer: &str, version: &str) -> DeviceInfo {
    DeviceInfo {
        vendor: "Apple Inc.".to_string(),
        renderer: renderer.to_string(),
        version: version.to_string(),
        glsl: "OpenGL ES GLSL ES 3.00".to_string(),
    }
}

fn apple_gpu() -> DeviceInfo {
    info("Apple A15 GPU", "OpenGL ES 3.0 APPLE-24.5.5")
}

fn software_gl() -> DeviceInfo {
    info("Google SwiftShader", "OpenGL ES 3.0 SwiftShader 4.0")
}

fn failed(stage: GpuStage, cause: &str) -> GpuFallback {
    GpuFallback::new(stage, cause)
}

// ---------------------------------------------------------------------------------------------
// the request
// ---------------------------------------------------------------------------------------------

/// Nothing asked is the CPU renderer: the phone shells drew on the CPU before the GPU path existed, and an app
/// that never heard of `AZ_BACKEND` must keep doing so.
///
/// Mutation: a default of `Auto` or `Gpu`.
#[test]
fn nothing_asked_is_the_cpu_renderer() {
    assert_eq!(GpuRequest::resolve(None, None), GpuRequest::Cpu);
    assert_eq!(
        GpuRequest::resolve(None, Some(HwAcceleration::DontCare)),
        GpuRequest::Cpu
    );
}

/// The environment decides before the window's own setting, in both directions.
///
/// Mutation: `hw_accel` read first.
#[test]
fn the_environment_decides_before_the_window() {
    assert_eq!(
        GpuRequest::resolve(Some("gpu"), Some(HwAcceleration::Disabled)),
        GpuRequest::Gpu
    );
    assert_eq!(
        GpuRequest::resolve(Some("cpu"), Some(HwAcceleration::Enabled)),
        GpuRequest::Cpu
    );
    assert_eq!(
        GpuRequest::resolve(Some("AUTO"), Some(HwAcceleration::Disabled)),
        GpuRequest::Auto
    );
    assert_eq!(
        GpuRequest::resolve(None, Some(HwAcceleration::Enabled)),
        GpuRequest::Gpu
    );
    assert_eq!(
        GpuRequest::resolve(None, Some(HwAcceleration::Disabled)),
        GpuRequest::Cpu
    );
}

/// `headless` is a test stand-in without a window and a word nobody knows is no renderer: neither takes a GL
/// context on a phone.
#[test]
fn headless_and_unknown_words_are_no_gpu_request() {
    assert_eq!(GpuRequest::resolve(Some("headless"), None), GpuRequest::Cpu);
    assert_eq!(GpuRequest::resolve(Some("quantum"), None), GpuRequest::Cpu);
    assert_eq!(
        GpuRequest::resolve(Some("opengl"), None),
        GpuRequest::Gpu,
        "the desktop's spellings of the GPU are the phone's too"
    );
}

/// The request's words are the ones `AZ_BACKEND` takes and the log line prints.
#[test]
fn the_requests_words() {
    assert_eq!(GpuRequest::Cpu.as_str(), "cpu");
    assert_eq!(GpuRequest::Gpu.as_str(), "gpu");
    assert_eq!(GpuRequest::Auto.as_str(), "auto");
}

// ---------------------------------------------------------------------------------------------
// the stages and the failure plan
// ---------------------------------------------------------------------------------------------

/// Every stage's word parses back to the stage, in any case and with `-` for `_`; a word that is no stage
/// parses to nothing. The words are what `AZ_GPU_FAIL` and the fallback line use, so they never change.
#[test]
fn a_stage_is_named_by_its_word_and_parses_back() {
    for stage in GpuStage::ALL {
        assert_eq!(GpuStage::parse(stage.name()), Some(stage));
        assert_eq!(
            GpuStage::parse(&stage.name().to_ascii_uppercase()),
            Some(stage)
        );
    }
    assert_eq!(GpuStage::parse("make-current"), Some(GpuStage::MakeCurrent));
    assert_eq!(GpuStage::parse(" surface "), Some(GpuStage::Surface));
    assert_eq!(GpuStage::parse("swapchain"), None);
    assert_eq!(GpuStage::parse(""), None);
    let words: Vec<&str> = GpuStage::ALL.iter().map(|s| s.name()).collect();
    assert_eq!(
        words,
        [
            "library",
            "context",
            "make_current",
            "functions",
            "probe",
            "surface",
            "renderer",
            "document",
            "shaders",
            "frame",
            "lost"
        ]
    );
}

/// No value, a blank one and an unset one plan nothing.
#[test]
fn an_empty_plan_fails_nothing() {
    for spec in [None, Some(""), Some("   ")] {
        let mut plan = FailPlan::parse(spec);
        assert!(plan.is_empty());
        for stage in GpuStage::ALL {
            assert!(!plan.fails(stage), "{spec:?} must fail nothing");
        }
    }
}

/// `<stage>` fails every time it is tried, and only that stage.
///
/// Mutation: a plan that fails once.
#[test]
fn a_bare_stage_fails_every_time() {
    let mut plan = FailPlan::parse(Some("context"));
    for _ in 0..5 {
        assert!(plan.fails(GpuStage::Context));
    }
    assert!(!plan.fails(GpuStage::Surface));
    assert!(!plan.fails(GpuStage::Lost));
}

/// `<stage>:<n>` fails n times and then works: a rebuild can succeed after one injected loss.
///
/// Mutation: a count that is never spent; a count one too many or too few.
#[test]
fn a_counted_stage_fails_n_times_then_works() {
    let mut plan = FailPlan::parse(Some("lost:2"));
    assert!(plan.fails(GpuStage::Lost));
    assert!(plan.fails(GpuStage::Lost));
    assert!(!plan.fails(GpuStage::Lost));
    assert!(!plan.fails(GpuStage::Lost));
    let mut zero = FailPlan::parse(Some("surface:0"));
    assert!(!zero.fails(GpuStage::Surface));
}

/// A list names several stages, each with its own count, and the same stage twice is two entries in a row.
#[test]
fn a_list_keeps_each_entry_apart() {
    let mut plan = FailPlan::parse(Some("context:1, surface , renderer:2,context:1"));
    assert!(plan.fails(GpuStage::Context));
    assert!(
        plan.fails(GpuStage::Context),
        "the second entry of the same stage"
    );
    assert!(!plan.fails(GpuStage::Context));
    assert!(plan.fails(GpuStage::Surface));
    assert!(plan.fails(GpuStage::Surface));
    assert!(plan.fails(GpuStage::Renderer));
    assert!(plan.fails(GpuStage::Renderer));
    assert!(!plan.fails(GpuStage::Renderer));
    assert!(plan.notes().is_empty());
}

/// A word that is no stage and a count that is no number are ignored, each with one note, and the rest of the
/// value still counts.
#[test]
fn a_bad_entry_is_ignored_with_a_note() {
    let mut plan = FailPlan::parse(Some("swapchain,surface:x,context:1"));
    assert!(plan.fails(GpuStage::Context));
    assert!(!plan.fails(GpuStage::Surface));
    assert_eq!(plan.notes().len(), 2, "{:?}", plan.notes());
    assert!(plan.notes()[0].contains("swapchain"));
    assert!(plan.notes()[1].contains("surface"));
}

/// The input is capped: a long value, a long list and a huge count are cut with a note, never a panic or a
/// plan that never ends.
#[test]
fn the_plan_is_capped() {
    let long = format!("{}context", "x,".repeat(FAIL_SPEC_MAX_BYTES));
    let plan = FailPlan::parse(Some(&long));
    assert!(
        plan.is_empty(),
        "the stage names after the cap are not read"
    );
    assert!(plan.notes().iter().any(|n| n.contains("bytes")));

    let many = vec!["lost:1"; FAIL_SPEC_MAX_ENTRIES + 4].join(",");
    let mut plan = FailPlan::parse(Some(&many));
    let mut fails = 0;
    while plan.fails(GpuStage::Lost) {
        fails += 1;
    }
    assert_eq!(fails, FAIL_SPEC_MAX_ENTRIES);
    assert!(plan.notes().iter().any(|n| n.contains("entries")));

    let mut huge = FailPlan::parse(Some("frame:4000000000"));
    let mut fails = 0u32;
    while huge.fails(GpuStage::Frame) {
        fails += 1;
    }
    assert_eq!(fails, FAIL_COUNT_MAX);
    assert!(huge.notes().iter().any(|n| n.contains("at most")));
}

/// A cut in the middle of a character does not panic.
#[test]
fn the_cap_cuts_at_a_character_boundary() {
    // 3 bytes each, so byte 256 is inside a character
    let spec = "\u{20ac}".repeat(100);
    let plan = FailPlan::parse(Some(&spec));
    assert!(plan.is_empty());
    assert!(!plan.notes().is_empty());
}

// ---------------------------------------------------------------------------------------------
// what the context says
// ---------------------------------------------------------------------------------------------

/// The version string names the OpenGL ES version the context speaks; a desktop or an unknown string names none.
#[test]
fn the_version_string_names_the_es_version() {
    let v = |s: &str| info("r", s).gles_version();
    assert_eq!(v("OpenGL ES 3.0 APPLE-24.5.5"), Some((3, 0)));
    assert_eq!(v("OpenGL ES 3.2 V@0502.0 (GIT@a1b2c3)"), Some((3, 2)));
    assert_eq!(v("OpenGL ES 2.0 Apple A7 GPU"), Some((2, 0)));
    assert_eq!(v("OpenGL ES-CM 1.1"), Some((1, 1)));
    assert_eq!(v("OpenGL ES 3"), Some((3, 0)));
    assert_eq!(v("4.1 Metal - 83.1"), None);
    assert_eq!(v("OpenGL ES"), None);
    assert_eq!(v(""), None);
}

/// WebRender needs OpenGL ES 3.0 and nothing older passes.
///
/// Mutation: `>= 2`.
#[test]
fn es3_is_the_floor() {
    assert!(info("r", "OpenGL ES 3.0 x").is_es3());
    assert!(info("r", "OpenGL ES 3.2 x").is_es3());
    assert!(!info("r", "OpenGL ES 2.0 x").is_es3());
    assert!(!info("r", "4.6.0 NVIDIA 535").is_es3());
}

/// Software renderers are the ones a phone's `auto` does not take.
#[test]
fn software_renderers_are_known_by_name() {
    for renderer in [
        "Apple Software Renderer",
        "Google SwiftShader",
        "Android Emulator OpenGL ES Translator (Google SwiftShader)",
        "llvmpipe (LLVM 15.0.7, 256 bits)",
        "softpipe",
        "Mesa swrast",
    ] {
        assert!(info(renderer, "OpenGL ES 3.0").is_software(), "{renderer}");
    }
    for renderer in [
        "Apple A15 GPU",
        "Apple M1 Pro",
        "Adreno (TM) 640",
        "Mali-G78",
        "Android Emulator OpenGL ES Translator (Apple M1)",
    ] {
        assert!(!info(renderer, "OpenGL ES 3.0").is_software(), "{renderer}");
    }
}

/// The renderer on a line is one line, capped, and never empty.
#[test]
fn the_device_text_is_one_capped_line() {
    assert_eq!(
        info("  Apple\nA15   GPU \r\n", "v").describe(),
        "Apple A15 GPU"
    );
    assert_eq!(info("", "v").describe(), "unknown");
    assert_eq!(info("   ", "v").describe(), "unknown");
    let long = "x".repeat(500);
    let text = info(&long, "v").describe();
    assert_eq!(text.chars().count(), LINE_TEXT_MAX_CHARS);
    assert!(text.ends_with('~'));
}

/// The engine's GL table returns every driver string WITH its terminating NUL: no line the engine writes may carry
/// it (a platform logger cuts or drops a line with a NUL in it: Android's dropped the whole contract line), and the
/// version and the software test read the string as it comes.
///
/// Mutation: a `one_line` that only splits at whitespace.
#[test]
fn a_driver_string_with_its_nul_is_one_clean_line() {
    let renderer = "Apple Software Renderer\0";
    assert_eq!(info(renderer, "v").describe(), "Apple Software Renderer");
    assert_eq!(info("A\0B\u{1b}[0mC \0", "v").describe(), "A B [0mC");
    assert_eq!(info("\0\0", "v").describe(), "unknown");
    // the contract line and the fallback line carry no control character
    let device = DeviceInfo {
        vendor: "Apple Inc.\0".to_string(),
        renderer: renderer.to_string(),
        version: "OpenGL ES 3.0 APPLE-23.0.2\0".to_string(),
        glsl: "OpenGL ES GLSL ES 3.00\0".to_string(),
    };
    assert_eq!(device.gles_version(), Some((3, 0)));
    assert!(device.is_es3());
    assert!(device.is_software());
    let decision = decide(GpuRequest::Gpu, Some(Ok(device)));
    assert_eq!(
        decision.render_line(),
        "[render] backend=gpu requested=gpu device=Apple Software Renderer"
    );
    let cause = GpuFallback::new(GpuStage::Probe, "bad\0cause");
    let fallback = decide(GpuRequest::Gpu, Some(Err(cause)));
    for line in [fallback.render_line(), fallback.fallback_line().unwrap_or_default()] {
        assert!(!line.chars().any(char::is_control), "{line:?}");
    }
}

// ---------------------------------------------------------------------------------------------
// the decision
// ---------------------------------------------------------------------------------------------

/// A request for the CPU makes no attempt and says nothing about the GPU, whatever an attempt would have found.
///
/// Mutation: a CPU request that takes the GPU when the attempt succeeded.
#[test]
fn a_cpu_request_runs_the_cpu_renderer_and_says_nothing() {
    let d = decide(GpuRequest::Cpu, None);
    assert_eq!(d.ran, Ran::Cpu);
    assert_eq!(d.fallback, None);
    assert_eq!(d.fallback_line(), None);
    let d = decide(GpuRequest::Cpu, Some(Ok(apple_gpu())));
    assert_eq!(d.ran, Ran::Cpu, "an attempt nobody asked for is not taken");
    assert_eq!(d.fallback, None);
}

/// A GPU request whose bring-up worked runs the GPU, on a software renderer too: `gpu` is how the emulator's and
/// the Simulator's GL is tested.
///
/// Mutation: `gpu` that refuses a software renderer.
#[test]
fn gpu_runs_on_any_renderer_that_works() {
    let d = decide(GpuRequest::Gpu, Some(Ok(apple_gpu())));
    assert_eq!(d.ran, Ran::Gpu);
    assert_eq!(d.fallback, None);
    let d = decide(GpuRequest::Gpu, Some(Ok(software_gl())));
    assert_eq!(d.ran, Ran::Gpu, "software GL is what the runners have");
    assert_eq!(d.fallback_line(), None);
}

/// `auto` takes the GPU when there is a real one and the CPU renderer when the GL is software, and says why.
///
/// Mutation: `auto` that takes a software renderer.
#[test]
fn auto_takes_a_hardware_gpu_and_not_a_software_one() {
    let d = decide(GpuRequest::Auto, Some(Ok(apple_gpu())));
    assert_eq!(d.ran, Ran::Gpu);
    let d = decide(GpuRequest::Auto, Some(Ok(software_gl())));
    assert_eq!(d.ran, Ran::Cpu);
    let fb = d.fallback.expect("the CPU runs for a reason");
    assert_eq!(fb.stage, GpuStage::Probe);
    assert!(fb.cause.contains("SwiftShader"), "{}", fb.cause);
    assert!(fb.cause.contains("AZ_BACKEND=gpu"), "{}", fb.cause);
}

/// A context that is not OpenGL ES 3.0 is no GPU path whoever asked: the CPU runs and the fallback names the version.
///
/// Mutation: an ES 2.0 context taken for `gpu`.
#[test]
fn a_context_below_es3_falls_back() {
    for request in [GpuRequest::Gpu, GpuRequest::Auto] {
        let d = decide(
            request,
            Some(Ok(info("Apple A7 GPU", "OpenGL ES 2.0 APPLE-1"))),
        );
        assert_eq!(d.ran, Ran::Cpu);
        let fb = d.fallback.expect("a reason");
        assert_eq!(fb.stage, GpuStage::Probe);
        assert!(fb.cause.contains("OpenGL ES 3.0"), "{}", fb.cause);
        assert!(fb.cause.contains("OpenGL ES 2.0"), "{}", fb.cause);
    }
}

/// A bring-up that failed runs the CPU renderer with the stage and the cause the platform gave.
///
/// Mutation: a fallback that loses the stage or the cause.
#[test]
fn a_failed_attempt_runs_the_cpu_with_its_stage_and_cause() {
    let fb = failed(
        GpuStage::Surface,
        "renderbufferStorage:fromDrawable: returned NO",
    );
    let d = decide(GpuRequest::Gpu, Some(Err(fb.clone())));
    assert_eq!(d.ran, Ran::Cpu);
    assert_eq!(d.fallback, Some(fb));
    assert_eq!(
        d.fallback_line().as_deref(),
        Some("[render] gpu unavailable at surface: renderbufferStorage:fromDrawable: returned NO; drawing on the CPU")
    );
}

/// A GPU request in a build that has no GPU path (no attempt made) is a fallback too: a request that silently
/// ran on the CPU is the finding the QA harness exists to make.
///
/// Mutation: `None` taken for "nothing to say".
#[test]
fn a_gpu_request_without_a_gpu_path_says_so() {
    let d = decide(GpuRequest::Gpu, None);
    assert_eq!(d.ran, Ran::Cpu);
    let fb = d.fallback.clone().expect("a reason");
    assert_eq!(fb.stage, GpuStage::Library);
    assert!(fb.cause.contains("no GPU path"));
    assert!(d.fallback_line().is_some());
}

/// The cause is one line whatever the platform put in it.
#[test]
fn a_cause_is_one_line() {
    let fb = failed(
        GpuStage::Context,
        "eglChooseConfig failed\n(EGL error 0x3009)\r\n",
    );
    assert_eq!(fb.cause, "eglChooseConfig failed (EGL error 0x3009)");
    assert_eq!(failed(GpuStage::Context, "  ").cause, "no cause reported");
    assert_eq!(
        GpuFallback::injected(GpuStage::Frame).cause,
        "injected by AZ_GPU_FAIL"
    );
}

// ---------------------------------------------------------------------------------------------
// the lines
// ---------------------------------------------------------------------------------------------

/// The line the QA harness reads: exactly this shape, for each way a window can end up.
///
/// Mutation: any word, order or separator of the line changed (the harness reads `backend=`, `requested=`
/// and `device=`).
#[test]
fn the_render_line_is_the_contract_of_the_qa_harness() {
    let gpu = decide(GpuRequest::Gpu, Some(Ok(apple_gpu())));
    assert_eq!(
        gpu.render_line(),
        "[render] backend=gpu requested=gpu device=Apple A15 GPU"
    );
    let auto = decide(GpuRequest::Auto, Some(Ok(apple_gpu())));
    assert_eq!(
        auto.render_line(),
        "[render] backend=gpu requested=auto device=Apple A15 GPU"
    );
    let cpu = decide(GpuRequest::Cpu, None);
    assert_eq!(
        cpu.render_line(),
        "[render] backend=cpu requested=cpu device=software"
    );
    let fell = decide(
        GpuRequest::Gpu,
        Some(Err(failed(GpuStage::Context, "no config"))),
    );
    assert_eq!(
        fell.render_line(),
        "[render] backend=cpu requested=gpu device=software"
    );
    let auto_soft = decide(GpuRequest::Auto, Some(Ok(software_gl())));
    assert_eq!(
        auto_soft.render_line(),
        "[render] backend=cpu requested=auto device=software"
    );
}

/// The diagnostic line carries no `backend=`, so the harness's "last `[render] backend=` line wins" never
/// takes it for the answer, and it says which stage failed and why.
#[test]
fn the_fallback_line_is_never_taken_for_the_answer() {
    let d = decide(
        GpuRequest::Gpu,
        Some(Err(failed(GpuStage::Renderer, "shader link failed"))),
    );
    let line = d.fallback_line().expect("a fallback line");
    assert!(
        line.starts_with("[render] gpu unavailable at renderer: "),
        "{line}"
    );
    assert!(!line.contains("backend="), "{line}");
    assert!(line.ends_with("; drawing on the CPU"), "{line}");
}

/// The startup timeline's `render_backend` mark carries the same words as the line, without its prefix.
#[test]
fn the_timeline_detail_is_the_line_without_its_prefix() {
    let d = decide(GpuRequest::Gpu, Some(Ok(apple_gpu())));
    assert_eq!(
        d.timeline_detail(),
        "backend=gpu requested=gpu device=Apple A15 GPU"
    );
}
