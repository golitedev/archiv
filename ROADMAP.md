# Roadmap

Status: native browsing/collections and an opt-in recovery slice implemented; deployment acceptance remains.
Updated: 2026-10-31.

## M0 — preparation (complete)

- Git repository and minimal compiling Cargo workspace.
- Archiv UI design specification and asset guidelines.
- Architecture, safety boundaries, conceptual config and staged implementation plan.
- Preparation established the layer boundaries; runtime implementation is tracked under M1.

## M1 — native shell and read-only local browsing (in progress)

- Implemented: egui/eframe 0.37 shell with theme tokens, physical sources, filtering and details.
- Implemented: byte-preserving IDs, no-follow directory reads, bounded workers, cancel/refresh,
  stale-result rejection and headless `--list`; no implicit startup scan.
- Native X11 appearance and egui interaction evidence are recorded in docs/validation.md.
- Implemented: verified Inter/JetBrains Mono build inputs, explicit acquisition/check tooling and
  documented offline fallback; no font binaries in Git or runtime downloads.
- Remaining: desktop text-scale/motion discovery, broader keyboard/native Wayland
  acceptance. Configured NFS mount checks are implemented in the first M2 slice.
- Verify font/icon redistribution rights and evaluate shared widget needs before adding dependencies.
- Acceptance: synthetic directory fixtures; opt-in recovery is tracked under M4; long/non-UTF-8 labels; visible errors;
  native screenshot/interaction checks. Cold startup performs no unrequested NAS scan.

## M2 — merged views and mounted NAS sources (in progress)

- Implemented: optional exact-name recursive directory merging, duplicate-location display,
  source browsing, native library settings and atomic versioned persistence.
- Implemented: configured NFS mount/export/descriptor checks and explicit partial-source errors.
- Implemented: lazy nested rows, explicit bounded recursive search and configurable external file applications.
- Remaining: aliases, source filters within collections and persistent indexed search.
- NFS mounts/exports remain operating-system administration; no automatic NAS connection setup.
- Configurable applications include mpv; no filesystem/process work runs in the UI frame.
- Acceptance: collisions across four synthetic sources stay distinct; missing mounts fail closed;
  cancelled scans cannot overwrite newer views. Do not benchmark using real library mutations.

## M3 — transfer orchestration (not started)

- rsync over SSH for many-file PC/NAS transfers; structured argument execution and progress.
- Durable job states, partial-file strategy, destination/conflict preview and retry behavior.
- Copy only initially. No source removal, implicit mirroring, `--delete` or secret storage.
- Acceptance: interruption/disconnect/conflict tests with synthetic fixtures; no source data lost.

## M4 — configurable recovery and permissions (in progress)

- Implemented: versioned JSON configuration, conservative recovery allowlists/protections and single-use previews.
- Implemented: protected structure/subtrees, virtual-target denial, source/collection root protection,
  filesystem identity and no-follow descriptor checks.
- Implemented: same-source recovery, durable prepared/completed manifests, explicit reconciliation
  and restore without overwriting. No purge, recursive unlink or transfer operation.
- Revalidate targets/mounts at execution. Post-rename conflicts retain payloads and report uncertainty;
  see docs/file-actions.md for the final filename-race window and durability limits.
- Copy-verify-recover moves only after recovery works. Purge remains a distinct future capability.
- Synthetic acceptance covers policy, path/symlink swaps, stale previews, missing mounts, restore
  collisions, permission failures and injected persistence failures. Native confirmation is tested.
- Remaining: real writable-NFS/disconnect/server-durability acceptance and broader adversarial fault
  testing. No user-library mutation tests. Permission editing, transfers and purge remain absent.
- Separately decide whether a restricted NAS helper is warranted. App policy is not global protection.

## Later — only after the foundations

Permissions editing, optional NAS-local indexing helper, authenticated automation, packaging,
large-library performance measurements, recovery retention UI, translations and additional backends.
No implied commitment to a FUSE filesystem, multi-platform ports or automatic NFS configuration.

## Progress log

- 2026-10-31: prepared docs, rules and standard-library-only crate placeholders. No feature code.
- 2026-10-31: made project instructions self-contained and removed external design-reference material.
- 2026-10-31: completed the design-guidance review; clarified theme/scale/font behavior, exactly-once
  command dispatch, resource limits and native acceptance criteria. Documentation only.

- 2026-10-31: implemented the first read-only native browser slice, headless listing, bounded
  worker engine and synthetic domain/platform/UI tests. No NAS access or mutation enabled.

- 2026-10-31: added saved mounted/local sources, optional merged collections, recursive browsing,
  per-location errors and NFS mount checks. Native settings and synthetic four-disk evidence added.
  Read-only NAS layout inspection only; no server or PC exports/mounts were configured.

- 2026-10-31: replaced toolbar-heavy chrome with text menus, top-right search, home panels and
  categorized Preferences; added attributed tinted Lucide icons, compact inline expansion, explicit
  recursive search, configurable applications and opt-in journaled recovery with protected paths.
  NAS inspection remained read-only; client-specific NFS setup needs PC connection information.

- 2026-10-31: re-audited the requested features against PdfCraft; corrected typography, exact
  selection tokens, resolved-System theme cards, field/focus styling and appearance cancellation.
  Client-specific NFS deployment remains pending; see docs/request-audit.md.

- 2026-10-31: simplified chrome to one menu/search row, moved location entry to the bottom,
  removed sidebar action duplicates and the toolbar theme selector, added header-only column
  resizing and constrained Details content to the pane width. Synthetic interaction and native
  X11 checks cover this slice; broader desktop acceptance remains open.
