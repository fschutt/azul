//! The app's lifecycle: sent to the background and back, the handler the app installed hears `Paused` and then `Resumed`.
//!
//! The handler (`CallbackInfo::set_lifecycle_handler`) is installed in the create callback and keeps what it heard in the row that
//! runs (`SelfTest::note_lifecycle`: one code per event in the row's scratch, 1 Paused, 2 Resumed, 3 LowMemory, 4 WillTerminate). The
//! row passes when a `Paused` is followed by a `Resumed`. A person presses Home and opens the app again; a harness plays that part: it
//! answers the line `AZ_SELFTEST_AWAIT lifecycle.background` with HOME and `am start` on Android and with the Settings app on the
//! iOS Simulator.

use super::{await_feed, Ctx, ProbeDef, Step};
use crate::report::Verdict;

/// The codes the handler keeps in the row's scratch.
pub const PAUSED: u64 = 1;
pub const RESUMED: u64 = 2;

/// How long the row waits for the app to be sent away and back.
const WAIT_MS: u64 = 90_000;

pub static PROBES: &[ProbeDef] = &[ProbeDef {
    id: "lifecycle.background",
    group: "lifecycle",
    title: "Background and back: the lifecycle handler hears Paused, then Resumed",
    human: Some("press Home (or switch to another app) and open this app again"),
    note: "",
    widget: None,
    step: background,
}];

fn background(ctx: &mut Ctx<'_>) -> Step {
    if ctx.phase == 0 {
        ctx.phase = 1;
        await_feed("lifecycle.background");
    }
    let heard: &[u64] = ctx.scratch;
    let paused = heard.iter().position(|code| *code == PAUSED);
    let resumed = paused.and_then(|at| heard[at + 1..].iter().position(|code| *code == RESUMED));
    if resumed.is_some() {
        return Step::Done(Verdict::Pass(format!(
            "the handler heard Paused and then Resumed ({} events in {} ms)",
            heard.len(),
            ctx.elapsed_ms
        )));
    }
    if ctx.elapsed_ms > WAIT_MS {
        return Step::Done(Verdict::NeedsHuman(match paused {
            Some(_) => "the app was sent away and the handler heard Paused, but Resumed never came: open the app again".into(),
            None => "nobody sent the app to the background: press Home, then open it again".into(),
        }));
    }
    Step::Wait
}
