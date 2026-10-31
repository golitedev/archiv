# Architecture

Status: design baseline, not an implemented system. Updated: 2026-10-31.

## Goal and boundaries

Archiv is a Rust-native Linux file manager on an Arch PC. The NAS currently has four independent
XFS disks. Local and mounted NFS paths are storage sources; the app builds logical views over them.
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

The current libraries contain documentation only. The executable only prints a scaffold notice.
The manifests reserve these dependencies; there are no third-party dependencies yet. Enforce the
layer direction during reviews now; add automated architecture checks when enough code exists.
Public engine actions should be usable by tests and future CLI adapters without egui.

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

Plan for versioned TOML configuration and a local SQLite metadata index; schema/migration code is
not present. The conceptual example is not a supported config format yet. Unknown destructive
settings and malformed policies must not fall back to permissive defaults. Settings, jobs and
journals need atomic updates, crash reconciliation and bounded growth; do not store credentials
in them. User-specific state follows XDG directories and stays out of Git.

## Desktop integration

Plan for upstream egui/eframe with Wayland, X11, clipboard and AccessKit. Keep the UI crate independent
of a running network or GPU in domain tests. Launch external mpv and rsync with `Command` argument
arrays, never shell command strings. rsync SSH routing and destinations must be validated separately
from NFS mounts. Transfers over SSH bypass any Samba recycle policy.

## Deliberately deferred

GUI code, toolkit version selection, shared widget implementation, config parsing, indexing implementation,
mutation syscalls, SSH authentication code, packaging, CI services and a NAS agent. The design is
small enough to revise as the first read-only slice supplies evidence; do not add a plugin framework
or separate service for every crate.
