# HAVI Integration Tests

Each `*-test.sh` script starts an hpprd server, imports test content, launches
Servo, and runs JS assertions through the devtools protocol.

## Running

```bash
# All tests (parallel)
make -j4 test

# Single test
./address-test.sh
```

## Structure

```
*-test.sh          Shell runners (server setup, servo launch, result capture)
content/           HTML test pages with inline JS assertions
test-prelude.bash  Shared infrastructure (server, ACL, import, servo, runner)
content/test-utils.js  Shared JS assertion helpers
```

Each shell runner sources `test-prelude.bash`, sets up a server + content, opens
one HTML page in Servo, and calls `run_js_tests` which polls
`window.testResults`
via havi-debugger-cli.

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
