#!/usr/bin/env bash
# surface-cache-presentation-text-test.sh - Verify exact-present browser surface caching preserves final text output.

set -euo pipefail

TEST_NAME="surface-cache-presentation-text"
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
HAVI_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"
HAVI_BIN="$HAVI_ROOT/target/debug/havi"

log() { echo "[$TEST_NAME] $*" >&2; }
fail() { echo "FAIL: $*" >&2; exit 1; }

pkill -f "$HAVI_BIN --foreground --no-pylon" 2>/dev/null || true

if [[ ! -x "$HAVI_BIN" ]]; then
    log "Building HAVI via ./mach-havi build..."
    (
        cd "$HAVI_ROOT"
        ./mach-havi build >/dev/null
    )
fi

HTML_FILE="$(mktemp /tmp/havi-surface-cache-text-XXXXXX.html)"
cat >"$HTML_FILE" <<'HTML'
<!doctype html>
<html>
<head>
  <meta charset="utf-8">
  <title>Surface Cache Text Presentation Test</title>
  <style>
    html, body {
      margin: 0;
      background: #ffffff;
      color: #000000;
      font-family: serif;
    }
    main {
      max-width: 720px;
      padding: 24px 28px 40px;
      line-height: 1.35;
    }
    h1 {
      font-size: 34px;
      margin: 0 0 12px;
      font-weight: 700;
    }
    h2 {
      font-size: 22px;
      margin: 20px 0 8px;
      font-weight: 700;
    }
    p {
      margin: 0 0 10px;
      font-size: 16px;
    }
    .small {
      font-size: 13px;
    }
    .large {
      font-size: 20px;
    }
    .mono {
      font-family: monospace;
      font-size: 15px;
    }
  </style>
</head>
<body>
<main>
  <h1>H I l m A V W exact surface cache text probe</h1>
  <p class="large">High-contrast stems and diagonals should stay crisp when the final browser surface is reused.</p>
  <h2>Paragraph block</h2>
  <p>Lorem ipsum dolor sit amet, consectetur adipiscing elit. Integer feugiat, velit vitae varius volutpat, nisl velit pretium libero, id hendrerit velit nunc et risus.</p>
  <p>Vivamus vehicula, massa id vulputate vulputate, metus justo vulputate velit, vel varius velit massa vitae velit. Aliquam erat volutpat. Vestibulum ante ipsum primis in faucibus.</p>
  <p>Heavy letter mix: HHHH IIII llll mmmm AVAV WWWW. Dense black text on white background exposes any extra resampling immediately.</p>
  <p class="small">Small text line: H I l m A V W 0123456789 repeated three times. H I l m A V W 0123456789 repeated three times.</p>
  <p class="mono">Monospace line: HIlmAVW -- exact-present cached surface must match live retained rendering byte for byte.</p>
  <p>Lorem ipsum dolor sit amet, consectetur adipiscing elit. Integer feugiat, velit vitae varius volutpat, nisl velit pretium libero, id hendrerit velit nunc et risus.</p>
  <p>Vivamus vehicula, massa id vulputate vulputate, metus justo vulputate velit, vel varius velit massa vitae velit. Aliquam erat volutpat. Vestibulum ante ipsum primis in faucibus.</p>
  <p>Heavy letter mix: HHHH IIII llll mmmm AVAV WWWW. Dense black text on white background exposes any extra resampling immediately.</p>
  <p class="small">Small text line: H I l m A V W 0123456789 repeated three times. H I l m A V W 0123456789 repeated three times.</p>
  <p class="mono">Monospace line: HIlmAVW -- exact-present cached surface must match live retained rendering byte for byte.</p>
  <p>Lorem ipsum dolor sit amet, consectetur adipiscing elit. Integer feugiat, velit vitae varius volutpat, nisl velit pretium libero, id hendrerit velit nunc et risus.</p>
  <p>Vivamus vehicula, massa id vulputate vulputate, metus justo vulputate velit, vel varius velit massa vitae velit. Aliquam erat volutpat. Vestibulum ante ipsum primis in faucibus.</p>
  <p>Heavy letter mix: HHHH IIII llll mmmm AVAV WWWW. Dense black text on white background exposes any extra resampling immediately.</p>
  <p class="small">Small text line: H I l m A V W 0123456789 repeated three times. H I l m A V W 0123456789 repeated three times.</p>
  <p class="mono">Monospace line: HIlmAVW -- exact-present cached surface must match live retained rendering byte for byte.</p>
</main>
</body>
</html>
HTML

python3 - "$HAVI_ROOT" "$HTML_FILE" <<'PY'
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
html_path = pathlib.Path(sys.argv[2])
havi_bin = havi_root / "target" / "debug" / "havi"


def stop_existing_havi() -> None:
    subprocess.run(
        ["pkill", "-f", f"{havi_bin} --foreground --no-pylon"],
        stderr=subprocess.DEVNULL,
        check=False,
    )
    time.sleep(1)


def wait_for_socket(log_path: pathlib.Path, proc: subprocess.Popen[str]) -> str:
    for _ in range(600):
        time.sleep(0.1)
        if log_path.exists():
            for line in log_path.read_text().splitlines():
                if line.startswith("HAVI_MAKEPAD_SOCKET="):
                    socket = line.split("=", 1)[1].strip()
                    if socket:
                        return socket
        if proc.poll() is not None:
            raise SystemExit(log_path.read_text())
    raise SystemExit("Timed out waiting for HAVI_MAKEPAD_SOCKET")


@dataclass
class RunArtifacts:
    log: pathlib.Path
    dump: pathlib.Path
    shot: pathlib.Path


def run_case(mode: str) -> RunArtifacts:
    stop_existing_havi()
    log = pathlib.Path(tempfile.mktemp(prefix=f"havi-surface-cache-{mode}-", suffix=".log"))
    dump = pathlib.Path(tempfile.mktemp(prefix=f"havi-surface-cache-{mode}-", suffix=".dump"))
    shot = pathlib.Path(tempfile.mktemp(prefix=f"havi-surface-cache-{mode}-", suffix=".png"))
    if shot.exists():
        shot.unlink()
    config_dir = tempfile.mkdtemp(prefix=f"havi-surface-cache-{mode}-cfg-")
    extra_env = [] if mode == "on" else ["HAVI_BROWSER_SURFACE_CACHE=0"]
    shell_cmd = f"""cd {havi_root} && setsid env HAVI_CONFIG={config_dir} HAVI_URL=file://{html_path} HAVI_RENDER_STATS=1 {' '.join(extra_env)} python3 -u - <<'INNER'
import os, pathlib, sys
havi_root = pathlib.Path.cwd()
sys.path.insert(0, str(havi_root / 'ports' / 'havishell'))
from mach_havi_studio import run_desktop_makepad_socket
cmd = [str(havi_root / 'target' / 'debug' / 'havi'), '--foreground', '--no-pylon']
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
    socket = wait_for_socket(log, proc)
    for _ in range(300):
        if pathlib.Path(socket).exists():
            break
        time.sleep(0.1)
    else:
        raise SystemExit("Timed out waiting for Makepad socket")

    for _ in range(300):
        with dump.open("w") as fh:
            result = subprocess.run(
                [str(havi_root / "havi-makepad-cli"), "--socket", socket, "dump"],
                stdout=fh,
                stderr=subprocess.DEVNULL,
                check=False,
                text=True,
            )
        if result.returncode == 0 and " web_view " in dump.read_text():
            break
        if proc.poll() is not None:
            raise SystemExit(log.read_text())
        time.sleep(0.1)
    else:
        raise SystemExit("Timed out waiting for web_view widget dump")

    if mode == "on":
        for _ in range(300):
            log_text = log.read_text()
            if (
                "browser_surface_cache event=reuse" in log_text
                and "browser_surface_cache event=copy" in log_text
            ):
                break
            if proc.poll() is not None:
                raise SystemExit(log.read_text())
            time.sleep(0.1)
        else:
            raise SystemExit("Did not observe browser surface cache reuse+copy in mode=on")
    else:
        time.sleep(1)

    with dump.open("w") as fh:
        subprocess.run(
            [str(havi_root / "havi-makepad-cli"), "--socket", socket, "dump"],
            stdout=fh,
            check=True,
            text=True,
        )
    subprocess.run(
        [str(havi_root / "havi-makepad-cli"), "--socket", socket, "screenshot", str(shot)],
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
    raise SystemExit("final presented browser output differs between cache-on and cache-off runs")
PY

rm -f "$HTML_FILE"
log "PASS"
