# Archiv

A Rust-native Linux file manager for an Arch PC and a NAS. The first implementation is a
**read-only physical-folder browser** with a native egui/eframe interface.

## Available now

- Open explicitly chosen local folders through an absolute path or `--browse`.
- Navigate real subdirectories and return to a source root; add multiple physical sources.
- Incremental background enumeration, cancel/refresh, folder-first ordering and a virtualized list.
- Filter filenames in the current directory, show hidden files, select items and inspect physical
  paths, byte sizes and observed Unix modes. No recursive size computation or index scan.
- Light/Dark/System appearance and a saved reduced-animation preference. Sources and paths are
  session-only: restart never silently reopens and scans a previous source.
- Headless read-only listing through the same engine, without a display server.

No deletion, rename, move, upload, permission change or application-launch operation exists yet.
No NAS connection, export, permission or file has been changed during implementation.

## Run

Rust 1.95 or newer is required by the selected egui/eframe 0.37 release. Use stable Rust with
rustfmt and Clippy. GUI execution needs a native Linux Wayland/X11 session and OpenGL support.

```sh
cargo run -p archiv --locked
cargo run -p archiv --locked -- --browse "$HOME/Videos" --theme dark
cargo run -p archiv --locked -- --list /absolute/local/folder
cargo run -p archiv --locked -- --help
```

On Arch, install the standard native runtime libraries for your Wayland/X11/OpenGL session. This
project uses dynamic loading for the windowing/graphics stack; it does not install system packages.
The GUI must run on the machine with the graphical session (normally your Arch PC), or through
properly configured display forwarding. `--list` works over SSH without a GUI. All paths refer to
the machine **running Archiv**, not automatically to the NAS.

Ctrl+O opens the source-path dialog, Ctrl+F focuses filtering, F5 refreshes, and Alt+Up navigates
up within the source. Double-click a folder or select it and use Open selected folder. Symbolic
links appear in listings but are not traversed. File opening/playback is not implemented yet.

The first directory listing is limited to 50,000 entries, emitted in batches of at most 128 by two
bounded workers. Linux `openat2` (kernel 5.6+) and mounted procfs are required. Failures are reported;
there is no permissive path-resolution fallback. These reads are not a filesystem sandbox or a
substitute for NAS permissions. NFS mount identity/availability checking is still a later milestone.

## Appearance and current limits

The interface implements Archiv's neutral surfaces, blue accents, spacing, control geometry and
shared theme treatment. It currently uses egui's default fonts and text action labels. Inter,
JetBrains Mono, Lucide icons, full desktop text-scaling/reduced-motion discovery, broader font
coverage and complete keyboard/accessibility acceptance remain M1 work. See
[font resources](docs/third-party-assets.md) and [validation evidence](docs/validation.md).

Merged Movies/TV/Anime categories are visibly disabled placeholders. Recursive/merged search,
SQLite indexing, configuration parsing, NFS mount management, mpv, rsync and configurable recovery
are not implemented. The [example config](examples/archiv.example.toml) remains a design sketch.

## Project documents

- [Instructions](AGENTS.md), [development](docs/development.md), [roadmap](ROADMAP.md).
- [Architecture](docs/architecture.md), [decisions](docs/decisions.md), [safety](docs/safety.md).
- [UI design](docs/ui-design.md), [native UI checklist](docs/ui-review.md), [asset guidelines](docs/assets.md).

## Checks

```sh
cargo fmt --all -- --check
cargo check --workspace --all-targets --locked -j 2
cargo clippy --workspace --all-targets --locked -j 2 -- -D warnings
cargo test --workspace --locked -j 2
```

Tests use temporary synthetic directories; they never operate on your real library. No private
keys, personal inventories or machine-specific source paths are committed. `artifacts/` is ignored.

## License

Archiv code and documentation use [0BSD](LICENSE). Dependencies and their resources retain their
own terms; the default-font records are in [third-party assets](docs/third-party-assets.md).
