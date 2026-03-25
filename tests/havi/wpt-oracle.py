#!/usr/bin/env -S uv --quiet run --script
# /// script
# requires-python = ">=3.11"
# dependencies = ["Pillow", "numpy"]
# ///

from __future__ import annotations

import argparse
import html.parser
import os
import shutil
import subprocess
from dataclasses import dataclass
from pathlib import Path

import numpy as np
from PIL import Image


@dataclass
class ReftestCase:
    operator: str
    test_path: Path
    ref_path: Path
    manifest_line: str


class LinkParser(html.parser.HTMLParser):
    def __init__(self) -> None:
        super().__init__()
        self.matches: list[str] = []
        self.mismatches: list[str] = []

    def handle_starttag(self, tag: str, attrs: list[tuple[str, str | None]]) -> None:
        if tag.lower() != "link":
            return
        attr_map = {name.lower(): value for name, value in attrs}
        rel = (attr_map.get("rel") or "").lower()
        href = attr_map.get("href")
        if not href:
            return
        if rel == "match":
            self.matches.append(href)
        elif rel == "mismatch":
            self.mismatches.append(href)


def parse_manifest(path: Path) -> list[ReftestCase]:
    cases: list[ReftestCase] = []
    for raw_line in path.read_text().splitlines():
        line = raw_line.strip()
        if not line or line.startswith("#"):
            continue
        parts = line.split()
        if len(parts) != 3 or parts[0] not in {"==", "!="}:
            raise SystemExit(f"invalid manifest line: {raw_line}")
        cases.append(
            ReftestCase(
                operator=parts[0],
                test_path=(path.parent / parts[1]).resolve(),
                ref_path=(path.parent / parts[2]).resolve(),
                manifest_line=raw_line,
            )
        )
    return cases


def parse_wpt_links(test_path: Path) -> list[ReftestCase]:
    parser = LinkParser()
    parser.feed(test_path.read_text(encoding="utf-8", errors="ignore"))
    cases: list[ReftestCase] = []
    for href in parser.matches:
        ref_path = (test_path.parent / href).resolve()
        cases.append(
            ReftestCase(
                operator="==",
                test_path=test_path.resolve(),
                ref_path=ref_path,
                manifest_line=f"== {test_path} {ref_path}",
            )
        )
    for href in parser.mismatches:
        ref_path = (test_path.parent / href).resolve()
        cases.append(
            ReftestCase(
                operator="!=",
                test_path=test_path.resolve(),
                ref_path=ref_path,
                manifest_line=f"!= {test_path} {ref_path}",
            )
        )
    if not cases:
        raise SystemExit(f"no rel=match or rel=mismatch links found in {test_path}")
    return cases


def load_cases(manifest: Path | None, wpt_test: Path | None, wpt_manifest: Path | None) -> list[ReftestCase]:
    if manifest is not None:
        return parse_manifest(manifest)
    if wpt_test is not None:
        return parse_wpt_links(wpt_test.resolve())
    if wpt_manifest is not None:
        cases: list[ReftestCase] = []
        for raw_line in wpt_manifest.read_text().splitlines():
            line = raw_line.strip()
            if not line or line.startswith("#"):
                continue
            # Support "== test ref" lines inline in wpt-manifest files
            parts = line.split()
            if len(parts) == 3 and parts[0] in {"==", "!="}:
                cases.append(ReftestCase(
                    operator=parts[0],
                    test_path=Path(parts[1]).resolve(),
                    ref_path=Path(parts[2]).resolve(),
                    manifest_line=line,
                ))
                continue
            # Try WPT rel=match parsing; fall back to self-compare
            test_path = Path(line).resolve()
            wpt_cases = _try_parse_wpt_links(test_path)
            if wpt_cases:
                cases.extend(wpt_cases)
            else:
                cases.append(ReftestCase(
                    operator="==",
                    test_path=test_path,
                    ref_path=test_path,
                    manifest_line=f"== {test_path} (self)",
                ))
        return cases
    raise SystemExit("one of --manifest, --wpt-test, or --wpt-manifest is required")


def _try_parse_wpt_links(test_path: Path) -> list[ReftestCase]:
    """Like parse_wpt_links but returns [] instead of raising on no links."""
    parser = LinkParser()
    parser.feed(test_path.read_text(encoding="utf-8", errors="ignore"))
    cases: list[ReftestCase] = []
    for href in parser.matches:
        ref_path = (test_path.parent / href).resolve()
        cases.append(ReftestCase(
            operator="==",
            test_path=test_path.resolve(),
            ref_path=ref_path,
            manifest_line=f"== {test_path} {ref_path}",
        ))
    for href in parser.mismatches:
        ref_path = (test_path.parent / href).resolve()
        cases.append(ReftestCase(
            operator="!=",
            test_path=test_path.resolve(),
            ref_path=ref_path,
            manifest_line=f"!= {test_path} {ref_path}",
        ))
    return cases


def slice_cases(cases: list[ReftestCase], offset: int, limit: int) -> list[ReftestCase]:
    if offset < 0:
        raise SystemExit("--offset must be >= 0")
    if limit <= 0:
        raise SystemExit("--limit must be > 0")
    return cases[offset:offset + limit]


def build_havi(havi_root: Path) -> None:
    result = subprocess.run(
        ["./mach-havi", "build"],
        cwd=havi_root,
        stdout=subprocess.PIPE,
        stderr=subprocess.STDOUT,
        text=True,
    )
    if result.returncode == 0:
        return
    raise SystemExit(result.stdout or "HAVI build failed")


def render_havi(havi_bin: Path, page: Path, output: Path, log_path: Path) -> None:
    env = dict(os.environ)
    env["HAVI_URL"] = page.resolve().as_uri()
    result = subprocess.run(
        [str(havi_bin), "--no-pylon", "--screenshot", str(output)],
        env=env,
        stdout=subprocess.PIPE,
        stderr=subprocess.STDOUT,
        text=True,
    )
    log_path.write_text(result.stdout or "", encoding="utf-8")
    if result.returncode == 0 and output.is_file():
        return
    raise SystemExit(result.stdout or f"render failed for {page}")


def render_browser(
    browser: str,
    page: Path,
    output: Path,
    log_path: Path,
    width: int,
    height: int,
    firefox_profile_dir: Path | None = None,
) -> None:
    if browser == "chromium":
        command = [
            browser,
            "--headless",
            "--disable-gpu",
            "--no-sandbox",
            "--disable-software-rasterizer",
            f"--window-size={width},{height}",
            f"--screenshot={output}",
            page.resolve().as_uri(),
        ]
    elif browser == "firefox":
        if firefox_profile_dir is None:
            raise SystemExit("firefox_profile_dir is required for firefox renders")
        command = [
            browser,
            "--headless",
            "--no-remote",
            "--profile",
            str(firefox_profile_dir),
            f"--window-size={width},{height}",
            "--screenshot",
            str(output),
            page.resolve().as_uri(),
        ]
    else:
        raise SystemExit(f"unsupported browser: {browser}")

    result = subprocess.run(
        command,
        stdout=subprocess.PIPE,
        stderr=subprocess.STDOUT,
        text=True,
    )
    log_path.write_text(result.stdout or "", encoding="utf-8")
    if result.returncode == 0 and output.is_file():
        return
    raise SystemExit(result.stdout or f"{browser} render failed for {page}")


def crop_common_from_origin(image_paths: list[Path]) -> None:
    existing = [path for path in image_paths if path.exists()]
    if not existing:
        return

    arrays: list[tuple[Path, np.ndarray]] = []
    min_height: int | None = None
    min_width: int | None = None
    for path in existing:
        array = np.array(Image.open(path).convert("RGB"))
        arrays.append((path, array))
        height, width = array.shape[0], array.shape[1]
        min_height = height if min_height is None else min(min_height, height)
        min_width = width if min_width is None else min(min_width, width)

    assert min_height is not None
    assert min_width is not None

    union = np.zeros((min_height, min_width), dtype=bool)
    for _, array in arrays:
        clipped = array[:min_height, :min_width]
        union |= np.any(clipped != 255, axis=2)

    ys, xs = np.where(union)
    if len(xs) == 0:
        crop_width = min_width
        crop_height = min_height
    else:
        crop_width = int(xs.max()) + 1
        crop_height = int(ys.max()) + 1

    for path, array in arrays:
        clipped = array[:crop_height, :crop_width]
        Image.fromarray(clipped.astype(np.uint8)).save(path, optimize=True)


def compare_images(left_png: Path, right_png: Path, diff_png: Path) -> tuple[bool, str]:
    left = np.array(Image.open(left_png).convert("RGB"))
    right = np.array(Image.open(right_png).convert("RGB"))
    height = min(left.shape[0], right.shape[0])
    width = min(left.shape[1], right.shape[1])
    left = left[:height, :width]
    right = right[:height, :width]

    diff = np.abs(left.astype(int) - right.astype(int))
    differs = np.any(diff > 50, axis=2)
    pixel_pct = 100.0 * np.sum(differs) / differs.size

    left_ink = np.any(left != 255, axis=2)
    right_ink = np.any(right != 255, axis=2)
    active = left_ink | right_ink
    if np.any(active):
        missed_left = left_ink & ~right_ink
        missed_right = right_ink & ~left_ink
        structure_pct = 100.0 * (np.sum(missed_left) + np.sum(missed_right)) / np.sum(active)
    else:
        structure_pct = 0.0

    mismatch_pct = max(pixel_pct, structure_pct)
    if mismatch_pct == 0.0:
        return True, ""

    overlay = left.copy()
    overlay[differs | (left_ink ^ right_ink)] = [255, 0, 0]
    Image.fromarray(overlay.astype(np.uint8)).save(diff_png)
    return False, f"visual mismatch ({mismatch_pct:.2f}%: pixel={pixel_pct:.2f}% structure={structure_pct:.2f}%)"


def case_stem(case: ReftestCase) -> str:
    stem = case.test_path.stem
    if case.operator == "!=":
        return f"{stem}-mismatch"
    return stem


def cleanup_success(paths: tuple[Path, ...]) -> None:
    for path in paths:
        path.unlink(missing_ok=True)


def compare_metric(left_png: Path, right_png: Path, diff_png: Path) -> float:
    left = np.array(Image.open(left_png).convert("RGB"))
    right = np.array(Image.open(right_png).convert("RGB"))
    height = min(left.shape[0], right.shape[0])
    width = min(left.shape[1], right.shape[1])
    left = left[:height, :width]
    right = right[:height, :width]

    diff = np.abs(left.astype(int) - right.astype(int))
    differs = np.any(diff > 50, axis=2)
    pixel_pct = 100.0 * np.sum(differs) / differs.size

    left_ink = np.any(left != 255, axis=2)
    right_ink = np.any(right != 255, axis=2)
    active = left_ink | right_ink
    if np.any(active):
        missed_left = left_ink & ~right_ink
        missed_right = right_ink & ~left_ink
        structure_pct = 100.0 * (np.sum(missed_left) + np.sum(missed_right)) / np.sum(active)
    else:
        structure_pct = 0.0

    overlay = left.copy()
    overlay[differs | (left_ink ^ right_ink)] = [255, 0, 0]
    Image.fromarray(overlay.astype(np.uint8)).save(diff_png)
    return max(pixel_pct, structure_pct)


def run_case(
    havi_bin: Path,
    artifacts_dir: Path,
    width: int,
    height: int,
    case: ReftestCase,
) -> tuple[bool, str]:
    if case.operator != "==":
        raise SystemExit("wpt-oracle.py currently supports only rel=match / == cases")

    stem = case_stem(case)
    havi_test_png = artifacts_dir / f"{stem}-havi-test.png"
    chrome_ref_png = artifacts_dir / f"{stem}-chromium-ref.png"
    chrome_test_png = artifacts_dir / f"{stem}-chromium-test.png"
    firefox_ref_png = artifacts_dir / f"{stem}-firefox-ref.png"
    firefox_test_png = artifacts_dir / f"{stem}-firefox-test.png"
    havi_test_cropped_png = artifacts_dir / f"{stem}-havi-test-cropped.png"
    chrome_ref_cropped_png = artifacts_dir / f"{stem}-chromium-ref-cropped.png"
    chrome_test_cropped_png = artifacts_dir / f"{stem}-chromium-test-cropped.png"
    firefox_ref_cropped_png = artifacts_dir / f"{stem}-firefox-ref-cropped.png"
    firefox_test_cropped_png = artifacts_dir / f"{stem}-firefox-test-cropped.png"
    diff_havi_chrome_ref = artifacts_dir / f"{stem}-diff-havi-chromium-test.png"
    diff_havi_firefox_ref = artifacts_dir / f"{stem}-diff-havi-firefox-test.png"
    diff_chrome_ref = artifacts_dir / f"{stem}-diff-chromium-ref.png"
    diff_firefox_ref = artifacts_dir / f"{stem}-diff-firefox-ref.png"
    diff_chrome_firefox = artifacts_dir / f"{stem}-diff-chromium-firefox.png"
    havi_test_log = artifacts_dir / f"{stem}-havi-test.log"
    chrome_ref_log = artifacts_dir / f"{stem}-chromium-ref.log"
    chrome_test_log = artifacts_dir / f"{stem}-chromium-test.log"
    firefox_ref_log = artifacts_dir / f"{stem}-firefox-ref.log"
    firefox_test_log = artifacts_dir / f"{stem}-firefox-test.log"
    for path in (
        havi_test_png,
        chrome_ref_png,
        chrome_test_png,
        firefox_ref_png,
        firefox_test_png,
        havi_test_cropped_png,
        chrome_ref_cropped_png,
        chrome_test_cropped_png,
        firefox_ref_cropped_png,
        firefox_test_cropped_png,
        diff_havi_chrome_ref,
        diff_havi_firefox_ref,
        diff_chrome_ref,
        diff_firefox_ref,
        diff_chrome_firefox,
        havi_test_log,
        chrome_ref_log,
        chrome_test_log,
        firefox_ref_log,
        firefox_test_log,
    ):
        path.unlink(missing_ok=True)

    firefox_profile_dir = artifacts_dir / "firefox-profile"
    firefox_profile_dir.mkdir(parents=True, exist_ok=True)

    render_havi(havi_bin, case.test_path, havi_test_png, havi_test_log)
    render_browser("chromium", case.ref_path, chrome_ref_png, chrome_ref_log, width, height)
    render_browser("chromium", case.test_path, chrome_test_png, chrome_test_log, width, height)
    render_browser("firefox", case.ref_path, firefox_ref_png, firefox_ref_log, width, height, firefox_profile_dir)
    render_browser("firefox", case.test_path, firefox_test_png, firefox_test_log, width, height, firefox_profile_dir)

    shutil.copyfile(havi_test_png, havi_test_cropped_png)
    shutil.copyfile(chrome_ref_png, chrome_ref_cropped_png)
    shutil.copyfile(chrome_test_png, chrome_test_cropped_png)
    shutil.copyfile(firefox_ref_png, firefox_ref_cropped_png)
    shutil.copyfile(firefox_test_png, firefox_test_cropped_png)
    crop_common_from_origin([
        havi_test_cropped_png,
        chrome_ref_cropped_png,
        chrome_test_cropped_png,
        firefox_ref_cropped_png,
        firefox_test_cropped_png,
    ])

    havi_chrome = compare_metric(havi_test_cropped_png, chrome_test_cropped_png, diff_havi_chrome_ref)
    havi_firefox = compare_metric(havi_test_cropped_png, firefox_test_cropped_png, diff_havi_firefox_ref)
    chrome_ref = compare_metric(chrome_test_cropped_png, chrome_ref_cropped_png, diff_chrome_ref)
    firefox_ref = compare_metric(firefox_test_cropped_png, firefox_ref_cropped_png, diff_firefox_ref)
    chrome_firefox = compare_metric(chrome_test_cropped_png, firefox_test_cropped_png, diff_chrome_firefox)

    best_havi = min(havi_chrome, havi_firefox)
    threshold = max(5.0, chrome_firefox * 2.0)
    passed = best_havi <= threshold
    if chrome_ref > 10.0 or firefox_ref > 10.0:
        classification = "bad-ref"
    elif passed:
        classification = "pass"
    else:
        classification = "likely-havi-error"
    summary = (
        f"{classification}: "
        f"havi↔ch={havi_chrome:.2f}% "
        f"havi↔ff={havi_firefox:.2f}% "
        f"ch↔ff={chrome_firefox:.2f}% "
        f"ch↔ref={chrome_ref:.2f}% "
        f"ff↔ref={firefox_ref:.2f}% "
        f"threshold={threshold:.2f}%"
    )
    return passed, summary


def main() -> int:
    parser = argparse.ArgumentParser(description="Run WPT screenshot oracle checks against a browser-rendered reference")
    parser.add_argument(
        "--manifest",
        type=Path,
        help="Manifest file with explicit ==/!= lines",
    )
    parser.add_argument(
        "--wpt-test",
        type=Path,
        help="Single WPT-style test file with rel=match or rel=mismatch links",
    )
    parser.add_argument(
        "--wpt-manifest",
        type=Path,
        help="File listing WPT-style test files, one per line",
    )
    parser.add_argument(
        "--artifacts-dir",
        default=Path(__file__).with_name("wpt-oracle-artifacts"),
        type=Path,
        help="Failure artifact directory",
    )
    parser.add_argument(
        "--offset",
        default=0,
        type=int,
        help="Start at this case index after manifest expansion",
    )
    parser.add_argument(
        "--limit",
        default=10,
        type=int,
        help="Maximum number of cases to run (default: 10)",
    )
    parser.add_argument(
        "--havi-bin",
        default=Path(__file__).resolve().parents[2] / "target" / "debug" / "havi",
        type=Path,
        help="Path to HAVI binary",
    )
    parser.add_argument(
        "--window-width",
        default=1280,
        type=int,
        help="Browser window width",
    )
    parser.add_argument(
        "--window-height",
        default=720,
        type=int,
        help="Browser window height",
    )
    args = parser.parse_args()

    if args.manifest is None and args.wpt_test is None and args.wpt_manifest is None:
        args.wpt_manifest = Path(__file__).with_name("reftest").joinpath("wpt-transforms.list")

    havi_root = Path(__file__).resolve().parents[2]
    build_havi(havi_root)

    all_cases = load_cases(args.manifest, args.wpt_test, args.wpt_manifest)
    cases = slice_cases(all_cases, args.offset, args.limit)
    if not cases:
        raise SystemExit(f"no cases selected (total={len(all_cases)}, offset={args.offset}, limit={args.limit})")
    print(f"Running {len(cases)} of {len(all_cases)} oracle case(s) with chromium+firefox (offset={args.offset}, limit={args.limit})")
    args.artifacts_dir.mkdir(parents=True, exist_ok=True)

    failed = 0
    skipped = 0
    for case in cases:
        if case.operator != "==":
            skipped += 1
            print(f"SKIP {case.manifest_line} (oracle mode only supports == cases)")
            continue
        ok, reason = run_case(
            args.havi_bin,
            args.artifacts_dir,
            args.window_width,
            args.window_height,
            case,
        )
        if ok:
            print(f"PASS {case.manifest_line}")
        else:
            failed += 1
            stem = case_stem(case)
            print(f"FAIL {case.manifest_line} ({reason})")
            print(f"  artifacts: {args.artifacts_dir}/{stem}-{{havi-test,chromium-test,firefox-test}}-cropped.png")
            print(f"             {args.artifacts_dir}/{stem}-diff-{{havi-chromium-test,havi-firefox-test,chromium-firefox}}.png")
            print(f"             {args.artifacts_dir}/{stem}-havi-test.log")

    if skipped:
        print(f"Skipped {skipped} mismatch case(s)")
    if failed:
        print(f"{failed}/{len(cases) - skipped} oracle reftests failed")
        print("Advice: inspect the browser test image, browser reference image, HAVI image, and both diffs before treating the result as an engine bug.")
        return 1
    print(f"All {len(cases) - skipped} oracle reftests passed")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
