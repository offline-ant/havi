# Readme

Run from the root servo directory with:
```shell
uv run etc/blink-perf-test-runner/main.py SERVO_BINARY [--webdriver port] [--prepend name]
```
It will return a results.json in bencher bmf format.
Not every test currently produces an output.
`--prepend` prepends e.g. a cargo profile name to result
keys, useful when uploading to bencher to distinguish
measurements across profiles.
