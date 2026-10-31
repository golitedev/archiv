# Validation evidence

Updated: 2026-10-31. Fixtures and native captures use synthetic local trees, never the real NAS.

## Automated checks

Workspace checks use locked dependencies and parallelism of two: fmt, all-target check, Clippy
with warnings denied, 45 passing tests and a native debug build. The tests cover byte-preserving identities,
merge collisions, settings validation, unavailable sources, worker cancellation/stale results,
policy restrictions, mount refusal, permanent deletion previews/confirmation, rename and copy.

Platform file-action fixtures verify complete scoped deletion without auxiliary storage; changed
scope/identity, protected descendants, symlink parents, missing mounts and cancellation refusal;
non-UTF-8 rename and no-overwrite/traversal rejection; nested file/folder copies, self/descendant
copy rejection and source retention. All destructive tests operate in temporary directories.
Engine evidence covers denied default writes, physical Copy/Paste, Rename and single-use Delete
confirmation, preserving the independent copy. Frontend tests drive actual right-click Copy/Paste
and Rename via native egui events, plus Cancel/confirm deletion through the real engine.

Other egui interaction coverage includes menus, inline expansion/search, address submission,
settings saves, theme-card/Cancel behavior, scaling, header drags and incremental Details drags.
Verified font hashes, absent/corrupt build inputs, explicit acquisition and light/dark typography
were checked using pinned official inputs; font binaries remain outside Git.

## Native checks

Native X11 checks use Xvfb with Mesa, XTest input, isolated XDG folders and four synthetic sources.
The appearance slice covers home, physical/merged/nested rows, short header dividers, a single
menu/search row, bottom address, resizable Details, preferences and Light/Dark themes. The current
file-action slice checks physical context menus, Rename, Paste and permanent-delete confirmation.
Native screenshots/logs are ignored under artifacts; they are manual evidence, not a maintained
screenshot-test suite.

## Remaining limits

No live writable NFS/disconnect/server-power-loss or real-library mutation test was performed.
No native Wayland/hardware compositor, complete IME/screen-reader, mixed-DPI or desktop text/motion
discovery acceptance is claimed. Snapshot/descriptor checks do not close the Linux final-filename
race against hostile concurrent writers. See file-actions.md for exact scope, staging, cancellation
and durability limitations. No copy metadata preservation or system file-clipboard interoperability.
