use std::env;
use std::fs;
use std::path::PathBuf;
use std::process::Command;

fn main() {
    let target_os = env::var("CARGO_CFG_TARGET_OS").unwrap_or_default();
    if target_os == "ios" {
        // Compatibility stubs for iOS < 17:
        // - JIT W^X: SpiderMonkey calls BrowserEngineKit symbols; we
        //   provide them via pthread_jit_write_protect_np (iOS 14+).
        // - libc++ __libcpp_verbose_abort: missing from older libc++.
        //
        // Always compiled so a single binary runs on iOS 15 through
        // current.  On iOS 17+ the JIT stubs are harmless (same
        // underlying primitive) and the libc++ stub is unused (the
        // system dylib symbol wins via two-level namespacing).
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
