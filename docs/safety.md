# File-action safety

File changes require explicit source configuration. The core `WritePolicy` denies roots,
protected folders/ancestors/subtrees, unsupported file kinds and targets outside the allowlist.
The engine shares these decisions with menus and shortcuts. UI button states never grant rights.

Platform operations use held Linux descriptors, openat2 with no-follow/beneath/no-cross-mount
resolution, observed inode/change-time validation and configured NFS export checks. Source and
collection root guards apply across overlapping source definitions. No symlink traversal, silent
overwrite, destination fallback, shell interpolation, recursive std::fs::remove_dir_all, sudo or
permission escalation is used. Source policies and file operations stay outside rendering.

Delete is permanent. A single-use confirmation names its complete bounded physical scope.
Execution re-enumerates that scope before unlinking; changed scope fails before the first deletion.
Directories are removed only when empty. Partial failure/cancellation is not rollback. Copy uses
bounded staging and NOREPLACE publication; failures retain partial data rather than silently
cleaning up an uncertain destination. Rename also refuses existing destinations.

Read [file actions](file-actions.md) for exact permissions, limits, staging behavior, the final
filename concurrency window and durability limits. App rules do not constrain external programs
or other NAS clients. Do not claim protection against arbitrary concurrent writers or backups.

Regression fixtures cover policy denials, protected ancestors, stale snapshots, source/parent
changes, links, missing mounts, collisions, cancellation, literal/non-UTF-8 paths and replayed
confirmation. Never run development deletion experiments on the real NAS library. Live writable
NFS/disconnect and power-loss acceptance remains incomplete. Read-only SFTP browsing is documented in [sftp.md](sftp.md). Transfers over SSH and permission
editing are future work and must use this same engine/policy boundary.
