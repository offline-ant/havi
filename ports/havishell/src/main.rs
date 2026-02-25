fn main() {
    // Handle --version before Makepad takes over args.
    if std::env::args().any(|a| a == "--version") {
        println!("havi {} (havishell)", env!("CARGO_PKG_VERSION"),);
        return;
    }

    // Parse HAVI-specific flags before Makepad takes over args.
    let args: Vec<String> = std::env::args().collect();
    let argv0 = std::path::Path::new(args.first().map(|s| s.as_str()).unwrap_or("havi"))
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or("havi")
        .to_string();

    // argv0 self-name service dispatch: if invoked as managed service name,
    // route directly through pylon dispatch.
    if pylon::catalog::service_from_argv0(&argv0).is_some() {
        env_logger::init();
        pylon::cli::main_with_argv(args);
        return;
    }

    // Pylon subcommand: `havi pylon [args...]`
    if args.get(1).map(|s| s.as_str()) == Some("pylon") {
        env_logger::init();
        pylon::cli::main(args[2..].to_vec());
        return;
    }

    // Self-exec service dispatch compatibility:
    // pylon self-exec mode can launch current_exe as:
    //   havi --embedded-services exec <service> ...
    // Route that argv shape into pylon CLI dispatch instead of launching UI.
    if args.get(1).map(|s| s.as_str()) == Some("--embedded-services")
        && args.get(2).map(|s| s.as_str()) == Some("exec")
    {
        env_logger::init();
        pylon::cli::main(args[1..].to_vec());
        return;
    }

    // Extract HAVI runtime flags and set env vars for downstream use.
    {
        let mut i = 1;
        let mut force_no_pylon = false;
        let mut force_external_pylon = false;

        while i < args.len() {
            match args[i].as_str() {
                "--path" => {
                    if let Some(v) = args.get(i + 1) {
                        std::env::set_var("HAVI_CONFIG", v);
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
                "--no-pylon" => {
                    force_no_pylon = true;
                    i += 1;
                },
                "--external-pylon" => {
                    force_external_pylon = true;
                    i += 1;
                },
                _ => i += 1,
            }
        }

        let pylon_mode = if force_no_pylon {
            "none"
        } else if force_external_pylon {
            "external"
        } else if cfg!(feature = "embedded-services") {
            "embedded"
        } else {
            "external"
        };
        std::env::set_var("HAVI_PYLON_MODE", pylon_mode);
    }

    // Single-instance check: if another HAVI is running, ask it to open a new tab.
    let url = std::env::var("HAVI_URL").unwrap_or_else(|_| "hppr://u/web/index.html".to_string());
    if havi_protocols::instance::try_send_open(&url).is_ok() {
        eprintln!("[havi] Sent open command to running instance");
        return;
    }

    havishell::app::app_main()
}
