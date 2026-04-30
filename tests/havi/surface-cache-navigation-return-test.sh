#!/usr/bin/env bash
# surface-cache-navigation-return-test.sh - Verify browser surface cache survives A->B->A navigation without presenting stale content.

set -euo pipefail

TEST_NAME="surface-cache-navigation-return"
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
source "$SCRIPT_DIR/havi-build.bash"

log() { echo "[$TEST_NAME] $*" >&2; }
fail() { echo "FAIL: $*" >&2; exit 1; }

pkill -f "$HAVI_BIN --foreground" 2>/dev/null || true

ensure_havi_built

PAGE_A="$(mktemp /tmp/havi-surface-cache-nav-a-XXXXXX.html)"
PAGE_B="$(mktemp /tmp/havi-surface-cache-nav-b-XXXXXX.html)"
cat >"$PAGE_A" <<'HTML'
<!doctype html>
<html>
<head>
  <meta charset="utf-8">
  <title>surface-cache-nav-a</title>
  <style>
    html, body {
      margin: 0;
      width: 100%;
      height: 100%;
      background: #b42020;
      color: #ffffff;
      font-family: system-ui, sans-serif;
    }
    body {
      display: grid;
      place-items: center;
    }
    main {
      text-align: center;
      border: 8px solid rgba(255,255,255,0.9);
      padding: 32px 48px;
      background: rgba(0,0,0,0.08);
      box-shadow: 0 10px 40px rgba(0,0,0,0.25);
    }
    h1 { font-size: 72px; margin: 0 0 16px; }
    p { font-size: 28px; margin: 0; }
  </style>
</head>
<body>
  <main>
    <h1>PAGE A</h1>
    <p>surface cache return probe A</p>
  </main>
</body>
</html>
HTML
cat >"$PAGE_B" <<'HTML'
<!doctype html>
<html>
<head>
  <meta charset="utf-8">
  <title>surface-cache-nav-b</title>
  <style>
    html, body {
      margin: 0;
      width: 100%;
      height: 100%;
      background: #1e389f;
      color: #f5f86a;
      font-family: system-ui, sans-serif;
    }
    body {
      display: grid;
      place-items: center;
    }
    main {
      text-align: center;
      border: 8px solid rgba(245,248,106,0.95);
      padding: 32px 48px;
      background: rgba(0,0,0,0.12);
      box-shadow: 0 10px 40px rgba(0,0,0,0.25);
    }
    h1 { font-size: 72px; margin: 0 0 16px; }
    p { font-size: 28px; margin: 0; }
  </style>
</head>
<body>
  <main>
    <h1>PAGE B</h1>
    <p>surface cache return probe B</p>
  </main>
</body>
</html>
HTML

python3 - "$HAVI_ROOT" "$PAGE_A" "$PAGE_B" <<'PY'
import os
import pathlib
import signal
import subprocess
import sys
import tempfile
import time
from dataclasses import dataclass
from PIL import Image, ImageChops

havi_root = pathlib.Path(sys.argv[1])
page_a = pathlib.Path(sys.argv[2])
page_b = pathlib.Path(sys.argv[3])
havi_bin = havi_root / "target" / "debug" / "havi"
makepad_cli = havi_root / "havi-makepad-cli"
devtools_cli = havi_root / "havi-devtools-cli"


def stop_existing_havi() -> None:
    subprocess.run(
        ["pkill", "-f", f"{havi_bin} --foreground"],
        stderr=subprocess.DEVNULL,
        check=False,
    )
    time.sleep(1)


def wait_for_line(prefix: str, log_path: pathlib.Path, proc: subprocess.Popen[str]) -> str:
    for _ in range(600):
        time.sleep(0.1)
        if log_path.exists():
            for line in log_path.read_text().splitlines():
                if line.startswith(prefix):
                    value = line.split("=", 1)[1].strip()
                    if value:
                        return value
        if proc.poll() is not None:
            raise SystemExit(log_path.read_text())
    raise SystemExit(f"Timed out waiting for {prefix}")


@dataclass
class RunArtifacts:
    log: pathlib.Path
    dump: pathlib.Path
    shot: pathlib.Path


def wait_for_web_view(socket: str, dump_path: pathlib.Path, proc: subprocess.Popen[str], log_path: pathlib.Path) -> None:
    for _ in range(300):
        with dump_path.open("w") as fh:
            result = subprocess.run(
                [str(makepad_cli), "--socket", socket, "dump"],
                stdout=fh,
                stderr=subprocess.DEVNULL,
                check=False,
                text=True,
            )
        if result.returncode == 0 and " web_view " in dump_path.read_text():
            return
        if proc.poll() is not None:
            raise SystemExit(log_path.read_text())
        time.sleep(0.1)
    raise SystemExit("Timed out waiting for web_view widget dump")


def wait_for_cache_reuse(log_path: pathlib.Path, proc: subprocess.Popen[str]) -> None:
    for _ in range(300):
        if "browser_surface_cache event=reuse" in log_path.read_text():
            return
        if proc.poll() is not None:
            raise SystemExit(log_path.read_text())
        time.sleep(0.1)
    raise SystemExit("Did not observe browser surface cache reuse")


def wait_for_tab_url(port: str, url: str) -> None:
    for _ in range(200):
        out = subprocess.check_output(
            [str(devtools_cli), "-p", port, "tabs"],
            text=True,
        )
        if url in out:
            return
        time.sleep(0.1)
    raise SystemExit(f"Timed out waiting for tab url {url}")


def navigate(port: str, url: str) -> None:
    subprocess.run(
        [str(devtools_cli), "-p", port, "navigate", url],
        check=True,
        text=True,
    )
    wait_for_tab_url(port, url)
    time.sleep(0.5)


def run_case(mode: str) -> RunArtifacts:
    stop_existing_havi()
    log = pathlib.Path(tempfile.mktemp(prefix=f"havi-surface-cache-nav-{mode}-", suffix=".log"))
    dump = pathlib.Path(tempfile.mktemp(prefix=f"havi-surface-cache-nav-{mode}-", suffix=".dump"))
    shot = pathlib.Path(tempfile.mktemp(prefix=f"havi-surface-cache-nav-{mode}-", suffix=".png"))
    config_dir = tempfile.mkdtemp(prefix=f"havi-surface-cache-nav-{mode}-cfg-")
    extra_env = [] if mode == "on" else ["HAVI_BROWSER_SURFACE_CACHE=0"]
    shell_cmd = f"""cd {havi_root} && setsid env HAVI_CONFIG={config_dir} HAVI_DEVTOOLS=127.0.0.1:0 HAVI_URL=file://{page_a} HAVI_RENDER_STATS=1 {' '.join(extra_env)} python3 -u - <<'INNER'
import os, pathlib, sys
havi_root = pathlib.Path.cwd()
sys.path.insert(0, str(havi_root / 'ports' / 'havishell'))
from mach_havi_studio import run_desktop_makepad_socket
cmd = [str(havi_root / 'target' / 'debug' / 'havi'), '--foreground']
raise SystemExit(run_desktop_makepad_socket(cmd, os.environ.copy(), havi_root))
INNER"""
    with log.open("w") as fh:
        proc = subprocess.Popen(
            shell_cmd,
            shell=True,
            stdout=fh,
            stderr=subprocess.STDOUT,
            preexec_fn=os.setsid,
            text=True,
        )

    socket = wait_for_line("HAVI_MAKEPAD_SOCKET=", log, proc)
    port = wait_for_line("HAVI_DEVTOOLS=", log, proc).rsplit(":", 1)[1]
    for _ in range(300):
        if pathlib.Path(socket).exists():
            break
        time.sleep(0.1)
    else:
        raise SystemExit("Timed out waiting for Makepad socket")

    wait_for_web_view(socket, dump, proc, log)
    if mode == "on":
        wait_for_cache_reuse(log, proc)
    else:
        time.sleep(1)

    navigate(port, f"file://{page_b}")
    navigate(port, f"file://{page_a}")
    navigate(port, f"file://{page_b}")
    navigate(port, f"file://{page_a}")

    with dump.open("w") as fh:
        subprocess.run(
            [str(makepad_cli), "--socket", socket, "dump"],
            stdout=fh,
            check=True,
            text=True,
        )
    subprocess.run(
        [str(makepad_cli), "--socket", socket, "screenshot", str(shot)],
        check=True,
        text=True,
    )
    os.killpg(os.getpgid(proc.pid), signal.SIGTERM)
    time.sleep(1)
    stop_existing_havi()
    return RunArtifacts(log=log, dump=dump, shot=shot)


@dataclass
class Widget:
    x: float
    y: float
    w: float
    h: float


def read_web_view(path: pathlib.Path) -> Widget:
    for line in path.read_text().splitlines():
        parts = line.split()
        if len(parts) < 8 or parts[0] in {"O", "W"}:
            continue
        if parts[2] != "web_view" and parts[3] != "ServoWebView":
            continue
        return Widget(
            x=float(parts[4]),
            y=float(parts[5]),
            w=float(parts[6]),
            h=float(parts[7]),
        )
    raise SystemExit(f"missing web_view widget in {path}")


def crop(path: pathlib.Path, widget: Widget) -> Image.Image:
    img = Image.open(path).convert("RGBA")
    left = max(int(round(widget.x)), 0)
    top = max(int(round(widget.y)), 0)
    right = min(int(round(widget.x + widget.w)), img.width)
    bottom = min(int(round(widget.y + widget.h)), img.height)
    if right <= left or bottom <= top:
        raise SystemExit(f"invalid crop rect {left},{top}..{right},{bottom} for {path}")
    return img.crop((left, top, right, bottom))


on = run_case("on")
off = run_case("off")
widget_on = read_web_view(on.dump)
widget_off = read_web_view(off.dump)
crop_on = crop(on.shot, widget_on)
crop_off = crop(off.shot, widget_off)
if crop_on.size != crop_off.size:
    raise SystemExit(f"cropped browser sizes differ: {crop_on.size} vs {crop_off.size}")
if ImageChops.difference(crop_on, crop_off).getbbox() is not None:
    raise SystemExit("final presented browser output differs between cache-on and cache-off after A->B->A navigation")
PY

rm -f "$PAGE_A" "$PAGE_B"
log "PASS"
