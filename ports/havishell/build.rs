use std::env;
use std::fs;
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

fn main() {
    let out = Path::new(&env::var_os("OUT_DIR").unwrap()).join("build_id.rs");
    let build_id = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system clock before unix epoch")
        .as_secs()
        .to_string();
    fs::write(out, format!("pub const BUILD_ID: &str = \"{}\";\n", build_id)).unwrap();
}
