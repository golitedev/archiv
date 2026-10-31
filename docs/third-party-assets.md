# Initial shell font resources

Archiv code uses 0BSD. Dependencies and their bundled resources retain their own terms.
No font binary is committed to this repository or downloaded by Archiv at runtime.

The first UI uses egui's default fonts while Inter/JetBrains Mono build inputs are deferred.
The renderer therefore follows the documented layout and colors but does not yet match the target
typography. No Unicode/emoji symbols are used as application action icons. Lucide assets are not
bundled yet; controls use text labels. The dependency's private-use icon font is included by its
normal font configuration, not used for Archiv branding.

Source: [epaint_default_fonts 0.37.0](https://docs.rs/crate/epaint_default_fonts/0.37.0/source/fonts/),
pinned in Cargo.lock. Resource authors and license texts:

- Hack: Source Foundry contributors and the upstream Bitstream/DejaVu contributors;
  [included terms](../licenses/fonts/Hack-Regular.txt).
- Ubuntu Light: Canonical Ltd.; [Ubuntu Font Licence](../licenses/fonts/UFL.txt).
- egui private-use icon subset: John Slegers and contributors;
  [MIT terms](../licenses/fonts/emoji-icon-font-mit-license.txt).

Recorded SHA-256 of dependency-bundled input files:

| Resource | SHA-256 |
| --- | --- |
| `Hack-Regular.ttf` | `15f55cc0c85a2988d2b4b3a8cdb5d77fdfbaf319e1bb5309d725db9818fb7125` |
| `Ubuntu-Light.ttf` | `80307b8da7649aa4ee4d484b232140e3ce1ec0ca093073d3c53c8f5a5ced7a70` |
| `egui-icons.ttf` | `122806555c73df7cef55aa1d945289ae27fbc4dbbd31abbb4204ee0cda829cc8` |

The `monochrome_emoji_fonts` feature is not enabled. The toolkit's fonts are already part of the
Cargo dependency package; these records do not authorize redistributing unrelated system fonts.
Recheck this inventory when upgrading the GUI toolkit or replacing the fallback fonts.
