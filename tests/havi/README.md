# HAVI Integration Tests

Each `*-test.sh` script starts an hpprd server, imports test content, launches
HAVI, and runs either JS assertions through the DevTools protocol or screenshot
assertions through the native `--screenshot` path. Some shell-layout and final
presentation tests instead drive desktop HAVI through `havi-makepad-cli` and
capture the final Makepad window output.

## Running

```bash
# All tests (parallel)
make -j4 test

# Single behavior test
./address-test.sh

# Tiny exact screenshot reftest suite
./reftest.py

# Browser-oracle screenshot validation on curated WPT transforms / 3D cases
./wpt-oracle.py

# Exact HAVI-vs-reference checks for curated WPT cases
./reftest.py --wpt-manifest reftest/wpt-transforms.list

# Browser-oracle checks for curated WPT cases
./wpt-oracle.py --wpt-manifest reftest/wpt-transforms.list

# Focused MaskFallback projected-clip validation
./reftest.py --wpt-manifest reftest/mask-fallback-wpt.list
./wpt-oracle.py --wpt-manifest reftest/mask-fallback-wpt.list

# Run only a slice at a time
./reftest.py --wpt-manifest reftest/wpt-transforms.list --limit 10
./reftest.py --wpt-manifest reftest/wpt-transforms.list --offset 10 --limit 10
./wpt-oracle.py --wpt-manifest reftest/wpt-transforms.list --limit 10
./wpt-oracle.py --wpt-manifest reftest/wpt-transforms.list --offset 10 --limit 10
```

## Structure

```
*-test.sh              Shell runners (server setup, launch, result capture)
content/               HTML test pages with inline JS assertions
reftest/               Tiny screenshot reftest cases + references
reftest.py             Exact HAVI screenshot reftest runner
wpt-oracle.py          Browser-oracle screenshot validator (HAVI vs Chromium/Firefox)
test-prelude.bash      Shared infrastructure (server, launch, JS-result polling)
content/test-utils.js  Shared JS assertion helpers
```

Behavior tests keep using `window.testResults` and `run_js_tests`, which polls
that JSON summary via `havi-devtools-cli`.

## Screenshot harnesses

### `reftest.py`

`reftest.py` is the exact screenshot runner.

It:

- builds HAVI with `./mach-havi build`
- renders the test page with `havi --screenshot <png>`
- renders the reference page the same way
- compares PNG output by exact RGBA equality
- saves failure artifacts:
  - test PNG
  - reference PNG
  - diff PNG
  - HAVI logs for both renders

Inputs:

- explicit local manifests with `==` / `!=`
- one WPT-style file with `rel=match` / `rel=mismatch`
- a manifest listing WPT test files, one per line

Use `reftest.py` when:

- the local reference is trusted
- exact output equality is the goal
- a reduced repro should stay pixel-identical over time

### `wpt-oracle.py`

`wpt-oracle.py` is the browser-oracle validator.

It:

- builds HAVI with `./mach-havi build`
- renders HAVI test output
- renders Chromium test and reference output
- renders Firefox test and reference output
- crops all images to the common non-white content region from the origin
- compares using two mismatch metrics:
  - pixel mismatch percentage
  - structure mismatch percentage based on missing ink regions
- uses the larger of those metrics as the mismatch score
- classifies cases using browser disagreement:
  - `pass`
  - `bad-ref`
  - `likely-havi-error`

Current limitations:

- oracle mode only supports `==` / `rel=match` cases
- mismatch cases are skipped

Use `wpt-oracle.py` when:

- validating semantic rendering work against browsers
- working on transforms, perspective, matrix, matrix3d, 3D ordering
- working on sticky, overflow, clip, or other visual CSS behavior where a single ref may be noisy
- determining whether a failure is a likely HAVI bug or a questionable reference

Interpretation:

- `bad-ref` means browser disagreement against the nominal reference is large enough that the case is not reliable evidence by itself
- `likely-havi-error` means HAVI falls outside the Chromium/Firefox envelope and needs engine work

## Adding a test

### Behavior test

1. Create `content/foo.html` using `test-utils.js` (see existing pages for
   pattern)
2. Create `foo-test.sh` following the standard runner template:
   ```bash
   source "$(dirname "${BASH_SOURCE[0]}")/test-prelude.bash"
   TEST_NAME="foo"
   TEST_GROUP="footest"
   TEST_APP="testapp"
   start_server
   setup_acl "$TEST_GROUP" "$TEST_APP"
   create_key
   import_content "$SCRIPT_DIR/content" "$TEST_GROUP" "$TEST_APP"
   start_servo "hppr://$TEST_GROUP/$TEST_APP/foo.html"
   run_js_tests
   ```
3. `chmod +x foo-test.sh`

### Exact screenshot reftest

1. Add a test and reference pair under `reftest/`
2. Add a manifest line to `reftest/reftest.list`:
   ```text
   == test.html ref.html
   ```
   or
   ```text
   != test.html ref.html
   ```
3. Run `./reftest.py`

### WPT-based screenshot validation

1. Add the WPT test path to a manifest such as `reftest/wpt-transforms.list`
2. Use:
   ```bash
   ./reftest.py --wpt-manifest reftest/wpt-transforms.list
   ./wpt-oracle.py --wpt-manifest reftest/wpt-transforms.list
   ```
3. Inspect saved artifacts before deciding whether a failure is:
   - an engine bug
   - a bad reference
   - harmless browser disagreement

The Makefile auto-discovers all `*-test.sh` files.
