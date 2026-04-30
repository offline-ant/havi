include!(concat!(env!("OUT_DIR"), "/build_id.rs"));

fn havi_socket_path() -> std::path::PathBuf {
    let app_name = "havi";

    #[cfg(target_os = "macos")]
    {
        if let Some(home) = std::env::var_os("HOME") {
            return std::path::PathBuf::from(home)
                .join("Library")
                .join("Application Support")
                .join(app_name)
                .join(format!("app-{}.sock", BUILD_ID));
        }
    }

    #[cfg(all(unix, not(target_os = "macos")))]
    {
        if let Ok(xdg) = std::env::var("XDG_RUNTIME_DIR") {
            if !xdg.is_empty() {
                return std::path::PathBuf::from(xdg)
                    .join(app_name)
                    .join(format!("app-{}.sock", BUILD_ID));
            }
        }
        if let Some(home) = std::env::var_os("HOME") {
            return std::path::PathBuf::from(home)
                .join(".local")
                .join("state")
                .join(app_name)
                .join(format!("app-{}.sock", BUILD_ID));
        }
    }

    #[cfg(windows)]
    {
        return std::env::temp_dir().join(format!("makepad-dev.makepad.havi-{}.port", BUILD_ID));
    }

    std::env::temp_dir().join(format!("dev.makepad.havi-{}.sock", BUILD_ID))
}

fn havi_state_file_path() -> std::path::PathBuf {
    std::path::PathBuf::from(format!("{}.state", havi_socket_path().display()))
}

fn main() {
    let args: Vec<String> = std::env::args().collect();

    // Handle --version before Makepad takes over args.
    if args.iter().any(|a| a == "--version") {
        println!("havi {} (havishell)", env!("CARGO_PKG_VERSION"),);
        return;
    }

    // Extract HAVI runtime flags and set env vars for downstream use.
    {
        let mut i = 1;
        let mut screenshot_path: Option<String> = None;

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
                "--screenshot" => {
                    if let Some(v) = args.get(i + 1) {
                        screenshot_path = Some(v.clone());
                        i += 2;
                    } else {
                        eprintln!("missing value for --screenshot <output.png>");
                        std::process::exit(2);
                    }
                },
                _ => i += 1,
            }
        }

        if let Some(path) = screenshot_path {
            std::env::set_var("HAVI_SCREENSHOT", path);
        }

    }

    let screenshot_mode = std::env::var("HAVI_SCREENSHOT")
        .ok()
        .filter(|path| !path.is_empty())
        .is_some();
    let startup_url = std::env::var("HAVI_URL").ok().filter(|url| !url.is_empty());
    let items: Vec<&str> = startup_url.iter().map(|url| url.as_str()).collect();
    if !screenshot_mode {
        match havishell::makepad_widgets::makepad_platform::Cx::enable_single_instance_with_build(
            "dev.makepad.havi",
            BUILD_ID,
            &items,
        ) {
            havishell::makepad_widgets::makepad_platform::SingleInstanceResult::Secondary => {
                if let Ok(state) = std::fs::read_to_string(havi_state_file_path()) {
                    print!("{}", state);
                }
                return;
            },
            havishell::makepad_widgets::makepad_platform::SingleInstanceResult::DifferentBuild => {
                eprintln!(
                    "another HAVI instance is already running from a different build; refusing to start"
                );
                std::process::exit(1);
            },
            havishell::makepad_widgets::makepad_platform::SingleInstanceResult::Primary => {}
        }
    }

    havishell::app::app_main()
}
