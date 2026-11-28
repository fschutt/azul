//! The engine's `DisplayFeatures` of an iOS display: the answer of `ios_display` (points, the table of the display class and
//! what UIKit measured) in the shapes the layout window takes (`LayoutWindow::report_display_features`). Host-compiled and tested
//! (the shell that calls it, `shell2/ios/system_ui.rs`, only exists for an iOS target).

use azul_core::{
    display_features::{
        CornerRadii, CutoutShape, DisplayCutout, DisplayCutoutVec, DisplayFeatures, FingerprintKind, FingerprintSensor,
        OptionFingerprintSensor, OptionFingerprintSpot, OptionSystemBar, SystemBar, SystemBars,
    },
    geom::{LogicalPosition, LogicalRect, LogicalSize},
};

use super::ios_display::{IosCutout, IosDisplay, IosFingerprint, IosRect};

fn logical(rect: &IosRect) -> LogicalRect {
    LogicalRect::new(
        LogicalPosition { x: rect.x, y: rect.y },
        LogicalSize { width: rect.width, height: rect.height },
    )
}

/// The engine's `DisplayFeatures` of an iOS display: the cutout `approximate` (it is from a table), the corners, the
/// status bar with the rect it has when shown and whether it is on screen, the home indicator and the fingerprint
/// sensor (never with a place on the glass).
#[must_use]
pub fn features_of(display: &IosDisplay, status_visible: bool, navigation_visible: bool) -> DisplayFeatures {
    let cutouts = display.cutout.map_or_else(Vec::new, |(rect, kind)| {
        vec![DisplayCutout {
            rect: logical(&rect),
            shape: match kind {
                IosCutout::Island => CutoutShape::Pill,
                // `None` never has a rect (`ios_display` answers no cutout for it)
                IosCutout::Notch | IosCutout::None => CutoutShape::Notch,
            },
            approximate: true,
        }]
    });
    let bar = |rect: Option<IosRect>, visible: bool| match rect {
        Some(rect) => OptionSystemBar::Some(SystemBar { rect: logical(&rect), visible }),
        None => OptionSystemBar::None,
    };
    DisplayFeatures {
        cutouts: DisplayCutoutVec::from_vec(cutouts),
        corner_radii: CornerRadii {
            top_left: display.corner_radius,
            top_right: display.corner_radius,
            bottom_right: display.corner_radius,
            bottom_left: display.corner_radius,
        },
        bars: SystemBars {
            status: bar(display.status_bar, status_visible),
            navigation: bar(display.home_indicator, navigation_visible),
        },
        fingerprint: match display.fingerprint {
            IosFingerprint::None => OptionFingerprintSensor::None,
            IosFingerprint::Front => {
                OptionFingerprintSensor::Some(FingerprintSensor { kind: FingerprintKind::Front, spot: OptionFingerprintSpot::None })
            }
            IosFingerprint::Unknown => OptionFingerprintSensor::Some(FingerprintSensor {
                kind: FingerprintKind::Unknown,
                spot: OptionFingerprintSpot::None,
            }),
        },
    }
}


#[cfg(test)]
#[path = "ios_display_features_test.rs"]
mod ios_display_features_test;
