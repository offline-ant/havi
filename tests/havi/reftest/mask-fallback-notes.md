# MaskFallback validation notes

This file records focused backend-local validation for projected-clip derivation-limit cases that route through `MaskFallback`.

## Focused WPT manifest

- `tests/havi/reftest/mask-fallback-wpt.list`

Current focused cases:

- `css/css-transforms/transform3d-perspective-001.html`
- `css/css-transforms/transform3d-matrix3d-001.html`
- `css/css-transforms/transform3d-translatez-001.html`
- `css/css-transforms/css-rotate-2d-3d-001.html`

## Current result

Backend-local mask compositing now executes instead of skipping child content.

Focused validation still shows transform-semantic failures:

- `transform3d-perspective-001.html`
  - exact reftest: fail
  - oracle: `likely-havi-error`
- `transform3d-matrix3d-001.html`
  - exact reftest: pass against its WPT reference in the reduced slice run
  - oracle: `likely-havi-error`

## Interpretation

These failures are not evidence that the backend-local mask compositing path is regressing into a rect fallback lie.
They remain part of the broader transform/reference-frame semantic gap already documented in status and handoff.

Backend-local conclusion for this phase:

- `MaskFallback` now performs real masked child compositing
- focused validation exists for derivation-limit cases
- current failures do not justify semantic-scene distortion or backend rect lies
- further fixes for these cases belong to later semantic architecture phases unless a strictly backend-local compositing defect is isolated
