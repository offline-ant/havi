fn main() {
    // Handle --version before Makepad takes over args.
    if std::env::args().any(|a| a == "--version") {
        println!(
            "havi {} (havishell)",
            env!("CARGO_PKG_VERSION"),
        );
        return;
    }

    // Pylon subcommand: `havi pylon [args...]`
    let args: Vec<String> = std::env::args().collect();
    if args.get(1).map(|s| s.as_str()) == Some("pylon") {
        env_logger::init();
        pylon::cli::main(args[2..].to_vec());
        return;
    }

    // Single-instance check: if another HAVI is running, ask it to open a new tab.
    let url = std::env::var("HAVI_URL")
        .unwrap_or_else(|_| "hppr://u/web/index.html".to_string());
    if havi_protocols::instance::try_send_open(&url).is_ok() {
        eprintln!("[havi] Sent open command to running instance");
        return;
    }

    havishell::app::app_main()
}
