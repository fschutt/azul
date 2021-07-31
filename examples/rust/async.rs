#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use azul::prelude::*;
use self::ConnectionStatus::*;

// data model for the main thread
#[derive(Default)]
struct MyDataModel {
    connection_status: ConnectionStatus,
}

enum ConnectionStatus {
    NotConnected {
        // which database to connect to, ex. "user@localhost:5432"
        database: String,
    },
    InProgress {
        // handle to the background thread
        background_thread_id: ThreadId,
        // time when the thread was started
        start_time: Instant,
        // estimated time to completion
        estimated_wait: Duration,
        // data that has been loaded so far
        data_in_progress: Vec<usize>,
        // stage of the connection (initial connection done)
        stage: ConnectionStage,
    },
    DataLoaded {
        // the established connection
        data: Vec<usize>
    },
    Error {
        // error establishing a connection
        error: String,
    }
}

impl Default for ConnectionStatus {
    fn default() -> Self {
        ConnectionStatus::NotConnected {
            database: String::new()
        }
    }
}

enum ConnectionStage {
    EstablishingConnection,
    ConnectionEstablished,
    LoadingData {
        percent_done: f32
    },
    LoadingFinished,
}

// Main function that renders the UI
extern "C" fn render_ui(data: &mut RefAny, _: LayoutCallbackInfo) -> StyledDom {

    use self::ConnectionStatus::*;

    let body = Dom::body();

    let downcasted = match data.downcast_ref::<MyDataModel>() {
        Some(f) => f,
        None => return body, // error
    };

    match downcasted.connection_status {
        NotConnected { database } => {

        },
        InProgress { stage, start_time, estimated_wait, .. } => {

        },
        DataLoaded { data } => {

        },
        Error { error } => {

        },
    }
}

// Callback that runs when the "connect to database" button is clicked
extern "C" fn start_background_thread(datamodel: &mut RefAny, event: CallbackInfo) -> Update {

    // Copy the string of what database to connect to and
    // use it to initialize a new background thread
    let data_mut = datamodel.downcast_mut::<MyDataModel>()?;
    let database_to_connect_to = match data_mut.connection_status {
        NotConnected { database } => database.clone(),
        _ => return Update::DoNothing, // error
    };
    let init_data = RefAny::new(BackgroundThreadInit { database_to_connect_to });
    let thread_id = event.start_thread(init_data, datamodel.clone(), background_thread);

    *data_mut.connection_status = InProgress {
        background_thread_id: thread_id,
        start_time: Instant::now(),
        estimated_wait: Duration::from_secs(10),
        stage: ConnectionStage::EstablishingConnection,
    };

    // Update the UI
    Update::RefreshDom
}

// Callback that runs when the "cancel" button is clicked while the background thread is running
extern "C" fn stop_background_thread(data: &mut RefAny, event: CallbackInfo) -> Update {
    let data_mut = data.downcast_mut::<MyDataModel>()?;
    let thread_id = match data_mut.connection_status {
        InProgress { background_thread_id, .. } => background_thread_id.clone(),
        _ => return Update::DoNothing, // error
    };
    event.stop_thread(thread_id);
    *data_mut.connection_status = ConnectionStatus::default();
    Update::RefreshDom
}

// Callback that runs when the "reset" button is clicked (resets the data)
extern "C" fn reset(data: &mut RefAny, event: CallbackInfo) -> Update {
    let data_mut = data.downcast_mut::<MyDataModel>()?;
    match data_mut.connection_status {
        DataLoaded { .. } => { },
        _ => return Update::DoNothing, // error
    };
    *data_mut.connection_status = ConnectionStatus::default();
    Update::RefreshDom
}

// Data model of data that is sent from the main to the background thread
struct BackgroundThreadInit {
    database: String,
}

// Data model of data that is returned from the background thread
enum BackgroundThreadReturn {
    StatusUpdated { new: ConnectionStage },
    ErrorOccurred { error: String },
    NewDataLoaded { data: Vec<usize> },
}

// Callback that "writes data back" from the background thread to the main thread
// This function runs on the main thread, so that there can't be any data races
// Returns whether the UI should update
extern "C" fn writeback_callback(app_data: &mut RefAny, incoming_data: RefAny) -> Update {

    use crate::BackgroundThreadReturn::*;

    let data_mut = data.downcast_mut::<MyDataModel>()?;
    let incoming_data = incoming_data.downcast_mut::<BackgroundThreadReturn>()?;

    match &mut *incoming_data {
        StatusUpdated { new } => {
            match &mut data_mut.connection_status {
                InProgress { stage, .. } => {
                    *stage = new;
                    Update::RefreshDom
                },
                _ => Update::DoNothing,
            }
        },
        ErrorOccurred { error } => {
            data_mut.connection_status = Error { error };
            Update::RefreshDom
        },
        NewDataLoaded { data } => {
            match &mut data_mut.connection_status {
                InProgress { data_in_progress, .. } => {
                    data_in_progress.append(data);
                    Update::RefreshDom
                },
                _ => Update::DoNothing,
            }
        }
    }
}

// Function that executes in a non-main thread
extern "C" fn background_thread(
    initial_data: RefAny,
    sender: ThreadSender,
    recv: ThreadReceiver,
    _: DropCheck
) {

    let initial_data = match initial_data.downcast_ref::<BackgroundThreadInit>() {
        Some(s) => s,
        None => return, // error
    };

    // connect to the database (blocking)
    let connection = match postgres::establish_connection(&initial_data.database) {
        Ok(db) => db,
        Err(e) => {
            sender.send(RefAny::new(ErrorOccurred { error: e }));
            return;
        }
    };

    // if in the meantime we got a "cancel" message, quit the thread
    if recv.recv() == ThreadReceiveMsg::Terminate {
        return;
    }

    // update the UI again to notify the user that the connection has been established
    sender.send(RefAny::new(StatusUpdated {
        new: ConnectionStatus::ConnectionEstablished
    }));

    let total_items = postgres::estimate_item_count(&connection, "SELECT * FROM large_table;");
    let mut items_loaded = 0;

    for row in postgres::query_rows(&connection, "SELECT * FROM large_table;") {
        // If in the meantime we got a "cancel" message, quit the thread
        if recv.recv() == ThreadReceiveMsg::Terminate {
            return;
        } else {
            items_loaded += data.len();
            // As soon as each row is loaded, update the UI
            sender.send(RefAny::new(NewDataLoaded { data: row }));
            // Calculate and update the percentage count
            sender.send(RefAny::new(ConnectionStage {
                stage: ConnectionStage::LoadingData {
                    percent_done: items_loaded as f32 / total_items as f32 * 100.0,
                }
            }));
        }
    }

    sender.send_msg(RefAny::new(ConnectionStage {
        stage: ConnectionStage::LoadingDone,
    }));
}

// mock module to simulate a database
mod postgres {

    // Mock database connection
    struct Database { }

    type Row = [usize;10];

    static LARGE_TABLE: &[Row] = &[
        [0, 1, 2, 3, 4, 5, 6, 7, 8, 9],
        [0, 1, 2, 3, 4, 5, 6, 7, 8, 9],
        [0, 1, 2, 3, 4, 5, 6, 7, 8, 9],
        [0, 1, 2, 3, 4, 5, 6, 7, 8, 9],
        [0, 1, 2, 3, 4, 5, 6, 7, 8, 9],
        [0, 1, 2, 3, 4, 5, 6, 7, 8, 9],
        [0, 1, 2, 3, 4, 5, 6, 7, 8, 9],
        [0, 1, 2, 3, 4, 5, 6, 7, 8, 9],
        [0, 1, 2, 3, 4, 5, 6, 7, 8, 9],
        [0, 1, 2, 3, 4, 5, 6, 7, 8, 9],
    ];

    // each of these functions blocks to simulate latency on a real database
    pub(in super) fn establish_connection(database: &str) -> Result<Database, String> {
        std::thread::sleep(Duration::from_secs(1));
        Ok(Database { })
    }

    pub(in super) fn estimate_item_count(db: &Database, _query: &str) -> usize {
        LARGE_TABLE.len() * LARGE_TABLE[0].len()
    }

    pub(in super) fn query_rows(db: &Database, _query: &str) -> impl Iterator<Item=Row> {
        LARGE_TABLE.iter().map(|i| {
            // let's simulate that each row / query takes one second to load in
            std::thread::sleep(Duration::from_secs(1));
            i
        })
    }
}

fn main() {
    let app = App::new(RefAny::new(MyDataModel::default()), AppConfig::new(LayoutSolver::Default));
    app.run(WindowCreateOptions::new(render_ui));
}
