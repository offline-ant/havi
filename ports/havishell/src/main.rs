fn main() {
    // Handle --version before Makepad takes over args.
    if std::env::args().any(|a| a == "--version") {
        println!(
            "havi {} (havishell)",
            env!("CARGO_PKG_VERSION"),
        );
        return;
    }

    // Parse HAVI-specific flags before Makepad takes over args.
    let args: Vec<String> = std::env::args().collect();

    // Pylon subcommand: `havi pylon [args...]`
    if args.get(1).map(|s| s.as_str()) == Some("pylon") {
        env_logger::init();
        pylon::cli::main(args[2..].to_vec());
        return;
    }

    // Extract --path and --home flags, set env vars for downstream use.
    {
        let mut i = 1;
        while i < args.len() {
            match args[i].as_str() {
                "--path" => {
                    if let Some(v) = args.get(i + 1) {
                        std::env::set_var("HAVI_PATH", v);
                        i += 2;
                    } else {
                        i += 1;
                    }
                },
                "--home" => {
                    if let Some(v) = args.get(i + 1) {
                        std::env::set_var("HAVI_HOME", v);
                        i += 2;
                    } else {
                        i += 1;
                    }
                },
                _ => i += 1,
            }
        }
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
