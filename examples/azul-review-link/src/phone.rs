//! The phone's side of the conversation: what its screens ask of the connection ([`PhoneApi`]), and the client that
//! answers it over a [`crate::link::Link`] ([`crate::client::PhoneClient`]). The screens never see a frame, a peer or a
//! socket: they read a [`PhoneView`] and call these methods, so the same screens run against the real client on a
//! phone and against a scripted Mac in the simulator.

use crate::frame::{ButtonSet, Focus, Phase};
use crate::limits::Capped;
use crate::link::Millis;
use crate::pairing::OfferError;
use crate::view::PhoneView;

/// What the phone's screens ask of the connection.
pub trait PhoneApi {
    /// What the screens draw from.
    fn view(&self) -> &PhoneView;

    /// Lets the connection work (receive, send, time out, reconnect). True when the view changed.
    fn poll(&mut self, now: Millis) -> bool;

    /// A drag and / or a pinch on the pad: `pan` is how far the viewport has moved over the page and `zoom` the factor
    /// so far, both CUMULATIVE since the gesture `id` began (page fractions; see `viewport.rs`).
    fn gesture(&mut self, now: Millis, id: u32, phase: Phase, pan: (f32, f32), zoom: f32, focus: Focus);

    /// Fit the page to the width of the Mac's window (double tap).
    fn fit(&mut self, now: Millis);

    /// The next (+1) or previous (-1) page.
    fn page_step(&mut self, now: Millis, step: i8);

    /// The default zoom at the top of the page.
    fn reset_view(&mut self, now: Millis);

    /// "Continue reviewing": AzReview goes back to the return point it saved (and clears it).
    fn continue_reviewing(&mut self, now: Millis);

    /// Asks for the rows of a folder from row `offset` on.
    fn list_dir(&mut self, now: Millis, path: &str, offset: u32);

    /// AzReview opens the file (at the line, or at its first unreviewed line).
    fn open_file(&mut self, now: Millis, path: &str, line: Option<u32>);

    /// A tap on quick button `index`: the instruction goes to AzReview with the context at the pen.
    fn press_button(&mut self, now: Millis, index: usize);

    /// Saves the four buttons in AzReview's settings (cut to their caps first: what was cut is returned).
    fn set_buttons(&mut self, now: Millis, buttons: &ButtonSet) -> Vec<Capped>;

    /// Starts pairing with the offer a QR code, a pasted text or the cable gave.
    ///
    /// # Errors
    /// The text is no offer.
    fn begin_pairing(&mut self, now: Millis, offer: &str) -> Result<(), OfferError>;

    /// Tries again after a failure.
    fn retry(&mut self, now: Millis);

    /// Stops what is under way (a pairing, a search for the Mac) WITHOUT forgetting the Mac the phone is paired with: a phone
    /// that has none is unpaired again, one that has one looks for it again (or, when it was already looking, stops and says
    /// it did not find it).
    fn cancel(&mut self, now: Millis);

    /// Forgets the Mac: the phone is unpaired again.
    fn forget_mac(&mut self, now: Millis);
}
