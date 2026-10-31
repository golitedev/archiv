# UI design and interaction contract

Status: implementation requirements, not existing UI. Updated: 2026-10-31.
This document defines Archiv's native desktop appearance and interaction requirements.

## Character and toolkit

A quiet, compact desktop tool: restrained neutral surfaces, clear hierarchy, thin separators,
blue interaction accents, readable labels and consistent controls. Give file lists the useful
space; avoid decorative cards, large banners and marketing copy. Use Archiv's own identity.

Use Rust egui/eframe with Wayland/X11, clipboard and AccessKit support. There is no web runtime.
Build a small shared set of buttons, fields, tabs and navigation controls in the UI crate. Prefer
maintained toolkit releases; evaluate external components before adopting them. GUI dependencies
are not installed yet. Centralize appearance tokens and layout metrics rather than duplicating them.

## Color tokens

Use the following baseline tokens consistently. Reference semantic roles from widgets rather
than embedding RGB literals in rendering code. Add destructive and warning roles only with
light/dark contrast checks and clear non-color indicators.
System theme follows desktop preference, with explicit Light/Dark overrides. Persist the user's
choice; handle missing desktop settings without failing startup. Default to System on first launch,
with Light as the initial fallback when the desktop supplies no preference. While following System,
react to desktop changes; an explicit Light/Dark choice stays fixed until the user changes it.

Apply the resolved theme to native chrome, menus, combo popups, tooltips, dialogs and text inputs,
not just the main panels. Theme controls in different places share one saved preference and setter.
Do not let a framework popup switch to a different palette after repaint or restore.

| Token | Light | Dark |
| --- | --- | --- |
| `titlebar` | `#E9E9EB` | `#1B1B1E` |
| `chrome` | `#FFFFFF` | `#26262A` |
| `panel` | `#FFFFFF` | `#26262A` |
| `pasteboard` | `#F1F1F3` | `#19191C` |
| `card` | `#FFFFFF` | `#2E2E33` |
| `border` | `#DADADE` | `#3C3C43` |
| `divider` | `#E8E8EB` | `#333339` |
| `text` | `#222226` | `#ECECEF` |
| `text_muted` | `#5E5E66` | `#AEAEB6` |
| `text_faint` | `#6B6B73` | `#97979E` |
| `icon` | `#44444B` | `#D4D4DA` |
| `hover` | `#F0F0F3` | `#34343A` |
| `pressed` | `#E4E4E9` | `#3E3E45` |
| `selected` | `#E6EEFD` | `#233A63` |
| `accent` | `#1B63E0` | `#4B8BF5` |
| `accent_text` | `#1755C4` | `#8CB6FA` |
| `accent_soft` | `#E3ECFD` | `#243657` |
| `field` | `#FFFFFF` | `#1E1E22` |
| `badge_new` | `#1B63E0` | `#3D7DEE` |

Use `pasteboard` for the file-list workspace, `chrome`/`panel` for navigation and toolbars,
`card` for dialogs/inspectors, `field` for text entry, `accent_soft` for selected controls and
`selected` for stronger pressed/selection states. Retain readable text on every state. Dialog/window
shadows use offset (0, 8), blur 28, no spread, black alpha 38 (light) / 110 (dark). Popup shadows use
offset (0, 4), blur 16, no spread, black alpha 30 (light) / 90 (dark). Treat shadows as elevation cues;
never use them as the only boundary of a field or selected row.
Do not make selection, warning, permission or availability distinguishable by color alone.

## Typography, geometry and controls

- UI face: Inter Regular, Medium and SemiBold. Monospace: JetBrains Mono for paths/details when
  appropriate, not every filename. Open assets only; no committed font binaries. See assets.md.
  A temporary egui default-font shell must be documented as visually incomplete, not a match.
- Baseline sizes in logical points: body/buttons 13, secondary/small 11, headings 17 semibold.
  Respect system/user text scaling, HiDPI and font fallback; do not impose a minimum physical pixel size.
- Item spacing: horizontal 8, vertical 6. Button padding: horizontal 10, vertical 5.
- General control radius: 6. Windows/modals: 10. Menus: 8 with 6-point margins.
- Fields: minimum height 30, horizontal inset 10, vertical inset 6. Dialog controls use visible
  outlines and 4-point control corners. Dialog content padding: horizontal 20, vertical 16.
- Borders: normally 1 logical point; use a clearly visible 1.5-point accent for dialog focus.
  A field needs an identifiable boundary, not just a low-contrast background fill.
- Floating scrollbars: baseline width 8, collapsed width 3. Preserve usability with keyboard,
  touchpad, scaling and platform accessibility preferences; do not hide the only navigation cue.
- Tooltip delay baseline: 350 ms. Tooltips supplement labels and accessible names, never replace
  them. Disabled actions explain the reason, including denied policy and offline-source conditions.
- Truncate display labels on character/grapheme-safe boundaries, keep paths available through
  details/copy, and bound expensive text layout. Full underlying path bytes remain untouched.

## Display scale and font fallback

Provide Auto interface scaling and a saved manual override. Auto follows display scale and desktop
text preferences without applying either twice. Define manual percentages consistently, document
their meaning, and keep icon/text metrics in logical points. Recalculate layout and cached visual
resources when moving between differently scaled displays. Font baselines, field height, clipping
and line spacing must be checked at ordinary and fractional scales, not just enlarged screenshots.
Do not patch font metrics or text rasterization globally without native evidence of the problem.

Regular, Medium and SemiBold faces use the same ordered fallback coverage. Support mixed-script
filenames without shifting neighboring baselines or silently dropping characters. Missing glyphs
need a visible fallback and the exact original path in the engine. A runtime-installed fallback
font may be read in place, but must not be copied into artifacts or redistributed. Reproducible
captures use only approved fonts and report missing coverage rather than relying on the host.

## Icons and motion

Use Lucide SVGs from a pinned release, preserving required notices when assets are introduced.
Tint icons from semantic tokens; the baseline SVG stroke width is 1.75. Render at the actual
on-screen scale. Do not use emoji, Unicode glyphs, icon fonts or proprietary stock imagery as icons.
Each icon-only button has a meaningful accessible name, tooltip, keyboard activation and focus ring.
Unknown icons must not silently turn a dangerous action into an ambiguous symbol.

Keep hover and press feedback subtle: quadratic-out easing around 120 ms for hover and 60 ms
for presses, up to a 1-point offset and 8% icon compression. These are baseline limits,
not a requirement to animate every row. Honor reduced-motion settings with immediate feedback;
no kinetic animation stacking, idle repaint loop, flashing progress or animation required for meaning.
Use monotonic time. Preserve normal egui scrolling behavior unless evidence warrants customization.
Reduced-motion handling covers popup transitions, programmatic reveal/scroll, list effects and
spinners as well as buttons. Offer System/On/Off; unknown system settings must not break startup.
Turning animation off changes timing, not the distance of a scroll or the result of an action.

## Archiv layout and content

- Left navigation: merged collections (Movies, TV, Anime), physical sources, local folders,
  recovery and transfer queue. Virtual collection icons/labels differ from physical source locations.
- Top area: navigation/breadcrumbs, search, source filters and contextual actions. A future Ctrl+K
  command palette shares command definitions with menus and shortcuts. Do not add a second action registry.
- Center: virtualized file/title list, with optional later grid view. Visible source badges and
  contributor counts resolve duplicate locations; selection stays stable through sort/filter updates.
- Optional details pane: exact physical locations, size/status, observed permissions and policy
  explanation. Keep pane widths/visibility as presentation state, independent of domain state.
- Bottom/queue: real transfer/scan state with progress, retry and honest cancellation results.
  No invented percentage when totals are unknown. Folder-size discovery is cancellable background work.
- Native mpv opens an explicit physical file. Show which location is chosen when duplicates exist.
  Slow operations don't block selection, typing, scrolling or closing the window.
- Use plain language: "Move to recovery", "Restore", "Copy" and eventually "Permanently delete".
  Do not call a recoverable move "freed space". Avoid implementation jargon in routine product flows.

## Input, accessibility and localization readiness

Use meaningful stable widget IDs and proper roles/names, including text fields, icon buttons,
tabs, lists, checkboxes and panel splitters. Plan AccessKit support and test the accessibility tree
when UI exists. Every primary workflow must support keyboard operation.

Keyboard paths: Tab/Shift+Tab focus, Enter activation/open, Escape cancel/dismiss, normal Linux
Ctrl shortcuts and selection modifiers. Destructive keys use the same engine command/policy route
as buttons; Shift+Delete must never secretly bypass recovery. Context-menu and toolbar actions
must agree. Text entry consumes editing shortcuts before file actions; typing in search must never
delete selected rows. Drag/drop requires a destination preview and explicit move/copy semantics.
Dialogs have a deliberate focus order, an accessible title and a safe cancel route; opening them
must not place a destructive action under a preexisting held key. Destructive confirmation never
runs from key repeat or a generic default-Enter handler. Focus returns to the invoking control when
possible. Context-menu dismissal cannot activate a row underneath it. Command names and shortcut
labels come from shared definitions and remain consistent if interface translation is added.

Preserve IME preedit/commit behavior, Unicode text, clipboard semantics, non-Latin layouts and
long filenames. Keep user text and accessible labels in logical order; do not reverse strings for
RTL. UI chrome may mirror in a later translation layer, but exact paths/extensions must remain
unambiguous and copied text unchanged. Keep source paths byte-preserving even when labels are lossy.

Prevent ambiguous overlapping hit targets. One input gesture has one action owner; Escape cancels
captured gestures. No independent shortcut/pointer handlers may both execute the same operation.
No files are modified as a side effect of hovering, previewing, opening a menu or changing selection.

## Frame lifecycle and effect ownership

Immediate-mode rendering may replay layout within a frame. Admit each user intent once using a
request ID and relevant view/plan generation; repeated widget evaluation cannot enqueue another
transfer, launch mpv again or move a second item. Accepted work is not rolled back merely because
a drawing pass is discarded. Keep engine jobs independent of repaint count and UI visibility.

Own pointer gestures and cursor choice centrally for each overlapping region. A completed gesture
has one target; cancelled drag/press state is not reused on pointer release or after navigation.
Use appropriate platform cursors for links, editable text and resizing, and preserve visible focus.
Do not poll raw pointer state independently from several widgets to decide the same operation.

## Rendering, testing and acceptance

The UI renders engine state, dispatches intents and consumes bounded events. No filesystem scan,
SSH connection, rsync wait, index query or mutation may block the frame loop. Prefer a bounded
worker model and list virtualization over repeated whole-library work. Respect occluded/minimized
windows and repaint only for state/input/animation changes. Bound thumbnail decoding, texture uploads,
GPU cache memory and work admitted per frame as visual previews are introduced. Late results carry
source/view generations; stale results cannot replace newer entries or leak textures. Release
unused visual resources when views close. A renderer startup/device failure reports an actionable
error; evaluate a supported fallback rather than assuming every Arch GPU works. Do not implement
renderer recovery or a custom rendering backend in this documentation-only stage.

Before a UI slice is called complete, inspect actual native light/dark output on synthetic data,
keyboard and focus behavior, scaling, empty/loading/offline/error states, long labels and duplicate
locations. Add egui interaction/accessibility tests once tooling exists. Linux Wayland and X11
support is a target requiring evidence, not something supplied by this document. Never publish real
NAS paths, personal media lists, account data or screenshots containing unapproved assets.

Use the [native UI acceptance checklist](ui-review.md) for each implemented slice. Record actual
coverage and outstanding limitations; requirements in these documents are not passing tests.

## Destructive UI rules

Virtual categories and whole-source roots have no delete command. A mutation preview names every
physical contributor affected, its source, estimated size and recovery destination; a title that
exists on multiple disks requires explicit scope. Show denied rules and unavailable capabilities.
Confirmation is bound to a plan revision and revalidated by the engine; a UI toggle cannot grant
filesystem authority. Permission editing and permanent purge are separate workflows, not context
menu shortcuts hidden behind a modifier. See safety.md for enforceable behavior and limitations.
