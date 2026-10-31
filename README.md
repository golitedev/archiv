# Archiv

Archiv is a planned Rust-native Linux file manager for an Arch PC and a NAS.
It will present one searchable library across separate mounted filesystems,
keep every physical location visible, and make file operations explicit and recoverable.
Its native egui/eframe interface will use Archiv's documented desktop design system.

**Status: preparation scaffold only.** No graphical interface, scanning, configuration parser,
index, transfers, player integration, deletion policy, or filesystem mutation is implemented.
No NAS settings or files have been changed. The binary prints a scaffold notice and exits.

## Intended behavior

- Browse local folders and NFS-mounted NAS folders through a native Rust desktop UI.
- Visually combine same-named folders across configured sources. Explicit aliases can combine
  `movies` with `movies remote`, and `tv` with `tv remote`. No physical pooling or relocation.
- Keep source IDs and exact physical paths for every entry. Never silently deduplicate files.
- Search a cached library index; keep slow filesystem work off the UI thread.
- Open selected media in external mpv. Use external rsync for queued PC/NAS transfers,
  preferably over SSH for many-file workloads; NFS remains available for browsing/playback.
- Configure operation policies: protected structural paths, explicit allowed roots,
  selection/size limits, confirmations, recovery destinations, and restore behavior.
- Distinguish app-enforced policy from NAS permissions. A writable NFS mount remains writable
  to other programs; Archiv cannot make it globally safe.

## Start here

- [Agent and contributor rules](AGENTS.md) — authoritative project instructions.
- [Architecture](docs/architecture.md) — crate boundaries and planned data flow.
- [UI design](docs/ui-design.md) — palette, typography, controls and interactions.
- [Safety and permissions](docs/safety.md) — policies, recovery, race conditions and enforcement limits.
- [Development](docs/development.md) — toolchain, validation and implementation workflow.
- [Roadmap](ROADMAP.md) — small future milestones and current status.
- [Decision log](docs/decisions.md), [configuration sketch](examples/archiv.example.toml),
  and [asset policy](docs/assets.md).

## Check the scaffold

```sh
cargo fmt --all -- --check
cargo check --workspace --all-targets --locked
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test --workspace --locked
cargo run -p archiv --locked
```

Only standard-library placeholders and workspace path dependencies are present. egui, eframe,
SQLite and transport dependencies are deliberately deferred until their implementation milestone.
The current stable Rust toolchain is used; 1.85 is the edition-2024 scaffold baseline, not a tested
MSRV promise for future GUI dependencies. `Cargo.lock` is committed for the application workspace.

## Scope

Linux first, Wayland and X11. No Electron, Tauri, webview, JS/TS frontend, custom kernel module,
FUSE wrapper, disk-format migration or replacement of NFS. System services and external tools
(NFS, OpenSSH, rsync, mpv) are explicit integrations, not rewritten into the app.

## License

Archiv code and documentation use the [Zero-Clause BSD (0BSD) license](LICENSE).
