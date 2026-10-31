# Archiv — instructions for agents and contributors

Read this file, `README.md`, `ROADMAP.md`, and the relevant `docs/` files before work.
These rules define Archiv's engineering, interface and data-safety requirements.
`CLAUDE.md` points here; do not maintain a competing set of rules. User instructions take priority.

## 1. Scope and truthful progress

- Archiv is a pure Rust Linux desktop file manager. It does not implement a filesystem.
  Rust owns product code and build tooling; no handwritten JS/TS, Electron or webview frontend.
- Use maintained egui/eframe releases for the UI. Toolkit forks and vendored patches require
  a documented need; do not add them as part of routine scaffolding.
- Native browsing, optional mounted-source collections and opt-in recovery slices are implemented.
  Follow the user's requested slice; ROADMAP tracks remaining UI, NFS acceptance and transfer work.
  Never present planned behavior as shipped. Update ROADMAP honestly.
- Never run development experiments on the user's real NAS library. Use temporary synthetic
  trees. Do not change NAS ownership, permissions, mounts, exports, services or contents unless
  the user has authorized that work. Build fixtures locally; no real deletion tests on mounted data.
- Complete authorized work, run appropriate checks, stage only the request's changes, and commit
  before the final response. Report the commit at the top when code changed. Do not push, publish,
  or create a GitHub repository unless requested. No unrequested delegation.

## 2. UI contract

Read `docs/ui-design.md` before any UI change and use `docs/ui-review.md` for acceptance.
- Maintain the documented Archiv design tokens: light/dark/system themes, quiet neutral
  surfaces, blue accents, Inter typography, Lucide SVG icons, shared geometry and spacing.
- Use Archiv branding and original or properly licensed visual materials.
- Use one theme/token module and reusable widgets. No scattered colors, fonts or button metrics.
  Share buttons, tabs, navigation and field widgets within Archiv. Evaluate any external UI
  component for compatibility, maintenance and dependency cost before adopting it.
- Business logic and filesystem/process work belong outside rendering. UI sends typed commands
  and receives bounded events. Never scan NFS, launch transfers or wait on processes in a frame.
- Provide accessible names, visible keyboard focus, non-color status cues, keyboard navigation,
  logical text order, IME-safe input, scaling and reduced-motion behavior. No Unicode/emoji icons.
- Stable item IDs must include source identity. A display label or row index is not a file ID.
- Repeated egui layout passes must never repeat commands, launches or mutations. Dispatch intents
  with unique request identities; deduplicate before executing effects, not by counting paint calls.
- Menus, toolbars, shortcuts and any future automation share command IDs, availability checks and
  policy decisions. Layout code does not invent a second command or permission registry.
- Disabled actions explain why. Virtual collections never masquerade as physical directories.
  Dangerous previews list physical targets and sources. No hidden selection expansion.
- Check real native UI output when UI is implemented. Use synthetic filenames and data in captures.
  Do not substitute HTML mockups for native UI verification or claim a screenshot test exists today.

## 3. Architecture and operations

Follow `docs/architecture.md` and `docs/safety.md`.
- `archiv-core` owns domain types and deterministic rules, without IO or GUI dependencies.
  `archiv-platform` owns filesystem, process and transport boundaries. `archiv-engine` orchestrates
  use cases; `archiv-ui-egui` renders them; `apps/archiv` wires the executable.
- All user actions must be callable and testable through the engine without opening the GUI.
  Future CLI/automation adapters reuse that engine. No hidden side-effect route from widgets.
- Physical source identity is preserved through merging, selection, sorting, filtering, transfers
  and recovery. Identical names do not imply identical files. Never automatically merge on disk.
- Default future mutation behavior is deny-until-configured; Delete requests recovery, not purge.
  Never infer permission from a button state, path prefix, ownership display or successful scan.
- Validate and revalidate targets at execution. Path traversal, symlinks, disappearing mounts,
  stale plans and races must fail closed. Canonicalize-then-prefix-check alone is not a confinement
  mechanism. UI restrictions never replace engine rules or NAS permissions.
- Invoke external processes with structured arguments; no shell interpolation of filenames.
  Transfers never implicitly add `--delete` or remove source files. Honor cancellation honestly:
  completed work may remain, and cancellation is not rollback.
- No sudo, Docker socket access or root application runtime as an implementation shortcut.
- No telemetry, automatic library uploads or unrequested network listeners. Credentials stay in
  the user's SSH agent/system secret store. Never commit keys, tokens, actual configs or inventories.
  Any future automation/control endpoint is opt-in, local by default and authenticated.

## 4. Never crash or lose data

- Use typed `Result` errors and `?`; explain what failed and what remains safe.
- No `unsafe` in workspace code. Prefer maintained safe wrappers for platform APIs.
- No `unwrap`, `expect`, `panic!`, `todo!`, `unimplemented!`, or `unreachable!` in production code.
  Tests may use assertion helpers; test-only Clippy exceptions are configured in `clippy.toml`.
- Treat filenames, metadata, configs and process output as untrusted input. Preserve non-UTF-8
  paths internally with `PathBuf`/`OsString`; a lossy display label must never become an operation path.
- Use bounds-checked access and checked/saturating arithmetic for input-derived values. Bound
  allocation, traversal depth, worker concurrency, queues and output capture. Detect cycles.
- Recover from worker failure/lock poisoning without performing fallback deletions. Do not swallow
  errors or use panic-catching as the primary error model. Keep the default unwind strategy.
- No silent overwrite, purge, permission escalation or destination fallback when a mount is absent.
  Persist settings/journals atomically with documented durability; crashes must not orphan data.
- Every crash, confinement or data-loss fix needs a meaningful synthetic regression test. Verify
  that the regression actually fails without the fix. Use a calendar library for date conversion
  when dates are introduced; use monotonic time for progress deadlines and animation.

## 5. Assets, reuse and licensing

`docs/assets.md` governs visual resources and code reuse. Verify redistribution rights before
adding material. Keep any required third-party notices beside the dependency or asset concerned.
Use Lucide SVGs and openly licensed fonts from maintained releases; no proprietary graphics,
unknown-origin files, copied branding or redistributed system fonts. Pin and verify external font
inputs rather than committing font binaries. Review dependency-bundled resources as well.
Choose dependencies compatible with Archiv's project licensing; external programs remain separate
integrations with their own terms. Only openly reusable or contributor-original assets are allowed;
"free for personal use" and unverified source claims are insufficient. Runtime system-font fallback
must never become bundled output. Refer to the asset document before adding or publishing resources.

## 6. Verification and development habits

- Every new crate inherits workspace lints. Keep GUI dependencies out of lower layers.
- Review arithmetic and path handling, function behavior on malformed input, module responsibilities,
  and whole-project safety assumptions as changes are made.
- Run fmt, check, Clippy with warnings denied, and relevant tests before committing. Bound build
  parallelism (normally `-j 2`). The current workspace permits a workspace sweep; grow toward scoped
  checks as the project grows. Do not claim asset/layer/GUI tooling exists until implemented.
- Tests use synthetic fixtures, not personal files, NAS paths or real SSH credentials. Add meaningful
  tests for merge identity and destructive decisions before enabling mutation, not tests of empty stubs.
- A UI feature needs headless engine evidence and native interaction/appearance checks before it
  is marked complete. A compiling placeholder or screenshot alone is not functional evidence.
- Keep public docs and status aligned with reality. Document only Archiv's actual tooling,
  implementation status and requirements; do not rely on private repositories or outside workflows.
