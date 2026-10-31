# Visual resources and code reuse

Status: project guidelines; no visual assets are bundled. Updated: 2026-10-31.

## Resource selection

Archiv uses a compact desktop interface with Inter for ordinary UI text, JetBrains Mono for
selected technical details, and Lucide SVG icons. These are design choices; no font or icon files
have been added yet. Use original artwork for Archiv branding.

Before adding a resource, verify that its terms permit the intended use and distribution. Obtain
files from an identifiable original release and pin the version. Avoid extracted system fonts,
proprietary application graphics, copied logos, stock imagery with unclear rights, and files of
unknown origin. Screenshots and sample directories must use synthetic, non-personal content.

## Fonts and delivery

Keep font binaries out of Git. Introduce explicit, versioned build inputs with integrity checks
when custom fonts are implemented. Document offline and fallback behavior. The application must
not silently download fonts at runtime. Review resources bundled by GUI dependencies as well.

## Third-party additions

When a future dependency or asset requires a notice, include it alongside that material and
record its version, origin and intended use. Keep this bookkeeping proportional to the resources
actually present; no standalone asset registry or validation command exists in this scaffold.

Review dependencies for maintained safe APIs, compatible terms, build cost and Linux support.
Use external mpv, rsync and OpenSSH as integrations; do not copy their implementation into Archiv.
