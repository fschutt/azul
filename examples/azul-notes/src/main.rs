fn main() {
    match aznotes::Args::parse(std::env::args().skip(1)) {
        Ok(args) => aznotes::start(args),
        Err(message) => {
            if message.starts_with("aznotes -") {
                println!("{message}");
                std::process::exit(0);
            }
            eprintln!("aznotes: {message}");
            std::process::exit(2);
        }
    }
}
