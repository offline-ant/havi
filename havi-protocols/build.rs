/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

use std::path::Path;
use std::process::Command;

/// Generate TypeScript definitions from WebIDL.
///
/// Runs gen-havi-dts.mjs which reads HAVI WebIDL files and emits
/// hppr-html.d.ts and havi.d.ts.
fn generate_havi_typings(manifest_dir: &Path, js_dir: &Path) {
    let script = js_dir.join("gen-havi-dts.mjs");
    println!("cargo:rerun-if-changed={}", script.display());

    // Track HAVI-specific IDL inputs consumed by the generator.
    let webidl_dir = manifest_dir.join("../components/script_bindings/webidls");
    let idl_inputs = [
        "HpprClient.webidl",
        "EnvelopeHpprClient.webidl",
        "HpprPacket.webidl",
        "WatchSocket.webidl",
        "StreamIn.webidl",
        "StreamOut.webidl",
        "URC.webidl",
        "Address.webidl",
        "HpprResult.webidl",
        "HpprError.webidl",
        "HpprRepoInfo.webidl",
        "H3.webidl",
        "Window.webidl",
        "Document.webidl",
    ];
    for file in idl_inputs {
        println!(
            "cargo:rerun-if-changed={}",
            webidl_dir.join(file).display()
        );
    }

    let status = Command::new("bun")
        .arg(&script)
        .current_dir(manifest_dir)
        .status();

    match status {
        Ok(s) if s.success() => {},
        Ok(_) => {
            panic!("failed to generate havi.d.ts from WebIDL");
        },
        Err(e) => {
            println!(
                "cargo:warning=bun not found ({}), skipping HAVI typings generation",
                e
            );
        },
    }
}

/// Type-check protocol page scripts against generated HAVI typings.
///
/// Each .js file in src/js/ is checked in isolation with strict TypeScript
/// settings via `bun x tsc`.
fn check_page_scripts(js_dir: &Path) {
    for entry in std::fs::read_dir(js_dir).unwrap() {
        let entry = entry.unwrap();
        let path = entry.path();
        if path.extension().is_some_and(|e| e == "js" || e == "ts") {
            println!("cargo:rerun-if-changed={}", path.display());
        }
    }

    let hppr_html_dts = js_dir.join("hppr-html.d.ts");
    let havi_dts = js_dir.join("havi.d.ts");
    let mut errors = 0;
    for entry in std::fs::read_dir(js_dir).unwrap() {
        let entry = entry.unwrap();
        let path = entry.path();
        if path.extension().map_or(true, |e| e != "js") {
            continue;
        }
        let status = Command::new("bun")
            .args([
                "x",
                "tsc",
                "--noEmit",
                "--strict",
                "--target",
                "ES2022",
                "--lib",
                "ES2022,DOM",
                "--checkJs",
                "--allowJs",
            ])
            .arg(&hppr_html_dts)
            .arg(&havi_dts)
            .arg(&path)
            .status();
        match status {
            Ok(s) if s.success() => {},
            Ok(_) => {
                errors += 1;
                println!(
                    "cargo:warning=bun x tsc type-check failed: {}",
                    path.file_name().unwrap().to_string_lossy()
                );
            },
            Err(e) => {
                println!(
                    "cargo:warning=bun x tsc not available ({}), skipping page JS type-check",
                    e
                );
                return;
            },
        }
    }
    if errors > 0 {
        panic!(
            "{} page script(s) failed type-check. Run: bash {}",
            errors,
            js_dir.join("check.sh").display()
        );
    }
}

fn main() {
    let manifest_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    let js_dir = manifest_dir.join("src/js");
    if js_dir.exists() {
        generate_havi_typings(manifest_dir, &js_dir);
        check_page_scripts(&js_dir);
    }
}
