fn main() {
    match azsheets::Args::parse(std::env::args().skip(1)) {
        Ok(args) => azsheets::start(args),
        Err(message) => {
            // appkit hands the usage back as an "error" for -h / --help.
            if message.contains("USAGE") {
                println!("{message}");
                std::process::exit(0);
            }
            eprintln!("AzSheets: {message}");
            std::process::exit(2);
        }
    }
}
