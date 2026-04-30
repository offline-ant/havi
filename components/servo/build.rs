/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use chrono::Local;

fn generate_havi_typings(protocol_root: &Path, js_dir: &Path, webidl_dir: &Path) {
    let script = js_dir.join("gen-havi-dts.mjs");
    println!("cargo:rerun-if-changed={}", script.display());

    let idl_inputs = [
        "HpprClient.webidl",
        "EnvelopeHpprClient.webidl",
        "HpprPacket.webidl",
        "HpprResolveResult.webidl",
        "HpprSource.webidl",
        "WatchSocket.webidl",
        "StreamPub.webidl",
        "StreamSub.webidl",
        "URC.webidl",
        "Address.webidl",
        "WindowAddress.webidl",
        "HpprWindowAddress.webidl",
        "FileWindowAddress.webidl",
        "HpprResult.webidl",
        "HpprError.webidl",
        "H3.webidl",
        "Window.webidl",
        "Document.webidl",
    ];
    for file in idl_inputs {
        println!("cargo:rerun-if-changed={}", webidl_dir.join(file).display());
    }

    let status = Command::new("bun")
        .arg(&script)
        .current_dir(protocol_root)
        .status();

    match status {
        Ok(s) if s.success() => {},
        Ok(_) => panic!("failed to generate havi.d.ts from WebIDL"),
        Err(e) => {
            println!(
                "cargo:warning=bun not found ({}), skipping HAVI typings generation",
                e
            );
        },
    }
}

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

fn write_build_id() {
    let path = Path::new(&env::var_os("OUT_DIR").unwrap()).join("build_id.rs");
    fs::write(
        path,
        format!(
            "const BUILD_ID: &str = \"{}\";",
            Local::now().format("%Y%m%d%H%M%S")
        ),
    )
    .unwrap();
}

fn rewrite_script_binding_paths(contents: String) -> String {
    contents
        .replace("crate::dom::", "crate::script::dom::")
        .replace("use crate::*;", "use crate::script::*;")
}

fn copy_script_binding_file(from: PathBuf, to: PathBuf) {
    let contents = std::fs::read_to_string(&from).unwrap();
    std::fs::write(to, rewrite_script_binding_paths(contents)).unwrap();
}

fn copy_script_bindings_outputs() {
    let script_bindings_out_dir =
        PathBuf::from(env::var_os("DEP_SCRIPT_BINDINGS_CRATE_OUT_DIR").unwrap());
    let out_dir = PathBuf::from(env::var_os("OUT_DIR").unwrap());

    [
        "InterfaceTypes.rs",
        "DomTypeHolder.rs",
        "InterfaceObjectMap.rs",
        "ConcreteInheritTypes.rs",
        "UnionTypes.rs",
        "InterfaceObjectMapPhf.rs",
    ]
    .iter()
    .map(Path::new)
    .for_each(|file| {
        let from = script_bindings_out_dir.join(file);
        let to = out_dir.join(file.file_name().unwrap());
        println!("cargo::rerun-if-changed={}", from.display());
        copy_script_binding_file(from, to);
    });

    let _ = std::fs::create_dir(out_dir.join("ConcreteBindings"));
    let script_concrete_bindings_out_dir = script_bindings_out_dir.join("ConcreteBindings");
    println!(
        "cargo::rerun-if-changed={}",
        script_concrete_bindings_out_dir.display()
    );
    std::fs::read_dir(script_concrete_bindings_out_dir)
        .unwrap()
        .filter_map(|res| res.map(|e| e.path()).ok())
        .filter(|path| path.is_file())
        .for_each(|file| {
            copy_script_binding_file(
                file.clone(),
                out_dir
                    .join("ConcreteBindings")
                    .join(file.file_name().unwrap()),
            );
        });
}

fn main() {
    write_build_id();
    copy_script_bindings_outputs();

    let manifest_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    let js_dir = manifest_dir.join("js");
    let webidl_dir = manifest_dir.join("../script_bindings/webidls");

    if js_dir.exists() {
        generate_havi_typings(manifest_dir, &js_dir, &webidl_dir);
        check_page_scripts(&js_dir);
    }
}
