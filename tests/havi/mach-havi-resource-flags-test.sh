#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
HAVI_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"
MAKEPAD_ROOT="$(cd "$HAVI_ROOT/../makepad" && pwd)"

python3 - <<'PY' "$HAVI_ROOT" "$MAKEPAD_ROOT"
import pathlib
import sys

havi_root = pathlib.Path(sys.argv[1])
makepad_root = pathlib.Path(sys.argv[2])
ports_dir = havi_root / "ports" / "havishell"
sys.path.insert(0, str(ports_dir))

import mach_havi_main as m

m.HAVI_ROOT = havi_root
m.HAVISHELL_DIR = ports_dir
m.MAKEPAD_ROOT = makepad_root
m.CARGO_MAKEPAD_DIR = makepad_root / "tools" / "cargo_makepad"

for builder in (
    lambda: m._cargo_makepad_desktop_cmd(),
    lambda: m._cargo_makepad_android_cmd("aarch64"),
    lambda: m._cargo_makepad_ios_cmd(),
):
    cmd = builder()
    assert "--embed-resources" in cmd, cmd
    assert "--small-fonts" in cmd, cmd

print("mach-havi resource flags ok")
PY
