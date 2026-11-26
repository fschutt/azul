//! How AzReview plugs the companion host in: the runnable version of `PLUGIN.md`.
//!
//! `Review` is a toy stand-in for AzReview (a view of the pages, a settings object, an agent that answers button presses); the
//! `wire_*` functions and `Review::handle` are the part AzReview writes for itself, in the order of the document. The network is the
//! mock one so that this runs anywhere; in AzReview `MockLink` is `IrohLink` (feature `azul`) and nothing else changes.
//!
//! Run it: `cargo run -p azul-review-link --example host_wiring`. It pairs a phone, drags the page, presses a button and lets the
//! agent finish, and asserts what AzReview saw at each step (so the example fails when the API stops matching the document).

use azul_review_link::access::{ScreenId, ScreenInfo, TrustRecord};
use azul_review_link::client::{ClientConfig, PhoneClient};
use azul_review_link::frame::{
    AgentFinished, ButtonSet, Context, FinishKind, Focus, HostSettings, PageSurface, PenMark, PenState, Phase, Rect, ReplyStatus, Role, Rows, Surface,
    ViewportOp,
};
use azul_review_link::host::{CompanionHost, HostConfig, HostEvent};
use azul_review_link::link::{Link, Millis};
use azul_review_link::mock::{Knobs, MockLink, MockNet};
use azul_review_link::pairing::{format_code, SeededEntropy};
use azul_review_link::phone::PhoneApi;
use azul_review_link::store::MemoryStore;
use azul_review_link::viewport::{apply_gesture_in, fit_width_in, reset_to_page_top, resolve_focus, snap_to_page_top, step_page, Space};

/// What AzReview keeps and shows of the companion (a toy: a real one has settings pages, a dialog, a conversation list).
struct Review {
    // the zoom model: the surface on screen, the view of it, the view a gesture began on
    page: PageSurface,
    view: Rect,
    pen: Option<(f32, f32)>,
    gesture_base: Option<(u32, Rect)>,
    settings: HostSettings,
    // the pairing dialog: who asks, and the code both screens show
    dialog: Option<(String, String, u32)>,
    // what is persisted in AzReview's settings file
    paired: Vec<TrustRecord>,
    buttons: ButtonSet,
    // what the user would see
    log: Vec<String>,
    opened: Vec<(String, Option<u32>)>,
    instructions: Vec<(String, String, String)>,
}

impl Review {
    fn new() -> Review {
        let page = PageSurface { file: "src/fc.rs".into(), page_index: 0, page_count: 9, line: 1337, line_count: 3000, ..PageSurface::empty() };
        Review {
            view: Rect { x: 0.0, y: 0.0, w: 1.0, h: 0.6 },
            page,
            pen: Some((0.5, 0.3)),
            gesture_base: None,
            settings: HostSettings::default(),
            dialog: None,
            paired: Vec::new(),
            buttons: ButtonSet::defaults(),
            log: Vec::new(),
            opened: Vec::new(),
            instructions: Vec::new(),
        }
    }

    /// WHAT THE PEN LOOKS AT, said whenever it changes (the host sends it at most 30 times a second, only when it changed).
    fn publish_pen<L: Link>(&mut self, host: &mut CompanionHost<L>) {
        host.publish_pen(PenState {
            // the host numbers the pen states itself, per phone
            seq: 0,
            screen: ScreenId::MAIN,
            folder: "azul".into(),
            surface: Surface::Page(self.page.clone()),
            view: self.view,
            zoom: 1.0 / self.view.w,
            pen: self.pen.map(|(x, y)| PenMark { x, y, down: false }),
            thumb_id: 0,
            context: Context::Line,
            return_point: None,
        });
    }

    /// WHAT AZREVIEW DOES FOR EACH EVENT the host reports.
    fn handle<L: Link>(&mut self, now: Millis, event: HostEvent, host: &mut CompanionHost<L>) {
        let space = Space::of_pages(&self.page);
        match event {
            // the endpoint knows its address: a QR code made before this has a stale one
            HostEvent::Ready { .. } => {}
            // a phone proved the one-time secret: show the code, ask the person (the dialog calls accept_pairing / reject_pairing)
            HostEvent::PairingRequested { peer, name, code } => self.dialog = Some((peer, name, code)),
            // the person said yes: keep the trust records in the settings file
            HostEvent::Paired { .. } => self.paired = host.paired().to_vec(),
            // a paired phone is here: the list of phones in the settings page; it is told the pen state at once by the host
            HostEvent::PeerConnected { name, .. } => self.log.push(format!("{name} is here")),
            HostEvent::PeerGone { reason, .. } => self.log.push(format!("a phone is gone: {reason}")),
            // THE ZOOM MODEL: a gesture's numbers are totals since its Begin, so the view is a function of the view it began on
            HostEvent::Viewport { op, .. } => match op {
                ViewportOp::Gesture { id, phase, pan_x, pan_y, zoom, focus } => {
                    if phase == Phase::Begin || self.gesture_base.is_none_or(|(known, _)| known != id) {
                        self.gesture_base = Some((id, self.view));
                    }
                    if let Some((_, base)) = self.gesture_base {
                        let about = resolve_focus(focus, base, self.pen);
                        self.view = apply_gesture_in(base, (pan_x, pan_y), zoom, about, space);
                        if phase == Phase::End {
                            if self.settings.snap_to_page_top {
                                self.view = snap_to_page_top(self.view, &self.page);
                            }
                            self.gesture_base = None;
                        }
                    }
                }
                ViewportOp::Fit => self.view = fit_width_in(self.view, space),
                ViewportOp::PageStep(steps) => self.view = step_page(self.view, &self.page, i32::from(steps)),
                ViewportOp::Reset => self.view = reset_to_page_top(self.view, &self.page),
            },
            // the Files screen: AzReview lists its own tree (at most MAX_ROWS rows to a frame; `more` for the rest)
            HostEvent::ListDir { peer, request_id, path, offset } => {
                host.publish_rows(now, &peer, Rows { request_id, dir: path, offset, entries: Vec::new(), more: false });
            }
            // a file to open (at a line, or its first unreviewed one); the answer is a status for the request
            HostEvent::OpenFile { peer, request_id, path, line, .. } => {
                self.opened.push((path, line));
                host.reply(now, &peer, request_id, ReplyStatus::Done);
            }
            // a quick button: the instruction goes to the agent with the context at the pen; its conversation answers with statuses
            HostEvent::ButtonPressed { peer, label, text, request_id, .. } => {
                self.instructions.push((peer.clone(), label, text));
                host.reply(now, &peer, request_id, ReplyStatus::Working);
            }
            // the phone edited the buttons (already cut to their caps by the host): keep them in the settings
            HostEvent::ButtonsChanged { buttons } => self.buttons = buttons,
            // "Continue reviewing": go back to the saved place (and forget it)
            HostEvent::ContinueReviewing { .. } => self.log.push("back to the return point".into()),
            // a cap that was hit, a refused connection, a frame that was not understood: for a log, never a crash
            HostEvent::Diagnostic(text) => self.log.push(text),
        }
    }
}

/// The host as AzReview builds it: a link with the Mac's own key, the trust records read from the settings, the screens it has.
fn wire_host<L: Link>(link: L, review: &Review) -> CompanionHost<L> {
    let config = HostConfig { mac_name: "Studio Mac".into(), app_version: "0.1.0".into(), max_phones: 4 };
    let mut host = CompanionHost::new(link, config, Box::new(SeededEntropy(7)));
    host.load_paired(review.paired.clone());
    host.set_settings(0, review.settings);
    let _cut = host.set_buttons(0, &review.buttons);
    let me = host.node_id().to_string();
    // one main screen, owned and driven by this node (more screens: one ScreenInfo each, with the controller they are handed to)
    host.set_screens(vec![ScreenInfo { id: ScreenId::MAIN, owner: me.clone(), controller: me, role: Role::Review }]);
    host
}

/// The Mac and the phone, with a clock.
struct Desk {
    now: Millis,
    review: Review,
    host: CompanionHost<MockLink>,
    phone: PhoneClient<MockLink>,
}

impl Desk {
    /// THE POLL LOOP: AzReview calls this from a timer, at least 30 times a second; the host does everything else (the phone's
    /// own client is polled the same way, by the phone's app).
    fn step(&mut self, ms: u64) {
        for _ in 0..ms {
            self.now += 1;
            for event in self.host.poll(self.now) {
                self.review.handle(self.now, event, &mut self.host);
            }
            self.phone.poll(self.now);
        }
    }
}

fn main() {
    let net = MockNet::with_knobs(Knobs { latency_ms: 2, ..Knobs::default() });
    let review = Review::new();
    let host = wire_host(net.endpoint("mac"), &review);
    let phone = PhoneClient::new(net.endpoint("phone"), ClientConfig { phone_name: "Test phone".into(), app_version: "0.1.0".into() }, Box::new(MemoryStore::new()));
    let mut desk = Desk { now: 1_000, review, host, phone };

    // 1. PAIRING: "Pair a phone" opens a window and shows the offer (a QR code, and the text to paste)
    desk.step(5);
    let offer = desk.host.begin_pairing(desk.now).expect("the host knows its address after the first polls");
    desk.phone.begin_pairing(desk.now, &offer.to_text()).expect("the phone takes the offer");
    desk.step(60);
    let (peer, name, code) = desk.review.dialog.take().expect("the phone asks to be paired: AzReview shows the code and asks");
    println!("pairing dialog: {name} asks to pair, code {}", format_code(code));
    desk.host.accept_pairing(desk.now, &peer).expect("the person says yes");
    desk.step(60);
    assert_eq!(desk.review.paired.len(), 1, "the trust record is kept in AzReview's settings");
    assert!(desk.phone.is_active());

    // 2. THE PEN: the phone shows what AzReview publishes
    desk.review.publish_pen(&mut desk.host);
    desk.step(80);
    assert_eq!(desk.phone.view().pen.as_ref().map(PenState::file), Some("src/fc.rs"));

    // 3. A DRAG: the phone sends totals, AzReview applies them to the view the gesture began on
    let before = desk.review.view;
    desk.phone.gesture(desk.now, 1, Phase::Begin, (0.0, 0.0), 1.0, Focus::Pen);
    desk.step(30);
    desk.phone.gesture(desk.now, 1, Phase::Update, (0.0, 0.5), 1.0, Focus::Pen);
    desk.step(30);
    desk.phone.gesture(desk.now, 1, Phase::End, (0.0, 0.5), 1.0, Focus::Pen);
    desk.step(30);
    assert!(desk.review.view.y > before.y + 0.4, "the viewport moved down the column: {before:?} -> {:?}", desk.review.view);
    desk.review.publish_pen(&mut desk.host);

    // 4. A QUICK BUTTON, and the agent finishing: statuses for the request, then the buzz
    desk.phone.press_button(desk.now, 1);
    desk.step(30);
    let (peer, label, text) = desk.review.instructions.pop().expect("the instruction reached AzReview");
    println!("instruction from {peer:.8}: {label}: {text}");
    desk.host.agent_finished(desk.now, AgentFinished { conversation_id: 1, file: "src/fc.rs".into(), line: 1337, kind: FinishKind::Done });
    desk.step(30);
    assert_eq!(desk.phone.view().finished_count, 1, "the phone was told, and buzzes by the settings it was given");

    println!("done: {} lines of log", desk.review.log.len());
}
