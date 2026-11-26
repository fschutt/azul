//! The lifecycle machine of the phones' GPU path, against a fake platform that enforces the rules of the real
//! ones: Android's window has one producer and its surface must go before the window; iOS ends an app that
//! issues a GL command in the background; a context is made once, current before a renderer is built on it.
//! No window, no GL.

use std::collections::HashMap;

use super::*;

fn surface(width: u32, height: u32) -> Surface {
    Surface::new(width, height, 3.0)
}

/// A phone's portrait screen at 3x.
fn portrait() -> Surface {
    surface(1170, 2532)
}

/// The same screen turned.
fn landscape() -> Surface {
    surface(2532, 1170)
}

fn name(action: &Action) -> &'static str {
    match action {
        Action::CreateContext => "CreateContext",
        Action::MakeCurrent => "MakeCurrent",
        Action::CreateSurface(_) => "CreateSurface",
        Action::ResizeSurface(_) => "ResizeSurface",
        Action::DestroySurface => "DestroySurface",
        Action::BuildRenderer => "BuildRenderer",
        Action::DestroyRenderer => "DestroyRenderer",
        Action::DestroyContext => "DestroyContext",
        Action::ReleaseCaches => "ReleaseCaches",
        Action::StopFrames => "StopFrames",
        Action::StartFrames => "StartFrames",
        Action::Finish => "Finish",
        Action::Present => "Present",
        Action::AbandonGl => "AbandonGl",
        Action::FallBackToCpu(_) => "FallBackToCpu",
    }
}

/// The platform the actions are performed on: what exists, and the rules that end an app that breaks them.
struct Platform {
    machine: GpuLifecycle,
    context: bool,
    current: bool,
    surface: Option<Surface>,
    renderer: bool,
    /// The native window the surface draws on (Android's `ANativeWindow`; iOS's layer).
    window: bool,
    /// The CPU renderer has taken the window.
    cpu_has_window: bool,
    /// The platform forbids GL commands (iOS, after `applicationDidEnterBackground:` has returned).
    forbidden: bool,
    frames: bool,
    leaked: usize,
    presented: usize,
    /// Per action: the stages it is to fail with, one per attempt, the last first.
    fail: HashMap<&'static str, Vec<GpuStage>>,
}

impl Platform {
    fn new() -> Self {
        Self {
            machine: GpuLifecycle::new(),
            context: false,
            current: false,
            surface: None,
            renderer: false,
            window: true,
            cpu_has_window: false,
            forbidden: false,
            frames: false,
            leaked: 0,
            presented: 0,
            fail: HashMap::new(),
        }
    }

    /// The next attempt of `action` fails at `stage` (several calls: the attempts after it).
    fn failing(mut self, action: &'static str, stage: GpuStage) -> Self {
        self.fail.entry(action).or_default().insert(0, stage);
        self
    }

    /// A window with everything up.
    fn ready() -> Self {
        let mut platform = Self::new();
        platform.feed(Event::SurfaceCreated(portrait()));
        assert_eq!(platform.machine.phase(), Phase::Ready);
        platform
    }

    /// Give the machine an event and perform its answer, as the shell's executor does: in order, stopping at
    /// the first failure, which is reported back. Returns what was performed (a failed action with a `!`).
    fn feed(&mut self, event: Event) -> Vec<String> {
        // The app that is told it is back is active again while the handler runs; one that is in the
        // background has no GL for any other event.
        self.forbidden =
            self.machine.suspend() == Suspend::Background && !matches!(event, Event::Resumed);
        let actions = self.machine.step(event);
        // An answer that ends the machine is a teardown: the executor performs all of it, whatever fails.
        let ended = matches!(self.machine.phase(), Phase::Fallen | Phase::Closed);
        let mut performed = Vec::new();
        for (index, action) in actions.iter().enumerate() {
            assert!(
                !matches!(action, Action::FallBackToCpu(_)) || index + 1 == actions.len(),
                "FallBackToCpu must be the last action of its answer: {actions:?}"
            );
            match self.perform(action) {
                Ok(()) => performed.push(name(action).to_string()),
                Err(_) if ended => performed.push(format!("{}!", name(action))),
                Err(fallback) => {
                    performed.push(format!("{}!", name(action)));
                    performed.extend(self.feed(Event::ActionFailed(fallback)));
                    break;
                }
            }
        }
        // the handler has returned: from here on the platform's rule for the app's state holds
        self.forbidden = self.machine.suspend() == Suspend::Background;
        self.agrees();
        performed
    }

    /// One action on the platform. A GL command in the background, a second context, a surface on a window
    /// the CPU holds, a renderer without a current context: each is what the real platform punishes.
    fn perform(&mut self, action: &Action) -> Result<(), GpuFallback> {
        let kind = name(action);
        let gl_command = !matches!(
            action,
            Action::StopFrames | Action::StartFrames | Action::AbandonGl | Action::FallBackToCpu(_)
        );
        assert!(
            !(gl_command && self.forbidden),
            "{kind}: a GL command in the background ends the app"
        );
        if let Some(stages) = self.fail.get_mut(kind) {
            if let Some(stage) = stages.pop() {
                return Err(GpuFallback::injected(stage));
            }
        }
        match action {
            Action::CreateContext => {
                assert!(!self.context, "a second context");
                self.context = true;
            }
            Action::MakeCurrent => {
                assert!(self.context, "MakeCurrent without a context");
                self.current = true;
            }
            Action::CreateSurface(made) => {
                assert!(self.context, "a surface without a context");
                assert!(self.window, "a surface for a window that is gone");
                assert!(
                    !self.cpu_has_window,
                    "the CPU path holds the window: no EGL surface can connect to it"
                );
                assert!(self.surface.is_none(), "a second surface");
                self.surface = Some(*made);
            }
            Action::ResizeSurface(made) => {
                assert!(self.surface.is_some(), "a resize without a surface");
                assert!(
                    self.current,
                    "storage reallocated without a current context"
                );
                self.surface = Some(*made);
            }
            Action::DestroySurface => self.surface = None,
            Action::BuildRenderer => {
                assert!(
                    self.context && self.surface.is_some() && self.current,
                    "a renderer needs a current context with a surface"
                );
                assert!(!self.renderer, "a second renderer");
                self.renderer = true;
            }
            Action::DestroyRenderer => {
                // the executor makes the context current itself: a renderer is deinit'd on a current one
                if self.context {
                    self.current = true;
                }
                self.renderer = false;
            }
            Action::DestroyContext => {
                assert!(self.context, "DestroyContext without a context");
                assert!(
                    !self.renderer,
                    "the context is destroyed with its renderer alive"
                );
                self.context = false;
                self.current = false;
            }
            Action::ReleaseCaches => {
                assert!(self.renderer && self.current, "caches without a renderer")
            }
            Action::StopFrames => self.frames = false,
            Action::StartFrames => self.frames = true,
            Action::Finish => assert!(self.context && self.current, "glFinish without a context"),
            Action::Present => {
                assert!(
                    self.context && self.renderer && self.surface.is_some() && self.current,
                    "a frame needs a current context, a surface and a renderer"
                );
                self.presented += 1;
            }
            Action::AbandonGl => {
                self.leaked += usize::from(self.context)
                    + usize::from(self.renderer)
                    + usize::from(self.surface.is_some());
                self.context = false;
                self.renderer = false;
                self.surface = None;
                self.current = false;
            }
            Action::FallBackToCpu(_) => {
                assert!(
                    self.surface.is_none(),
                    "the CPU path locks the window: the GL surface must be destroyed first"
                );
                assert!(!self.context && !self.renderer, "GL objects left behind");
                self.cpu_has_window = true;
            }
        }
        Ok(())
    }

    /// The machine believes what is true.
    fn agrees(&self) {
        assert_eq!(self.machine.has_context(), self.context, "context");
        assert_eq!(self.machine.has_renderer(), self.renderer, "renderer");
        assert_eq!(self.machine.surface(), self.surface, "surface");
        if matches!(self.machine.phase(), Phase::Fallen | Phase::Closed) {
            assert!(
                !self.context && !self.renderer && self.surface.is_none(),
                "left behind"
            );
            assert!(!self.frames, "frames run after the end");
        }
    }

    /// The native window is released (Android: after `TerminateWindow`).
    fn release_window(&mut self) {
        assert!(
            self.surface.is_none(),
            "the window is released with its surface alive: the surface goes first"
        );
        self.window = false;
    }

    /// A new native window.
    fn new_window(&mut self) {
        assert!(!self.window, "a window made over a window");
        self.window = true;
    }
}

fn trace(performed: &[String]) -> Vec<&str> {
    performed.iter().map(String::as_str).collect()
}

const BRING_UP: [&str; 5] = [
    "CreateContext",
    "CreateSurface",
    "MakeCurrent",
    "BuildRenderer",
    "StartFrames",
];

// ---------------------------------------------------------------------------------------------
// first surface, double init, resize
// ---------------------------------------------------------------------------------------------

/// The first surface brings the context, the surface and the renderer up, in the one order both platforms can
/// follow, and frames run.
///
/// Mutation: any step of the order moved or left out.
#[test]
fn the_first_surface_brings_everything_up_in_order() {
    let mut platform = Platform::new();
    assert_eq!(platform.machine.phase(), Phase::NoContext);
    let performed = platform.feed(Event::SurfaceCreated(portrait()));
    assert_eq!(trace(&performed), BRING_UP);
    assert_eq!(platform.machine.phase(), Phase::Ready);
    assert!(platform.frames);
    assert_eq!(trace(&platform.feed(Event::Frame)), ["Present"]);
    assert_eq!(platform.presented, 1);
}

/// A second `SurfaceCreated` of the same size is the same surface: nothing is made twice. Of another size it is
/// a resize.
///
/// Mutation: a second context or surface; a second surface of another size taken for the same one.
#[test]
fn a_second_init_of_the_same_surface_does_nothing() {
    let mut platform = Platform::ready();
    assert!(platform.feed(Event::SurfaceCreated(portrait())).is_empty());
    assert_eq!(
        trace(&platform.feed(Event::SurfaceCreated(landscape()))),
        ["MakeCurrent", "ResizeSurface"]
    );
    assert_eq!(platform.surface, Some(landscape()));
}

/// A resize reallocates the drawable once; the same size, a collapse to nothing and a size before any surface
/// asked for are no resize.
///
/// Mutation: a resize at every `layoutSubviews`; a drawable of no area.
#[test]
fn a_resize_reallocates_the_drawable_only_when_it_changed() {
    let mut platform = Platform::ready();
    assert!(platform.feed(Event::Resized(portrait())).is_empty());
    assert_eq!(
        trace(&platform.feed(Event::Resized(landscape()))),
        ["MakeCurrent", "ResizeSurface"]
    );
    assert!(platform.feed(Event::Resized(landscape())).is_empty());
    assert!(
        platform.feed(Event::Resized(surface(0, 0))).is_empty(),
        "a view that collapsed for a moment keeps its drawable"
    );
    assert_eq!(platform.surface, Some(landscape()));
    // a new scale (the user moved the window to another screen, the density changed)
    let denser = Surface::new(2532, 1170, 2.0);
    assert_eq!(
        trace(&platform.feed(Event::Resized(denser))),
        ["MakeCurrent", "ResizeSurface"]
    );
    // a resize before any surface was asked for creates nothing
    let mut fresh = Platform::new();
    assert!(fresh.feed(Event::Resized(portrait())).is_empty());
    assert_eq!(fresh.machine.phase(), Phase::NoContext);
}

/// A view that is not laid out yet has no area: nothing is made until it has one.
///
/// Mutation: a context made for a surface of 0 x 0.
#[test]
fn a_surface_with_no_area_is_waited_for() {
    let mut platform = Platform::new();
    assert!(platform
        .feed(Event::SurfaceCreated(surface(0, 0)))
        .is_empty());
    assert_eq!(platform.machine.phase(), Phase::NoContext);
    assert!(!platform.context);
    let performed = platform.feed(Event::Resized(portrait()));
    assert_eq!(trace(&performed), BRING_UP);
    assert_eq!(platform.machine.phase(), Phase::Ready);
}

/// A size no GPU takes, and a scale that is no number, are cut before they reach the platform.
#[test]
fn the_surface_is_capped() {
    let mut platform = Platform::new();
    platform.feed(Event::SurfaceCreated(Surface::new(
        u32::MAX,
        100_000,
        f32::NAN,
    )));
    let made = platform.surface.expect("a surface");
    assert_eq!(made.width, SURFACE_EDGE_MAX);
    assert_eq!(made.height, SURFACE_EDGE_MAX);
    assert!((made.scale - 1.0).abs() < f32::EPSILON);
}

/// A frame is asked for while there is nothing to draw on: nothing is done.
#[test]
fn a_frame_without_a_surface_is_nothing() {
    let mut platform = Platform::new();
    assert!(platform.feed(Event::Frame).is_empty());
    platform.feed(Event::SurfaceCreated(surface(0, 0)));
    assert!(platform.feed(Event::Frame).is_empty());
}

// ---------------------------------------------------------------------------------------------
// the surface goes away under a live context (Android)
// ---------------------------------------------------------------------------------------------

/// `TerminateWindow`: the surface is destroyed BEFORE the native window is released; the context and the
/// renderer are kept, and the next window gets a surface without a new context or renderer.
///
/// Mutation: a surface left to the window's release; a context made again with the next window.
#[test]
fn the_surface_goes_before_the_window_and_the_context_stays() {
    let mut platform = Platform::ready();
    let performed = platform.feed(Event::SurfaceDestroyed);
    assert_eq!(trace(&performed), ["StopFrames", "DestroySurface"]);
    platform.release_window();
    assert_eq!(platform.machine.phase(), Phase::Incomplete);
    assert!(platform.context && platform.renderer, "the context is kept");
    assert!(platform.feed(Event::Frame).is_empty(), "nothing to draw on");

    platform.new_window();
    let performed = platform.feed(Event::SurfaceCreated(landscape()));
    assert_eq!(
        trace(&performed),
        ["CreateSurface", "MakeCurrent", "StartFrames"]
    );
    assert_eq!(platform.machine.phase(), Phase::Ready);
    assert_eq!(trace(&platform.feed(Event::Frame)), ["Present"]);
}

/// A destroyed surface that was never made, or already gone, is a no-op.
#[test]
fn a_destroyed_surface_twice_is_destroyed_once() {
    let mut platform = Platform::ready();
    platform.feed(Event::SurfaceDestroyed);
    assert!(platform.feed(Event::SurfaceDestroyed).is_empty());
    let mut fresh = Platform::new();
    assert!(fresh.feed(Event::SurfaceDestroyed).is_empty());
}

/// A size reported after the window went (a late density change) makes no surface.
#[test]
fn a_resize_after_the_window_went_makes_no_surface() {
    let mut platform = Platform::ready();
    platform.feed(Event::SurfaceDestroyed);
    assert!(platform.feed(Event::Resized(landscape())).is_empty());
    assert_eq!(platform.surface, None);
}

// ---------------------------------------------------------------------------------------------
// the context is lost
// ---------------------------------------------------------------------------------------------

/// A loss with a surface alive rebuilds: the old objects are abandoned (the context is gone, so is every GL
/// object), a new context, surface and renderer are made under the same window.
///
/// Mutation: a rebuild that deinit's the renderer of a lost context; a rebuild without the renderer.
#[test]
fn a_lost_context_is_rebuilt_under_the_same_window() {
    let mut platform = Platform::ready();
    let performed = platform.feed(Event::ContextLost);
    assert_eq!(
        trace(&performed),
        [
            "StopFrames",
            "DestroySurface",
            "AbandonGl",
            "CreateContext",
            "CreateSurface",
            "MakeCurrent",
            "BuildRenderer",
            "StartFrames"
        ]
    );
    assert_eq!(platform.machine.phase(), Phase::Ready);
    assert_eq!(platform.leaked, 2, "the lost context and its renderer");
    assert_eq!(platform.machine.losses(), 1);
}

/// Twice without a frame in between is the CPU renderer's turn: the second loss tears down in the order the
/// platforms need and falls back, with the cause that says why.
///
/// Mutation: a third rebuild; the fall back before the surface is destroyed.
#[test]
fn a_context_lost_twice_without_a_frame_falls_back() {
    let mut platform = Platform::ready();
    platform.feed(Event::ContextLost);
    let performed = platform.feed(Event::ContextLost);
    assert_eq!(
        trace(&performed),
        [
            "StopFrames",
            "DestroySurface",
            "DestroyRenderer",
            "DestroyContext",
            "FallBackToCpu"
        ]
    );
    assert_eq!(platform.machine.phase(), Phase::Fallen);
    assert!(platform.cpu_has_window);
    let fallback = platform.machine.fallback().expect("a fallback");
    assert_eq!(fallback.stage, GpuStage::Lost);
    assert!(fallback.cause.contains("twice"), "{}", fallback.cause);
    assert!(platform.feed(Event::Frame).is_empty());
    assert!(platform.feed(Event::SurfaceCreated(portrait())).is_empty());
    assert!(platform.feed(Event::Resized(landscape())).is_empty());
}

/// A frame that went out between two losses forgives the first.
///
/// Mutation: losses that are never forgiven.
#[test]
fn a_frame_between_two_losses_forgives_the_first() {
    let mut platform = Platform::ready();
    platform.feed(Event::ContextLost);
    platform.feed(Event::Frame);
    platform.feed(Event::FramePresented);
    assert_eq!(platform.machine.losses(), 0);
    let performed = platform.feed(Event::ContextLost);
    assert_eq!(performed.first().map(String::as_str), Some("StopFrames"));
    assert!(
        performed.contains(&"BuildRenderer".to_string()),
        "rebuilt, not fallen: {performed:?}"
    );
    assert_eq!(platform.machine.phase(), Phase::Ready);
}

/// A frame that cannot go out is the same trouble as a lost context: one rebuild, then the CPU.
///
/// Mutation: a failing frame that falls back at once, or never.
#[test]
fn a_frame_that_cannot_go_out_is_a_loss() {
    let mut platform = Platform::ready().failing("Present", GpuStage::Frame);
    let performed = platform.feed(Event::Frame);
    assert_eq!(performed.first().map(String::as_str), Some("Present!"));
    assert!(
        performed.contains(&"AbandonGl".to_string()),
        "{performed:?}"
    );
    assert_eq!(platform.machine.phase(), Phase::Ready, "rebuilt once");
    platform = platform
        .failing("Present", GpuStage::Frame)
        .failing("Present", GpuStage::Frame);
    let performed = platform.feed(Event::Frame);
    assert_eq!(performed.first().map(String::as_str), Some("Present!"));
    assert_eq!(
        platform.machine.phase(),
        Phase::Fallen,
        "a second loss in a row"
    );
}

/// A loss with no surface alive (Android between two windows) has nothing to rebuild a frame on: the old
/// objects are abandoned and the next surface brings a new context.
#[test]
fn a_lost_context_with_no_surface_waits_for_the_next_surface() {
    let mut platform = Platform::ready();
    platform.feed(Event::SurfaceDestroyed);
    platform.release_window();
    assert_eq!(trace(&platform.feed(Event::ContextLost)), ["AbandonGl"]);
    assert_eq!(platform.machine.phase(), Phase::NoContext);
    platform.new_window();
    assert_eq!(
        trace(&platform.feed(Event::SurfaceCreated(portrait()))),
        BRING_UP
    );
}

/// A loss before any context exists is nothing.
#[test]
fn a_loss_without_a_context_is_nothing() {
    let mut platform = Platform::new();
    assert!(platform.feed(Event::ContextLost).is_empty());
    assert_eq!(platform.machine.losses(), 0);
}

// ---------------------------------------------------------------------------------------------
// the app is paused, in the background, back (iOS)
// ---------------------------------------------------------------------------------------------

/// `applicationWillResignActive:`: the frames stop and every command is made to finish, in the state the app
/// wants to be found in.
///
/// Mutation: frames that run on; no `glFinish`.
#[test]
fn pausing_stops_the_frames_and_finishes() {
    let mut platform = Platform::ready();
    let performed = platform.feed(Event::Paused);
    assert_eq!(trace(&performed), ["StopFrames", "MakeCurrent", "Finish"]);
    assert!(!platform.frames);
    assert_eq!(platform.machine.phase(), Phase::Suspended);
    assert!(
        platform.feed(Event::Frame).is_empty(),
        "no frame while inactive"
    );
    assert!(
        platform.feed(Event::Paused).is_empty(),
        "paused twice is paused once"
    );
}

/// `applicationDidEnterBackground:`: the last GL the platform allows, and then the fake platform bites at any
/// GL command at all: every other event of the background is answered without one.
///
/// Mutation: any event that answers with a GL action in the background (the fake panics).
#[test]
fn the_background_is_answered_without_a_gl_command() {
    let mut platform = Platform::ready();
    platform.feed(Event::Paused);
    let performed = platform.feed(Event::Backgrounded);
    assert_eq!(trace(&performed), ["MakeCurrent", "Finish"]);
    assert!(platform.forbidden);
    assert_eq!(platform.machine.suspend(), Suspend::Background);
    assert!(!platform.machine.gl_allowed());
    for event in [
        Event::Frame,
        Event::Resized(landscape()),
        Event::SurfaceCreated(landscape()),
        Event::TrimMemory(TrimLevel::Light),
        Event::TrimMemory(TrimLevel::Heavy),
        Event::ContextLost,
        Event::Paused,
        Event::Backgrounded,
        Event::FramePresented,
    ] {
        let performed = platform.feed(event.clone());
        assert!(
            performed.is_empty(),
            "{event:?} in the background must not touch GL: {performed:?}"
        );
    }
}

/// The fake really does refuse a GL command in the background (so the test above can fail).
#[test]
#[should_panic(expected = "a GL command in the background")]
fn the_fake_platform_refuses_a_gl_command_in_the_background() {
    let mut platform = Platform::ready();
    platform.forbidden = true;
    let _ = platform.perform(&Action::Present);
}

/// Back in the foreground the frames start again and one frame is drawn at once.
///
/// Mutation: no first frame; frames that stay stopped.
#[test]
fn resuming_restarts_the_frames_and_draws_one() {
    let mut platform = Platform::ready();
    platform.feed(Event::Paused);
    platform.feed(Event::Backgrounded);
    let before = platform.presented;
    let performed = platform.feed(Event::Resumed);
    assert_eq!(trace(&performed), ["MakeCurrent", "StartFrames", "Present"]);
    assert!(!platform.forbidden && platform.frames);
    assert_eq!(platform.presented, before + 1);
    assert_eq!(platform.machine.phase(), Phase::Ready);
    assert!(
        platform.feed(Event::Resumed).is_empty(),
        "resumed twice is resumed once"
    );
}

/// A pause that ends without the background (the control centre) resumes the same way.
#[test]
fn an_inactive_spell_without_the_background_resumes() {
    let mut platform = Platform::ready();
    platform.feed(Event::Paused);
    let performed = platform.feed(Event::Resumed);
    assert_eq!(trace(&performed), ["MakeCurrent", "StartFrames", "Present"]);
}

/// A rotation that came while the app was away is applied when it is back, once, before the first frame.
///
/// Mutation: the old size drawn; a resize at every event in the background.
#[test]
fn a_rotation_in_the_background_is_applied_at_the_resume() {
    let mut platform = Platform::ready();
    platform.feed(Event::Paused);
    platform.feed(Event::Backgrounded);
    platform.feed(Event::Resized(landscape()));
    platform.feed(Event::Resized(landscape()));
    assert_eq!(
        platform.surface,
        Some(portrait()),
        "nothing was reallocated in the background"
    );
    let performed = platform.feed(Event::Resumed);
    assert_eq!(
        trace(&performed),
        ["MakeCurrent", "ResizeSurface", "StartFrames", "Present"]
    );
    assert_eq!(platform.surface, Some(landscape()));
}

/// A memory request that came in the background is answered when the app is back (or, if the app went to the
/// background owing one, as the last GL of the handler).
///
/// Mutation: a trim that is dropped; one that touches GL in the background.
#[test]
fn a_memory_warning_in_the_background_is_answered_at_the_resume() {
    let mut platform = Platform::ready();
    platform.feed(Event::Paused);
    platform.feed(Event::Backgrounded);
    assert!(platform
        .feed(Event::TrimMemory(TrimLevel::Light))
        .is_empty());
    let performed = platform.feed(Event::Resumed);
    assert_eq!(
        trace(&performed),
        ["MakeCurrent", "ReleaseCaches", "StartFrames", "Present"]
    );
}

/// A memory request that came while the app was inactive, when it goes on into the background: the owed trim is
/// the first GL of the handler, before everything is made to finish.
#[test]
fn a_trim_owed_when_the_background_comes_runs_in_its_handler() {
    let mut platform = Platform::ready();
    platform.feed(Event::Paused);
    assert!(platform
        .feed(Event::TrimMemory(TrimLevel::Light))
        .is_empty());
    let performed = platform.feed(Event::Backgrounded);
    assert_eq!(
        trace(&performed),
        ["MakeCurrent", "ReleaseCaches", "Finish"]
    );
}

/// A loss that was reported in the background is rebuilt when the app is back, with the frames and a first
/// frame after it.
#[test]
fn a_lost_context_in_the_background_is_rebuilt_at_the_resume() {
    let mut platform = Platform::ready();
    platform.feed(Event::Paused);
    platform.feed(Event::Backgrounded);
    assert!(platform.feed(Event::ContextLost).is_empty());
    let performed = platform.feed(Event::Resumed);
    assert_eq!(
        trace(&performed),
        [
            "DestroySurface",
            "AbandonGl",
            "CreateContext",
            "CreateSurface",
            "MakeCurrent",
            "BuildRenderer",
            "StartFrames",
            "Present"
        ]
    );
    assert_eq!(platform.machine.phase(), Phase::Ready);
}

/// A window made while the app is in the background gets its context when the app is back, not before.
///
/// Mutation: a context made in the background (the fake panics).
#[test]
fn a_surface_asked_for_in_the_background_is_made_at_the_resume() {
    let mut platform = Platform::new();
    platform.feed(Event::Backgrounded);
    assert!(platform.feed(Event::SurfaceCreated(portrait())).is_empty());
    assert!(!platform.context);
    let performed = platform.feed(Event::Resumed);
    assert_eq!(
        trace(&performed),
        [
            "CreateContext",
            "CreateSurface",
            "MakeCurrent",
            "BuildRenderer",
            "StartFrames",
            "Present"
        ]
    );
}

/// Closing in the background makes no GL command and no call that flushes one: what exists is abandoned.
///
/// Mutation: a deletion or a context destroy in the background (the fake panics).
#[test]
fn closing_in_the_background_abandons_instead_of_deleting() {
    let mut platform = Platform::ready();
    platform.feed(Event::Paused);
    platform.feed(Event::Backgrounded);
    let performed = platform.feed(Event::Shutdown);
    assert_eq!(trace(&performed), ["AbandonGl"]);
    assert_eq!(platform.machine.phase(), Phase::Closed);
}

/// A surface that goes in the background is abandoned too (not an iOS event, but the rule holds for it).
#[test]
fn a_surface_that_goes_in_the_background_is_abandoned() {
    let mut platform = Platform::ready();
    platform.feed(Event::Paused);
    platform.feed(Event::Backgrounded);
    let performed = platform.feed(Event::SurfaceDestroyed);
    assert_eq!(trace(&performed), ["AbandonGl"]);
    assert_eq!(platform.machine.phase(), Phase::Suspended);
    assert!(
        platform.feed(Event::Resumed).is_empty(),
        "no surface was asked for: nothing is made"
    );
}

// ---------------------------------------------------------------------------------------------
// memory
// ---------------------------------------------------------------------------------------------

/// A light trim releases the renderer's caches and keeps everything else; before there is anything to trim it
/// is nothing.
#[test]
fn a_light_trim_releases_the_caches_and_keeps_everything() {
    let mut platform = Platform::ready();
    assert_eq!(
        trace(&platform.feed(Event::TrimMemory(TrimLevel::Light))),
        ["MakeCurrent", "ReleaseCaches"]
    );
    assert_eq!(platform.machine.phase(), Phase::Ready);
    let mut fresh = Platform::new();
    assert!(fresh.feed(Event::TrimMemory(TrimLevel::Heavy)).is_empty());
}

/// A heavy trim with a window on screen is a light one: the window needs its renderer.
///
/// Mutation: a renderer destroyed under a visible window.
#[test]
fn a_heavy_trim_with_a_surface_is_a_light_one() {
    let mut platform = Platform::ready();
    assert_eq!(
        trace(&platform.feed(Event::TrimMemory(TrimLevel::Heavy))),
        ["MakeCurrent", "ReleaseCaches"]
    );
    assert!(platform.renderer && platform.context);
}

/// A heavy trim with no surface gives the renderer and the context back; the next surface makes them again.
///
/// Mutation: a trim that keeps the context for ever; a next surface that finds no renderer.
#[test]
fn a_heavy_trim_without_a_surface_gives_the_context_back() {
    let mut platform = Platform::ready();
    platform.feed(Event::SurfaceDestroyed);
    platform.release_window();
    let performed = platform.feed(Event::TrimMemory(TrimLevel::Heavy));
    assert_eq!(trace(&performed), ["DestroyRenderer", "DestroyContext"]);
    assert_eq!(platform.machine.phase(), Phase::NoContext);
    platform.new_window();
    assert_eq!(
        trace(&platform.feed(Event::SurfaceCreated(portrait()))),
        BRING_UP
    );
}

// ---------------------------------------------------------------------------------------------
// the end
// ---------------------------------------------------------------------------------------------

/// Closing with everything up tears down in the order the platforms need: the surface first, the renderer on a
/// current context, then the context. Nothing is left, and nothing more is answered.
///
/// Mutation: the context destroyed before the renderer; the surface after the window.
#[test]
fn shutdown_tears_down_in_order() {
    let mut platform = Platform::ready();
    let performed = platform.feed(Event::Shutdown);
    assert_eq!(
        trace(&performed),
        [
            "StopFrames",
            "DestroySurface",
            "DestroyRenderer",
            "DestroyContext"
        ]
    );
    platform.release_window();
    assert_eq!(platform.machine.phase(), Phase::Closed);
    assert!(platform.feed(Event::Frame).is_empty());
    assert!(platform.feed(Event::Shutdown).is_empty());
    assert!(platform.feed(Event::SurfaceCreated(portrait())).is_empty());
}

/// Closing a window that never got a context is nothing, and closing one between two windows (no surface) keeps
/// the order.
#[test]
fn shutdown_of_a_window_without_a_surface() {
    let mut fresh = Platform::new();
    assert!(fresh.feed(Event::Shutdown).is_empty());
    let mut platform = Platform::ready();
    platform.feed(Event::SurfaceDestroyed);
    platform.release_window();
    assert_eq!(
        trace(&platform.feed(Event::Shutdown)),
        ["DestroyRenderer", "DestroyContext"]
    );
}

// ---------------------------------------------------------------------------------------------
// a failure at each step falls back
// ---------------------------------------------------------------------------------------------

/// A failure of each step of the bring-up ends the GPU attempt: what was made comes down in order, the surface
/// before the CPU path takes the window, and nothing is left. The fallback names the stage that failed.
///
/// Mutation: a step whose failure is not reported; a teardown that leaves the context or the surface; the CPU
/// path before the surface is destroyed.
#[test]
fn a_failure_at_each_step_of_the_bring_up_falls_back() {
    let steps: [(&str, GpuStage, &[&str]); 7] = [
        (
            "CreateContext",
            GpuStage::Library,
            &["CreateContext!", "StopFrames", "FallBackToCpu"],
        ),
        (
            "CreateContext",
            GpuStage::Context,
            &["CreateContext!", "StopFrames", "FallBackToCpu"],
        ),
        (
            "CreateSurface",
            GpuStage::Surface,
            // no surface was made, so none is destroyed; the context is
            &[
                "CreateContext",
                "CreateSurface!",
                "StopFrames",
                "DestroyContext",
                "FallBackToCpu",
            ],
        ),
        (
            "MakeCurrent",
            GpuStage::MakeCurrent,
            &[
                "CreateContext",
                "CreateSurface",
                "MakeCurrent!",
                "StopFrames",
                "DestroySurface",
                "DestroyContext",
                "FallBackToCpu",
            ],
        ),
        (
            "BuildRenderer",
            GpuStage::Functions,
            &[
                "CreateContext",
                "CreateSurface",
                "MakeCurrent",
                "BuildRenderer!",
                "StopFrames",
                "DestroySurface",
                "DestroyContext",
                "FallBackToCpu",
            ],
        ),
        (
            "BuildRenderer",
            GpuStage::Shaders,
            &[
                "CreateContext",
                "CreateSurface",
                "MakeCurrent",
                "BuildRenderer!",
                "StopFrames",
                "DestroySurface",
                "DestroyContext",
                "FallBackToCpu",
            ],
        ),
        (
            "BuildRenderer",
            GpuStage::Document,
            &[
                "CreateContext",
                "CreateSurface",
                "MakeCurrent",
                "BuildRenderer!",
                "StopFrames",
                "DestroySurface",
                "DestroyContext",
                "FallBackToCpu",
            ],
        ),
    ];
    for (action, stage, expected) in steps {
        let mut platform = Platform::new().failing(action, stage);
        let performed = platform.feed(Event::SurfaceCreated(portrait()));
        assert_eq!(trace(&performed), expected, "{action} failing at {stage:?}");
        assert_eq!(
            platform.machine.phase(),
            Phase::Fallen,
            "{action} / {stage:?}"
        );
        assert_eq!(platform.machine.fallback().map(|f| f.stage), Some(stage));
        assert!(platform.cpu_has_window, "the CPU renderer took the window");
        assert_eq!(
            platform.leaked, 0,
            "nothing was abandoned: everything came down"
        );
    }
}

/// A failure of the renderer's own build, with every stage that can name it, ends the same way.
#[test]
fn every_stage_of_the_renderer_falls_back() {
    for stage in [
        GpuStage::Functions,
        GpuStage::Probe,
        GpuStage::Renderer,
        GpuStage::Document,
        GpuStage::Shaders,
    ] {
        let mut platform = Platform::new().failing("BuildRenderer", stage);
        platform.feed(Event::SurfaceCreated(portrait()));
        assert_eq!(platform.machine.phase(), Phase::Fallen, "{stage:?}");
        assert!(!platform.context && platform.surface.is_none() && !platform.renderer);
    }
}

/// A resize that fails at the drawable falls back too, with the frames stopped and the surface destroyed
/// before the CPU renderer takes the window.
#[test]
fn a_failed_resize_falls_back() {
    let mut platform = Platform::ready().failing("ResizeSurface", GpuStage::Surface);
    let performed = platform.feed(Event::Resized(landscape()));
    assert_eq!(performed.first().map(String::as_str), Some("MakeCurrent"));
    assert!(
        performed.contains(&"ResizeSurface!".to_string()),
        "{performed:?}"
    );
    assert_eq!(performed.last().map(String::as_str), Some("FallBackToCpu"));
    assert_eq!(platform.machine.phase(), Phase::Fallen);
}

/// A rebuild after a loss that fails at the new context falls back, and the new window then belongs to the CPU.
#[test]
fn a_rebuild_that_fails_falls_back() {
    let mut platform = Platform::ready().failing("CreateContext", GpuStage::Context);
    let performed = platform.feed(Event::ContextLost);
    assert!(
        performed.contains(&"CreateContext!".to_string()),
        "{performed:?}"
    );
    assert_eq!(performed.last().map(String::as_str), Some("FallBackToCpu"));
    assert_eq!(platform.machine.phase(), Phase::Fallen);
    assert!(platform.cpu_has_window && !platform.context);
}

/// A failure injected into one attempt only (`context:1`) lets the rebuild after a loss succeed: the injected
/// failure of a stage is an event of that stage, not of the whole run.
#[test]
fn the_attempt_after_an_injected_failure_can_succeed() {
    // the first frame fails once; the rebuild works; the next frame goes out
    let mut platform = Platform::ready().failing("Present", GpuStage::Frame);
    platform.feed(Event::Frame);
    assert_eq!(platform.machine.phase(), Phase::Ready);
    assert_eq!(trace(&platform.feed(Event::Frame)), ["Present"]);
}

/// The CPU path never precedes the end of the GPU attempt: no event after the fall back makes a surface.
///
/// Mutation: a surface made for a window the CPU holds (the fake panics).
#[test]
fn nothing_is_made_for_a_window_the_cpu_holds() {
    let mut platform = Platform::new().failing("CreateContext", GpuStage::Context);
    platform.feed(Event::SurfaceCreated(portrait()));
    assert!(platform.cpu_has_window);
    for event in [
        Event::SurfaceCreated(landscape()),
        Event::Resized(portrait()),
        Event::Resumed,
        Event::Frame,
        Event::ContextLost,
        Event::TrimMemory(TrimLevel::Heavy),
    ] {
        assert!(platform.feed(event).is_empty());
    }
}

/// A late failure report after the end is ignored, not answered a second time.
#[test]
fn a_late_failure_report_after_the_end_is_ignored() {
    let mut platform = Platform::new().failing("CreateContext", GpuStage::Context);
    platform.feed(Event::SurfaceCreated(portrait()));
    assert!(platform
        .feed(Event::ActionFailed(GpuFallback::injected(
            GpuStage::Surface
        )))
        .is_empty());
}

// ---------------------------------------------------------------------------------------------
// every order of events
// ---------------------------------------------------------------------------------------------

/// A small deterministic generator (xorshift64*), so that a failing sequence can be replayed.
struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 >> 12;
        self.0 ^= self.0 << 25;
        self.0 ^= self.0 >> 27;
        self.0.wrapping_mul(0x2545_f491_4f6c_dd1d)
    }

    fn below(&mut self, n: u64) -> u64 {
        self.next() % n
    }
}

/// Thousands of events in every order, with failures injected at random: whatever happens, the fake platform's
/// rules hold (no GL in the background, nothing on a window the CPU holds, a surface before a renderer, no
/// second context), the machine's belief matches what exists, an ended path stays ended, and the CPU fall back is
/// always the last thing an answer does.
///
/// Mutation: any of the transitions above that breaks a rule of the fake in some order the tests do not name.
#[test]
fn any_order_of_events_keeps_the_rules_of_the_platforms() {
    for seed in 1..=500u64 {
        let mut rng = Rng(seed.wrapping_mul(0x9e37_79b9_7f4a_7c15) | 1);
        let mut platform = Platform::new();
        for _ in 0..400 {
            // sometimes a step fails: the stage fits the action that is about to be tried
            if rng.below(20) == 0 {
                let (action, stage) = match rng.below(6) {
                    0 => ("CreateContext", GpuStage::Context),
                    1 => ("CreateSurface", GpuStage::Surface),
                    2 => ("MakeCurrent", GpuStage::MakeCurrent),
                    3 => ("BuildRenderer", GpuStage::Renderer),
                    4 => ("ResizeSurface", GpuStage::Surface),
                    _ => ("Present", GpuStage::Frame),
                };
                platform.fail.entry(action).or_default().push(stage);
            }
            let size = surface(100 + rng.below(3000) as u32, 100 + rng.below(3000) as u32);
            let event = match rng.below(14) {
                0 | 1 => {
                    if !platform.window {
                        platform.new_window();
                    }
                    Event::SurfaceCreated(if rng.below(8) == 0 {
                        surface(0, 0)
                    } else {
                        size
                    })
                }
                2 => Event::Resized(size),
                3 => Event::Paused,
                4 => Event::Backgrounded,
                5 => Event::Resumed,
                6 => Event::ContextLost,
                7 => Event::TrimMemory(if rng.below(2) == 0 {
                    TrimLevel::Light
                } else {
                    TrimLevel::Heavy
                }),
                8 | 9 | 10 => Event::Frame,
                11 => Event::FramePresented,
                12 => Event::SurfaceDestroyed,
                _ => {
                    if rng.below(12) == 0 {
                        Event::Shutdown
                    } else {
                        Event::Frame
                    }
                }
            };
            let destroyed = matches!(event, Event::SurfaceDestroyed);
            let ended_before = matches!(platform.machine.phase(), Phase::Fallen | Phase::Closed);
            let performed = platform.feed(event.clone());
            if ended_before {
                assert!(
                    performed.is_empty(),
                    "seed {seed}: {event:?} after the end: {performed:?}"
                );
            }
            if destroyed && platform.window && platform.surface.is_none() {
                platform.release_window();
            }
            // what the shell does with a frame that went out
            if performed.last().map(String::as_str) == Some("Present") {
                platform.feed(Event::FramePresented);
            }
        }
    }
}
