# Current file-manager request audit

- PdfCraft-style quiet neutral panels, ordinary File/Edit/View/Help menus, top-right search,
  Inter typography, blue preference controls and meaningful colored Lucide icons.
- One top row; navigation and hidden-file visibility in View, Preferences in Edit, theme only
  in Preferences. Editable physical location at the bottom with Ctrl+L and Enter.
- Compact nested rows, one-click disclosure arrows, rounded selection, short draggable header
  separators, flexible Name column and width-constrained Details content.
- Generic optional visual collections, arbitrary configured names and multiple member roots,
  no hardcoded media categories. Duplicate files retain distinct physical sources.
- Bounded explicit recursive search and configurable image/media/text/other executable handlers.
- Physical file/folder context menus for Open, Copy, Copy path, Paste, Rename and Delete.
  Edit and shortcuts share engine commands. Empty workspace menus offer Paste and Refresh.
- Copy/Paste works with Archiv's session clipboard; Rename is within the original directory.
  Existing destinations are refused. File changes require explicit source opt-in and allowlists.
- Delete is permanent. Its bounded preview names every physical descendant; confirmation is
  single-use and separate from selection. Source/collection roots and configured protections
  remain enforced. Merged folders have no implicit physical mutation target.
- The application has one clean configuration schema and no compatibility/migration layer.
- Mounted-source NFS checks exist, but actual Debian exports and PC mounts are not configured.
  Real writable-NFS/disconnect acceptance remains pending; no real NAS mutation testing.

See file-actions.md for copy metadata/staging limits, partial cancellation and concurrent-writer
limitations. Multi-selection, Cut, cross-application file clipboard support, queued transfers,
indexed search and broader Wayland/IME/screen-reader/hardware acceptance remain future work.
