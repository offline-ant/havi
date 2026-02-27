# iOS simulator bring-up report (quick-fix status + next-agent plan)

## Scope
This report documents:
1. Everything already done in this branch to get `./mach-havi build --ios-simulator` building/running.
2. The **quick-fix runtime changes** I applied to unblock launch.
3. What the next agent must do to complete iOS GL compatibility for `paint.rs` / `webgl_thread.rs` (including threading/lifetime contract issues).

---

## Current status
- `./mach-havi build --ios-simulator` now builds and launches the app in Simulator.
- Proof screenshot captured at: `/tmp/havi-ios-sim.png`.
- App UI is visible and running.
- Current behavior is a **graceful-degradation quick fix** for WebGL on iOS, not final iOS GL parity.

---

## What was already done before this final quick fix (in this working tree)

### A) mozjs iOS cross-build unblock
Files:
- `/Users/claude/Projects/mozjs/mozjs-sys/build.rs`
- `/Users/claude/Projects/mozjs/mozjs-sys/makefile.cargo`

Key changes:
- Cleared leaked iOS deployment/toolchain env for SpiderMonkey host tools:
  - `IPHONEOS_DEPLOYMENT_TARGET`
  - `IPHONESIMULATOR_DEPLOYMENT_TARGET`
  - `SDKROOT`
- Added host-tool C/C++ flags (`HOST_CFLAGS`, `HOST_CXXFLAGS`) for cross-build correctness.
- Ensured host tools (e.g. `nsinstall`) are built for host architecture instead of iOS target.

Result:
- Moved mozjs from hard-fail to successful compile.

### B) iOS cfg/build-gap fixes across Servo/HAVI crates
Files modified earlier in this branch:
- `components/background_hang_monitor/lib.rs`
- `components/constellation/sandboxing.rs`
- `components/fonts/Cargo.toml`
- `components/fonts/font.rs`
- `components/fonts/platform/mod.rs`
- `components/script/dom/webgl/webglshader.rs`
- `components/shared/fonts/font_identifier.rs`
- `components/shared/paint/gl_device/mod.rs`
- `havi-protocols/src/pylon.rs`
- `ports/havishell/src/app/runtime.rs`

Examples of what was fixed:
- Added/expanded missing iOS cfg coverage where code assumed macOS/linux/windows only.
- Enabled iOS paths for shared paint/font modules.
- Fixed iOS return-type mismatch in constellation sandboxing path.
- Switched background hang monitor to dummy sampler on iOS.
- Adjusted script/webgl shader path to avoid mozangle dependence on iOS in current state.

Result:
- Build progressed to successful iOS simulator binary + app launch attempt.

---

## Runtime blockers encountered (and quick fix applied)

### Blocker 1: panic `Can't find resources directory`
Stack location:
- `ports/havishell/src/app.rs` (`ResourceReader::read` non-Android path)

Root cause:
- iOS app bundle layout did not satisfy the desktop-style executable-relative `resources/` scan path.

### Quick fix applied
File:
- `ports/havishell/src/app.rs`

Change:
- Replaced cfg split so iOS uses embedded resources (`include_bytes!`) same as Android.

Before:
- Desktop-style filesystem discovery was used for iOS.

After:
- `#[cfg(any(target_os = "android", target_os = "ios"))]` uses baked-in resource bytes.
- `#[cfg(not(any(target_os = "android", target_os = "ios")))]` keeps desktop file lookup.

Effect:
- Removed startup panic due to resource directory lookup.

---

### Blocker 2: panic `RenderingContext must provide gl_display_info for WebGL`
Panic site:
- `components/paint/paint.rs` in `register_rendering_context`

Root cause:
- `ports/havishell/src/app/runtime.rs` currently sets iOS `display_info = None`.
- WebGL thread then expected painter GL details to exist and panicked.

### Quick fix applied (graceful degradation)

#### 1) paint: stop panicking when display info is absent
File:
- `components/paint/paint.rs`

Change:
- Replaced `expect(...)` with optional insertion:
  - If `gl_display_info()` exists: insert painter GL details.
  - Else: log warning and continue.

#### 2) webgl_thread: make missing GL details non-fatal
File:
- `components/webgl/webgl_thread.rs`

Changes:
- `get_or_create_device_for_painter(...)` now returns `Option<Rc<GlDevice>>`.
- `create_webgl_context(...)` now returns `Err("WebGL is unavailable: missing GL display info for painter")` when absent.

Effect:
- App launches instead of crashing.
- WebGL creation on iOS currently fails cleanly instead of panic.

---

## Why this is still incomplete for iOS compatibility
Current iOS runtime path in `ports/havishell/src/app/runtime.rs` still does:
- `display_info = None` for iOS.

So the app is launchable, but iOS WebGL is intentionally degraded.

---

## Next-agent work: complete iOS GL compatibility (EAGL path)

## 1) Add a real iOS GL backend in `paint_api::gl_device`

### Problem
`components/shared/paint/gl_device/mod.rs` currently aliases iOS to EGL:
- `pub type GlDisplayInfo = egl::EglDisplayInfo` for iOS.

But Makepad iOS bridge is EAGL-based (`EaglRenderBridge`), not EGL.

### Required work
- Add new backend module:
  - `components/shared/paint/gl_device/eagl.rs`
- Define iOS display info type (example shape):
  - sharegroup/context handle needed to create shared contexts
  - proc loader function/source
- Implement backend API equivalent to other backends:
  - `new(info)`
  - `create_context(share_with)`
  - `destroy_context(ctx)`
  - `make_context_current(ctx)`
  - `get_proc_address(name)`
  - `gl_api() -> GLES`

- Update `mod.rs` cfg wiring:
  - iOS should use `eagl` backend + `GlDisplayInfo = eagl::EaglDisplayInfo`.

## 2) Extend Makepad bridge API so HAVI can construct iOS display info

### Problem
`makepad/platform/src/gl_render_bridge.rs` iOS exposes `GlApi`, loader, texture bridge, but no explicit display-info constructor contract for Servo WebGL.

### Required work
- Add iOS accessor(s) on `GlRenderBridge` (or equivalent constructor helper) needed to build `EaglDisplayInfo`.
- In `../makepad/platform/src/os/apple/metal.rs`, expose stable handles for:
  - root EAGL context and/or sharegroup needed for context sharing.

## 3) Wire runtime to provide real iOS `display_info`

File:
- `ports/havishell/src/app/runtime.rs`

### Required work
- Replace iOS `display_info = None` with real iOS display info builder.
- Keep non-iOS paths unchanged.

After this, `RenderingContext::gl_display_info()` should return `Some(...)` on iOS and WebGL thread should receive painter details.

## 4) Validate end-to-end WebGL context creation on iOS

- Ensure `WebGLThread::create_webgl_context` succeeds on iOS.
- Confirm no `missing GL display info` errors.
- Exercise WebGL page/canvas and verify rendering path is functional.

---

## Threading / lifetime contract issues that must be handled correctly

These are required for correctness when implementing the real iOS EAGL backend.

1. **Cross-thread context creation contract**
   - Main thread has Makepad render bridge context.
   - WebGL thread creates additional shared contexts.
   - The shared object passed across threads must be a stable shared primitive (sharegroup), not an invalid thread-bound temporary.

2. **Ownership/lifetime of ObjC handles**
   - Any EAGL context/sharegroup passed into paint/webgl code must remain valid for all dependent contexts.
   - Explicit retain/release ownership rules are needed; avoid dangling raw pointers.

3. **Context destruction ordering**
   - Destroy child/shared contexts before releasing root/sharegroup owner.
   - Ensure thread-local current context is cleared where required before destruction.

4. **Send/Sync boundary safety**
   - If raw pointers are wrapped for cross-thread use, document and enforce why this is safe.
   - Avoid exposing ObjC objects across threads without a clear synchronization and ownership model.

5. **Proc address loading stability**
   - `get_proc_address` must remain valid across threads/contexts used by WebGL thread.
   - Keep loader behavior consistent with context API being used.

6. **Single source of truth for shared-context root**
   - Avoid creating multiple unrelated roots accidentally (which breaks texture/resource sharing).
   - Ensure all iOS WebGL contexts for a painter derive from the same share root.

---

## Suggested implementation order for next agent
1. Implement `gl_device/eagl.rs` backend + cfg wiring in `gl_device/mod.rs`.
2. Expose required EAGL share-context/sharegroup info from Makepad bridge.
3. Build and pass iOS `display_info` in `ports/havishell/src/app/runtime.rs`.
4. Run `./mach-havi build --ios-simulator` and launch.
5. Verify WebGL context creation succeeds (no fallback error path).
6. Keep the quick-fix graceful handling until all tests pass; then decide whether to keep or tighten.

---

## Verification done for quick fix
- Command executed: `./mach-havi build --ios-simulator`
- iOS simulator app process confirmed running.
- Screenshot captured from booted simulator:
  - `/tmp/havi-ios-sim.png`

This confirms launch is unblocked; remaining work is full iOS WebGL compatibility via proper display-info/backend plumbing.
