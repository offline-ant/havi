use hppr_client::{DEFAULT_PORT, TransportScheme, ViaSpec, parse_via};

const ENV_KEY: &str = "HAVI_REPO_TARGET";


pub fn get() -> ViaSpec {
    std::env::var(ENV_KEY)
        .ok()
        .and_then(|s| parse_via(&s).ok())
        .unwrap_or(ViaSpec::Net {
            host: "127.0.0.1".to_string(),
            port: DEFAULT_PORT,
            scheme: Some(TransportScheme::Tcp),
        })
}

pub fn endpoint() -> String {
    hppr_client::repo_endpoint_from(&get())
}
