<div align="center">

# Rustina Office

**Microsoft Office, on your phone — the same three apps, the same ribbon, the same options as the
desktop, rebuilt lightweight for Android.**

[English](README.md) · [فارسی](README.fa.md) · [Write](Write/README.md) · [Tables](Tables/README.md) · [Slides](Slides/README.md)

![Platform: Android 10+](https://img.shields.io/badge/Android-10%2B%20(arm64)-3ddc84?style=flat-square&logo=android&logoColor=white)
![Written in Rust](https://img.shields.io/badge/written%20in-Rust-b7410e?style=flat-square&logo=rust&logoColor=white)
![License: MIT](https://img.shields.io/badge/license-MIT-blue?style=flat-square)
![Version 1.0.2](https://img.shields.io/badge/version-1.0.2-2b47b5?style=flat-square)

</div>

---

## What this is

Rustina Office is three Android apps — a word processor, a spreadsheet and presentations — that bring
the desktop workflow you already know to a phone. They read and write the Office file formats, so the
work you already have opens here:

| App | What it is | Open | Save |
|---|---|---|---|
| **[Rustina Write](Write/README.md)** | A word processor — documents, styles, tables, track changes, references, mail merge | `.docx`, `.txt`, `.md`, `.html`, `.rtf`, `.odt` | `.docx`, `.pdf`, `.txt`, `.html` |
| **[Rustina Tables](Tables/README.md)** | A spreadsheet — formulas, formatting, tables, charts, data tools | `.xlsx`, `.csv` | `.xlsx`, `.csv`, `.pdf` |
| **[Rustina Slides](Slides/README.md)** | Presentations — themes, layouts, shapes, pictures, charts, transitions, animations, slide shows | `.pptx` | `.pptx`, `.pdf` |

The whole suite is written in **Rust** — one engine, one UI, three apps — and the Android build reuses
the *same* desktop front end: the ribbon, the tabs, the dialogs and every option are the ones from the
desktop apps, laid out for a touch screen. Nothing is a stripped-down "mobile edition".

<div align="center">

| **Write** — File page | Document | New document | Options |
|---|---|---|---|
| <img src="Write/screenshots/file-page.png" width="190"> | <img src="Write/screenshots/document.png" width="190"> | <img src="Write/screenshots/new-document.png" width="190"> | <img src="Write/screenshots/options.png" width="190"> |
| **Tables** — File page | Sheet | New workbook | Options |
| <img src="Tables/screenshots/file-page.png" width="190"> | <img src="Tables/screenshots/grid.png" width="190"> | <img src="Tables/screenshots/new-workbook.png" width="190"> | <img src="Tables/screenshots/options.png" width="190"> |
| **Slides** — File page | Slide | New presentation | Slide show |
| <img src="Slides/screenshots/file-page.png" width="190"> | <img src="Slides/screenshots/slide.png" width="190"> | <img src="Slides/screenshots/new-deck.png" width="190"> | <img src="Slides/screenshots/presenting.png" width="190"> |

</div>

## Why people use it

- **Manage your laptop's files from your phone, anywhere, without any hassle.** If the folder you work
  in is synced to the phone (Google Drive, OneDrive, Dropbox, Nextcloud, Syncthing, a NAS…) the apps
  open and save straight into it through the system file manager — there is no import/export dance,
  no "share to the app, edit, share back".
- **All options exactly like the desktop.** The same ribbon tabs, the same dialogs, the same settings.
  If you know Word, Excel or PowerPoint, you already know these apps.
- **Light and fast.** A native Rust binary. It starts in a moment, scrolls at the display's frame rate
  and is a fraction of the size of the alternatives — the whole app is a few megabytes, fonts and all.
- **Your writing is safe.** AutoSave keeps the file up to date, and Android's Back button asks
  *Save / Don't Save / Cancel* before a document with unsaved changes is closed — exactly like closing
  it on a computer.
- **Persian, Arabic and other scripts, done properly.** 22 open-licensed Persian/Arabic font families (and five metric-compatible Latin ones)
  ship in the box, right-to-left text is laid out correctly, and any font you own can be added with
  **Add Font…** and stays installed for good.

## Install

**This is the first release.** The three apps are new, so if you hit a bug anywhere, please
[open an issue](https://github.com/RustinaCo/Office/issues) — with what you did, your phone model and
your Android version — and it gets fixed quickly.

1. Download the APK you want — these links download the file straight away, no release page needed:
   - **Rustina Write** (documents, 18.3 MB) —
     [Rustina-Write-1.0.2.apk](https://github.com/RustinaCo/Office/raw/v1.0.2/apk/Rustina-Write-1.0.2.apk)
   - **Rustina Tables** (spreadsheets, 9.3 MB) —
     [Rustina-Tables-1.0.2.apk](https://github.com/RustinaCo/Office/raw/v1.0.2/apk/Rustina-Tables-1.0.2.apk)
   - **Rustina Slides** (presentations, 12.8 MB) —
     [Rustina-Slides-1.0.2.apk](https://github.com/RustinaCo/Office/raw/v1.0.2/apk/Rustina-Slides-1.0.2.apk)
   (All three also live in [`apk/`](apk), with their SHA-256 checksums.)
2. Open it on the phone and allow installing from this source when Android asks.
3. That's it — no account, no ads, no telemetry, no network permission.

**Requirements:** Android 10 (API 29) or newer, arm64-v8a. The app asks only for the files you hand it
through the system picker.

## What the phone build changes

Everything below is on top of the desktop apps; the desktop layout is untouched.

- **The File page, rebuilt for the thumb.** Phones open on **File**: a full-screen list of destinations
  (Home, New, Open, Info, Save, Save As, Print, Export, About, Options) — the same list on the app's
  first screen and when you tap **File** from the document. Tap one and you get that one page, big and
  full screen; **Back** returns to the list. The old launch menu is gone, and Close is left out: on a
  phone the document is closed with Android's Back.
- **Android's Back button behaves.** It closes the keyboard first, then the page or dialog on top, and
  in the document it asks **Save / Don't Save / Cancel** when there is unsaved work. On the File list
  **Back** appears as soon as there is a document behind it to step into; on the app's first screen it
  is not shown, and Back there leaves the app.
- **AutoSave that actually saves.** Once the document has a file, the AutoSave switch in the title bar
  turns on and keeps writing it — including the copy the app keeps for itself, so nothing is lost even
  if the phone is put away mid-edit.
- **A keyboard that stays put.** Tapping the page raises the keyboard; tapping a toolbar button, a menu
  or any other control puts it away and keeps it away. Typing, selection and the mini toolbar are
  untouched.
- **Large touch targets.** Big, tall ribbon buttons on every tab except Home (which keeps the real
  two-row layout), pill-shaped tab tokens, and a footer with the page and zoom controls.

## Screenshots

<table>
  <tr>
    <td align="center"><img src="Write/screenshots/file-page.png" width="200"><br><sub>Write — File</sub></td>
    <td align="center"><img src="Write/screenshots/document.png" width="200"><br><sub>Write — Editing</sub></td>
    <td align="center"><img src="Write/screenshots/new-document.png" width="200"><br><sub>Write — New</sub></td>
    <td align="center"><img src="Write/screenshots/about.png" width="200"><br><sub>Write — About</sub></td>
  </tr>
  <tr>
    <td align="center"><img src="Tables/screenshots/file-page.png" width="200"><br><sub>Tables — File</sub></td>
    <td align="center"><img src="Tables/screenshots/grid.png" width="200"><br><sub>Tables — Editing</sub></td>
    <td align="center"><img src="Tables/screenshots/new-workbook.png" width="200"><br><sub>Tables — New</sub></td>
    <td align="center"><img src="Tables/screenshots/options.png" width="200"><br><sub>Tables — Options</sub></td>
  </tr>
  <tr>
    <td align="center"><img src="Slides/screenshots/file-page.png" width="200"><br><sub>Slides — File</sub></td>
    <td align="center"><img src="Slides/screenshots/slide.png" width="200"><br><sub>Slides — Editing</sub></td>
    <td align="center"><img src="Slides/screenshots/new-deck.png" width="200"><br><sub>Slides — New</sub></td>
    <td align="center"><img src="Slides/screenshots/presenting.png" width="200"><br><sub>Slides — Slide show</sub></td>
  </tr>
</table>

Write's desktop build is in the same source, unchanged:

<img src="Write/screenshots/desktop.png" width="640">

## Building from source

Each folder is a self-contained pair: the cross-platform app plus its Android shell. The only thing
they need from outside is the folder of bundled fonts, which is inside each app folder.

```bash
# toolchain
rustup target add aarch64-linux-android
cargo install cargo-apk

# Android SDK/NDK, then, in any app folder's android/ directory:
cd Write/word-android
export ANDROID_HOME=$HOME/Android/Sdk
export ANDROID_NDK_ROOT=$HOME/Android/Sdk/ndk/<version>
cargo apk build --lib --release     # cargo-apk ≥ 0.10: `cargo apk2 build --lib --release`
```

For the desktop build of an app, `cargo run --release -p wordcraft` (or `gridcraft`, `deckcraft`) in
its app folder works on Linux, macOS, Windows and BSD, and the same crates compile to WebAssembly.

## Credits

This suite stands on the **ArtCraft team**'s open-source work — the original clean-room
WordCraft / GridCraft / DeckCraft engine and interface — released under MIT OR Apache-2.0. Thank you.
Full credit, links and licence texts: [`CREDITS.md`](CREDITS.md).

## Contributing

Contributions are very welcome — this is a young fork of a big codebase:

- **Bug reports** with the phone model, the Android version and what you did.
- **Translations** (the interface is translated; Persian and English come first).
- **Fonts**: more open-licensed families for Persian, Arabic, Urdu and the other scripts.
- **Pull requests**: please keep the desktop layout working, and add a test or a screenshot with the
  change.

Open an [issue](https://github.com/RustinaCo/Office/issues) or a pull request. By contributing you
agree your work is licensed under the MIT licence below.

## License

MIT — see [`LICENSE`](LICENSE). The upstream apps are MIT OR Apache-2.0; their texts are kept next to
the source in each app folder (`LICENSE-MIT`, `LICENSE-APACHE`).

Microsoft, Word, Excel and PowerPoint are trademarks of Microsoft Corporation. This project is an
independent, clean-room open-source implementation and is not affiliated with, endorsed by or
sponsored by Microsoft.
