# Initial decisions

Status: initial decisions with the M1 implementation update. Updated: 2026-10-31.

## 001 — native Rust Linux desktop

Use Rust with egui/eframe. Linux/Arch first, Wayland and X11. No web frontend or PDF engine.
Rationale: the owner wants a standalone pure Rust app with a consistent native desktop interface.
M1 selects upstream egui/eframe 0.37 (Rust 1.95 minimum), using Glow/OpenGL, Wayland and X11.

## 002 — logical aggregation only

Preserve four independent XFS sources. Combine directory names in a view, with explicit alias
rules and every physical contributor retained. No mergerfs, FUSE, physical pooling or automatic
on-disk renaming. Duplicate files remain distinct until the user explicitly chooses locations.

## 003 — NFS browsing, rsync transfers

Browse/play using normal mounted NFS paths. Prefer rsync over SSH for queued many-file transfers;
local enumeration on the remote peer can avoid repeated client filesystem round trips. Measure,
do not promise a universal speed advantage or hardcode tuning copied from unrelated hardware.
No NFS/SSH configuration is created by Archiv. An SSH alias working on this server does
not establish that the same alias works on the owner's Arch PC.

## 004 — engine policy plus explicit enforcement boundaries

Core rules and engine execution are headless. Default future writes off; deletion goes to
permanent deletion only after policy/revalidation and explicit physical confirmation. These rules do not protect external writers.
A restricted NAS helper remains an optional later decision, not an implied deployment task.

## 005 — one Archiv design system

Keep palette, typography, layout metrics and interaction behavior in a single documented system.
Reusable native widgets belong in the UI layer. Select dependencies for Archiv's needs rather than
importing another application's structure. Accessibility goals need implementation and validation,
not just styling. Project documentation must remain self-contained.

## 006 — minimal preparation

The initial preparation provided documentation, lint configuration, a lockfile and layer placeholders.
Implementation proceeds in small, tested slices. The first slice adds read-only browsing and the
native shell; transfers, merging and mutation are not part of it. No CI service is configured.
