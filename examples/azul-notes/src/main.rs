fn main() {
    match aznotes::Args::parse(std::env::args().skip(1)) {
        Ok(args) => aznotes::start(args),
        Err(message) => {
            // The usage (-h / --help) is an answer, anything else a mistake.
            if message.contains("USAGE") && !message.starts_with("unknown option") {
                println!("{message}");
                std::process::exit(0);
            }
            eprintln!("AzNotes: {message}");
            std::process::exit(2);
        }
    }
}
