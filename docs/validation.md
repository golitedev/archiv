# First read-only browser validation

Date: 2026-10-31. This records evidence for the first M1 slice, not completion of every UI goal.

## Automated evidence

`cargo test --workspace --locked -j 2`: 11 tests, using synthetic temporary directory trees.

- Core (2): reject absolute/traversal relative paths; keep distinct non-UTF-8 filename bytes even
  when their lossy labels collide; escape control characters for display.
- Platform (2): bounded batch delivery; list symlinks without traversal; refuse entering an
  external symlink; preserve fixture bytes; cancellation before opening an unavailable source.
- Engine (4): folder-first browsing, hidden filtering and directory-only navigation; idle startup;
  replayed request admission; stale completion/cancel rejection; source switching and IO errors.
- UI (3): accessible source dialog, filename filtering, physical selection/details, directory and
  keyboard parent navigation; Ctrl+O/Escape with no implicit scan; theme controls synchronize
  framework Light/Dark style branches. egui_kittest accessibility checks remain enabled.

Formatting, workspace/all-target compilation and Clippy with warnings denied pass. The application
builds with the stable 1.98.1 toolchain on the development server; 1.95 is the toolkit minimum,
not a separate compiler version exercised in this session.

CLI smoke checks: `--help`, successful `--list` of a synthetic directory and failure for a missing
source. Partial results return nonzero rather than pretending a truncated listing is complete.
No real NAS path or SSH connection was used for tests or GUI verification.

## Native appearance and input

Built and launched the actual eframe/Glow executable under Xvfb with Mesa software rendering.
Inspected native light and dark windows at 1180×760, including list alignment, panels, selection
colors and disabled actions. A native XTest Ctrl+O opened the source dialog; Escape dismissed it.
The GUI used isolated XDG config/cache/state directories and a synthetic local sample library.
No browser, HTML mockup, webview or UI-control server was used.

Screenshots and logs are ignored developer artifacts under `artifacts/`; they are not release
assets or committed baselines. Tests run without needing those artifacts.

## Initial slice limits (mounted-source updates below supersede those items)

- Native X11/software OpenGL and egui synthetic-input coverage only. No native Wayland/compositor,
  hardware GPU, screen-reader, mixed-DPI/multi-monitor or fractional-scaling acceptance is claimed.
- Toolkit fallback typography, text-only actions and limited default glyph coverage remain visible.
  Target Inter/JetBrains Mono fonts, Lucide icons and desktop text-scale/motion discovery are pending.
  The reduced-motion toggle disables normal style animation, but complete platform scroll/animation
  adaptation is not implemented and must not be advertised as complete.
- A root path is user-selected and reopened for each request; it is not a permanent filesystem
  identity. NFS mount-disappearance/identity handling, source persistence and configuration parsing
  remain deferred. Source root symlinks and symlink navigation are refused in the current adapter.
- Traversal is one directory deep, limited to 50,000 entries. No recursive search/index, merge,
  filesystem notifications, mpv/rsync, transfer/recovery operation or permission editor exists yet.
- Cancellation is cooperative between entries, not interruption of a blocked kernel syscall.
  Exactly two reader workers and bounded queues prevent unbounded replacement threads. New work
  can report busy if they are occupied. Shutdown does not join indefinitely blocked IO workers.
- No automated architecture/asset validation task, packaging workflow or hosted CI is present.

## Optional mounted sources and collections (2026-10-31)

The expanded workspace suite has 20 passing tests: core 4, platform 5, engine 7 and native-egui
interaction 4. Added evidence covers exact-byte/case-sensitive folder merging, non-UTF-8 name
separation, duplicate file retention, four synthetic disk roots, recursive collection navigation,
physical browsing, unavailable contributors and refresh recovery. Configuration tests cover
validation, private atomic replacement, preservation after invalid saves, unknown/oversized input,
request replay and restart without a scan. The settings interaction test creates a source and
collection through the editor, saves them, browses and navigates Up. Test input waits for layout
and scrolls fields into view before clicking; accessibility checks remain enabled.

The NFS guard has synthetic mountinfo cases for missing/wrong/local/bind-subtree mounts and an
actual temporary local directory that must never be enumerated as an expected NFS export.
Temporarily disabling the guard makes that test fail; temporarily disabling directory grouping
makes the four-disk engine test fail. The correct implementations were restored and checks rerun.
No real NFS unmount, server modification or library mutation was used as a test.

Native Xvfb/Mesa runs at 1180×760 in Light and Dark used a separate synthetic four-disk library,
isolated XDG paths and XTest mouse input. Inspected idle startup with restored definitions, the
merged Movies row with four location contributors, physical Details, entering the title to
see four separate same-name files, and scrolling Settings to the collection-member fields. Captures/scripts/logs remain ignored artifacts. Headless `--list-collection` emits all
four physical contributors using the same engine. Fmt, all-target workspace check, Clippy with
warnings denied and workspace tests pass.

A read-only SSH inspection of the authorized NAS confirmed the disk/category layout; its personal
inventory/configuration was not copied into Git. Exports, mounts, permissions and contents were
not changed. Live NFS connection/reconnect acceptance, native Wayland, screen readers and mixed-DPI
acceptance remain unverified. NFS configuration remains system administration, and mount checks
are per read rather than continuous monitoring. Aliases, cached search and playback remain pending.

## File-manager polish, applications and guarded recovery (2026-10-31)

The final suite has **39 passing tests**: core 6, engine 11, platform 13 and UI 9 (one settings regression and eight interactions).
The settings regression preserves multiple collection folders on one source and the explicit root
allowlist through editing; this test fails against the previous editor implementation, which collapsed those
member paths. Additional coverage includes inline expansion/collapse and generation invalidation, recursive name
search with distinct physical hits, address navigation retaining source identity, source policy and
single-use preview/confirm/restore flows, literal external-process arguments, and generic example
configuration with recovery disabled. Existing byte-preserving identity and missing-mount tests remain.

Recovery tests use synthetic local trees only. They exercise protected folders/ancestors/subtrees,
virtual-target denial, stale entries and parent-symlink replacement, unsafe/private recovery storage,
missing NFS mounts, permission refusal, restore collisions, non-UTF-8 names, and prepared-manifest
reconciliation. Thread-local fault injection fails manifest persistence before and after rename:
the original or recoverable payload remains intact. Empty pre-manifest jobs are reconciled without
cleanup. Temporarily disabling protection evaluation makes its regression fail; disabling stale
name checks makes the source-preservation regression fail. Implementations were restored afterward.
No NAS contents were moved, deleted or used as fixtures.

The UI tests create/save a generic source/collection, expand a folder without navigation, retain its
parent while filtering, save an application through OK, and cancel then confirm a physical recovery
move. The Delete shortcut requests a preview; the cancel path preserves the source. Accessibility
checks remain enabled. Manual 100/125/150/200% scales retain menus and the folder dialog in egui input
checks. Narrow logical widths fold side panels while menus retain source/collection access.

The actual native eframe/Glow application was inspected with Xvfb/Mesa and XTest mouse input in both
Light and Dark at a 1180×760 client area. Captures show text menus, full top-right search, neutral
home panels, aligned metadata headers without vertical resize lines, 26-point nested rows, meaningful
tinted icons, rounded selection and categorized Preferences. Native inspection caught and corrected
search-field squeezing and header/row misalignment. The Light recovery preview shows one physical
source/path and recovery destination; it was cancelled. Safeguards and appearance preferences were
also inspected. Captures and scripts remain ignored under artifacts; no HTML mockup or webview was
used. Default egui fonts remain in use; Inter is not claimed as shipped.

Fmt, workspace/all-target check, Clippy with warnings denied and workspace tests pass. Headless
--list --search was exercised on the synthetic tree. Lucide SVG hashes match the pinned provenance
record; licensing is retained beside assets. No vendor/toolkit patch or private design dependency.

Deployment limits remain: no writable-NFS rename/fsync/disconnect/server-crash or real-library
mutation acceptance. A read-only NAS inspection found NFS inactive. Server/client setup needs the
intended PC's LAN address/access; no packages, exports, services, mounts or permissions were changed.
Native Wayland, hardware GPUs, screen readers, mixed-DPI and native high-scale compositor acceptance
remain unverified. See file-actions.md for persisted identity, durability and final-rename race limits.
