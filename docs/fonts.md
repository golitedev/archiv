# Verified typography build inputs

Archiv uses Inter Regular, Medium and SemiBold 4.1 for interface text and JetBrains Mono Regular
2.304 for technical paths. The three Inter faces exactly match the PdfCraft reference's SHA-256
records. Regular, Medium and SemiBold use the same ordered toolkit fallback coverage. No font binary
is committed and no font is read from an installed system location or downloaded at runtime.

Prepare inputs once, then build/run the normal application:

```sh
cargo run -p archiv-fonts --locked
cargo run -p archiv --locked
```

The explicit Rust acquisition tool needs `curl` and `unzip`. It downloads the official, versioned
Inter archive and JetBrains Mono input over HTTPS, verifies the archive and every extracted face,
and installs only verified files in the ignored `artifacts/font-inputs` cache. It is independent
of the GUI crate, so it can repair missing or changed cache files even if GUI font verification
fails. `cargo run -p archiv-fonts --locked -- --check` checks the cache without networking.

`ARCHIV_FONT_DIR=/absolute/directory` overrides the build-input directory for both commands. The UI
build script performs no networking. It verifies and copies bytes into Cargo's build output before
embedding them, so editing an external input after verification cannot replace the included bytes.
An entirely absent cache builds with documented toolkit fallback fonts and a build warning; a
partial or changed cache fails the build instead of silently embedding unverified material.
Offline builds work with a prepared cache or the fallback. After fetching, rebuild Archiv on the
machine making the executable; an older PC executable does not gain fonts by changing the server.

Original sources and authors:

- Inter 4.1, The Inter Project Authors / Rasmus Andersson:
  https://github.com/rsms/inter/releases/tag/v4.1
- JetBrains Mono 2.304, The JetBrains Mono Project Authors:
  https://github.com/JetBrains/JetBrainsMono/releases/tag/v2.304

Both use SIL Open Font License 1.1. Complete notices are in `licenses/fonts/OFL-Inter.txt` and
`licenses/fonts/OFL-JetBrainsMono.txt`, also included in the application's About window. Distribute
these notices with builds. These are upstream open-font releases, not extracted system fonts.
Integrity pins are shared by the build script and acquisition tool in
`crates/ui-egui/font_inputs.rs`:

| Input | SHA-256 |
| --- | --- |
| Inter-4.1.zip | 9883fdd4a49d4fb66bd8177ba6625ef9a64aa45899767dde3d36aa425756b11e |
| Inter-Regular.ttf | 40d692fce188e4471e2b3cba937be967878f631ad3ebbbdcd587687c7ebe0c82 |
| Inter-Medium.ttf | 97ad806f526e41546d46365bb3a393145f75b7b1568913db74549ad8b8dba872 |
| Inter-SemiBold.ttf | 78a843fade9d4612a5567302fb595b56976eb5fcebf4fea5a5912d638bafcde3 |
| JetBrainsMono-Regular.ttf | a0bf60ef0f83c5ed4d7a75d45838548b1f6873372dfac88f71804491898d138f |
