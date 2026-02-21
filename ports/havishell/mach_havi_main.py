# Copyright 2025 The Servo Project Developers. See the COPYRIGHT
# file at the top-level directory of this distribution.
#
# Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
# http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
# <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
# option. This file may not be copied, modified, or distributed
# except according to those terms.

"""
mach_havi_main — Core logic for the havishell build system.

Called by the thin mach-havi launcher at the repo root.
Reuses servo's Python env-setup modules, delegates to cargo makepad
for Android packaging.
"""

from __future__ import annotations

import argparse
import glob
import os
import pathlib
import platform
import shutil
import subprocess
import sys
import tempfile
import time
from typing import Any


# ---------------------------------------------------------------------------
# Paths (set by run())
# ---------------------------------------------------------------------------

HAVI_ROOT: pathlib.Path
HAVISHELL_DIR: pathlib.Path
MAKEPAD_ROOT: pathlib.Path
CARGO_MAKEPAD_DIR: pathlib.Path

TRIPLE_TO_ABI = {
    "aarch64-linux-android": "aarch64",
    "x86_64-linux-android": "x86_64",
    "armv7-linux-androideabi": "armv7",
    "i686-linux-android": "i686",
}

DEFAULT_ANDROID_TRIPLE = "aarch64-linux-android"
EMULATOR_TRIPLE = "x86_64-linux-android"


# ---------------------------------------------------------------------------
# SDK / NDK / tool discovery
# ---------------------------------------------------------------------------


def _host_os_tag() -> str:
    """Return e.g. 'linux_x64', 'darwin_aarch64'."""
    os_name = platform.system().lower()
    if os_name == "darwin":
        os_name = "macos"
    cpu = platform.machine().lower()
    if cpu in ("x86_64", "x86-64", "x64", "amd64"):
        cpu = "x64"
    elif cpu in ("aarch64", "arm64"):
        cpu = "aarch64"
    return f"{os_name}_{cpu}"


def _detect_cargo_makepad_sdk() -> pathlib.Path | None:
    """cargo-makepad's downloaded SDK dir (android_33_<os>_<arch>/)."""
    tag = _host_os_tag()
    candidate = CARGO_MAKEPAD_DIR / f"android_33_{tag}"
    if candidate.is_dir():
        return candidate.resolve()
    for p in sorted(CARGO_MAKEPAD_DIR.glob("android_33_*")):
        if p.is_dir():
            return p.resolve()
    return None


def _detect_ndk_root(sdk_dir: pathlib.Path | None) -> pathlib.Path | None:
    """NDK root inside cargo-makepad SDK: <sdk>/ndk/<version>/."""
    if sdk_dir is None:
        return None
    ndk_parent = sdk_dir / "ndk"
    if not ndk_parent.is_dir():
        return None
    versions = sorted(ndk_parent.iterdir())
    return versions[-1].resolve() if versions else None


def _find_android_sdk() -> pathlib.Path | None:
    """Find the system Android SDK (for emulator/adb), trying common locations."""
    for env_var in ("ANDROID_HOME", "ANDROID_SDK_ROOT", "ANDROID_SDK"):
        val = os.environ.get(env_var)
        if val and os.path.isdir(val):
            return pathlib.Path(val).resolve()

    home = pathlib.Path.home()
    candidates = [
        home / "Android-Sdk",
        home / "Android" / "Sdk",
        home / "android-sdk",
        home / "Library" / "Android" / "sdk",
        pathlib.Path("/opt/android-sdk"),
        pathlib.Path("/usr/local/android-sdk"),
    ]
    for c in candidates:
        if c.is_dir():
            return c.resolve()
    return None


def _find_tool(name: str) -> str | None:
    """Find an Android tool binary: first on PATH, then in the system SDK."""
    on_path = shutil.which(name)
    if on_path:
        return on_path

    sdk = _find_android_sdk()
    if sdk is None:
        return None

    search = {
        "adb": [sdk / "platform-tools" / "adb"],
        "emulator": [sdk / "emulator" / "emulator"],
        "avdmanager": [
            sdk / "cmdline-tools" / "latest" / "bin" / "avdmanager",
            sdk / "tools" / "bin" / "avdmanager",
        ],
    }
    for candidate in search.get(name, []):
        if candidate.is_file():
            return str(candidate)
    return None


def _list_avds() -> list[str]:
    """List available AVDs by name."""
    avdmanager = _find_tool("avdmanager")
    if avdmanager:
        try:
            out = subprocess.check_output(
                [avdmanager, "list", "avd"], encoding="utf-8", stderr=subprocess.DEVNULL
            )
            return [
                line.split(":", 1)[1].strip()
                for line in out.splitlines()
                if line.strip().startswith("Name:")
            ]
        except (subprocess.CalledProcessError, OSError):
            pass

    # Fallback: scan ~/.android/avd/
    avd_dir = pathlib.Path.home() / ".android" / "avd"
    if avd_dir.is_dir():
        return [p.stem for p in avd_dir.glob("*.ini")]
    return []


# ---------------------------------------------------------------------------
# Helpers
# ---------------------------------------------------------------------------


def triple_to_abi(triple: str) -> str:
    abi = TRIPLE_TO_ABI.get(triple)
    if abi is None:
        sys.exit(
            f"error: unsupported Android target triple '{triple}'. "
            f"Supported: {', '.join(TRIPLE_TO_ABI.keys())}"
        )
    return abi


def _log(action: str, *, target: str | None = None, abi: str | None = None,
         env: dict[str, str] | None = None, cmd: list[str] | None = None) -> None:
    parts = [f"[mach-havi] {action}"]
    if target:
        parts.append(f"target={target}")
    if abi:
        parts.append(f"abi={abi}")
    print(" | ".join(parts))
    if env:
        for key in ("ANDROID_NDK_ROOT", "ANDROID_SDK_ROOT", "LIBCLANG_PATH", "CC", "CXX"):
            val = env.get(key)
            if val:
                print(f"  {key}={val}")
    if cmd:
        print(f"  $ {' '.join(cmd)}")


# ---------------------------------------------------------------------------
# Environment setup
# ---------------------------------------------------------------------------


def _make_config(ndk_path: str = "", sdk_path: str = "") -> dict[str, Any]:
    """Minimal config dict matching CommandBase.__init__ layout."""
    from servo.platform.build_target import SanitizerKind
    return {
        "android": {"sdk": sdk_path, "ndk": ndk_path, "toolchain": ""},
        "build": {"sanitizer": SanitizerKind.NONE},
    }


def setup_android_env(target_triple: str) -> dict[str, str]:
    """Full cross-compilation environment for an Android target."""
    from servo.platform.build_target import AndroidTarget

    cm_sdk = _detect_cargo_makepad_sdk()
    cm_ndk = _detect_ndk_root(cm_sdk)

    ndk_root = os.environ.get("ANDROID_NDK_ROOT", str(cm_ndk) if cm_ndk else "")
    sdk_root = os.environ.get("ANDROID_SDK_ROOT", str(cm_sdk) if cm_sdk else "")

    if not ndk_root:
        sys.exit(
            "error: Cannot find Android NDK. Set ANDROID_NDK_ROOT or run:\n"
            f"  cd {MAKEPAD_ROOT} && cargo makepad android install-toolchain"
        )
    if not sdk_root:
        sys.exit(
            "error: Cannot find Android SDK. Set ANDROID_SDK_ROOT or run:\n"
            f"  cd {MAKEPAD_ROOT} && cargo makepad android install-toolchain"
        )

    env = os.environ.copy()
    env.setdefault("CC", "clang")
    env.setdefault("CXX", "clang++")
    env.setdefault("RUSTFLAGS", "")
    env["ANDROID_NDK_ROOT"] = ndk_root
    env["ANDROID_SDK_ROOT"] = sdk_root

    config = _make_config(ndk_path=ndk_root, sdk_path=sdk_root)
    target = AndroidTarget(target_triple)
    target.configure_build_environment(env, config, HAVI_ROOT)

    # cc-rs underscore-separated target triple vars.
    # Force these (not setdefault) to ensure a single consistent NDK toolchain.
    # If the parent env already has CXX_x86_64_linux_android pointing at a
    # different NDK (e.g. r28), mixing it with our NDK's libc++_shared.so
    # causes ABI mismatches at runtime.
    u = target_triple.replace("-", "_")
    env[f"CC_{u}"] = env.get("TARGET_CC", "")
    env[f"CXX_{u}"] = env.get("TARGET_CXX", "")
    env[f"AR_{u}"] = env.get("TARGET_AR", "")
    env[f"RANLIB_{u}"] = env.get("TARGET_RANLIB", "")

    linker_var = f"CARGO_TARGET_{u.upper()}_LINKER"
    env[linker_var] = env.get("TARGET_CC", "")

    return env


def setup_desktop_env() -> dict[str, str]:
    """Build environment for a desktop (host) build."""
    env = os.environ.copy()
    env.setdefault("CC", "clang")
    env.setdefault("CXX", "clang++")
    env.setdefault("RUSTFLAGS", "")

    if "LIBCLANG_PATH" not in env:
        for candidate in [
            "/usr/lib/llvm-18/lib",
            "/usr/lib/llvm-17/lib",
            "/usr/lib/llvm-16/lib",
            "/usr/lib/llvm-15/lib",
            "/usr/lib/llvm-14/lib",
            "/usr/lib64/llvm",
            "/usr/local/opt/llvm/lib",
            "/opt/homebrew/opt/llvm/lib",
        ]:
            if os.path.isdir(candidate) and glob.glob(os.path.join(candidate, "libclang*")):
                env["LIBCLANG_PATH"] = candidate
                break

    if "CLANG_PATH" not in env:
        clang = shutil.which("clang")
        if clang:
            env["CLANG_PATH"] = clang

    return env


# ---------------------------------------------------------------------------
# Emulator management
# ---------------------------------------------------------------------------


def _wait_for_device(adb: str, timeout: int = 120) -> bool:
    """Wait for an emulator to become ready (booted)."""
    print(f"[mach-havi] waiting for device (timeout {timeout}s)...")
    deadline = time.time() + timeout

    # First wait for adb to see the device
    try:
        subprocess.run(
            [adb, "wait-for-device"],
            timeout=min(timeout, 60),
            check=True,
        )
    except (subprocess.TimeoutExpired, subprocess.CalledProcessError):
        return False

    # Then wait for boot to complete
    while time.time() < deadline:
        try:
            result = subprocess.run(
                [adb, "shell", "getprop", "sys.boot_completed"],
                capture_output=True, encoding="utf-8", timeout=5,
            )
            if result.stdout.strip() == "1":
                print("[mach-havi] device is ready")
                return True
        except (subprocess.TimeoutExpired, subprocess.CalledProcessError, OSError):
            pass
        time.sleep(2)
    return False


DEFAULT_AVD_NAME = "havi-test"


def _find_system_image() -> str | None:
    """Find a usable x86_64 system image for AVD creation."""
    sdk = _find_android_sdk()
    if not sdk:
        return None
    si_root = sdk / "system-images"
    if not si_root.is_dir():
        return None
    # Prefer google_apis, fall back to google_apis_playstore, then default
    for api_dir in sorted(si_root.iterdir(), reverse=True):
        for variant in ("google_apis", "google_apis_playstore", "default"):
            x86_dir = api_dir / variant / "x86_64"
            if (x86_dir / "system.img").is_file():
                # e.g. "system-images;android-36;google_apis_playstore;x86_64"
                return f"system-images;{api_dir.name};{variant};x86_64"
    return None


def _create_avd(avd_name: str) -> None:
    """Create a new AVD with the best available x86_64 system image."""
    avdmanager = _find_tool("avdmanager")
    if not avdmanager:
        sys.exit("error: avdmanager not found. Install Android SDK cmdline-tools.")

    image = _find_system_image()
    if not image:
        sys.exit(
            "error: no x86_64 system image found. Install one with:\n"
            "  sdkmanager 'system-images;android-33;google_apis;x86_64'"
        )

    print(f"[mach-havi] creating AVD '{avd_name}' with {image}...")
    ret = subprocess.run(
        [avdmanager, "create", "avd", "-n", avd_name, "-k", image,
         "--device", "pixel", "--force"],
        input="no\n", text=True,
    ).returncode
    if ret != 0:
        sys.exit(f"error: failed to create AVD '{avd_name}'")


def _emulator_env() -> dict[str, str]:
    """Environment for the emulator process. Always sets XDG_RUNTIME_DIR."""
    env = os.environ.copy()
    runtime_dir = os.path.join(tempfile.gettempdir(), "android-emu-runtime")
    os.makedirs(runtime_dir, exist_ok=True)
    env["XDG_RUNTIME_DIR"] = runtime_dir
    return env


def _ensure_emulator(avd_name: str | None = None) -> str | None:
    """Ensure an emulator is running. Returns the serial (e.g. 'emulator-5554')
    or None on failure.

    If an emulator is already running, reuses it. Otherwise creates an AVD
    if needed and launches one."""
    adb = _find_tool("adb")
    if not adb:
        sys.exit("error: adb not found. Install Android SDK platform-tools.")

    # Check if an emulator is already online
    try:
        out = subprocess.check_output([adb, "devices"], encoding="utf-8")
        for line in out.strip().splitlines()[1:]:
            parts = line.split()
            if len(parts) >= 2 and parts[0].startswith("emulator-") and parts[1] == "device":
                serial = parts[0]
                print(f"[mach-havi] reusing running emulator: {serial}")
                return serial
    except (subprocess.CalledProcessError, OSError):
        pass

    # Need to launch one
    emulator_bin = _find_tool("emulator")
    if not emulator_bin:
        sys.exit("error: emulator not found. Install via Android SDK Manager.")

    if not avd_name:
        avd_name = DEFAULT_AVD_NAME

    # Create AVD if it doesn't exist
    avds = _list_avds()
    if avd_name not in avds:
        _create_avd(avd_name)

    print(f"[mach-havi] launching emulator with AVD '{avd_name}'...")
    emu_env = _emulator_env()
    subprocess.Popen(
        [emulator_bin, "-avd", avd_name],
        stdout=subprocess.DEVNULL,
        stderr=subprocess.DEVNULL,
        env=emu_env,
    )

    if not _wait_for_device(adb, timeout=180):
        sys.exit("error: emulator failed to boot within timeout")

    # Re-detect serial
    try:
        out = subprocess.check_output([adb, "devices"], encoding="utf-8")
        for line in out.strip().splitlines()[1:]:
            parts = line.split()
            if len(parts) >= 2 and parts[0].startswith("emulator-") and parts[1] == "device":
                return parts[0]
    except (subprocess.CalledProcessError, OSError):
        pass

    return None


def _install_and_launch_apk(serial: str, apk_path: str, package: str) -> int:
    """Install APK and launch it on the given device."""
    adb = _find_tool("adb")
    if not adb:
        sys.exit("error: adb not found")

    print(f"[mach-havi] installing {os.path.basename(apk_path)} on {serial}...")
    ret = subprocess.call([adb, "-s", serial, "install", "-r", apk_path])
    if ret != 0:
        print("[mach-havi] install failed")
        return ret

    activity = f"{package}/{package}.MakepadApp"
    print(f"[mach-havi] launching {activity}...")
    ret = subprocess.call([adb, "-s", serial, "shell", "am", "start", "-n", activity])
    if ret != 0:
        print("[mach-havi] launch failed")
        return ret

    # Tail logcat for the app
    print(f"[mach-havi] tailing logcat (Ctrl+C to stop)...")
    try:
        # Get PID
        for _ in range(30):
            result = subprocess.run(
                [adb, "-s", serial, "shell", "pidof", package],
                capture_output=True, encoding="utf-8", timeout=5,
            )
            pid = result.stdout.strip()
            if pid:
                break
            time.sleep(0.5)
        else:
            print("[mach-havi] could not get app PID, showing unfiltered logcat")
            pid = ""

        if pid:
            subprocess.call([adb, "-s", serial, "logcat", "--pid", pid, "Makepad:D", "*:S"])
        else:
            subprocess.call([adb, "-s", serial, "logcat"])
    except KeyboardInterrupt:
        print("\n[mach-havi] logcat stopped")

    return 0


def _find_apk(target_triple: str, release: bool) -> str | None:
    """Find the APK that cargo-makepad built."""
    profile = "release" if release else "debug"
    # havishell is a workspace member, so output lands in the workspace target dir.
    # cargo-makepad puts APKs in target/makepad-android-apk/<crate>/apk/
    search_roots = [
        HAVI_ROOT / "target",
    ]
    for root in search_roots:
        for apk in sorted(root.rglob("havishell*.apk"), key=lambda p: p.stat().st_mtime, reverse=True):
            return str(apk)
        for apk in sorted(root.rglob("*.apk"), key=lambda p: p.stat().st_mtime, reverse=True):
            return str(apk)
    return None


# ---------------------------------------------------------------------------
# Commands
# ---------------------------------------------------------------------------


def cmd_build(args: argparse.Namespace) -> int:
    if args.android or args.emulator:
        return _build_android(args)
    return _build_desktop(args)


DEFAULT_PACKAGE_NAME = "dev.makepad.havishell"

# NDK version that cargo-makepad expects (must match sdk.rs).
_EXPECTED_NDK_VERSION = "28.2.13676358"


def _ensure_cargo_makepad_ndk() -> None:
    """Make sure the cargo-makepad SDK dir contains the expected NDK.

    Strategy:
      1. If the NDK dir already exists with the right version → nothing to do.
      2. If a system NDK r28 is available → symlink it in.
      3. Otherwise → run ``cargo makepad android install-toolchain``.

    This lets ``./mach-havi build`` work out-of-the-box for new devs.
    """
    cm_sdk = _detect_cargo_makepad_sdk()
    if cm_sdk is None:
        # No SDK dir at all — create the expected one so install-toolchain
        # has somewhere to put things, then fall through.
        tag = _host_os_tag()
        cm_sdk = CARGO_MAKEPAD_DIR / f"android_33_{tag}"
        cm_sdk.mkdir(parents=True, exist_ok=True)

    ndk_parent = cm_sdk / "ndk"
    expected = ndk_parent / _EXPECTED_NDK_VERSION

    if expected.is_dir():
        # Quick sanity: does it look like an NDK?
        if (expected / "toolchains").is_dir() or (expected / "source.properties").is_file():
            return  # Already good.

    # ---------- Try symlinking a system NDK r28 ----------
    sys_sdk = _find_android_sdk()
    if sys_sdk:
        for ndk_dir in sorted((sys_sdk / "ndk").iterdir()) if (sys_sdk / "ndk").is_dir() else []:
            props = ndk_dir / "source.properties"
            if props.is_file():
                version_line = props.read_text().strip().split("\n")
                for line in version_line:
                    if line.startswith("Pkg.Revision"):
                        ver = line.split("=")[-1].strip().split(".")[0]
                        if ver == "28" and (ndk_dir / "toolchains").is_dir():
                            ndk_parent.mkdir(parents=True, exist_ok=True)
                            # Remove stale entry if present (e.g. broken symlink).
                            if expected.is_symlink() or expected.exists():
                                if expected.is_symlink():
                                    expected.unlink()
                                else:
                                    import shutil as _sh
                                    _sh.rmtree(expected)
                            expected.symlink_to(ndk_dir.resolve())
                            print(f"[mach-havi] Symlinked system NDK r28: {ndk_dir} → {expected}")
                            return

    # ---------- Fall back to downloading via cargo-makepad ----------
    print("[mach-havi] NDK r28 not found locally. Running cargo makepad android install-toolchain...")
    ret = subprocess.call(
        ["cargo", "makepad", "android", f"--sdk-path={cm_sdk}", "install-toolchain"],
        cwd=str(MAKEPAD_ROOT),
    )
    if ret != 0:
        sys.exit(f"[mach-havi] install-toolchain failed (exit {ret})")


def _cargo_makepad_android_cmd(abi: str, package_name: str | None = None) -> list[str]:
    """Base cargo makepad android command with --abi and --sdk-path."""
    cmd = ["cargo", "makepad", "android", f"--abi={abi}"]
    cm_sdk = _detect_cargo_makepad_sdk()
    if cm_sdk:
        cmd.append(f"--sdk-path={cm_sdk}")
    if package_name:
        cmd.append(f"--package-name={package_name}")
    return cmd


def _build_android(args: argparse.Namespace) -> int:
    _ensure_cargo_makepad_ndk()
    target_triple = args.target or (EMULATOR_TRIPLE if args.emulator else DEFAULT_ANDROID_TRIPLE)
    abi = triple_to_abi(target_triple)
    env = setup_android_env(target_triple)
    package_name = getattr(args, "package_name", None)

    cmd = _cargo_makepad_android_cmd(abi, package_name)
    cmd.append("build")
    cmd.extend(["-p", "havishell"])
    if args.release:
        cmd.append("--release")
    if args.extra:
        cmd.extend(args.extra)

    _log("android build", target=target_triple, abi=abi, env=env, cmd=cmd)
    ret = subprocess.call(cmd, env=env, cwd=str(HAVI_ROOT))
    if ret != 0:
        return ret

    if args.emulator:
        return _emulator_install_and_run(args, target_triple)

    return 0


def _emulator_install_and_run(args: argparse.Namespace, target_triple: str) -> int:
    """After a successful build, launch emulator, install APK, start app."""
    serial = _ensure_emulator(getattr(args, "avd", None))
    if not serial:
        sys.exit("error: could not connect to emulator")

    apk = _find_apk(target_triple, args.release)
    if not apk:
        sys.exit("error: APK not found after build. Check cargo-makepad output above.")

    print(f"[mach-havi] APK: {apk}")
    package = getattr(args, "package_name", None) or DEFAULT_PACKAGE_NAME
    return _install_and_launch_apk(serial, apk, package)


def _build_desktop(args: argparse.Namespace) -> int:
    env = setup_desktop_env()
    cmd = ["cargo", "build", "--manifest-path", str(HAVISHELL_DIR / "Cargo.toml")]
    if args.release:
        cmd.append("--release")
    if args.extra:
        cmd.extend(args.extra)

    _log("desktop build", env=env, cmd=cmd)
    return subprocess.call(cmd, env=env, cwd=str(HAVI_ROOT))


def cmd_check(args: argparse.Namespace) -> int:
    """Run cargo check with the same environment as build."""
    env = setup_desktop_env()
    cmd = ["cargo", "check", "--manifest-path", str(HAVISHELL_DIR / "Cargo.toml")]
    if args.extra:
        cmd.extend(args.extra)

    _log("desktop check", env=env, cmd=cmd)
    return subprocess.call(cmd, env=env, cwd=str(HAVI_ROOT))


def cmd_run(args: argparse.Namespace) -> int:
    if args.emulator:
        return _run_emulator(args)
    if args.android:
        return _run_android(args)
    return _run_desktop(args)


def _run_android(args: argparse.Namespace) -> int:
    """Build + run via cargo makepad android run (physical device)."""
    _ensure_cargo_makepad_ndk()
    target_triple = args.target or DEFAULT_ANDROID_TRIPLE
    abi = triple_to_abi(target_triple)
    env = setup_android_env(target_triple)
    package_name = getattr(args, "package_name", None)

    cmd = _cargo_makepad_android_cmd(abi, package_name)
    cmd.append("run")
    cmd.extend(["-p", "havishell"])
    if args.release:
        cmd.append("--release")
    if args.extra:
        cmd.extend(args.extra)

    _log("android run (device)", target=target_triple, abi=abi, env=env, cmd=cmd)
    return subprocess.call(cmd, env=env, cwd=str(HAVI_ROOT))


def _run_emulator(args: argparse.Namespace) -> int:
    """Build for x86_64, boot emulator, install, launch, tail logcat."""
    target_triple = args.target or EMULATOR_TRIPLE
    abi = triple_to_abi(target_triple)
    env = setup_android_env(target_triple)
    package_name = getattr(args, "package_name", None)

    # Build
    cmd = _cargo_makepad_android_cmd(abi, package_name)
    cmd.append("build")
    cmd.extend(["-p", "havishell"])
    if args.release:
        cmd.append("--release")
    if args.extra:
        cmd.extend(args.extra)

    _log("emulator build", target=target_triple, abi=abi, env=env, cmd=cmd)
    ret = subprocess.call(cmd, env=env, cwd=str(HAVI_ROOT))
    if ret != 0:
        return ret

    return _emulator_install_and_run(args, target_triple)


def _run_desktop(args: argparse.Namespace) -> int:
    env = setup_desktop_env()
    profile = "release" if args.release else "debug"
    # Auto-enable remote control in debug builds.
    if profile == "debug" and "MAKEPAD_REMOTE" not in env:
        env["MAKEPAD_REMOTE"] = "0"
    binary = HAVI_ROOT / "target" / profile / "havi"

    if not binary.exists():
        print(f"[mach-havi] binary not found at {binary}, building first...")
        ret = _build_desktop(args)
        if ret != 0:
            return ret

    cmd = [str(binary)]
    if args.extra:
        cmd.extend(args.extra)

    _log("desktop run", env=env, cmd=cmd)
    return subprocess.call(cmd, env=env, cwd=str(HAVI_ROOT))


# ---------------------------------------------------------------------------
# Argument parsing & entry point
# ---------------------------------------------------------------------------


def run(topdir: str) -> int:
    global HAVI_ROOT, HAVISHELL_DIR, MAKEPAD_ROOT, CARGO_MAKEPAD_DIR
    HAVI_ROOT = pathlib.Path(topdir)
    HAVISHELL_DIR = HAVI_ROOT / "ports" / "havishell"
    MAKEPAD_ROOT = HAVI_ROOT.parent / "makepad"
    CARGO_MAKEPAD_DIR = MAKEPAD_ROOT / "tools" / "cargo_makepad"

    parser = argparse.ArgumentParser(
        prog="mach-havi",
        description="Build entry point for the havishell port of havi",
    )
    sub = parser.add_subparsers(dest="command", required=True)

    for name, handler, verb in [
        ("build", cmd_build, "Build"),
        ("check", cmd_check, "Check"),
        ("run", cmd_run, "Run"),
    ]:
        p = sub.add_parser(name, help=f"{verb} havishell")
        p.add_argument("--release", "-r", action="store_true",
                        help=f"{verb} in release mode")
        p.add_argument("--android", action="store_true",
                        help=f"{verb} for/on a physical Android device")
        p.add_argument("--emulator", "-e", action="store_true",
                        help=f"{verb} on Android emulator (implies --target {EMULATOR_TRIPLE})")
        p.add_argument("--avd", default=None,
                        help="AVD name for --emulator (default: first available)")
        p.add_argument("--target", "-t", default=None,
                        help="Target triple (e.g. aarch64-linux-android, x86_64-linux-android)")
        p.add_argument("--package-name", default=None,
                        help="Android package name (default: dev.makepad.havishell)")
        p.add_argument("extra", nargs="*",
                        help="Extra arguments forwarded to cargo")
        p.set_defaults(func=handler)

    args = parser.parse_args()
    return args.func(args)
