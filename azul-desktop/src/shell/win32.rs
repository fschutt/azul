#![cfg(target_os = "windows")]

//! Win32 implementation of the window shell containing all functions
//! related to running the application

use azul_core::window::{MonitorVec, WindowCreateOptions};
use crate::app::App;

pub fn get_monitors(app: &App) -> MonitorVec {
    MonitorVec::from_const_slice(&[])
}

/// Main function that starts when app.run() is invoked
pub fn run(mut app: App, root_window: WindowCreateOptions) {
    println!("running app!");

    let App {
        mut data,
        config,
        windows,
        mut image_cache,
        mut fc_cache,
    } = app;

}