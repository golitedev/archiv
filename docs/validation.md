# Validation evidence

Updated: 2026-10-31. Fixtures and native captures use synthetic local trees, never the real NAS.

## Automated checks

Workspace checks use locked dependencies and parallelism of two: fmt, all-target check, Clippy
with warnings denied, 53 passing tests and a native debug build. The tests cover byte-preserving identities,
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

## Back/Forward navigation (2026-11-01)

The search icon is replaced by Back/Forward Lucide chevrons. Navigation uses typed engine
commands shared with View, Alt+Left/Alt+Right and egui Extra1/Extra2 mouse releases (mapped by
egui-winit to native Back/Forward buttons). Up and its shortcut are removed.

Engine fixtures cover physical locations, collection contributor identity across sources,
recursive search restoration, branch replacement, refresh exclusion, rejected navigation,
128-entry bounds and repeated layout dispatch. An egui interaction test drives the chevrons,
keyboard and mouse side buttons and checks that a modal blocks side-button navigation. Native
X11 checks inspect Light/Dark toolbar placement and exercise folder/back/forward navigation with
synthetic files. Physical hardware/Wayland mouse acceptance remains outside this environment.

## Stable file-row hover (2026-11-01)

Rows use container hover for names/icons/metadata, so child controls do not steal their shared
background. The disclosure button intentionally suppresses grey row hover while blue selection
persists. A half-open bottom edge gives exact row boundaries one owner. A rendering regression
failed on the original name hover, then caught duplicate highlights at y=135 between adjacent
rows. It now covers every one-point vertical step through eight folder names, horizontal name/
icon/column movement, chevron exclusion, pointer exit, modal coverage and persistent selection.
Native Light/Dark synthetic checks exercise a slow vertical sweep and inspect name/chevron
hover. No filesystem actions or NAS access are involved.

## Scrollbar lane (2026-11-01)

Header and file-row widths reserve the themed scrollbar's full width plus margins, while the
scroll area retains its existing floating behavior. A 100-file regression failed before the
fix: the row extended to x=937 while the scrollbar occupied x=929–937. It now verifies separate
selection/hover fills, a clear inner margin, matching header geometry, no file hover under the
scrollbar and stable columns after filtering away overflow. Native Light/Dark checks use an
overflowing synthetic folder list and inspect selection/hover beside the expanded scrollbar.
