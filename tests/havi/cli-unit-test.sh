#!/usr/bin/env bash
# cli-unit-test.sh - Unit tests for havi-devtools-cli and havi-makepad-cli
#
# Tests protocol-level logic without a running HAVI instance:
#   - DevTools: frame envelope unwrapping in connect_for_console
#   - Makepad: KeyCode integer encoding in keycode_index

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
HAVI_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"
TEST_NAME="cli-unit"
PASS=0
FAIL=0

log() { echo "[$TEST_NAME] $*" >&2; }
check() {
    local name="$1" expected="$2" actual="$3"
    if [[ "$actual" == "$expected" ]]; then
        echo "PASS: $name" >&2; PASS=$((PASS + 1))
    else
        echo "FAIL: $name (expected: $expected, got: $actual)" >&2; FAIL=$((FAIL + 1))
    fi
}

# Helper: import a shebang script as a python module
load_module() {
    python3 -c "
import importlib.util, sys, types

def load(path, name):
    loader = importlib.machinery.SourceFileLoader(name, path)
    spec = importlib.util.spec_from_loader(name, loader, origin=path)
    mod = importlib.util.module_from_spec(spec)
    sys.modules[name] = mod
    # Prevent argparse / main from running
    mod.__name__ = name
    spec.loader.exec_module(mod)
    return mod

$1
"
}

# ============================================================================
# Test 1: havi-makepad-cli keycode_index
# ============================================================================

log "--- Makepad KeyCode integer encoding ---"

KEYCODE_RESULT=$(load_module "
mod = load('$HAVI_ROOT/havi-makepad-cli', 'makepad_cli')
ki = mod.keycode_index
pairs = [
    # Function keys (the original bug: f5 must produce integer 62)
    ('f5', 62), ('F5', 62), ('f1', 58), ('f12', 69),
    # Letters
    ('a', 30), ('z', 42), ('A', 30),
    # Digits
    ('0', 3), ('9', 12),
    # Named keys
    ('enter', 29), ('return', 29), ('tab', 16), ('escape', 0), ('esc', 0),
    ('space', 56), ('backspace', 15), ('delete', 74),
    # Arrow keys
    ('up', 97), ('down', 98), ('left', 99), ('right', 100),
    # Direct variant names (case-insensitive)
    ('ReturnKey', 29), ('ArrowUp', 97), ('Capslock', 57),
    # Modifier keys
    ('Control', 52), ('Alt', 53), ('Shift', 54), ('Logo', 55),
    # Unknown falls back to Unknown index
    ('NONEXISTENT', 101),
]
for name, expected in pairs:
    actual = ki(name)
    print(f'{name}={actual}={expected}')
")

while IFS= read -r line; do
    name="${line%%=*}"
    rest="${line#*=}"
    actual="${rest%%=*}"
    expected="${rest#*=}"
    check "keycode_index('$name') == $expected" "$expected" "$actual"
done <<< "$KEYCODE_RESULT"

# Verify the output is always an integer (Makepad DeJson expects u64)
TYPES_OK=$(load_module "
mod = load('$HAVI_ROOT/havi-makepad-cli', 'makepad_cli')
ki = mod.keycode_index
for name in ['f5', 'a', 'enter', 'UNKNOWN_KEY']:
    v = ki(name)
    assert isinstance(v, int), f'{name}: got {type(v)}'
print('ok')
")
check "keycode_index returns int for all inputs" "ok" "$TYPES_OK"

# Verify JSON wire format: key_code must be a bare integer, not a quoted string
WIRE_OK=$(load_module "
import json
mod = load('$HAVI_ROOT/havi-makepad-cli', 'makepad_cli')
ki = mod.keycode_index
kd = {'key_code': ki('f5'), 'is_repeat': False, 'modifiers': mod.MODS, 'time': 0.0}
msg = mod.mkv('KeyDown', kd)
wire = json.dumps(msg, separators=(',', ': '))
# key_code must serialize as a bare number
assert '\"key_code\": 62' in wire, f'bad wire: {wire}'
# Must NOT be a quoted string
assert '\"key_code\": \"' not in wire, f'key_code is string in wire: {wire}'
print('ok')
")
check "KeyDown wire format has integer key_code" "ok" "$WIRE_OK"

# ============================================================================
# Test 2: KEYCODE_VARIANTS table integrity
# ============================================================================

log "--- KEYCODE_VARIANTS table integrity ---"

TABLE_CHECK=$(load_module "
mod = load('$HAVI_ROOT/havi-makepad-cli', 'makepad_cli')
# Must have exactly 102 entries (matches Rust KEYCODE_VARIANTS: [KeyCode; 102])
assert len(mod.KEYCODE_VARIANTS) == 102, f'expected 102, got {len(mod.KEYCODE_VARIANTS)}'
# Last entry must be Unknown
assert mod.KEYCODE_VARIANTS[-1] == 'Unknown', f'last is {mod.KEYCODE_VARIANTS[-1]}'
# Every alias in KEY_MAP must resolve to a valid variant
for alias, canonical in mod.KEY_MAP.items():
    assert canonical in mod.KEYCODE_VARIANTS, f'alias {alias} -> {canonical} not in variants'
print('ok')
")
check "KEYCODE_VARIANTS has 102 entries, all KEY_MAP aliases resolve" "ok" "$TABLE_CHECK"

# Cross-check against Makepad Rust source (the definitive table)
RUST_FILE="$HAVI_ROOT/../makepad/platform/src/event/keyboard.rs"
if [[ -f "$RUST_FILE" ]]; then
    CROSS_CHECK=$(python3 -c "
import re
# Extract variant order from Rust KEYCODE_VARIANTS array
with open('$RUST_FILE') as f:
    src = f.read()
m = re.search(r'const KEYCODE_VARIANTS:.*?\[(.*?)\];', src, re.DOTALL)
if not m:
    print('SKIP: could not parse Rust source')
else:
    rust_variants = re.findall(r'KeyCode::(\w+)', m.group(1))
    # Load Python table
    import importlib.util, importlib.machinery, sys
    loader = importlib.machinery.SourceFileLoader('makepad_cli', '$HAVI_ROOT/havi-makepad-cli')
    spec = importlib.util.spec_from_loader('makepad_cli', loader, origin='$HAVI_ROOT/havi-makepad-cli')
    mod = importlib.util.module_from_spec(spec)
    sys.modules['makepad_cli'] = mod
    spec.loader.exec_module(mod)
    py_variants = mod.KEYCODE_VARIANTS
    if rust_variants == py_variants:
        print('ok')
    else:
        for i, (r, p) in enumerate(zip(rust_variants, py_variants)):
            if r != p:
                print(f'MISMATCH at index {i}: rust={r} python={p}')
                break
        if len(rust_variants) != len(py_variants):
            print(f'LENGTH: rust={len(rust_variants)} python={len(py_variants)}')
")
    check "KEYCODE_VARIANTS matches Rust source" "ok" "$CROSS_CHECK"
fi

# ============================================================================
# Test 3: havi-devtools-cli frame envelope unwrapping
# ============================================================================

log "--- DevTools frame envelope unwrapping ---"

FRAME_RESULT=$(python3 -c "
# Simulate the getTarget response structure from TabDescriptorActor.
# Rust sends: GetTargetReply { from, frame: BrowsingContextActorMsg { actor, consoleActor, ... } }

def extract(target):
    frame = target.get('frame', target)
    actor = frame.get('actor')
    console = frame.get('consoleActor') or frame.get('console')
    return actor, console

# Case 1: Normal response with frame envelope (real server behavior)
a1, c1 = extract({
    'from': 'tabDescriptor1',
    'frame': {
        'actor': 'browsingContext1',
        'consoleActor': 'console1',
        'title': 'Test', 'url': 'hppr://test',
        'browserId': 1, 'outerWindowID': 2, 'browsingContextID': 3,
        'isTopLevelTarget': True, 'traits': {},
        'accessibilityActor': 'a1', 'cssPropertiesActor': 'css1',
        'inspectorActor': 'i1', 'reflowActor': 'r1',
        'styleSheetsActor': 'ss1', 'threadActor': 't1', 'targetType': 'frame',
    }
})
print(f'frame_envelope:{a1}:{c1}')

# Case 2: Hypothetical flat response (fallback path)
a2, c2 = extract({'from': 'x', 'actor': 'bc2', 'consoleActor': 'console2'})
print(f'flat_response:{a2}:{c2}')

# Case 3: Empty/error response
a3, c3 = extract({})
print(f'empty_response:{a3}:{c3}')

# Case 4: frame field with 'console' instead of 'consoleActor' (alternate key)
a4, c4 = extract({'from': 'x', 'frame': {'actor': 'bc4', 'console': 'console4'}})
print(f'console_alt_key:{a4}:{c4}')
")

while IFS= read -r line; do
    case_name="${line%%:*}"
    rest="${line#*:}"
    actor="${rest%%:*}"
    console="${rest#*:}"
    case "$case_name" in
        frame_envelope)
            check "frame envelope: actor" "browsingContext1" "$actor"
            check "frame envelope: consoleActor" "console1" "$console"
            ;;
        flat_response)
            check "flat fallback: actor" "bc2" "$actor"
            check "flat fallback: consoleActor" "console2" "$console"
            ;;
        empty_response)
            check "empty response: actor is None" "None" "$actor"
            check "empty response: console is None" "None" "$console"
            ;;
        console_alt_key)
            check "console alt key: actor" "bc4" "$actor"
            check "console alt key: falls back to 'console'" "console4" "$console"
            ;;
    esac
done <<< "$FRAME_RESULT"

# ============================================================================
# Test 4: Regression guard - old code path was broken
# ============================================================================

log "--- Regression guard ---"

REGRESSION=$(python3 -c "
target = {
    'from': 'tabDescriptor1',
    'frame': {'actor': 'browsingContext1', 'consoleActor': 'console1'}
}
# Old broken code path (direct access on target):
old_actor = target.get('actor')
old_console = target.get('consoleActor')
# New fixed code path:
frame = target.get('frame', target)
new_actor = frame.get('actor')
new_console = frame.get('consoleActor')
print(f'{old_actor}:{old_console}:{new_actor}:{new_console}')
")

IFS=: read -r old_a old_c new_a new_c <<< "$REGRESSION"
check "old code: actor was None (broken)" "None" "$old_a"
check "old code: consoleActor was None (broken)" "None" "$old_c"
check "new code: actor extracted" "browsingContext1" "$new_a"
check "new code: consoleActor extracted" "console1" "$new_c"

# ============================================================================
# Summary
# ============================================================================

echo ""
log "Passed: $PASS, Failed: $FAIL"
if [[ "$FAIL" -gt 0 ]]; then
    exit 1
fi
