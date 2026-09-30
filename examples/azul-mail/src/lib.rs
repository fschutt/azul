//! AzMail: sign in to an IMAP account and sync its mail to files.

pub mod account;
pub mod auth;
pub mod folders;
pub mod html;
pub mod message;
pub mod mutf7;
pub mod store;
pub mod sync;

#[cfg(test)]
mod testutil;

pub fn start() {
    todo!()
}
