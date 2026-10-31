# Native UI acceptance checklist

Status: requirements for later implementation; no GUI or UI test harness exists yet.
Updated: 2026-10-31.

Use this checklist when implementing a UI slice. Run against synthetic local sources first; use
injected errors/offline states rather than altering the real NAS. Record what was checked, how,
and what remains unverified. Inapplicable cases may be explained, not silently marked passed.

## Appearance and layout

- Light, Dark and System preferences agree across main panels, menus, popups and dialogs. Saved
  manual choices survive restart; System responds to desktop changes and has a defined fallback.
- Text, fields and icons remain legible at 100%, 125%, 150% and 200% display scale and with desktop
  large-text settings. Check small windows and moving between displays where available.
- Body/heading/monospace roles, outlines, radii, spacing, selection and hover/pressed states match
  the design tokens. Dialog fields and disabled controls stay distinguishable in both themes.
- Long names, mixed scripts, missing glyphs, RTL text and non-UTF-8 names neither clip controls nor
  change the physical path used by an action. Capture fonts/assets are approved and reproducible.

## Navigation and accessibility

- Complete the slice using only keyboard input. Verify tab order, visible focus, Enter activation,
  Escape cancellation and returning focus after menus/dialogs close. Inputs consume editing keys.
- Inspect accessible roles/names/states for text inputs, icon buttons, tabs, lists, progress and
  splitters. Status does not depend only on color or tooltip text. Disabled actions explain why.
- Test selection through filtering, sorting and incremental scan results. Duplicate names on
  different sources retain distinct identities; virtual categories cannot become physical targets.
- Check IME/preedit, non-Latin shortcuts, copy/paste and pointer/keyboard selection. A single gesture
  produces one action and only one cursor owner. Cancelled gestures cannot fire after navigation.

## Responsiveness and platform behavior

- Exercise empty, loading, partial, stale, offline, read-only, error and large-list views. Background
  work stays bounded and the interface remains usable while a source is slow or disconnected.
- Reduced motion changes timing without changing action results or scroll distance. Hidden/idle
  windows do not repaint or rescan continuously. Reopening a view does not leak preview resources.
- Check native Wayland and X11 separately when supported; document unavailable platform coverage.
  Startup/render failures are actionable and do not trigger file operations as fallback behavior.

## Commands and operation safety

- Menu, toolbar, shortcut and engine invocation share IDs, enabled state and policy outcomes.
  A forced layout replay issues each command once; cancellation and retry do not duplicate jobs.
- Before any mutation slice is enabled, validate physical previews and stale-plan refusal through
  the headless engine as well as native UI. A screenshot alone is not proof of safe execution.
- Destructive confirmations have a safe initial focus, name the exact locations and survive neither
  held-key repeats nor an accidental click-through. Closing the dialog performs no operation.

## Evidence and completion

Use native screenshots for appearance, interaction/accessibility tests for inputs and roles, and
engine tests for data behavior. Add failure regressions that demonstrably fail without the fix.
Keep captures out of Git unless they are approved, synthetic and useful as maintained baselines.
Do not add implementation, snapshot files or test dependencies just to complete this checklist
while Archiv remains in preparation. Future tooling must be real and documented before claiming it.
