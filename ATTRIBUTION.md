# Attribution

TextCraft's own code and original assets are under MIT OR Apache-2.0 (see [NOTICE](NOTICE)).
Third-party material in TextCraft, with its author, source and licence. The notices to ship with
the app are in [NOTICE](NOTICE); the licence texts, copied unmodified from the sources named below,
are in `licenses/`.

## Assets

- `app/assets/logo.png`: our own work. Drawn in code by `logo_rgba` in `app/src/main.rs` (export with `textcraft --export-logo <path>`); no third-party source.

## Material embedded through dependencies

None of these files are in this repository; the dependency compiles them into the app.

| Material | Author | Source | Licence |
|---|---|---|---|
| Ubuntu Light font 0.83 (`epaint_default_fonts::UBUNTU_LIGHT`, egui's default proportional font) | Canonical Ltd. | [storytold/egui@6d518fc](https://github.com/storytold/egui/tree/6d518fc799681571963842f95b55a6b68431b15a/crates/epaint_default_fonts) (epaint_default_fonts 0.37.0; the files are identical to the [crates.io release](https://crates.io/crates/epaint_default_fonts/0.37.0)), `fonts/Ubuntu-Light.ttf`, unmodified (sha256 `80307b8da7649aa4ee4d484b232140e3ce1ec0ca093073d3c53c8f5a5ced7a70`) | Ubuntu-font-1.0, [`licenses/egui-default-fonts/UFL.txt`](licenses/egui-default-fonts/UFL.txt) |
| Hack Regular font (`epaint_default_fonts::HACK_REGULAR`, egui's default monospace font, the editor font) | Source Foundry Authors; Bitstream, Inc.; DejaVu authors | [storytold/egui@6d518fc](https://github.com/storytold/egui/tree/6d518fc799681571963842f95b55a6b68431b15a/crates/epaint_default_fonts) (epaint_default_fonts 0.37.0; the files are identical to the [crates.io release](https://crates.io/crates/epaint_default_fonts/0.37.0)), `fonts/Hack-Regular.ttf`, unmodified (sha256 `15f55cc0c85a2988d2b4b3a8cdb5d77fdfbaf319e1bb5309d725db9818fb7125`) | MIT AND Bitstream-Vera (DejaVu parts public domain), [`licenses/egui-default-fonts/Hack-Regular.txt`](licenses/egui-default-fonts/Hack-Regular.txt) |
| egui-icons font (`epaint_default_fonts::EGUI_ICONS`), a five-glyph subset of emoji-icon-font made by egui | John Slegers | [storytold/egui@6d518fc](https://github.com/storytold/egui/tree/6d518fc799681571963842f95b55a6b68431b15a/crates/epaint_default_fonts) (epaint_default_fonts 0.37.0; the files are identical to the [crates.io release](https://crates.io/crates/epaint_default_fonts/0.37.0)), `fonts/egui-icons.ttf`, unmodified (sha256 `122806555c73df7cef55aa1d945289ae27fbc4dbbd31abbb4204ee0cda829cc8`); [original](https://github.com/jslegers/emoji-icon-font) | MIT, [`licenses/egui-default-fonts/emoji-icon-font-mit-license.txt`](licenses/egui-default-fonts/emoji-icon-font-mit-license.txt), subset note in [`egui-icons.txt`](licenses/egui-default-fonts/egui-icons.txt) |
| Lucide icon font (`lucide_icons::LUCIDE_FONT_BYTES`), the interface icons | Lucide Icons and Contributors; Cole Bemis (icons derived from Feather) | [lucide-icons 1.78.0](https://crates.io/crates/lucide-icons/1.78.0) (wrapper crate by WhySoBad), `lucide.ttf`, unmodified (sha256 `3c501c6ffd4ab74a2138e5348ef8387ddb5329961be4ed5c730ed0b6a6042130`), built from [Lucide 1.78.0](https://github.com/lucide-icons/lucide/tree/1.78.0) | ISC; Feather-derived icons also MIT. [`licenses/lucide/LICENSE`](licenses/lucide/LICENSE), from Lucide's repository at tag `1.78.0` (the crate ships no licence file) |
| Unicode normalization and character property data (`icu_normalizer_data`, `icu_properties_data` 2.4.0), used by `url`/`idna` through eframe's link opening | Unicode, Inc. and ICU4X contributors | [ICU4X](https://github.com/unicode-org/icu4x), published crates, unmodified | Unicode-3.0, [`licenses/icu4x/LICENSE`](licenses/icu4x/LICENSE) |

egui's emoji fonts (Noto Emoji, the full emoji-icon-font) are not embedded: TextCraft builds
without `epaint_default_fonts`' `monochrome_emoji_fonts` feature.
