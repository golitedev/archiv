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

## Limits and remaining acceptance

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
