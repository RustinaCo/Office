# Rustina Office APKs

Signed release builds of the three apps: **arm64-v8a only**, **Android 10 (API 29) or newer**.
Install one by opening the file on the phone and allowing installs from this source when Android
asks (and turning off "install via USB" restrictions if your phone has them).

| File | App | What it opens | Size |
|---|---|---|---|
| [`Rustina-Write-1.0.1.apk`](Rustina-Write-1.0.1.apk) | Rustina Write | `.docx`, `.txt`, `.md`, `.html`, `.rtf`, `.odt` | 23.5 MB |
| [`Rustina-Gride-1.0.1.apk`](Rustina-Gride-1.0.1.apk) | Rustina Gride | `.xlsx`, `.csv` | 9.8 MB |
| [`Rustina-Slides-1.0.1.apk`](Rustina-Slides-1.0.1.apk) | Rustina Slides | `.pptx` | 17.0 MB |

These are built from the sources in this same tag — `Write/`, `Gride/` and `Slides/` — with
`cargo apk2 build --lib --release` (each app folder's README has the exact commands). They are signed
with a self-signed release key so they can be sideloaded; Android will warn about an unknown
developer the first time.

SHA-256 of each file at this tag, if you want to check what you downloaded:

```
8d3960e9126114ccfddc5c6f9a4cb5dbd9c586eb16ec7d25ed1abbf7e7e8138d  Rustina-Write-1.0.1.apk
b1ef9f0a67c1d6b6166ec978dc07f0a7e375af7c94bb639db51afd579435ddfb  Rustina-Gride-1.0.1.apk
d58134f88158a5427d609e90fad812fd5c206662d14bf3185720810703377a86  Rustina-Slides-1.0.1.apk
```

**1.0.1** rebuilds all three on the phone File screen: every app now opens on the same list of
destinations (Home, New, Open, Info, Save, Save As, Print, Export, About, Options), **About** is a
page there (Gride and Slides gained the row; Write already had it), Write and Gride dropped the Close
row and Slides dropped Close and Account (on a phone Android's Back closes the document), and the
list's **Back** bar is shown only when there is a document behind it — not on the app's first screen.

No account, no ads, no telemetry and no network permission: the apps ask only for the files you hand
them through the system file picker.
