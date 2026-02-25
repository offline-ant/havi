from __future__ import annotations

import json
import os
import pathlib
import subprocess
import sys
import threading
from typing import Any


def _log(action: str, *, cmd: list[str] | None = None) -> None:
    print(f"[mach-havi] {action}")
    if cmd:
        print(f"  $ {' '.join(cmd)}")


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
    """Launch HAVI with stdin/stdout piped, relay via Unix domain socket.

    Creates a Unix socket so havi-makepad-cli can connect. JSON lines from
    socket clients are forwarded to HAVI's stdin; JSON lines from HAVI's
    stdout are broadcast to all connected clients.
    """
    import select
    import socket as sock_mod
    import tempfile

    sock_path = os.path.join(tempfile.gettempdir(), f"havi-makepad-{os.getpid()}.sock")

    # Clean up stale socket file.
    try:
        os.unlink(sock_path)
    except FileNotFoundError:
        pass

    # Create the Unix domain socket.
    server = sock_mod.socket(sock_mod.AF_UNIX, sock_mod.SOCK_STREAM)
    server.bind(sock_path)
    server.listen(8)
    server.setblocking(False)

    # Tell HAVI to use stdin/stdout event injection mode.
    env["HAVI_MAKEPAD_EVENTS"] = "1"

    # Forward Wayland/display env to HAVI.
    for var in ("WAYLAND_DISPLAY", "XDG_RUNTIME_DIR", "DISPLAY"):
        if var in os.environ and var not in env:
            env[var] = os.environ[var]

    _log("desktop run (makepad-socket)", cmd=cmd)
    havi_proc = subprocess.Popen(
        cmd,
        env=env,
        stdin=subprocess.PIPE,
        stdout=subprocess.PIPE,
        stderr=None,  # stderr passes through
        cwd=str(havi_root),
    )

    assert havi_proc.stdin is not None
    assert havi_proc.stdout is not None

    # Connected socket clients.
    clients: list[sock_mod.socket] = []
    client_bufs: dict[int, str] = {}  # fd -> partial line buffer

    # Wait for ReadyToStart from HAVI stdout.
    ready = threading.Event()       # set on ReadyToStart OR stdout EOF
    got_ready = threading.Event()   # set only on actual ReadyToStart
    stdout_lock = threading.Lock()
    devtools_addr: list[str] = []

    def _read_havi_stdout() -> None:
        """Read JSON lines from HAVI stdout, broadcast to clients."""
        assert havi_proc.stdout is not None
        for raw in havi_proc.stdout:
            line = raw.decode("utf-8", errors="replace") if isinstance(raw, bytes) else raw
            line = line.rstrip("\n")
            if not line:
                continue

            # Check for ReadyToStart
            try:
                msg: Any = json.loads(line)
                if isinstance(msg, dict) and "ReadyToStart" in msg:
                    got_ready.set()
                    ready.set()
            except json.JSONDecodeError:
                # Not JSON — might be stderr leak or log line.  Print it.
                print(line, file=sys.stderr)
                # Check for HAVI_DEVTOOLS in non-JSON output
                if line.strip().startswith("HAVI_DEVTOOLS=") and not devtools_addr:
                    devtools_addr.append(line.strip().split("=", 1)[1])
                continue

            # Broadcast to all connected clients.
            with stdout_lock:
                dead = []
                for c in clients:
                    try:
                        c.sendall((line + "\n").encode("utf-8"))
                    except (OSError, BrokenPipeError):
                        dead.append(c)
                for c in dead:
                    clients.remove(c)
                    client_bufs.pop(id(c), None)
                    c.close()

        # stdout EOF — HAVI process died. Unblock the wait.
        ready.set()

    reader = threading.Thread(target=_read_havi_stdout, daemon=True)
    reader.start()

    ready.wait(timeout=30)

    # Detect startup crash: process exited before sending ReadyToStart.
    rc = havi_proc.poll()
    if rc is not None and not got_ready.is_set():
        print(f"[mach-havi] HAVI crashed during startup (exit code {rc})", file=sys.stderr)
        server.close()
        try:
            os.unlink(sock_path)
        except FileNotFoundError:
            pass
        return rc

    if not got_ready.is_set():
        print("[mach-havi] warning: HAVI did not send ReadyToStart within 30s", file=sys.stderr)

    dt = devtools_addr[0] if devtools_addr else ""
    dt_port = dt.rsplit(":", 1)[-1] if dt else ""
    print(f"\nHAVI_MAKEPAD_SOCKET={sock_path}")
    if dt:
        print(f"HAVI_DEVTOOLS={dt}")
    print(f"# havi-makepad-cli --socket {sock_path} screenshot /tmp/test.png")
    if dt_port:
        print(f"# havi-devtools-cli -p {dt_port} eval 'document.title'")

    # Main relay loop: accept client connections, relay lines to HAVI stdin.
    stdin_lock = threading.Lock()

    def _write_to_havi(data: bytes) -> None:
        assert havi_proc.stdin is not None
        with stdin_lock:
            try:
                havi_proc.stdin.write(data)
                havi_proc.stdin.flush()
            except (OSError, BrokenPipeError):
                pass

    try:
        while havi_proc.poll() is None:
            # Build read list: server socket + all clients.
            rlist = [server] + clients
            try:
                readable, _, _ = select.select(rlist, [], [], 0.5)
            except (ValueError, OSError):
                # Bad fd — prune dead clients.
                with stdout_lock:
                    alive = []
                    for c in clients:
                        try:
                            c.fileno()
                            alive.append(c)
                        except Exception:
                            client_bufs.pop(id(c), None)
                    clients[:] = alive
                continue

            for s in readable:
                if s is server:
                    conn, _ = server.accept()
                    with stdout_lock:
                        clients.append(conn)
                        client_bufs[id(conn)] = ""
                else:
                    try:
                        data = s.recv(65536)
                    except (OSError, ConnectionResetError):
                        data = b""
                    if not data:
                        with stdout_lock:
                            if s in clients:
                                clients.remove(s)
                            client_bufs.pop(id(s), None)
                        s.close()
                        continue
                    # Buffer and extract complete lines.
                    buf = client_bufs.get(id(s), "") + data.decode("utf-8", errors="replace")
                    while "\n" in buf:
                        line, buf = buf.split("\n", 1)
                        line = line.strip()
                        if line:
                            _write_to_havi((line + "\n").encode("utf-8"))
                    client_bufs[id(s)] = buf
    except KeyboardInterrupt:
        pass
    finally:
        # Cleanup.
        server.close()
        try:
            os.unlink(sock_path)
        except FileNotFoundError:
            pass
        if havi_proc.poll() is None:
            havi_proc.terminate()
            havi_proc.wait(timeout=5)

    return havi_proc.returncode or 0
