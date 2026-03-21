# Render transform phase note

Focused reduced repro used:

```bash
cd havi && HAVI_URL="file:///tmp/test.html" HAVI_SCREENSHOT=/tmp/test.png ./mach-havi run
```

Reduced repro HTML:
- one 100x100 red box
- `transform: translate(40px, 20px)`
- `transform-origin: 0 0`
- body margin 0

Observed result after current structural changes:
- scene/root origin plumbing is no longer the primary failure
- transformed paint containers receive non-root spatial execution origins
- focused oracle artifacts still show blank or missing transformed content
- the remaining failure is not a small offset bug

Backend rewrite work completed so far:
- transformed boxes now classify for surface composition in backend routing (`has_transform` participates in compositor routing)
- compositor scene now records explicit target-surface ownership per paint container
- backend traversal now enforces the contract that a paint container paints into exactly one target surface
- parent traversal skips children owned by another target surface
- subtree bounds now filter descendants by compositor target-surface ownership instead of following mixed implicit direct-path assumptions

Current result after this rewrite slice:
- reduced translate repro still renders blank
- this means the remaining failure is deeper than traversal ownership alone

Current likely cause:
- ordinary transformed 2D surface composition is now routed structurally, but the offscreen surface render/composite path still does not make transformed subtree content visible
- the remaining bug is likely in one of these backend-local areas:
  1. surface-local painting basis inside offscreen passes
  2. quad composition transform from surface-local bounds into parent target
  3. clip/effect interaction causing transformed surface content to be discarded
  4. Makepad pass/root-turtle interaction for offscreen subtree content

Evidence against upstream semantic failure being the only issue:
- transformed paint-container origins are non-zero as expected
- all focused transformed cases fail by disappearance/blank output rather than by consistent wrong placement

Next backend-local steps:
1. instrument surface-local bounds and composed quad transform for the reduced translate repro
2. verify whether offscreen surface receives painted pixels at all
3. if the surface is populated but final output is blank, fix quad composition transform
4. if the surface is blank, fix offscreen subtree local painting basis
5. rerun reduced translate repro after each substantial change, then the focused oracle slice
