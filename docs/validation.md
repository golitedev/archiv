# Validation evidence

Updated: 2026-10-31. Fixtures and native captures use synthetic local trees, never the real NAS.

## Automated checks

Workspace checks use locked dependencies and parallelism of two: fmt, all-target check, Clippy
with warnings denied, 47 passing tests and a native debug build. The tests cover byte-preserving identities,
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

## Sources sidebar sizing

A synthetic drag regression with long source and collection names failed before the fix: labels
forced the workspace's Name header to x=562.94 and the first 4-point pointer move did not move
the divider. The sidebar now fills its requested width and truncates navigation labels, with
full names/physical paths available on hover. The test checks small incremental expansion,
contraction and released geometry; the existing Details drag regression remains in the suite.
Native Light/Dark checks use the same synthetic long labels and actual splitter drags.

## Single-line field alignment (2026-11-01)

Single-line folder, location, Rename, Preferences and search inputs share a centered field
builder with the documented minimum height and symmetric padding. Multiline fields retain
top alignment. A geometry regression measures actual laid-out text against its field center
in horizontal/vertical layouts at 100%, 125% and 200% scale and ordinary/taller field heights.
Removing vertical centering makes it fail (text center y=14 versus field center y=15 at 100%).
Native X11 Light/Dark checks inspect typed synthetic text and caret placement in the folder
dialog and bottom location field. Input/navigation interaction tests remain passing.

## Consolidated settings directory (2026-11-01)

The native executable explicitly places eframe's `app.ron` beside the default `library.json`
in `$XDG_CONFIG_HOME/archiv` (normally `~/.config/archiv`). `--library FILE` still overrides only
library storage. A native X11 launch with isolated config/data directories followed by File →
Quit wrote presentation/window state in the config directory; no `archiv/app.ron` appeared in
XDG_DATA_HOME. No migration or compatibility reader is present. Workspace fmt/check/Clippy and
all 47 tests passed; the native executable was rebuilt for this persistence check.
