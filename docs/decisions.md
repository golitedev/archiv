# Initial decisions

Status: preparation baseline. Updated: 2026-10-31.

## 001 — native Rust Linux desktop

Use Rust with egui/eframe. Linux/Arch first, Wayland and X11. No web frontend or PDF engine.
Rationale: the owner wants a standalone pure Rust app with a consistent native desktop interface.
Toolkit dependencies and exact versions will be selected when UI implementation starts.

## 002 — logical aggregation only

Preserve four independent XFS sources. Combine directory names in a view, with explicit alias
rules and every physical contributor retained. No mergerfs, FUSE, physical pooling or automatic
on-disk renaming. Duplicate files remain distinct until the user explicitly chooses locations.

## 003 — NFS browsing, rsync transfers

Browse/play using normal mounted NFS paths. Prefer rsync over SSH for queued many-file transfers;
local enumeration on the remote peer can avoid repeated client filesystem round trips. Measure,
do not promise a universal speed advantage or hardcode tuning copied from unrelated hardware.
No NFS/SSH configuration is created by the app scaffold. An SSH alias working on this server does
not establish that the same alias works on the owner's Arch PC.

## 004 — engine policy plus explicit enforcement boundaries

Core rules and engine execution are headless. Default future writes off; deletion goes to
same-filesystem recovery after policy/revalidation. These rules do not protect external writers.
A restricted NAS helper remains an optional later decision, not an implied deployment task.

## 005 — one Archiv design system

Keep palette, typography, layout metrics and interaction behavior in a single documented system.
Reusable native widgets belong in the UI layer. Select dependencies for Archiv's needs rather than
importing another application's structure. Accessibility goals need implementation and validation,
not just styling. Project documentation must remain self-contained.

## 006 — minimal preparation

This commit provides documentation, lint configuration, a lockfile and empty layer placeholders.
No feature implementation, third-party dependency resolution, GUI, assets, test framework or CI
service is required yet. Add behavior and necessary tests in small slices after the owner requests it.
