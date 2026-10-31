# Applications, safeguards and recovery

## Opening files

Preferences → Applications selects executable names or absolute paths for images, media, text
and other files. Empty means no handler. Values are executable paths, not shell commands; argument
strings such as `mpv --fullscreen` are not parsed. Double-click/Enter or Open with application
launches the configured program with `--` and the exact absolute physical filename as separate
arguments. Paths retain their original bytes. Links and special files are refused. Descriptor
resolution and the listed inode/change time are revalidated before launching.

The external program runs as the normal user with its ordinary filesystem permissions. It receives
a pathname, not a confinement promise: files may change after launch, editors may write them, and
single-instance applications may delegate opening to another process. Archiv's safeguard settings
do not constrain those applications. No selected script/executable is executed as the handler by
inference. Launches are capped at eight tracked children per worker (sixteen total), reaped without
blocking rendering; unsuccessful exits are reported. No application output/credentials are captured.

## Configuring recovery permissions

Recovery is disabled by default. Under Preferences → Safeguards, choose a source, explicitly
allow recovery moves, and enter allowed source-relative folders, one per line. `.` allows the
source subtree, while the source root itself remains protected.

Two protection lists are available:

- **Protected folders:** the folder itself and any ancestor containing it cannot be removed.
  Descendants can be recovered if the allowlist and other protections permit them.
- **Protected subtrees:** the folder, ancestors and every descendant are protected.

For example, a structural protection on `data/media` prevents recovering `data` or `data/media`
but can permit an intentional recovery of `data/media/Example Title`. Paths are exact and
case-sensitive. Source roots, configured collection roots and `.archiv-recovery` are always
protected. Protections from all configured sources are rechecked by paths and observed filesystem
identities so overlapping source definitions cannot bypass them. An unavailable configured guard
causes recovery to fail closed, even if the selected source is online. Depth is limited to 64.

These are Archiv rules, not a filesystem permission system. Writable NFS/SSH, Dolphin and external
applications have separate authority. Unix ownership/mode displays never grant Archiv permission.
NFS root squashing and existing server ownership/ACLs are not changed by Archiv.

## Move to recovery and restore

Select one explicit physical file/folder, then use Edit → Move to recovery, the row context menu,
Details, or Delete. Text input consumes its editing keys. A virtual collection/folder never expands
into hidden destructive targets: browse the physical source first. Duplicate files are distinct
physical selections. The preview names the source, exact path and recovery destination. Cancel is
the initial keyboard choice; Move to recovery requires its own activation. The engine consumes the
preview once. Navigation/settings changes invalidate pending previews; settings changes cannot race
an admitted operation.

Recovery uses `.archiv-recovery` inside the physical source root, on the same filesystem. It must
be private (0700); Archiv creates new storage privately but never chmods an existing folder to make
an operation succeed. The source/root, parent, target identity, protections and expected NFS mount
are checked again. Resolution is descriptor-relative, no-follow and forbids nested mounts. There
is no cross-device copy fallback, recursive unlink, silent overwrite, permanent purge or permission
escalation. The atomic rename uses NOREPLACE.

Each unique private recovery job first receives an atomic, file-synced manifest with the exact
original filename bytes, source root inode, filesystem identity and target identity. Its directory
and recovery parent are synced before rename. Source/destination directories are synced after the
rename, followed by a durable completed-manifest replacement. File contents are not copied or
rewritten. Active writers/hard links can still change retained content; recovery is not a backup.
Retained items occupy space. No automatic retention deletion is implemented.

Open Recovery, choose a physical source, and explicitly inspect its records. At most 1000 jobs and
64 KiB per manifest are read. Prepared records with matching payloads reconcile interrupted moves;
empty pre-manifest jobs contain no data and are ignored. Interrupted restores reconcile against the
original inode when the payload is absent. Malformed manifests, changed filesystems, unknown payload
identities or unsafe storage produce errors, never cleanup. Restore revalidates current policy,
root/filesystem and payload, then renames back to the exact original path with NOREPLACE. Missing
parents, symlinks, replaced mounts and existing destinations are refused. Successful changes refresh
the current listing. There is no purge button or automatic removal of old journal directories.

## Durability, concurrency and current acceptance

Rename plus manifest update is not one filesystem transaction. A post-rename sync/manifest error
reports that the payload was retained in a named recovery job; do not blindly retry the original
operation. The prepared manifest allows explicit reconciliation after restart. Restore sync errors
can mean the original path is already restored but crash durability is uncertain. Device numbers
are snapshot checks; persisted records additionally check the filesystem ID and root inode across
restarts/remounts. A filesystem without a nonzero exposed recovery identity is refused.

Descriptor resolution rejects path/symlink escapes, and snapshot checks reject observed target or
parent replacements. Linux does not provide an inode-conditional rename by filename. A hostile
concurrent rename in the final syscall window can therefore be detected after quarantine; Archiv
retains the moved object, reports the conflict and does not attempt an automatic rollback that
might overwrite another file. These protections do not defend against a NAS administrator or
another actor with equivalent access changing recovery records. Do not treat app policy as global
confinement against arbitrary concurrent clients.

Synthetic tests cover denied structure/ancestors, subtree protections, virtual targets, replayed
confirmation, stale targets, parent symlink swaps, missing NFS mounts, private-store validation,
permission failure, colliding restores, injected persistence failures before/after rename, prepared
record reconciliation, non-UTF-8 paths and literal process arguments. No real NAS mutation, live
NFS unmount/disconnect, server crash or power-loss experiment has been performed. Server/filesystem
fsync and NFS rename support remain deployment-specific acceptance work.
