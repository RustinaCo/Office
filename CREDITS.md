# Credits

Rustina Office would not exist without the people who wrote the programs it is built on. This page
says exactly who did what, and under which licence.

## The ArtCraft team — the upstream apps

**Thank you.** The ArtCraft team published their engine and interface openly, and that single
decision is the reason Rustina Office exists at all; these three apps are a rebuild of their work for
the phone, and everything good about them starts there.

The document, spreadsheet and presentation engines, the ribbon and the whole desktop interface come
from the **ArtCraft team**'s clean-room, open-source work:

| Upstream | What it is | This repository's app |
|---|---|---|
| [WordCraft](https://github.com/storytold/wordcraft) | Word-style word processor, pure Rust | [Rustina Write](Write/README.md) |
| [GridCraft](https://github.com/storytold/gridcraft) | Excel-style spreadsheet, pure Rust | [Rustina Tables](Tables/README.md) |
| [DeckCraft](https://github.com/storytold/deckcraft) | PowerPoint-style presentations, pure Rust | [Rustina Slides](Slides/README.md) |

Website: **https://getartcraft.com/** — thank you. Their licence is **MIT OR Apache-2.0**, and both
texts are kept with the source in each app folder (`LICENSE-MIT`, `LICENSE-APACHE`), together with
their `NOTICE` and `ATTRIBUTION.md` files. Nothing in this repository removes or replaces them.

What this repository adds on top:

- the **Android shell** for each app (activity, file picker, IME, system bars, pinned window, SAF
  save flow, the library copy and the write-back mirror);
- the **phone interface**: the full-screen File list, the per-destination pages, the touch ribbon,
  the bigger controls, the splash;
- the **Back-button behaviour** (keyboard first, then the page or dialog, then the save question),
  **working AutoSave** on Android, and the keyboard that stays where you put it;
- **22 open-licensed Persian/Arabic font families** plus the *Add Font…* path that keeps a font you
  installed inside the app;
- **Rustina Slides**' original template gallery.

## Rustina Office

The suite itself — the three Android apps, the phone work listed above, the Rustina names, the
packaging and these pages — is by **Sina Khanzadeh** ([@programmer-1998](https://github.com/programmer-1998),
`sina-khanzadeh.ir`), released under the MIT licence in [`LICENSE`](LICENSE).

## Fonts

Every bundled font is under the **SIL Open Font License 1.1**, which allows redistribution. The
author, source and licence of each file are listed in `fonts/ATTRIBUTION.md`, inside each app folder.
No commercial font (including the Iranian families B Nazanin, B Titr, B Yekan, IRANSans and
IranNastaliq) is bundled: users who own one add it themselves with **Add Font…**.

## Everything else

- **Rust**, **egui/eframe/winit**, **vello**, **krilla** and the rest of the crates in `Cargo.toml`
  — the Rust ecosystem, by its many authors.
- The **Craft libraries** (`craft-ui`, `craft-fonts`, `craftrules`), by the same team.
- The screenshots in each app folder are of these apps themselves; the desktop screenshot is from
  the upstream project's own documentation.

## Contributing

If you fix something or add a feature, please add a line here if it is anyone's work but your own —
and open a pull request: https://github.com/RustinaCo/Office.
