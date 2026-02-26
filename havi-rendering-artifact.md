# HAVI Tab Bar Rendering Artifact (Linux/OpenGL) — Resolved

## Symptom

Dark rectangular blocks appeared in the tab bar on Linux/Wayland, covering
parts of tab label text. Stable, did not flicker. Block color was the live DSL
template default (#2a2a2a), not the runtime-applied uniform value.

Not reproduced on macOS (Metal) or Windows (D3D11).

## Root Cause

On Linux, `GlRenderBridge` wraps makepad's own EGL context (same context,
not a separate one). Servo's `webview.paint()` calls WebRender which does
heavy GL operations — binds VAOs, UBOs, shader programs, framebuffers,
changes blend/scissor/color mask state — all on the shared context.

When makepad's renderer ran afterward, it found dirty GL state. Bound VAOs
from WebRender caused makepad's `glBufferData` calls to write into wrong
buffers, producing ghost DrawQuad instances with template-default uniform
values at wrong positions.

On macOS (CGL + Metal) and Windows (ANGLE + D3D11), the bridge creates a
separate GL context, so Servo's GL state never leaks into makepad's.

## Fix

`restore_gl_context()` on Linux/Android now resets critical GL bindings to
defaults after external rendering:

- `glBindVertexArray(0)`
- `glBindBuffer(ARRAY_BUFFER/ELEMENT_ARRAY_BUFFER/UNIFORM_BUFFER, 0)`
- `glBindFramebuffer(0)`, `glBindRenderbuffer(0)`
- `glUseProgram(0)`
- `glActiveTexture(TEXTURE0)`, `glBindTexture(TEXTURE_2D, 0)`
- `glDisable(SCISSOR_TEST)`, `glDisable(BLEND)`
- `glColorMask(1,1,1,1)`, `glDepthMask(1)`

Commit: makepad `gl_render_bridge: reset GL state in restore_gl_context on Linux/Android`

## Cost

The reset runs only when the application calls `restore_gl_context()` — once
per frame in HAVI after Servo paint. Pure makepad programs never call it.
14 GL state writes, ~1-5 microseconds total.

## Why not a separate EGL context?

Separate shared EGL contexts isolate GL state but share GPU resources
(textures, buffers). VAOs are context-local per the GL spec, so Servo's
VAOs (created during init) are only valid on the context they were created on.
Servo doesn't call `make_current` internally — it assumes one context for its
lifetime. Wrapping every Servo entry point (init, spin, paint, resize, input)
with context switches would be fragile and invasive. The state reset achieves
the same isolation with a single function.

## Bisection evidence

| Change | Artifact? |
|--------|-----------|
| Baseline (all Servo rendering) | Yes |
| Disable `webview.paint()` + `rc.present()` + `restore_gl_context()` | No |
| `webview.paint()` only (no present, no restore) | Yes |
| `rc.present()` + `restore_gl_context()` only (no paint) | No |
| `webview.paint()` + `restore_gl_context()` (no present) | Yes |
| Full rendering + GL state reset in `restore_gl_context` | No |

`webview.paint()` alone is sufficient to trigger the artifact. The GL state
reset in `restore_gl_context` eliminates it.
