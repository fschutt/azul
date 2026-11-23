//! Whether a pass of the GPU path waits for the renderer's threads: the ONE
//! decision behind "send the scene, draw it when it is built" (audit A4-31).
//!
//! A shell used to wait after every send (`RenderApi::flush_scene_builder`)
//! until the scene builder had built the scene and the render backend the
//! frame: the UI thread sat out every scene build (10-30 ms est. for a map
//! view's list) with nothing to do. The wait bought two things: the frame was
//! drawn in the pass that sent its transaction, and no transaction could
//! overtake another. [`FrameSchedule`] keeps both where they matter and drops
//! the wait where it costs most:
//!
//! 1. Nothing sent before is still on its way and the transaction carries no
//!    display list (a zoom step, a pan, a hover, an image update): as before -
//!    send, wait, draw. The frames of a gesture stay in step with their
//!    transactions.
//! 2. It carries a display list (a scene build) and the pass is not forced to
//!    be in step: sent WITHOUT the wait. The window keeps the frame it shows;
//!    the renderer's notifier wakes the shell when the frame is built and
//!    that pass draws it. The scene is "in flight" until the renderer reports
//!    its epoch ([`FrameSchedule::on_published`]).
//! 3. While a scene is in flight every later transaction takes the scene
//!    builder's queue too and is not waited for: it arrives behind the scene,
//!    never before it. That keeps the order of everything azul relies on -
//!    an image's add before its update (they travel with different
//!    transactions), a set of bound values before the newer set, scroll
//!    offsets after the swap that resets them.
//! 4. At most ONE scene is in flight: a second display list while one is in
//!    flight is sent with the wait (the pass sits out both). The frame on
//!    screen is then never more than one list epoch behind, which is what the
//!    GL texture cache keeps textures for.
//! 5. A pass that must be in step waits whatever it sends: a window's first
//!    frame, a pass at a new size, a live resize, and every pass with
//!    `AZ_WR_SYNC_SCENE=1` (`RenderSettings::sync_scene`, read once at the app's start).
//! 6. Anything sent without a wait leaves the queue "not known empty"
//!    ([`FrameSchedule`]'s `queued`) until a pass waits again: the scene may
//!    be published while a transaction sent after it is still queued, and
//!    the next one must not pass that one either. So the first transaction
//!    after the scene landed takes the queue once more (and is waited for,
//!    rule 1); the one after it goes straight again.
//! 7. A valve: a scene in flight for [`FrameSchedule::VALVE`] ends the next
//!    pass's patience - it waits, as before. A wake that got lost costs one
//!    late frame, never a window that stays on its old picture.
//!
//! Pure and platform-free, like the frame pacer next door: no window, no
//! renderer. The shell says what its transaction is ([`FrameShape`]) and
//! what the pass is ([`FramePass`]) and does what [`FrameSend`] says.

use std::time::{Duration, Instant};

use webrender::api::{Epoch, PipelineId};

/// What a pass is about to hand the renderer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FrameShape {
    /// Nothing: the pass only shows a frame the renderer published.
    pub empty: bool,
    /// The transaction carries a display list - the first one's pipeline and
    /// epoch (`Transaction::first_display_list`) - so the scene is built again.
    pub scene: Option<(PipelineId, Epoch)>,
}

impl FrameShape {
    /// The shape of `txn`, or of nothing when the pass does not send it.
    #[must_use]
    pub fn of(txn: &webrender::Transaction, only_shows_a_published_frame: bool) -> Self {
        Self {
            empty: only_shows_a_published_frame,
            scene: if only_shows_a_published_frame {
                None
            } else {
                txn.first_display_list()
            },
        }
    }
}

/// What the shell knows about the pass that asks.
#[derive(Debug, Clone, Copy)]
pub struct FramePass {
    /// The pass must show what it sends (rule 5).
    pub forced_sync: bool,
    /// The renderer published a frame the shell has not drawn yet.
    pub frame_waiting: bool,
    /// The time of the pass.
    pub now: Instant,
}

/// What the shell does with the pass's transaction.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FrameSend {
    /// Send the transaction (`false` only for a pass with nothing to send).
    pub send: bool,
    /// Before the send: `Transaction::use_scene_builder_thread`, whatever the
    /// builder decided ([`Self::route`]).
    pub through_scene_builder: bool,
    /// After the send: wait until both of the renderer's threads are done
    /// with everything sent so far.
    pub flush: bool,
    /// Take in what the renderer published, draw and present. `false`: the
    /// pass ends after the send and the window keeps the frame it shows.
    pub render: bool,
    /// A scene went out without a wait: have a pass come once
    /// [`FrameSchedule::VALVE`] has gone by, in case no wake does
    /// ([`FrameSchedule::valve_due`]).
    pub arm_valve: bool,
}

impl FrameSend {
    /// Put `txn` on the queue this decision names. A transaction with a
    /// display list or a resource update takes the scene builder's queue by
    /// itself; this is for the one the builder let skip it.
    pub fn route(&self, txn: &mut webrender::Transaction) {
        if self.through_scene_builder {
            txn.use_scene_builder_thread();
        }
    }
}

/// A scene sent without a wait and not seen in a frame yet.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct SceneInFlight {
    pipeline: PipelineId,
    epoch: Epoch,
    sent_at: Instant,
}

/// The state behind [`FrameSend`]. See the module docs.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct FrameSchedule {
    /// The scene that was sent without a wait and that no frame the renderer
    /// took in shows yet. At most one (rule 4).
    in_flight: Option<SceneInFlight>,
    /// Something was sent without a wait since the last wait: the scene
    /// builder's queue is not known to be empty (rule 6).
    queued: bool,
}

impl FrameSchedule {
    /// How long a scene may be in flight before a pass waits for it (rule 7).
    pub const VALVE: Duration = Duration::from_secs(2);

    /// Nothing in flight, nothing queued.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            in_flight: None,
            queued: false,
        }
    }

    /// The scene in flight: the pipeline and the epoch the renderer reports
    /// once a frame shows it.
    #[must_use]
    pub fn scene_in_flight(&self) -> Option<(PipelineId, Epoch)> {
        self.in_flight.map(|scene| (scene.pipeline, scene.epoch))
    }

    /// Whether anything sent without a wait may still be on its way.
    #[must_use]
    pub const fn unsettled(&self) -> bool {
        self.in_flight.is_some() || self.queued
    }

    /// Whether the scene in flight has been so for [`Self::VALVE`]: the shell
    /// then makes a pass by itself, and that pass waits (rule 7). Never true
    /// with nothing in flight: an idle window asks for nothing.
    #[must_use]
    pub fn valve_due(&self, now: Instant) -> bool {
        self.in_flight
            .is_some_and(|scene| now.saturating_duration_since(scene.sent_at) >= Self::VALVE)
    }

    /// The shell waited for the renderer's threads OUTSIDE a pass (it sent a
    /// transaction of its own and flushed: a hidden window giving its halo
    /// pictures back, audit A5-20): everything sent so far has been taken, so
    /// nothing is in flight and the queue is empty. Such a transaction must
    /// take the scene builder's queue (one with a display list or a delete
    /// does by itself; never `skip_scene_builder` there), or it could pass
    /// what a pass sent without the wait.
    ///
    /// Not calling this is safe - the schedule then goes on believing in a
    /// scene in flight, routes through the queue and waits a little more than
    /// it has to, until a pass waits - but it is not what is true.
    pub fn waited_outside_a_pass(&mut self) {
        *self = Self::new();
    }

    /// The pass is about to send a transaction of `shape` (or nothing): what
    /// it does, and what is in flight afterwards.
    pub fn on_send(&mut self, shape: FrameShape, pass: FramePass) -> FrameSend {
        let unsettled = self.unsettled();
        let valve = self.valve_due(pass.now);

        if shape.empty {
            // Nothing to send. Only a pass that must be in step, or whose
            // patience ended, waits for what is still on its way.
            let flush = unsettled && (pass.forced_sync || valve);
            if flush {
                *self = Self::new();
            }
            return FrameSend {
                send: false,
                through_scene_builder: false,
                flush,
                render: flush || pass.frame_waiting,
                arm_valve: false,
            };
        }

        let flush = match (self.in_flight.is_some(), shape.scene.is_some()) {
            // Rule 1: a transaction without a display list, no scene in
            // flight. (With something still queued it takes the queue, rule 6.)
            (false, false) => true,
            // Rule 2 - unless the pass must be in step.
            (false, true) => pass.forced_sync,
            // Rule 3 - unless the pass must be in step or the valve opened.
            (true, false) => pass.forced_sync || valve,
            // Rule 4: never two scenes in flight.
            (true, true) => true,
        };
        // Whatever is sent while something is on its way goes behind it; a
        // display list goes through the scene builder in any case.
        let through_scene_builder = unsettled || shape.scene.is_some();

        let mut arm_valve = false;
        if flush {
            // The wait covers everything sent before it.
            *self = Self::new();
        } else {
            self.queued = true;
            if let Some((pipeline, epoch)) = shape.scene {
                self.in_flight = Some(SceneInFlight {
                    pipeline,
                    epoch,
                    sent_at: pass.now,
                });
                arm_valve = true;
            }
        }

        FrameSend {
            send: true,
            through_scene_builder,
            flush,
            render: flush || pass.frame_waiting,
            arm_valve,
        }
    }

    /// The renderer took in the frames published so far
    /// (`Renderer::update`); `epoch_of` is its `current_epoch` of a pipeline.
    /// Returns `true` when the scene in flight is in the renderer's frame now:
    /// it is in flight no more, and the shell forgets what it believes the
    /// renderer holds of values and scroll offsets
    /// (`GpuStateManager::invalidate_submitted_digests`), so that the next
    /// pass sends the current ones whatever travelled with or behind the
    /// scene - the second line behind rule 3.
    ///
    /// The queue stays "not known empty" (rule 6): a transaction sent behind
    /// the scene may still be in it.
    pub fn on_published(&mut self, epoch_of: impl Fn(PipelineId) -> Option<Epoch>) -> bool {
        let Some(scene) = self.in_flight else {
            return false;
        };
        let landed = epoch_of(scene.pipeline).is_some_and(|epoch| epoch.0 >= scene.epoch.0);
        if landed {
            self.in_flight = None;
        }
        landed
    }
}

/// The renderer's two queues, as far as order goes, for the tests of the
/// schedule: a transaction either waits in the scene builder's queue or is
/// applied by the render backend at once. The scene builder is as slow as it
/// can be - nothing leaves its queue before a test says so or a pass waits -
/// and the direct way as fast: the worst case for one transaction overtaking
/// another.
#[cfg(test)]
pub(crate) struct Wires<T> {
    /// In the scene builder's queue, in the order sent: number, whether it
    /// carries a display list, payload.
    queue: Vec<(usize, bool, T)>,
    /// What the render backend applied, in its order.
    pub(crate) applied: Vec<(usize, T)>,
    sent: usize,
}

#[cfg(test)]
impl<T> Wires<T> {
    pub(crate) const fn new() -> Self {
        Self {
            queue: Vec::new(),
            applied: Vec::new(),
            sent: 0,
        }
    }

    /// A shell sends a transaction; returns its number.
    pub(crate) fn send(&mut self, takes_scene_builder_queue: bool, scene: bool, payload: T) -> usize {
        let n = self.sent;
        self.sent += 1;
        if takes_scene_builder_queue {
            self.queue.push((n, scene, payload));
        } else {
            assert!(!scene, "a display list always takes the scene builder's queue");
            self.applied.push((n, payload));
        }
        n
    }

    /// The scene builder gets through its queue (what a flush waits for).
    pub(crate) fn drain(&mut self) {
        for (n, _, payload) in self.queue.drain(..) {
            self.applied.push((n, payload));
        }
    }

    /// The scene builder gets through its queue up to and including the first
    /// display list in it; what was sent behind that one stays queued.
    pub(crate) fn drain_through_the_scene(&mut self) {
        let Some(at) = self.queue.iter().position(|(_, scene, _)| *scene) else {
            return;
        };
        for (n, _, payload) in self.queue.drain(..=at) {
            self.applied.push((n, payload));
        }
    }

    /// The display lists waiting in the scene builder's queue.
    pub(crate) fn scenes_queued(&self) -> usize {
        self.queue.iter().filter(|(_, scene, _)| *scene).count()
    }

    /// Whether anything waits in the scene builder's queue.
    pub(crate) fn anything_queued(&self) -> bool {
        !self.queue.is_empty()
    }

    /// Whether the render backend applied the transactions in the order they
    /// were sent.
    pub(crate) fn in_order(&self) -> bool {
        self.applied.windows(2).all(|pair| pair[0].0 < pair[1].0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const VIEW: PipelineId = PipelineId(1, 7);

    fn scene(epoch: u32) -> FrameShape {
        FrameShape {
            empty: false,
            scene: Some((VIEW, Epoch(epoch))),
        }
    }

    /// A transaction without a display list: values, offsets, image updates.
    const VALUES: FrameShape = FrameShape {
        empty: false,
        scene: None,
    };

    /// Nothing to send.
    const NOTHING: FrameShape = FrameShape {
        empty: true,
        scene: None,
    };

    fn pass(now: Instant) -> FramePass {
        FramePass {
            forced_sync: false,
            frame_waiting: false,
            now,
        }
    }

    fn forced(now: Instant) -> FramePass {
        FramePass {
            forced_sync: true,
            ..pass(now)
        }
    }

    fn woken(now: Instant) -> FramePass {
        FramePass {
            frame_waiting: true,
            ..pass(now)
        }
    }

    /// (send, through the scene builder, flush, render, arm the valve)
    fn said(send: FrameSend) -> (bool, bool, bool, bool, bool) {
        (
            send.send,
            send.through_scene_builder,
            send.flush,
            send.render,
            send.arm_valve,
        )
    }

    /// Rule 1. The frames of a gesture are what they were: sent the way the
    /// builder routed them, waited for, drawn in their pass.
    /// Mutation: `(false, false) => false` in `on_send` fails the first line.
    #[test]
    fn a_transaction_without_a_list_is_sent_waited_for_and_drawn_as_before() {
        let t = Instant::now();
        let mut schedule = FrameSchedule::new();
        for _ in 0..3 {
            assert_eq!(
                said(schedule.on_send(VALUES, pass(t))),
                (true, false, true, true, false)
            );
            assert_eq!(schedule, FrameSchedule::new(), "nothing is in flight after a wait");
        }
    }

    /// Rule 2. A display list goes out without the wait and without a draw;
    /// the scene is in flight and the valve is armed.
    /// Mutation: `(false, true) => true` waits (the old path) and fails it.
    #[test]
    fn a_display_list_is_sent_without_the_wait_and_drawn_when_it_is_published() {
        let t = Instant::now();
        let mut schedule = FrameSchedule::new();
        assert_eq!(
            said(schedule.on_send(scene(5), pass(t))),
            (true, true, false, false, true)
        );
        assert_eq!(schedule.scene_in_flight(), Some((VIEW, Epoch(5))));

        // The woken pass has nothing to send: it draws, and does not wait.
        assert_eq!(
            said(schedule.on_send(NOTHING, woken(t))),
            (false, false, false, true, false)
        );
        // The renderer still reports the list before: in flight.
        assert!(!schedule.on_published(|_| Some(Epoch(4))));
        assert!(!schedule.on_published(|_| None));
        assert_eq!(schedule.scene_in_flight(), Some((VIEW, Epoch(5))));
        // Now it reports the scene's epoch (or a later one).
        assert!(schedule.on_published(|pipeline| (pipeline == VIEW).then_some(Epoch(5))));
        assert_eq!(schedule.scene_in_flight(), None);
        assert!(!schedule.on_published(|_| Some(Epoch(9))), "reported once");
    }

    /// Rule 3. While a scene is in flight a transaction without a list takes
    /// the scene builder's queue, is not waited for, and its pass draws only
    /// when a published frame waits.
    /// Mutation: `through_scene_builder = shape.scene.is_some()` sends it
    /// straight to the render backend, ahead of the scene.
    #[test]
    fn while_a_scene_is_in_flight_everything_queues_behind_it() {
        let t = Instant::now();
        let mut schedule = FrameSchedule::new();
        let _ = schedule.on_send(scene(5), pass(t));
        assert_eq!(
            said(schedule.on_send(VALUES, pass(t))),
            (true, true, false, false, false)
        );
        assert_eq!(
            said(schedule.on_send(VALUES, woken(t))),
            (true, true, false, true, false)
        );
        assert_eq!(schedule.scene_in_flight(), Some((VIEW, Epoch(5))), "still the one scene");
    }

    /// Rule 4. A second display list while one is in flight is waited for:
    /// never two scenes in flight.
    /// Mutation: `(true, true) => false` leaves two.
    #[test]
    fn a_second_display_list_waits_for_both() {
        let t = Instant::now();
        let mut schedule = FrameSchedule::new();
        let _ = schedule.on_send(scene(5), pass(t));
        assert_eq!(
            said(schedule.on_send(scene(6), pass(t))),
            (true, true, true, true, false)
        );
        assert_eq!(schedule, FrameSchedule::new());
    }

    /// Rule 5. A pass that must be in step waits whatever it sends, and
    /// whatever is in flight.
    /// Mutation: dropping `pass.forced_sync` from the `(false, true)` arm
    /// sends a resize's list without the wait.
    #[test]
    fn a_forced_pass_waits_whatever_it_sends() {
        let t = Instant::now();
        let mut schedule = FrameSchedule::new();
        assert_eq!(
            said(schedule.on_send(scene(5), forced(t))),
            (true, true, true, true, false)
        );
        assert_eq!(schedule, FrameSchedule::new(), "a forced scene is never in flight");

        let _ = schedule.on_send(scene(6), pass(t));
        assert_eq!(
            said(schedule.on_send(VALUES, forced(t))),
            (true, true, true, true, false)
        );
        assert_eq!(schedule, FrameSchedule::new());

        let _ = schedule.on_send(scene(7), pass(t));
        assert_eq!(
            said(schedule.on_send(NOTHING, forced(t))),
            (false, false, true, true, false),
            "nothing to send, and still in step with what is on its way"
        );
        assert_eq!(schedule, FrameSchedule::new());
    }

    /// Rule 6. The scene landed, but something sent behind it may still be
    /// queued: the next transaction takes the queue once more and is waited
    /// for; the one after it goes the way the builder routed it.
    /// Mutation: `self.queued = false` in `on_published` sends the next one
    /// straight, ahead of what is queued.
    #[test]
    fn after_the_scene_landed_the_next_transaction_still_takes_the_queue() {
        let t = Instant::now();
        let mut schedule = FrameSchedule::new();
        let _ = schedule.on_send(scene(5), pass(t));
        let _ = schedule.on_send(VALUES, pass(t));
        assert!(schedule.on_published(|_| Some(Epoch(5))));
        assert!(schedule.unsettled(), "the queue is not known empty");
        assert_eq!(
            said(schedule.on_send(VALUES, pass(t))),
            (true, true, true, true, false)
        );
        assert_eq!(
            said(schedule.on_send(VALUES, pass(t))),
            (true, false, true, true, false)
        );
    }

    /// Rule 7. A scene in flight for the valve's time: the next pass waits,
    /// whatever it sends; before that time it does not. And an idle window
    /// asks for nothing: the valve is never due with nothing in flight, and
    /// one pass closes it.
    /// Mutation: `>= Self::VALVE` -> `> Self::VALVE` in `valve_due` fails
    /// `valve_due(after)`; `|| valve` dropped from the `(true, false)` arm of
    /// `on_send` fails the pass with a transaction.
    #[test]
    fn a_scene_in_flight_too_long_is_waited_for_and_an_idle_window_asks_nothing() {
        let t = Instant::now();
        let just_before = t + FrameSchedule::VALVE - Duration::from_millis(1);
        let after = t + FrameSchedule::VALVE;

        let idle = FrameSchedule::new();
        assert!(!idle.valve_due(t + Duration::from_secs(3600)), "nothing in flight, nothing due");

        let mut schedule = FrameSchedule::new();
        assert!(schedule.on_send(scene(5), pass(t)).arm_valve);
        assert!(!schedule.valve_due(just_before));
        assert!(schedule.valve_due(after));
        // A pass before the time does not wait ...
        assert_eq!(
            said(schedule.on_send(NOTHING, woken(just_before))),
            (false, false, false, true, false)
        );
        // ... the valve's pass does, with nothing to send,
        assert_eq!(
            said(schedule.on_send(NOTHING, woken(after))),
            (false, false, true, true, false)
        );
        assert!(!schedule.valve_due(after + Duration::from_secs(3600)), "one pass, then nothing");

        // ... and with a transaction.
        let mut schedule = FrameSchedule::new();
        let _ = schedule.on_send(scene(5), pass(t));
        assert_eq!(
            said(schedule.on_send(VALUES, pass(after))),
            (true, true, true, true, false)
        );
        assert_eq!(schedule, FrameSchedule::new());

        // A scene that was waited for arms nothing; neither does a
        // transaction without a list.
        let mut schedule = FrameSchedule::new();
        assert!(!schedule.on_send(scene(5), forced(t)).arm_valve);
        assert!(!schedule.on_send(VALUES, pass(t)).arm_valve);
        assert!(!schedule.valve_due(t + Duration::from_secs(3600)));
    }

    /// A wait outside a pass (a hidden window's release, A5-20) while a scene
    /// is in flight: told of it, the schedule is settled and the next
    /// transaction goes as its builder routed it; NOT told, it errs to the
    /// safe side only - it still believes in the scene, so the next
    /// transaction takes the queue and a second list waits - and the first
    /// pass that waits (the return pass is forced) puts it right.
    /// Mutation: `waited_outside_a_pass` leaving `queued` set fails the first
    /// block; `forced_sync` no longer resetting the state fails the second.
    #[test]
    fn a_wait_outside_a_pass_settles_the_schedule_and_without_the_word_it_errs_safely() {
        let t = Instant::now();
        let mut told = FrameSchedule::new();
        let _ = told.on_send(scene(5), pass(t));
        let _ = told.on_send(VALUES, pass(t));
        told.waited_outside_a_pass();
        assert_eq!(told, FrameSchedule::new());
        assert!(!told.valve_due(t + FrameSchedule::VALVE), "nothing in flight: no valve");
        assert_eq!(
            said(told.on_send(VALUES, pass(t))),
            (true, false, true, true, false)
        );

        let mut not_told = FrameSchedule::new();
        let _ = not_told.on_send(scene(5), pass(t));
        // The release sent its own list at the next epoch and waited; the
        // schedule was not told.
        assert_eq!(
            said(not_told.on_send(VALUES, pass(t))),
            (true, true, false, false, false),
            "over-cautious: behind a scene that has landed"
        );
        // A frame the renderer took in reports the release's epoch, past the
        // scene's: landed.
        assert!(not_told.on_published(|_| Some(Epoch(6))));
        // The pass after the window is shown again is forced: it waits, and
        // the schedule is what is true.
        assert_eq!(
            said(not_told.on_send(scene(7), forced(t))),
            (true, true, true, true, false)
        );
        assert_eq!(not_told, FrameSchedule::new());
    }

    /// The state a schedule is in, for the table below.
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    enum Was {
        Settled,
        /// The scene landed, something may still be queued.
        Queued,
        /// A scene in flight, sent just now.
        Flying,
        /// A scene in flight for the valve's time.
        Stale,
    }

    fn schedule_that(was: Was, now: Instant) -> FrameSchedule {
        let mut schedule = FrameSchedule::new();
        match was {
            Was::Settled => {}
            Was::Queued => {
                let _ = schedule.on_send(scene(5), pass(now));
                assert!(schedule.on_published(|_| Some(Epoch(5))));
            }
            Was::Flying => {
                let _ = schedule.on_send(scene(5), pass(now));
            }
            Was::Stale => {
                let _ = schedule.on_send(scene(5), pass(now - FrameSchedule::VALVE));
            }
        }
        schedule
    }

    /// EVERY decision: 4 states x 3 shapes x forced x a frame waiting = 48
    /// cases, each against the rule written out here a second time, as a
    /// table and not as the code under test.
    /// Mutation: any changed arm of `on_send` differs from a row.
    #[test]
    fn every_decision_is_the_one_the_rules_name() {
        // Some time after the process started, so that `now - VALVE` exists.
        let now = Instant::now() + FrameSchedule::VALVE + FrameSchedule::VALVE;
        for was in [Was::Settled, Was::Queued, Was::Flying, Was::Stale] {
            for (shape, what) in [(NOTHING, "nothing"), (VALUES, "values"), (scene(6), "scene")] {
                for forced_sync in [false, true] {
                    for frame_waiting in [false, true] {
                        let mut schedule = schedule_that(was, now);
                        let got = schedule.on_send(
                            shape,
                            FramePass {
                                forced_sync,
                                frame_waiting,
                                now,
                            },
                        );
                        let case = format!(
                            "{was:?}, {what}, forced {forced_sync}, waiting {frame_waiting}"
                        );

                        // Does the pass wait?
                        let waits = match (what, was) {
                            ("nothing", Was::Settled) => false,
                            ("nothing", Was::Queued | Was::Flying) => forced_sync,
                            ("nothing", Was::Stale) => true,
                            ("values", Was::Settled | Was::Queued | Was::Stale) => true,
                            ("values", Was::Flying) => forced_sync,
                            ("scene", Was::Settled | Was::Queued) => forced_sync,
                            ("scene", Was::Flying | Was::Stale) => true,
                            _ => unreachable!(),
                        };
                        assert_eq!(got.flush, waits, "{case}: flush");
                        assert_eq!(got.send, what != "nothing", "{case}: send");
                        assert_eq!(got.render, waits || frame_waiting, "{case}: render");
                        // Behind what is on its way, and a list always.
                        let queue = match what {
                            "nothing" => false,
                            "values" => was != Was::Settled,
                            _ => true,
                        };
                        assert_eq!(got.through_scene_builder, queue, "{case}: queue");
                        let takes_off = what == "scene" && !waits;
                        assert_eq!(got.arm_valve, takes_off, "{case}: valve");

                        // What is in flight afterwards.
                        let after = if waits {
                            None
                        } else if takes_off {
                            Some((VIEW, Epoch(6)))
                        } else {
                            schedule_that(was, now).scene_in_flight()
                        };
                        assert_eq!(schedule.scene_in_flight(), after, "{case}: in flight");
                        let unsettled = !waits && (what != "nothing" || was != Was::Settled);
                        assert_eq!(schedule.unsettled(), unsettled, "{case}: unsettled");
                    }
                }
            }
        }
    }

    /// One step of a window's life, for the walk below.
    #[derive(Debug, Clone, Copy)]
    enum Step {
        /// A pass with a transaction without a list, as its builder routed
        /// it: straight to the render backend.
        Values,
        /// A pass with a display list.
        Scene,
        /// A pass with a display list that must be in step (a resize).
        ForcedScene,
        /// The scene builder got as far as the scene in its queue, the
        /// renderer's wake brought a pass with nothing to send, and the
        /// renderer took the frame in.
        SceneLands,
        /// The valve's time goes by.
        TwoSeconds,
    }

    /// The order is kept BY CONSTRUCTION: every sequence of six steps (5^6 =
    /// 15,625), with the scene builder as slow as it can be and the direct
    /// way as fast. After every step: no transaction went straight to the
    /// render backend while another waited in the scene builder's queue (it
    /// would be applied first: an image updated before it is added, a set of
    /// values replaced by an older one), the render backend applied
    /// everything in the order sent, at most one display list is in flight,
    /// and a pass that waited left nothing queued.
    /// Mutations: `through_scene_builder = shape.scene.is_some()` (rule 3),
    /// `self.queued = false` in `on_published` (rule 6), `(true, true) =>
    /// false` (rule 4) each fail a sequence.
    #[test]
    fn nothing_overtakes_and_one_scene_flies_in_every_sequence_of_six_steps() {
        const STEPS: [Step; 5] = [
            Step::Values,
            Step::Scene,
            Step::ForcedScene,
            Step::SceneLands,
            Step::TwoSeconds,
        ];
        let start = Instant::now();
        let mut walked = 0u32;
        for code in 0..5usize.pow(6) {
            let mut schedule = FrameSchedule::new();
            let mut wires = Wires::<()>::new();
            let mut now = start;
            let mut epoch = 1u32;
            let mut landed = 0u32;
            let mut digits = code;
            let mut sequence = Vec::new();
            for _ in 0..6 {
                let step = STEPS[digits % 5];
                digits /= 5;
                sequence.push(step);
                match step {
                    Step::Values | Step::Scene | Step::ForcedScene => {
                        let shape = if matches!(step, Step::Values) {
                            VALUES
                        } else {
                            epoch += 1;
                            scene(epoch)
                        };
                        let said = schedule.on_send(
                            shape,
                            FramePass {
                                forced_sync: matches!(step, Step::ForcedScene),
                                frame_waiting: false,
                                now,
                            },
                        );
                        let takes_queue = said.through_scene_builder || shape.scene.is_some();
                        assert!(
                            takes_queue || !wires.anything_queued(),
                            "{sequence:?}: sent straight to the render backend past the queue"
                        );
                        let _ = wires.send(takes_queue, shape.scene.is_some(), ());
                        if said.flush {
                            wires.drain();
                            landed = epoch;
                            assert!(!schedule.unsettled(), "{sequence:?}: a wait settles all");
                        }
                    }
                    Step::SceneLands => {
                        if let Some((_, flying)) = schedule.scene_in_flight() {
                            wires.drain_through_the_scene();
                            landed = flying.0;
                        }
                        let said = schedule.on_send(
                            NOTHING,
                            FramePass {
                                forced_sync: false,
                                frame_waiting: true,
                                now,
                            },
                        );
                        if said.flush {
                            wires.drain();
                        }
                        assert!(said.render, "{sequence:?}: a woken pass draws");
                        let _ = schedule.on_published(|_| Some(Epoch(landed)));
                        assert_eq!(schedule.scene_in_flight(), None, "{sequence:?}");
                    }
                    Step::TwoSeconds => now += FrameSchedule::VALVE,
                }
                assert!(wires.in_order(), "{sequence:?}: applied out of order");
                assert!(wires.scenes_queued() <= 1, "{sequence:?}: two scenes in flight");
                assert_eq!(
                    wires.scenes_queued() == 1,
                    schedule.scene_in_flight().is_some(),
                    "{sequence:?}: the schedule knows what is in flight"
                );
                assert!(
                    schedule.unsettled() || !wires.anything_queued(),
                    "{sequence:?}: something is queued and the schedule calls it settled"
                );
            }
            walked += 1;
        }
        assert_eq!(walked, 15_625);
    }
}
