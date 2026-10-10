//! `azul-bridge`: the Azlin Bridge's command line (see `azul_bridge::cli`).

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let options = match azul_bridge::cli::parse_args(&args) {
        Ok(options) => options,
        Err(why) => {
            if why != "help" {
                eprintln!("azul-bridge: {why}");
            }
            eprintln!("{}", azul_bridge::cli::USAGE);
            std::process::exit(if why == "help" { 0 } else { 3 });
        }
    };
    if let Err(why) = azul_bridge::cli::run(&options) {
        eprintln!("azul-bridge: {why}");
        std::process::exit(1);
    }
}
