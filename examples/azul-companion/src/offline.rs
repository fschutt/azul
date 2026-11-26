//! The connection of a phone that has no network layer yet: it never finds a Mac and says so instead of pretending to. The
//! screens run on it as they run on the real client (the same [`PhoneApi`]); "Try it without a Mac" is the way to see the rest.

use azul_review_link::frame::{ButtonSet, Focus, Phase};
use azul_review_link::limits::Capped;
use azul_review_link::link::Millis;
use azul_review_link::pairing::{OfferError, PairingOffer};
use azul_review_link::phone::PhoneApi;
use azul_review_link::view::{FailureKind, PhoneView, Session};

/// How long it looks before it says it found nothing, in milliseconds.
pub const SEARCH_MS: Millis = 2_500;

/// A phone that is looking for a Mac and finds none.
pub struct OfflinePhone {
    view: PhoneView,
    attempt: u32,
    /// When the search that is under way ends.
    searching_until: Option<Millis>,
}

impl OfflinePhone {
    /// A phone that knows no Mac.
    #[must_use]
    pub fn new() -> OfflinePhone {
        OfflinePhone { view: PhoneView::new(), attempt: 0, searching_until: None }
    }

    fn search(&mut self, now: Millis) {
        self.attempt += 1;
        self.view.session = Session::Searching { attempt: self.attempt };
        self.searching_until = Some(now + SEARCH_MS);
    }
}

impl Default for OfflinePhone {
    fn default() -> Self {
        OfflinePhone::new()
    }
}

impl PhoneApi for OfflinePhone {
    fn view(&self) -> &PhoneView {
        &self.view
    }

    fn poll(&mut self, now: Millis) -> bool {
        match self.searching_until {
            Some(until) if now >= until => {
                self.searching_until = None;
                self.view.session = Session::Failed { kind: FailureKind::NotFound };
                true
            }
            _ => false,
        }
    }

    fn gesture(&mut self, _now: Millis, _id: u32, _phase: Phase, _pan: (f32, f32), _zoom: f32, _focus: Focus) {}

    fn fit(&mut self, _now: Millis) {}

    fn page_step(&mut self, _now: Millis, _step: i8) {}

    fn reset_view(&mut self, _now: Millis) {}

    fn continue_reviewing(&mut self, _now: Millis) {}

    fn list_dir(&mut self, _now: Millis, path: &str, _offset: u32) {
        self.view.begin_listing(path);
        self.view.files_failed("not-connected");
    }

    fn open_file(&mut self, _now: Millis, _path: &str, _line: Option<u32>) {}

    fn press_button(&mut self, _now: Millis, _index: usize) {}

    fn set_buttons(&mut self, _now: Millis, _buttons: &ButtonSet) -> Vec<Capped> {
        Vec::new()
    }

    fn begin_pairing(&mut self, now: Millis, offer: &str) -> Result<(), OfferError> {
        PairingOffer::parse(offer)?;
        self.search(now);
        Ok(())
    }

    fn retry(&mut self, now: Millis) {
        self.search(now);
    }

    fn cancel(&mut self, _now: Millis) {
        // no Mac is ever kept here: stopping the search is going back to the start
        self.view.session = Session::Unpaired;
        self.searching_until = None;
    }

    fn forget_mac(&mut self, _now: Millis) {
        self.view.session = Session::Unpaired;
        self.searching_until = None;
    }
}

#[cfg(test)]
#[path = "offline_test.rs"]
mod offline_test;
