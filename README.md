# Archiv

A Rust-native Linux file manager for local folders and mounted NAS sources. Archiv can combine
folders from separate disks into configurable **visual collections**, without pooling disks or
moving their contents.

## Available now

- Native File/Edit/View/Help menus, a top-right search field, neutral home panels and Light/Dark/System
  preferences. Compact rounded rows use licensed, tinted Lucide icons for folders and file types.
- Explicit physical sources and optional collections with any names/folders you choose. There are
  no built-in media categories. Saved definitions never start a scan automatically.
- Ctrl+L edits a physical location; double-click/Enter enters a folder; single-click its arrow to expand/collapse it in place.
  Exact-name directories combine recursively; duplicate files stay separate with source labels.
- Type to filter the current/expanded view; press Enter in search to explicitly search subfolders.
  Recursive search is bounded, cancellable and returns physical locations rather than guessed duplicates.
- Open regular files with configurable executable names/paths, for example imv, mpv or kwrite.
  No shell command interpolation or automatic execution of the selected file.
- Opt-in **Move to recovery**, protected folders/subtrees and allowlists per source. Nothing is
  permanently deleted. Explicit previews name one physical target; virtual folders cannot be deleted.
- Same-source recovery manifests, restart reconciliation and restore without overwriting. Recovery
  storage is excluded from browsing and stays on the source filesystem.
- Configured NFS sources verify their expected export/mount against held descriptors. Missing mounts
  never fall back to the local directory underneath them. Linux sets up the actual NFS connection.

See [NAS sources and collections](docs/nas-setup.md), [recovery and applications](docs/file-actions.md),
[validation evidence](docs/validation.md), and the [roadmap](ROADMAP.md).

## Run

Stable Rust **1.95+**, rustfmt and Clippy are required. Native GUI execution needs a Linux
Wayland/X11 session and OpenGL. Run the GUI on your graphical PC, not an unrelated SSH server;
paths refer to the machine running Archiv. Headless listing/search needs no display.

```sh
cargo run -p archiv --locked
cargo run -p archiv --locked -- --browse "$HOME/Videos" --theme dark
cargo run -p archiv --locked -- --list /absolute/folder
cargo run -p archiv --locked -- --list /absolute/folder --search example
cargo run -p archiv --locked -- --list-collection 1 --library /absolute/library.json
cargo run -p archiv --locked -- --help
```

Ctrl+O opens a folder, Ctrl+L edits its address, Ctrl+F focuses search, F5 refreshes, and Alt+Up navigates to the parent.
Delete requests a recovery preview only when a text field is not being edited. Confirmation is
separate. Shift+Delete provides no purge shortcut. Applications are configured under
**Edit → Preferences → Applications**. Source definitions, collection membership and recovery
permissions are saved under the corresponding Preferences tabs. OK saves before closing;
Save library applies without closing. Appearance preferences apply immediately.

Library definitions use versioned JSON at `$XDG_CONFIG_HOME/archiv/library.json`, falling back to
`~/.config/archiv/library.json`. `--library FILE` selects another file. The [example JSON](examples/archiv.example.json) uses placeholder values and keeps recovery disabled;
replace its paths/export identities with your own or use Preferences. No credentials or inventories
are saved. Open folder adds a temporary source; use Preferences to save one deliberately.

## Limits

Linux openat2 (kernel 5.6+) and procfs are required. Links are listed but never traversed. Bounded
workers keep filesystem/process work outside rendering; a blocked NFS kernel call cannot always
be interrupted. Listings/inline trees retain at most 50,000 entries; recursive search visits at
most 50,000 entries, 10,000 queued directories and 32 relative levels. Limits/errors are visible.
There is no persistent index or automatic background library scan.

Recovery is denied until configured and remains constrained by actual filesystem/NAS permissions.
Archiv safeguards govern Archiv actions, not Dolphin, an external editor, SSH or other NFS clients.
Recovery retains disk usage and is not a backup. See the documented [durability and concurrency
limits](docs/file-actions.md) before enabling it on writable sources. No rename/copy/move-to-another-disk,
transfer, purge or permission editor exists. Actual writable NFS recovery/disconnect testing has
not been performed on the user's NAS; tests use synthetic local fixtures and injected failures.

The interface uses egui fallback fonts; Inter/JetBrains Mono, full desktop font/motion discovery,
complete Wayland/accessibility and mixed-DPI acceptance remain unfinished. Manual UI scaling and
reduced animation are available. Icons are pinned Lucide SVGs; [provenance/licenses](assets/icons/README.md).
No toolkit fork, copied PdfCraft branding, telemetry, network control listener or runtime font download.

## Checks and project documents

```sh
cargo fmt --all -- --check
cargo check --workspace --all-targets --locked -j 2
cargo clippy --workspace --all-targets --locked -j 2 -- -D warnings
cargo test --workspace --locked -j 2
```

[Instructions](AGENTS.md), [development](docs/development.md), [architecture](docs/architecture.md),
[safety](docs/safety.md), [UI design](docs/ui-design.md), [UI acceptance](docs/ui-review.md),
[assets](docs/assets.md), [font notices](docs/third-party-assets.md).

Code/documentation: [0BSD](LICENSE). Dependencies and assets retain their own terms. Tests/captures
use synthetic data. `artifacts/` is ignored; personal paths/configs and NAS inventories stay out of Git.
