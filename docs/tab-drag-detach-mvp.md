# Tab Drag Detach-To-New-Window Design

Status: Proposed

Last updated: 2026-03-15

## Summary

This document extends the existing same-window tab drag reorder MVP to support
dragging a tab out into a new window.

Recommended first implementation:

- Press and initial drag behave exactly like the current reorder MVP.
- While the pointer stays near the tab strip, drag intent is `Reorder`.
- Once the pointer moves far enough away from the tab strip vertically, drag
  intent switches to `DetachPending`.
- The source window keeps rendering a floating tab overlay, but the base strip
  collapses as if the dragged tab has left the window.
- On left release, the tab is moved into a newly created window positioned from
  the release `screen_coords`.

Important scope choice:

- This first detach phase does **not** try to create the new GUI window during
  the drag and continue dragging it under the cursor.
- It commits on release, just like the reorder MVP.

That scope fits the current architecture well and avoids relying on a cross-window
mouse capture mechanism that does not exist in the current window layer.

## Current Baseline

The reorder MVP already provides the core pieces we need:

- `TabDragState` in `kaku-gui/src/termwindow/mod.rs`
- `drag_tab()` and release-time commit in `kaku-gui/src/termwindow/mouseevent.rs`
- overlay-only drag rendering in:
  - `kaku-gui/src/termwindow/render/tab_bar.rs`
  - `kaku-gui/src/termwindow/render/fancy_tab_bar.rs`
- stable drag identity via `TabId`
- release-time same-window commit via `move_specific_tab_to_slot(...)`

The rest of the codebase already has the window lifecycle primitives needed for
detach:

- `MouseEvent` includes `screen_coords`
- `Mux::new_empty_window(workspace, position)` can create a new mux window at a
  specific `GuiPosition`
- dropping `MuxWindowBuilder` emits `MuxNotification::WindowCreated`
- the frontend reconciler creates a `TermWindow` when it sees `WindowCreated`
- `Mux::prune_dead_windows()` already removes empty source windows later

This means the missing piece is not "can Kaku create a new window?" but rather:

1. when tab drag should switch from reorder to detach intent
2. how to move an existing live tab into the new window safely
3. how to avoid losing the tab if target GUI window creation fails

## Goals

- Preserve the current reorder MVP when the pointer stays in the tab strip.
- Make drag-out-to-new-window feel like a natural continuation of the same drag.
- Keep release-time commit semantics.
- Avoid mutating mux state on every drag frame.
- Avoid losing an existing tab if target window creation fails.
- Support both fancy and retro tab bars.
- Support both top and bottom tab bars.
- Preserve existing click activation, rename, close-button, tab navigator, wheel,
  and titlebar drag behavior.

## Non-Goals

- Live cross-window drag handoff while the mouse button is still down.
- Dragging a detached tab into another existing window.
- Drag animations or easing.
- OS-native drag-and-drop integration.
- Reworking click-to-activate semantics.

## Key Constraint

The current window abstraction exposes:

- `request_drag_move()`
- `set_window_drag_position(...)`
- `set_window_position(...)`

but it does **not** expose a generic "capture all mouse events globally until
release" or a cross-window drag session abstraction.

That matters because a true Chrome-style detach flow normally does this:

1. user leaves the tab strip
2. a new top-level window appears immediately
3. that new window keeps following the cursor while the same mouse press is
   still active

Implementing that cleanly would require a drag handoff between two `TermWindow`
instances while the button is still down. Nothing in the current stack provides
that handoff yet.

This is why the recommended first detach phase remains release-time commit.

## Recommended UX Model

### In-strip drag

While the dragged tab remains within the detach band around the tab strip:

- behavior is identical to the reorder MVP
- overlay follows the pointer
- base strip omits the dragged tab and inserts a spacer at `target_slot_idx`
- release commits same-window reorder

### Out-of-strip drag

Once the dragged tab moves far enough away from the strip vertically:

- drag intent becomes `DetachPending`
- overlay continues following the pointer
- base strip omits the dragged tab with **no spacer**
- release creates a new window and moves the tab there

### Re-entry

Detach is not sticky. If the pointer comes back into the strip band before
release:

- drag intent switches back to `Reorder`
- `target_slot_idx` is recomputed from the current overlay center
- the base strip shows the spacer again

This keeps the interaction reversible and browser-like.

## Recommended Config Gate

Detach should be gated separately from same-window reorder.

Recommended new config:

```rust
#[dynamic(default)]
pub mouse_drag_detaches_tabs: bool,
```

Recommended initial default: `false`

Reasoning:

- `mouse_drag_reorders_tabs` already controls whether tab dragging exists at all
- detach is a materially bigger behavior change than reorder
- separate gating makes rollout safer and easier to test

Interaction with existing config:

- `mouse_drag_reorders_tabs = false` disables both reorder and detach
- `mouse_drag_reorders_tabs = true` and `mouse_drag_detaches_tabs = false`
  keeps the current reorder MVP only
- `mouse_wheel_scrolls_tabs` remains independent and unrelated

## Runtime Drag State

The current drag state only tracks horizontal overlay motion plus a reorder slot.
Detach needs a small extension.

Recommended shape:

```rust
enum TabDragIntent {
    Reorder { target_slot_idx: usize },
    DetachPending,
}

struct TabDragState {
    tab_id: TabId,
    start_event: MouseEvent,
    start_bounds: UIItem,
    source_slot_idx: usize,
    has_dragged: bool,
    overlay_offset_x: isize,
    overlay_offset_y: isize,
    last_screen_coords: ::window::ScreenPoint,
    intent: TabDragIntent,
}
```

Notes:

- `overlay_offset_y` is required so the floating tab can visibly move away from
  the strip while detach is armed
- `last_screen_coords` is used to compute the target window position on release
- `target_slot_idx` moves into `TabDragIntent::Reorder`, so detach mode no longer
  pretends there is still a spacer target

## Render-Only Drag Snapshot

The render layer should still receive a distilled snapshot rather than the full
runtime state.

Recommended shape:

```rust
enum TabDragVisualMode {
    Reorder { target_slot_idx: usize },
    Detach,
}

struct TabDragRenderInfo {
    tab_id: TabId,
    dragged_tab_idx: usize,
    source_slot_idx: usize,
    mode: TabDragVisualMode,
    overlay_left_px: f32,
    overlay_top_px: f32,
    overlay_width_px: f32,
    overlay_height_px: f32,
}
```

Notes:

- `tab_id` stays in the render snapshot so base-strip builders can key off a
  stable identity if tab indices shift
- `overlay_top_px` and `overlay_height_px` let both paint paths render the tab
  floating vertically, not just horizontally

## Detach Threshold

Detach should use a different threshold from `Self::TAB_DRAG_THRESHOLD`.

The existing drag threshold (`6px`) is appropriate for "this is a drag now",
but too small for "the user intends to pull the tab out into a new window".

Recommended helper:

```rust
fn distance_outside_strip(y: isize, strip_top: isize, strip_bottom: isize) -> isize
```

Behavior:

- returns `0` when `y` is inside the strip bounds
- returns the vertical distance to the nearest strip edge otherwise

Recommended detach criterion:

- `has_dragged == true`
- `mouse_drag_detaches_tabs == true`
- `distance_outside_strip(...) >= tab_detach_threshold_px`

Recommended threshold magnitude:

- about one tab-bar height, or at least one text cell height

That is deliberate enough to avoid accidental detach while still feeling easy.

## Event Model

### Mouse press on tab

Keep current behavior:

- activate the pressed tab immediately if needed
- clear `is_window_dragging` / `window_drag_position`
- initialize `TabDragState`
- do not mutate mux state

### Mouse move before drag threshold

Same as today:

- no overlay
- no mux mutation
- no reorder slot updates

### Mouse move after drag threshold

Once drag is active:

1. update `overlay_offset_x`
2. update `overlay_offset_y`
3. update `last_screen_coords`
4. resolve drag intent

Intent resolution:

- if detach config is disabled:
  - intent is always `Reorder { target_slot_idx }`
- else if pointer is outside the detach band:
  - intent becomes `DetachPending`
- else:
  - intent becomes `Reorder { target_slot_idx }`

Visual invalidation rules:

- entering first real drag frame: invalidate fancy tab bar cache
- changing reorder slot: invalidate fancy cache
- switching `Reorder <-> DetachPending`: invalidate fancy cache
- overlay-only x/y movement inside the same mode does not require fancy cache rebuild

### Mouse release

On left release:

- if `has_dragged == false`, clear drag state and return
- if intent is `Reorder`, keep the existing release-time reorder commit
- if intent is `DetachPending`, commit detach-to-new-window
- clear drag state and invalidate

### Lost release / stale cleanup

Keep the current stale cleanup behavior:

- if the next event arrives without left button held, drop the drag state silently

Important limitation for phase 1:

- if a backend fails to deliver the final release after the pointer fully leaves
  the source window, detach may cancel instead of commit
- this is acceptable for the first phase because the design does not rely on a
  cross-window mouse capture API that does not exist yet

## Render Model

### Reorder mode

Exactly as today:

- omit the dragged tab from the base strip
- insert a spacer at `target_slot_idx`
- render the dragged tab as a visual-only overlay

### Detach mode

While `DetachPending` is active:

- omit the dragged tab from the base strip
- do **not** insert any spacer
- render the dragged tab as a visual-only overlay at
  `start_bounds + (overlay_offset_x, overlay_offset_y)`

This makes the source strip look like the tab has already left the window.

### Fancy tab bar

Required changes:

- support `TabDragVisualMode::Detach`
- build the base tree without the dragged tab and without spacer when detached
- render overlay using both `overlay_left_px` and `overlay_top_px`
- continue suppressing `UIItemType::TabBar(Tab)` and `UIItemType::CloseTab`
  for the dragged tab

### Retro tab bar

Required changes:

- support base-strip layout with no spacer in detach mode
- render overlay at an arbitrary `top_pixel_y`, not just the fixed tab-bar row
- keep registered `UIItem`s aligned with the current mode-specific base layout

## Failure-Safe Detach Commit

This is the most important implementation detail.

Naive detach would do this on release:

1. create new mux window
2. move the live tab there immediately
3. let frontend create a `TermWindow` for it

That is unsafe.

If `TermWindow::new_window(...)` fails in the frontend reconciliation path, the
current code kills the newly created mux window. If the existing live tab has
already been moved there, the user loses the tab.

The design must therefore make detach commit rollback-safe.

### Recommended transaction model

Add a model-level helper that moves an existing tab between mux windows without
pruning empty windows immediately:

```rust
fn move_tab_to_window(
    &self,
    tab_id: TabId,
    target_window_id: WindowId,
    target_slot_idx: usize,
) -> anyhow::Result<MoveTabBetweenWindowsResult>
```

Recommended result payload:

```rust
struct MoveTabBetweenWindowsResult {
    source_window_id: WindowId,
    source_slot_idx: usize,
}
```

Behavior:

1. resolve source window from `tab_id`
2. resolve source slot
3. remove the tab from the source window vector only
4. insert the same `Arc<Tab>` into the target window vector
5. set target active tab appropriately
6. notify/invalidate both windows
7. do **not** call `prune_dead_windows()` yet

This is key:

- if the source window becomes empty, it stays alive temporarily
- that makes rollback possible even when detaching the last tab

### Pending detach registry

The frontend should own a rollback registry keyed by the new mux window id:

```rust
struct PendingDetachedTab {
    tab_id: TabId,
    source_window_id: WindowId,
    source_slot_idx: usize,
}
```

Reason:

- the frontend already owns the success/failure path for `WindowCreated`
- that is exactly where rollback must happen if GUI window creation fails

### Release-time detach flow

Recommended sequence on detach release:

1. compute target `GuiPosition` from `last_screen_coords`
2. create a new empty mux window at that position
3. move the tab into that mux window immediately using `move_tab_to_window(...)`
4. register `PendingDetachedTab` for the new mux window id
5. drop the builder so `WindowCreated` is emitted
6. clear local drag state

### Frontend success path

In the existing frontend `WindowCreated` task:

- if `TermWindow::new_window(mux_window_id)` succeeds
- and `mux_window_id` exists in the pending detach registry:
  - remove the pending record
  - call `mux.prune_dead_windows()`

This is when an empty source window is finally allowed to close.

### Frontend failure path

If `TermWindow::new_window(mux_window_id)` fails
and `mux_window_id` is a pending detach target:

1. remove the pending record
2. move the tab back to `source_window_id` at `source_slot_idx`
3. kill the failed target window
4. do not prune the source window

This prevents data loss.

## Computing the New Window Position

For phase 1, the best-effort position should reuse the same anchor logic as the
existing manual window-drag path:

```rust
new_top_left = ScreenPoint::new(
    last_screen_coords.x - start_event.coords.x,
    last_screen_coords.y - start_event.coords.y,
)
```

Then convert that to:

```rust
GuiPosition {
    x: Dimension::Pixels(new_top_left.x as f32),
    y: Dimension::Pixels(new_top_left.y as f32),
    origin: GeometryOrigin::ScreenCoordinateSystem,
}
```

Why this is good enough for phase 1:

- it keeps the same client-area mouse anchor as the original press
- it works for both top and bottom tab bars
- it reuses already-proven window-drag math

It does not guarantee pixel-perfect "tab stays exactly under cursor after
release" alignment, but that only really matters for live handoff, which is
explicitly deferred.

## Single-Tab Windows

Recommended phase-1 behavior: allow detach even if the source window has only
one tab.

Why this is acceptable:

- the rollback-safe transaction model above keeps the empty source window alive
  until target window creation succeeds
- if target window creation succeeds, `mux.prune_dead_windows()` will remove the
  now-empty source window
- if target window creation fails, rollback restores the original window intact

One caveat remains:

- if `hide_tab_bar_if_only_one_tab = true`, the resulting detached window may
  open without a visible tab bar
- if the source window had only one tab, the old GUI window may exist briefly in
  an empty state until target window creation succeeds and deferred pruning runs

That is acceptable for phase 1 because the drag is already finished at that
point. It would not be acceptable for live cross-window handoff.

## File-Level Change Plan

Primary files:

- `config/src/config.rs`
  - add `mouse_drag_detaches_tabs`

- `config/src/lib.rs`
  - add config docs/example comment

- `kaku/src/config_tui/mod.rs`
  - expose the new config toggle

- `mux/src/lib.rs`
  - add `move_tab_to_window(...)`
  - possibly add a small helper for inserting at a specific slot
  - keep pruning outside the helper

- `kaku-gui/src/frontend.rs`
  - add pending-detach registry
  - finalize or rollback detach in the `WindowCreated` task

- `kaku-gui/src/termwindow/mod.rs`
  - expand `TabDragState`
  - expand `TabDragRenderInfo`
  - add helper to compute detach target `GuiPosition`

- `kaku-gui/src/termwindow/mouseevent.rs`
  - resolve drag intent (`Reorder` vs `DetachPending`)
  - track vertical overlay motion
  - commit release via reorder or detach

- `kaku-gui/src/termwindow/render/tab_bar.rs`
  - support `Detach` base-strip layout
  - render overlay with arbitrary `top_pixel_y`

- `kaku-gui/src/termwindow/render/fancy_tab_bar.rs`
  - support `Detach` base-strip layout
  - render overlay with `overlay_top_px`

## Pure Helpers Worth Testing

Add unit tests for:

- `distance_outside_strip(...)`
- detach intent resolution from `(strip bounds, y, threshold)`
- `compute_target_slot(...)` regression coverage remains intact
- new window top-left computation from `(start_event.coords, last_screen_coords)`
- `move_tab_to_window(...)`:
  - middle tab between two windows
  - move active tab
  - move last tab out of a window without pruning yet
  - rollback move back to original slot

## Manual Verification Plan

Required manual coverage:

1. Fancy tab bar: drag within strip still reorders.
2. Retro tab bar: drag within strip still reorders.
3. Top tab bar: drag downward into content, release, new window opens.
4. Bottom tab bar: drag upward into content, release, new window opens.
5. Drag out, then re-enter strip before release: reverts to reorder mode.
6. Detach active tab from a multi-tab window.
7. Detach the last visible tab from a single-tab window with tab bar shown.
8. `hide_tab_bar_if_only_one_tab = true`: detached window still opens correctly.
9. `mouse_drag_detaches_tabs = false`: vertical escape does not detach.
10. Window-creation failure rollback path does not lose the tab.

The failure path likely needs a temporary debug hook or forced `TermWindow::new_window`
failure so it can be verified intentionally.

## Alternatives Considered

### Create the GUI window first, then move the tab later

Rejected for phase 1.

It is safer than naive immediate move, but it has two downsides:

- the new window may appear briefly empty
- the source tab would snap back visually until the async move completes unless
  extra temporary state is added

The rollback-safe immediate move with deferred prune gives a cleaner UX.

### Commit detach on `mouse_leave_impl()`

Rejected for phase 1.

That would create the new window before release but still would not provide a
real drag handoff to the new window. It also makes accidental leave events more
dangerous.

### No separate config gate

Possible, but not recommended.

Detach is more invasive than reorder, so separating the gate makes rollout and
user preference management cleaner.

## Phase 2: True Live Detach Handoff

If phase 1 lands cleanly, the next step toward full Chrome-style behavior is:

1. arm detach before release
2. create target GUI window during drag
3. transfer drag ownership from source `TermWindow` to target `TermWindow`
4. keep moving the new window under the cursor until release

That would likely require:

- a global drag-transfer registry keyed by new mux window id or `tab_id`
- a way for the target `TermWindow` to start in a temporary forced-tab-bar mode
  even when `hide_tab_bar_if_only_one_tab = true`
- backend validation that move/release events continue to be delivered in a way
  that makes cross-window handoff reliable

This should be treated as a separate project, not folded into the first detach
phase.
