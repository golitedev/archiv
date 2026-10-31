# Roadmap

Status: M1 in progress; first native read-only browsing slice implemented.
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
- Remaining: target fonts/icons, desktop scale/motion discovery, broader keyboard/native Wayland
  acceptance and mount identity checks before claiming NAS support.
- Verify font/icon redistribution rights and evaluate shared widget needs before adding dependencies.
- Acceptance: synthetic directory fixtures; no mutation route; long/non-UTF-8 labels; visible errors;
  native screenshot/interaction checks. Cold startup performs no unrequested NAS scan.

## M2 — merged views and mounted NAS sources (not started)

- Exact-name directory merging, explicit aliases, duplicate-location display and source filters.
- NFS mount identity/availability checks, stale/offline states, incremental cached search.
- mpv launch for an explicit physical file. No network traversal on the UI thread.
- Acceptance: collisions across four synthetic sources stay distinct; missing mounts fail closed;
  cancelled scans cannot overwrite newer views. Do not benchmark using real library mutations.

## M3 — transfer orchestration (not started)

- rsync over SSH for many-file PC/NAS transfers; structured argument execution and progress.
- Durable job states, partial-file strategy, destination/conflict preview and retry behavior.
- Copy only initially. No source removal, implicit mirroring, `--delete` or secret storage.
- Acceptance: interruption/disconnect/conflict tests with synthetic fixtures; no source data lost.

## M4 — configurable recovery and permissions (not started)

- Versioned config parser, conservative policy evaluation and dry-run physical operation plans.
- Protected paths, virtual-view restrictions, explicit limits, permissions/capability checks.
- Same-source-filesystem recovery, journal, restore and restart reconciliation.
- Revalidate descriptor-relative targets and mount identity at execution; fail closed on races.
- Copy-verify-recover moves only after recovery works. Purge remains a distinct future capability.
- Acceptance: path escape, symlink swaps, stale plan, missing mount, full disk, collisions and crash
  tests; deny by default; engine decisions identical with and without the GUI.
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
