# Mounted NAS sources and optional collections

Archiv browses already-mounted NFS shares. It does not SSH into a NAS, install packages, change
exports/permissions or request root privileges. SSH aliases and local mount paths are distinct:
`ssh nas` working does not mean that `nas` resolves as an NFS server name on the PC.

## Debian server and Arch PC

Configure the connection using normal system administration, outside Archiv:

1. On Debian, install/configure the NFS server (`nfs-kernel-server`). Export only the folders needed,
   initially read-only, restricting clients to the PC's LAN address. Retain root squashing and
   existing ownership/permissions. Review existing exports rather than replacing them wholesale.
2. On Arch, install the NFS client (`nfs-utils`). Create separate mount points for the exported
   disks and mount them read-only using NFSv4, with a server IP/hostname reachable from that PC.
   Configure persistent mounts/automounts in Linux if desired; Archiv does not edit fstab.
3. Confirm the expected mount point, filesystem type and server/export with
   `findmnt -t nfs,nfs4 -o TARGET,SOURCE,FSTYPE` **on the PC running Archiv**.
4. Open Archiv Settings. Add a source for each disk. Set its label and exact local mount point;
   keep **Require expected NFS mount** enabled and enter the SOURCE value from findmnt, such as
   `192.0.2.20:/exports/disk-a`. Do not enter an SSH path as the local folder.

Standard AUTH_SYS NFS trusts client-supplied numeric user/group identities; it is suitable only
within the chosen trust boundary and still needs correct server permissions. Use authenticated
NFS (for example Kerberos) when stronger identity assurance is required. Archiv's mount check
is a local identity/availability check, not server authentication or a filesystem sandbox.

An unmounted directory, different export, bind subtree or changed held mount ID produces an error.
NFS navigation refuses nested mounts and symbolic links. There is no fallback to the directory
under the missing mount. Mount checks happen before and after each read, not continuously while
idle; displayed metadata is an observed snapshot. Use Refresh to revalidate. Kernel NFS calls
can block; fixed workers keep drawing responsive, but cancellation cannot interrupt every syscall.

## Optional visual merging

Sources remain individually browsable. To combine four movie folders:

1. Add and save the four disk sources in Settings.
2. Add a collection named Movies.
3. Explicitly include each disk and specify its relative movie-folder path, for example
   `data/media/movies` when the source mounts the disk root. Paths may differ per source.
4. Save library, close Settings, and choose Movies in the sidebar.

An empty relative path means the source root. Paths with `..`, absolute paths and unknown source
references are rejected. Nothing is included by default. TV, Anime or other collections can be
created independently. Removing a source from settings also removes its collection references;
empty collections must be removed or given another member before saving.

Folders combine only when their exact filename bytes match, case-sensitively, at each browsed
level. Every contributing directory stays attached to its virtual row. Files with identical
names are separate rows with source labels and physical paths in Details; no deduplication,
renaming, movement or on-disk pooling occurs. Up/Alt+Up returns within the collection. Missing
contributors produce an incomplete-results error with physical context; Refresh retries them.
No recursive scan runs merely because a collection exists.

## Settings and headless use

Saved definitions live in `$XDG_CONFIG_HOME/archiv/library.json`, falling back to
`~/.config/archiv/library.json`. `--library /absolute/file.json` selects another file. Version 1
supports at most 32 physical sources and 16 collections, each with 1–32 explicit member folders.
The JSON file is limited to 256 KiB, rejects unknown fields, and contains no credentials or inventory.
Use the UI to edit it. Source and collection IDs persist across restarts and ordinary edits.

Settings saves run in bounded workers. The adapter writes a private (0600) temporary file beside
the destination, syncs its contents, atomically replaces the file and syncs the parent directory.
Save failures remain visible. If parent-directory sync fails after rename, the new file may be
visible with uncertain crash durability; restart/reload before relying on it. There is no migration
from the conceptual TOML policy example. Invalid existing JSON stops startup with a path and error,
without replacing it with defaults. A saved library never starts a scan until explicitly opened.

`archiv --list-collection ID --library /absolute/file.json` uses the same engine without a GUI.
Output lists physical contributors with their source labels/paths; incomplete listings return an
error. The total visible collection is bounded at 50,000 physical entries before folder grouping,
with a visible truncation warning. There is no index/search cache, playback, transfer or mutation.
