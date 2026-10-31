# Deletion, recovery and permission design

Status: requirements only; no safety mechanism is implemented. Updated: 2026-10-31.

## Enforcement scope

Archiv rules protect requests executed by Archiv. They do not intercept `rm`, Dolphin, writable
NFS access, containers, SSH transfers or NAS applications operating outside Archiv. Root ownership
of `tv` does not protect writable show contents from recursive deletion. A read-only source in
Archiv is not a read-only NFS export. Display these distinctions honestly.

Global enforcement would need read-only exports/filesystem permissions plus a restricted NAS
helper, a protocol-specific mechanism, or a different storage layer. Snapshots/backups are separate
recovery systems. This scaffold configures none of them. Never promise that an app-only rule protects
against arbitrary NAS commands or a compromised unrestricted account.

## Planned policy model

Start deny-by-default for mutation until valid explicit configuration exists. Policy is evaluated
in the core/engine, never just in a button. A future rule set may specify:

- Source capability: browse/copy/play, upload, recover, restore, permission editing; purge off.
- Protected structural paths (`data`, `media`, category roots) and forbidden virtual-node targets.
- Explicit allowed physical roots, per-operation selection count and total byte limits.
- Unknown size/type, unavailable source or ambiguous merged target: deny pending resolution.
- Confirmation requirements including physical paths, source badges, sizes and the recovery action.
- Recovery destination per source filesystem; no implicit cross-filesystem copy fallback.

Deny wins over allow, including ancestor/overlap rules. Specify exact subtree semantics in the
schema and tests before implementation. Reject contradictory/unknown rules rather than silently
weakening them. A protected parent is a structural target restriction, not a blanket deny of
intentional operations on permitted descendants. Conversely, deleting a parent cannot evade
restrictions on any selected descendant. Virtual collections are not physical operation targets.

## Planned recovery protocol

Delete means "move to recovery"; permanent purge is a separate capability and is not in the first
mutation milestone. For an explicit physical item, reserve a unique recovery identity on the same
filesystem, durably record the source/destination and operation state, perform a descriptor-relative
rename without replacing an existing item, then commit the journal state. Account for crashes
between each step and reconcile from actual filesystem state. Never claim rename+journal is a
single atomic transaction. Verify server/filesystem durability semantics, especially over NFS.

Recovery lives outside library discovery and is not accessible to ordinary app deletion targets.
Store the original source ID/path, operation time, identities and result. A default retention
expiry must not silently purge data. Refuse a missing or unsafe recovery location. Retained data
continues consuming capacity; hard-linked or open files may change through other writable aliases,
so a moved inode is not an immutable backup or a snapshot.

Restore checks destination conflicts; never overwrite a newly created title. Restore/purge requires
explicit item identity and permissions, with clear previews. Purge cannot follow links out of its
recovery store. Persistent journals contain sensitive paths; keep them user-private and out of Git.

## Path confinement and mount failure

Lexical `..` rejection and canonicalization are useful validation, but not enough against races.
Use safe descriptor-relative APIs and no-follow/constrained resolution at execution, checking both
source and destination parents. Validate component boundaries, aliases, symlinks, bind mounts,
replaced parents, hardlinks and nested mount points. A preview can become stale immediately.

NFS source availability must be checked against expected mount identity and configured source
location. A folder that remains after unmount is not the NAS. Never create recovery or transfer
output there as a fallback. Cross-disk moves are copy/verify/recover jobs, not same-filesystem renames.
Mount and remote-server identity do not remove the need for per-entry revalidation.

## Transfers and external processes

Copy first, then verify against a quiescent/revalidated source before any source recovery action.
Define verification and durability precisely before implementing Move. Cancellation or failed
verification leaves the source intact. Partial destination data must have a documented retry and
cleanup policy; never remove preexisting destination data as generic cleanup.

Invoke external rsync/mpv/SSH using typed arguments and proper remote-argument protection, not
interpolated shell strings. Filenames with whitespace, newlines, leading `-`, shell syntax and
non-UTF-8 bytes are test cases. Do not assume local `Command::arg` alone prevents remote-shell
interpretation. Do not add rsync `--delete`, `--remove-source-files` or `--inplace` implicitly.
SSH credentials may allow writes even when NFS is read-only; report this separate capability.

## Permission editing

Show observed UID/GID/mode and effective operation results separately. UID 1000 on one system does
not automatically prove authority on another. Do not recursively chown/chmod for convenience,
request root at startup or relax permissions to make a failed action pass. Future permission editing
needs its own plan, limits, validation and authorization; recovery cannot generally undo permissions
changed by another program. Effective filesystem rights are always the final authority.

## Required mutation evidence

Before enabling writes: synthetic tests of denied structural roots, merged-target ambiguity,
ancestor/child selections, path escapes, symlink swaps, stale identities, missing mounts,
non-UTF-8 paths, full disk, read-only exports, name collisions, disconnected NFS, partial transfer,
journal interruption and restore conflicts. Use injected failures and temporary filesystems;
never use the real NAS library as a test fixture. No mutation API exists at the scaffold stage.
