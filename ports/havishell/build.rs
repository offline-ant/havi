use std::env;
use std::fs;
use std::path::PathBuf;
use std::process::Command;

fn main() {
    let target_os = env::var("CARGO_CFG_TARGET_OS").unwrap_or_default();
    if target_os == "ios" {
        // libc++ __libcpp_verbose_abort is missing from iOS 15's
        // libc++.  Provide a fallback so a single binary runs on
        // iOS 15 through current.
        cc::Build::new()
            .cpp(true)
            .file("ios_compat_stubs.cpp")
            .compile("ios_compat_stubs");

        // Force flat namespace so that our __libcpp_verbose_abort stub
        // is found at runtime on iOS 15, where libc++.1.dylib lacks it.
        // With two-level namespaces (default), dyld only looks in the
        // recorded dylib and fails.  Flat namespace searches all loaded
        // images including the main binary.
        println!("cargo:rustc-link-arg=-Wl,-flat_namespace");
    }
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
