//! AzMail's HTML sanitizer as a filter: mail HTML on stdin, the XHTML azul
//! renders on stdout.
//!
//! What `scripts/refci/mail_boxes.py` runs each mail of `tests/mail_corpus/`
//! through before comparing azul's layout boxes with Chrome's, so the
//! comparison sees exactly the markup AzMail's reading pane shows.
//!
//! ```text
//! AZ_LINK_PATH=$PWD/target/azul-lib cargo build --release -p AzMail --bin azmail-sanitize
//! target/release/azmail-sanitize < mail.html > mail.xhtml
//! ```
//!
//! Exit 0; the number of blocked images goes to stderr.

use std::io::{Read, Write};

fn main() {
    let mut input = String::new();
    if let Err(e) = std::io::stdin().read_to_string(&mut input) {
        eprintln!("azmail-sanitize: stdin is not UTF-8 text: {e}");
        std::process::exit(2);
    }
    let sanitized = azmail::html::sanitize(&input);
    let mut out = std::io::stdout().lock();
    if let Err(e) = out
        .write_all(sanitized.xhtml.as_bytes())
        .and_then(|()| out.flush())
    {
        eprintln!("azmail-sanitize: cannot write: {e}");
        std::process::exit(1);
    }
    if sanitized.blocked_images > 0 {
        eprintln!("azmail-sanitize: {} image(s) blocked", sanitized.blocked_images);
    }
}
