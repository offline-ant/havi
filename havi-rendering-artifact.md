# HAVI Tab Bar Rendering Artifact (Linux/OpenGL)

## Symptom

Two dark rectangular blocks appear in the tab bar on Linux/Wayland, covering
parts of the tab label text. They are stable — they do not flicker and do not
disappear after mouse moves or any user interaction.

Screenshot pixel analysis shows the blocks are solid dark grey (~rgb 42-55),
matching the tab `draw_bg` background color. They are not truly black; they are
tab View backgrounds drawn at stale positions with no text on top.

Observed positions (1280×800 window):
- Block 1: approximately x=55–70, y=16–31 (covers "De" in "HAVI Demo")
- Block 2: approximately x=138–166, y=16–41 (covers gap between tab labels)

Not reproduced on macOS (Metal) or Windows.

## Widget Tree Evidence

`havi-makepad-cli dump` shows orphaned widgets with parent `-1`:

```
22 -1 tab_label Label 283 17 11 14
23 -1 tab_close Label 506 23 12 12
24 -1 tab_label Label 21 17 11 14
25 -1 tab_close Label 244 23 12 12
```

These are `tab_label` and `tab_close` children of dynamically-instantiated tab
View widgets. They have no parent in the widget tree graph because their parent
View was created via `script_from_value` and is not part of the static tree.
Their `area` coordinates reflect the last position they were drawn.

## Root Cause (under investigation — prior hypothesis invalidated)

The initial root cause analysis attributed the ghost blocks to stale VBO data
retained after widget instances were dropped. Both the mechanism and the fix
were wrong.

**The VBO tail hypothesis is mechanically incorrect.** `update_array_buffer`
calls `glBufferData` with the exact byte count of the current instances slice.
`glBufferData` replaces the entire GL buffer with that data. No tail is
retained; the GPU buffer is exactly as large as the upload.

**Widget reuse (Attempt 4) does not fix the artifact.** If the ghost were
caused by stale data from dropped widgets, reusing the same `WidgetRef` would
prevent it. It does not.

**Attempts 1 and 2 confirm the ghost items are fresh, not stale.** Both
attempts filtered on `draw_item.redraw_id != list_redraw_id`. Neither fixed the
artifact because the ghost draw items carry the current-frame `redraw_id` —
they are not leftover from a prior frame. The wrong geometry is being written
into fresh draw items in the current draw pass.

### What is known about the actual mechanism

The ghost blocks are solid-color DrawQuad instances at positions that do not
correspond to any current tab widget's layout position. They are rendered from
fresh, current-frame draw items (redraw_id matches). The wrong data is being
written to those items during the current draw pass.

Two paths in Makepad update instance position data without setting
`instance_dirty`:

- `Area::set_rect()` — called by `draw_quad.end()` to patch the final rect
  after layout. Does not set `instance_dirty`.
- `move_align_list()` — called by the turtle to shift aligned instances into
  their final positions for `Fit`-sized widgets. Does not set `instance_dirty`.

Both paths rely on `instance_dirty = true` having been set upstream by
`push_item` (which `clear_draw_items` triggers at the start of a full redraw).
In a full redraw this chain is correct. The ghost persisting through
`redraw_all()` means either:

1. The position patch is not being applied (silent `set_rect` early-return due
   to `redraw_id` mismatch between the area and the draw list), leaving the
   DrawQuad's stale struct-field `rect_pos`/`rect_size` in the VBO, or
2. The patch is applied but to the wrong instance offset, or
3. A draw item is produced by a code path that does not go through the normal
   `begin()`/`end()` pair and therefore never receives the correct rect.

`Area::set_rect()` early-returns silently when
`draw_list.redraw_id != inst.redraw_id`. If `draw_bg.begin()` and
`draw_bg.end()` execute in frames with different `cx.redraw_id` values — which
the `DrawState` async mechanism permits for widgets whose draw spans multiple
ticks — the patch is skipped and the VBO is uploaded with the DrawQuad struct's
previous-frame `rect_pos`/`rect_size`.

Whether the tab View widgets created via `script_from_value` can span draw
ticks (via `script_async` or `on_render`) has not been confirmed.

### Why `visible = false` / `set_visible` does not help

`View::draw_walk` early-outs when `!self.visible`, suppressing new draw calls.
This cannot affect already-rendered ghost blocks because those come from fresh
draw calls in the same frame, not from the hidden widget.

### Why `redraw_all` does not help

`redraw_all` clears all draw lists and re-runs every widget's `draw_walk`.
The ghost is written during the re-run itself — it is not a holdover from a
prior frame.

## Architecture: how inline drawing works

Plain `View{}` widgets without `optimize: DrawList` or `optimize: Texture` draw
directly into the parent draw list (no sub-list). Their `draw_bg` `DrawQuad`
and child `Label` draw calls all land as `CxDrawItem` entries in the tab bar's
`CxDrawList`. The `CxDrawItems` pool reuses slots by position (`used` index),
not by identity. When a widget is dropped and a smaller or different set of
widgets draws next frame, the tail of the pool (slots beyond the new `used`
count) retains old GPU data but is not rendered — except when the draw list
itself is not fully redrawn (partial redraw), in which case `clear_draw_items`
is never called and all old items remain active.

The orphaned `tab_label`/`tab_close` Labels from dynamically-created tab Views
register themselves in the global widget tree graph with no parent (because
their parent View is not in the static tree). They show up as roots in the dump
with parent `-1`.

## What was tried

### Attempt 1: `redraw_id` staleness skip in `render_view` (makepad)

Added a check in `platform/src/os/linux/opengl.rs` `render_view` to skip draw
items whose `redraw_id` does not match the draw list's current `redraw_id`.
Removed after confirming it did not fix the artifact. The stale items are being
reused (written at the same pool slot with the correct `redraw_id`) but via
`append_to_draw_call` rather than `new_draw_call`, so the redraw_id matches
even for partially-stale geometry.

### Attempt 2: zero instances on stale items in `render_view` (makepad)

Same location. When `draw_item.redraw_id != list_redraw_id`, clear
`draw_item.instances` and set `instance_dirty = true`, so the zero-length
upload goes to the VBO and the `instances == 0` guard skips the draw. Did not
fix the artifact. Same reason as above — the items are being reused correctly
by index so their `redraw_id` matches.

### Attempt 3: remove `tab_template` from children (havi)

On first `sync_tab_bar` call, extract `template_source` and immediately remove
`tab_template` from `tab_bar.children` via `retain`. Cache the source as
`ScriptValue` in `App::tab_template_source`. This eliminated the `tab_template`
widget pair from the dump (down from 4 orphaned widgets to 2) but the visual
artifact persisted. The artifact is caused by the instantiated tab widgets, not
the template itself.

### Attempt 4: reuse widget instances across `sync_tab_bar` calls (havi)

Added `widget: Option<WidgetRef>` to `TabInfo`. In `sync_tab_bar`, use the
cached widget rather than calling `script_from_value` on every sync. This was
the "most promising fix" in the previous investigation. It was implemented,
built, and run. The artifact persists unchanged.

## How testing was done

- `havi-makepad-cli --socket $HAVI_MAKEPAD_SOCKET screenshot /tmp/out.png`
  for visual confirmation.
- `havi-makepad-cli --socket $HAVI_MAKEPAD_SOCKET dump` to inspect the widget
  tree and identify orphaned widgets.
- Python/Pillow pixel sampling to characterize the artifact color and bounds
  precisely.
- Two-screenshot diff (`ImageChops.difference`) to confirm the artifact is
  stable across frames (not a flicker).
- Mouse move between screenshots to confirm the artifact persists through
  redraws triggered by hover state changes.

## Current state

Attempt 4 (widget reuse) is in place in the codebase. It does not fix the
artifact. The root cause is not yet fully understood.

## Open questions

1. Do `script_from_value` tab View widgets use `script_async` or `on_render`
   in a way that causes their `draw_walk` to span multiple `cx.redraw_id`
   cycles? If so, `set_rect` would silently skip the rect patch.

2. Is there a draw path for dynamically-created Views that bypasses
   `draw_bg.end()` entirely, leaving `rect_pos`/`rect_size` at their stale
   struct-field values when the VBO is uploaded?

3. Does `move_align_list` correctly address the instance offset for the tab
   View's draw_bg when the tab View is not in the static widget tree?

## Next investigative step

Confirm whether `draw_bg.end()` → `area.set_rect()` is actually patching the
correct rect for the tab View widgets, or silently returning. Add a log or
assert inside `Area::set_rect` that fires when the redraw_id mismatch
early-return is taken, and re-run HAVI to see if it triggers.

## Step-back option

Add `optimize: DrawList` to the tab template definition. Each tab widget then
has its own isolated `CxDrawList`. Its draw calls do not interact with the
parent's draw list pool. Partial and full redraws are both handled correctly
through the existing sub-list mechanism. This eliminates the entire class of
inline-draw-list position-update races without requiring further diagnosis of
the exact broken path.
