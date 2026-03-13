# HAVI 3D compositor handoff

## Goal

Finish the renderer/compositor migration so that:

- HAVI uses the new Makepad compositor scheme for true CSS 3D / perspective cases
- ordinary 2D content stays on the existing fast path
- `file:///home/devops/Projects/forge/site/presentation/index.html` renders correctly under the new compositor architecture
- the implementation is structurally clean and durable
- the final state is tested, with focused tests for the compositor path and a successful real-page validation using the Reveal presentation

This is **not** asking for complete browser-wide CSS 3D perfection. It is asking for a solid architecture with correct handling for the Reveal presentation and a clean path for future edge-case work.

The accepted end state is:

- architecture is correct and easy to extend
- Reveal presentation renders correctly with the new compositor path
- remaining unimplemented edge cases are isolated and explicit

## Important current state

### 1. Renderer state in `havi/crates/render`

The renderer was previously repaired enough to make the Reveal presentation render by using a disciplined 2D fallback.

That work established the correct broad structure:

- `stacking_context.rs` — CSS paint order / stacking context classification
- `frame_tree.rs` — explicit frame tree and paint list
- `frame_builder.rs` — scene construction and frame ownership
- `makepad_builder.rs` — current Makepad 2D painting path
- `transform.rs` — CSS transform extraction and fallback handling

That path is now good enough to validate paint order and ownership, but it is **not** the final architecture for true CSS 3D.

The remaining strategic decision has already been made:

- keep the 2D draw-list path for ordinary content
- introduce a dedicated compositor path for true 3D / perspective cases
- do **not** keep expanding the 2D fallback as the long-term solution

### 2. Makepad compositor investigation already completed

Read and use:

- `../makepad/makepad-3d-css.md`

That document is the main source of truth for the compositor direction. It already includes direct Makepad source review and a concrete recommendation.

The conclusion from that investigation is:

- Makepad already has enough low-level GPU primitives
- the missing layer is a compositor abstraction, not basic rendering capability
- the right direction is a companion compositor layer, not forcing browser semantics into Makepad core

### 3. Implementation progress already completed

Per user note, implementation has reached:

- **up to and including phase 2**
- `../makepad/compositor` exists

That means the next work is **not** “design the compositor from scratch”.
It is to integrate it correctly into HAVI and complete the architecture.

## Key references

### Existing renderer handoff / local debugging history

Read:

- `./handoff.md`

That file explains the previous structural renderer fixes and the Reveal-specific bug investigation.
It is useful background, but the final system should move past the old 2D-fallback-centric approach.

### Makepad compositor design / capability review

Read completely:

- `../makepad/makepad-3d-css.md`

Important conclusions from that document:

- Makepad already has:
  - 4x4 matrix math
  - child passes / render-to-texture
  - depth buffers
  - arbitrary geometry and custom shaders
  - enough low-level pieces for projected textured quads
- Makepad does **not** already have:
  - browser-style compositor scene semantics
  - projected clipping as a first-class high-level feature
  - preserve-3d group handling at the API layer
- Recommended direction:
  - keep Makepad core generic
  - use companion compositor primitives
  - keep CSS semantics in HAVI

### Servo / Gecko reference behavior

Use these as semantic references, not as code to copy blindly.

#### Servo reference-frame construction

Files:

- `../servo-mainline/components/layout/display_list/stacking_context.rs`
- `../servo-mainline/components/layout/style_ext.rs`
- `../servo-mainline/components/layout/fragment_tree/box_fragment.rs`

Key observed behavior:

- transform / perspective / `preserve-3d` establish stacking contexts and containing blocks
- transform matrices preserve full 4x4 semantics
- perspective is represented as a perspective reference frame, not flattened away
- some layout-overflow details remain incomplete even in Servo, so do not assume every geometry helper is perfect

#### Gecko / WebRender compositor behavior

Files:

- `/mnt/llm/gecko-dev/layout/painting/nsDisplayList.cpp`
- `/mnt/llm/gecko-dev/gfx/webrender_bindings/src/bindings.rs`
- `/mnt/llm/gecko-dev/gfx/wr/webrender/src/spatial_node.rs`
- `/mnt/llm/gecko-dev/gfx/wr/webrender/src/scene_building.rs`
- `/mnt/llm/gecko-dev/gfx/wr/webrender/src/picture.rs`

Key observed behavior:

- perspective becomes explicit compositor/spatial reference-frame state
- preserve-3d participation is tracked in the scene/compositor layer
- incompatible coordinate systems are introduced once perspective / non-2D transforms appear
- preserve-3d groups require special picture/surface handling
- browsers generally stay on cheap 2D-compatible paths until real 3D semantics are required

This is the model to emulate architecturally:

- ordinary content: existing 2D path
- 3D/perspective content: escalated compositor path

## Architectural target

The target architecture should be:

### 1. Two rendering paths, one renderer

HAVI should remain a single renderer logically, but with two backend execution paths:

#### A. Ordinary 2D path

Use existing draw-list based rendering for:

- normal CSS boxes
- text
- images
- clips that remain axis-aligned in the existing model
- ordinary transforms that fit the current 2D frame model cleanly

This path should remain fast and simple.

#### B. 3D compositor path

Use `../makepad/compositor` for subtrees that require true CSS 3D semantics, including:

- ancestor `perspective`
- `transform-style: preserve-3d`
- transform chains that cannot be represented safely as a local 2D affine frame without semantic loss

This compositor path should own:

- projected textured quads and/or direct projected subtree nodes
- flattening boundaries via offscreen surfaces
- preserve-3d subtree participation
- backface visibility
- projected clip handling appropriate for this compositor model
- depth-tested ordering inside a preserve-3d context

### 2. CSS semantics stay in HAVI

Do **not** push browser semantics into Makepad.

HAVI should continue to own:

- stacking-context classification
- flattening-boundary decisions
- transform-style / perspective interpretation
- subtree ownership
- which parts flatten into surfaces
- which parts remain live in a preserve-3d group

Makepad / `makepad-compositor` should remain generic rendering infrastructure.

### 3. The scene model must become explicit

The current frame tree is a good structural base, but the final architecture likely needs one more explicit layer for 3D compositor participation.

The implementation does **not** need a giant redesign, but it does need a clear internal representation for the compositor path.

At minimum HAVI should have explicit internal concepts equivalent to:

- `CompositorSurface`
- `CompositorNode`
- `CompositorGroup`
  - `Flat`
  - `Preserve3d`

Those names do not have to be literal, but the semantics should be explicit in code.

### 4. Flattening boundaries must be first-class

The most important architectural rule is:

- preserve-3d descendants in the same context stay together in a shared compositor context
- flattening boundaries render to a surface, then submit that surface as a transformed node in the parent context

This is the main shift away from the current fallback path.

## What the final product should look like

A clean final system should have:

### In `havi/crates/render`

- one clear place where content is classified into:
  - ordinary 2D frame path
  - compositor 3D path
- one clear place where flattening decisions are made
- one clear place where compositor nodes/surfaces/groups are built
- the existing 2D painter kept largely intact for non-3D content
- minimal ad hoc transform fallback logic left in the final path

### In `../makepad/compositor`

Use the companion compositor crate as the backend for:

- projected surfaces/nodes
- offscreen surfaces
- depth-aware composition
- clip planes or equivalent projected clip support

Do not re-implement compositor logic independently in HAVI if the crate already provides the needed abstraction.

### Testing

- focused unit tests in `havi-render`
- possibly focused tests for `../makepad/compositor` if missing
- real-page validation using `site/presentation/index.html`
- screenshot proof using `havi-makepad-cli`

## Phase plan

Use **5 phases**.
That is the right split for this work: enough structure to keep the architecture clean, not so many phases that progress fragments.

---

## Phase 1 — Audit and formalize the scene boundary

### Goal

Determine exactly where the current renderer transitions from pure scene construction to backend painting, and insert the compositor scene boundary cleanly.

### Tasks

1. Read these files fully:

- `crates/render/src/frame_builder.rs`
- `crates/render/src/frame_tree.rs`
- `crates/render/src/makepad_builder.rs`
- `crates/render/src/transform.rs`
- `crates/render/src/stacking_context.rs`

2. Read `../makepad/compositor` completely.
Understand its actual API, not just the design document.

3. Decide the narrowest clean internal representation HAVI needs for the compositor path.
Likely options:

- extend the existing frame scene with compositor-specific commands
- or add a dedicated compositor scene representation produced after frame building

4. Choose the clean boundary.
Recommended default:

- keep `frame_builder` responsible for structural ownership / stacking / clip lineage
- add a second scene-building layer that derives compositor groups and surfaces from that structure

5. Identify and isolate current fallback logic that should disappear from the final architecture.
Most likely in:

- `transform.rs`
- `frame_builder.rs`

### Deliverable

A clean internal scene boundary in code, even if phase 1 only scaffolds the types and does not fully wire behavior yet.

### Notes

Do not do a giant rewrite here.
This phase is about placing the boundary so the next phases do not become messy.

---

## Phase 2 — Integrate compositor groups and flattening boundaries

### Goal

Make HAVI build a real compositor scene for 3D/perspective-participating content.

### Tasks

1. Detect 3D-participating subtrees using CSS semantics from existing style/transform logic:

- `perspective`
- `transform-style: preserve-3d`
- true 3D transforms

2. Introduce explicit compositor group semantics:

- `Flat`
- `Preserve3d`

3. Implement flattening-boundary handling:

- content that must flatten is rendered to a surface
- that surface is then submitted as a compositor node in the parent group

4. Preserve shared preserve-3d participation:

- descendants that belong to the same 3D context should not be flattened between each other
- they should be submitted into the same compositor context

5. Ensure backface visibility has an explicit place in the model, even if some edge cases remain deferred.

### Deliverable

A compositor scene integrated into HAVI’s render pipeline, with explicit flattening/group semantics.

### Notes

This is the core architectural phase.
If this phase is done cleanly, later phases should mostly be correctness work and cleanup.

---

## Phase 3 — Backend painting through `makepad/compositor`

### Goal

Actually paint the compositor scene using the companion Makepad compositor crate.

### Tasks

1. Wire HAVI’s compositor scene output into `../makepad/compositor`.

2. Keep existing 2D content on the current `makepad_builder.rs` path.
Only send the necessary subtrees through the compositor path.

3. Implement offscreen surface rendering for flattening boundaries.

4. Submit projected nodes/quads/surfaces into the compositor backend with:

- full 4x4 transforms
- proper group boundaries
- opacity as needed
- backface visibility as supported
- projected clip data as supported by the compositor crate

5. Ensure the ordinary 2D painter and compositor path compose cleanly in one frame.

### Deliverable

A working end-to-end compositor path that is actually rendering through `../makepad/compositor` rather than a fallback approximation.

### Notes

If `../makepad/compositor` still needs small API adjustments, make only the narrowest changes needed and keep them generic.
Do not move CSS semantics into Makepad.

---

## Phase 4 — Correctness passes for Reveal semantics

### Goal

Make the Reveal presentation render correctly on the new compositor scheme.

### Tasks

1. Reproduce the Reveal presentation using:

```bash
cd /home/devops/Projects/forge/havi
MAKEPAD_DISABLE_PULSE_AUDIO=1 \
HAVI_PYLON_MODE=none \
HAVI_URL="file:///home/devops/Projects/forge/site/presentation/index.html" \
./mach-havi run --makepad-socket
```

2. Validate the specific semantic cases Reveal uses:

- ancestor perspective on `.slides`
- transform-style / transition participation as present in Reveal CSS
- present/future/past slide transforms
- flattening boundaries between the slide world and overlay UI pieces
- correct first-slide display after removing pause overlay

3. Use reduced local files if needed, but do not lose focus on the real target page.
The final validation target remains the actual presentation page.

4. Fix only the compositor-path bugs needed to achieve a solid architecture and correct output.
Do not accumulate one-off hacks for Reveal classes.

### Deliverable

The real page renders correctly under the new compositor path.
A screenshot must visibly show the Reveal first slide.

### Required screenshot step

Use:

```bash
./havi-makepad-cli --socket "$HAVI_MAKEPAD_SOCKET" screenshot /tmp/havi-presentation-compositor.png
```

If the pause overlay appears, remove it via DevTools or the page API before final screenshot capture.

### Notes

The Reveal page is the integration proof, not the architecture guide.
Use it to validate the design, not to dictate hacks.

---

## Phase 5 — Cleanup, tests, and final polish

### Goal

Leave the codebase in a clean, durable state.

### Tasks

1. Remove temporary fallback branches or obsolete code that the compositor path replaces.

2. Reduce warning-producing dead code where touched by this work.
Do not do a broad cleanup unrelated to this feature.

3. Add focused tests for the architectural rules you settled on.
Good candidates include:

- perspective-only owners create compositor participation without bogus local rebasing
- preserve-3d descendants stay in the same compositor group until a flattening boundary
- flattening boundary renders subtree to a surface and composes it as a parent node
- 2D ordinary content still stays on the old path

4. Re-run focused renderer tests.

5. Re-run the real presentation and capture a final screenshot.

### Deliverable

- clean code
- focused tests
- successful real-page validation screenshot

---

## What “done” means

This task is done when all of the following are true:

1. HAVI uses the new compositor scheme for the Reveal presentation path
2. The architecture is explicit and clean
3. Ordinary 2D content still uses the existing 2D path
4. The Reveal presentation renders correctly
5. A screenshot exists proving correct rendering
6. Focused tests cover the new architectural rules
7. Any remaining unsupported edge cases are isolated and clearly left outside the current scope

## Non-goals for this task

Do **not** expand scope into these unless absolutely required for the target page:

- generic Makepad widget-level projected hit-testing
- full plane splitting / BSP / polygon intersection resolution
- exact handling of every pathological CSS 3D edge case
- a large redesign of the renderer unrelated to the compositor boundary
- moving browser semantics into Makepad core

## Practical guidance

### Prefer architectural clarity over incremental hacks

If a local patch makes Reveal work but leaves the compositor boundary muddy, it is the wrong fix.
The accepted end state is the clean architecture.

### Keep the 2D path alive and simple

The compositor path should be activated only where needed.
Most content should remain on the existing fast path.

### Keep Makepad generic

If `../makepad/compositor` needs changes, they should stay generic:

- surfaces
- projected nodes/quads
- depth behavior
- clip planes

Not:

- CSS DOM concepts
- stacking-context semantics
- browser-specific naming or state

## Final validation checklist

Before calling the work done, verify all of these:

- [ ] `../makepad/compositor` is actually being used for the relevant 3D/perspective path
- [ ] no temporary debug logging remains
- [ ] focused tests pass
- [ ] presentation page launches cleanly with the required env vars
- [ ] final screenshot shows the Reveal first slide correctly
- [ ] compositor path and 2D path boundaries are obvious in code
- [ ] fallback logic that was only a temporary bridge is either removed or explicitly documented as a residual compatibility path

## Minimal commands

### Focused tests

```bash
cd /home/devops/Projects/forge/havi
cargo test -p havi-render
```

### Run presentation

```bash
cd /home/devops/Projects/forge/havi
MAKEPAD_DISABLE_PULSE_AUDIO=1 \
HAVI_PYLON_MODE=none \
HAVI_URL="file:///home/devops/Projects/forge/site/presentation/index.html" \
./mach-havi run --makepad-socket
```

### Screenshot

```bash
cd /home/devops/Projects/forge/havi
./havi-makepad-cli --socket "$HAVI_MAKEPAD_SOCKET" screenshot /tmp/havi-presentation-compositor.png
```

## Bottom line

The old renderer repairs solved the ownership problem well enough to get Reveal working via fallback.
That is not the final design.

The final design is:

- existing 2D path for normal content
- explicit compositor scene for true 3D/perspective content
- `../makepad/compositor` as the rendering backend for those compositor groups
- CSS semantics owned by HAVI
- flattening boundaries explicit in HAVI
- Reveal presentation used as the real integration proof

That is the path from the current state to a pristine final implementation.