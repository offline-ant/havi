# HAVI Tab Bar Rendering Artifact (Linux/OpenGL)

## Symptom

Dark rectangular blocks appear in the tab bar on Linux/Wayland, covering parts
of tab label text. They are stable — they do not flicker and do not disappear
after mouse moves, redraws, or any user interaction.

Pixel analysis shows the blocks are solid dark grey (~rgb 42-55), matching the
tab View `draw_bg` background color (#2a2a2a). They are tab-background-colored
rectangles drawn on top of text at wrong positions.

Observable behaviors:

- With one tab active, two dark blocks cover portions of the tab label.
- Switching focus to the second tab makes the blocks on the first tab
  nearly invisible (because the first tab's background and the block color
  both become #2a2a2a — they merge).
- Closing the first tab does not remove the artifacts. They persist at the
  same screen positions even when the surviving tab shifts left.
- The blocks are NOT the configured tab background color. When tab bg was
  changed to bright blue (#0040e6), the blocks remained dark grey. The
  block color comes from the `draw_bg` default in the live DSL template
  (`uniform(#x2a2a2a)`), not from the runtime `set_uniform` call.

Not reproduced on macOS (Metal) or Windows (D3D11).

## Widget Tree Evidence

`havi-makepad-cli dump` shows orphaned widgets with parent `-1`:

```
22 -1 tab_label Label 20 15 18 17
23 -1 tab_close Label 263 23 12 12
24 -1 tab_label Label 300 15 18 17
25 -1 tab_close Label 542 23 12 12
```

These are `tab_label` and `tab_close` children of dynamically-instantiated tab
View widgets. They have no parent in the widget tree graph because their parent
View was created via `script_from_value` and is not part of the static tree.

## How tabs are created

`sync_tab_bar` in `havi/ports/havishell/src/app/tabs.rs`:

1. Extracts `ScriptObjectRef` source from the `tab_template` View.
2. For each tab, calls `WidgetRef::script_from_value(vm, template_val)` to
   create a new View widget from the template.
3. Sets label text via `widget.widget(cx, ids!(tab_label)).set_text(cx, ...)`.
4. Sets bg color via `view.draw_bg.draw_vars.set_uniform(cx, ...)`.
5. Replaces `tab_bar.children` with the new widget list.
6. Calls `cx.redraw_all()`.

The template itself is kept in `new_children` as the first entry, hidden via
`set_visible(cx, false)`.

## Key finding: the live DSL template color leaks through

The `tab_template` definition in the live DSL sets:

```
draw_bg +: { color: uniform(#x2a2a2a) ... }
```

At runtime, `sync_tab_bar` overrides this via `set_uniform(cx, ...)` to either
#353535 (active) or #2a2a2a (inactive). When the active tab color was changed
to bright blue, the ghost blocks stayed dark grey (#2a2a2a). This means the
ghost blocks are rendered from a draw path that uses the template's compiled-in
uniform default, NOT the runtime-applied uniform value.

This rules out the blocks being a simple z-order or draw-order issue between
the bg and text of the same widget. The ghost is a separate DrawQuad instance
carrying stale/default uniform values.

## What has been ruled out

### VBO tail data (ruled out early)

`update_array_buffer` calls `glBufferData` with exact byte count. No tail
is retained. The GPU buffer is exactly as large as the upload.

### Stale draw items from prior frames (ruled out)

Attempts 1 and 2 filtered on `draw_item.redraw_id != list_redraw_id`. Neither
fixed the artifact because the ghost draw items carry the current-frame
`redraw_id`. The wrong geometry is written into fresh draw items during the
current draw pass.

### Widget reuse (ruled out — Attempt 4)

Caching `WidgetRef` in `TabInfo` and reusing across `sync_tab_bar` calls
instead of creating new widgets each time. Did not fix the artifact.

### `visible = false` / `set_visible` (ruled out)

`View::draw_walk` early-outs when `!self.visible`. This cannot suppress the
ghost because the ghost comes from a fresh draw call in the same frame, not
from the hidden widget.

### `redraw_all` (ruled out)

`redraw_all` clears all draw lists and re-runs every widget's `draw_walk`.
The ghost is written during the re-run itself.

### `instance_dirty` flag on `set_rect` and `move_align_list` (tested, no effect)

Added `instance_dirty = true` + `paint_dirty = true` to:

- `Area::set_rect()` in `platform/src/area.rs`
- `move_align_list()` in `draw/src/turtle.rs`
- `clip_and_shift_align_list()` in `draw/src/turtle.rs`

Hypothesis: the OpenGL renderer skips re-uploading instance VBOs when
`instance_dirty` is false, and these late-patching paths don't set the flag.

Result: artifact persists unchanged. The issue is not about stale GPU data
from a missed upload — the wrong data is in the CPU-side instance buffer
itself.

### Unconditional VBO upload in OpenGL renderer (tested, no effect)

Changed `platform/src/os/linux/opengl.rs` to always call
`update_array_buffer` on every draw call, removing the `instance_dirty` guard.

Result: artifact persists. Confirms the problem is in the CPU-side instance
data, not in a missed GPU upload.

### Disabling background-lane cross-content batching on Linux (tested, no effect)

In `find_appendable_drawcall`, prevented background-lane (lane 0) draw calls
from crossing content-lane (lane 1) barriers on Linux, and also prevented
background-lane draw calls from finding any appendable target at all.

Result: artifact persists. The batching/append logic is not the cause.

### `new_batch: true` on tab template (tested, no effect)

Added `new_batch: true` to the `tab_template` View definition, forcing each
tab to use its own `CxDrawList` (DrawList optimization). This isolates each
tab's draw calls from the parent draw list pool.

Result: artifact persists. The ghost DrawQuad is not caused by draw-list
pool slot reuse between tabs.

## What is known about the mechanism

1. The ghost blocks are DrawQuad instances at positions that don't correspond
   to any current tab widget's layout position.

2. They carry the current frame's `redraw_id` — they are freshly created
   draw items, not leftovers.

3. Their color is the live DSL template default (#2a2a2a), NOT the runtime
   uniform value set by `set_uniform`. This means they come from a draw
   path that executes `draw_bg.begin()`/`draw_bg.end()` using the template's
   compiled defaults, separate from the path where `set_uniform` is applied.

4. They persist across `redraw_all()`, across tab switches, and across tab
   closes. Their screen positions are stable.

5. The `tab_template` hidden widget (first child, `set_visible(false)`) is
   ruled out as the source — removing it from children (Attempt 3) reduced
   orphaned widgets but did not eliminate the artifact.

6. None of the following Makepad-level changes affect it:
   - `instance_dirty` on all instance-mutation paths
   - unconditional VBO upload
   - draw-call batching changes
   - `new_batch: true` (DrawList isolation)

## Architecture notes

### Inline drawing

Plain `View{}` widgets without `optimize: DrawList` draw directly into the
parent draw list. Their `draw_bg` DrawQuad and child Label draw calls land as
`CxDrawItem` entries in the tab bar's `CxDrawList`.

### `script_from_value` widget creation

`WidgetRef::script_from_value(vm, value)` calls `script_new()` then
`script_apply()`. The `#[source]` ScriptObjectRef on View captures the live
DSL object. `script_apply` reads properties from that object to initialize
the widget. Layout properties (`padding`, `width`, `height`) are stored in
the `#[layout]` and `#[walk]` fields — whether these flow through
`script_apply` from the source object was not conclusively verified.

Confirmed: changing `padding` in the live DSL template had no visible effect
on the rendered tab widgets, suggesting layout properties may NOT propagate
through `script_from_value`. The tab widgets may be using default Layout
values.

### `set_uniform` vs compiled defaults

`draw_vars.set_uniform(cx, id, value)` patches the draw call's
`dyn_uniforms` buffer for an existing Area. This only works if the widget's
Area is valid (correct `redraw_id`). If the Area becomes invalid between
widget creation and uniform application, the set_uniform is silently
dropped and the compiled default from the live DSL template is used instead.

## Hypotheses still open

### 1. `script_from_value` creates widgets whose draw_bg.begin() fires before Area is valid

When `sync_tab_bar` creates a widget via `script_from_value`, the widget
exists but has not yet been drawn. The `set_uniform` call may target an Area
that is either empty or from a prior frame. The uniform patch is silently
dropped. When `cx.redraw_all()` triggers the actual draw, `draw_bg` uses its
compiled-in default color.

This would explain why the ghost color is always #2a2a2a (template default)
regardless of what `set_uniform` sets.

To test: move `set_uniform` calls to happen AFTER the first draw pass, or
set color directly on the DrawQuad struct field before drawing.

### 2. Multiple draw_bg.begin()/end() calls per widget

If `script_from_value` or `script_apply` triggers an implicit draw during
widget construction, a DrawQuad instance is created with template defaults.
Then the normal `draw_walk` in the redraw pass creates a second instance.
The first instance persists in the draw list at a stale position with
template-default uniforms.

To test: add a counter/log to DrawQuad begin() calls for draw_bg instances
with the tab template's shader, and check if there are 2× the expected
number of instances.

### 3. The hidden tab_template widget draws despite set_visible(false)

`set_visible` sets a field on the widget. If `draw_walk` is called before
`set_visible` takes effect (e.g., during `script_apply` or a redraw queued
before the visibility flag is set), the template widget draws with its
default position and default uniforms.

To test: instead of `set_visible(false)`, remove the template from children
entirely and cache only the ScriptObjectRef source.

## Reproduction

```bash
cd havi && ./debug-artifact.sh
```

Automated: builds, launches HAVI with `--makepad-socket`, waits for ready,
takes a screenshot, crops the top-left 640×120 region. Output path is
printed to stdout.

## How testing was done

- `havi-makepad-cli --socket $SOCKET screenshot /tmp/out.png`
- `havi-makepad-cli --socket $SOCKET dump` for widget tree
- Python/Pillow pixel sampling for artifact color characterization
- Two-screenshot diff to confirm stability
- Color override experiments (blue active, green inactive, magenta template)
  to trace which draw path produces the ghost blocks
- `debug-artifact.sh` for automated build+run+screenshot cycle

## Current state

All speculative Makepad-level fixes have been reverted. The working tree is
clean except for `debug-artifact.sh` (added foreground build step).

The artifact is unfixed. The root cause is in how `script_from_value` tab
widgets interact with the draw system — specifically, how their `draw_bg`
DrawQuad instances get created with template-default uniforms at wrong
positions, in a way that none of the standard Makepad draw-list, batching,
or VBO upload mechanisms can prevent.

## Next steps

1. Test hypothesis 1: set draw_bg color via struct field
   (`draw_bg.color = ...`) instead of `set_uniform`, before any draw pass.

2. Test hypothesis 2: count DrawQuad instances per tab to detect double-draw.

3. Test hypothesis 3: remove template from children entirely, cache only
   the ScriptObjectRef source in App state.

4. Step-back option: stop using `script_from_value` for tab creation. Build
   tab widgets manually (construct View, add Label children, set properties)
   without cloning from a template. This eliminates the entire template
   draw-path interaction.
