#!/usr/bin/env -S uv --quiet run --script
# /// script
# requires-python = ">=3.11"
# dependencies = ["Pillow"]
# ///

from __future__ import annotations

import argparse
import html.parser
import os
import shutil
import subprocess
from dataclasses import dataclass
from pathlib import Path

from PIL import Image, ImageChops


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
    # WPT input files are read in place. They can live anywhere, including the
    # shared source tree under experiment/servo-mainline/tests/wpt/tests. The
    # runner resolves rel=match and rel=mismatch references relative to the test
    # file location, so no local copy into havi/tests/havi is required.
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
            cases.extend(parse_wpt_links(Path(line).resolve()))
        return cases
    raise SystemExit("one of --manifest, --wpt-test, or --wpt-manifest is required")


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


def render_page(havi_bin: Path, page: Path, output: Path, log_path: Path) -> None:
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


def compare_images(test_png: Path, ref_png: Path, diff_png: Path) -> tuple[bool, str]:
    with Image.open(test_png) as test_image, Image.open(ref_png) as ref_image:
        test_rgba = test_image.convert("RGBA")
        ref_rgba = ref_image.convert("RGBA")
        if test_rgba.size != ref_rgba.size:
            return False, f"size mismatch: {test_rgba.size} vs {ref_rgba.size}"

        if test_rgba.tobytes() == ref_rgba.tobytes():
            return True, ""

        diff = ImageChops.difference(test_rgba, ref_rgba)
        diff.save(diff_png)
        return False, "pixel mismatch"


def case_stem(case: ReftestCase) -> str:
    stem = case.test_path.stem
    if case.operator == "!=":
        return f"{stem}-mismatch"
    return stem


def run_case(havi_bin: Path, artifacts_dir: Path, case: ReftestCase) -> tuple[bool, str]:
    stem = case_stem(case)
    test_png = artifacts_dir / f"{stem}-test.png"
    ref_png = artifacts_dir / f"{stem}-ref.png"
    diff_png = artifacts_dir / f"{stem}-diff.png"
    test_log = artifacts_dir / f"{stem}-test.log"
    ref_log = artifacts_dir / f"{stem}-ref.log"
    for path in (test_png, ref_png, diff_png, test_log, ref_log):
        path.unlink(missing_ok=True)

    render_page(havi_bin, case.test_path, test_png, test_log)
    render_page(havi_bin, case.ref_path, ref_png, ref_log)
    equal, reason = compare_images(test_png, ref_png, diff_png)
    passed = equal if case.operator == "==" else not equal
    if passed:
        test_png.unlink(missing_ok=True)
        ref_png.unlink(missing_ok=True)
        diff_png.unlink(missing_ok=True)
        test_log.unlink(missing_ok=True)
        ref_log.unlink(missing_ok=True)
        return True, ""

    if not diff_png.exists() and test_png.exists() and ref_png.exists():
        shutil.copyfile(test_png, diff_png)
    return False, reason or "comparison failed"


def main() -> int:
    parser = argparse.ArgumentParser(description="Run tiny HAVI screenshot reftests")
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
        default=Path(__file__).with_name("reftest-artifacts"),
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
    args = parser.parse_args()

    if args.manifest is None and args.wpt_test is None and args.wpt_manifest is None:
        args.manifest = Path(__file__).with_name("reftest").joinpath("reftest.list")

    havi_root = Path(__file__).resolve().parents[2]
    build_havi(havi_root)

    all_cases = load_cases(args.manifest, args.wpt_test, args.wpt_manifest)
    cases = slice_cases(all_cases, args.offset, args.limit)
    if not cases:
        raise SystemExit(f"no cases selected (total={len(all_cases)}, offset={args.offset}, limit={args.limit})")
    print(f"Running {len(cases)} of {len(all_cases)} case(s) (offset={args.offset}, limit={args.limit})")
    args.artifacts_dir.mkdir(parents=True, exist_ok=True)

    failed = 0
    for case in cases:
        ok, reason = run_case(args.havi_bin, args.artifacts_dir, case)
        if ok:
            print(f"PASS {case.manifest_line}")
        else:
            failed += 1
            stem = case_stem(case)
            print(f"FAIL {case.manifest_line} ({reason})")
            print(f"  artifacts: {args.artifacts_dir / (stem + '-test.png')}")
            print(f"             {args.artifacts_dir / (stem + '-ref.png')}")
            print(f"             {args.artifacts_dir / (stem + '-diff.png')}")
            print(f"             {args.artifacts_dir / (stem + '-test.log')}")
            print(f"             {args.artifacts_dir / (stem + '-ref.log')}")

    if failed:
        print(f"{failed}/{len(cases)} reftests failed")
        print("Advice: inspect the failing test source, reference source, PNG artifacts, and saved HAVI logs manually to confirm whether the reftest itself is correct before treating the result as an engine bug.")
        return 1
    print(f"All {len(cases)} reftests passed")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
