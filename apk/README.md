# Rustina Office APKs

[← Rustina Office](../README.md) · [Write](../Write/README.md) · [Tables](../Tables/README.md) · [Slides](../Slides/README.md) · [Draw](../Draw/README.md)

Signed release builds of the four apps: **arm64-v8a only**, **Android 10 (API 29) or newer**.
Install one by opening the file on the phone and allowing installs from this source when Android
asks (and turning off "install via USB" restrictions if your phone has them).

| File | App | What it opens | Size |
|---|---|---|---|
| [`Rustina-Write-1.0.2.apk`](https://github.com/RustinaCo/Office/raw/v1.0.3/apk/Rustina-Write-1.0.2.apk) | Rustina Write | `.docx`, `.txt`, `.md`, `.html`, `.rtf`, `.odt` | 18.3 MB |
| [`Rustina-Tables-1.0.2.apk`](https://github.com/RustinaCo/Office/raw/v1.0.3/apk/Rustina-Tables-1.0.2.apk) | Rustina Tables | `.xlsx`, `.csv` | 9.4 MB |
| [`Rustina-Slides-1.0.2.apk`](https://github.com/RustinaCo/Office/raw/v1.0.3/apk/Rustina-Slides-1.0.2.apk) | Rustina Slides | `.pptx` | 12.8 MB |
| [`Rustina-Draw-0.9.0.apk`](https://github.com/RustinaCo/Office/raw/v1.0.3/apk/Rustina-Draw-0.9.0.apk) | Rustina Draw | `.dxf`, `.dwg` | 13.6 MB |

These are built from the sources in this same tag — `Write/`, `Tables/`, `Slides/` and `Draw/` — with
`cargo apk2 build --lib --release` (each app folder's README has the exact commands). They are signed
with a self-signed release key so they can be sideloaded; Android will warn about an unknown
developer the first time.

SHA-256 of each file at this tag, if you want to check what you downloaded:

```
31f006252a6d71f10f1f500c1a28f3969da5ed472382f428d7280719531a70bb  Rustina-Write-1.0.2.apk
54ff19feb00fd2ac7da9bbc3038b19aecad27f6b6098a28f8707ddd107f92379  Rustina-Tables-1.0.2.apk
ed6ad06a055fdc6daf6e2d42f11193960570c8964660803cfd3cd71bfe2a7ccb  Rustina-Slides-1.0.2.apk
71c2b2c202ce73ee9380fa2dba80a7fb7faf33b165e0dda7f08c163028a4eafc  Rustina-Draw-0.9.0.apk
```

**1.0.3** adds the fourth app, **Rustina Draw** (0.9.0): the ArtCraft team's CADCraft, the AutoCAD-style 2D drafter, rebuilt for Android with the command line, object snaps, layers, dimensions, hatches, blocks and DXF/DWG files. It is a preview — the engine is pre-alpha, the canvas renders on the CPU and the gaps are listed in [Draw/README.md](../Draw/README.md). The three office apps are unchanged.

**1.0.2** gives the three apps their own artwork and renames one of them: every app now ships a
launcher icon in its own colour with its initial — Write blue, Tables green, Slides red-orange — as a
sharp-cornered square with a soft gradient, a matching Android 8+ adaptive icon, and the same mark on
the launch splash. No Office artwork anywhere. **Rustina Gride is now Rustina Tables** (same package,
so it installs as an update over the old one). Write and Slides also got the size-optimised release
profile, so their APKs are smaller. In Slides the theme tiles now show the theme's real slides — the title slide and a content slide — instead of a decorative drawing, so the preview matches what you get. And a theme now designs the slides themselves: every layout and the master carry a theme-coloured accent (a band across the top, a rail down the left), so a themed deck looks deliberate on every slide instead of plain pages with a coloured background. Tables and Slides gained **Add Font…** in Options: pick a font already on your phone (B Nazanin, IRANSans and the rest) and it joins the font list next to the 27 bundled families. And in Slides, tapping the ribbon while you are typing now puts the on-screen keyboard away, where it stays until you tap the slide again.

**1.0.1** rebuilds all three on the phone File screen: every app now opens on the same list of
destinations (Home, New, Open, Info, Save, Save As, Print, Export, About, Options), **About** is a
page there (Tables and Slides gained the row; Write already had it), Write and Tables dropped the Close
row and Slides dropped Close and Account (on a phone Android's Back closes the document), and the
list's **Back** bar is shown only when there is a document behind it — not on the app's first screen.

No account, no ads, no telemetry and no network permission: the apps ask only for the files you hand
them through the system file picker.
