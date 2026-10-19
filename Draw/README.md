# Rustina Draw

**CADCraft, on your phone.**

[← Rustina Office](../README.md) · [فارسی](README.fa.md) · [Write](../Write/README.md) ·
[Tables](../Tables/README.md) · [Slides](../Slides/README.md)

![Platform: Android 10+](https://img.shields.io/badge/Android-10%2B%20(arm64)-3ddc84?style=flat-square&logo=android&logoColor=white)
![Written in Rust](https://img.shields.io/badge/written%20in-Rust-b7410e?style=flat-square&logo=rust&logoColor=white)
![License: MIT OR Apache-2.0](https://img.shields.io/badge/license-MIT%20%2F%20Apache--2.0-blue?style=flat-square)
![Version 0.9.0](https://img.shields.io/badge/version-0.9.0-e51050?style=flat-square)
![Status: pre-alpha](https://img.shields.io/badge/status-pre--alpha-f07a3a?style=flat-square)
![No account](https://img.shields.io/badge/account-none-2ea44f?style=flat-square)
![No network permission](https://img.shields.io/badge/network%20permission-none-2ea44f?style=flat-square)

**Download — direct APK link:**
[**Rustina Draw** · 0.9.0 · 13.6 MB](https://github.com/RustinaCo/Office/raw/v1.0.3/apk/Rustina-Draw-0.9.0.apk)

---

## What it is

The ArtCraft team's open-source **CADCraft** — the same engine and the same interface as the desktop
application — rebuilt for Android. It is a 2D drafting program in the AutoCAD mould: the command
line, object snaps, layers, dimensions, hatches, blocks and DXF/DWG drawings you already know, at
phone size.

> **Status: pre-alpha.** The engine is real and the workflows below work, but four of AutoCAD's five
> core workflows still fall short of the upstream alpha gate: precise drafting lacks object snap
> tracking and has stubbed TRIM/EXTEND/OFFSET options, blocks cannot be edited in a block editor,
> plot styles are not applied, and a DWG saved by the app keeps only what it models. Treat this as a
> preview — the desktop build of CADCraft is where the project is heading.

## What works

- **The command line**, the way CAD is actually driven: `LINE`, `CIRCLE`, `RECTANG`, `MOVE`, `COPY`,
  `ROTATE`, `SCALE`, `OFFSET`, `TRIM`, `EXTEND`, `FILLET`, `CHAMFER`, `HATCH`, `DIM`, `MTEXT`,
  `BLOCK`, `INSERT` and the rest — with prompts, keyword options, `@dx,dy`, `@d<a` and direct
  distance entry. A floating keyboard button raises Android's own keyboard (suggestions and
  dictation included) over the drawing.
- **Files.** DXF opens and saves, DWG opens, and the engine's own drawings round-trip. Open and save
  go through Android's file picker, so Google Drive, OneDrive, Dropbox, Nextcloud, a NAS or a USB
  stick all work — without a network permission.
- **Drafting aids.** Object snaps (endpoint, midpoint, centre, intersection, perpendicular, tangent,
  nearest, quadrant), ortho and polar tracking, grid and snap, layers, linetypes, lineweights, text
  styles, dimension styles and hatch patterns — every hatch pattern and linetype defined by the
  project itself, not copied from anyone.
- **Annotation.** Single-line text and MText in the bundled stroke font or any TrueType family,
  dimensions with the usual styles, leaders and multileaders, tables.
- **Blocks and references**, with a bounded block depth so a cyclic reference cannot hang the app.
- **Zoom and pan** with pinch and drag; the interface is scaled up for fingers (135 % of the
  display's own scale) and the tool bar keeps big touch targets.

## What the phone build changes

Everything below is on top of the desktop app; the desktop layout is untouched.

- **The keyboard is a first-class citizen.** CAD is typed, so the real Android keyboard (with its
  suggestions, swipe typing and dictation) is overlaid on the drawing and its text is fed to the
  command line and every dialog, Backspace and Enter included. A floating button toggles it, because
  the command line is painted by the app and never asks egui for a keyboard by itself.
- **Android's file picker, both ways.** Opening hands the engine the bytes of the picked or shared
  drawing; saving writes to app storage at once and mirrors the file to the destination you pick, so
  later saves and autosaves follow the same file.
- **System bars and safe areas** are drawn by the shell, theme-matched, so nothing hides under the
  status bar or the gesture bar.
- **No runtime permissions.** The app asks Android for nothing: no network, no storage, no account.
- **The canvas draws on the CPU.** The desktop app hands its display list to a wgpu paint callback;
  the phone build lets egui tessellate it instead. Correct for any drawing, slower on very heavy
  ones — the escape hatch is in place to switch later.

## Known gaps (first version)

- The floating keyboard button's position and the keyboard overlay are reasoned, not device-tested.
- A picked or shared drawing opens *pathless*, so its own **Save** asks for a destination instead of
  rewriting the file you opened.
- `IMPORT`, `RECOVER` and *New from Template* start the picker but cannot finish: those commands need
  a path and the picker returns bytes.
- Pasting is unavailable: eframe's Android backend does not read the clipboard, so egui never emits a
  paste event (the JNI read exists, unused).
- The soft keyboard supplies text, Backspace and Enter only — no Escape, arrows or modifiers; those
  stay on the on-screen UI.
- A picked drawing is decoded on the UI thread, so a very large file freezes a frame while it opens.

## Install

**This is the first release of Rustina Draw, and it is a preview.** If you hit a bug, please
[open an issue](https://github.com/RustinaCo/Office/issues) — with what you did, your phone model and
your Android version — and it gets looked at.

1. Download the APK — the link downloads the file straight away, no release page needed:
   [Rustina-Draw-0.9.0.apk](https://github.com/RustinaCo/Office/raw/v1.0.3/apk/Rustina-Draw-0.9.0.apk)
   (it also lives in [`apk/`](../apk) with its SHA-256).
2. Open it on the phone and allow installing from this source when Android asks.
3. That's it — no account, no ads, no telemetry, no network permission.

**Requirements:** Android 10 (API 29) or newer, arm64-v8a.

## Building from source

This folder is a self-contained pair: the upstream app (`cadcraft/`) plus its Android shell
(`cadcraft-android/`). Fonts are an optional build input: `fonts/` is the same 27 open-licensed
families the other Rustina apps ship, with a craft-fonts `manifest.txt`.

```bash
cd cadcraft-android
export ANDROID_HOME=$HOME/Android/Sdk
export ANDROID_NDK_ROOT=$HOME/Android/Sdk/ndk/<version>
export CRAFT_FONTS_DIR=../fonts          # optional: embeds the TrueType families
export CARGO_APK_RELEASE_KEYSTORE=/path/to/release.keystore
export CARGO_APK_RELEASE_KEYSTORE_PASSWORD=…
cargo apk2 build --lib --release        # cargo-apk ≥ 0.10
```

For the desktop build, `cargo run --release -p cadcraft` in `cadcraft/` works on Linux, macOS,
Windows and BSD (and compiles to WebAssembly the same way).

## Credits

Rustina Draw is the **ArtCraft team**'s CADCraft, rebuilt for the phone. The engine, the command
system, the renderer, the interface and the standards behind them are their open-source work, and
without their choice to publish it openly this app would not exist. **Thank you.** Full credit and
licence texts: [`CREDITS.md`](../CREDITS.md).

## License

MIT OR Apache-2.0 — the upstream CADCraft licence, kept as it was. See [`cadcraft/LICENSE-MIT`](cadcraft/LICENSE-MIT)
and [`cadcraft/LICENSE-APACHE`](cadcraft/LICENSE-APACHE).
