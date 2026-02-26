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

## Root Cause (established)

`sync_tab_bar` in `ports/havishell/src/app/tabs.rs` rebuilds the tab bar
children on every call by creating fresh widget instances via
`script_from_value`. Each call:

1. Allocates new `WidgetRef` instances for every tab.
2. Pushes them into `tab_bar.children`, replacing the previous set.
3. Drops the old `WidgetRef` instances.

The old widgets' draw calls (specifically their `draw_bg` `DrawQuad`) were
written inline into the parent `tab_bar` draw list's VBO on the previous frame.
On Linux/OpenGL those VBO entries are not cleared when the widget stops drawing
— the GPU retains whatever was last uploaded. On the next frame the new widgets
draw at their correct positions, but the old VBO entries also remain and are
re-issued by the GL driver, producing ghost backgrounds at the old coordinates.

On macOS/Metal this does not occur because Metal rebuilds its command buffer
from scratch on every frame; there is no persistent VBO state to corrupt.

### Why `visible = false` / `set_visible` does not help

The `View::draw_walk` early-out at `if !self.visible` prevents the widget from
emitting new draw calls. But the VBO on the GPU already contains the data from
when the widget last drew. The GL `render_view` loop iterates all draw items in
the draw list's pool — including items written by now-dropped widgets — and
issues draw calls for all of them with no staleness check.

### Why `redraw_all` does not help

`cx.redraw_all()` sets `DrawEvent::redraw_all = true`, which causes every draw
list's `clear_draw_items` to be called at the start of the next draw pass.
`clear_draw_items` sets `draw_items.used = 0` (pool cursor reset) but does not
zero the pool buffer. New draw calls are written starting from index 0, reusing
slots — but the GPU VBO is only re-uploaded when `instance_dirty = true`. If
the same shader/geometry combination is reused, Makepad may append to the
existing draw call rather than creating a new one, leaving orphan geometry in
the VBO tail.

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

Added `widget: Option<WidgetRef>` to `TabInfo`. In `sync_tab_bar`, use
`get_or_insert_with` to create the widget only once per tab, reusing the same
`WidgetRef` on subsequent calls. This prevents new widget instances from being
allocated each sync, so no new orphaned draw calls accumulate. Not confirmed to
fix the artifact — build was aborted before testing. Reverted along with all
other changes.

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

All changes reverted. Codebase is at the pre-investigation state.

## Most promising fix

Reuse the same `WidgetRef` per tab across `sync_tab_bar` calls (Attempt 4).
This is the correct application-level fix: the same widget instance draws at
its new position each frame, so the draw list slot is reused with fresh data
and no ghost geometry accumulates. The makepad-level fix (zeroing stale VBO
entries in `render_view`) is the correct generic fix for the class of problem
but requires more investigation into why the `redraw_id` approach did not work
as expected.
