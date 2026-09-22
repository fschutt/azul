use azul::prelude::*;
use std::env;

extern "C" fn layout(_: RefAny, _: LayoutCallbackInfo) -> Dom {
    Dom::create_body().with_css(AzString::from("body { background: #1e1e1e; }"))
}

fn open_browser(url: &str) {
    #[cfg(target_os = "macos")]
    let _ = std::process::Command::new("open").arg(url).spawn();
    #[cfg(target_os = "windows")]
    let _ = std::process::Command::new("cmd").args(&["/C", "start", url]).spawn();
    #[cfg(target_os = "linux")]
    let _ = std::process::Command::new("xdg-open").arg(url).spawn();
}

fn main() {
    // Force the debug server to start on port 8080
    env::set_var("AZ_DEBUG", "8080");
    env::set_var("AZUL_DEBUG", "8080");

    // Start a thread to open the browser after a short delay
    std::thread::spawn(|| {
        std::thread::sleep(std::time::Duration::from_secs(1));
        open_browser("http://localhost:8080/debugger.html");
    });

    let config = AppConfig::create();
    let app = App::create(RefAny::new(()), config);
    let options = WindowCreateOptions::create(layout);
    
    app.run(options);
}
