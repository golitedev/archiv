# Visual resources and code reuse

Status: default toolkit fonts are dependency-bundled; no font binaries are committed. Updated: 2026-10-31.

## Resource selection

Archiv uses a compact desktop interface with Inter for ordinary UI text, JetBrains Mono for
selected technical details, and Lucide SVG icons as its target design. The initial shell uses
toolkit fallback fonts and text actions; see [resource records](third-party-assets.md). Pinned, attributed Lucide SVGs are now bundled; custom fonts remain deferred. Use original artwork for Archiv branding.

Before adding a resource, verify that its terms permit the intended use and distribution. Obtain
files from an identifiable original release and pin the version. Avoid extracted system fonts,
proprietary application graphics, copied logos, stock imagery with unclear rights, and files of
unknown origin. Use only openly reusable or contributor-original resources; personal-use-only
terms are not acceptable for bundled assets. Verify authorship, source, intended use and required
notices before adding anything. Do not trace or extract proprietary interface graphics. An open
font obtained from its official release is different from a copy extracted from a proprietary app.
Screenshots and sample directories must use synthetic, non-personal content.

## Fonts and delivery

Keep font binaries out of Git. Introduce explicit, versioned build inputs with integrity checks
when custom fonts are implemented. Document offline and fallback behavior. The application must
not silently download fonts at runtime. Review resources bundled by GUI dependencies as well.
A system font opened in place for runtime fallback is not permission to copy, embed or redistribute
it. Published captures must use verified reusable assets, including any fonts visible in them.

## Third-party additions

When a future dependency or asset requires a notice, include it alongside that material and
record its version, original source URL, author, intended use and applicable terms.
Track file integrity for downloaded/build-input binaries and retain the required license text. Keep this bookkeeping proportional to the resources
actually present; no standalone asset registry or validation command exists yet.

Review dependencies for maintained safe APIs, compatible terms, build cost and Linux support.
Use external mpv, rsync and OpenSSH as integrations; do not copy their implementation into Archiv.
