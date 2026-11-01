# Archiv

A Rust-native Linux file manager for local folders and mounted NAS sources. Optional collections
combine folders visually across disks while preserving each physical location.

- One File/Edit/View/Help and search row, bottom editable address, compact expandable file rows,
  draggable header dividers and a Details pane. Themes are configured only in Preferences.
- Configurable sources/collections, no built-in media categories, no automatic startup scanning.
- Right-click physical files/folders for Open, Copy, Copy path, Paste, Rename and permanent Delete.
  Copy/Paste uses Archiv's session clipboard. Delete first shows every physical target and requires
  explicit confirmation. File changes are denied until configured in Preferences → Safeguards.
- Configurable imv/mpv/kwrite or other application executables, launched without shell interpolation.
- Protected folders/subtrees and source/collection roots; no-overwrite rename/copy. Missing
  configured NFS mounts fail closed. Linux mounts the actual NFS connection separately.

## Build and run

Stable Rust 1.95+, rustfmt and Clippy. Run the GUI on your graphical Linux PC with Wayland/X11
and OpenGL. Paths refer to that PC. Linux openat2 (kernel 5.6+) and procfs are required.

```sh
cargo run -p archiv-fonts --locked
cargo build --release -p archiv --locked -j 2
./target/release/archiv
```

Fonts are verified build inputs embedded in the executable; builds without inputs use toolkit
fallbacks. No runtime downloads. [Font preparation](docs/fonts.md), [asset notices](docs/third-party-assets.md).

Ctrl+O opens a folder, Ctrl+L edits the bottom address, Ctrl+F focuses search, F5 refreshes and
Alt+Up navigates to the parent. Folder arrows expand in place; double-click/Enter opens.
Search filters visible rows; Enter explicitly searches subfolders. Ctrl+C/ Ctrl+V copy/paste,
F2 renames, Delete requests confirmation. Editing text consumes its normal input shortcuts.
View → Cancel operation requests cancellation; completed changes may remain.

Library version-1 JSON uses `sources`, `collections`, `openers` and `writes`. It is saved at
`$XDG_CONFIG_HOME/archiv/library.json` or `~/.config/archiv/library.json`; `--library FILE` chooses
another path for the library file. Theme, UI preferences and window/layout state are stored in
`$XDG_CONFIG_HOME/archiv/app.ron` (normally `~/.config/archiv/app.ron`). All default application
settings live together in `~/.config/archiv/`. Open folder adds a temporary source; save one in Preferences before enabling writes.
[Example](examples/archiv.example.json), [NAS setup](docs/nas-setup.md), [file actions](docs/file-actions.md).

## Limits and checks

There is no undo service, Cut, multi-selection, indexed search or queued transfer scheduler.
File actions refuse links, special files and nested mounts. Copy does not preserve metadata;
failed copies can retain named partial staging. Cancellation is not rollback. The final filename
race and durability limitations are documented in [safety](docs/safety.md). App safeguards govern
Archiv only, not external applications, SSH or other NAS clients. Live writable-NFS/disconnect,
Wayland/hardware and complete accessibility acceptance remain open.

```sh
cargo fmt --all -- --check
cargo check --workspace --all-targets --locked -j 2
cargo clippy --workspace --all-targets --locked -j 2 -- -D warnings
cargo test --workspace --locked -j 2
```

[Architecture](docs/architecture.md), [UI design](docs/ui-design.md), [UI acceptance](docs/ui-review.md),
[validation](docs/validation.md), [roadmap](ROADMAP.md), [request audit](docs/request-audit.md).
Code/docs: [0BSD](LICENSE). Assets/dependencies retain their own terms. No telemetry, root runtime,
network listener or toolkit fork. Synthetic captures and build artifacts stay out of Git.
