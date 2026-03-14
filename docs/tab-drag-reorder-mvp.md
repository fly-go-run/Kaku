# Tab Drag Reorder MVP Design

Status: Proposed

Last updated: 2026-03-14

## Summary

This document proposes a Chrome-like same-window tab drag reorder MVP for Kaku:

- Press on a tab starts a drag candidate.
- Once movement passes the drag threshold, the pressed tab becomes a floating overlay that follows the cursor.
- The base tab strip does not mutate mux state while dragging. Instead, it shows a spacer at the candidate drop slot and shifts the other tabs to make room.
- Reordering is committed only on mouse release.
- No animation, no detach-to-new-window, no cross-window dragging.

The design preserves the current tab bar feature set and minimizes behavioral churn outside of tab dragging.

## Current Behavior

The current implementation lives primarily in:

- `kaku-gui/src/termwindow/mouseevent.rs`
- `kaku-gui/src/termwindow/mod.rs`
- `kaku-gui/src/tabbar.rs`
- `kaku-gui/src/termwindow/render/tab_bar.rs`
- `kaku-gui/src/termwindow/render/fancy_tab_bar.rs`

Today:

1. Left mouse press on a tab activates it immediately if it is not already active.
2. The same press also initializes `TabDragState`.
3. On mouse move, once `max(dx, dy)` exceeds `Self::TAB_DRAG_THRESHOLD` (an associated constant on `impl TermWindow`, value `6`, see `mouseevent.rs:76`), `drag_tab()` calls `move_tab(...)` immediately via the neighbor-only helper `drag_tab_target_idx()` (`mouseevent.rs:108`), which only checks the immediately adjacent tabs (`current_tab_idx ± 1`).
4. The real mux tab order changes on every qualifying move.

This causes the current "jumpy" behavior:

- real tab order churns continuously during drag
- the dragged tab does not visually float under the cursor
- there is no persistent gap/spacer showing the candidate drop slot
- fancy and retro tab bars both inherit the same immediate-commit semantics

## Goals

- Match common browser behavior closely enough for same-window tab reorder.
- Make the dragged tab visually follow the cursor.
- Shift non-dragged tabs to show an obvious insertion slot.
- Commit the final order only on release.
- Preserve single-click tab activation.
- Preserve double-click rename, middle-click close, right-click tab navigator, wheel-tab-switch (independently gated by `mouse_wheel_scrolls_tabs`), and titlebar drag behavior.
- Support both `use_fancy_tab_bar = true` and `false`.
- Respect the existing `mouse_drag_reorders_tabs` config gate.

## Non-Goals

- Drag animation or easing.
- Dragging a tab out of the window to create a new window.
- Cross-window tab dragging.
- Changing the click-to-activate policy.
- Reworking tab title formatting or close button behavior.

## Constraints and Existing Structure

The implementation must work with two different tab bar paint paths:

1. Retro tab bar
   - Built as a single `Line` in `tabbar.rs`
   - Painted through `render_screen_line()` in `render/tab_bar.rs`
   - Hit regions come from `TabBarState::compute_ui_items()`

2. Fancy tab bar
   - Built as a box-model tree in `render/fancy_tab_bar.rs`
   - Cached in `self.fancy_tab_bar`
   - Hit regions come from the computed element tree

The drag logic itself is owned by `TermWindow`, so the design should keep runtime drag state there and pass only render-specific drag data into the tab bar builders.

Note: the empty titlebar area retains its own window-drag (`is_window_dragging`) priority via the existing UI-item dispatch order (`mouseevent.rs:529`). Tab UI items are matched first; blank title regions fall through to window drag. This means tab drag and window drag do not conflict for clicks that hit a tab.

## Proposed Design

### 1. Runtime Drag State

Replace the current minimal tab drag state with a richer runtime state. The important change is that runtime identity should be based on `TabId`, not only `tab_idx`.

Proposed fields:

```rust
struct TabDragState {
    tab_id: TabId,
    start_event: MouseEvent,
    start_bounds: UIItem,
    source_slot_idx: usize,
    target_slot_idx: usize,
    has_dragged: bool,
    overlay_offset_x: isize,
}
```

Notes:

- `tab_id` is the stable identity across re-renders and potential tab index shifts.
- `start_bounds` is the pressed tab rectangle at drag start, used as the overlay anchor.
- `source_slot_idx` and `target_slot_idx` are visual slot indices, not mux mutation history.
- `overlay_offset_x` is enough for the MVP because only horizontal dragging affects reorder.

### 2. Event Model

#### Mouse press on tab

Keep current behavior:

- If the pressed tab is not active, activate it immediately.
- Initialize `TabDragState`.
- Do not mutate tab order.

This preserves existing click semantics and avoids mixing the MVP with a separate click-activation redesign.

#### Mouse move before threshold

- Update nothing except normal mouse bookkeeping.
- `TabDragState` remains a drag candidate.
- No overlay is rendered.

#### Mouse move after threshold

Once movement exceeds `Self::TAB_DRAG_THRESHOLD`:

Note: the current implementation triggers on `max(dx, dy)`. The MVP should decide whether to keep this or switch to a horizontal-only (`dx`) threshold, since only horizontal movement affects reorder. Keeping `max(dx, dy)` is the safer default for the MVP.

- mark `has_dragged = true`
- compute `overlay_offset_x`
- compute the candidate `target_slot_idx`
- invalidate the window
- do not call `move_tab(...)`

#### Mouse release

On left release:

- If `has_dragged == false`, clear drag state and return.
- If `has_dragged == true`, commit exactly once using the final `target_slot_idx`.
- Clear drag state and invalidate the window.

#### Lost/stale drag cleanup

If drag state survives without a clean release event:

- on the next event with no left button held, clear the drag state without commit
- do not leave a persistent floating overlay
- note: `mouse_leave_impl()` (`mouseevent.rs:606`) currently clears hover state but does not clear `tab_drag_state`, so the stale-state-on-next-event approach covers cursor-leaves-window too — the next mouse event after re-entry will see no left button and clean up

This is sufficient for the MVP because detach-to-new-window is out of scope.

## Slot Resolution Algorithm

The MVP should use an absolute insertion algorithm, replacing the current `drag_tab_target_idx()` (`mouseevent.rs:108`) which only checks the immediately adjacent tabs (`current_tab_idx ± 1`) and therefore misses the correct slot when the cursor moves quickly across multiple tabs.

Inputs:

- all current tab UI bounds in visual left-to-right order
- dragged tab identity
- dragged tab overlay center x

Algorithm:

1. Collect the visible tab slots excluding the dragged tab.
2. Compute `dragged_center_x = start_bounds.x + overlay_offset_x + start_bounds.width / 2`.
3. Find the first remaining tab whose midpoint is strictly greater than `dragged_center_x`.
4. Insert before that tab.
5. If no midpoint is greater, insert at the end.
6. Clamp to `[0, tab_count - 1]`.

This handles variable-width tabs and fast pointer movement better than incremental adjacent swaps.

## Render Model

The key design choice is to separate:

- base strip layout
- dragged tab overlay

### Base strip layout

While dragging:

- the dragged tab is omitted from the normal tab list
- a fixed-width spacer is inserted at `target_slot_idx`
- the rest of the tabs render normally

When not dragging:

- behavior is unchanged

### Dragged tab overlay

While dragging:

- render the dragged tab again as a top-layer overlay
- horizontal position is `start_bounds.x + overlay_offset_x`
- style matches the tab's current active/inactive appearance
- overlay is visual only; it does not participate in hit-testing

This yields the Chrome-like "tab follows cursor while the strip makes room" behavior without mutating mux on every move.

## Shared Render Data

Add a small render-only structure derived from `TabDragState` each frame:

```rust
struct TabDragRenderInfo {
    dragged_tab_idx: usize,
    source_slot_idx: usize,
    target_slot_idx: usize,
    overlay_left_px: f32,
    overlay_width_px: f32,
}
```

`TermWindow` owns runtime drag state. The tab bar builders receive only the current render info for the frame.

## Retro Tab Bar Path

Retro mode is the harder path because it currently paints one full `Line`.

MVP plan:

1. Extend the logical tab bar model so the builder can emit a spacer entry.
2. Build the base `Line` without the dragged tab, but with blank cells matching the dragged tab width at `target_slot_idx`.
3. Keep `compute_ui_items()` generating hit boxes only for real tabs and normal controls.
4. After painting the base strip, paint the dragged tab title again as an overlay using `render_screen_line()` with a custom `left_pixel_x`.

Notes:

- Retro tabs do not currently embed close buttons in the tab itself, so there is no extra overlay control logic to preserve there.
- The overlay may be opaque in the MVP. Alpha and elevation styling can be added later.

## Fancy Tab Bar Path

Fancy mode already has per-tab element construction, so the base strip can be adapted with lower risk.

MVP plan:

1. Extend the fancy tab bar builder to understand a drag spacer item.
2. Build the normal element tree without the dragged tab.
3. Insert a transparent fixed-width spacer element at `target_slot_idx`.
4. Paint a separate overlay element for the dragged tab after the base tree has been painted.

Important cache behavior:

- changing only `overlay_left_px` should not require rebuilding `self.fancy_tab_bar`
- changing `target_slot_idx` does require invalidating the cached fancy tab bar via the existing `invalidate_fancy_tab_bar()` (`fancy_tab_bar.rs:54`), because the base layout changes

## Move Commit Semantics

The existing `move_tab(tab_idx)` helper (`mod.rs:3235`) always removes the **active** tab (`window.remove_by_idx(active)`) and inserts it at the target index. This works today because the current code activates the pressed tab on mouse-down, so the dragged tab is always active. But it is brittle for drag-release commit: if any future path allows dragging a non-active tab, or if an unrelated event changes the active tab during drag, `move_tab` would move the wrong tab.

For the MVP, add a dedicated helper:

```rust
fn move_specific_tab_to_slot(&mut self, tab_id: TabId, target_slot_idx: usize) -> anyhow::Result<()>
```

Behavior:

- resolve the current source index from `tab_id`
- remove that specific tab
- insert it at `target_slot_idx`
- set it active
- update title and scrollbar

This keeps drag commit stable even if unrelated state changes while the drag is in progress.

## Hover and Hit-Testing Rules During Drag

During an active drag:

- suppress hover-driven tab styling in the base strip
- do not give the overlay its own `UIItem`
- the dragged tab is omitted from the base strip entirely, which also eliminates its `UIItemType::CloseTab` entry in the fancy tab bar (the close button is controlled by `show_close_tab_button_in_tabs`, see `config.rs:522` and `fancy_tab_bar.rs:352`)
- the overlay should not emit any `UIItem` for close buttons either — it is visual only
- keep existing non-drag controls working again immediately after release

This avoids hover flicker and ambiguous input targets.

## File-Level Change Plan

Primary files:

- `kaku-gui/src/termwindow/mod.rs`
  - expand `TabDragState`
  - add helper(s) for resolving drag state to render info
  - add `move_specific_tab_to_slot(...)`

- `kaku-gui/src/termwindow/mouseevent.rs`
  - update `start_tab_drag()` (`mouseevent.rs:83`) to populate the new `TabDragState` fields
  - replace `drag_tab()` + `drag_tab_target_idx()` with the new absolute slot resolution and overlay-only update (no `move_tab(...)` calls during drag)
  - update release handling to use the new commit-on-release flow
  - add stale-drag cleanup

- `kaku-gui/src/tabbar.rs`
  - support spacer-aware logical tab strip construction
  - keep tab entry metadata for overlay rendering

- `kaku-gui/src/termwindow/render/tab_bar.rs`
  - paint the retro base strip with spacer
  - paint the dragged tab overlay after the base strip

- `kaku-gui/src/termwindow/render/fancy_tab_bar.rs`
  - add spacer element support
  - add a helper to build/paint the dragged tab overlay element

## Edge Cases

- Dragging the first or last tab should clamp cleanly.
- Dragging quickly across multiple tabs should resolve to the correct absolute slot.
- Releasing outside the tab bar but still inside the window should commit to the last computed slot.
- Releasing after moving back to the source slot should result in no-op reorder.
- If the dragged tab disappears mid-drag, cancel the drag without panic.
- If tab count changes mid-drag, recompute current index from `TabId`; if the tab no longer exists, cancel.
- `tab_bar_at_bottom = true` must behave the same as top tab bar.
- `mouse_drag_reorders_tabs = false` must continue to disable the whole feature.

## Testing Plan

### Unit-testable helpers

Add pure helpers for:

- computing `target_slot_idx` from slot midpoints
- building the visual slot list with a spacer
- resolving `TabId` to the current tab index

These are the pieces most likely to regress with variable tab widths.

### Manual verification

Required manual coverage:

1. Fancy tab bar drag reorder.
2. Retro tab bar drag reorder.
3. Top tab bar and bottom tab bar.
4. Single click activation still works.
5. Double click rename still works.
6. Middle click close still works.
7. Right click tab navigator still works.
8. Wheel tab switching still works.
9. Dragging with `mouse_drag_reorders_tabs = false` stays disabled.

## Alternatives Considered

### Keep immediate `move_tab(...)` and only improve visuals

Rejected. The render layer would still chase a constantly mutating mux order, which is exactly what makes the current interaction feel unstable.

### Support fancy tab bar first and leave retro unchanged

Rejected for the MVP. The feature would feel inconsistent and would complicate later cleanup more than it saves upfront.

### Delay tab activation until mouse release

Rejected for the MVP. It changes long-standing click behavior and is orthogonal to making drag reorder feel correct.

## Open Questions

These do not block the MVP, but should be confirmed before implementation:

1. Should the floating tab remain fully opaque, or use slight transparency/elevation?
2. Should release outside the window cancel instead of commit?
3. Should vertical movement beyond a future threshold be reserved now for detach-to-new-window, even though detach is out of scope for this MVP?

## Recommended Implementation Order

1. Add the new drag state and release-time commit helper.
2. Implement pure slot-resolution helpers and unit tests.
3. Implement retro spacer + overlay paint path.
4. Implement fancy spacer + overlay paint path.
5. Verify all existing tab-bar mouse behaviors manually.

This sequence keeps the hardest behavior isolated and testable before touching the cached fancy tab bar path.
