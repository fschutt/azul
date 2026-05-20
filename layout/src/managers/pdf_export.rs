//! PDF-export request channel (SUPER_PLAN_2 §4 P5.1).
//!
//! A callback calls `CallbackInfo::export_to_pdf(path)`, which queues the
//! target path here. The dll layout pass drains it and runs the `printpdf`
//! export (`dll::desktop::extra::pdf`) — which, at drain time, has the
//! freshly-laid-out display list to walk. Fire-and-forget: unlike the
//! biometric / keyring channels there's no result read back, so this is
//! just the request side (no manager struct).
//!
//! No platform deps (SUPER_PLAN_2 §0.5); same poison-recovering
//! `Mutex<Vec<_>>` pattern as the other channels.

use alloc::vec::Vec;
use azul_css::AzString;

static PENDING_EXPORTS: std::sync::Mutex<Vec<AzString>> = std::sync::Mutex::new(Vec::new());

/// Queue a PDF export to `path` from a callback. Picked up by the dll
/// layout pass. Thread-safe; poison-recovering.
pub fn push_pdf_export_request(path: AzString) {
    let mut q = PENDING_EXPORTS.lock().unwrap_or_else(|e| e.into_inner());
    q.push(path);
}

/// Drain every queued export path, in arrival order. Called once per layout
/// pass; the dll runs the printpdf export for each.
pub fn drain_pdf_export_requests() -> Vec<AzString> {
    let mut q = PENDING_EXPORTS.lock().unwrap_or_else(|e| e.into_inner());
    core::mem::take(&mut *q)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn requests_round_trip() {
        let _ = drain_pdf_export_requests();
        push_pdf_export_request(AzString::from_const_str("/tmp/a.pdf"));
        push_pdf_export_request(AzString::from_const_str("/tmp/b.pdf"));
        let drained = drain_pdf_export_requests();
        assert_eq!(drained.len(), 2, "both queued exports drain in order");
        assert_eq!(drained[0].as_str(), "/tmp/a.pdf");
        assert_eq!(drained[1].as_str(), "/tmp/b.pdf");
        assert!(drain_pdf_export_requests().is_empty());
    }
}
