/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

use api::{units::*, ColorF};

pub enum DebugItem {
    Text {
        msg: String,
        color: ColorF,
        position: DevicePoint,
    },
    Rect {
        outer_color: ColorF,
        inner_color: ColorF,
        rect: DeviceRect,
    },
}

pub struct DebugMessage {
    pub msg: String,
    pub timestamp: u64,
}
