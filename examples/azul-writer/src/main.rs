// Desktop / iOS entry point - the app logic lives in the library so the same
// crate can also build as an Android cdylib (see src/lib.rs).
fn main() {
    match azwriter::Args::parse(std::env::args().skip(1)) {
        Ok(args) => azwriter::start(args),
        Err(message) => {
            // `--help` comes back as an "error" carrying the usage text; a
            // real mistake carries a complaint. Usage goes to stdout and
            // exits 0, mistakes go to stderr and exit 2 - the shape every
            // shell script expects.
            let asked_for_help = message.starts_with("azwriter -");
            if asked_for_help {
                println!("{message}");
                std::process::exit(0);
            }
            eprintln!("azwriter: {message}");
            std::process::exit(2);
        }
    }
}
