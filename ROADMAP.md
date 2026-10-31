# Roadmap

Updated: 2026-10-31. This is a new application with one current configuration format.

## Implemented

- Rust/egui native Linux browsing, optional mounted-source collections, compact inline trees,
  explicit bounded recursive search, source identity and missing-NFS-mount refusal.
- One menu/search row, bottom editable location, configurable side panes, header-only column
  resizing, shared theme tokens, Inter/JetBrains Mono build inputs and licensed Lucide icons.
- Configurable external applications with structured argument launches.
- Physical file/folder context menus: Open, Copy, Copy path, Paste, Rename and permanent Delete.
  Edit and shortcuts use the same engine commands. Copy uses a session clipboard.
- Explicit write permissions, protected folders/subtrees/source/collection roots, bounded
  physical deletion previews, no-overwrite rename/copy and cooperative cancellation.
- Atomic library settings and synthetic domain/platform/engine/native-egui interaction tests.

## Remaining acceptance

- Actual Debian NFS exports and graphical-PC mounts, writable NFS operations/disconnects and
  server durability. No real NAS mutation tests have been performed.
- Wayland/hardware compositor, mixed-DPI, IME/screen reader coverage and desktop text/motion discovery.
- Larger-library performance measurements and broader adversarial concurrent-writer fault tests.

## Future features

Multi-selection, Cut/cross-disk moves, queued/progress-reporting transfers, system file-clipboard
interoperability, copy metadata preservation, aliases, source filters, indexed search, packaging,
translations and permissions editing. No automatic NAS configuration or filesystem pooling.
