fn main() {
    match azwriter::Args::parse(std::env::args().skip(1)) {
        Ok(args) => azwriter::start(args),
        Err(message) => {
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
