//! azinput — inject pointer/keyboard events into a KWin Wayland session via
//! the `org_kde_kwin_fake_input` protocol (no root, no uinput; events route
//! through the real compositor and reach apps as genuine wl_pointer input).
//!
//! Commands (chain several in one invocation, separated by a literal ","):
//!   move X Y                       absolute pointer motion (global coords)
//!   rel DX DY                      relative pointer motion
//!   btn NAME ACTION                NAME: left|right|middle; ACTION: down|up|click
//!   click X Y                      move + left click
//!   drag X1 Y1 X2 Y2 [STEPS] [MS]  press, interpolated move, release
//!                                  (defaults: 16 steps, 12 ms apart)
//!   wheel V                        vertical scroll axis; V in wl units,
//!                                  negative = scroll up / away from user
//!   wheelat X Y V                  move + wheel
//!   key CODE ACTION                evdev keycode (KEY_ESC=1, KEY_A=30, ...)
//!   sleep MS
//!
//! Example:
//!   azinput move 500 400 , btn left down , rel 30 0 , btn left up

use std::{thread, time::Duration};

use wayland_client::{protocol::wl_registry, Connection, Dispatch, EventQueue, QueueHandle};

mod proto {
    #![allow(non_upper_case_globals, non_camel_case_types, unused)]
    pub mod fake_input {
        use wayland_client;
        pub mod __interfaces {
            wayland_scanner::generate_interfaces!("./fake-input.xml");
        }
        use self::__interfaces::*;
        wayland_scanner::generate_client_code!("./fake-input.xml");
    }
}

use proto::fake_input::org_kde_kwin_fake_input::OrgKdeKwinFakeInput;

const BTN_LEFT: u32 = 0x110;
const BTN_RIGHT: u32 = 0x111;
const BTN_MIDDLE: u32 = 0x112;
const PRESSED: u32 = 1;
const RELEASED: u32 = 0;

struct App {
    fake: Option<(OrgKdeKwinFakeInput, u32)>,
}

impl Dispatch<wl_registry::WlRegistry, ()> for App {
    fn event(
        state: &mut Self,
        registry: &wl_registry::WlRegistry,
        event: wl_registry::Event,
        _: &(),
        _: &Connection,
        qh: &QueueHandle<App>,
    ) {
        if let wl_registry::Event::Global { name, interface, version } = event {
            if interface == "org_kde_kwin_fake_input" {
                let v = version.min(5);
                let fake = registry.bind::<OrgKdeKwinFakeInput, _, App>(name, v, qh, ());
                state.fake = Some((fake, v));
            }
        }
    }
}

impl Dispatch<OrgKdeKwinFakeInput, ()> for App {
    fn event(
        _: &mut Self,
        _: &OrgKdeKwinFakeInput,
        _: <OrgKdeKwinFakeInput as wayland_client::Proxy>::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<App>,
    ) {
        // interface has no events
    }
}

struct Ctx {
    fake: OrgKdeKwinFakeInput,
    conn: Connection,
    queue: EventQueue<App>,
    app: App,
}

impl Ctx {
    /// Flush + roundtrip so each step is delivered (and ordered) before the
    /// next — fake input only works reliably with real pacing.
    fn sync(&mut self) {
        let _ = self.queue.roundtrip(&mut self.app);
    }

    fn move_abs(&mut self, x: f64, y: f64) {
        self.fake.pointer_motion_absolute(x, y);
        self.sync();
    }

    fn button(&mut self, code: u32, state: u32) {
        self.fake.button(code, state);
        self.sync();
    }
}

fn btn_code(name: &str) -> u32 {
    match name {
        "right" => BTN_RIGHT,
        "middle" => BTN_MIDDLE,
        _ => BTN_LEFT,
    }
}

fn parse<T: std::str::FromStr>(s: &str, what: &str) -> T {
    s.parse().unwrap_or_else(|_| {
        eprintln!("azinput: cannot parse {what}: {s:?}");
        std::process::exit(2);
    })
}

fn run_cmd(ctx: &mut Ctx, cmd: &[String]) {
    match cmd[0].as_str() {
        "move" => {
            let (x, y) = (parse(&cmd[1], "x"), parse(&cmd[2], "y"));
            ctx.move_abs(x, y);
        }
        "rel" => {
            let (dx, dy): (f64, f64) = (parse(&cmd[1], "dx"), parse(&cmd[2], "dy"));
            ctx.fake.pointer_motion(dx, dy);
            ctx.sync();
        }
        "btn" => {
            let code = btn_code(&cmd[1]);
            match cmd[2].as_str() {
                "down" => ctx.button(code, PRESSED),
                "up" => ctx.button(code, RELEASED),
                _ => {
                    ctx.button(code, PRESSED);
                    thread::sleep(Duration::from_millis(40));
                    ctx.button(code, RELEASED);
                }
            }
        }
        "click" => {
            let (x, y) = (parse(&cmd[1], "x"), parse(&cmd[2], "y"));
            ctx.move_abs(x, y);
            thread::sleep(Duration::from_millis(40));
            ctx.button(BTN_LEFT, PRESSED);
            thread::sleep(Duration::from_millis(40));
            ctx.button(BTN_LEFT, RELEASED);
        }
        "drag" => {
            let (x1, y1): (f64, f64) = (parse(&cmd[1], "x1"), parse(&cmd[2], "y1"));
            let (x2, y2): (f64, f64) = (parse(&cmd[3], "x2"), parse(&cmd[4], "y2"));
            let steps: u32 = cmd.get(5).map(|s| parse(s, "steps")).unwrap_or(16);
            let ms: u64 = cmd.get(6).map(|s| parse(s, "ms")).unwrap_or(12);
            ctx.move_abs(x1, y1);
            thread::sleep(Duration::from_millis(60));
            ctx.button(BTN_LEFT, PRESSED);
            thread::sleep(Duration::from_millis(60));
            for i in 1..=steps {
                let t = i as f64 / steps as f64;
                ctx.move_abs(x1 + (x2 - x1) * t, y1 + (y2 - y1) * t);
                thread::sleep(Duration::from_millis(ms));
            }
            thread::sleep(Duration::from_millis(60));
            ctx.button(BTN_LEFT, RELEASED);
        }
        "wheel" => {
            let v: f64 = parse(&cmd[1], "value");
            ctx.fake.axis(0, v);
            ctx.sync();
        }
        "wheelat" => {
            let (x, y) = (parse(&cmd[1], "x"), parse(&cmd[2], "y"));
            let v: f64 = parse(&cmd[3], "value");
            ctx.move_abs(x, y);
            thread::sleep(Duration::from_millis(40));
            ctx.fake.axis(0, v);
            ctx.sync();
        }
        "key" => {
            let code: u32 = parse(&cmd[1], "keycode");
            match cmd[2].as_str() {
                "down" => {
                    ctx.fake.keyboard_key(code, PRESSED);
                    ctx.sync();
                }
                "up" => {
                    ctx.fake.keyboard_key(code, RELEASED);
                    ctx.sync();
                }
                _ => {
                    ctx.fake.keyboard_key(code, PRESSED);
                    ctx.sync();
                    thread::sleep(Duration::from_millis(40));
                    ctx.fake.keyboard_key(code, RELEASED);
                    ctx.sync();
                }
            }
        }
        "sleep" => {
            let ms: u64 = parse(&cmd[1], "ms");
            thread::sleep(Duration::from_millis(ms));
        }
        other => {
            eprintln!("azinput: unknown command {other:?}");
            std::process::exit(2);
        }
    }
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.is_empty() {
        eprintln!("usage: azinput CMD ARGS [, CMD ARGS]...   (see source header)");
        std::process::exit(2);
    }

    let conn = Connection::connect_to_env().expect("azinput: no wayland display");
    let mut queue = conn.new_event_queue();
    let qh = queue.handle();
    conn.display().get_registry(&qh, ());
    let mut app = App { fake: None };
    queue.roundtrip(&mut app).expect("azinput: initial roundtrip failed");

    let (fake, version) = app
        .fake
        .clone()
        .expect("azinput: compositor does not advertise org_kde_kwin_fake_input (not KWin?)");
    if version < 4 {
        eprintln!("azinput: warning: fake-input v{version} < 4, keyboard_key unavailable");
    }

    fake.authenticate("aztest".into(), "azul automated GUI verification".into());
    let mut ctx = Ctx { fake, conn, queue, app };
    ctx.sync();

    for cmd in args.split(|a| a == ",") {
        if cmd.is_empty() {
            continue;
        }
        run_cmd(&mut ctx, cmd);
    }
    ctx.sync();
}
