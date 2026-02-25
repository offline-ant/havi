use std::env;
use std::fs::{self, File};
use std::io;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::thread;
use std::time::{Duration, Instant};

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

    let manifest_dir = PathBuf::from(env::var("CARGO_MANIFEST_DIR").unwrap());
    let svg = manifest_dir.join("../../../hppr/logo.svg");
    let resources_dir = manifest_dir.join("../../resources");
    if let Err(err) = fs::create_dir_all(&resources_dir) {
        println!(
            "cargo:warning=havishell icon raster skipped: cannot create resources dir {}: {}",
            resources_dir.display(),
            err
        );
    }

    match find_debug_havi(&manifest_dir) {
        Some(havi_bin) => {
            if let Err(err) = raster_with_havi_devtools(&manifest_dir, &havi_bin, &svg, &resources_dir) {
                println!(
                    "cargo:warning=havishell icon raster skipped ({}): {}",
                    havi_bin.display(),
                    err
                );
                ensure_placeholder_icons(&resources_dir);
            }
        },
        None => {
            println!(
                "cargo:warning=havishell icon raster skipped: debug HAVI binary not found at {}",
                manifest_dir.join("../../target/debug/havi").display()
            );
            ensure_placeholder_icons(&resources_dir);
        },
    }

    println!("cargo:rerun-if-changed={}", svg.display());
}

fn find_debug_havi(manifest_dir: &Path) -> Option<PathBuf> {
    let path = manifest_dir.join("../../target/debug/havi");
    path.exists().then_some(path)
}

fn raster_with_havi_devtools(
    manifest_dir: &Path,
    havi_bin: &Path,
    svg_path: &Path,
    out_dir: &Path,
) -> Result<(), String> {
    let devtools_cli = manifest_dir.join("../../havi-devtools-cli");
    if !devtools_cli.exists() {
        return Err(format!("missing havi-devtools-cli at {}", devtools_cli.display()));
    }

    if let Ok(bind) = env::var("HAVI_DEVTOOLS") {
        let port = parse_devtools_port(&bind)?;
        for size in [64, 128, 1024] {
            let data_uri = eval_svg_to_png_data_uri(&devtools_cli, port, size)?;
            let png = decode_png_data_uri(&data_uri)?;
            let out_png = out_dir.join(format!("havi_icon_{}.png", size));
            fs::write(&out_png, png)
                .map_err(|e| format!("write {}: {e}", out_png.display()))?;
        }
        return Ok(());
    }

    let svg_abs = svg_path
        .canonicalize()
        .map_err(|e| format!("canonicalize {}: {e}", svg_path.display()))?;
    let svg_url = format!("file://{}", svg_abs.display());

    let run_dir = out_dir.join("icon-raster-runtime");
    fs::create_dir_all(&run_dir)
        .map_err(|e| format!("create runtime dir {}: {e}", run_dir.display()))?;

    let port = pick_free_port().map_err(|e| format!("pick free port: {e}"))?;
    let havi_log = run_dir.join("havi.log");
    let log_file = File::create(&havi_log)
        .map_err(|e| format!("create log {}: {e}", havi_log.display()))?;
    let log_file_err = log_file
        .try_clone()
        .map_err(|e| format!("clone log fd {}: {e}", havi_log.display()))?;

    let havi_root = manifest_dir.join("../..");
    let mut havi = Command::new(havi_bin)
        .current_dir(&havi_root)
        .env("HAVI_URL", svg_url)
        .env("HAVI_DEVTOOLS", format!("127.0.0.1:{port}"))
        .env("HAVI_CONFIG", run_dir.join("config"))
        .env("HAVI_HOME", "tcp+127.0.0.1:4777")
        .env("NO_PYLON", "1")
        .stdout(Stdio::from(log_file))
        .stderr(Stdio::from(log_file_err))
        .spawn()
        .map_err(|e| format!("spawn {}: {e}", havi_bin.display()))?;

    let result = (|| -> Result<(), String> {
        wait_for_devtools(&devtools_cli, port, 60, &mut havi, &havi_log)?;

        for size in [64, 128, 1024] {
            let data_uri = eval_svg_to_png_data_uri(&devtools_cli, port, size)?;
            let png = decode_png_data_uri(&data_uri)?;
            let out_png = out_dir.join(format!("havi_icon_{}.png", size));
            fs::write(&out_png, png)
                .map_err(|e| format!("write {}: {e}", out_png.display()))?;
        }
        Ok(())
    })();

    kill_child(&mut havi);
    result
}

fn wait_for_devtools(
    devtools_cli: &Path,
    port: u16,
    timeout_secs: u64,
    havi_child: &mut Child,
    havi_log: &Path,
) -> Result<(), String> {
    let deadline = Instant::now() + Duration::from_secs(timeout_secs);
    loop {
        if let Some(status) = havi_child
            .try_wait()
            .map_err(|e| format!("poll havi process: {e}"))?
        {
            let log_tail = tail_file(havi_log, 4000);
            return Err(format!(
                "havi exited before devtools became ready on port {port} (status: {status})\n{}",
                log_tail
            ));
        }

        if Instant::now() >= deadline {
            return Err(format!("devtools did not become ready on port {port}"));
        }

        let output = Command::new(devtools_cli)
            .args([
                "-p",
                &port.to_string(),
                "--timeout",
                "2",
                "--text",
                "eval",
                "true",
            ])
            .output();

        if let Ok(out) = output {
            if out.status.success() {
                return Ok(());
            }
        }

        thread::sleep(Duration::from_millis(300));
    }
}

fn eval_svg_to_png_data_uri(devtools_cli: &Path, port: u16, size: u32) -> Result<String, String> {
    let js = format!(
        "(async () => {{\n  const img = document.querySelector('img');\n  if (!img) throw new Error('no <img> found');\n  for (let i = 0; i < 200 && !(img.complete && img.naturalWidth > 0 && img.naturalHeight > 0); i++) {{\n    await new Promise(r => setTimeout(r, 50));\n  }}\n  if (!(img.complete && img.naturalWidth > 0 && img.naturalHeight > 0)) {{\n    throw new Error('svg image did not load');\n  }}\n  const c = document.createElement('canvas');\n  c.width = {size};\n  c.height = {size};\n  const ctx = c.getContext('2d');\n  ctx.clearRect(0, 0, c.width, c.height);\n  ctx.drawImage(img, 0, 0, c.width, c.height);\n  const data = ctx.getImageData(0, 0, c.width, c.height).data;\n  let nonTransparent = 0;\n  let nonGray = 0;\n  for (let i = 0; i < data.length; i += 4) {{\n    const r = data[i + 0], g = data[i + 1], b = data[i + 2], a = data[i + 3];\n    if (a !== 0) nonTransparent++;\n    if (!(r === g && g === b)) nonGray++;\n  }}\n  if (nonTransparent === 0) throw new Error('svg rasterization returned fully transparent result');\n  if (nonGray === 0) throw new Error('svg rasterization returned grayscale output (likely SVG drawImage limitation)');\n  return c.toDataURL('image/png');\n}})()"
    );

    let out = Command::new(devtools_cli)
        .args([
            "-p",
            &port.to_string(),
            "--timeout",
            "20",
            "--text",
            "eval",
            "--await",
            &js,
        ])
        .output()
        .map_err(|e| format!("run {} eval: {e}", devtools_cli.display()))?;

    if !out.status.success() {
        let stderr = String::from_utf8(out.stderr)
            .map_err(|e| format!("devtools eval stderr not utf8: {e}"))?;
        return Err(format!("devtools eval failed: {stderr}"));
    }

    let stdout =
        String::from_utf8(out.stdout).map_err(|e| format!("devtools eval stdout not utf8: {e}"))?;
    Ok(stdout.trim().to_string())
}

fn decode_png_data_uri(data_uri: &str) -> Result<Vec<u8>, String> {
    let prefix = "data:image/png;base64,";
    let b64 = data_uri
        .strip_prefix(prefix)
        .ok_or_else(|| format!("unexpected data URI prefix: {data_uri}"))?;
    decode_base64(b64)
}

fn decode_base64(input: &str) -> Result<Vec<u8>, String> {
    fn val(c: u8) -> Option<u8> {
        match c {
            b'A'..=b'Z' => Some(c - b'A'),
            b'a'..=b'z' => Some(c - b'a' + 26),
            b'0'..=b'9' => Some(c - b'0' + 52),
            b'+' => Some(62),
            b'/' => Some(63),
            _ => None,
        }
    }

    let bytes = input.as_bytes();
    if bytes.len() % 4 != 0 {
        return Err("invalid base64 length".to_string());
    }

    let mut out = Vec::with_capacity(bytes.len() / 4 * 3);
    let mut i = 0;
    while i < bytes.len() {
        let c0 = bytes[i];
        let c1 = bytes[i + 1];
        let c2 = bytes[i + 2];
        let c3 = bytes[i + 3];

        let v0 = val(c0).ok_or_else(|| format!("invalid base64 byte: {}", c0))?;
        let v1 = val(c1).ok_or_else(|| format!("invalid base64 byte: {}", c1))?;

        let v2 = if c2 == b'=' {
            0
        } else {
            val(c2).ok_or_else(|| format!("invalid base64 byte: {}", c2))?
        };
        let v3 = if c3 == b'=' {
            0
        } else {
            val(c3).ok_or_else(|| format!("invalid base64 byte: {}", c3))?
        };

        let n = ((v0 as u32) << 18) | ((v1 as u32) << 12) | ((v2 as u32) << 6) | (v3 as u32);
        out.push(((n >> 16) & 0xFF) as u8);
        if c2 != b'=' {
            out.push(((n >> 8) & 0xFF) as u8);
        }
        if c3 != b'=' {
            out.push((n & 0xFF) as u8);
        }

        i += 4;
    }

    Ok(out)
}

fn pick_free_port() -> io::Result<u16> {
    let listener = std::net::TcpListener::bind("127.0.0.1:0")?;
    let port = listener.local_addr()?.port();
    drop(listener);
    Ok(port)
}

fn kill_child(child: &mut Child) {
    let _ = child.kill();
    let _ = child.wait();
}

fn parse_devtools_port(bind: &str) -> Result<u16, String> {
    bind.rsplit(':')
        .next()
        .ok_or_else(|| format!("invalid HAVI_DEVTOOLS value: {bind}"))?
        .parse::<u16>()
        .map_err(|e| format!("invalid HAVI_DEVTOOLS port in {bind}: {e}"))
}

fn ensure_placeholder_icons(resources_dir: &Path) {
    for size in [64, 128, 1024] {
        let path = resources_dir.join(format!("havi_icon_{}.png", size));
        if path.exists() {
            continue;
        }
        if let Err(err) = fs::write(&path, []) {
            println!(
                "cargo:warning=havishell icon placeholder create failed {}: {}",
                path.display(),
                err
            );
        }
    }
}

fn tail_file(path: &Path, max_bytes: usize) -> String {
    match fs::read(path) {
        Ok(bytes) => {
            let start = bytes.len().saturating_sub(max_bytes);
            match String::from_utf8(bytes[start..].to_vec()) {
                Ok(s) => format!("havi log tail:\n{s}"),
                Err(e) => format!("havi log is not utf8: {e}"),
            }
        },
        Err(e) => format!("failed reading havi log {}: {e}", path.display()),
    }
}
