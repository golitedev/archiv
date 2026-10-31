# Development

Status: configurable native browser with opt-in recovery, updated 2026-10-31.

## Toolchain and execution

Use stable Rust, at least 1.95, with rustfmt and Clippy. Toolkit dependencies are pinned by Cargo.lock:
egui/eframe/egui_extras 0.37, using the Glow OpenGL renderer with Wayland/X11, clipboard, AccessKit
and presentation persistence. No WGPU backend or runtime font download is enabled.
The application is Linux-only: descriptor-relative browsing uses rustix/openat2 and procfs.

```sh
cargo run -p archiv --locked
cargo run -p archiv --locked -- --browse /absolute/fixture/folder --theme light
cargo run -p archiv --locked -- --list /absolute/fixture/folder
cargo fmt --all -- --check
cargo check --workspace --all-targets --locked -j 2
cargo clippy --workspace --all-targets --locked -j 2 -- -D warnings
cargo test --workspace --locked -j 2
```

`--list` does not create a window or save preferences. It returns an error for unreadable sources,
partial/limited results, and 30 seconds without worker progress. Normal filenames are escaped for
one-line output; output is human-readable, not a byte-preserving machine protocol.

GUI preferences/window state use eframe's Linux configuration location under the `archiv` app ID.
Theme, reduced-motion preference and framework window state use eframe persistence. Explicit library
definitions use the separate atomic JSON store described in nas-setup.md. Temporary Open folder
sources, queries and media inventories are not persisted. Restoring definitions performs no scan.

## Crate workflow

Read AGENTS.md and the relevant crate README before changing behavior. Keep domain and filesystem
adapters free of GUI dependencies. Use scoped tests while working; the current workspace is small
enough for the final workspace sweep. Bound build parallelism to two jobs. Add tests for meaningful
identity, IO, cancellation and command-routing behavior; do not test empty wrappers.

The unit tests cover path byte preservation, no-follow browsing, batching, cancellation and stale
results. The UI integration test uses egui_kittest's default accessibility checks and synthetic
sources for dialog/filter/select/navigation, keyboard cancellation and shared theme branches.
Native appearance checks complement these tests; see [validation](validation.md). No custom control
server, CLI automation protocol or screenshot endpoint exists.

The UI uses verified Inter/JetBrains Mono build inputs (docs/fonts.md), with toolkit fallback,
attributed Lucide SVGs and text menus. Additional font work follows
[asset guidelines](assets.md). Do not add product dependencies or a global task framework merely
to replace the small worker and command abstractions. No CI/push/publication workflow is configured.

## Architecture and safety

Run all user intents through the headless Browser and shared FrameCommands admission filter.
Never introduce IO into a rendering frame. Worker queues, event payloads and total listings are
bounded. A cancelled syscall may remain blocked in the kernel; do not join it on the UI thread
or spawn unlimited replacements. Read metadata without following links. Do not restore navigation
from persisted paths automatically. Recovery/restore writes use the typed engine/descriptor-relative platform adapter described in
file-actions.md. New mutation capabilities still require the safety evidence in safety.md; no direct
filesystem or process calls belong in widgets.

Review actual native output using the [UI checklist](ui-review.md). Use temporary synthetic files,
isolated XDG paths and an appropriate display session. Keep captures under ignored artifacts/;
never commit real inventories or private paths. Diagnose the chosen toolkit's actual APIs before
adding workaround code or fork-specific calls. Add synthetic regression tests for failures and
verify they fail without the fix. Update progress/status honestly and commit the request's changes.
