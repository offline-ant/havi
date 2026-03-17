# HAVI Integration Tests

Each `*-test.sh` script starts an hpprd server, imports test content, launches
HAVI, and runs either JS assertions through the DevTools protocol or screenshot
assertions through the native `--screenshot` path.

## Running

```bash
# All tests (parallel)
make -j4 test

# Single behavior test
./address-test.sh

# Tiny rendering suite
./reftest.py

# Curated WPT transform / 3D reftests
./reftest.py --wpt-manifest reftest/wpt-transforms.list

# Run only 10 cases at a time, or a specific slice
./reftest.py --wpt-manifest reftest/wpt-transforms.list --limit 10
./reftest.py --wpt-manifest reftest/wpt-transforms.list --offset 10 --limit 10
```

## Structure

```
*-test.sh          Shell runners (server setup, launch, result capture)
content/           HTML test pages with inline JS assertions
reftest/           Tiny screenshot reftest cases + references
reftest.py         Tiny screenshot reftest runner
test-prelude.bash  Shared infrastructure (server, launch, JS-result polling)
content/test-utils.js  Shared JS assertion helpers
```

Behavior tests keep using `window.testResults` and `run_js_tests`, which polls
that JSON summary via `havi-devtools-cli`.

Rendering tests use `reftest.py`, which sets `HAVI_URL`, runs
`havi --screenshot <output.png>` for the test and reference pages, then compares
PNG output exactly.

`reftest.py` also supports WPT-style files directly:

- `--wpt-test <path>` parses one test file for `rel=match` and `rel=mismatch`
- `--wpt-manifest <file>` reads one WPT test path per line and expands each file
  into one or more reftest cases

Use this for focused layout work such as transforms, perspective, matrix3d,
and other 3D rendering cases.

`reftest.py` runs at most 10 cases by default. Use `--offset` and `--limit`
to work through a manifest in small focused batches.

## Adding a test

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

The Makefile auto-discovers all `*-test.sh` files.
