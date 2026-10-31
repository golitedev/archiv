# File actions and safeguards

Right-click a physical file or folder for Open, Copy, Copy path, Paste, Rename and Delete.
Edit offers the same actions. Ctrl+C copies the selection into Archiv's session clipboard;
Ctrl+V pastes into the selected physical folder or current physical directory. Right-clicking
empty space offers Paste into the current folder and Refresh. F2 opens Rename; Enter submits
its new name. Copy path sends an exact UTF-8 path to the desktop text clipboard. File copying
uses Archiv's internal clipboard, not cross-application file clipboard formats.

## Permissions

File changes are disabled by default. Add a saved physical source in Preferences → Sources.
Under Safeguards, explicitly allow permanent delete, rename and pasted copies for that source,
then enter allowed source-relative folders, one per line. `.` permits the source subtree.
Source roots and configured collection roots remain protected. There is one current version-1
JSON schema with `writes` policies; no configuration migration or compatibility format.

Protected folders prevent modifying the folder itself or any ancestor containing it; permitted
children can still be changed. Protected subtrees also block changes to descendants. Renaming
checks both old and new names; pasting checks the destination. Protections are checked across
all configured sources by paths and filesystem identity. An unavailable protection fails closed.
These rules govern Archiv only. External applications, SSH and other NAS clients retain their
ordinary permissions. No ownership changes, escalation or permission editor is provided.

Merged folders never expand into hidden operation targets. Browse a physical source first.
Duplicate files retain their physical identity; an action affects only the chosen location.
Links, special files and nested mounts are refused by file actions.

## Delete

Delete permanently removes the selected physical item and, for a directory, its listed children.
It first builds a bounded preview naming every affected physical path. Cancel receives initial
focus. Only the separate Delete permanently action confirms the single-use preview. Navigation
or settings changes expire it. Nothing is moved to hidden storage; there is no undo service.

Immediately before the first deletion the full scope and observed identities are checked again.
Descriptor-relative operations refuse links, source replacements and missing configured NFS
mounts. Files are unlinked and directories removed only when empty. New/unexpected contents are
not recursively swept up. Cancellation/errors can leave a partially deleted tree; completed
deletions cannot be undone. Directory sync errors explicitly report uncertain crash durability.

## Rename and Copy/Paste

Rename changes one filename within its existing physical parent. Slashes, . and .. are refused.
The platform preserves raw filename bytes; a non-UTF-8 old name is never edited through a lossy
path. Rename and copy publication use NOREPLACE: existing files are never overwritten.

Copy stages one physical file/tree in a private `.archiv-copy-*` directory at the destination,
then publishes it under the original name after verifying source snapshots and destination.
Files use mode 0600 and folders 0700; ownership, ACLs, hard-link relationships, sparse layout,
extended attributes and timestamps are not preserved. Sources are never removed. Failed or
cancelled copies may retain partial staging at the reported path; there is no automatic cleanup
of retained data. Successful publication removes only the empty staging directory.

Work runs on the engine's bounded workers. View → Cancel operation requests cancellation for
scans, copy and deletion; a blocked kernel/NFS call may take time to return. No rollback is
promised. Tree operations cap at 10,000 items, 64 source-relative path levels and 1 TiB of observed
file sizes. Copy buffers are bounded to 1 MiB. There is no queued transfer scheduler or Cut.

## Concurrency and NFS limits

Linux has no inode-conditional rename/unlink-by-name API. Descriptor-relative confinement and
snapshot checks narrow races but do not make an operation transactional against another writer
swapping names immediately before a syscall. A concurrent final-name replacement can cause an
unexpected object to be renamed or unlinked; post-rename checks report a detected mismatch, with
no overwrite-prone rollback. Use filesystem/NAS permissions to exclude hostile concurrent writers.
Permanently deleted data has no application-managed fallback.

Tests use synthetic local fixtures. Real writable NFS, server disconnects, server power loss and
hardware durability remain deployment acceptance work. Missing configured mounts fail closed;
Archiv never writes into the local directory underneath a missing NAS mount.

## External applications

Preferences → Applications configures image/media/text/other executable names or absolute paths,
for example imv, mpv or kwrite. Launches pass `--` and the exact filename as separate arguments;
no shell is used. Executable argument strings are not parsed. Programs run with normal user
permissions; Archiv rules do not sandbox them. Files are revalidated before launch. Child tracking
is bounded to eight applications per worker and reaped without blocking rendering.
