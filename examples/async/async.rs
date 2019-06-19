#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

extern crate azul;

use azul::{
    prelude::*,
    widgets::{button::Button, label::Label},
};
use std::{
    thread,
    time::{Duration, Instant},
    sync::{Arc, Mutex},
};

#[derive(Debug, PartialEq)]
enum ConnectionStatus {
    NotConnected,
    Connected,
    InProgress(Instant, Duration),
}

struct MyDataModel {
    connection_status: Arc<Mutex<ConnectionStatus>>,
}

impl Layout for MyDataModel {

    fn layout(&self, _info: LayoutInfo<Self>) -> Dom<Self> {

        use self::ConnectionStatus::*;

        println!("layout!");

        let connection_status = &*self.connection_status.lock().unwrap();
        let status = match connection_status {
            ConnectionStatus::NotConnected       => format!("Not connected!"),
            ConnectionStatus::Connected          => format!("You are connected!"),
            ConnectionStatus::InProgress(_, d)   => format!("Loading... {}.{:02}s", d.as_secs(), d.subsec_millis()),
        };

        let mut dom = Dom::div()
            .with_child(Label::new(status.clone()).dom());

        match connection_status {
            NotConnected => {
                dom.add_child(
                    Button::with_label("Connect to database...").dom()
                    .with_callback(On::MouseUp, start_connection)
                );
            },
            Connected => {
                dom.add_child(
                    Button::with_label(format!("{}\nRetry?", status)).dom()
                    .with_callback(On::MouseUp, reset_connection)
                );
            }
            InProgress(_, _) => { },
        }

        dom
    }
}

fn reset_connection(event: CallbackInfo<MyDataModel>) -> UpdateScreen {
    event.state.data.connection_status.modify(|state| *state = ConnectionStatus::NotConnected);
    Redraw
}

fn start_connection(event: CallbackInfo<MyDataModel>) -> UpdateScreen {
    event.state.data.connection_status.modify(|state| {
        *state = ConnectionStatus::InProgress(Instant::now(), Duration::from_secs(0));
    });
    let task = Task::new(Arc::clone(&event.state.data.connection_status), connect_to_db_async);
    event.state.add_task(task);
    event.state.add_timer(TimerId::new(), Timer::new(timer_timer));
    Redraw
}

fn timer_timer(event: TimerCallbackInfo<MyDataModel>) -> (UpdateScreen, TerminateTimer) {
    if let ConnectionStatus::InProgress(start, duration) = &mut *event.state.connection_status.lock().unwrap() {
        *duration = Instant::now() - *start;
        (Redraw, TerminateTimer::Continue)
    } else {
        (DontRedraw, TerminateTimer::Terminate)
    }
}

fn connect_to_db_async(connection_status: Arc<Mutex<ConnectionStatus>>, _: DropCheck) {
    thread::sleep(Duration::from_secs(10)); // simulate slow load
    connection_status.modify(|state| { *state = ConnectionStatus::Connected; });
}

fn main() {
    let model = MyDataModel { connection_status: Arc::new(Mutex::new(ConnectionStatus::NotConnected)) };
    let mut app = App::new(model, AppConfig::default()).unwrap();
    let window = app.create_window(WindowCreateOptions::default(), css::native()).unwrap();
    app.run(window).unwrap();
}
