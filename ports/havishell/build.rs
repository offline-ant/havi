use std::env;
use std::fs;
use std::path::PathBuf;
use std::process::Command;

fn main() {
    let target_os = env::var("CARGO_CFG_TARGET_OS").unwrap_or_default();
    if target_os == "android" {
        let out_dir = PathBuf::from(env::var("OUT_DIR").unwrap());
        let stub_path = out_dir.join("libgcc.a");
        if !stub_path.exists() {
            let ar = env::var("AR")
                .or_else(|_| env::var("TARGET_AR"))
                .unwrap_or_else(|_| "llvm-ar".to_string());
            let status = Command::new(&ar)
                .args(["rc", stub_path.to_str().unwrap()])
                .status();
            match status {
                Ok(s) if s.success() => {},
                _ => {
                    fs::write(&stub_path, b"!<arch>\n").unwrap();
                },
            }
        }
        println!("cargo:rustc-link-search=native={}", out_dir.display());
    }
}
