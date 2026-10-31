# Architecture

Status: native browsing/collections, external applications and opt-in recovery implemented; remaining sections describe future work. Updated: 2026-10-31.

## Goal and boundaries

Archiv is a Rust-native Linux file manager. Physical sources may occupy independent local or
NAS filesystems. Local and mounted NFS paths are storage sources; the app builds logical views over them.
It does not pool disks, repartition, relocate libraries or implement a filesystem.

NFS is the planned browsing/playback route. External rsync over SSH is the planned transfer route
for many small files. A helper on the NAS is optional future work, not required to launch the app.
Do not couple the domain model to `/mnt/diskN`, a username, one SSH alias or a specific NAS.

## Workspace boundaries

| Crate | Owns when implemented | Allowed workspace dependencies |
| --- | --- | --- |
| `archiv-core` | source/entry/collection IDs, physical paths, aliases, operation intents, policy data, deterministic merge and policy rules | none |
| `archiv-platform` | filesystem enumeration/identity, safe mutation primitives, process execution, SSH/rsync/mpv adapters, persistence adapters | core |
| `archiv-engine` | use cases, planning/revalidation, job state, worker scheduling, index orchestration, recovery journal | core, platform |
| `archiv-ui-egui` | egui views, tokens/widgets, command dispatch, input/accessibility, presentation state | core, engine |
| `archiv` (`apps/archiv`) | native entry point, configuration/runtime startup and dependency wiring | engine, platform, ui-egui |

Core models, the Linux directory reader, the bounded browsing engine and the native shell now
exist. The executable supports a GUI and `--list` through the same engine. The UI uses upstream
egui/eframe 0.37 and Glow; rustix supplies safe descriptor-relative Linux APIs. Optional merged collections and guarded mounted NFS sources now exist. Opt-in recovery/restore and configurable external application launches are implemented.
No persistent index, transfer or SSH transport adapter is present. Enforce dependency direction
in reviews; no automated layer checker exists yet.

## Identity and visual merging

- A configured source has an opaque stable ID, a label, a local root and optional transport metadata.
  A mount root and an SSH remote path are distinct values; do not derive one from the other blindly.
- A physical entry retains source ID, source-relative path, file kind and observed filesystem identity.
  Identity observed during a scan is a snapshot, not a permanent authorization to modify that path.
- A collection references physical roots from multiple sources. Same-named directories can be
  grouped recursively. Default matching is exact and case-sensitive on Linux. Aliases are explicit;
  no automatic case folding, Unicode normalization or guessed movie-title equivalence.
- Virtual nodes keep **all** contributors. Colliding files remain separate physical results with
  location badges; same name is not evidence of byte equality. No implicit deduplication.
- A selection of a virtual collection is a browsing selection, never a deletion target. Expanding a
  merged title into multiple physical targets requires an explicit location choice and preview.
- Exact paths use `PathBuf`/`OsString`; display strings are separate. Stable UI IDs do not depend on
  sorting position, lossy UTF-8 labels or mutable names alone.

## State, IO and performance

UI frame -> typed engine intent -> bounded worker job -> platform operation -> bounded engine event
-> UI state. A later index adapter stores searchable metadata in SQLite; neither SQLite nor a
transport client is exposed to widgets. The UI shows cached results immediately and marks staleness.

Enumerate incrementally, cap concurrency per source, batch events and virtualize large lists.
Cancellation/generation IDs prevent stale scan results replacing newer navigation. Keep blocked NFS
calls in bounded workers so an unavailable NAS cannot block drawing or cause unbounded thread growth.
Cancellation of an intent does not guarantee a blocked kernel call stopped. Index invalidation needs
reconciliation scans; remote filesystem notifications are not assumed complete or immediate.

Do not parallel-scan all disks or repeatedly `stat` the whole library every frame. Scan/index the
metadata actually needed; expensive sizes/thumbnails are deferred work. A NAS-local indexing helper
may later reduce network round trips; document and measure its cost before adding it.

## Operations and policy

All entry points call the same engine planner and executor. The planner resolves explicit physical
targets, current capabilities and policy into a dry-run preview. Confirmation refers to that plan,
not a display label. Execution revalidates configuration revision, targets, containment, mount
identity and metadata. There is no public "delete this arbitrary path" widget convenience method.

Recovery, transfers and permission changes are separate intents/capabilities. Read-only source
configuration is independent of actual NFS permissions; effective capability is their intersection.
Record actual enforcement scope in the UI. Details: [safety.md](safety.md).

## Persistence and configuration

Versioned JSON library settings are implemented; transfer configuration and a local SQLite metadata
index are not. The supported example JSON contains placeholder sources and disabled recovery. Library version 1 rejects unknown
fields and invalid references. Settings are saved by bounded engine workers using a private
same-directory temporary file, file sync, atomic rename and parent-directory sync. A directory
sync failure reports uncertain durability, even if the replacement is visible. Sources restore
without a scan. Malformed files cause an actionable startup error and are never overwritten. Unknown destructive
settings and malformed policies must not fall back to permissive defaults. Settings, jobs and
journals need atomic updates, crash reconciliation and bounded growth; do not store credentials
in them. User-specific state follows XDG directories and stays out of Git.

## Desktop integration

The native shell uses upstream egui/eframe with Wayland, X11, clipboard and AccessKit. Keep the UI crate independent
of a running network or GPU in domain tests. Launch external mpv and rsync with `Command` argument
arrays, never shell command strings. rsync SSH routing and destinations must be validated separately
from NFS mounts. Transfers over SSH bypass any Samba recycle policy.

## Deliberately deferred

Aliases, transfer-policy parsing, indexing, cross-disk mutation/transfer, SSH authentication,
packaging, CI services and a NAS agent. The design is
small enough to revise as the first read-only slice supplies evidence; do not add a plugin framework
or separate service for every crate.

## Current recovery and application adapters

The engine admits single-use physical previews, recovery, inspection and restore through its same
command path. Policy is deny-by-default and shared with UI availability explanations. Platform IO
uses anchored descriptors, no-follow/no-cross-mount resolution, prepared manifests and atomic
no-replace rename. See file-actions.md for durability, final filename-race limits and test evidence.
Application launches are structured worker operations with bounded child tracking/reaping. Rendering
never scans storage, waits for a process, installs NFS packages or edits mount configuration.
Inline branches share the fixed worker pool and view generation; recursive search is explicit and
bounded, not a scan hidden behind text editing.
