fn main() {
    match azshow::Args::parse(std::env::args().skip(1)) {
        Ok(args) => azshow::start(args),
        Err(message) => {
            if message.contains("USAGE:") {
                println!("{message}");
                std::process::exit(0);
            }
            eprintln!("azshow: {message}");
            std::process::exit(2);
        }
    }
}
