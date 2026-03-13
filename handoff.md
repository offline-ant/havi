# HAVI renderer handoff

## Goal

Finish the structural renderer migration and make `file:///home/devops/Projects/forge/site/presentation/index.html` render correctly in HAVI, then produce a `havi-makepad-cli` screenshot that visibly shows the Reveal.js first slide.

## Current status

This is **not finished**.

What is working now:
- focused `havi-render` tests pass
- `./mach-havi check -- -p havishell` passed earlier before the later debugging passes
- a plain local file page renders correctly
- a minimal transformed subtree page renders correctly
- a minimal Reveal-like page with:
  - transformed `.slides`
  - `overflow:hidden` on `.reveal`
  - absolutely positioned `section.present`
  - sibling `.backgrounds`
  also renders correctly

What is still broken:
- the real Reveal.js presentation page still renders incorrectly
- after removing the pause overlay, the page shows a black viewport with corrupted / overlapping text clustered at the lower-right
- latest screenshot proving this is:
  - `/tmp/havi-presentation-3dignore.png`

That means the remaining bug is not the original 2D transformed-subtree bug. The remaining bug is in a more specific interaction that the real Reveal page uses and the local reduced pages do not fully reproduce.

## Files currently modified

In `havi/`:
- `crates/render/src/frame_builder.rs`
- `crates/render/src/frame_tree.rs`
- `crates/render/src/hit_test.rs`
- `crates/render/src/lib.rs`
- `crates/render/src/makepad_builder.rs`
- `crates/render/src/text.rs`
- `crates/render/src/transform.rs`
- `handoff.md` (this file)

In `makepad/`:
- `platform/src/os/linux/linux_media.rs`

## Important environment / runtime notes

### 1. PulseAudio panic workaround

This environment crashes on launch unless PulseAudio is bypassed.

I added a local Linux Makepad escape hatch in:
- `makepad/platform/src/os/linux/linux_media.rs`

Use this for all local HAVI runs here:

```bash
MAKEPAD_DISABLE_PULSE_AUDIO=1
```

Without it, HAVI often panics at:
- `Pulse audio pa_context_connect failed`

### 2. Use local file repros aggressively

This was useful and should continue.

Helpful launch form:

```bash
cd /home/devops/Projects/forge/havi
MAKEPAD_DISABLE_PULSE_AUDIO=1 \
HAVI_PYLON_MODE=none \
HAVI_URL="file:///tmp/target-test.html" \
./mach-havi run --makepad-socket
```

### 3. Do not trust only DevTools DOM state

On the real presentation page, DevTools often shows correct DOM / computed styles while the Makepad screenshot is still wrong.

The rendering bug is in the HAVI renderer layer, not in Reveal layout or DOM state.

## Screenshots produced

Useful screenshots:
- `/tmp/havi-simple-shot.png`
  - plain text page, renders correctly
- `/tmp/havi-transform-shot3.png`
  - minimal transformed subtree, renders correctly enough for current structural fix
- `/tmp/havi-revealish.png`
  - minimal Reveal-like layout, renders correctly
- `/tmp/havi-zorder.png`
  - `.slides` above sibling `.backgrounds`, renders correctly
- `/tmp/havi-presentation-3dignore.png`
  - current failing real Reveal page

## Local repro HTML files created during investigation

These are outside the repo but very useful:
- `/tmp/havi-simple.html`
- `/tmp/havi-transform.html`
- `/tmp/havi-revealish.html`
- `/tmp/havi-zorder.html`

They were used to isolate the bug from:
1. plain text
2. 2D transform
3. transform + overflow clip + absolute section
4. z-order with `.backgrounds`

All four render correctly with current code.

## Commands already run

### Tests / checks

```bash
cd /home/devops/Projects/forge/havi && cargo test -p havi-render
cd /home/devops/Projects/forge/havi && ./mach-havi check -- -p havishell
```

`cargo test -p havi-render` currently passes.

### Launches

Typical working launch pattern used:

```bash
cd /home/devops/Projects/forge/havi && \
MAKEPAD_DISABLE_PULSE_AUDIO=1 \
HAVI_PYLON_MODE=none \
HAVI_URL="file:///home/devops/Projects/forge/site/presentation/index.html" \
./mach-havi run --makepad-socket
```

### Screenshots

Use:

```bash
cd /home/devops/Projects/forge/havi && \
./havi-makepad-cli --socket "$HAVI_MAKEPAD_SOCKET" screenshot /tmp/out.png
```

### DevTools

Use:

```bash
cd /home/devops/Projects/forge/havi && \
HAVI_DEVTOOLS=127.0.0.1:<port> ./havi-devtools-cli --timeout 10 eval '...'
```

## Structural changes already made

### 1. Frame tree paint order is now explicit

`frame_tree.rs` now has:
- `FramePaintCommand`
- `paint_list`

This was necessary so frame subtrees paint in the same interleaved order as scene construction, instead of the old:
- paint all items in a frame
- then all child frames

That older behavior broke stacking / duplication semantics.

### 2. Scene builder owns more of the spatial rebasing

`frame_builder.rs` now carries:
- `frame_id`
- `clip_id`
- `local_origin`
- `origin_basis`

and builds local origins by subtracting the owning frame basis.

It also now tracks:
- `fragment_origins`
- `box_origins`

### 3. Reference-frame anchor rebasing changed

The key working change for the 2D transform repro was:
- keep the child frame transform as `translation(anchor) * css_transform`
- set `origin_basis = anchor`
- so owner subtree items paint in anchor-local coordinates inside that transformed frame

This fixed the earlier bug where transformed subtree descendants were double-offset or painted in root coordinates.

### 4. Clip ownership is more structural

`clip_tree.rs` remains explicit.

`makepad_builder.rs` now distinguishes:
- local clips applied when painting items in the owning frame
- child-frame composition clips applied around `paint_frame(child)` in the parent frame

This was needed because Makepad clips are pre-transform local clips. Mapping ancestor clips directly into transformed local frame space was producing bogus clip ranges.

### 5. Draw-list reuse is deliberately cleared each render

In `lib.rs`, both `render_fragments(...)` and `render_fragments_clipped(...)` now do:

```rust
frame_draw_lists.clear();
```

This was a deliberate correctness move.

The investigation previously showed stale draw-list reuse was likely contributing to wrong output when Reveal mutates the DOM after startup.

This is a brute-force correctness step, not a final optimization.

### 6. 3D / perspective reference frames are currently ignored

In `transform.rs`, `compute_css_reference_frame_matrix(...)` now returns `None` when the computed matrix is genuinely 3D:

```rust
if is_3d_matrix(&matrix) {
    return None;
}
```

This was added after debug logs showed Reveal was generating non-2D reference-frame matrices for some slide nodes, and those were producing pathological results.

This does **not** finish the issue. After this change, the real page improved from fully black to a corrupted lower-right text pile.

## Debug logging already added

There are temporary debug logs gated by:

```bash
HAVI_RENDER_DEBUG_TRANSFORM=1
```

These logs are currently present in:
- `frame_builder.rs`
- `makepad_builder.rs`
- `text.rs`

They print things like:
- reference frame matrices
- item local origins
- clip-chain mappings
- text draw positions
- glyph run stats

### Important logged discovery

The most important debug discovery so far:

The real Reveal page creates additional reference-frame matrices beyond the simple `.slides` 2D matrix. Example captured log:

```text
[render] ref-frame ... mat=[1.0, 0.0, 0.0, 0.0,
                           0.0, 1.0, 0.0, 0.0,
                           -1.6, -0.52444446, 1.0, -0.0016666667,
                           640.0, 555.0, 0.0, 1.0]
```

That is a real 3D / perspective-style matrix. Ignoring 3D reference frames improved the output, which strongly suggests the remaining issue is tied to Reveal transition / perspective semantics.

### Another important logged discovery

Before the `push_local_clip_chain(...)` split, transformed child frames were receiving absurd local clip ranges such as:

```text
Rect { pos: (-2254.84, -728.54), size: (1289.13, 729.16) }
```

That effectively clipped local y to `<= ~0.6`, which explained the all-black transformed output.

Moving ancestor clip application to child-frame composition fixed that class of bug.

## Current best hypothesis for the remaining bug

The remaining bad rendering on the real page is likely caused by one or both of these:

### A. Reveal transition / perspective nodes still affect the scene

Even after ignoring true 3D matrices for reference-frame creation, Reveal still has extra nodes / wrappers whose layout and paint ordering differ from the reduced test pages.

The corrupted lower-right pile in `/tmp/havi-presentation-3dignore.png` looks like multiple present/future slide text runs painting without the intended transition-space isolation.

### B. 3D / perspective nodes are currently dropped too bluntly

By returning `None` for 3D reference frames, the renderer avoids pathological transforms, but it also drops the structural isolation boundary those nodes may still need.

That can allow multiple slide subtrees to paint into the same 2D frame and overlap at the lower-right.

In other words:
- ignoring the 3D matrix stops the catastrophic transform
- but also collapses some subtree ownership that Reveal relies on

## What the next agent should do

### 1. Continue with local file reductions first

Start from the working `/tmp/havi-revealish.html` and add the next missing Reveal features one at a time until the corruption appears.

Suggested progression:
1. sibling `.backgrounds`
2. slide number / progress elements
3. nested vertical slide stack (`section > section`)
4. Reveal transition CSS / classes
5. perspective on ancestor
6. whatever exact style causes the 3D matrix log above

Do **not** jump straight back to the full presentation until you have a reduced case that reproduces the lower-right pile.

### 2. Use `HAVI_RENDER_DEBUG_TRANSFORM=1`

This is already wired up. Use it.

Especially inspect:
- when a 3D matrix would have created a frame
- which frame IDs own the corrupted text
- whether multiple slide sections are now painting in the same frame after the 3D-ignore fallback

### 3. Add focused tests

Add unit tests in `havi-render` for the structural rules you settle on.

Likely good tests:
- child-frame composition applies ancestor clip in parent space, not child local inverse space
- a child frame with inherited clip does not reapply the same ancestor clip locally
- reference-frame creation skips truly 3D matrices
- if you add a fallback ownership rule for 3D/perspective nodes, test it directly

### 4. Probably implement a disciplined fallback for 3D / perspective nodes

My current guess is that the next correct move is **not** to fully implement 3D.

It is more likely to be:
- detect nodes with true 3D / perspective
- still create a structural frame / isolation boundary for that subtree
- but paint it with a disciplined 2D fallback instead of dropping the frame entirely

That would preserve subtree ownership / clip / paint-once invariants without pretending full 3D is supported.

A likely design:
- keep a frame node for 3D owners
- use identity or 2D-projected matrix for painting
- keep the subtree isolated so multiple slides do not collapse into one parent frame

### 5. Remove temporary debug logs when done

Current debug logs are noisy and should not remain in the final state.

## Known-good / known-bad checkpoints

### Known good
- `/tmp/havi-simple.html`
- `/tmp/havi-transform.html`
- `/tmp/havi-revealish.html`
- `/tmp/havi-zorder.html`

### Known bad
- real page:
  - `file:///home/devops/Projects/forge/site/presentation/index.html`
- current screenshot:
  - `/tmp/havi-presentation-3dignore.png`

## Important repo rules still in effect

From AGENTS / task context:
- do not commit
- keep using `read`, not `cat` / `sed`, for inspection
- use `./mach-havi build/check/run`, never `cargo build` in `havi/`
- use `havi-makepad-cli` for screenshots
- update focused tests and docs/spec if final behavior/architecture changes materially

## Minimal launch / inspect recipe

```bash
cd /home/devops/Projects/forge/havi
MAKEPAD_DISABLE_PULSE_AUDIO=1 \
HAVI_RENDER_DEBUG_TRANSFORM=1 \
HAVI_PYLON_MODE=none \
HAVI_URL="file:///home/devops/Projects/forge/site/presentation/index.html" \
./mach-havi run --makepad-socket
```

Then from another shell, use the printed values:

```bash
cd /home/devops/Projects/forge/havi
HAVI_DEVTOOLS=127.0.0.1:<port> ./havi-devtools-cli --timeout 10 eval '...'
./havi-makepad-cli --socket /tmp/havi-makepad-XXXX.sock screenshot /tmp/out.png
```

## Bottom line

The original 2D transform ownership bug is mostly fixed.

The remaining failure is now a narrower Reveal-specific case involving:
- additional transition/perspective structure
- subtree ownership under those nodes
- and possibly the lack of a safe 3D fallback frame/isolation policy

Do not restart from scratch. The current branch already has the important structural pieces in place.
