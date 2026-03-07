Servo Page Load Time Test
==============

# Prerequisites

* Python3

# Basic Usage

`./mach test-perf` runs a performance test. Results go to
`etc/ci/performance/output/`. Compare results with
`python test_differ.py`. Run `-h` for instructions.

# Setup for CI machine
## CI for Servo

* Set env vars `TREEHERDER_CLIENT_ID` and
  `TREEHERDER_CLIENT_SECRET`
* Run `./mach test-perf --submit` to submit to Perfherder.

## CI for Gecko

* Install Firefox Nightly in your PATH
* Download [geckodriver](https://github.com/mozilla/geckodriver/releases)
  and add it to `PATH`
* `export FIREFOX_BIN=/path/to/firefox`
* `pip install selenium`
* Run `python gecko_driver.py` to test
* Run `test_all.sh --gecko --submit`
  (omit `--submit` to skip perfherder upload)

# How it works

* Testcases are from tp5, each runs 20 times (median).
* Some tests hang Servo; those are disabled.
  See https://github.com/servo/servo/issues/11087
* Each testcase is a Perfherder subtest; summary is the
  geometric mean.
* This is not the Talos TP5 test. Do NOT compare Servo
  and Gecko performance from these results.

# Comparing the performance before and after a patch

* Run the test once before you apply a patch, and once after you apply it.
* `python test_differ.py output/perf-<before time>.json output/perf-<after time>.json`
* Green = decreased load time, Blue = increased.

# Add your own test

* You can add two types of tests: sync test and async test
  * sync test: measures page load time, exits on load.
  * async test: custom JS time markers. See
    `page_load_test/example/example_async.html`.
* Add your test html to `page_load_test/`. Example:
  `page_load_test/example/example.html`
* Add or modify a manifest, e.g.
  `page_load_test/example.manifest`
* Add the lines like this to the manifest:

```
# Pages got served on a local server at localhost:8000
# Test case without any flag is a sync test
http://localhost:8000/page_load_test/example/example_sync.html
# Async test must start with a `async` flag
async http://localhost:8000/page_load_test/example/example.html
```
* Update `MANIFEST=...` in `test_all.sh` to point to the
  new manifest.

# Unit tests

Run all unit tests (including 3rd-party) with
`python -m pytest`.

Individual test can be run by `python -m pytest <filename>`:

* `test_runner.py`
* `test_submit_to_perfherder.py`

# Advanced Usage

## Reducing variance

Running the same performance test results in a lot of variance, caused
by the OS the test is running on. Experimentally, the things which
seem to tame randomness the most are a) disbling CPU frequency
changes, b) increasing the priority of the tests, c) running one one
CPU core, d) loading files directly rather than via localhost http,
and e) serving files from memory rather than from disk.

First run the performance tests normally (this downloads the test suite):
```
./mach test-perf
```
Disable CPU frequency changes, e.g. on Linux:
```
sudo cpupower frequency-set --min 3.5GHz --max 3.5GHz
```
Copy the test files to a `tmpfs` file,
such as `/run/user/`, for example if your `uid` is `1000`:
```
cp -r etc/ci/performance /run/user/1000
```
Then run the test suite on one core, at high priority,
using a `file://` base URL:
```
sudo nice --20 chrt -r 99 sudo -u *userid* taskset 1 ./mach test-perf --base file:///run/user/1000/performance/
```
This takes variance down to under 5% per test and under
0.5% total.

(IRC logs:
 [2017-11-09](https://mozilla.logbot.info/servo/20171109#c13829674) |
 [2017-11-10](https://mozilla.logbot.info/servo/20171110#c13835736)
)

## Test Perfherder Locally

To test `submit_to_perfherder.py` without production
credentials, set up a local treeherder VM. Skip this if
you don't need to test submission.

* Add `192.168.33.10    local.treeherder.mozilla.org` to `/etc/hosts`
* `git clone https://github.com/mozilla/treeherder; cd treeherder`
* `vagrant up`
* `vagrant ssh`
  * `./bin/run_gunicorn`
* Outside vm, open `http://local.treeherder.mozilla.org`
  and log in to create an account
* `vagrant ssh`
  * `./manage.py create_credentials <user> <email> "desc"`
    — email must match your logged-in user. Log in via
    the Web UI first.
  * Set env vars `TREEHERDER_CLIENT_ID` and
    `TREEHERDER_CLIENT_SECRET`
