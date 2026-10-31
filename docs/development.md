# Development

Status: scaffold workflow, updated 2026-10-31.

## Prerequisites and current commands

Install Rust stable with rustfmt and Clippy. `rust-toolchain.toml` requests those components.
The workspace uses edition 2024. Its baseline `rust-version = 1.85` covers the empty scaffold only;
raise it explicitly when selected dependencies require a newer compiler. No GUI development
libraries, network access, NAS credentials or mounted shares are needed for the current checks.

```sh
cargo fmt --all -- --check
cargo check --workspace --all-targets --locked -j 2
cargo clippy --workspace --all-targets --locked -j 2 -- -D warnings
cargo test --workspace --locked -j 2
cargo run -p archiv --locked
```

The run command prints a preparation notice and exits. The tests currently discover zero tests:
that verifies compilation, not file-manager functionality. No UI or safety test harness exists yet.
When adding dependencies, update and commit Cargo.lock deliberately. Runtime installation,
packaging dependencies and exact egui/eframe features belong to the first UI milestone.

## Work loop

1. Read AGENTS.md, ROADMAP.md and the relevant design document. Pick only the requested slice.
2. Inspect existing code and interfaces before adding abstractions. Preserve the layer dependency graph.
3. Add meaningful tests where behavior warrants them, especially identity and operation policy.
4. Implement against synthetic temporary sources. Never borrow a real NAS library as test data.
5. Run scoped checks (`cargo test -p archiv-core -j 2`, etc.); workspace checks remain cheap at this stage.
6. Inspect native UI output when changing UI. Keep native tests/screenshots reproducible and synthetic.
   Use [the UI acceptance checklist](ui-review.md), including replayed-layout command dispatch,
   focus/accessibility, theme consistency and fractional display scaling.
7. Update docs/status and asset notices when relevant, review the diff, stage the task's files and commit.

No automatic PR, push, publishing or CI workflow is set up. All project instructions live in
this repository. If architecture/assets checks are later introduced, document the actual
command and hook it into validation; do not reference hypothetical xtask commands as existing tools.

## Future implementation conventions

- Typed errors with actionable context; cancellation and partial completion are explicit job states.
- Stable opaque IDs, bounded queues, per-source worker budgets and generation-tagged scan results.
- Rust tests for domain rules; injected filesystem/process adapters for failure tests; native egui
  interaction tests after toolkit adoption. Add crash/path-confinement regressions before mutation.
- Settings/index/journals follow XDG base directories; machine-specific overrides live outside Git.
- Keep filenames byte-preserving and separate from presentation. Escape labels/logs, never rewrite
  a physical path from how it looks in a textbox or command preview.
- Prefer upstream maintained dependencies; audit safe APIs, bundled assets, licenses, build cost and
  platform coverage. Production Rust dependencies can internally use native libraries; the promise
  is a Rust application/UI with no web frontend, not an OS free of native dependencies.
- `std::process::Command` starts mpv/rsync/SSH; using established external tools is intentional.
  An all-Rust transport backend is not a requirement of a pure Rust interface.
- No perpetual repaint/scan loop; responsive UI and low idle resource use are acceptance criteria.
