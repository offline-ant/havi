from __future__ import annotations

import json
import os
import pathlib
import subprocess
import sys
import threading
from typing import Any


# Import generic Makepad control library.
_tools_dir = os.path.join(os.path.dirname(os.path.abspath(__file__)),
                          "..", "..", "..", "makepad", "tools")
sys.path.insert(0, _tools_dir)
import makepad_control as mc  # type: ignore[import-not-found]


def _log(action: str, *, cmd: list[str] | None = None) -> None:
    print(f"# [mach-havi] {action}")
    if cmd:
        print(f"#   $ {' '.join(cmd)}")


def run_studio(
    havi_root: pathlib.Path,
    makepad_root: pathlib.Path,
    env: dict[str, str],
    extra: list[str] | None = None,
) -> int:
    """Launch Makepad Studio with the havi workspace as a root."""
    cmd = [
        "cargo", "run", "-p", "makepad-studio",
        "--release", "--",
        f"--root=havi:{havi_root}",
    ]
    if extra:
        cmd.extend(extra)
    _log("studio", cmd=cmd)
    return subprocess.call(cmd, env=env, cwd=str(makepad_root))


def run_desktop_makepad_socket(
    cmd: list[str],
    env: dict[str, str],
    havi_root: pathlib.Path,
) -> int:
    """Launch desktop HAVI with automatic Makepad socket relay.

    The desktop launcher always exposes the Makepad socket. This helper owns
    socket-path selection, enables stdin/stdout event injection, and relays
    the stream over a Unix domain socket.
    """
    import tempfile

    sock_path = os.path.join(tempfile.gettempdir(), f"havi-makepad-{os.getpid()}.sock")

    # Tell HAVI to use stdin/stdout event injection mode.
    env["HAVI_MAKEPAD_EVENTS"] = "1"
    env["HAVI_MAKEPAD_SOCKET"] = sock_path
    env["HAVI_INCLUDE_STATE"] = "HAVI_MAKEPAD_SOCKET"

    print(f"HAVI_MAKEPAD_SOCKET={sock_path}")
    sys.stdout.flush()

    _log("desktop run", cmd=cmd)

    # HAVI-specific state collected during startup.
    devtools_addr: list[str] = []
    pylon_bind: list[str] = []
    ready_event = threading.Event()

    def on_nonjson_line(line: str) -> None:
        stripped = line.strip()
        if stripped.startswith("HAVI_DEVTOOLS=") and not devtools_addr:
            devtools_addr.append(stripped.split("=", 1)[1])
        if stripped.startswith("PYLON_BIND=") and not pylon_bind:
            pylon_bind.append(stripped.split("=", 1)[1])
        print(line, file=sys.stderr)

    def on_ready() -> None:
        dt = devtools_addr[0] if devtools_addr else ""
        dt_port = dt.rsplit(":", 1)[-1] if dt else ""
        print(f"# havi-makepad-cli --socket {sock_path} screenshot /tmp/test.png")
        if dt_port:
            print(f"# havi-devtools-cli -p {dt_port} eval 'document.title'")

    def on_json_line(msg: Any) -> None:
        if isinstance(msg, dict) and "ReadyToStart" in msg and not ready_event.is_set():
            ready_event.set()
            on_ready()

    return mc.run_relay(
        cmd, sock_path,
        env=env,
        on_json_line=on_json_line,
        on_nonjson_line=on_nonjson_line,
        ready_event=ready_event,
    )
