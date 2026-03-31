#!/bin/bash
# Type-check each page JS file independently against havi.d.ts.
# Each file is a separate page — they share type definitions but
# never run together, so they must be checked in isolation.
set -euo pipefail

DIR="$(cd "$(dirname "$0")" && pwd)"
ERRORS=0

for js in "$DIR"/*.js; do
    if ! bun x tsc --noEmit --strict --target ES2022 --lib ES2022,DOM \
         --checkJs --allowJs "$DIR/hppr-html.d.ts" "$DIR/havi.d.ts" "$js" 2>&1; then
        ERRORS=$((ERRORS + 1))
    fi
done

if [ "$ERRORS" -gt 0 ]; then
    echo "FAIL: $ERRORS file(s) had type errors"
    exit 1
fi

echo "OK: all page scripts pass type checking"
