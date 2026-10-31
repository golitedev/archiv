# File-manager / PdfCraft reference audit

Audited against the user's supplied screenshots and the reference source, then checked in the
actual native application using synthetic data. This is implementation evidence, not a claim
that every future roadmap feature or every desktop environment has passed acceptance.

- **Normal text menus:** File/Edit/View/Help, with shared engine commands for file actions.
  Navigation remains a file-manager toolbar rather than PDF tool modes.
- **Meaningful colored icons:** licensed, pinned Lucide folder/disclosure/image/video/text icons;
  no PdfCraft branding, decorative tool grid, emoji or copied proprietary assets.
- **Grey central panels:** rounded neutral Start browsing and Your workspace panels with working
  actions. No imitation community link or unrelated marketing content.
- **Top-right search:** immediate current/expanded-view filtering; Enter explicitly searches
  subfolders. Recursive search is bounded, not an unrequested startup/library scan or an index.
- **Compact nested folders:** 26-point rows, one-click disclosure arrows, double-click/Enter
  navigation and Ctrl+L address navigation. Exact paths retain source identity.
- **Selection / columns:** rounded selection, exact documented selection tokens, aligned metadata
  headers; no full-height column dividers or unstable drag handles. Metadata columns remain stable
  while the name column adapts to available width.
- **Configurable collections:** arbitrary names and member paths; multiple folders per source are
  supported. Nothing is automatically merged on disk or included by default. Files with the same
  name retain distinct physical rows and locations. No built-in Movies/TV/Anime categories.
- **Preferences:** categorized pages, shared field/button treatment, blue primary pills, light/dark
  file-manager preview cards. Whole cards select themes, System marks the resolved theme active,
  and Cancel/Escape revert appearance changes. Failed applies do not advance the cancel baseline. OK saves the library before closing; explicit Save
  library applies without closing. Appearance changes made before that apply become its baseline.
- **Typography:** verified Inter Regular/Medium/SemiBold build inputs and JetBrains Mono paths.
  Previously this was a real mismatch: the prior build used egui fallback fonts. Follow fonts.md
  when rebuilding to include the reference typography; no font binaries or runtime downloads.
- **Application opening:** user-configurable image/media/text/other programs (including imv, mpv,
  kwrite) with literal arguments, no shell interpolation, and physical-file revalidation. External
  applications retain their ordinary permissions; safeguards are not an application sandbox.
- **Configurable deletion safeguards:** deny-until-configured allowlists, protected structures and
  subtrees, source/collection root protection, explicit physical previews and recovery/restore.
  Virtual folders never expand into hidden deletion targets. No permanent purge is implemented.
  See file-actions.md for concurrency and durability limits.
- **NFS integration:** mounted-source export/descriptor checks are implemented and missing mounts
  fail closed. Actual Debian exports and Arch mounts are **not configured**: the intended PC's
  LAN address/access is still needed. Read-only inspection during this audit found the NAS NFS
  service inactive. No NAS settings or contents were changed. Writable NFS recovery/server-disconnect
  acceptance has not been performed on the real library.

Remaining platform acceptance includes native Wayland, hardware/mixed-DPI compositor behavior,
screen-reader/IME coverage and system text-scale/motion discovery. These are recorded limits;
renaming, transfers, copy/paste, indexed search and alias rules were not part of this implemented
slice and are not presented as shipped.
