fn main() {
    // Handle --version before Makepad takes over args.
    if std::env::args().any(|a| a == "--version") {
        println!(
            "havi {} (havishell)",
            env!("CARGO_PKG_VERSION"),
        );
        return;
    }
    havishell::app::app_main()
}
