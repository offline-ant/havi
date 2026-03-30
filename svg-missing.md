# SVG missing

Remaining SVG work is additive filler inside the native one-engine pipeline.
No second renderer path or compatibility bridge work remains.

## Rendering

- full SVG filter execution
- long-tail mask semantics beyond the current basic geometry subset
- long-tail marker semantics beyond the current basic path subset
- long-tail pattern semantics beyond the current basic path/text subset

## Layout and text

- `lengthAdjust="spacingAndGlyphs"`
- full `foreignObject` HTML formatting-context embedding
- full `textPath` path-following semantics

## DOM and queries

- long-tail SVG value-object mutation semantics
- full SVG geometry and text query semantics beyond the current placeholder-backed methods
- long-tail DOM API parity beyond the current Phase 1 SVG spine
