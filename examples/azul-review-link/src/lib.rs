//! azul-review-link: what the AzReview phone companion and AzReview say to each other.
//!
//! The phone is the touch surface of a desk with a pen tablet that has no touch: it pans and zooms the Mac's page
//! around the pen, browses the workspace's files and presses four quick buttons that stand for the user's
//! recurring instructions. This crate is the whole conversation, in the order a reader needs it:
//!
//! - [`limits`]: every cap (frames, strings, rows, rates, peers) and the [`limits::Capped`] diagnostic a cap leaves;
//! - [`frame`] and [`wire`]: the frames, and their bytes (versioned, compact, total decoding: no input panics);
//! - [`pairing`]: the offer, the proof of its one-time secret, the six-digit code both screens show;
//! - [`viewport`]: the arithmetic of a drag, a pinch and a fit on the page, shared by the phone's prediction and AzReview;
//! - [`pace`]: "newest wins" rate limiting (30 pen states and 60 gesture updates a second);
//! - [`link`]: the network seam (and, in a test, [`mock`]: an in-process network with latency, loss, reordering and a
//!   slow link); with the feature `azul` the real one over azul's iroh endpoint, `iroh_link`.
//!
//! Std only, and no azul types in the default build: the phone app and AzReview both depend on it.

pub mod frame;
pub mod limits;
pub mod link;
pub mod mock;
pub mod pace;
pub mod pairing;
pub mod phone;
pub mod view;
pub mod viewport;
pub mod wire;
