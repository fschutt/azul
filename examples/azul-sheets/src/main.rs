fn main() {
    match azsheets::Args::parse(std::env::args().skip(1)) {
        Ok(args) => azsheets::start(args),
        Err(message) => {
            if message.starts_with("azsheets -") {
                println!("{message}");
                std::process::exit(0);
            }
            eprintln!("azsheets: {message}");
            std::process::exit(2);
        }
    }
}
