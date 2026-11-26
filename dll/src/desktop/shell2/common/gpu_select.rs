//! Which renderer a phone window gets: what was asked for, what the bring-up of the GPU path said, the one
//! decision, and the one line the QA harness reads. Pure: no window, no GL, no environment.
//!
//! A phone shell (iOS, Android) draws a window with the CPU renderer or with WebRender on a GLES context, and
//! chooses once, when the window is made:
//!
//! 1. [`GpuRequest::resolve`]: what the environment (`AZ_BACKEND`) and the window's `hw_accel` ask for. Nothing
//!    asked is the CPU renderer, as it was before the GPU path existed.
//! 2. The shell brings the GPU path up stage by stage ([`GpuStage`]); a stage that fails ends the attempt with a
//!    [`GpuFallback`] (the stage and the cause the platform reports). In a TEST run `AZ_GPU_FAIL` makes a stage
//!    fail on purpose ([`FailPlan`]; the variable is a field of `core::run_mode::TestHooks`, so a shipped app
//!    ignores it).
//! 3. [`decide`] turns the request and the outcome of the attempt into a [`Decision`]: which renderer runs and, when
//!    the GPU was asked for and does not run, why.
//! 4. [`Decision::render_line`] is the ONE line the QA harness (`scripts/ci/mobile_qa.py`) reads to learn which
//!    renderer drew; [`Decision::fallback_line`] is the one diagnostic of a GPU request that fell back. A request
//!    for the GPU that silently ran on the CPU is a failure of the test, so neither line may be left out.

use azul_core::window::HwAcceleration;

use super::compositor::AzBackend;

/// The longest `AZ_GPU_FAIL` value read; the rest is ignored with a note.
pub const FAIL_SPEC_MAX_BYTES: usize = 256;
/// The most entries of an `AZ_GPU_FAIL` value read.
pub const FAIL_SPEC_MAX_ENTRIES: usize = 16;
/// The largest count of an entry (`stage:<n>`); a larger one is read as this.
pub const FAIL_COUNT_MAX: u32 = 1000;
/// The longest device or cause text a line carries.
pub const LINE_TEXT_MAX_CHARS: usize = 120;

/// The renderer an app or the environment asks for.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum GpuRequest {
    /// `AZ_BACKEND=cpu`, and what nothing asked for: the CPU renderer. No GL context is made.
    Cpu,
    /// `AZ_BACKEND=gpu`, `HwAcceleration::Enabled`: WebRender on a GLES context, whatever the renderer is (a
    /// software one included: that is how the emulator's and the Simulator's GL is tested). The CPU renderer runs
    /// only when the bring-up fails, and the fallback says why.
    Gpu,
    /// `AZ_BACKEND=auto`: as `Gpu`, but only on a hardware renderer. A software GL renderer is slower than the
    /// CPU renderer, which `auto` then prefers (the fallback says so; `gpu` forces the GL path).
    Auto,
}

impl GpuRequest {
    /// The word the log lines and `AZ_BACKEND` use.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Cpu => "cpu",
            Self::Gpu => "gpu",
            Self::Auto => "auto",
        }
    }

    /// What `AZ_BACKEND` (as the app's environment typed it) and the window's `hw_accel` ask for: the environment
    /// first, then the window, then the CPU renderer. `headless` and `web://..` are no phone renderers.
    #[must_use]
    pub fn resolve(backend: Option<&str>, hw_accel: Option<HwAcceleration>) -> Self {
        match AzBackend::resolve(backend, hw_accel) {
            AzBackend::Gpu => Self::Gpu,
            AzBackend::Auto => Self::Auto,
            _ => Self::Cpu,
        }
    }
}

/// The stages of the GPU bring-up and of its life, in the order a window meets them. The same words name the
/// stage in a fallback line and in `AZ_GPU_FAIL`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum GpuStage {
    /// The GL library or framework could not be opened (`libEGL` / `libGLESv2`, `OpenGLES.framework`).
    Library,
    /// The context (and the display or config it needs) could not be made.
    Context,
    /// The context could not be made current on the thread that draws.
    MakeCurrent,
    /// The GL entry points did not resolve (`glGetString` and `glCreateShader` are null).
    Functions,
    /// What the context says about itself is unusable: not OpenGL ES 3.0, a blacklisted or software renderer.
    Probe,
    /// The drawable could not be made (`eglCreateWindowSurface`, `renderbufferStorage:fromDrawable:`, the
    /// framebuffer is incomplete), or could not be resized.
    Surface,
    /// WebRender's renderer could not be made on the context.
    Renderer,
    /// The WebRender document or its hit tester could not be made.
    Document,
    /// The engine's own shaders do not compile at any GLSL version (`GlContextPtr::is_gl_usable`).
    Shaders,
    /// A frame could not be rendered or presented.
    Frame,
    /// The context was lost.
    Lost,
}

impl GpuStage {
    /// Every stage, in order.
    pub const ALL: [Self; 11] = [
        Self::Library,
        Self::Context,
        Self::MakeCurrent,
        Self::Functions,
        Self::Probe,
        Self::Surface,
        Self::Renderer,
        Self::Document,
        Self::Shaders,
        Self::Frame,
        Self::Lost,
    ];

    /// The stage's word (`AZ_GPU_FAIL=context`, `[render] gpu unavailable at context: ..`).
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Library => "library",
            Self::Context => "context",
            Self::MakeCurrent => "make_current",
            Self::Functions => "functions",
            Self::Probe => "probe",
            Self::Surface => "surface",
            Self::Renderer => "renderer",
            Self::Document => "document",
            Self::Shaders => "shaders",
            Self::Frame => "frame",
            Self::Lost => "lost",
        }
    }

    /// The stage a word names (any case, `-` for `_`), or `None`.
    #[must_use]
    pub fn parse(word: &str) -> Option<Self> {
        let word = word.trim().to_ascii_lowercase().replace('-', "_");
        Self::ALL.into_iter().find(|stage| stage.name() == word)
    }
}

/// The failures a TEST run asks for (`AZ_GPU_FAIL`): a comma separated list of `<stage>` (the stage fails every
/// time it is tried) and `<stage>:<n>` (it fails `n` times and then works, so a rebuild can succeed after one
/// injected failure): `context`, `surface:1`, `renderer:2,lost:1`. The stage `lost` is a simulated loss of the
/// context at the next swap. The plan is owned by the window that follows it (no global): every attempt of a
/// stage asks [`FailPlan::fails`] and a `true` is a failure with the cause "injected by AZ_GPU_FAIL".
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct FailPlan {
    /// Per stage the failures that remain; `None` is every time.
    entries: Vec<(GpuStage, Option<u32>)>,
    /// What was wrong with the value (a word that is no stage, a cap that applied), for one diagnostic.
    notes: Vec<String>,
}

impl FailPlan {
    /// The plan of an `AZ_GPU_FAIL` value; `None` or blank is the empty plan. An entry that names no stage is
    /// ignored with a note, and the caps ([`FAIL_SPEC_MAX_BYTES`], [`FAIL_SPEC_MAX_ENTRIES`],
    /// [`FAIL_COUNT_MAX`]) apply with a note.
    #[must_use]
    pub fn parse(spec: Option<&str>) -> Self {
        let mut plan = Self::default();
        let Some(spec) = spec.map(str::trim).filter(|s| !s.is_empty()) else {
            return plan;
        };
        let mut spec = spec;
        if spec.len() > FAIL_SPEC_MAX_BYTES {
            let mut end = FAIL_SPEC_MAX_BYTES;
            while !spec.is_char_boundary(end) {
                end -= 1;
            }
            spec = &spec[..end];
            plan.notes.push(format!(
                "AZ_GPU_FAIL: only the first {FAIL_SPEC_MAX_BYTES} bytes are read"
            ));
        }
        for (index, entry) in spec.split(',').enumerate() {
            if index >= FAIL_SPEC_MAX_ENTRIES {
                plan.notes.push(format!(
                    "AZ_GPU_FAIL: only the first {FAIL_SPEC_MAX_ENTRIES} entries are read"
                ));
                break;
            }
            let entry = entry.trim();
            if entry.is_empty() {
                continue;
            }
            let (word, count) = match entry.split_once(':') {
                Some((word, count)) => (word, Some(count.trim())),
                None => (entry, None),
            };
            let Some(stage) = GpuStage::parse(word) else {
                plan.notes.push(format!(
                    "AZ_GPU_FAIL: {:?} is no stage, ignored",
                    word.trim()
                ));
                continue;
            };
            let remaining = match count {
                None => None,
                Some(text) => match text.parse::<u32>() {
                    Ok(n) if n > FAIL_COUNT_MAX => {
                        plan.notes.push(format!(
                            "AZ_GPU_FAIL: {} fails at most {FAIL_COUNT_MAX} times",
                            stage.name()
                        ));
                        Some(FAIL_COUNT_MAX)
                    }
                    Ok(n) => Some(n),
                    Err(_) => {
                        plan.notes.push(format!(
                            "AZ_GPU_FAIL: {:?} is no count for {}, ignored",
                            text,
                            stage.name()
                        ));
                        continue;
                    }
                },
            };
            plan.entries.push((stage, remaining));
        }
        plan
    }

    /// Does this attempt of `stage` fail? Consumes one of a counted entry.
    pub fn fails(&mut self, stage: GpuStage) -> bool {
        for (entry_stage, remaining) in &mut self.entries {
            if *entry_stage != stage {
                continue;
            }
            match remaining {
                None => return true,
                // spent: a later entry of the same stage may still be owed
                Some(0) => {}
                Some(n) => {
                    *n -= 1;
                    return true;
                }
            }
        }
        false
    }

    /// Nothing is planned to fail.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// What was wrong with the value, one line each (the caller logs them once).
    #[must_use]
    pub fn notes(&self) -> &[String] {
        &self.notes
    }
}

/// What the context says about itself (the strings of `glGetString`).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct DeviceInfo {
    /// `GL_VENDOR`.
    pub vendor: String,
    /// `GL_RENDERER`: the line's `device=` text.
    pub renderer: String,
    /// `GL_VERSION`, `OpenGL ES 3.0 APPLE-24.5.5`.
    pub version: String,
    /// `GL_SHADING_LANGUAGE_VERSION`.
    pub glsl: String,
}

impl DeviceInfo {
    /// The major and minor of the `OpenGL ES x.y` the version string names, or `None` for any other text.
    #[must_use]
    pub fn gles_version(&self) -> Option<(u32, u32)> {
        let rest = self.version.trim().strip_prefix("OpenGL ES")?;
        // `OpenGL ES-CM 1.1` is the common profile of OpenGL ES 1.x
        let rest = rest.trim_start_matches("-CM").trim_start();
        let mut numbers = rest
            .split(|c: char| !c.is_ascii_digit())
            .filter(|part| !part.is_empty());
        let major = numbers.next()?.parse().ok()?;
        let minor = numbers
            .next()
            .and_then(|part| part.parse().ok())
            .unwrap_or(0);
        Some((major, minor))
    }

    /// WebRender needs OpenGL ES 3.0 (texture storage, instancing, uniform blocks, pixel buffers).
    #[must_use]
    pub fn is_es3(&self) -> bool {
        self.gles_version().is_some_and(|(major, _)| major >= 3)
    }

    /// Is the renderer a software one (the Simulator's and the emulator's SwiftShader, Mesa's rasterisers)?
    #[must_use]
    pub fn is_software(&self) -> bool {
        let renderer = self.renderer.to_ascii_lowercase();
        ["software", "swiftshader", "llvmpipe", "softpipe", "swrast"]
            .iter()
            .any(|word| renderer.contains(word))
    }

    /// The renderer as one line of at most [`LINE_TEXT_MAX_CHARS`] characters (`unknown` when blank).
    #[must_use]
    pub fn describe(&self) -> String {
        one_line(&self.renderer, "unknown")
    }
}

/// `text` as one line: whitespace and control characters (newlines included) become one space, capped at
/// [`LINE_TEXT_MAX_CHARS`] characters; `blank` stands for nothing.
fn one_line(text: &str, blank: &str) -> String {
    // Control characters too: a line with a NUL in it is dropped whole by Android's logger.
    let joined = super::gles_probe::clean_driver_string(text);
    if joined.is_empty() {
        return blank.to_string();
    }
    if joined.chars().count() <= LINE_TEXT_MAX_CHARS {
        return joined;
    }
    let mut cut: String = joined.chars().take(LINE_TEXT_MAX_CHARS - 1).collect();
    cut.push('~');
    cut
}

/// Why the GPU path does not run: the stage that ended the attempt and the cause the platform reports.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GpuFallback {
    /// The stage.
    pub stage: GpuStage,
    /// The platform's own words (an EGL error code, a failed selector), one line.
    pub cause: String,
}

impl GpuFallback {
    /// A fallback at `stage`, `cause` as one line.
    #[must_use]
    pub fn new(stage: GpuStage, cause: &str) -> Self {
        Self {
            stage,
            cause: one_line(cause, "no cause reported"),
        }
    }

    /// The cause of a stage that `AZ_GPU_FAIL` failed on purpose.
    #[must_use]
    pub fn injected(stage: GpuStage) -> Self {
        Self::new(stage, "injected by AZ_GPU_FAIL")
    }
}

/// The renderer that runs.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Ran {
    /// The CPU renderer.
    Cpu,
    /// WebRender on the GLES context.
    Gpu,
}

/// The outcome of the choice: what was asked, what runs, and why not the GPU where it was asked for.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Decision {
    /// What was asked for.
    pub requested: GpuRequest,
    /// What runs.
    pub ran: Ran,
    /// Why the GPU does not run although it was asked for (`None` when it runs, or was not asked for).
    pub fallback: Option<GpuFallback>,
    /// What the context said about itself, when a context was made.
    pub device: Option<DeviceInfo>,
}

/// The decision, once the bring-up has answered.
///
/// `attempt` is `None` when no attempt was made. For a [`GpuRequest::Cpu`] that is right; for a request of the GPU
/// it means this build has no GPU path, and that is a fallback at the first stage. An attempt that succeeded is
/// checked once more against what the context says: OpenGL ES 3.0 is required whatever was asked, and `auto` does
/// not take a software renderer.
#[must_use]
pub fn decide(requested: GpuRequest, attempt: Option<Result<DeviceInfo, GpuFallback>>) -> Decision {
    let cpu = |fallback: Option<GpuFallback>, device: Option<DeviceInfo>| Decision {
        requested,
        ran: Ran::Cpu,
        fallback,
        device,
    };
    if requested == GpuRequest::Cpu {
        return cpu(None, None);
    }
    match attempt {
        None => cpu(
            Some(GpuFallback::new(
                GpuStage::Library,
                "this build has no GPU path",
            )),
            None,
        ),
        Some(Err(fallback)) => cpu(Some(fallback), None),
        Some(Ok(info)) if !info.is_es3() => {
            let cause = format!(
                "WebRender needs OpenGL ES 3.0, this context says {:?}",
                one_line(&info.version, "nothing")
            );
            cpu(Some(GpuFallback::new(GpuStage::Probe, &cause)), Some(info))
        }
        Some(Ok(info)) if requested == GpuRequest::Auto && info.is_software() => {
            let cause = format!(
                "auto does not take the software renderer {:?} (AZ_BACKEND=gpu forces the GL path)",
                info.describe()
            );
            cpu(Some(GpuFallback::new(GpuStage::Probe, &cause)), Some(info))
        }
        Some(Ok(info)) => Decision {
            requested,
            ran: Ran::Gpu,
            fallback: None,
            device: Some(info),
        },
    }
}

impl Decision {
    /// The ONE line the QA harness reads, written once when a window gets its renderer (the last such line
    /// wins when a window is made again):
    ///
    /// `[render] backend=<cpu|gpu> requested=<cpu|gpu|auto> device=<text>`
    ///
    /// `device` is the GL renderer string of a GPU window and `software` for the CPU renderer.
    #[must_use]
    pub fn render_line(&self) -> String {
        let (backend, device) = match (self.ran, &self.device) {
            (Ran::Gpu, Some(info)) => ("gpu", info.describe()),
            (Ran::Gpu, None) => ("gpu", "unknown".to_string()),
            (Ran::Cpu, _) => ("cpu", "software".to_string()),
        };
        format!(
            "[render] backend={backend} requested={} device={device}",
            self.requested.as_str()
        )
    }

    /// The one diagnostic of a request for the GPU that fell back, naming the stage and the cause; `None` when
    /// there was nothing to say. It carries no `backend=` so that the harness takes only [`Self::render_line`].
    #[must_use]
    pub fn fallback_line(&self) -> Option<String> {
        let fallback = self.fallback.as_ref()?;
        Some(format!(
            "[render] gpu unavailable at {}: {}; drawing on the CPU",
            fallback.stage.name(),
            fallback.cause
        ))
    }

    /// The detail of the startup timeline's `render_backend` mark: the same words as [`Self::render_line`] after
    /// the `[render] ` prefix.
    #[must_use]
    pub fn timeline_detail(&self) -> String {
        self.render_line()
            .trim_start_matches("[render] ")
            .to_string()
    }
}

#[cfg(test)]
#[path = "gpu_select_test.rs"]
mod gpu_select_test;
